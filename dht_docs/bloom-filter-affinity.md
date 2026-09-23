# Bloom-filter affinity

§3.7.1. To declare explicit affinity ([[explicit-affinity]]) without revealing
exact topics, a node advertises a **bloom filter** of its topics. A smaller
filter means more false positives — more privacy but more wasted traffic; a
larger filter means fewer false positives but more identifiable affinity.

The filter is sent in the **protocol handshake** when a connection opens. When
affinity changes (e.g. a new or dropped RPC subscription), the node sends a
**bloom-filter update** to all peers; because applying it is heavy, updates are
**rate-limited per node**.

An empty filter matches nothing; a match-everything filter reproduces v1 flood
behaviour for that node. See the correlation risk in [[privacy-correlation-risk]].
