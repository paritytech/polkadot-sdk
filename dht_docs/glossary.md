# Glossary

Canonical terms from the design doc. Use these in code and docs; where the
current code diverges, see "Code vs design terminology" below.

## Affinity
- **Topic affinity** — a node is responsible for storing/gossiping a topic
  ([[topic-affinity]]). Two kinds: DHT and explicit.
- **DHT affinity** — responsibility implied by XOR closeness to the topic
  ([[xor-keyspace]]).
- **Explicit affinity** — responsibility declared for specific topics
  ([[explicit-affinity]]).

The design splits affinity into **four aspects** (a full node does all four):
- **DHT affinity routing** — forward toward the k-closest *connected* peers
  ([[dht-affinity-routing]]).
- **DHT affinity storing** — persist statements for topics the node is
  k-closest for, per its routing table ([[dht-affinity-storing]]).
- **Explicit affinity routing** — forward to peers whose declared affinity
  matches ([[forwarding-decision]]).
- **Explicit affinity storing** — persist statements for topics in the node's
  own explicit set ([[explicit-affinity]]).

## Peers
- **Known peer** — any discovered statement-capable peer; cheap metadata,
  unbounded, TTL'd ([[peer-sets]]).
- **Connected peer** — a known peer with an open statement substream; the capped
  active set ([[peer-sets]]).
- **Reserved / random slots** — the connected set's pinned (DHT-closest) vs
  churnable portions ([[peer-connection-model]]).

## DHT / routing
- **XOR distance / keyspace** — closeness metric between topic and peer keys
  ([[xor-keyspace]]).
- **Routing table / k-bucket** — Kademlia structure of distance-banded,
  k-bounded buckets ([[routing-table]]).
- **Replication factor (K)** — number of nodes responsible for storing a topic
  ([[peers-topology]]).
- **Gossip target** — max peers a statement is gossiped to ([[peers-topology]]).

## Privacy
- **Affinity (bloom) filter** — probabilistic advertisement of explicit affinity
  that hides exact topics ([[bloom-filter-affinity]]).

## Code vs design terminology (rename to track)
The current implementation introduces terms not in the design; aligning them
avoids drift:

| Code (`peers_topology.rs`) | Canonical design term |
| :-- | :-- |
| `is_dht_affine` / "DHT storage replicas" | **DHT affinity storing** decision |
| "DHT storage affinity" (PR description) | **DHT affinity storing** |
| `routing_targets` | **DHT affinity routing** targets |
| `dht_eligible` | full-node, V2 statement peer ([[node-roles]]) |

Note: "storage affinity" is **not** a design term — the concept is *DHT affinity
storing*.
