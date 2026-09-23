# Discovery piggybacks the chain's DHT

The statement store runs **no Kademlia of its own**. It builds its peer view
entirely from the events of the node's single, existing discovery instance — the
chain's DHT. That DHT is partitioned **by genesis hash**, which is what bounds
statement-store membership to one chain (today, people-chain). See the empirical
consequences in [[discovery-coverage]].

## The DHT is genesis-scoped
Kademlia only talks to peers speaking the same protocol string, and that string
is built from the chain's genesis hash:

```rust
// discovery.rs:1314 — kademlia_protocol_name
format!("/{genesis_hash_hex}/{fork_id}/kad")   // e.g. /e8e7…/kad
```

The statement notification protocol is scoped identically, from the **same**
genesis hash passed at setup (`service.rs:509` → `lib.rs:337`):

```rust
format!("/{hex}/{fork_id}/{STATEMENT_PROTOCOL_V2}")   // /e8e7…/statement/2
```

Two nodes on different chains have different `/kad` names → they sit in
**separate DHTs** and never enter each other's [[routing-table]].

## Discovery is the chain's random walk
Every 5s the chain's Kademlia runs a random-key `FIND_NODE` and reports results:

```rust
// litep2p/discovery.rs:68
const KADEMLIA_QUERY_INTERVAL: Duration = Duration::from_secs(5);
// …emits:
DiscoveryEvent::RoutingTableUpdate { peers }   // -> Event::PeerDiscovered
```

## The handler just listens
```rust
let network_event_stream = network.event_stream("statement-handler-network");
// handle_network_event:
Event::PeerDiscovered(peer)            => self.peers_topology.note_seen(peer),
Event::PeerIdentified { peer, supported_protocols } =>
    self.peers_topology.note_identified(peer, supported_protocols.contains(&self.protocol_name)),
```

So the topology's universe = whatever the **chain's** random walk surfaces.

## Consequence
```
discoverable universe  =  people-chain DHT members
statement-store nodes  ⊆  people-chain DHT members      (filtered by protocol support)
```
A statement node **not** on people-chain has a different `/kad`, lives in a
different DHT, is never returned by FIND_NODE here, and is therefore
undiscoverable — no config or code path reaches it. The limitation falls out of
genesis-hash partitioning, it is not a setting.

## Open participation
Letting any node (any chain) run the store requires a **common, statement-store-
scoped DHT** — a `/statement-store/kad`-style network every statement node joins
regardless of its chain — instead of the per-chain `/kad`. That dedicated DHT is
the unbuilt piece behind the [[discovery-coverage]] scope caveat.
