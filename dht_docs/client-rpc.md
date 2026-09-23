# Client RPC

How a client talks to a node (same wire shape as v1):

- **Submission** (§3.8.1) — client submits a SCALE-encoded statement; the node
  validates, stores it if criteria are met ([[statement-storage]]), and
  propagates it ([[forwarding-decision]]).
- **Subscription** (§3.8.2) — client opens a topic-filtered subscription;
  matching stored statements are sent immediately, then new ones stream live.

**New in v2:** creating a subscription auto-adds its topics to the node's
[[explicit-affinity]] set (updating the [[bloom-filter-affinity]]); dropping it
removes them. This assumes you trust the RPC node and need not hide topic
interest from it — for stronger privacy, run a [[light-node-protocol]].
