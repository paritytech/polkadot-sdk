# Statement storage

§3.5. A node stores a statement if it has affinity for **any** of its topics:

1. **DHT affinity storing** — among k-closest for a topic
   ([[dht-affinity-storing]]), or
2. **Explicit affinity storing** — a topic is in the node's explicit set
   ([[explicit-affinity]]).

Only **after** affinity is confirmed does it run the full v1 validation pipeline
(proof verification, signer quota, channel rules) — affinity is the cheap gate
first. Light nodes store only their explicit-affinity topics ([[node-roles]]).

Storage volume is capped per affinity reason ([[store-limits]]).
