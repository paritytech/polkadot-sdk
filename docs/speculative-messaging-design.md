# Speculative Messaging

## Design Document

| Field | Value |
|-------|-------|
| **Authors** | eskimor |
| **Status** | Draft |
| **Version** | 0.4 |
| **Related Designs** | [Low-Latency Parachains v2](low-latency-v2-design.md), [Off-Chain Block Verification](offchain-block-verification-design.md) |

### Version History

| Version | Area | Changes |
|---------|------|---------|
| 0.4 | | **Off-chain verification.** |
| | Header digest | The hash of the `ProvidesRoots` set is additionally committed into a header digest, deposited from the same computation that emits the UMP signal—batches become verifiable against a header alone, no candidate required. Digest format is protocol-standard (foreign nodes check it directly, no wasm): `Consensus(SPMS_ENGINE_ID, blake2_256(set.encode()))`, well-defined via `CommitmentSet`'s canonical encoding. |
| | Consumption tiers | Three tiers formalized: speculative (header digest + off-chain verification stack), optimistic (backed candidates via `CandidateBacked` event / `candidates_pending_availability` API), inclusion-based. Optimistic-tier failure (availability timeout) maps onto the existing enactment-dependency/resubmission machinery. |
| | Separate design | Verification of unincluded sender blocks (header lineage, authorship, ack confidence—generic over parachains, shared with Low-Latency v2) factored out into [Off-Chain Block Verification](offchain-block-verification-design.md). |
| 0.3 | | **Flat per-destination commitments.** |
| | Commitments | Hierarchical top-level Merkle root replaced by a flat canonical set of `(ParaId, per-destination MMR root)` entries (`CommitmentSet`), transported as UMP signals—per the analysis on [PR #10449](https://github.com/paritytech/polkadot-sdk/pull/10449) and the implementation direction ([#12346](https://github.com/paritytech/polkadot-sdk/issues/12346), [#12347](https://github.com/paritytech/polkadot-sdk/issues/12347), [#12350](https://github.com/paritytech/polkadot-sdk/issues/12350)). Top-level inclusion proofs gone; extension proofs only needed when the *specific pair's* MMR grew. |
| | Requires semantics | Explicit: consumed messages are a *prefix* of the required root. Two lifting mechanisms, cleanly separated: in-block catch-up proofs (new) for partial backlog consumption in normal operation, blocks self-contained per the [PR #11413](https://github.com/paritytech/polkadot-sdk/pull/11413) block/candidate split; POV-carried late block proofs (now a single MMR extension proof) for the resubmission flow only. |
| | Relay chain | Per-pair `RecentRoots` window—window depth = proof-free lag tolerance. Matching is one unified check against the window *virtually extended* by the provides of candidates at hand; extensions become permanent on enactment, matches against the virtual part form atomic enactment-dependency groups. |
| | Parachain state | MMR frontiers only (peaks + leaf count), no cached roots; outbound messages a contiguous per-destination vec (host-side append, gaps unrepresentable); positions derived, never stored. |
| | Wire format | `MessageBatch` = `root` + trust-free `base`/`leaf_version` hints + bare payloads; per-message positions and destinations implicit. |
| | Leaf hashing (new) | Domain tags (`LEAF_TAG`/`INNER_TAG`/`PEAK_TAG`) and a leaf-format version byte, each with its attack rationale; preimage deliberately minimal (`LEAF_TAG ++ LEAF_VERSION ++ payload`)—channel context bound structurally by per-pair roots, order/multiplicity by the MMR itself. |
| | Networking (new) | Position-addressed range requests, head queries, sender-side archives. |
| | Security analysis | ParaId-reuse assessed against `paras_registrar` (not practically possible; residual is liveness hygiene), structural replay mitigation, destination-count cap (`MaxCommitmentEntries`). |
| 0.2 | | Revisions before the changelog was introduced (resubmission logic, collator protocol notes, slot-based advertisement check). |
| 0.1 | | Initial version. |

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
   - [Candidate Commitments](#candidate-commitments-verified-by-relay-chain)
   - [Parachain Runtime State](#parachain-runtime-state-internal)
   - [Off-Chain Communication](#off-chain-communication-between-collators)
   - [Relay Chain Matching](#relay-chain-matching)
   - [Catch-Up: Partial Consumption](#catch-up-partial-consumption-in-normal-operation)
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

---

## Introduction

Speculative Messaging introduces a new cross-chain messaging mechanism for
Polkadot that replaces HRMP with a more scalable, lower-latency alternative. By
using cryptographic accumulators (such as Merkle Mountain Ranges) to commit to
messages off-chain and having the relay chain enforce these commitments at
inclusion time, we achieve:

- **Lower latency**: Messaging at parachain block times rather than relay chain
  block times
- **Better scalability**: Off-chain message passing with on-chain commitment
  verification
- **Compatibility with Low-Latency v2**: Works seamlessly with older relay
  parents

This design builds upon and complements the Low-Latency Parachains v2 design.
While that design introduces older relay parents (for relay chain fork
immunity), it would normally increase messaging latency. Speculative Messaging
solves this problem entirely by decoupling message passing from relay parents.

---

## Motivation

### The Problem with Current Messaging (HRMP)

Current cross-chain messaging in Polkadot (HRMP) relies on the relay chain as
the coordination layer:

1. Parachain A produces a block that sends a message
2. The block gets backed and included on the relay chain
3. The relay chain stores the message in its state
4. Parachain B observes the message via its relay parent
5. Parachain B can now receive the message in its next block

This process takes a minimum of 2-3 relay chain blocks (~12-18 seconds) under
ideal conditions. With Low-Latency v2 recommending finalized relay parents (for
fork immunity), this latency would increase significantly if we relied on HRMP.

Additionally, HRMP has scalability concerns:
- Messages flow through relay chain state
- Relay chain must store and manage message queues
- Every validator processes message routing

### Why This Matters

For many cross-chain use cases, 12-18+ second messaging latency is prohibitive:

- **DeFi**: Cross-chain arbitrage, liquidations, and atomic swaps require fast
  execution
- **Gaming**: Interactive cross-chain gameplay needs sub-second responses
- **User Experience**: Multi-chain dApps feel sluggish when every cross-chain
  action takes 20+ seconds

### The Opportunity

By moving message coordination off-chain and using cryptographic commitments for
verification, we can:

1. Achieve messaging latencies comparable to parachain block times
2. Remove message data from relay chain state entirely
3. Build super chains

---

## Goals

1. **Replace HRMP**: Provide a complete replacement for HRMP that is faster and
   more scalable.

2. **Low-Latency Messaging**: Reduce cross-chain messaging latency to parachain
   block times for chains in the same trust domain.

3. **Intra-Block Messaging**: Enable "super chains" (multiple parachains run by
   the same collator set) to exchange messages within the same block production
   cycle.

4. **Off-Chain Scalability**: Keep message data off the relay chain; only
   commitments are verified on-chain.

5. **Graceful Degradation**: When speculative messaging acknowledgements aren't
   available, fall back to inclusion-based commitment matching (still faster
   than HRMP).

6. **Horizontal Scaling**: Maintain Polkadot's horizontal scaling
   properties—full nodes only need to follow chains they care about.

---

## Background

### Relay Parent and Message Context

In current Polkadot, a parachain block's relay parent determines its "view" of
the world, including what messages are available to receive.

With Low-Latency v2, we decouple scheduling from the relay parent, allowing
older (finalized) relay parents for fork immunity. This means the relay
parent—and thus any HRMP-based message receiving context—could be significantly
behind the current relay chain head, making HRMP impractical.

### Low-Latency v2

Low-Latency v2 introduces acknowledgement signatures where collators commit to
blocks becoming canonical and decoupling of candidates from parachain blocks. We
build on those features in this design.

### Merkle Mountain Ranges (MMR)

An MMR is an append-only authenticated data structure that allows:
- Efficient appending of new elements
- Compact proofs of inclusion for any element
- Compact proofs connecting any two points in the accumulator's history

This makes MMRs ideal for accumulating messages over time while allowing
efficient proofs for late-arriving blocks.

---

## Solution Overview

Instead of routing messages through relay chain state, we:

1. **Accumulate Messages**: Each chain maintains one MMR per destination,
   accumulating all messages ever sent to that destination.

2. **Emit Commitments**: Sending chains emit a "provides" commitment (the set of
   per-destination MMR roots that changed in this block); receiving chains emit
   "requires" commitments (per source chain, the source's MMR root for us).

3. **Off-Chain Coordination**: Collators exchange messages directly, without
   relay chain involvement.

4. **Relay Chain Enforcement**: At inclusion time, the relay chain verifies that
   all "requires" are satisfied by corresponding "provides".

5. **Extension Proofs**: Requires may reference an older root than the
   sender's current provides—an MMR extension proof bridges the gap. Carried
   in the block body (catch-up: a lagging receiver consumes only part of a
   backlog) or in the POV (late block: an already-authored block resubmitted
   after the provides moved on).

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
│  (provides: MMR root)  msg passing        (requires: A's MMR root)  │
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
│  Chain B Block M (built against R_N, arrives late)                  │
│  POV includes: proof that R_{N+K} extends R_N                       │
│  PVF lifts B's requires from R_N to R_{N+K} before matching         │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Detailed Design

### Message Accumulators

Each parachain maintains one Merkle Mountain Range (MMR) per destination
parachain, accumulating all messages ever sent to that destination:

```
Chain A (sender):
  ├── MMR(A→B) → [Msg1, Msg2, Msg3, ...]
  ├── MMR(A→C) → [Msg1, Msg2, ...]
  └── MMR(A→D) → [Msg1, ...]
```

**Why per-destination MMRs?**
- Receiver only cares about their own MMR—high volume to other chains does not
  affect them
- Proof size: O(log m) where m = messages to that receiver
- Late block proofs only grow with messages to that specific receiver

The commitment is **flat**: the candidate commits to the set of per-destination
MMR roots directly, as a canonical sorted list of `(ParaId, MmrRoot)` pairs. There
is no top-level Merkle tree over the per-destination roots.

**Why flat and not a top-level Merkle commitment?**

An earlier revision of this design (≤ 0.2) committed to a single top-level
Merkle root over all per-destination MMR roots. The analysis on
[PR #10449](https://github.com/paritytech/polkadot-sdk/pull/10449) showed that
for realistic destination counts (D = 10–100) the flat list wins:

| Scheme | `provides` size | Late proof (top-level part) | Live matching |
|---|---|---|---|
| Top-level Merkle root | 32 B | ~2 × log₂(D) hashes ≈ 250–450 B | hash == hash |
| Flat `(ParaId, Hash)` list | ~36 B × entries | none | lookup by `source` |

- **No inclusion proofs**: the receiver's expected root is directly an entry in
  the sender's provides set—top-level Merkle inclusion proofs (and their two
  extra fields in `LateBlockProof`) disappear entirely.
- **Late proofs only when the pair changed**: with a top-level root, a message
  to *any* destination changes the sender's commitment, forcing every
  slightly-late receiver into a late block proof. With per-pair roots, a late
  proof is only needed when the sender appended messages *to this specific
  receiver* in the meantime.
- **Stable under destination changes**: adding/removing a destination in a
  Merkle tree repositions leaves and invalidates in-flight proofs referencing
  old sibling paths. In the flat list, entries are looked up by `ParaId`;
  in-flight references stay valid.
- **Cost**: commitment size grows linearly with the number of destinations
  *touched in this block* (typically 1–2 entries, ~36 B each). A Merkle
  commitment only wins back at D ≫ 100 simultaneous destinations per block; we
  cap the number of entries (see [Practical Limits](#practical-limits)) instead.

### Candidate Commitments (Verified by Relay Chain)

The commitments are minimal—just the per-destination roots needed for relay
chain verification. They are transported as UMP signals
([#12347](https://github.com/paritytech/polkadot-sdk/issues/12347)) rather than
new fields in `CandidateCommitments`, so no candidate receipt format change is
needed:

```rust
/// Root of a per-destination message MMR (bagged peaks).
///
/// Newtype over `Hash`: roots flow through every layer (commitments, relay
/// storage, wire batches, extension proofs) alongside block hashes, leaf
/// hashes and peaks—confusing them must not typecheck. Leaf and inner-node
/// hashes stay bare `Hash`: they are internal to the accumulator code and
/// already domain-separated cryptographically by the hash tags.
struct MmrRoot(Hash);

/// Canonical, bounded set of (ParaId, MmrRoot) entries. Manual `Decode`
/// REJECTS input whose ParaIds aren't strictly increasing (no silent
/// normalization): the bytes come from untrusted parachain wasm, so
/// malformed sets (duplicate sources with conflicting roots) must fail
/// loudly at the boundary; canonical bytes make decode∘encode the identity
/// (re-encode-and-hash agrees with the original candidate bytes); and other
/// implementations only need "sorted, unique, bounded" instead of replicating
/// a lenient parser's quirks. Construction is sealed: `try_from_iter` (sorts,
/// rejects duplicates) and `Decode` are the only ways in; no mutable access.
struct CommitmentSet(BoundedVec<(ParaId, MmrRoot), MaxCommitmentEntries>);

enum UMPSignal {
    // ... existing signals ...
    /// Sender side: (destination, root of our per-destination MMR) for every
    /// destination whose MMR changed in this block.
    ProvidesRoots(CommitmentSet),
    /// Receiver side: (source, expected root of the source's MMR *for us*)
    /// for every source we processed messages from.
    ///
    /// Semantics: asserts our consumed messages are a *prefix* of this root,
    /// NOT that we consumed exactly up to it. An extension proof lifts the
    /// entry to a newer committed root without consuming the messages in
    /// between—in-block for catch-up during normal operation, POV-carried
    /// for late blocks (see Catch-Up and Late Block Proofs). Consumption
    /// depth is therefore not derivable from requires; don't build
    /// acknowledgement or pruning logic on it.
    RequiresRoots(CommitmentSet),
}
```

Note that the expected root in a requires entry is the source's
**per-destination** MMR root (source→us), not any aggregate over the source's
other destinations.

The relay chain matches each "requires" entry against the corresponding
"provides" entry of the source chain. A parachain block will only be made
available/enacted when all its "requires" are provided.

Since a block only emits provides entries for destinations it actually sent to,
the relay chain maintains the **latest root per (sender, receiver) pair** in
storage (comparable in footprint to today's per-channel HRMP state, ~36 B per
active pair). Additionally, it keeps a small window of recent roots per pair
(see [Relay Chain Matching](#relay-chain-matching)), so receivers that are only
a few blocks behind match directly without a late block proof.

### Parachain Runtime State (Internal)

Each parachain runtime maintains internal state for message tracking.

```rust
/// Index of a message (= leaf) in a per-destination MMR, starting at 0.
///
/// Positions are never stored per message: they are always derived as
/// `frontier leaf count + index into the current block's message vec`. They
/// are not part of the leaf preimage either—order and multiplicity are
/// enforced by the MMR structure itself (permuted or duplicated appends
/// yield a different root). Positions materialize only as addressing in the
/// off-chain fetch protocol and as local bookkeeping. The newtype exists so
/// the places doing this arithmetic can't confuse positions with other
/// counters. Note this is the *leaf index*, not mmr_lib's internal node
/// position (which also counts inner nodes).
struct MessagePosition(u64);

/// The complete, minimal append-state of an MMR: peaks + leaf count. The
/// peaks are the roots of the O(log n) perfect subtrees; *which* peaks exist
/// is fully determined by leaf_count (its binary representation), so nothing
/// else is needed. Appending is O(1) amortized (a new leaf merges with
/// equal-height peaks), the root is computed by bagging the peaks.
///
/// Belongs in `cumulus-primitives-spec-messaging` next to `SpecMerge`—not
/// yet defined there (#12368 left accumulator assembly to callers).
struct MmrFrontier {
    /// Also the position of the next message to append.
    leaf_count: u64,
    /// ≤ 64 peaks for u64 leaf counts.
    peaks: BoundedVec<Hash, ConstU32<64>>,
}

/// Sender-side (in parachain runtime, see #12350):

/// Per-destination MMR *frontiers*: only the O(log n) peaks are stored, not
/// the leaves. The root is computed on demand by bagging the peaks.
OutboundFrontier: StorageMap<ParaId, MmrFrontier>,

/// Messages sent in *this block only*, per destination, so the collator can
/// extract them for off-chain delivery. The MMR frontier is the only
/// long-lived sender state.
///
/// Message i has position `OutboundFrontier[dest].leaf_count + i`: gaps are
/// unrepresentable, contiguity holds by construction (a
/// `(ParaId, position) → payload` map would need that invariant maintained
/// in code instead). A bare vec (no wrapper struct with a base field) so
/// appends can use the host-side storage `append`/`try_append` (as
/// System::Events does): O(1) per message, instead of decode + re-encode of
/// the whole vec per send, which would be quadratic over a block.
OutboundMessages: StorageMap<ParaId, BoundedVec<BoundedVec<u8, MaxMsgLen>, MaxMessagesPerBlock>>,

/// Receiver-side: tracking incoming messages (in parachain runtime)
struct IncomingMessageState {
    /// Per-source tracking
    per_source: BTreeMap<ParaId, SourceState>,
}

struct SourceState {
    /// Frontier of the source's per-destination (source→us) MMR as far as we
    /// have processed it. Both the last processed position (= the frontier's
    /// leaf count) and the root we built against (bag the peaks) are derived
    /// from it: we append incoming message leaves and recompute the root
    /// ourselves.
    frontier: MmrFrontier,
}
```

Note there is no stored root anywhere—on either side. The per-destination roots
that go into `ProvidesRoots` and `RequiresRoots` are computed on demand from
the respective frontiers. Storing a root would just cache a computable value
and add an invariant to maintain.

**Sender lifecycle**: the stored frontier is bumped in the same step that
clears `OutboundMessages`—at the *next* block's initialization, appending the
previous block's leaves. During all of block N the stored frontier therefore
reflects the state as of block N−1, so `position = leaf_count + i` holds
unchanged throughout the block, including in its finalization. The root
committed in block N's `ProvidesRoots` is computed transiently at block end
(bag the stored frontier plus this block's leaves in memory) without
persisting. One write per (block, destination), no mid-block ordering hazard,
and clear + bump are one atomic step. The messages of block N remain readable
in block N's state, which is where the collator extracts them.

### Off-Chain Communication (Between Collators)

Messages are exchanged off-chain between collators. The relay chain never sees
message contents—only commitments.

```rust
/// What a sender shares with receivers (off-chain)
struct MessageBatch {
    /// Source chain
    source: ParaId,
    /// Source block that produced these messages
    source_block: Hash,
    /// The source's per-destination (source→receiver) MMR root after this
    /// block. For a candidate-final block this is the root committed in
    /// `ProvidesRoots`; for blocks batched into one candidate (Basti blocks)
    /// intermediate roots are never committed and serve only as integrity
    /// checkpoints during fetching.
    root: MmrRoot,
    /// Position of the first message. Trust-free convenience: verification
    /// never relies on it (see below)—it serves fetch-protocol addressing
    /// and clean error reporting on mismatch.
    base: MessagePosition,
    /// Leaf-format version to hash these payloads under. A trust-free hint
    /// like `base`: versions are hash-disjoint domains, so only the correct
    /// one can reproduce the committed root—a lie is caught by the root
    /// check. (Strictly an optimization: trying all known versions would
    /// work too.) Single-version per batch by construction: the version is
    /// sender runtime state and can only change at a block boundary, and a
    /// batch is exactly one block's sends.
    leaf_version: u8,
    /// The actual message payloads (XCM or other data), in MMR order.
    /// Message i has position base + i: no per-message positions or
    /// destinations on the wire—gaps and mixed destinations are
    /// unrepresentable, mirroring the sender's storage layout.
    payloads: Vec<Vec<u8>>,
}
```

With the flat commitment there is nothing left to prove about tree structure:
no `provides_root` / `subtree_root` distinction and no inclusion proof. The
receiver verifies by *recomputing*:

1. Hash each payload into its leaf (`H(LEAF_TAG ‖ version ‖ payload)`) and
   append to the tracked frontier for this source, in batch order. Order and
   count need no explicit check: appending in any other order or skipping a
   message yields a different root.
2. Check the recomputed root equals `root`—a lying `base` or version hint
   cannot redirect anything, it only makes this check fail.
3. Authenticate against the source's *committed* state: the root the receiver
   ultimately requires must be (or be lifted via extension proof to) an entry
   in an observed `ProvidesRoots` commitment—intermediate batch roots along
   the way are checkpoints, not trust anchors

**Bounded fetching**: a batch is one source block's sends—bounded by the
sender's block limits, never "everything up to the current root". Lagging
receivers fetch a *sequence* of batches via position-addressed range requests
(see [Networking](#networking)), verify incrementally (each batch's stated
root checkpoints the recomputed frontier—garbage is detected at the first
checkpoint, drop the peer, refetch), and consume as much as fits per block,
requiring a windowed root—directly when the stopping point is one, lifted via
catch-up proof otherwise.

**Consumption boundaries**: a receiving block may stop consuming at any
point—the choice only decides whether a proof is needed. Proof-free requires
stopping at a root that is *currently in the relay's per-pair window*. Note a
source block boundary alone does not guarantee that:

1. The boundary root may have slid out of the window (sender kept sending to
   us).
2. It may never have been committed at all: when several parachain blocks are
   batched into one candidate (Basti blocks), only the batch-final root
   appears in `ProvidesRoots`—intermediate blocks' roots are as uncommitted
   as a mid-batch stop.

Any other stopping point—mid-batch, intermediate batched block, outdated
boundary—is bridged to a windowed root by an extension proof, which is
exactly what the catch-up mechanism provides (see
[Catch-Up](#catch-up-partial-consumption-in-normal-operation)). Per-block
batch roots still serve as integrity checkpoints during fetching either way;
they just aren't all *requirable*.

#### Leaf Hashing and Domain Separation

```
leaf preimage:
  LEAF_TAG ++ LEAF_VERSION ++ payload
```

The preimage is **transient**: assembled, hashed into the leaf, discarded. It
is never stored and never sent. What exists anywhere is the bare `payload`
(sender storage, wire batches, archives) and hashes (frontiers, proofs). Every
hasher reconstructs the preimage itself: tag is a constant, version a
trust-free hint, payload the message.

Every preimage byte has an arguable purpose—and nothing else is included (see
"What is deliberately NOT in the preimage" below):

**Why the domain tag.** Distinct tags (`LEAF_TAG` / `INNER_TAG` / `PEAK_TAG`)
for leaves, inner nodes and peak bagging. Without them, every node is just
`H(bytes)` and the root commits to a web of hash relations, *not* to the
tree's shape—several shapes can be consistent with the same root, and the
prover picks the reading that suits them. Concretely: without tags a 63-byte
payload plus the version byte makes a 64-byte preimage—byte-for-byte also a
valid *inner node* preimage `x ‖ y`, where the last 32 bytes (`y`) are freely
attacker-chosen. Any ordinary user can plant such a leaf simply by sending one
message with crafted content; the honest runtime appends it and commits the
root. With `y = H(leaf of a fabricated message m')`, a proof can later walk
*through* that leaf as if it were an inner node, down to `m'`: every hash
checks, the walk ends at the genuinely committed root, yet `m'` was never
sent. No hash inversion anywhere—the attacker arranged all equations forward,
exploiting only the ambiguous parse. (A size-pedantic verifier could catch the
too-deep path, but that makes soundness a bookkeeping property of every
verifier forever; tags make the ambiguous reading not exist.) Same disease as
Bitcoin's CVE-2012-2459 (Merkle shape ambiguity via duplicated last element,
used to poison block-validity caches); the fix is the standard one from
RFC 6962 §2.1 (Certificate Transparency: leaves hashed as `H(0x00 ‖ …)`,
interior nodes as `H(0x01 ‖ …)`).

**Why a leaf version.** This versions the *preimage layout above*, not the
payload (payloads version themselves, e.g. XCM). It is the tag argument
applied across *time*: if a future format change moves field boundaries, some
byte string can be a valid v1 preimage under one parse and a valid v2 preimage
under another—payload bytes migrating into context fields or back—recreating
the injection attack between format epochs. A version byte at a fixed offset
makes the epochs hash-disjoint: nothing verifies under two formats, a
misapplied format fails loudly at the root check instead of silently
misparsing. It must be present from leaf #0: the MMR is append-only and can
never be rehashed, so the byte cannot be retrofitted (a v1 preimage whose
first byte happens to equal the new version marker would misparse—the
ambiguity returns exactly at the boundary). Example of a plausible future
field: a genesis hash for stronger sender-identity binding, should that ever
become necessary (see the ParaId-reuse threat in Security Analysis—currently
assessed as not practically possible)—a v2 format, deployable per channel
with mixed-version trees instead of a network-wide flag day.

Note the version byte is *not* wire self-description: verifiers never receive
preimages, they reconstruct them. The applicable version arrives as a
trust-free hint (`MessageBatch.leaf_version`) authenticated by the root
check—since versions are hash-disjoint, only the correct one can reproduce a
committed root, so the root check doubles as a version oracle and the hint is
merely an optimization over trying all known versions.

**What is deliberately NOT in the preimage.** Earlier drafts (and the initial
primitives, [#12346](https://github.com/paritytech/polkadot-sdk/issues/12346)/
[PR #12368](https://github.com/paritytech/polkadot-sdk/pull/12368)) also bound
`source`, `destination`, `position` and a length prefix. Removed—each fails
the "name the attack it prevents" test in this architecture:

- **source / destination**: channel context is already bound *structurally*.
  Roots are per (sender, receiver) pair, committed as keyed
  `(destination, root)` entries and stored keyed on the relay—a root never
  exists detached from its channel. A message to C lives under the (A→C)
  root and can never verify against the (A→B) root, regardless of preimage
  contents. Cross-destination/cross-source replay is impossible without any
  leaf-level binding.
- **position**: order and multiplicity are what the MMR *is*—appending the
  same payloads in a different order or count yields a different root.
  Nothing is left for a position field to prevent.
- **length prefix**: with the payload as the single trailing variable-length
  field, the encoding is already injective.

Security must remain arguable: fields without a nameable attack train readers
to stop reasoning ("it's probably needed for something") and mask the fields
that do carry weight. If a future verification mode ever judges leaves by
inclusion proof *without* per-pair root context (light-client evidence,
cross-channel aggregation), the fields it actually needs get added then, with
their argument, via a version bump—that is precisely what `LEAF_VERSION` is
for.

### Relay Chain Matching

When the relay chain processes candidates for inclusion, it performs commitment
matching. The relay chain only sees the minimal commitments (hashes), not
internal state.

The relay chain maintains, per (sender, receiver) pair, a small window of the
most recent provides roots:

```rust
/// Bounded ring of the last W provides roots for one (sender, receiver)
/// pair. Pushed on each inclusion of a sender candidate whose ProvidesRoots
/// carries an entry for this receiver; oldest root drops out. This is the
/// only historical-root storage anywhere in the system—parachain runtimes
/// keep frontiers only, which reproduce just the current root.
struct RecentRoots(BoundedVec<MmrRoot, ConstU32<W>>);

RecentProvides: StorageMap<(ParaId, ParaId), RecentRoots>,
```

The window means a receiver that built against a root from a few sender blocks
ago still matches directly—extension proofs (catch-up or late block) are only
needed when the sender out-ran the window. W is thus the proof-free lag
tolerance, at a cost of W × 32 B per active pair.

#### Matching Against the Virtually Extended Window

There is only *one* check. Candidates arriving together (live communication)
are not a special case: before checking, the stored window is **virtually
extended** by the `ProvidesRoots` of all candidates being processed in this
relay chain block. Every requires entry is then matched against the extended
window. On enactment the transient extensions become permanent (pushed into
`RecentRoots`); if a providing candidate doesn't make it, its extensions
evaporate with it.

```rust
fn verify_requires(
    candidates: &[CandidateReceipt],  // all candidates in this relay block
    stored: &BTreeMap<(ParaId, ParaId), RecentRoots>,
) -> Result<(), Error> {
    // Transient: stored window ∪ provides of the candidates at hand
    let window = VirtualWindow::new(stored, candidates);

    for receiver_candidate in candidates {
        let receiver = receiver_candidate.para_id();
        for (source, expected_root) in receiver_candidate.requires_roots().iter() {
            if !window.contains((*source, receiver), expected_root) {
                // Not stored, not provided alongside - needs a late block
                // proof in the POV (resubmission flow)
                return Err(Error::RequiresProof);
            }
        }
    }
    Ok(())
}
```

Mutual dependencies (A requires B's provides and vice versa, the Basti block /
super-chain case) match naturally—both entries are in the virtual extension.
The price is that matches against the virtual part create **enactment
dependencies**: a candidate whose requires matched another candidate's
transient provides can only enact if that candidate enacts. Dependent groups
become available/enacted atomically—all or nothing (see
[Cycle Prevention](#cycle-prevention)).

Note the graceful property of per-pair roots: if the sender sent *no further
messages to this receiver*, the stored root is unchanged no matter how much the
sender talked to other chains—the receiver matches directly without any proof.
Under the old top-level-root scheme, any unrelated send would have forced a
late block proof.

### Catch-Up: Partial Consumption in Normal Operation

A receiver can fall behind without any block being late: the sender produced
many blocks' worth of messages while the receiver produced none (offline,
on-demand, congested). Consuming the whole backlog in one block may exceed its
weight/POV budget—so a *current* block must be able to emit a requires that
matches the sender's current provides **without having consumed all messages**.
The prefix semantics of requires permit exactly this; the question is where
the proof lives.

Where does the proof live? The deciding question is *when the need for it is
known*. Here the backlog is visible at authoring time, so the proof goes into
the block itself. That keeps the block **self-contained**—a pure function of
its own body and parent state: every collator can verify and acknowledge it
as-is, and nothing extra needs to be distributed alongside the block for it
to be valid. Putting authoring-time-known data into the POV instead would
break exactly that: the block's requires would only match with side data
attached, so the bare block would no longer speak for itself.

Late block proofs (next section) are the opposite case: the need arises only
*after* the block is sealed, so the block body is no longer an option—the POV
is, and legitimately so, since a resubmitting collator assembles a fresh
candidate anyway ([PR #11413](https://github.com/paritytech/polkadot-sdk/pull/11413):
blocks are permanent, candidates transient).

So the proof goes *into the block*. Incoming messages already enter via an
inherent ([#12531](https://github.com/paritytech/polkadot-sdk/issues/12531));
the same inherent carries, per lagging source, the lift for the unconsumed
tail:

```rust
/// Proves that the MMR at an older root is a prefix of the MMR at a newer
/// root (append-only extension). O(log n) hashes; carries no message
/// payloads and no per-message leaf hashes. Mirrors mmr_lib's
/// `AncestryProof`; verification in Appendix B.
struct MMRExtensionProof {
    /// Peaks of the MMR at the old root (bag them to reproduce it)
    old_peaks: Vec<Hash>,
    /// Leaf count at the old root—determines which peaks must exist
    old_leaf_count: u64,
    /// Nodes connecting the old peaks into the new MMR
    connecting_nodes: Vec<Hash>,
}

/// Part of the messaging inherent (in the block body, not the POV)
struct CatchUpProof {
    source: ParaId,
    /// A currently committed (source→us) root, ahead of what we consume
    new_root: MmrRoot,
    /// Extension proof: our post-consumption frontier root is a prefix
    /// of new_root
    extension: MMRExtensionProof,
}
```

The runtime appends the consumed leaves to its frontier, verifies the
extension proof from the resulting (possibly intermediate) root to `new_root`
as part of the STF, and emits the requires entry at `new_root` directly. No
PVF special-casing, no commitment transformation—the block validates anywhere
from its own body. Unconsumed messages stay pending; the frontier catches up
over subsequent blocks, each within its weight budget.

Two regimes, graduated by lag:
1. **Small lag**: the consumed-boundary root is still in the relay's per-pair
   window → direct match, no proof at all. Window depth = proof-free lag
   tolerance.
2. **Backlog beyond the window**: in-block catch-up proof as above, O(log n)
   (~1 KB) regardless of backlog size.

### Late Block Proofs

Late block proofs solve a different problem: the block was *already authored*
against state that was current then, and arrives at the relay chain after the
window slid past. The block body can't be changed anymore (blocks are
permanent, candidates transient—[PR
#11413](https://github.com/paritytech/polkadot-sdk/pull/11413)), so the proof
lives in the POV, and the PVF overrides the block's requires with the lifted
entry. This only arises in the resubmission flow, where a collator assembles a
fresh candidate/POV around the unaltered block anyway—and each resubmission
attempt regenerates the proof against the *then-current* provides, so the
block never goes stale no matter how often it is retried. Blocks in normal
operation never carry one. The mechanism is similar to the scheduling parent
header chain in Low-Latency v2.

#### The Problem

```
Timeline (roots of A's per-destination MMR for B):
  Block A_N: sends to B, provides (B, R_N)
  Block A_{N+1}: sends to B again, provides (B, R_{N+1})
  Block A_{N+2}: sends to B again, provides (B, R_{N+2})

  Block B_M: built against A_N's state (requires (A, R_N))

  By the time B_M arrives at the relay chain, A_{N+2} is already included and
  R_N has dropped out of the relay chain's provides window for (A, B).
  B_M's requires (R_N) doesn't match any stored root (latest: R_{N+2}).
```

Note this only happens when A kept sending *to B*. If A's later blocks sent to
other chains only, the stored (A, B) root would still be R_N and B_M would
match directly.

#### The Solution

The late block includes a proof in its POV (outside the block itself)
demonstrating that the root it built against is an ancestor of the current one:
since MMRs are append-only, a single extension proof shows that the messages
B processed are a prefix of the current MMR.

```rust
/// Late block proof included in POV (not in commitments!)
struct LateBlockProof {
    /// Source chain this proof is for
    source: ParaId,

    /// The current (source→us) provides root we're updating to
    new_root: MmrRoot,

    /// Proof that the MMR at the old root (the block's requires) is a prefix
    /// of the MMR at new_root (defined in Catch-Up above)
    extension: MMRExtensionProof,
}
```

This is the entire proof: with the flat commitment there are no top-level
inclusion proofs to carry (an earlier revision needed two Merkle proofs plus
the extension proof).

```
      MMR(A→B) at R_old                MMR(A→B) at R_new
      (what B_M built against)         (currently committed by A)

leaves: [0 ........ 41]                [0 ........ 41 | 42 ...... 99]
         ^^^^^^^^^^^^^                  ^^^^^^^^^^^^^   ^^^^^^^^^^^^
         consumed by B_M                same leaves,    appended since;
                                        untouched       NOT in the proof

Extension proof (all the POV carries):

    R_new
     ├──▣ inner nodes covering leaves 0..41   ──┐ old peaks: bag them
     │        ▲                                 │ and R_old must come out
     │     old peaks ───────────────────────────┘
     └──▣ O(log n) hashes summarizing 42..99  ← messages 42..99 appear
                                                 ONLY as aggregate hashes

Neither payloads nor per-message leaf hashes of 42..99 are included. B_M
proves its consumed prefix is contained in R_new while knowing nothing about
the newer messages—proof size stays O(log n) regardless of how many were
appended (see Requires semantics: prefix-of, not consumed-exactly).
```

The extension proof itself is identical in shape to the one carried in-block
by catch-up proofs—only the transport (POV vs. inherent) and the verifier
(PVF transform vs. STF) differ.

#### Verification

The PVF verifies the late block proof and **transforms** the block's original
`requires` entry into an updated one that references the current per-pair
root. This way, the relay chain only ever sees a commitment it can verify
against currently-available state.

```rust
fn process_late_block_requires(
    source: ParaId,
    expected_root: MmrRoot,  // From the block itself (references old root)
    proof: &LateBlockProof,  // From POV
) -> Result<(ParaId, MmrRoot), Error> {
    // Verify the old MMR is a prefix of the MMR at new_root
    verify_mmr_extension(expected_root, proof.new_root, &proof.extension)?;

    // Return UPDATED entry for the candidate's RequiresRoots.
    // The relay chain will verify this against the stored provides window.
    Ok((source, proof.new_root))
}
```

Note: The PVF verifies the proof—the relay chain only sees the transformed
commitment. Message positions, MMR sizes, and proof details are all internal to
the parachain. The proof just demonstrates that the messages the receiver
processed are a prefix of the source's current per-destination MMR.

### Proof Size Considerations

With Low-Latency v2 allowing relay parents up to ~14,400 blocks old (24 hours),
we must consider commitment and proof sizes for worst-case scenarios.

#### Commitment Size

- `ProvidesRoots`: ~36 B per destination *touched in this block*—typically 1–2
  entries. A chain fanning out to 100 destinations in one block commits ~3.6 KB.
- `RequiresRoots`: ~36 B per source processed in this block.

#### Late Block Proof Size

A late block proof is a single MMR extension proof per stale source:
O(log m) where m = messages the source has sent to this receiver.

- Typical (1000 messages to us): ~log₂(1000) ≈ 10 hashes ≈ 320 bytes
- Worst case (24 hours of messages to one receiver, ~10⁹ leaves): ~30 hashes
  ≈ 960 bytes

There is no top-level proof component: proof size is entirely independent of
how many destinations the sender talks to and how much it sends to others.

#### Practical Limits

Proofs are expected to stay small and should therefore practically fit into any
POV. To be sure, we should nevertheless set aside a few kB (e.g. 50) for not
breaking the late submission opportunity due to the POV getting too large.

The number of entries in a `CommitmentSet` is capped
(`MaxCommitmentEntries`, on the order of 100–200). This bounds candidate
receipt growth and relay chain matching work per candidate. Chains needing to
fan out to more destinations than the cap must spread the sends over multiple
blocks—at which point (D ≫ 100 *per block*, sustained) a top-level Merkle
commitment would be worth reconsidering.

Per-destination MMRs naturally keep proofs small because:
- Receiver only proves against their own MMR
- That MMR only contains messages to that specific receiver
- High volume to other chains doesn't affect proof size

### Acknowledgement Extensions

For low-latency chains using speculative messaging, the acknowledgement rules
from Low-Latency v2 are extended:

#### Extended Rule for Message Dependencies

> A collator must not acknowledge a block if it depends on speculative messages
  from blocks that are not yet sufficiently confirmed.

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

For different trust domains, acknowledgement of Block B depends on relay chain
inclusion of Block A instead of collator acknowledgement.

### Cycle Prevention

When two chains want to exchange messages speculatively in the same block, we
risk deadlock: each waits for the other's acknowledgement. For non-super chains
(above scenario), we trivially break cycles, by sticking to the procedure
above. In particular t=1: We only process the messages in block `A` once we
have seen the entire block. By doing this both ways, block `A` can not depend
on the current block `B`, because it did not exist when `A` was built. This
holds even for multi-party communication.

Conclusion: By not allowing intra-block communication, no cycles between blocks
can exist and above acknowledgment procedure is sound. For Basti Blocks, we
will end up with cycles between POVs, but those don't seem problematic, apart
from the fact that those candidates can only become available atomically: All
or nothing.

### Super Chains

Super chains are a set of parachains operated by the same collator set, enabling
the tightest possible integration including intra-block messaging.

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

Within a super-block, messages can flow in both directions between any member
chains because:

1. The same collator produces all blocks
2. They have access to all chains' state simultaneously 
3. They can resolve message dependencies during block production
4. Cycles are fine and supported

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

Instead of acknowledging individual blocks, collators acknowledge the entire
super-block:

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

#### Partial Failures

If a collator cannot produce a block for one member chain (e.g., state
unavailable):

1. **Independent chains**: If the failing chain has no message dependencies with
   others in this super-block, other chains can proceed normally.

2. **Dependent chains**: Chains with message dependencies on the failing chain
   must also skip this super-block.

3. **Next collator takes over**: The next collator in the slot rotation handles
   the skipped chains.

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
- Still faster than HRMP (off-chain message passing, on-chain commitment
  verification only)

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

Speculative messaging introduces new censorship dynamics that must be
understood.

### Cascading Dependencies

If Chain A's backing group censors Chain A's block, and Chain B has a `requires`
dependency on that block:

- Chain B's block cannot be included until Chain A's block is included
- If Chain A is delayed long enough, Chain B's availability will time out and B
  must be resubmitted
- When both are resubmitted (likely around the same time), they'll typically
  arrive together—no late block proof needed

### Mitigation Strategies

#### 1. Domain Size Limits

Limit trust domains to a reasonable size (e.g., 5-10 chains). This bounds the
"blast radius" of cascading delays.

#### 2. Resubmission

If Chain A is censored long enough that Chain B's availability times out, Chain
B simply resubmits. Since both chains are likely resubmitting around the same
time, they'll typically be included together without needing late block proofs,
although they are available if necessary, adding robustness.

#### 3. On-Demand Parachains

If a chain detects persistent censorship, it can use on-demand parachain slots
(different backing group) to get a block included.

#### 4. Cross-Domain Independence

Organize chains such that critical paths don't depend on speculative messaging
across many chains. Keep the speculative "hot path" short; use inclusion-based
for less time-sensitive communication.

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

Super chains provide similar developer experience (tight integration, fast
messaging) while maintaining horizontal scaling.

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

1. **New UMP signals**: `ProvidesRoots` / `RequiresRoots` carrying
   `CommitmentSet`s ([#12347](https://github.com/paritytech/polkadot-sdk/issues/12347)).
   Reusing UMP signals avoids a candidate receipt format change. Rollout
   caveat: older validators reject unknown UMP signals, so the node-side
   support must be deployed to a supermajority of validators before the
   corresponding `node_features` bit is enabled.
2. **Per-pair provides storage**: Maintain a window of recent roots per
   (sender, receiver) pair, updated on inclusion of sender candidates. Entries
   must be pruned when a parachain is offboarded.
3. **Commitment matching**: At inclusion time, verify each requires entry
   `(source, expected_root)` against the stored window *virtually extended*
   by the `ProvidesRoots` of the candidates at hand—one unified check;
   extensions become permanent on enactment, and matches against the virtual
   part create atomic enactment dependencies.

Note: The relay chain has no MMR verification logic and does not track message
history. Extension proofs are verified in the receiving parachain's STF
(catch-up, in-block) or in the PVF, which transforms commitments before the
relay chain sees them (late block, POV). The relay chain only performs simple
hash matching.

### PVF Changes

Similar to how Low-Latency v2 introduces a separate PVF entry point for
scheduling information (verifying header chains and signed core selection),
speculative messaging requires PVF logic for processing late block proofs and
transforming commitments.

The PVF receives additional inputs via the POV (outside the block itself):

```rust
struct MessagingProofInputs {
    /// Late block proofs for each source chain where the block's requires
    /// references an older root than currently available
    late_block_proofs: Vec<LateBlockProof>,
}
```

The PVF then:

1. **Executes the block**: The block produces `requires` commitments based on
   the messages it processed (referencing the `provides` roots it was built
   against)

2. **Processes late block proofs**: For each requires entry where a
   `LateBlockProof` is provided:
   - Verifies the extension proof connects the old root (the block's
     `expected_root` for this source) to the new root (`proof.new_root`)
   - Transforms the entry to reference the new root

3. **Outputs transformed commitments**: The `RequiresRoots` commitment set
   contains the (possibly transformed) entries that the relay chain can verify
   against currently available provides

```rust
fn process_messaging_commitments(
    block_requires: CommitmentSet,        // From block execution
    proof_inputs: &MessagingProofInputs,  // From POV
) -> Result<CommitmentSet, Error> {
    CommitmentSet::try_from_iter(block_requires.iter().map(|(source, root)| {
        if let Some(proof) = find_proof_for_source(&proof_inputs, *source) {
            // Transform: verify proof and update to current root
            process_late_block_requires(*source, *root, proof)
        } else {
            // No transformation needed - block was built against a root still
            // in the relay chain's provides window
            Ok((*source, *root))
        }
    }))
}
```

This follows the same pattern as the scheduling parent header chain in
Low-Latency v2: the PVF verifies proofs and transforms inputs so the relay chain
only sees commitments it can verify against current state.

### Parachain Runtime Changes 

1. **MMR maintenance**: Append messages to the per-destination MMR frontiers,
   emit `ProvidesRoots` for destinations touched in this block
2. **Requires generation**: Track incoming frontiers per source, emit
   `RequiresRoots`
3. **Trust domain configuration**: Define trusted peers for speculative messaging
4. **Message processing**: Consume incoming batches via the messaging inherent,
   appending to the per-source frontier; verify catch-up proofs in the STF

Note: this document deviates from the initial sketches in
[#12346](https://github.com/paritytech/polkadot-sdk/issues/12346) /
[#12350](https://github.com/paritytech/polkadot-sdk/issues/12350) and the
merged primitives ([PR #12368](https://github.com/paritytech/polkadot-sdk/pull/12368))
in three places:
- `OutboundMessages` is a per-destination vec rather than a
  `(ParaId, position)` map (invalid states unrepresentable, host-side append)
- the off-chain batch carries `base + payloads` rather than per-message
  `(destination, position, payload)`—positions are derived, not stored
- the leaf preimage is reduced to `LEAF_TAG ++ LEAF_VERSION ++ payload`;
  `source`/`destination`/`position`/length carry no arguable attack in this
  architecture (see Leaf Hashing) and were dropped

The issues and the primitives crate should be updated accordingly.

### Collator Changes

1. **Cross-chain message fetching**: Obtain messages from peer chains
2. **MMR proof generation**: Create extension proofs for late blocks. This is
   necessarily node-side: the runtime only stores frontiers (peaks), which is
   enough to *verify* but not to *generate* an ancestry proof. The receiver's
   node has the required data anyway—its old frontier plus all subsequently
   fetched messages for the (source→us) pair let it rebuild the MMR segment
   and generate the proof off-chain.
3. **Extended acknowledgement rules**: Verify message dependencies before acknowledging
4. **Super-block production** (if applicable): Coordinate multi-chain block production

### Networking

Addressing is always by *message position*, never by block. Per-pair positions
are dense (0, 1, 2, ... with no gaps, by construction), so a receiver resuming
after downtime does not traverse the sender's blocks looking for ones that
sent to it—source blocks that sent nothing simply don't occupy position space.
Blocks appear only as batch boundaries in responses: the committed roots a
receiver can emit requires at.

Sender-side full nodes maintain an off-chain (destination, position) → message
archive, built while following their own chain (`OutboundMessages` only holds
the current block), and serve:

```rust
/// "Give me your messages for me, from position `start` onward."
struct MessageRangeRequest {
    /// The requesting (receiving) chain.
    destination: ParaId,
    /// Typically the receiver's frontier leaf count.
    start: MessagePosition,
    /// Response size bound—fetching stays chunked and resumable no matter
    /// how large the backlog.
    max_bytes: u32,
}

/// Consecutive `MessageBatch`es (each one source block's sends, with its
/// boundary root), starting at `start`, ending early if `max_bytes` is hit.
struct MessageRangeResponse {
    batches: Vec<MessageBatch>,
}

/// "Where does (you → me) currently stand?" Lets a resuming receiver size
/// its backlog and pick the root to catch up toward.
struct HeadRequest { destination: ParaId }
struct HeadResponse {
    /// Leaf count of the sender's per-destination MMR.
    head: MessagePosition,
    /// Root and source block it was committed in.
    root: MmrRoot,
    source_block: Hash,
}
```

Beyond request/response, the protocol needs:

1. **Live propagation**: push new `MessageBatch`es to (collators of) the
   destination chain on block production—the speculative hot path.
2. **Acknowledgement propagation**: quick distribution of acknowledgement
   signatures (Low-Latency v2).
3. **MMR state sharing**: allow peers to request proofs/peaks where node-local
   data doesn't suffice.

### Off-Chain Verification

Consuming messages *before* the sending block's provides commitment is on
chain (the speculative and optimistic tiers) requires verifying the sending
block itself: header lineage from included state, authorship by a currently
valid collator, and acknowledgement confidence. This is a generic subsystem
(shared with Low-Latency v2 ack verification) and specified separately:
[Off-Chain Parachain Block
Verification](offchain-block-verification-design.md).

Interface points with this document:

- The sender commits the hash of its `ProvidesRoots` set into a **header
  digest**, deposited via `frame_system::deposit_log` at block end from the
  same computation that emits the UMP signal. This lets a batch be verified
  against a header alone—no candidate required.

  Unlike headers and ack blobs (chain-opaque, judged by the chain's own wasm
  via [Off-Chain Block
  Verification](offchain-block-verification-design.md)), this check is
  performed by the *foreign node directly*—a pure function of header and
  set, no state involved. The digest format is therefore **protocol
  standard**, not chain-internal:

  - `DigestItem::Consensus(SPMS_ENGINE_ID, blake2_256(provides_set.encode()))`,
    at most one per header (engine id value TBD);
  - well-defined because `CommitmentSet`'s encoding is canonical—one logical
    set, exactly one hash;
  - receiver check: recompute batch root → `(receiver, root)` ∈ supplied
    set → `blake2_256(set.encode())` == digest payload.

  A chain deviating from the format self-excludes: receivers cannot verify
  its digests and it simply gets no speculative delivery.

  Freedom removed by this standardization: participating chains must use the
  standard Substrate header layout (this is the one place a foreign node
  parses a header itself—everywhere else headers stay chain-opaque), and
  multiple messaging pallet instances must aggregate into one set per
  header. Both constraints gate only the speculative/optimistic tiers;
  inclusion-based messaging rides on UMP signals and is header-format
  agnostic. Hash and encoding were already protocol-fixed at the messaging
  layer, so no chain-internal choice is overridden.
- Tiers, all served by that digest:

| Tier | Provides root source | Trust |
|---|---|---|
| Speculative | Header digest + off-chain verification stack | Trust domain (acks + slashing) |
| Optimistic (backed) | `CandidateBacked` event (`HeadData` incl. digest) or `candidates_pending_availability` API | Backing validity; availability may time out |
| Inclusion-based | `CandidateIncluded` / `paras::Heads` | Relay chain |

The optimistic tier's failure mode (provider availability timeout) is exactly
the enactment-dependency / resubmission machinery already specified—a
latency-vs-certainty knob, not a new mechanism.

---

## Security Analysis

### Threat: Fake Provides

**Attack**: Malicious collator claims a provides root that doesn't match the
messages actually sent.

**Mitigation**: Commitments are not collator-supplied—they are outputs of
block execution, byte-checked against the PVF's result by validators. A
provides root not produced by the block's actual sends cannot appear in a
valid candidate. Independently, receivers recompute the root from the payloads
they are served and only build against roots they could reproduce.

### Threat: Invalid Extension Proof

**Attack**: Late block includes a fabricated extension proof.

**Mitigation**: Extension proofs are cryptographically verified by the PVF.
Invalid proofs cause candidate validation to fail.

### Threat: Message Replay/Skip

**Attack**: Receiving chain processes messages out of order or skips messages.

**Mitigation**: The parachain runtime tracks which messages have been processed
and enforces consecutive processing. This is internal to the parachain—the relay
chain only sees the resulting `requires` commitment.

### Threat: Acknowledgement Without Verification

**Attack**: Collator acknowledges a block without verifying message
availability.

**Mitigation**: If the block later fails inclusion due to unmet requires, the
acknowledging collator violated Low-Latency v2 rules and is slashable.

### Threat: Cross-Destination Forgery / Replay

**Attack**: A message sent to chain C is replayed to chain B, or a message is
replayed/reordered within a channel.

**Mitigation**: Structural, not leaf-level. Channel context is bound by the
per-pair roots themselves: a message to C lives only under the committed
(A→C) root and cannot verify against the (A→B) root a receiver checks. Order
and multiplicity within a channel are what the MMR structure encodes—replayed
or reordered messages yield a different root. Domain tags prevent inner nodes
or roots from being reinterpreted as leaves (see
[Leaf Hashing](#leaf-hashing-and-domain-separation), including what is
deliberately *not* in the preimage and why).

### Threat: ParaId Reuse

**Attack**: A parachain is deregistered and its ParaId later reassigned to a
different chain, which would inherit the trust relationships and receivers'
tracked frontiers keyed by that ParaId.

**Assessment**: Not practically possible—verified against `paras_registrar`.
New ParaIds come exclusively from `NextFreeParaId`, a monotonically increasing
counter, so fresh reservations never hand out an old id. Deregistration
removes the registrar entry entirely (`Paras::take`), after which *nobody*—
including the former manager—can register at that id again: registration
requires a reservation, and reservations only come from the counter. The only
path to resurrecting an old id is root (`force_register`), i.e. governance,
which is outside the threat model. (The registrar's `swap` was also checked:
it exchanges leases/scheduling status between two live paras via `OnSwap`,
not chain identity—code, state and MMR history stay at their ids.)

**Residual (liveness, not security)**: should governance ever resurrect an old
id with a fresh chain, receivers' stale frontiers would never match the new
chain's MMR—a stuck channel, not forgery (nothing wrong verifies; nothing
verifies at all). Hygiene that covers this and plain state bloat: the relay
prunes `RecentProvides` entries on offboarding, and receivers reset their
`SourceState` for a source they observe being offboarded. Should stronger
identity binding ever become necessary (e.g. genesis hash in the leaf
preimage), the leaf-format version byte allows adding it.

### Threat: Super-Chain Collusion

**Attack**: All collators in a super-chain collude to equivocate across chains.

**Mitigation**: Same as Low-Latency v2—requires at least one honest collator to
submit proofs. For high-value super-chains, ensure diverse collator set.

---

## Conclusion

Speculative Messaging replaces HRMP with a more scalable, lower-latency
alternative that:

- **Eliminates relay chain message storage**: Messages flow off-chain; only
  commitments are verified on-chain
- **Enables parachain-speed messaging**: Within trust domains, messaging latency
  drops to parachain block times
- **Supports super chains**: Tightly coupled chains can exchange messages within
  the same block production cycle
- **Gracefully handles lag and late blocks**: MMR extension proofs let lagging
  receivers consume backlogs incrementally (in-block catch-up) and let
  resubmitted blocks with older requirements still be included (POV late
  block proofs)
- **Maintains horizontal scaling**: Even for super chains: Full nodes can still
  be per chain and don't need to keep the entire state or process all sub-chain
  blocks.

Combined with Low-Latency Parachains v2, this positions Polkadot to offer user
experiences competitive with monolithic chains while preserving its core value
propositions of decentralization, security, and horizontal scalability.

---

## Appendix A: Separation of Concerns

Different layers handle different data:

| Layer | Data | Purpose |
|-------|------|---------|
| **UMP Signals** | `ProvidesRoots` / `RequiresRoots` commitment sets of `(ParaId, per-destination MMR root)` | Relay chain verification |
| **Relay Chain State** | Window of recent roots per (sender, receiver) pair | Matching against included blocks |
| **Messaging Inherent (block body)** | Incoming messages, catch-up extension proofs | Consume messages; lift requires past unconsumed backlog (self-contained) |
| **Late Block Proofs (POV)** | MMR extension proofs | Prove old requires is a prefix of current provides (resubmission only) |
| **Parachain Runtime** | Per-destination / per-source MMR frontiers | Internal bookkeeping |
| **Off-Chain (Collators)** | Actual messages | Message delivery |

The relay chain only sees hashes. It verifies that provides/requires match. It
never sees message contents, MMR sizes, or processing positions—proofs are
verified in the receiving runtime's STF (catch-up) or the PVF (late block).

## Appendix B: MMR Extension Proof Details

An MMR extension proof demonstrates that a newer MMR root extends an older
one. The structure is defined at first use (see
[Catch-Up](#catch-up-partial-consumption-in-normal-operation)); in the
implementation it is covered by `polkadot-ckb-merkle-mountain-range`
(`gen_ancestry_proof` / `verify_incremental`), an audited `no_std` workspace
dependency—no hand-rolled accumulator (see
[PR #12368](https://github.com/paritytech/polkadot-sdk/pull/12368)).
Conceptual verification:

```rust
impl MMRExtensionProof {
    fn verify(
        &self,
        old_root: MmrRoot,
        new_root: MmrRoot,
    ) -> bool {
        // 1. The claimed old peaks must reproduce the old root
        if bag_peaks(&self.old_peaks) != old_root {
            return false;
        }

        // 2. The peak structure (count and heights) must be exactly the one
        //    determined by old_leaf_count
        if !peaks_consistent_with(&self.old_peaks, self.old_leaf_count) {
            return false;
        }

        // 3. Recompute the new root, treating the old peaks as opaque,
        //    fixed subtrees and merging in the connecting nodes. This can
        //    only reproduce new_root if the old MMR's leaves are a strict
        //    prefix of the new MMR's leaves.
        bag_peaks(&merge_prefix(&self.old_peaks, &self.connecting_nodes))
            == new_root
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
| Cycle prevention | No intra block communication apart from super chains (wait for next block, not inclusion) |

## Appendix D: Commitment Schema Summary

```rust
// === COMMITMENTS (UMP signals, verified by relay chain) ===

// Canonical sorted set, strictly increasing ParaIds enforced at decode
struct CommitmentSet(BoundedVec<(ParaId, MmrRoot), MaxCommitmentEntries>);

enum UMPSignal {
    // ...
    ProvidesRoots(CommitmentSet),  // (destination, our MMR root for them)
    RequiresRoots(CommitmentSet),  // (source, their MMR root for us)
}

// === CATCH-UP PROOF (in the block body, via the messaging inherent) ===
// Normal operation: lift requires past an unconsumed backlog; block stays
// self-contained.

struct CatchUpProof {
    source: ParaId,
    new_root: MmrRoot,
    extension: MMRExtensionProof,
}

// === LATE BLOCK PROOF (in POV, not commitments) ===
// Resubmission flow only: PVF overrides the unaltered block's requires.

struct LateBlockProof {
    source: ParaId,
    new_root: MmrRoot,
    extension: MMRExtensionProof,
}

// === RELAY CHAIN STATE ===

// Window of recent roots per (sender, receiver) pair

// === PARACHAIN RUNTIME STATE (internal, not on relay chain) ===

// Sender tracks: per-destination MMR frontiers (roots computed on demand)
// Receiver tracks: per-source MMR frontier (position and root derived from it)

// === OFF-CHAIN (between collators) ===

// MessageBatch: source, source_block, root, base, leaf_version, payloads
// (positions implicit: base + i; verification derives them from the
// receiver's own frontier; base and leaf_version are trust-free hints,
// authenticated by the root check)
```

## Appendix E: Comparison of Messaging Modes

| Mode | Latency | Trust | Use Case |
|------|---------|-------|----------|
| Super-chain (intra-block) | < 1 block | Same collator set | Tightly coupled shards |
| Speculative (acknowledged) | ~1-2 blocks | Trust domain collators | Fast cross-chain DeFi |
| Inclusion-based | ~2-3 relay blocks | Relay chain only | Cross-domain, untrusted |
| HRMP (legacy) | ~3+ relay blocks | Relay chain only | Deprecated |
