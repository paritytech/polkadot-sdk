# Explicit affinity sources

Where a node's [[explicit-affinity]] topics come from (#11934):

- **RPC subscriptions** — an active topic subscription automatically grants
  explicit affinity for its topics ([[client-rpc]]).
- **CLI / config file** — operator-defined static affinity.
- **Extensible** — the design keeps the source set open for future inputs
  (e.g. [[premium-prioritisation]]).

Source matters: storing makes sense for static config but not for short-lived
subscriptions ([[node-roles]]), and the store can apply per-source limits
([[store-limits]]).
