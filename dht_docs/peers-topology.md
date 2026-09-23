# Peers Topology module

Issue #11933. Builds the node's knowledge of all peers running the
statement-store protocol, using substrate/litep2p network primitives, and keeps
the list of currently connected peers.

Tracks two sets ([[peer-sets]]): the unbounded, TTL'd **known** set and the
capped **connected** set ([[peer-set-eviction]]). Updated as peers are
discovered, connect, disconnect, or leave the network. The connected set keeps
its DHT-closest peers in a reserved slot ([[peer-connection-model]]).

Configurable on:
- **Replication factor (K)** — nodes responsible for storing a topic.
- **Gossip target** — max peers to gossip a statement to.

Answers three questions efficiently:
- Should I store this statement by DHT affinity? → [[dht-affinity-storing]]
- Which connected peers should it be routed to? → [[dht-affinity-routing]]
- Which peers should I connect to for my explicit-affinity topics, minimising
  connection count? → [[explicit-affinity-discovery]]

Built on [[xor-keyspace]] and the [[routing-table]]; defaults informed by
[[simulation]]. Its peer view is fed entirely by the chain's own discovery, not
a statement-specific DHT ([[discovery-piggyback]]).
