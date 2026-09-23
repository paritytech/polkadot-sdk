# Peer-set eviction and bounding (white-space)

The cost model in [[peer-sets]] only holds if each set is bounded by the right
mechanism. The current `PeersTopology` enforces **neither**.

## Required bounds

| Set | Bound | Why this bound |
| :-- | :-- | :-- |
| Known peers | TTL (time) | Entries are cheap, so count is not the constraint; staleness is. A discovered peer that is never seen again must eventually expire. |
| Connected peers | Configurable max count | Connections are expensive (sockets, bandwidth), so the active set must be hard-capped regardless of age. |

## Current gaps

In `substrate/client/network/statement/src/v2dht/peers_topology.rs`:

- **Known set has no TTL.** `note_seen` inserts an entry on every
  `Event::PeerDiscovered`; nothing expires it. `on_peer_disconnected` only
  clears the `connected` flag — it never removes the entry. `note_peer_removed`
  (the only method that shrinks the map) is unused outside tests. The map
  therefore grows monotonically for the life of the node.
- **Active set has no enforced cap.** Connectedness is tracked, but there is no
  configurable limit on how many peers may be in the connected subset at once.

## Implications

- The unbounded *count* is **by design**, not a leak: the Kademlia
  [[routing-table]] is lossy (~130 of 1000 nodes), so the topology must retain
  every discovered peer to have a whole-network view ([[discovery-coverage]]).
  The real need is therefore **staleness** eviction (drop dead nodes), not a
  count cap — and a scaling ceiling (see below), not memory per se.
- Without the TTL, dead/departed peers are never dropped, so the set drifts
  stale and `closest_known` / `is_dht_affine` re-sort an ever-growing list of
  peers that may no longer exist.
- This holds only for a **closed** node set (people-chain). Open participation
  would make even an unbounded known set untenable and needs a dedicated
  statement-store DHT ([[discovery-coverage]]).

## Notes

- TTL must be refreshed whenever a peer is re-observed (discovery, identify, or
  an opened substream), not only on first insert.
- A peer in the active set should be exempt from known-set TTL eviction while
  connected.
- The active-set cap is split into reserved (DHT-closest, always kept) and
  random slots; only the random slots are subject to churn
  ([[peer-connection-model]], [[peer-steering]]).
