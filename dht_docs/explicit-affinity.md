# Explicit affinity

Issue #11934. A node's *explicit affinity set* is the set of topics it has
declared interest in ([[explicit-affinity-sources]]). Two aspects:

- **Explicit affinity routing** — forward statements to peers whose declared
  affinity matches the topic ([[forwarding-decision]]).
- **Explicit affinity storing** — persist statements for topics in the node's
  own set ([[statement-storage]]).

The module advertises an affinity filter to peers ([[bloom-filter-affinity]]),
exposes its topic list so [[peers-topology]] can choose peers to connect to, and
answers whether a statement must be stored based on the affinity source.
