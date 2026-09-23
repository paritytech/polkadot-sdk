# Rollout plan

Issue #11938. Once #11932 is implemented, switching the live network needs a
strategy. Open questions:

- **Migration** — existing stored statements must be preserved and moved into
  the new topology ([[node-join-sync]]).
- **Coordination** — nodes need to start speaking the new protocol together,
  not piecemeal.
- **No v1 interop** — v2 does not speak v1 on the wire ([[v2-overview]]), so
  nodes cannot upgrade piecemeal; the switch must be coordinated (effectively a
  flag day).

## Open questions (white-space)
- No concrete migration path or coordination mechanism (version gating, flag
  day, dual-protocol period) is specified yet.
- The current handler still negotiates a **v1 fallback**
  (`PeerProtocolVersion::V1` via `negotiated_fallback`) and tracks V1 peers.
  Under a no-compat coordinated switch, is that fallback purely transitional for
  the migration window, and when is it removed? (This is why connected V1 peers
  are not [[dht-affinity-storing]]-eligible — see [[node-roles]].)
