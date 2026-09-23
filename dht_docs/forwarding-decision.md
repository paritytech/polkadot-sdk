# Forwarding decision

§3.4.1. On receiving or creating a statement with topics T1..Tn, the node
evaluates **each topic against every connected peer** and forwards to a peer if
**any** holds:

1. the peer's declared explicit affinity matches the topic
   ([[explicit-affinity]]), or
2. the peer is among the k-closest connected peers to the topic
   ([[dht-affinity-routing]]).

A statement is sent to a peer **at most once**, even if several topics select
it. Duplicates a peer has already seen are suppressed ([[duplicate-tracking]]),
and the decision is re-run periodically for late peers ([[periodic-propagation]]).
