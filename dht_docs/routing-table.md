# Routing table

Each node maintains a Kademlia-style routing table, optimised for fast
store/route decisions, used to test whether it is among the k-closest for a
topic ([[dht-affinity-storing]]) and to find peers for explicit affinity
([[explicit-affinity-discovery]]).

## Buckets
The table is split into **buckets**, each holding up to *k* peers. A bucket
covers a band of XOR distance ([[xor-keyspace]]) measured from the local node's
own key: bucket *i* holds peers whose distance from us lands in
[2^i, 2^(i+1)) — i.e. peers that share the first *i* bits of our key. Far-away
bands are enormous but sparsely filled (few peers, one bucket); nearby bands are
fine-grained. The result is detailed knowledge of our own neighbourhood and
coarse knowledge of the rest — enough to route toward any key in O(log n) hops.

**Adding a peer to the right bucket** means: XOR the peer's key with our own to
get its distance, find which band that distance falls in, and insert the peer
into that band's bucket. If the bucket is already full, Kademlia keeps the *k*
most-recently-seen *live* peers and drops or declines stale ones (see liveness
below) — so a bucket is a bounded, self-pruning slot, not an unlimited list.

## Maintenance (standard Kademlia)
- **Bootstrap** — connect to seed nodes, look up own PeerId to find neighbours.
- **Bucket refresh** — periodic lookups for random keys in sparse buckets; also
  privacy cover traffic, indistinguishable from affinity lookups.
- **Learning from traffic** — file each newly seen peer into the bucket its
  distance selects.
- **Liveness** — periodically ping peers, evict unresponsive ones.

Bounded by these per-bucket *k* limits, unlike the flat known set in
[[peer-sets]]. Because of that bound it is **lossy**: empirically a 1000-node
network leaves only ~130 peers in the table, so it cannot serve as a complete
membership view — the topology must track all discovered peers separately
([[discovery-coverage]]).
