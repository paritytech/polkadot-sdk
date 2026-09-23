# DHT affinity routing

Forwarding a statement toward the **k closest connected peers** to a topic by
XOR distance ([[xor-keyspace]]). Per the design (§3.2.1), only peers with an
active connection are considered — not the true k-closest in the network.

The "gossip target" config bounds k here ([[peers-topology]]). Because each hop
forwards to closer-and-closer peers, propagation converges on the topic's
neighbourhood ([[propagation-termination]]).

Contrast with [[dht-affinity-storing]], which uses the full [[routing-table]],
not just connected peers.

## Open questions (requires deeper research)
Whether routing targets should be the closest k **connected** peers (as written
in §3.2.1 and #11933) or the closest k **known** peers — with connections then
established toward them — is unresolved. Targeting only the connected set may not
converge toward the true responsible storers if the node is not already
connected to closer peers; targeting the known set adds a connection step, since
a statement can only be sent to a connected peer. Needs investigation; possibly a
documentation error. The reserved-set mechanism (#12272, [[peer-connection-model]])
is the intended way to keep the DHT-closest peers always connected, which narrows
this gap.
