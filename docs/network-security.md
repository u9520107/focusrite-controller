# Network Security

## Current decision

V1 foundation and native touchscreen use local IPC only. `focusrited` exposes a
Unix socket with dedicated group permissions for `focusrite-ui`; it does not
listen on TCP/IP by default. The touchscreen client is a local Rust process,
not a network client.

This keeps Phase 1 through Phase 4 inside one Pi: no LAN listener, browser
pairing, bearer token, CORS, WebSocket, TLS, port forwarding, or router
configuration is needed to use core hardware controls.

## Later LAN mode

LAN access is a later Phase 5 feature and remains a deliberate product/security
decision. It must be opt-in and must not weaken local-only operation.
`focusrited` owns the LAN HTTP/WebSocket listener.

Design must define, review, and test:

- private-interface binding and discovery behavior;
- controller and administrator authorization boundaries;
- HTTP and WebSocket authentication, Origin and CORS policy;
- administrator credential lifecycle and audit-safe logging;
- when TLS becomes required beyond trusted home-LAN use;
- firewall, router port-forwarding, and UPnP policy;
- recovery when network configuration or paired clients change.

Direct Internet exposure and automatic router configuration are out of scope.
The appliance must remain fully usable with LAN mode disabled.

## Transitional source-IP allowlist

LAN mode may optionally restrict HTTP and WebSocket connections to configured
source IP addresses. This supports a small fixed set of trusted devices with
DHCP reservations while a separate guest network is being introduced. When the
allowlist is configured, addresses not on it are rejected before serving the UI
or API. IPv4 and IPv6 addresses must be considered explicitly; an IPv4-only
allowlist must not accidentally leave an IPv6 listener open.

Source-IP filtering is exposure reduction, not authentication. DHCP/MAC
reservations and source addresses can be changed or impersonated by a device on
the same network. Router/firewall isolation between guest and trusted networks
is the primary boundary; the daemon allowlist is an additional safeguard.

## Threat boundary

A home network is not automatically trusted: guests, compromised devices, and
traffic visible on shared Wi-Fi can exist. No later LAN design may treat a
private IP address alone as authorization. Device serials, LAN tokens, home IP
addresses, and service logs are sensitive and must be redacted from committed
fixtures and documentation.
