# Simulation

§4.1. Preliminary simulations (github.com/paritytech/stmt-store-network-sim)
explore the protocol's two goals — more total network storage, less per-node
storage/bandwidth — across configurations. The protocol is meant to be
configurable; the production config is TBD.

Findings:
- per-node storage **falls** as the DHT replication factor is shared over more
  nodes, and **decreases** as the network grows at fixed K,
- network traffic **rises** with the number of gossip targets — there is a sweet
  spot for fast dissemination without saturating bandwidth,
- under tight storage, light nodes must contact **more** full nodes and wait
  **longer** to reach a target delivery rate.

Informs the K and gossip-target defaults in [[peers-topology]].
