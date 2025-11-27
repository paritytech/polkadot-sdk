# Speculative Messaging

## Design Document

| Field | Value |
|-------|-------|
| **Authors** | Robert Klotzner |
| **Status** | Draft |
| **Version** | 0.2 |
| **Related Designs** | [Low-Latency Parachains v2](link-to-low-latency-doc) |

---

## Table of Contents

1. [Introduction](#introduction)
2. [Motivation](#motivation)
3. [Goals](#goals)
4. [Non-Goals](#non-goals)
5. [Background](#background)
6. [Solution Overview](#solution-overview)
7. [Detailed Design](#detailed-design)
   - [Message Accumulators](#message-accumulators)
   - [Provides Commitment](#provides-commitment)
   - [Requires Commitment](#requires-commitment)
   - [Relay Chain Matching](#relay-chain-matching)
   - [Late Block Proofs](#late-block-proofs)
   - [Proof Size Considerations](#proof-size-considerations)
   - [Acknowledgement Extensions](#acknowledgement-extensions)
   - [Cycle Prevention](#cycle-prevention)
   - [Super Chains](#super-chains)
8. [Trust Domains](#trust-domains)
9. [Censorship Considerations](#censorship-considerations)
10. [Comparison with Alternatives](#comparison-with-alternatives)
11. [Implementation Considerations](#implementation-considerations)
12. [Security Analysis](#security-analysis)
13. [Future Work](#future-work)

---

## Introduction

Speculative Messaging introduces a new cross-chain messaging mechanism for Polkadot that replaces HRMP with a more scalable, lower-latency alternative. By using cryptographic accumulators (such as Merkle Mountain Ranges) to commit to messages off-chain and having the relay chain enforce these commitments at inclusion time, we achieve:

- **Lower latency**: Messaging at parachain block times rather than relay chain block times
- **Better scalability**: Off-chain message passing with on-chain commitment verification
- **Compatibility with Low-Latency v2**: Works seamlessly with older relay parents

This design builds upon and complements the Low-Latency Parachains v2 design. While that design introduces older relay parents (for relay chain fork immunity), it would normally increase messaging latency. Speculative Messaging solves this problem entirely by decoupling message passing from relay parents.

---

## Motivation

### The Problem with Current Messaging (HRMP)

Current cross-chain messaging in Polkadot (HRMP) relies on the relay chain as the coordination layer:

1. Parachain A produces a block that sends a message
2. The block gets backed and included on the relay chain
3. The relay chain stores the message in its state
4. Parachain B observes the message via its relay parent
5. Parachain B can now receive the message in its next block

This process takes a minimum of 2-3 relay chain blocks (~12-18 seconds) under ideal conditions. With Low-Latency v2 recommending finalized relay parents (for fork immunity), this latency would increase significantly if we relied on HRMP.

Additionally, HRMP has scalability concerns:
- Messages flow through relay chain state
- Relay chain must store and manage message queues
- Every validator processes message routing

### Why This Matters

For many cross-chain use cases, 12-18+ second messaging latency is prohibitive:

- **DeFi**: Cross-chain arbitrage, liquidations, and atomic swaps require fast execution
- **Gaming**: Interactive cross-chain gameplay needs sub-second responses
- **User Experience**: Multi-chain dApps feel sluggish when every cross-chain action takes 20+ seconds

### The Opportunity

By moving message coordination off-chain and using cryptographic commitments for verification, we can:

1. Achieve messaging latencies comparable to parachain block times
2. Remove message data from relay chain state entirely
3. Build super chains

---

## Goals

1. **Replace HRMP**: Provide a complete replacement for HRMP that is faster and more scalable.

2. **Low-Latency Messaging**: Reduce cross-chain messaging latency to parachain block times for chains in the same trust domain.

3. **Intra-Block Messaging**: Enable "super chains" (multiple parachains run by the same collator set) to exchange messages within the same block production cycle.

4. **Off-Chain Scalability**: Keep message data off the relay chain; only commitments are verified on-chain.

5. **Graceful Degradation**: When speculative messaging acknowledgements aren't available, fall back to inclusion-based commitment matching (still faster than HRMP).

6. **Horizontal Scaling**: Maintain Polkadot's horizontal scaling properties—full nodes only need to follow chains they care about.

---

## Background

### Relay Parent and Message Context

In current Polkadot, a parachain block's relay parent determines its "view" of the world, including what messages are available to receive.

With Low-Latency v2, we decouple scheduling from the relay parent, allowing older (finalized) relay parents for fork immunity. This means the relay parent—and thus any HRMP-based message receiving context—could be significantly behind the current relay chain head, making HRMP impractical.

### Low-Latency v2

Low-Latency v2 introduces acknowledgement signatures where collators commit to blocks becoming canonical and decoupling of candidates from parachain blocks. We build on those features in this design.

### Merkle Mountain Ranges (MMR)

An MMR is an append-only authenticated data structure that allows:
- Efficient appending of new elements
- Compact proofs of inclusion for any element
- Compact proofs connecting any two points in the accumulator's history

This makes MMRs ideal for accumulating messages over time while allowing efficient proofs for late-arriving blocks.

---

## Solution Overview

Instead of routing messages through relay chain state, we:

1. **Accumulate Messages**: Each chain maintains an MMR of all outgoing messages to all destinations.

2. **Emit Commitments**: Sending chains emit a "provides" commitment (the MMR root); receiving chains emit "requires" commitments (per source chain).

3. **Off-Chain Coordination**: Collators exchange messages directly, without relay chain involvement.

4. **Relay Chain Enforcement**: At inclusion time, the relay chain verifies that all "requires" are satisfied by corresponding "provides".

5. **Late Block Proofs**: When blocks arrive at different times, the late block includes a proof in its POV connecting the current provides to its older requires.

```
┌──────────────────────────────────────────────────────────────────────┐
│                     Current HRMP Flow (Slow)                         │
├──────────────────────────────────────────────────────────────────────┤
│  Chain A Block    →    Relay Chain     →    Relay Chain  →  Chain B  │
│  (sends msg)           stores msg           State lookup    receives │
│                        ~12-18s total                                 │
└──────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│                  Speculative Messaging (Fast)                       │
├─────────────────────────────────────────────────────────────────────┤
│  Chain A Block    →    Off-chain     →    Chain B Block             │
│  (provides: MMR)       msg passing        (requires: A's MMR pos)   │
│                        ~block time                                  │
│                                                                     │
│  Relay chain only verifies: provides(A) satisfies requires(B)       │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│              Late Block with Proof (Fallback)                       │
├─────────────────────────────────────────────────────────────────────┤
│  Chain A Block N   ...time passes...   Chain A Block N+K            │
│  (provides: R_N)                       (provides: R_{N+K})          │
│                                                                     │
│  Chain B Block M (late, requires A at position P from block N)      │
│  POV includes: proof that R_{N+K} extends R_N covering position P   │
│  Commitment includes: matched requires with proof reference         │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Detailed Design

### Message Accumulators

Each parachain maintains a Merkle Mountain Range (MMR) accumulating all outgoing messages:

We use a hierarchical structure: per-destination MMRs with a top-level Merkle commitment.

```
Top-Level Root (Merkle tree over per-destination MMR roots)
├── Chain B: MMR_B Root → [Msg1, Msg2, Msg3, ...]
├── Chain C: MMR_C Root → [Msg1, Msg2, ...]
├── Chain D: MMR_D Root → [Msg1, ...]
└── ...
```

**Why hierarchical?**
- Receiver only needs to prove their subtree, not traverse all messages
- Proof size: O(log D + log m) where D = destinations, m = messages to receiver
- Much better than O(k log n) for a flat structure where k =number of messages to prove, n = total number of messages sent by the chain.
- Late block proofs only grow with messages to that specific receiver

### Candidate Commitments (Verified by Relay Chain)

The commitments in candidate receipts are minimal—just the hashes needed for relay chain verification:

```rust
/// In candidate commitments - what the relay chain verifies
struct ProvidesCommitment {
    /// Top-level Merkle root over all per-destination MMR roots
    root: Hash,
}

struct RequiresCommitment {
    /// Source parachain we're receiving from
    source: ParaId,
    /// The root we built against (from source's provides)
    expected_root: Hash,
}
```

The relay chain verifies matches the "requires" commitment with the corresponding"provides" commitmentment. A parachain block will only be made available/enacted when all its "requires" are provided.

### Parachain Runtime State (Internal)

Each parachain runtime maintains internal state for message tracking.

```rust
/// Sender-side: tracking outgoing messages (in parachain runtime)
struct OutgoingMessageState {
    /// Per-destination MMRs
    per_destination: BTreeMap<ParaId, MMR>,
    /// Current top-level root (this goes into ProvidesCommitment)
    current_root: Hash,
}

/// Receiver-side: tracking incoming messages (in parachain runtime)  
struct IncomingMessageState {
    /// Per-source tracking
    per_source: BTreeMap<ParaId, SourceState>,
}

struct SourceState {
    /// Last processed position in source's per-destination MMR for us
    last_processed: u64,
    /// The source's root we last built against
    last_seen_root: Hash,
    /// The source's per-destination MMR root for us
    /// TODO: Why do we need this?
    last_seen_subtree_root: Hash,
}
```

### Off-Chain Communication (Between Collators)

Messages are exchanged off-chain between collators. The relay chain never sees message contents—only commitments.

```rust
/// Message exchanged off-chain between collators
struct OutgoingMessage {
    /// Destination parachain
    destination: ParaId,
    /// Message payload (actual XCM or other data)
    payload: Vec<u8>,
    /// Position in sender's per-destination MMR
    /// Is this field needed? Or can this be constructed from the proof?
    position: u64,
}

/// What a sender shares with receivers (off-chain)
struct MessageBatch {
    /// Source chain
    source: ParaId,
    /// Source block that produced these messages
    source_block: Hash,
    /// The provides root for this block
    provides_root: Hash,
    /// Our per-destination MMR root (for the receiver)
    /// TODO: Why do we need this?
    subtree_root: Hash,
    /// Proof that subtree_root is in provides_root
    subtree_inclusion_proof: MerkleProof,
    /// The actual messages
    messages: Vec<OutgoingMessage>,
}
```

Receivers verify:
1. `subtree_inclusion_proof` proves `subtree_root` is in `provides_root`
2. Messages hash to leaves in the subtree MMR
3. Messages are sequential from last processed position

### Relay Chain Matching

When the relay chain processes candidates for inclusion, it performs commitment matching.
The relay chain only sees the minimal commitments (hashes), not internal state.

#### Live Communication (Simultaneous Arrival)

When both sending and receiving blocks arrive at the relay chain at approximately the same time:

```rust
fn verify_live_matching(
    sender_candidate: &CandidateReceipt,
    receiver_candidate: &CandidateReceipt,
) -> Result<(), Error> {
    let provides = &sender_candidate.commitments.provides;
    let requires = receiver_candidate.commitments.requires
        .iter()
        .find(|r| r.source == sender_candidate.para_id)
        .ok_or(Error::NoRequirement)?;
    
    // Direct match: receiver expects exactly what sender provides
    if requires.expected_root == provides.root {
        return Ok(());
    }
    
    Err(Error::MissingRequirement)
}
```

#### Matching with Included Blocks

For requirements against already-included blocks:

```rust
fn verify_against_included(
    receiver_candidate: &CandidateReceipt,
    included_provides: &BTreeMap<ParaId, Hash>,  // Just the roots
) -> Result<(), Error> {
    for requires in &receiver_candidate.commitments.requires {
        let provides_root = included_provides
            .get(&requires.source)
            .ok_or(Error::MissingRequirement)?;
        
        if &requires.expected_root == provides_root {
            // Exact match
            continue;
        }
        
        // Roots don't match - need late block proof in POV
        return Err(Error::RequiresProof);
    }
    Ok(())
}
```

### Late Block Proofs

When a receiving block's requirements reference an older state than what's currently available, we need a proof mechanism. This is similar to the scheduling parent header chain in Low-Latency v2.

#### The Problem

```
Timeline:
  Block A_N: provides root R_N
  Block A_{N+1}: provides root R_{N+1}
  Block A_{N+2}: provides root R_{N+2}
  
  Block B_M: built expecting A_N's state (requires.expected_root = R_N)
  
  By the time B_M arrives at relay chain, A_{N+2} is already included.
  B_M's requires (R_N) doesn't match current provides (R_{N+2}).
```

#### The Solution

The late block includes a proof in its POV (outside the block itself) demonstrating that the messages it processed are still valid under the current provides root.

With the hierarchical structure, B only needs to prove its subtree:

```rust
/// Late block proof included in POV (not in commitments!)
struct LateBlockProof {
    /// Source chain this proof is for
    source: ParaId,
    
    /// Prove our subtree was in the old (expected) root
    old_subtree_root: Hash,
    old_subtree_proof: MerkleProof,
    
    /// The current provides root we're updating to
    new_provides_root: Hash,
    
    /// Prove our subtree is in the new (current) root
    new_subtree_root: Hash,
    new_subtree_proof: MerkleProof,
    
    /// If our subtree's MMR grew, prove it extended correctly
    subtree_extension: Option<MMRExtensionProof>,
}

/// MMR extension proof (only needed if new messages were added to our subtree)
struct MMRExtensionProof {
    /// MMR proof data (peaks and connecting nodes)
    proof: Vec<Hash>,
}
```

#### Verification

The PVF verifies the late block proof and **transforms** the block's original `requires` commitment into an updated one that references the current `provides` root. This way, the relay chain only ever sees a commitment it can verify against currently-available state.

```rust
fn process_late_block_requires(
    block_requires: &RequiresCommitment,  // From the block itself (references old root)
    proof: &LateBlockProof,               // From POV
) -> Result<RequiresCommitment, Error> {
    // 1. Verify old subtree was in the root the block expected
    verify_merkle_proof(
        block_requires.expected_root,
        &proof.old_subtree_proof,
        (block_requires.source, proof.old_subtree_root),
    )?;
    
    // 2. Verify new subtree is in the current root (which we'll output)
    verify_merkle_proof(
        proof.new_provides_root,
        &proof.new_subtree_proof,
        (block_requires.source, proof.new_subtree_root),
    )?;
    
    // 3. Verify subtrees are related (same or extended)
    if proof.old_subtree_root != proof.new_subtree_root {
        if let Some(ext) = &proof.subtree_extension {
            // Subtree grew - verify extension
            verify_mmr_extension(
                proof.old_subtree_root,
                proof.new_subtree_root,
                ext,
            )?;
        } else {
            return Err(Error::SubtreeChangedWithoutProof);
        }
    }
    
    // 4. Return UPDATED commitment for the candidate
    // The relay chain will verify this against the current provides root
    Ok(RequiresCommitment {
        source: block_requires.source,
        expected_root: proof.new_provides_root,
    })
}
```

Note: The PVF verifies the proof—the relay chain only sees the transformed commitment. Message ranges, MMR sizes, and proof details are all internal to the parachain. The proof just demonstrates that the receiver's view of their subtree is consistent with the current provides root.

### Proof Size Considerations

With the hierarchical structure and Low-Latency v2 allowing relay parents up to ~14,400 blocks old (24 hours), we must consider proof sizes for worst-case scenarios.

#### Late Block Proof Components

A late block proof consists of:
1. **Top-level Merkle proofs**: O(log D) where D = number of destinations
2. **Subtree MMR extension proof**: O(log m) where m = messages to this receiver

#### Proof Size Analysis

For a sender with 100 destinations, receiver getting 1000 messages:
- Top-level proofs: ~2 × log₂(100) ≈ 14 hashes ≈ 450 bytes
- Subtree extension: ~log₂(1000) ≈ 10 hashes ≈ 320 bytes
- **Total: ~770 bytes**

Worst case (1000 destinations, 24 hours of messages to one receiver):
- Top-level proofs: ~2 × log₂(1000) ≈ 20 hashes ≈ 640 bytes
- Subtree extension: ~30 hashes ≈ 960 bytes
- **Total: ~1.6 KB**

This is much better than a flat structure where proof size depends on ALL messages, not just messages to the receiver.

#### Practical Limits

Proofs are expected to stay small and should therefore practically fit into any POV. To be sure, we should nevertheless set aside a few kB (e.g. 50) for not breaking the late submission opportunity due to the POV getting too large.

The hierarchical structure naturally keeps proofs small because:
- Receiver only proves their subtree
- Subtree only contains messages to that specific receiver
- High volume to other chains doesn't affect proof size

### Acknowledgement Extensions

For low-latency chains using speculative messaging, the acknowledgement rules from Low-Latency v2 are extended:

#### Extended Rule for Message Dependencies

> A collator must not acknowledge a block if it depends on speculative messages from blocks that are not yet sufficiently confirmed.

"Sufficiently confirmed" depends on the trust relationship:

| Source Chain Type | Confirmation Required |
|-------------------|----------------------|
| Same super-chain | Same super-block (co-authored) |
| Same trust domain (low-latency) | Acknowledged by source chain collators |
| Different trust domain | Included on relay chain |

#### Acknowledgement Timing

```
Timeline for Block B receiving message from Block A (same trust domain):

t=0:    Chain A collator produces Block A (sends message, provides P_A)
t=1:    Chain B collator sees Block A + messages, produces Block B (requires P_A)
t=1:    Chain A collator acknowledges Block A (in parallel with above)
t=2:    Chain B collator sees A's acknowledgement, acknowledges Block B
...
t=N:    Both blocks included on relay chain, commitments verified
```

Note: Block *building* can proceed immediately upon seeing messages. Only *acknowledgement* of the receiving block must wait for acknowledgement of the sending block.

For different trust domains, acknowledgement of Block B waits for relay chain inclusion of Block A instead of collator acknowledgement.

### Cycle Prevention

When two chains want to exchange messages speculatively in the same block, we risk deadlock: each waits for the other's acknowledgement.

#### The Odd/Even Solution

We break cycles using a deterministic, alternating priority scheme:

```rust
fn can_send_speculatively_this_block(
    source_para: ParaId,
    dest_para: ParaId, 
    source_block_number: BlockNumber,
) -> bool {
    let is_lower_id = source_para.0 < dest_para.0;
    let is_odd_block = source_block_number % 2 == 1;
    
    // Odd blocks: lower ParaId sends speculatively (higher waits for next block)
    // Even blocks: higher ParaId sends speculatively (lower waits for next block)
    (is_odd_block && is_lower_id) || (!is_odd_block && !is_lower_id)
}
```

**How it works:**

- On odd blocks: ParaId 100 can send speculatively to ParaId 200, but 200 must wait until the next block (even) to send back speculatively
- On even blocks: ParaId 200 can send speculatively to ParaId 100, but 100 must wait until the next block (odd)
- Over time, both directions get equal opportunities for speculative messaging
- Worst-case round-trip latency: **2 parachain blocks** (not relay chain inclusion!)

**Why this prevents cycles:**

For a cycle A→B→C→A to form, each link would need to be speculative in the same block. But the odd/even rule ensures that for any pair, only one direction is speculative per block. Therefore, at least one link must wait for the next block, breaking the cycle.

### Super Chains

Super chains are a set of parachains operated by the same collator set, enabling the tightest possible integration including intra-block messaging.

#### Definition

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

#### Super-Block Production

When a collator's slot arrives, they produce blocks for ALL member chains atomically:

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
        // Merkle root of constituent block hashes for efficient individual proofs
        let block_hashes: Vec<(ParaId, Hash)> = self.blocks
            .iter()
            .map(|(id, b)| (*id, b.hash()))
            .collect();
        merkle_root(&block_hashes)
    }
}
```

#### Intra-Block Messaging

Within a super-block, messages can flow in both directions between any member chains because:

1. The same collator produces all blocks
2. They have access to all chains' state simultaneously  
3. They can resolve message dependencies during block production
4. The odd/even rule doesn't apply (same author, coordinated production)

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

#### Super-Block Acknowledgements

Instead of acknowledging individual blocks, collators acknowledge the entire super-block:

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

This binds all constituent blocks together—either all make it to the relay chain, or the acknowledging collators are slashable.

#### Partial Failures

If a collator cannot produce a block for one member chain (e.g., state unavailable):

1. **Independent chains**: If the failing chain has no message dependencies with others in this super-block, other chains can proceed normally.

2. **Dependent chains**: Chains with message dependencies on the failing chain must also skip this super-block.

3. **Next collator takes over**: The next collator in the slot rotation handles the skipped chains.

---

## Trust Domains

Not all chains trust each other equally. We organize chains into trust domains:

```
┌─────────────────────────────────────────────────────────────────┐
│                         Trust Domain A                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐              │
│  │  Chain 1    │←→│  Chain 2    │←→│  Chain 3    │              │
│  │  (super)    │  │  (super)    │  │             │              │
│  └─────────────┘  └─────────────┘  └─────────────┘              │
│         ↑               ↑               ↑                       │
│         └───────────────┴───────────────┘                       │
│              Fast speculative messaging                         │
│              (acknowledgement-based)                            │
└─────────────────────────────────────────────────────────────────┘
          │ 
          │ Inclusion-based (still faster than HRMP,
          │ still off-chain, just waits for provides inclusion)
          ↓
┌─────────────────────────────────────────────────────────────────┐
│                         Trust Domain B                          │
│  ┌─────────────┐  ┌─────────────┐                               │
│  │  Chain 4    │←→│  Chain 5    │                               │
│  └─────────────┘  └─────────────┘                               │
└─────────────────────────────────────────────────────────────────┘
```

#### Within a Trust Domain

- Speculative messaging based on acknowledgements
- Low latency (parachain block times)
- Chains trust each other's collators to acknowledge honestly

#### Across Trust Domains  

- Inclusion-based messaging (wait for provides to be included)
- Higher latency but no trust assumptions beyond relay chain
- Still faster than HRMP (off-chain message passing, on-chain commitment verification only)

#### Establishing Trust

Trust domains are configured at the parachain runtime level:

```rust
// In parachain runtime configuration
parameter_types! {
    pub TrustedPeers: Vec<ParaId> = vec![
        ParaId(1001),  // Trust chain 1001 for speculative messaging  
        ParaId(1002),  // Trust chain 1002
    ];
}
```

---

## Censorship Considerations

Speculative messaging introduces new censorship dynamics that must be understood.

### Cascading Dependencies

If Chain A's backing group censors Chain A's block, and Chain B has a `requires` dependency on that block:

- Chain B's block cannot be included until Chain A's block is included
- If Chain A is delayed long enough, Chain B's availability will time out and B must be resubmitted
- When both are resubmitted (likely around the same time), they'll typically arrive together—no late block proof needed

### Mitigation Strategies

#### 1. Domain Size Limits

Limit trust domains to a reasonable size (e.g., 5-10 chains). This bounds the "blast radius" of cascading delays.

#### 2. Resubmission

If Chain A is censored long enough that Chain B's availability times out, Chain B simply resubmits. Since both chains are likely resubmitting around the same time, they'll typically be included together without needing late block proofs, although they are available if necessary, adding robustness.

#### 3. On-Demand Parachains

If a chain detects persistent censorship, it can use on-demand parachain slots (different backing group) to get a block included.

#### 4. Cross-Domain Independence

Organize chains such that critical paths don't depend on speculative messaging across many chains. Keep the speculative "hot path" short; use inclusion-based for less time-sensitive communication.

---

## Comparison with Alternatives

### vs. Current HRMP

| Aspect | HRMP | Speculative Messaging |
|--------|------|----------------------|
| Latency | 12-18+ seconds | Parachain block time (speculative) or 2 relay blocks (inclusion-based) |
| Scalability | Limited (relay chain state) | High (off-chain, only commitments on-chain) |
| Trust | Relay chain only | Relay chain + optional collator acknowledgements |
| Message data | Flows through relay chain | Never touches relay chain |

### vs. Parallel Processing Runtimes (Solana-style)

| Aspect | Parallel Runtime | Super Chains |
|--------|------------------|--------------|
| Scaling | Vertical (all nodes process everything) | Horizontal (load distributed) |
| State | All nodes hold all state | Sharded across chains |
| Development | Implicit parallelism | Explicit sharding |
| Hardware | High requirements for all nodes | Lower requirements, specialized by chain |

Super chains provide similar developer experience (tight integration, fast messaging) while maintaining horizontal scaling.

### vs. Ethereum L2 Preconfirmations  

| Aspect | Preconfirmations | Speculative Messaging |
|--------|------------------|----------------------|
| Confirmation source | L1 validators | Parachain collators |
| Complexity | Very high (L1 understands L2 txs) | Moderate (chain-agnostic commitments) |
| Decentralization | Often centralized sequencers | Decentralized collator sets |
| Enforcement | Limited (many failure modes) | Higher (clear rules) |

---

## Implementation Considerations

### Relay Chain Runtime Changes

1. **New commitment types**: Add `provides` and `requires` to candidate commitments
2. **Commitment matching**: At inclusion time, verify that each `requires.expected_root` matches a `provides.root` from a currently backed or included candidate

Note: The relay chain has no MMR verification logic and does not track history. All proof verification happens in the PVF, which transforms commitments before the relay chain sees them. The relay chain only performs simple hash matching on current candidates.

### PVF Changes

Similar to how Low-Latency v2 introduces a separate PVF entry point for scheduling information (verifying header chains and signed core selection), speculative messaging requires PVF logic for processing late block proofs and transforming commitments.

The PVF receives additional inputs via the POV (outside the block itself):

```rust
struct MessagingProofInputs {
    /// Late block proofs for each source chain where the block's requires
    /// references an older root than currently available
    late_block_proofs: Vec<LateBlockProof>,
}
```

The PVF then:

1. **Executes the block**: The block produces `requires` commitments based on the messages it processed (referencing the `provides` roots it was built against)

2. **Processes late block proofs**: For each `requires` commitment where a `LateBlockProof` is provided:
   - Verifies the proof connects the old root (block's `requires.expected_root`) to the new root (`proof.new_provides_root`)
   - Transforms the commitment to reference the new root

3. **Outputs transformed commitments**: The candidate commitments contain the (possibly transformed) `requires` that the relay chain can verify against currently available `provides`

```rust
fn process_messaging_commitments(
    block_requires: Vec<RequiresCommitment>,  // From block execution
    proof_inputs: &MessagingProofInputs,      // From POV
) -> Result<Vec<RequiresCommitment>, Error> {
    block_requires.into_iter().map(|req| {
        if let Some(proof) = find_proof_for_source(&proof_inputs, req.source) {
            // Transform: verify proof and update to current root
            process_late_block_requires(&req, proof)
        } else {
            // No transformation needed - block was built against current root
            Ok(req)
        }
    }).collect()
}
```

This follows the same pattern as the scheduling parent header chain in Low-Latency v2: the PVF verifies proofs and transforms inputs so the relay chain only sees commitments it can verify against current state.

### Parachain Runtime Changes  

1. **MMR maintenance**: Append messages to outgoing MMR, emit provides
2. **Requires generation**: Track incoming message positions, emit requires
3. **Trust domain configuration**: Define trusted peers for speculative messaging
4. **Message processing**: Consume messages based on requires ranges

### Collator Changes

1. **Cross-chain message fetching**: Obtain messages from peer chains
2. **MMR proof generation**: Create extension proofs for late blocks
3. **Extended acknowledgement rules**: Verify message dependencies before acknowledging
4. **Super-block production** (if applicable): Coordinate multi-chain block production

### Networking

1. **Message propagation**: Efficient cross-chain message dissemination
2. **Acknowledgement propagation**: Quick distribution of acknowledgement signatures
3. **MMR state sharing**: Allow peers to request MMR proofs

---

## Security Analysis

### Threat: Fake Provides

**Attack**: Malicious collator claims provides root that doesn't match actual messages.

**Mitigation**: Receiving chains verify actual message content against the requires commitment. The MMR root commits to specific message hashes. Any mismatch is detectable.

### Threat: Invalid Extension Proof

**Attack**: Late block includes a fabricated extension proof.

**Mitigation**: Extension proofs are cryptographically verified by the PVF. Invalid proofs cause candidate validation to fail.

### Threat: Message Replay/Skip

**Attack**: Receiving chain processes messages out of order or skips messages.

**Mitigation**: The parachain runtime tracks which messages have been processed and enforces consecutive processing. This is internal to the parachain—the relay chain only sees the resulting `requires` commitment.

### Threat: Acknowledgement Without Verification

**Attack**: Collator acknowledges a block without verifying message availability.

**Mitigation**: If the block later fails inclusion due to unmet requires, the acknowledging collator violated Low-Latency v2 rules and is slashable.

### Threat: Super-Chain Collusion

**Attack**: All collators in a super-chain collude to equivocate across chains.

**Mitigation**: Same as Low-Latency v2—requires at least one honest collator to submit proofs. For high-value super-chains, ensure diverse collator set.

---

## Conclusion

Speculative Messaging replaces HRMP with a more scalable, lower-latency alternative that:

- **Eliminates relay chain message storage**: Messages flow off-chain; only commitments are verified on-chain
- **Enables parachain-speed messaging**: Within trust domains, messaging latency drops to parachain block times
- **Supports super chains**: Tightly coupled chains can exchange messages within the same block production cycle
- **Gracefully handles late blocks**: MMR extension proofs allow blocks with older requirements to still be included
- **Maintains horizontal scaling**: Even for super chains: Full nodes can still be per chain and don't need to keep the entire state or process all sub-chain blocks.

Combined with Low-Latency Parachains v2, this positions Polkadot to offer user experiences competitive with monolithic chains while preserving its core value propositions of decentralization, security, and horizontal scalability.

---

## Appendix A: Separation of Concerns

Different layers handle different data:

| Layer | Data | Purpose |
|-------|------|---------|
| **Candidate Commitments** | `provides.root`, `requires.{source, expected_root}` | Relay chain verification |
| **Late Block Proofs (POV)** | Merkle proofs, MMR extension proofs | Prove old requires valid under new provides |
| **Parachain Runtime** | MMR structures, message positions, last processed indices | Internal bookkeeping |
| **Off-Chain (Collators)** | Actual messages, inclusion proofs | Message delivery |

The relay chain only sees hashes. It verifies that provides/requires match (or that a valid proof exists). It never sees message contents, MMR sizes, or processing positions.

## Appendix B: MMR Extension Proof Details

An MMR extension proof demonstrates that a newer MMR root extends an older one:

```rust
/// MMR extension proof structure
struct MMRExtensionProof {
    /// Peaks of the old MMR
    old_peaks: Vec<Hash>,
    
    /// Peaks of the new MMR  
    new_peaks: Vec<Hash>,
    
    /// Nodes connecting old peaks to new peaks
    /// (proves old peaks are prefix of new structure)
    connecting_nodes: Vec<Hash>,
}

impl MMRExtensionProof {
    fn verify(
        &self,
        old_root: Hash,
        new_root: Hash,
    ) -> bool {
        // 1. Verify old_peaks produce old_root
        let computed_old_root = bag_peaks(&self.old_peaks);
        if computed_old_root != old_root {
            return false;
        }
        
        // 2. Verify new_peaks produce new_root
        let computed_new_root = bag_peaks(&self.new_peaks);
        if computed_new_root != new_root {
            return false;
        }
        
        // 3. Verify old structure is prefix of new structure
        // using connecting_nodes
        verify_prefix_relationship(
            &self.old_peaks,
            &self.new_peaks,
            &self.connecting_nodes,
        )
    }
}
```

## Appendix C: Acknowledgement Rule Summary

| Rule | Description |
|------|-------------|
| Base rules | All rules from Low-Latency v2 |
| Message verification | Don't acknowledge if dependent messages aren't confirmed |
| Same super-chain | Messages from co-authored blocks are immediately trusted |
| Same trust domain | Wait for source block acknowledgement |
| Cross-domain | Wait for source block inclusion on relay chain |
| Cycle prevention | Respect odd/even sending restrictions (wait for next block, not inclusion) |

## Appendix D: Commitment Schema Summary

```rust
// === CANDIDATE COMMITMENTS (minimal, verified by relay chain) ===

struct ProvidesCommitment {
    root: Hash,  // Top-level Merkle root over per-destination MMR roots
}

struct RequiresCommitment {
    source: ParaId,
    expected_root: Hash,
}

// === LATE BLOCK PROOF (in POV, not commitments) ===

struct LateBlockProof {
    source: ParaId,
    old_subtree_root: Hash,
    old_subtree_proof: MerkleProof,
    new_subtree_root: Hash,
    new_subtree_proof: MerkleProof,
    subtree_extension: Option<MMRExtensionProof>,
}

// === PARACHAIN RUNTIME STATE (internal, not on relay chain) ===

// Sender tracks: per_destination MMRs, current top-level root
// Receiver tracks: last_processed position per source, last seen roots

// === OFF-CHAIN (between collators) ===

// MessageBatch: source, provides_root, subtree_root, subtree_proof, messages
```

## Appendix E: Comparison of Messaging Modes

| Mode | Latency | Trust | Use Case |
|------|---------|-------|----------|
| Super-chain (intra-block) | < 1 block | Same collator set | Tightly coupled shards |
| Speculative (acknowledged) | ~1-2 blocks | Trust domain collators | Fast cross-chain DeFi |
| Inclusion-based | ~2-3 relay blocks | Relay chain only | Cross-domain, untrusted |
| HRMP (legacy) | ~3+ relay blocks | Relay chain only | Deprecated |
