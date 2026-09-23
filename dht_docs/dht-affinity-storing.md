# DHT affinity storing

A node persists statements for topics it is among the **k-closest** for,
according to its [[routing-table]] (which tracks far more nodes than are
connected) — not just connected peers as in [[dht-affinity-routing]].

"k" is the replication factor K ([[peers-topology]]). Storage happens only after
affinity is confirmed and the statement passes validation
([[statement-storage]]).

## Open questions (white-space)
A remote peer's DHT eligibility (full vs light) is not derivable from
discovery/identify alone — a peer advertising the protocol may still be a light
node ([[node-roles]]). Until eligibility is known, only connected peers can
contribute to the closest-set, which weakens storing decisions over the wider
[[routing-table]]. In practice this is softened: discovering ~99% of the network
also connects to ~88% of it ([[discovery-coverage]]), so eligibility gets
populated for most nodes over time — but it remains undecidable for the
never-connected remainder.
