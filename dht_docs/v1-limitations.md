# v1 limitations

Statement-store v1 uses flood gossip: every node stores every statement and
forwards it to all connected peers. Two hard limits follow:

- **Storage** — every node holds the entire network's statements, so total
  network capacity is bounded by a single node's disk.
- **Bandwidth** — each statement is sent to and received from every peer, so
  per-node throughput bounds the whole network.

v2 ([[v2-overview]]) removes both by storing and forwarding only toward nodes
with [[topic-affinity]] for a statement.
