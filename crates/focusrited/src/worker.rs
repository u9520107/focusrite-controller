//! Single-worker boundary for all blocking device operations.

use std::{
    sync::{
        Mutex,
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    thread::{self, JoinHandle},
};

use crate::{
    ControlId, Device, DeviceSnapshot, Service, ServiceError, Value, profile_store::Profiles,
};

const QUEUE_LIMIT: usize = 32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct State {
    pub snapshot: DeviceSnapshot,
    pub revision: u64,
    pub online: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkerError {
    Service(ServiceError),
    Stopped,
}

impl std::fmt::Display for WorkerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Service(error) => write!(formatter, "service error: {error:?}"),
            Self::Stopped => formatter.write_str("device worker stopped"),
        }
    }
}

impl std::error::Error for WorkerError {}

pub struct DeviceWorker {
    sender: SyncSender<Request>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

enum Request {
    State(std::sync::mpsc::Sender<State>),
    Refresh(std::sync::mpsc::Sender<Result<State, ServiceError>>),
    Command {
        control: ControlId,
        value: Value,
        reply: std::sync::mpsc::Sender<Result<State, ServiceError>>,
    },
    Stop(std::sync::mpsc::Sender<()>),
}

impl DeviceWorker {
    /// Starts one bounded queue and one serial device-owning thread.
    pub fn start<D: Device + Send + 'static>(device: D) -> Result<Self, WorkerError> {
        Self::start_with_profiles(device, Profiles::new())
    }

    /// Starts with stored profiles. Loading profiles never applies a write.
    pub fn start_with_profiles<D: Device + Send + 'static>(
        device: D,
        profiles: Profiles,
    ) -> Result<Self, WorkerError> {
        let (sender, receiver) = sync_channel(QUEUE_LIMIT);
        let (ready_sender, ready_receiver) = sync_channel(1);
        let thread = thread::spawn(move || match Service::connect(device) {
            Ok(mut service) => {
                service.set_profiles(profiles);
                let _ = ready_sender.send(Ok(()));
                run(service, receiver);
            }
            Err(error) => {
                let _ = ready_sender.send(Err(error));
            }
        });

        match ready_receiver.recv().map_err(|_| WorkerError::Stopped)? {
            Ok(()) => Ok(Self {
                sender,
                thread: Mutex::new(Some(thread)),
            }),
            Err(error) => {
                let _ = thread.join();
                Err(WorkerError::Service(error))
            }
        }
    }

    pub fn state(&self) -> Result<State, WorkerError> {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.sender
            .send(Request::State(sender))
            .map_err(|_| WorkerError::Stopped)?;
        receiver.recv().map_err(|_| WorkerError::Stopped)
    }

    pub fn refresh(&self) -> Result<State, WorkerError> {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.sender
            .send(Request::Refresh(sender))
            .map_err(|_| WorkerError::Stopped)?;
        receiver
            .recv()
            .map_err(|_| WorkerError::Stopped)?
            .map_err(WorkerError::Service)
    }

    pub fn command(&self, control: ControlId, value: Value) -> Result<State, WorkerError> {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.sender
            .send(Request::Command {
                control,
                value,
                reply: sender,
            })
            .map_err(|_| WorkerError::Stopped)?;
        receiver
            .recv()
            .map_err(|_| WorkerError::Stopped)?
            .map_err(WorkerError::Service)
    }

    pub fn stop(&self) -> Result<(), WorkerError> {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.sender
            .send(Request::Stop(sender))
            .map_err(|_| WorkerError::Stopped)?;
        receiver.recv().map_err(|_| WorkerError::Stopped)?;
        if let Some(thread) = self.thread.lock().map_err(|_| WorkerError::Stopped)?.take() {
            let _ = thread.join();
        }
        Ok(())
    }
}

fn run<D: Device>(mut service: Service<D>, receiver: Receiver<Request>) {
    while let Ok(request) = receiver.recv() {
        match request {
            Request::State(reply) => {
                let _ = reply.send(state(&service));
            }
            Request::Refresh(reply) => {
                let _ = reply.send(service.refresh().map(|_| state(&service)));
            }
            Request::Command {
                control,
                value,
                reply,
            } => {
                let _ = reply.send(service.command(&control, value).map(|_| state(&service)));
            }
            Request::Stop(reply) => {
                let _ = reply.send(());
                return;
            }
        }
    }
}

fn state<D: Device>(service: &Service<D>) -> State {
    State {
        snapshot: service.snapshot().clone(),
        revision: service.revision(),
        online: service.is_online(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{ControlCapability, DeviceError, ValueDomain};

    struct MockDevice(DeviceSnapshot);

    impl Device for MockDevice {
        fn snapshot(&mut self) -> Result<DeviceSnapshot, DeviceError> {
            Ok(self.0.clone())
        }

        fn write(&mut self, control: &ControlId, value: Value) -> Result<(), DeviceError> {
            self.0.values.insert(control.clone(), value);
            Ok(())
        }
    }

    #[test]
    fn serial_worker_confirms_command_before_returning_state() {
        let volume = ControlId("output.volume".into());
        let worker = DeviceWorker::start(MockDevice(DeviceSnapshot {
            device_id: "mock-device".into(),
            capability_schema: "mock-v1".into(),
            capabilities: vec![ControlCapability {
                id: volume.clone(),
                domain: ValueDomain::Integer,
                writable: true,
                available: true,
                minimum: Some(0),
                maximum: Some(100),
            }],
            values: BTreeMap::from([(volume.clone(), Value::Integer(50))]),
        }))
        .unwrap();

        let state = worker.command(volume.clone(), Value::Integer(75)).unwrap();

        assert_eq!(state.snapshot.values[&volume], Value::Integer(75));
        assert_eq!(state.revision, 2);
        worker.stop().unwrap();
    }
}
