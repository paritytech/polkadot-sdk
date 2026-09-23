# Set reconciliation

§3.9.2. To cut the duplicate traffic of [[duplicate-statement-problem]] without
losing coverage, a node can keep only 1-2 **live forwarding peers** (low latency,
some duplicates) and **periodically reconcile** sets with additional peers to
catch anything missed (higher latency, bandwidth-efficient, no duplicates).

## Open questions (deferred)
The specific set-reconciliation mechanism is explicitly deferred to future work.
