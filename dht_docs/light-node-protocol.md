# Light node protocol

§3.8.3. A light node ([[node-roles]]) both sends and receives statements but does
not route or store for others. It builds its own [[routing-table]], so it can
run the protocol to find DHT- and explicit-affine peers.

- **Receiving** — use the routing table to connect to nodes likely to hold its
  topics, advertise a privacy/bandwidth-tuned [[bloom-filter-affinity]], and run
  [[peer-steering]].
- **Sending** — target nodes with affinity for the statement's topic and submit
  directly, so it reaches storers fast with little network traffic.

Light nodes should connect to **multiple full nodes** so at least one honest node
runs the protocol correctly. Implemented by extending smoldot (#11288): see
[[light-node-implementation]] for how the current code works,
[[light-node-client-api]] for the RPC surface, and
[[statement-protocol-versioning]] for v1/v2 negotiation. The routing-table and
[[peer-steering]] parts of this design are not yet implemented there.
