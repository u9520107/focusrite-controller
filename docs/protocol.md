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

## Local IPC v1

Phase 4a local IPC is newline-delimited JSON over the daemon-owned Unix socket.
Each message is at most 64 KiB. Client requests include `v: 1` and a `type`:
`snapshot`, `command` with opaque `control` and typed `value`,
`group_command` with a configured opaque `group` ID and canonical integer
`position` in `0..=1000`, `profile_save` with a validated `name`,
`profile_list`, `profile_review` with `name`, or `profile_apply` with exact
`name` and returned `review`. The daemon returns `snapshot`, `command_result`,
`group_command_result`, `profile_save_result`, `profile_list_result`,
`profile_review_result`, `profile_apply_result`, `event`, or bounded `error`
JSON. State messages carry
`instance_id`, `revision`, `online`, and full authoritative snapshot. A group
result additionally names applied/skipped members and its first failed member
with a safe error code. Typed values use tagged JSON, for example
`{"type":"integer","value":75}`.

Malformed, oversized, and unsupported-version requests receive one safe error
then their connection closes. Each connection has a bounded outbound queue;
newer unsent state events may replace older unsent events, while replies do not
coalesce. Queue overflow disconnects only that client.

Profile save/list/review replies carry a `profile_result`; profile apply also
carries authoritative state after its ordered attempt. A review includes
binding state, sorted control entries, and an opaque revision/fingerprint pair.
Apply rejects a missing, stale, or mismatched review before any write. Group
commands are limited to persisted, adapter-declared `relative_level` groups
and remain non-atomic ordered writes. Dangerous-control confirmation and
idempotency keys remain deferred to Phase 5.

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
