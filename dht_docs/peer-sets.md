# Known peers vs connected peers

The statement-store v2 [[peers-topology]] partitions peers into two sets with
very different cost profiles and lifecycle rules.

## Known peers (discovery set)

- Every peer learned from the discovery protocol — Kademlia routing-table
  updates and identify metadata (`Event::PeerDiscovered`, `Event::PeerIdentified`).
- Holds only lightweight metadata per peer: statement-protocol support, DHT
  eligibility, and the XOR key material used for affinity math.
- **Unbounded in count.** Each entry is cheap, so there is no hard size cap.
- Maintained by the discovery protocol: entries are added as peers are seen.
- **Bounded by TTL, not by count.** An entry expires if it is not refreshed
  within the TTL window, so stale peers are evicted even though the set has no
  size limit. This is what keeps an "unlimited" set from growing forever.
- A deliberate **superset** of the Kademlia [[routing-table]], which caps at
  k-per-bucket and holds only a fraction of the network (~130 of 1000 nodes).
  The topology tracks every `RoutingTableUpdate` peer, not just those the kad
  table keeps ([[discovery-coverage]]).

## Connected peers (active set)

- The subset of known peers with an open statement-protocol notification
  substream.
- **Bounded by a configurable maximum** (the active-set limit). Connections are
  expensive — sockets, handshakes, sustained bandwidth — so this set is capped.
- Membership is driven by notification stream open/close, not by discovery.
- Split into **reserved** slots (the DHT-closest peers, always kept) and
  **random** slots (free for new/unknown peers) — see [[peer-connection-model]].

## Relationship

`connected ⊆ known`. A peer is first *known* (cheap, metadata only), and may
later be *promoted* to the active set when a substream opens, then *demoted*
back to known-only on disconnect without being forgotten.

## Mapping to code

In `PeersTopology` (`substrate/client/network/statement/src/v2dht/peers_topology.rs`):
- `peers: HashMap<PeerId, PeerInfo>` is the **known set**.
- `PeerInfo.connected` marks the **active subset**.

## Open questions (white-spaces)

The current implementation enforces **neither** bound — see
[[peer-set-eviction]].
