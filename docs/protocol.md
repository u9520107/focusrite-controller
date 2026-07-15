# API and State Protocol

## Versioning

All network and Unix-socket messages use versioned JSON under `v1`. Shared Rust
types define wire schema. Fict and later Pebble clients consume documented JSON,
not Rust internals.

## Endpoints

| Endpoint | Purpose |
| --- | --- |
| `GET /api/v1/state` | full authoritative snapshot |
| `POST /api/v1/commands` | validated device mutation |
| `POST /api/v1/ws-ticket` | authenticated one-use, short-lived WebSocket ticket |
| `GET /api/v1/events` | WebSocket: snapshot then events |

Unix-socket clients use equivalent snapshot, command, and event messages.

## State rules

- Snapshot includes `instance_id`, device identity, connection state,
  capabilities, controls, per-device user metadata (custom labels and linked
  groups), profiles metadata, and monotonic `revision`.
- Event includes `instance_id`, new `revision`, kind, and authoritative changed
  state.
- Command includes stable `client_id`, client-generated `request_id`, target
  control or compound operation, requested value, and optional observed
  revision. `(client_id, request_id)` is idempotency key; same key returns its
  original accepted result and a conflicting reused payload is rejected.
- Daemon serializes commands, validates capability/range/safety, writes hardware,
  confirms canonical state, then broadcasts result.
- A non-native linked-group command is one validated, ordered compound command;
  it reports per-member confirmation and cannot promise atomic hardware writes.
- Revision gaps, failed event delivery, or changed `instance_id` require full
  snapshot resync. Clients cannot reconstruct state from missed events.
- Concurrent writers use last confirmed write wins. Clients immediately replace
  optimistic values with daemon-confirmed values.
- Fader clients debounce outbound updates; target budget is 30–60 commands/sec.
  Server also bounds queues and coalesces queued fader updates per control.

## Browser authentication

Browser/LAN access is deferred. Foundation and native touchscreen operation use
the Unix socket only; see [Network Security](network-security.md). The design
below is retained as a proposal, not an accepted implementation requirement.

- HTTP API uses bearer token in `Authorization` header.
- Browser first makes authenticated `POST /api/v1/ws-ticket`; response is a
  short-lived, one-use ticket bound to token subject and allowed Origin.
- Browser opens same-origin WebSocket with ticket in `Sec-WebSocket-Protocol`,
  never URL/query. Long-lived bearer token never appears in WebSocket URL,
  logs, history, or referrer. Ticket expiry/reuse fails upgrade.
- Server requires configured same Origin on ticket mint and WebSocket upgrade;
  reject missing/unexpected Origin. CORS has explicit allowed origins only,
  never wildcard.
- Pairing starts from local touchscreen/owner path and requires explicit local
  approval before showing a newly generated scoped token once for copy or scan.
  Browser keeps bearer token in `sessionStorage`, never URL/query, cookie, or
  persistent browser storage; pairing repeats for new browser sessions.
  Rotation replaces token and invalidates old token/tickets; revocation
  invalidates token/tickets immediately and clears access on next request.

## Error classes

- unauthenticated or unauthorized client;
- unknown/unavailable capability;
- invalid value or dependency unmet;
- confirmation required for dangerous control;
- device offline or command timed out;
- stale request completed after newer confirmed state.

Errors never expose tokens, device serials, or raw control dumps to web clients.

## Safety

Server validates all mutations, including commands from local touchscreen.
Confirmation gates protect phantom power, clock/source changes, routing changes,
factory reset, and firmware actions. Server returns short-lived confirmation
challenge bound to `client_id`, `request_id`, control/operation, and exact
requested value. Confirm command must return that challenge; changed value or
expired/reused challenge fails. Last two remain unimplemented in v1.

## Profile application

Profile dry-run returns binding check, deterministic ordered diff, dangerous
operations needing confirmation, and skipped unavailable controls. Apply names
the reviewed profile/diff and uses normal idempotency/confirmation rules. Final
report lists each operation as applied, skipped with reason, or failed with
safe error. No automatic rollback follows partial failure.
