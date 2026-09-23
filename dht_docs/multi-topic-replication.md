# Multi-topic replication

§3.4.2. Because [[forwarding-decision]] is evaluated per topic, a statement with
4 topics is routed along up to 4 different DHT paths, reaching nodes with DHT
affinity for each topic.

This is deliberate redundancy: the statement can be retrieved via *any* of its
topics, so losing the neighbourhood of one topic does not lose the statement.
