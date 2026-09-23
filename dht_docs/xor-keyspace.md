# XOR keyspace

DHT affinity ([[topic-affinity]]) measures closeness between a topic and a peer
as the XOR of their keys, read as a big-endian integer — standard Kademlia
distance. The smaller the XOR, the "closer" the peer is to the topic.

Used by [[dht-affinity-routing]], [[dht-affinity-storing]] and
[[explicit-affinity-discovery]].

## Open questions (white-space)
The design does not pin down key derivation. For local affinity decisions to
approximate the node's true Kademlia responsibility, the peer-key hash must
match the network DHT's keyspace (libp2p/litep2p use SHA-256 of the peer-id
multihash) and the topic-key derivation must be agreed. The current code hashes
peer ids with blake2-256 — internally consistent, but diverging from the DHT
keyspace.
