# Explicit affinity discovery

To receive statements for a topic it cares about, a node must connect to peers
likely to hold them (DHT-affine for that topic). It finds them with a
**local routing-table lookup only** ([[routing-table]]): search the local table
for the closest known peers to the topic key and connect to them.

No FIND_NODE or other query goes on the wire, so the node never leaks which
topics it is interested in. This works when the table is well-populated, which
periodic random lookups ensure; the closest *known* peer may not be the true
k-closest but is usually close enough.

Connection targets are minimised by [[peers-topology]] (set-cover over topics)
and maintained by [[peer-steering]].
