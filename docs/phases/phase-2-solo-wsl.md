# Phase 2: Solo Service Development in WSL

## Goal

Develop and test the Rust service with mocks and a Scarlett Solo attached
directly to WSL2. This is development evidence, not Pi compatibility proof.

## Scope

- Add bounded read-only Solo discovery and sanitized mock fixtures.
- Implement capability discovery, serialized device work, state reconciliation,
  reconnect handling, validation, and explicit profile persistence.
- Test supported Solo controls, external changes, and disconnect/reconnect.

## Guardrails

- Start with mock tests, then read-only hardware discovery.
- Do not write device state, change routing/clock, reset, or update firmware
  without explicit approval.
- Redact serials, tokens, home addresses, and unrelated system details from
  saved captures.
- Treat WSL2 results as insufficient for Pi or 16i16 support claims.

## Exit checks

- [ ] Mock tests cover writes, failures, disconnect/reconnect, and persistence.
- [ ] Sanitized Solo discovery fixture records supported and unsupported controls.
- [ ] Solo behavior is verified through WSL2 without unsafe state changes.
- [ ] Pi and 16i16 work remain deferred to Phases 3 and 7.

## Update rule

Record decisions, blockers, approvals, fixtures, and completed checks here
while Phase 2 is active.
