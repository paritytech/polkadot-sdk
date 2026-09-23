# Node join & initial sync

§3.6. When a peer connects, instead of v1's "send everything", the node sends
only locally-stored statements the peer should receive:

1. statements whose topics match the peer's **explicit affinity**, or
2. statements whose topics the peer is **DHT-closest** to (by its PeerId).

Only statements meeting at least one condition are sent, done in round-robin
bursts across newly connected peers (same as v1). The [[orchestrator]] also
re-migrates statements as nodes join or leave the network, which is central to
the [[rollout-plan]].
