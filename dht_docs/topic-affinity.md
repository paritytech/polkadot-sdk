# Topic affinity

A node has *topic affinity* when it is responsible for storing and gossiping
statements for a topic. Two kinds:

- **DHT affinity** — the node is close to the topic in XOR keyspace
  ([[xor-keyspace]]), so it is *implicitly* responsible. See
  [[dht-affinity-storing]] and [[dht-affinity-routing]].
- **Explicit affinity** — the node *declares* interest in specific topics (RPC
  subscriptions, config, …). See [[explicit-affinity]].

Both feed the same two decisions: should I store this statement
([[statement-storage]]) and to whom should I forward it
([[forwarding-decision]]).
