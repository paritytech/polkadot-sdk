# Super Chains

## Design Document

| Field | Value |
|-------|-------|
| **Authors** | eskimor |
| **Status** | **Draft / sketch** — future work, not implementation-ready |
| **Related Designs** | [Speculative Messaging](speculative-messaging-design.md), [Low-Latency Parachains v2](low-latency-v2-design.md) |

> **Status note.** This document is an early sketch, factored out of the
> Speculative Messaging design, which defines everything super chains need
> from the messaging layer: mutual requires through the virtually extended
> window, atomic enactment groups, bundle requires settling at committed
> boundary roots, and cycle handling. What
> remains here—production coordination, super-block acknowledgements,
> failure handling—has not received the same rigor and will change.

Super chains are a set of parachains operated by the same collator set,
enabling the tightest possible integration including intra-block
messaging: multiple chains exchanging messages within the same block
production cycle, with horizontal scaling preserved (full nodes still
follow only the chains they care about).

## Definition

```rust
struct SuperChainConfig {
    /// The parachains that form this super chain
    member_chains: BTreeSet<ParaId>,

    /// Collator set (must be identical across all members)
    collators: Vec<CollatorId>,

    /// Slot duration (must be synchronized)
    slot_duration: Duration,
}
```

**Open question**: where this configuration lives (member runtimes,
off-chain coordination, or elsewhere) and how membership changes are
coordinated.

## Super-Block Production

When a collator's slot arrives, they produce blocks for ALL member chains
atomically:

```rust
struct SuperBlock {
    /// Individual chain blocks, keyed by ParaId
    blocks: BTreeMap<ParaId, Block>,

    /// Slot this super-block was produced in
    slot: Slot,

    /// The collator who produced this super-block
    author: CollatorId,
}

impl SuperBlock {
    fn hash(&self) -> Hash {
        // Merkle root of constituent block hashes for efficient individual
        // proofs
        let block_hashes: Vec<(ParaId, Hash)> = self.blocks
            .iter()
            .map(|(id, b)| (*id, b.hash()))
            .collect();
        merkle_root(&block_hashes)
    }
}
```

**Open question**: the `SuperBlock` is a collator-side coordination
object—each member chain still produces its own candidate toward the relay
chain, coupled through mutual requires (atomic enactment groups, see
Speculative Messaging: Relay Chain Matching). The exact
candidate-assembly flow needs specification.

## Intra-Block Messaging

Within a super-block, messages can flow in both directions between any
member chains because:

1. The same collator produces all blocks
2. They have access to all chains' state simultaneously
3. They can resolve message dependencies during block production
4. Cycles are fine and supported (candidate-level cycles form atomic
   enactment groups—see Speculative Messaging: Cycle Handling)

```
┌─────────────────────────────────────────────────────────────────┐
│                     Super-Block N (Slot S)                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   Chain A Block    ←──── messages ────→    Chain B Block        │
│        │                                        │               │
│        │           ←──── messages ────→         │               │
│        ↓                                        ↓               │
│   Chain C Block    ←──── messages ────→    Chain D Block        │
│                                                                 │
│   All blocks co-authored, bidirectional messages in one cycle   │
└─────────────────────────────────────────────────────────────────┘
```

## Super-Block Acknowledgements

Instead of acknowledging individual blocks, collators acknowledge the
entire super-block:

```rust
struct SuperBlockAcknowledgement {
    /// Merkle root of constituent block hashes
    super_block_hash: Hash,

    /// Slot the super-block was produced in
    slot: Slot,

    /// Signature from the acknowledging collator
    signature: Signature,
}
```

This binds all constituent blocks together—either all make it to the relay
chain, or the acknowledging collators are slashable.

## Partial Failures

If a collator cannot produce a block for one member chain (e.g., state
unavailable):

1. **Independent chains**: If the failing chain has no message dependencies
   with others in this super-block, other chains can proceed normally.

2. **Dependent chains**: Chains with message dependencies on the failing
   chain must also skip this super-block.

3. **Next collator takes over**: The next collator in the slot rotation
   handles the skipped chains.

## Security: Super-Chain Collusion

**Attack**: All collators in a super-chain collude to equivocate across
chains.

**Mitigation**: Same as Low-Latency v2—requires at least one honest
collator to submit proofs. For high-value super-chains, ensure diverse
collator set.

## Comparison: vs. Parallel Processing Runtimes (Solana-style)

| Aspect | Parallel Runtime | Super Chains |
|--------|------------------|--------------|
| Scaling | Vertical (all nodes process everything) | Horizontal (load distributed) |
| State | All nodes hold all state | Sharded across chains |
| Development | Implicit parallelism | Explicit sharding |
| Hardware | High requirements for all nodes | Lower requirements, specialized by chain |

Super chains provide similar developer experience (tight integration, fast
messaging) while maintaining horizontal scaling.
