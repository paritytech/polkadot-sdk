# Store limits

Issue #11936. The store should bound how much it keeps, with **separate
configurable limits by the reason a statement is stored**:

- a limit for statements held due to DHT affinity ([[dht-affinity-storing]]),
- a limit for statements held due to explicit affinity ([[explicit-affinity]]).

This lets an operator size DHT participation independently from its own
subscriptions ([[explicit-affinity-sources]]).

## Open questions (white-space)
The eviction policy when a limit is hit (which statement to drop, interaction
with statement expiry, and what happens to DHT-affinity coverage when the bucket
is full) is not specified.
