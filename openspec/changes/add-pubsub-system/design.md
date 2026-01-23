# Design: Pub-Sub System

## Context

RFC-0160 specifies a publish-subscribe mechanism for cross-chain data sharing via the relay chain. Publishers (parachains) store key-value data in per-publisher child tries on the relay chain. Subscribers (other parachains) declare subscriptions and receive data via relay state proofs included in their inherent data.

Key stakeholders:
- Publisher parachains (e.g., POP for ring signature roots)
- Subscriber parachains (e.g., chains verifying ring proofs)
- Relay chain governance (publisher registration, limits)

Constraints:
- PoV size limits
- Relay chain storage costs
- XCM v5 compatibility required

## Goals / Non-Goals

**Goals:**
- Enable parachains to publish key-value data to relay chain (subject to size limits)
- Enable parachains to subscribe to and receive published data with proofs
- Minimize PoV overhead via trie node caching
- Support TTL-based automatic expiration
- System parachain privilege (publish without registration)

**Non-Goals:**
- Prefix-based key enumeration (subscribers must know exact keys)
- Real-time streaming (latency is 2+ blocks)

## Decisions

### Decision 1: Fixed 32-byte Keys

Keys are 32-byte values. Publishers are responsible for hashing their logical key names before publishing.

**Rationale:**
- Evenly distributed trie structure
- Predictable storage calculations

### Decision 2: Per-Publisher Child Tries

Each publisher gets a dedicated child trie under key `(b"pubsub", para_id).encode()`.

**Rationale:**
- Prevents unbalanced main trie
- Efficient cleanup on deregistration
- Child trie roots enable change detection

### Decision 3: On-Chain Trie Node Caching

Subscriber parachain caches all trie nodes needed to access subscribed keys. There is no upper bound on cache size - we cache exactly what we need.

**Block building process:**

1. **Skip unchanged child tries:** For child tries where the root hasn't changed, remove them entirely from the relay chain proof.

2. **Prune changed child tries:** For child tries that have changed:
   - Remove nodes already cached on-chain
   - Remove nodes leading to keys we haven't subscribed to
   - Include only new/changed nodes for subscribed keys

3. **Enforce size limit:** Ensure total proof size stays within budget. Nodes that don't fit are removed from the proof.

4. **Cursor for partial processing:** On-chain logic sets a cursor if the previous block couldn't include all nodes to access a subscribed key. This indicates the limit was hit.

5. **Malicious collator protection:** On-chain verification must ensure that when a trie node is missing, the proof is at the upper limit. If not at limit but nodes are missing, panic (collator is cheating).

6. **Resume from cursor:** Next block starts pruning from the cursor position. Child trie nodes that don't fit are removed from the proof.

### Decision 4: Budget Allocation

Pub-sub uses remaining PoV space after messages, with a minimum guaranteed budget.

**Formula:**
```
messages_budget = calculated per existing logic
remaining = allowed_pov - messages_used
pubsub_budget = max(remaining, 1 MiB)
```

If remaining space after messages is below 1 MiB, pub-sub still gets 1 MiB minimum.

### Decision 5: TTL with on_idle Cleanup

Keys can have finite TTL (expire after N blocks) or infinite TTL (0 = never expires).

**Rationale:**
- Prevents relay chain storage bloat
- Publishers control data lifecycle
- Subscribers receive TTL metadata for freshness decisions

**Cleanup mechanism:**
- `on_idle` scans `TtlData` storage
- Uses cursor for incremental processing across blocks

### Decision 6: Single-Key Publish Instruction

XCM `Publish { key, value, ttl }` publishes one key at a time. Batch via multiple instructions.

**Rationale:**
- Simpler instruction semantics
- Predictable weight calculation
- Aligns with XCM instruction granularity

## Risks / Trade-offs

### Risk: PoV Exhaustion Under High Load

Heavy HRMP message blocks may compete with pub-sub for PoV space.

**Mitigation:**
- Minimum 1 MiB budget guarantees pub-sub progress
- Cursor tracks resumption point for next block
- System parachains prioritized in subscription ordering

### Risk: TTL Cleanup Delays

`on_idle` may not have enough weight to clean all expired keys immediately.

**Mitigation:**
- Best-effort expiration (may delay 1-2 blocks)
- Subscribers should check TTL metadata for freshness
- Manual deletion available for immediate removal

### Trade-off: Exact Keys Only

No prefix-based enumeration. Subscribers must know exact keys.

**Justification:**
- Prevents unbounded PoV from key enumeration
- Publishers and subscribers coordinate on key naming conventions

## Migration Plan

Not applicable - new capability with no existing implementation.

### Rollback

If issues discovered post-deployment:
1. Governance can force-deregister problematic publishers
2. Subscribers can return empty subscriptions to disable
3. Full removal requires runtime upgrade

## Open Questions

1. **System parachain threshold:** The threshold for system parachains (ID < 2000) is hardcoded. Should this be configurable?
2. **Metrics:** What on-chain metrics should be exposed for monitoring pub-sub health?
