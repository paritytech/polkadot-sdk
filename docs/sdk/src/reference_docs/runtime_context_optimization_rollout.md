# Runtime Context Optimization Rollout

This note records the intended split for the relay-parent context, compatibility, dependency, and
channel-policy work. The implementation is deliberately incremental: each tranche should compile
and preserve existing subsystem contracts while moving repeated runtime and channel behavior behind
shared primitives.

## Stack Order

1. Runtime API and chain API coalescing.
   Land the low-level duplicate-call reductions first. These changes are mostly local to
   `polkadot/node/core/runtime-api` and `polkadot/node/core/chain-api`, and they make later
   subsystem adoption cheaper to validate.

2. RelayParentContextBroker core.
   Introduce the broker in `polkadot/node/subsystem-util/src/runtime`, typed relay-parent/session
   snapshots, active-leaf/finality invalidation, and focused tests. This PR should expose the API
   without forcing every subsystem to migrate at once.

3. Hot-path subsystem adoption.
   Move availability distribution, availability recovery, collator protocol, dispute distribution,
   and statement distribution onto broker-backed `RuntimeInfo` calls. Keep direct request helpers
   available for code paths whose error handling or lifecycle does not yet match the broker.

4. Observability.
   Register subsystem-prefixed broker metrics so cache hit/miss and pruning behavior can be
   attributed without colliding in the shared Prometheus registry.

5. Compatibility catalog.
   Expand `RuntimeApiFeature` from ParachainHost-only checks into a node-side catalog covering
   ParachainHost, XCM, bridge, and Snowbridge-facing runtime surfaces. Replace hard-coded
   compatibility checks opportunistically as call sites are touched.

6. Channel policy rollout.
   Convert high-volume `tracing_unbounded` call sites to named `ChannelPolicy` constants first.
   This gives operators and follow-up PRs a stable inventory before any semantic change from
   instrumented unbounded queues to bounded/shared-capacity queues.

7. Dependency-world harmonization.
   Keep the dependency-world guard in CI while carving out tooling-only dependency stacks. Treat
   Zombienet/test SDK drift separately from production runtime/node dependency alignment so the
   production lockfile surface does not silently inherit tooling churn.

## Review Boundaries

- Coalescing and cache metrics can be reviewed without the broker adoption PRs.
- Broker API and broker adoption should stay separate unless the reviewer wants an end-to-end
  slice.
- Channel policy naming is intentionally behavior-preserving and can land before bounded-channel
  semantics.
- Dependency-world checks should fail only on newly introduced cross-world drift, not on known
  historical duplicates until each world is explicitly cleaned.

## Validation Targets

- `polkadot-node-subsystem-util` for broker API and tests.
- Availability, collator, dispute, and statement distribution crates for RuntimeInfo adoption.
- `polkadot-node-subsystem-types` and `polkadot-overseer` for compatibility catalog usage.
- `sc-network`, `sc-network-sync`, and `sc-transaction-pool` for channel policy call sites.
- The dependency-world CI check for duplicate production/tooling drift.
