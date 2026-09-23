# v2 overview

v2 replaces v1's flood gossip ([[v1-limitations]]) with **topic-based DHT
routing**. A node stores statements only for topics it has [[topic-affinity]]
for, and gossips a statement toward the nodes likely to store it instead of to
everyone.

Goals: scale to millions of users, preserve decentralisation and privacy
([[bloom-filter-affinity]]), and degrade gracefully when the network is
saturated.

v2 is a clean break: it does **not** interoperate with v1 on the wire. The
network switches over via a coordinated rollout ([[rollout-plan]]), not gradual
per-node upgrades.

Core pieces: [[node-roles]], [[peers-topology]], [[explicit-affinity]],
[[forwarding-decision]], [[statement-storage]]. Delivered by the modules under
issue #11932 and rolled out via [[rollout-plan]].
