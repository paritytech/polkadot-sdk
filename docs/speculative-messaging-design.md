# Speculative Messaging

## Design Document

| Field | Value |
|-------|-------|
| **Authors** | eskimor |
| **Status** | Ready for review |
| **Version** | 0.6 |
| **Related Designs** | [Low-Latency Parachains v2](low-latency-v2-design.md), [Off-Chain Block Verification](offchain-block-verification-design.md), [Super Chains](super-chains-design.md) |

### Version History

Each entry says what changed and which sections to (re)read; small
changesets can be absorbed from here alone, large ones (like 0.5) mean
re-reading the listed sections.

| Version | Changes |
|---------|---------|
| 0.6 | Hardening and simplification, driven by implementation review. **Security fix**: stream roots now bind their leaf count, closing a position-forgery attack ([#12659 review](https://github.com/paritytech/polkadot-sdk/pull/12659#discussion_r4285793338)). **Channel layer redesigned** for simplicity: sessions with receiver-advertised version negotiation, two signals, a draining close, sender-side flow control instead of a receiver window, delivery by enqueueing. **Fetch protocol unified**: one request pair serves every read, head reads included; one serving obligation replaces three rules. **Made implementable**: receiver state and the messaging inherent restructured around typed ids and the runtime's own inherent filtering; relay matching and lifting clarified (group enactment, root choice); proof sizes measured and bounded by type. The 0.5 architecture is unchanged. **Re-read**: Channels and Flow Control, Fetch Protocol, Liftability and Proof Size Bounds entirely; elsewhere The Stream Commitment Tree, Candidate Commitments, Parachain Runtime State, Runtime API, Messaging Inherent, Verification, Relay Chain Matching, Requires Lifting, Leaf Hashing, Event Streams, Archive Pruning, Cycle Handling, Establishing Trust, Implementation Considerations, Security Analysis (new Position Forgery threat). |
| 0.5 | Root-hash-only commitments return (reverting 0.3's flat sets, as its own analysis [PR #10449](https://github.com/paritytech/polkadot-sdk/pull/10449) anticipated): relay state is a fixed-size ring of recent `StreamsRoot`s per sender; the structured `StreamId` replaces the plain destination `ParaId`; channels become unidirectional with flow control via a lossy per-channel register (acceptance, advisory credit, watermark, close); lossy broadcast streams (pub-sub). **Requires handling unified into one code path for all scenarios** (steady state, partial consumption, resubmission, bundles): blocks emit no `Requires` and never see a `StreamsRoot`—they record per touched stream an interval (consumption start/end), the validate_block wrapper stitches the intervals across the bundle (gaps advance-proven forward) and synthesizes the candidate's entries via one POV-carried lift per stream, binding the chain's endpoint; 0.3's in-block catch-up proofs and separate late-block proofs are gone. Authoring always targets the newest root at its tier; the relay window is pipeline slack only. Fetch protocol reduced to two root-keyed request pairs, every response independently verifiable against a requester-named root. Lift-serving obligations bounded at 25 h, mirroring availability retention. **Full rewrite—re-read the Detailed Design entirely**, plus Trust Domains (the tier table). |
| 0.4 | The provides commitment is additionally deposited as a header digest, so batches verify against a header alone; consumption tiers formalized (speculative / optimistic / inclusion). Verification of unincluded sender blocks factored out into [Off-Chain Block Verification](offchain-block-verification-design.md). **Read**: Off-Chain Verification. |
| 0.3 | Top-level Merkle commitment replaced by flat per-destination `(ParaId, root)` sets (superseded in 0.5). Requires semantics made explicit (consumed = *prefix* of the required root); in-block catch-up proofs and POV late block proofs cleanly separated; per-pair `RecentRoots` window with virtual extension and atomic enactment dependencies; frontier-only parachain state; leaf hashing with domain tags and version byte; position-addressed networking; security analysis (ParaId reuse, replay). **Largely a full rewrite of the detailed design.** |
| 0.2 | Revisions before the changelog was introduced (resubmission logic, collator protocol notes, slot-based advertisement check). |
| 0.1 | Initial version. |

---

## Table of Contents

1. [Introduction](#introduction)
2. [Motivation](#motivation)
3. [Goals](#goals)
4. [Non-Goals](#non-goals)
5. [Background](#background)
6. [Solution Overview](#solution-overview)
7. [Detailed Design](#detailed-design)
   - [Message Accumulators and Streams](#message-accumulators-and-streams)
   - [Candidate Commitments](#candidate-commitments-verified-by-relay-chain)
   - [Parachain Runtime State](#parachain-runtime-state-internal)
   - [Off-Chain Communication](#off-chain-communication-between-collators)
   - [Relay Chain Matching](#relay-chain-matching)
   - [Requires Lifting](#requires-lifting-pov-proofs)
   - [Proof Size Considerations](#proof-size-considerations)
   - [Channels and Flow Control](#channels-and-flow-control)
   - [Event Streams](#event-streams)
   - [Acknowledgement Extensions](#acknowledgement-extensions)
   - [Cycle Handling](#cycle-handling)
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
- **Unbounded channels**: As many channels and lanes per peer as chains
  want—no relay-chain deposits or state per channel, no governance in the
  loop; acceptance is priced locally by the receiving chain
- **Native pub-sub**: Broadcast event streams any chain can subscribe to
  unilaterally—oracle feeds, notifications—a long-requested capability
  ([#606](https://github.com/paritytech/polkadot-sdk/issues/606)) that HRMP
  has no answer to, delivered here without a single byte of relay chain
  state per feed or subscriber
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

For many cross-chain use cases—DeFi arbitrage, liquidations and atomic
swaps, interactive multi-chain applications—12-18+ second messaging
latency is prohibitive. And beyond latency, HRMP simply lacks primitives the ecosystem keeps asking
for: one-to-many dissemination (an oracle publishing the DOT price to every
interested chain—the motivating example of
[#606](https://github.com/paritytech/polkadot-sdk/issues/606)) has no
efficient HRMP shape at all (n channels, n copies of every message, n× relay
state), and channels themselves are scarce, deposit-gated resources.

### The Opportunity

By moving message coordination off-chain and using cryptographic commitments for
verification, we can:

1. Achieve messaging latencies comparable to parachain block times
2. Remove message data from relay chain state entirely
3. Make channels abundant and add pub-sub—one broadcast stream serves any
   number of subscribers at zero marginal cost
4. Build super chains

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

7. **Richer Primitives**: Make channels abundant (no relay-chain deposits,
   no per-channel relay state, multiple lanes per peer) and provide native pub-sub event
   streams ([#606](https://github.com/paritytech/polkadot-sdk/issues/606))—
   subscribe unilaterally, zero marginal cost per subscriber.

---

## Non-Goals

1. **Sender-stalling delivery guarantees**: "guaranteed delivery" here means
   a channel either advances past a message or stalls at it—we never stall
   the *sender's* block production waiting for receipt (see the Delivery
   Contract).
2. **Relay-chain-managed channels**: no channel registry, deposits, or
   flow-control enforcement on the relay chain. Channels, event streams and
   any future semantics are parachain-layer conventions over stream ids the
   relay chain never sees; it holds one fixed-size commitment window per
   sender and nothing else.
3. **Message confidentiality**: payloads are public, as all chain data is.
4. **Cross-source ordering**: ordering is guaranteed within a stream only;
   no ordering relation is defined between streams or between sources.

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

1. **Accumulate Messages**: Each chain maintains append-only message
   *streams* (MMRs), identified by structured stream ids the relay chain
   never sees. The standard layout: one channel stream per channel a chain
   sends on, one ack register per channel it receives on, plus optional
   broadcast streams—the ids' meaning is parachain convention.

2. **Emit Commitments**: Sending chains commit **one hash** per block—the
   `StreamsRoot`, root of a keyed commitment tree over all their stream
   roots. Receiving chains' candidates carry "requires" commitments—one
   `(source, StreamsRoot)` entry per source chain depended on—synthesized
   during validation from what their blocks consumed.

3. **Off-Chain Coordination**: Collators exchange messages directly, without
   relay chain involvement.

4. **Relay Chain Enforcement**: At inclusion time, the relay chain verifies
   that all "requires" are satisfied by corresponding "provides"—a hash
   membership check against a small per-sender window of recent
   `StreamsRoot`s. It never sees streams, positions, or proofs.

5. **Proofs, all parachain-side**: Everything below the top hash is
   verified by the receiver itself. Its runtime verifies payloads by
   recomputation (the block body carries payloads and trust-free
   positioning hints—no proof objects, never a `StreamsRoot`); the PVF
   binds the block's
   consumption to a committed `StreamsRoot` via POV-carried lifts (tree
   inclusion proofs plus MMR extension proofs bridging any unconsumed
   tail)—regenerable by anyone from public data, whether the block
   consumed only part of a backlog or was resubmitted after the window
   moved on.

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
│  Chain A Block      →     Off-chain    →   Chain B Block            │
│  (provides:               msg passing      (requires:               │
│   StreamsRoot(A))         ~block time       (A, StreamsRoot(A)))    │
│                                                                     │
│  Relay chain only verifies: requires(B) ∈ recent provides of A      │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│              Lifted Requires (e.g. Resubmission)                    │
├─────────────────────────────────────────────────────────────────────┤
│  Chain A Block N   ...time passes...   Chain A Block N+K            │
│  (provides: T_N)                       (provides: T_{N+K})          │
│                                                                     │
│  Chain B Block M (consumed A's messages up to T_N, arrives late)    │
│  POV includes: lift binding B's consumption to T_{N+K}              │
│  PVF verifies it and synthesizes B's requires before matching       │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Detailed Design

### Message Accumulators and Streams

Each parachain maintains a set of append-only message **streams**—one Merkle
Mountain Range (MMR) each—identified by a `StreamId`. Ids are
**relay-invisible, parachain-structured**: the relay chain never interprets
(or even sees) an id—only the `StreamsRoot` reaches it; ids exist in tree
paths, proofs and networking. The parachain protocol therefore defines them
outright:

```rust
/// Stream identifier, scoped to the sending chain: the full stream key is
/// (sender ParaId, StreamId), with the sender implicit wherever the
/// context is the sender's own candidate.
///
/// StreamId has a MANUAL, CANONICAL SCALE ENCODING (no derive), and that
/// encoding is used everywhere—wire, storage, and as the commitment-tree
/// key. One format, always exactly 8 bytes:
///
///   Channel   → 0x00 ++ recipient.to_be_bytes() ++ [domain] ++ num.to_be_bytes()
///   Ack       → 0x01 ++ recipient.to_be_bytes() ++ [domain] ++ num.to_be_bytes()
///   Broadcast → 0x02 ++ domain.to_be_bytes() ++ [subdomain] ++ num.to_be_bytes()
///   Private   → kind ++ body
///               kinds 0x03..=0x7F: reserved for future standard kinds
///               kinds 0x80..=0xFF: private use—body semantics defined by
///                                  the chain, never assigned by the
///                                  standard
///
/// The kind byte doubles as the variant discriminant; multi-byte fields
/// are big-endian, so the encoding, compared lexicographically, sorts
/// like the field tuple compared numerically (kind subtrees cluster in
/// the trie; sequential ids are neighbors). Note this deliberately
/// deviates from default SCALE integer encoding (little-endian)—which is
/// exactly why the impl is manual: SCALE-encoding a StreamId IS the
/// sanctioned key derivation, and there is no second format to confuse
/// it with.
///
/// This encoding is CONSENSUS-CRITICAL and frozen: every implementation
/// must reproduce it bit-identically (a receiver derives the same trie
/// path the sender used), locked by the conformance vector suite (see
/// Conformance—the id vectors are one part of it).
/// Decode enforces canonicality (decode∘encode = identity; fixed length,
/// no redundant encodings) and REJECTS reserved kinds 0x03..=0x7F: no
/// correct consensus path ever decodes a kind it does not know (you only
/// key your own streams and streams you chose to consume), so an unknown
/// kind is a loud boundary error, not a value. Tooling that walks foreign
/// trees parses raw key bytes with its own lenient presentation parser.
///
/// One uniform rule: a stream id's ParaId field names the chain the
/// stream is ADDRESSED TO—its `recipient`, the party that reads it. A
/// channel's messages are for the recipient; a register is for the chain
/// that sends on the channel. Broadcast streams have no addressee, hence no
/// field.
enum StreamId {
    Channel   { recipient: ParaId, domain: u8, num: u16 },
    Ack       { recipient: ParaId, domain: u8, num: u16 },
    Broadcast { domain: u16, subdomain: u8, num: u32 },
    /// `kind >= 0x80`; 7 body bytes, chain-defined.
    Private   { kind: u8, body: [u8; 7] },
}
```

**Domain / subdomain are reserved fields—set to 0 by default.** The
transport treats a kind's whole body as one opaque discriminator; the named
fields are *allocation convention*, not protocol semantics (port-ranges
logic: TCP doesn't know well-known from ephemeral, allocation policy does).
Their intent: a chain can delegate address space to applications—hand a
pallet or contract subsystem a `domain`, within which it manages `num`
autonomously; broadcast gets a second level (`subdomain`) for app-internal
classes. Until a chain does this, everything runs at domain 0, and APIs may
hide the fields entirely.

Two properties fall out of the structured definition:

- **Deterministic addressing**: `Channel { recipient, 0, 0 }` is computable
  by both sides—no id negotiation, no directory. B knows which of A's
  streams to watch and which tree key to verify before any contact.
- **Trivial extensibility, no versioning needed**: unlike message payloads
  (which ordered consumption *must* parse), a stream id never has to be
  understood—a chain simply doesn't consume streams whose kind it doesn't
  know. Unknown kinds are inert, not errors, so new kinds are pure tag
  assignments: standard ones from the reserved range, chain-specific
  semantics in the private range, collision-free by construction.

**The standard kinds** (what the tags mean):

- **Channel stream** `Channel { recipient, domain, num }`: ordered,
  flow-controlled, guaranteed delivery, **unidirectional**—the
  HRMP-replacement workhorse (see
  [Channels and Flow Control](#channels-and-flow-control)). The
  discriminator allows any number of independent channels to the same
  recipient.
- **Ack stream** `Ack { recipient, domain, num }`: the lossy confirmation
  register the *receiver* of a channel maintains, addressed to the
  channel's sender—same discriminator as the channel, kind flipped,
  recipient swapped to the other end: acceptance, version range,
  watermark and close in one latest-wins register, read out-of-band (see
  [Flow Control](#flow-control)). Naming note: despite the id's name, these
  carry *confirmations* between parachains—unrelated to Low-Latency v2's
  collator *acknowledgements* (see the Terminology note in Channels and
  Flow Control).
- **Broadcast stream** `Broadcast { domain, subdomain, num }`: sender-wide
  event streams, no addressee, lossy latest-wins consumption by any
  interested chain (see [Event Streams](#event-streams)).
- Anything else a chain wants—an out-of-band priority lane, or semantics
  not yet invented—rides a private-use kind, deployable without anyone's
  coordination, by construction.

Channels are unidirectional, and one channel spans **two chains' stream
sets**: the sender's data stream plus the *receiver's* register (domain/
subdomain zeros elided throughout):

```
  Channel A→B  =  A's Channel{B, 0}  (data + signals)
               +  B's Ack{A, 0}      (register)

  Channel B→A  =  B's Channel{A, 0}
               +  A's Ack{B, 0}

Chain A's full stream set (channels with B and C, one event feed):
  ├── Channel{B, 0}   → [Msg1, Msg2, Msg3, ...]   A's half of channel A→B
  ├── Channel{C, 0}   → [Msg1, Msg2, ...]         A's half of channel A→C
  ├── Ack{B, 0}       → [.., Register]            A's half of channel B→A
  └── Broadcast{0}    → [Evt1, Evt2, ...]         events, any subscriber
```

Note nothing of A's completes A's own channels—`Ack{B, 0}` above exists
only because B opened a channel *toward* A; a chain that only ever sends
to B has no Ack streams at all.

**Why per-destination streams (for the channel convention)?**
- Receiver only cares about their own stream—high volume to other chains
  does not affect them
- Proof size: O(log m) where m = messages to that receiver
- Lifts only grow with messages to that specific receiver

Multiple channels to the same recipient are supported (distinct
discriminators) and inherit all of the above. One caveat belongs with that
choice: the no-selective-censorship property of ordered delivery holds
*per channel*—within one channel, the receiver's collators can only stall
everything or nothing; across several channels they can stall one lane
while advancing another. Splitting traffic over multiple channels is
therefore also choosing the granularity at which censorship is
all-or-nothing (see [Censorship Profile](#censorship-profile) for the same
consideration in stronger form for event streams).

#### The Stream Commitment Tree

A sender's block commits to *all* its streams with **one hash**: the
`StreamsRoot`, root of a keyed commitment tree whose entries are
`StreamId → stream MMR root`. This is the only thing the relay chain ever
stores or compares—everything below it is proven parachain-side.

```rust
/// Root of a sender's stream commitment tree: a binary compact (Patricia)
/// trie keyed by the canonical SCALE encoding of StreamId (8 bytes),
/// leaves = the streams' MMR roots, leaf preimage
/// `TREE_LEAF_TAG ‖ StreamId ‖ MmrRoot`—the key is IN the leaf hash, see
/// TreeInclusionProof for why that is load-bearing.
/// Distinct newtype from MmrRoot—the two kinds of root flow through
/// different checks and must not be confusable.
struct StreamsRoot(Hash);

/// Bit width of the canonical 8-byte StreamId encoding; a path cannot
/// have more branches than the key has bits.
const KEY_BITS: u8 = 64;

/// One branch on the path. `split_bit` is the branch's bit position,
/// counted as offset from the key's first (most significant) bit.
struct TreeStep {
    split_bit: u8,
    sibling: Hash,
}

/// Proof that one stream's MMR root is the entry at its StreamId under a
/// given StreamsRoot: one step per branch on the id's path (~log₂(S) of
/// them). Each step must carry its split-bit position alongside the
/// sibling hash—path compression makes the branch positions a function
/// of the *other* keys in the tree, which the verifier does not know,
/// and without the position neither the step's direction (our key's bit
/// at the split decides left/right) nor the number of compressed levels
/// is determinable. The verifier reconstructs the path from
/// (StreamId, MmrRoot) upward and compares the result against the
/// StreamsRoot—id and root are both bound by the path, neither is taken
/// on faith.
///
/// The node encoding is CONSENSUS-CRITICAL and protocol-fixed: every
/// implementation must reproduce byte-identical StreamsRoots and every
/// foreign node must verify these proofs. The concrete byte format is
/// specified with the primitives (companion to the #12346 family),
/// locked by the conformance vector suite (see Conformance), under
/// four constraints mandated here:
///
/// 1. Domain-tagged node hashing with no ambiguous parses (same
///    rationale as Leaf Hashing).
/// 2. Injective node encoding.
/// 3. The split bit committed inside the inner-node preimage
///    (`TREE_INNER_TAG ‖ split_bit ‖ left ‖ right`)—otherwise the
///    claimed positions are free parameters and one sibling list
///    verifies under many of them.
/// 4. The full key committed inside the leaf preimage
///    (`TREE_LEAF_TAG ‖ StreamId ‖ MmrRoot`). The path binds only the
///    key's bits AT the branch positions; the compressed bits in
///    between are, by construction, bits no *present* key differs
///    on—so a bare-value leaf would let any key agreeing with a
///    present key on just the branch bits verify against that key's
///    entry (e.g. Channel{B,0,1} aliasing Channel{B,0,0} while the
///    sender runs no stream branching on a num bit: same messages
///    consumable under two stream ids—replay). The leaf field binds
///    the compressed bits; branch bits and leaf together cover all
///    KEY_BITS. It is also what makes non-inclusion proofs
///    well-defined: "the lookup lands on a leaf holding a DIFFERENT
///    key" requires leaves to provably hold their key.
///
/// Together 3 and 4 pin every hash on the path, so each (key, root)
/// pair has exactly one verifying proof; the step-order rule below is
/// early rejection, not what uniqueness rests on.
///
/// The audit rule behind 3 and 4, for reviewing any change to this
/// structure: every one of the KEY_BITS key bits must be committed
/// exactly once—as some step's split bit (branch positions) or inside
/// the leaf preimage (everything compressed away). A bit committed
/// nowhere is an aliasing bug; twice, redundancy.
struct TreeInclusionProof {
    /// Leaf to root: split-bit offsets strictly decreasing (deeper
    /// branches split on later bits), each < KEY_BITS. Structurally
    /// impossible in a well-formed trie to be otherwise—the verifier
    /// rejects anything else as early garbage (uniqueness itself rests on
    /// constraints 3 and 4 above).
    steps: BoundedVec<TreeStep, KEY_BITS>,
}
```

Fixed by the protocol, for bit-identical implementations (the byte values
themselves live in the primitives' encoding spec, #12676):

- **One hash function for every tree and MMR node: BLAKE2b-256**,
  protocol-fixed—never the chain's own `Hashing` type, so chains with
  different hashing still verify each other's commitments. Streams are
  parachain communication, verified only inside Polkadot, where BLAKE2b is
  the native choice (a host function, and what the rest of Polkadot uses);
  a format meant for verification in other ecosystems (e.g. on Ethereum)
  would pick its own.
- **Six pairwise-distinct tag bytes**: MMR leaf, inner, peak and root
  (`LEAF_TAG`, `INNER_TAG`, `PEAK_TAG`, `MMR_ROOT_TAG`), tree leaf and
  inner (`TREE_LEAF_TAG`, `TREE_INNER_TAG`).
- **Bit order**: bit offset 0 is the most significant bit of the key's
  first byte (MSB-first within each byte, bytes in encoding order).
- **No deletion**: tree entries are only inserted or updated (stream
  positions are permanent), so implementations need nothing else.
- **The empty bag** (no peaks) is the zero hash, so the empty stream's
  root is `H(MMR_ROOT_TAG ‖ 0 ‖ 0^32)`; a single peak bags to itself.

What compression is, on toy 4-bit keys `A=0000, B=0001, C=0100`—the
uncompressed tree spends one level per key bit; the compressed one
keeps a node only where present keys diverge, and its edges carry the
bit strings in between:

```
Uncompressed — one level per key bit:

                [·]
bit 0         0/    \1
             [·]     ∅
bit 1      0/    \1
          [·]    [·]
bit 2      0|      0|
          [·]     [·]
bit 3    0/  \1    0|
          A    B    C

Compressed — nodes only at forks; an edge carries exactly one bit, the
decision at its parent's @-position. Nothing else of the key exists in
the structure:

             [r] @1
           0/     \1
           /       \
        [n] @3      C
       0/   \1
       A     B

What the structure knows about A: bit 1 = 0, bit 3 = 0. Nothing more.
Bits 0 and 2 appear nowhere—A = 0000 is NOT reconstructible from the
tree; the missing bits exist solely inside leaf(A)'s preimage. That
absence is why constraint 4 exists.

Proof for A, uncompressed: [leaf(B), ∅, subtree-C, ∅]
  — always exactly one sibling per key bit (∅ = empty-subtree hash);
    positions implicit: leaf to root, step i sits at bit KEY_BITS−1−i
Proof for A, compressed:   [(3,leaf(B)), (1,leaf(C))]
  — only the real forks; the grid is gone, so each step must carry
    its position

Hashes—what is committed:
  leaf(X) = H(TREE_LEAF_TAG  ‖ X ‖ MmrRoot_X)         leaf commits its FULL key
  n       = H(TREE_INNER_TAG ‖ 3 ‖ leaf(A) ‖ leaf(B))     3 = [n]'s fork bit
  r       = H(TREE_INNER_TAG ‖ 1 ‖ n ‖ leaf(C))           1 = [r]'s fork bit

Legend:
  [x] @k   inner node forking on key bit k; k is IN the node's hash
           preimage
  0/ \1    edge = the single decision bit at the parent's @k
           (0 = left, 1 = right)—the only key bits the structure
           holds; all skipped bits are in NO hash except the leaf's
           committed key (constraint 4)
  ∅        empty side (uncompressed tree only)
```

The real thing, at full width: chain
A runs a channel to B (ParaId 2001 = `0x7D1`) with B's ack register for
the reverse channel, a channel to C (`0x7D2`), and one broadcast
stream. Four keys, and the tree they span:

```
Keys: the four StreamIds in their canonical 8-byte encodings (see
Message Accumulators and Streams), shown as hex bytes. Bit offsets
count from the first (most significant) bit:
  Channel{B}    00 000007D1 00 0000
  Channel{C}    00 000007D2 00 0000
  Ack{B}        01 000007D1 00 0000
  Broadcast{0}  02 0000 00 00000000

              [r] @6              [r]'s hash is the StreamsRoot
            0/     \1
        [n1] @7     Broadcast{0}
       0/     \1
   [n2] @38    Ack{B}
    0/     \1
Channel{B}  Channel{C}

Same shape and notation as the toy. What the toy could not show—the
scale of what is absent at full width:
  @6, @7    forks inside the kind byte (0x00 / 0x01 / 0x02)
  @38       the next fork is 30 bits further down, inside the
            recipient bytes (0x7D1 vs 0x7D2)
  absent    bits 0–5 above [r], bits 8–37 between @7 and @38, and
            every leaf's tail (bits 39–63 for the channels)—the
            structure holds 3 of 64 bits per path; the missing 61
            live only in the leaf preimages (constraint 4)

  leaf  = H(TREE_LEAF_TAG  ‖ StreamId ‖ MmrRoot)
  n2    = H(TREE_INNER_TAG ‖ 38 ‖ leaf(Channel{B}) ‖ leaf(Channel{C}))
  n1    = H(TREE_INNER_TAG ‖  7 ‖ n2 ‖ leaf(Ack{B}))
  r     = H(TREE_INNER_TAG ‖  6 ‖ n1 ‖ leaf(Broadcast{0}))  = StreamsRoot
```

One branch per *distinguishing* bit among the keys present—proof length
tracks the number of streams, not the key width.

Proof and verification, concretely:

```
Proof for Channel{B} (leaf to root, split bits strictly decreasing):
  steps = [ (38, leaf(Channel{C})), (7, leaf(Ack{B})), (6, leaf(Broadcast{0})) ]

Verifier, holding key K = Channel{B} and its computed MmrRoot:
  h = H(TREE_LEAF_TAG ‖ K ‖ MmrRoot)
  (38, s): K[38] = 0 → we are LEFT  → h = H(TREE_INNER_TAG ‖ 38 ‖ h ‖ s)
  ( 7, s): K[7]  = 0 → we are LEFT  → h = H(TREE_INNER_TAG ‖  7 ‖ h ‖ s)
  ( 6, s): K[6]  = 0 → we are LEFT  → h = H(TREE_INNER_TAG ‖  6 ‖ h ‖ s)
  h == StreamsRoot?

Same for Ack{B} (2 steps—its sibling at bit 7 is the whole n2 subtree):
  steps = [ (7, n2), (6, leaf(Broadcast{0})) ]
  h = H(TREE_LEAF_TAG ‖ Ack{B} ‖ MmrRoot)
  (7, s): key[7] = 1 → we are RIGHT → h = H(TREE_INNER_TAG ‖ 7 ‖ s ‖ h)
  (6, s): key[6] = 0 → we are LEFT  → h = H(TREE_INNER_TAG ‖ 6 ‖ h ‖ s)
```

The split-bit offsets are what lets the verifier pick left/right from
its *own* key's bits (direction is never in the proof) and jump the
compressed levels in between; the sibling hashes alone would leave both
undetermined. The whole tree is a pure function of the entry set—the
canonical construction, which every implementation must reproduce
bit-identically:

```
/// entries: non-empty, sorted by key, keys distinct.
fn tree_hash(entries: &[(StreamId, MmrRoot)]) -> Hash {
    match entries {
        [(key, root)] => H(TREE_LEAF_TAG ‖ key ‖ root),
        _ => {
            let b = smallest bit offset (MSB-first) at which entries' keys differ;
            let (zeros, ones) = entries.split_at(first key with bit b set);
            H(TREE_INNER_TAG ‖ b ‖ tree_hash(zeros) ‖ tree_hash(ones))
        }
    }
}
```

A prover holding all entries extracts the proof for key K by recording,
at each recursion step on K's side, `(b, tree_hash(other side))`—then
reversing into leaf-to-root order. A sender should not rebuild from
scratch per block, though: it caches the node hashes and updates the k
touched paths (see below).

Properties the tree must have, and why a keyed binary trie:

- **Inclusion proofs O(log S)** (S = the sender's stream count): a receiver
  proves "stream X has root R under StreamsRoot T" with ~log₂(S) hashes
  (~300 B at S=100; proving several streams of one sender could share
  sibling paths, though today each lift carries its own tree proof).
- **Stable under insertion**: adding a stream perturbs only the insert
  path—it does not reposition other leaves (a *positional* Merkle tree over
  a sorted list would; that instability was a correct objection against the
  hierarchical scheme in the 0.3 analysis, and it is an artifact of the
  positional structure, not of hierarchy).
- **Cheap maintenance—given a node cache**: the sender updates the k
  touched leaves per block along their paths, O(k·log S) node reads and
  writes. This needs the tree's nodes cached in storage (children's hashes
  kept in the parent; insertion only—streams are never removed).
  Rebuilding the root from scratch is correct but must bag *every* stream's
  root from its frontier, putting all S frontiers into each sending
  block's storage proof: at 1000 streams, hundreds of KB per block,
  against a few KB with the cache. The cache is fully redundant—the
  frontiers determine the tree—so its layout is chain-internal, not part
  of the protocol; only the root is. A wrong cache yields a wrong
  `StreamsRoot`, which receivers' lifts reject, so it can't fail silently.
- **Domain-separated hashing**: tree leaf and inner nodes carry distinct
  hash tags, same discipline and same attack rationale as the message MMR
  (see [Leaf Hashing](#leaf-hashing-and-domain-separation))—a leaf must
  never be parseable as an inner node or vice versa.

**History: why the aggregated commitment, when 0.3 deliberately removed
it?**

Version ≤0.2 had a top-level commitment; the
[PR #10449](https://github.com/paritytech/polkadot-sdk/pull/10449) analysis
flattened it into per-destination entries—correctly, *for its regime*:
10–100 destination entries per sender, requires keyed to the receiving
para, and only transient costs (receipt size, PoV bytes) in play. The
analysis stated its own reversal condition: the aggregated commitment
"wins back" once entry counts grow.

Streams crossed that line, three times over:

- **Entry counts multiply**: channel + ack + broadcast streams, several per
  peer, unbounded in principle.
- **A new cost class appears** that the 0.3 analysis never had to price: a
  flat scheme keys *persistent relay state* by stream, dragging in
  permanence rules for dormant entries, a per-sender stream budget, and
  offboarding hygiene.
- **Flat entries ossify semantics**: the relay-visible schema is party to
  what an entry means—introducing ack registers and broadcast streams
  would have changed the relay chain interface again, as would every
  future kind. Under the root, the relay chain holds one hash and is done
  evolving: lossy, multi-lane, or not-yet-invented semantics are parachain
  convention, deployable with zero relay chain changes (this is what makes
  the private-use kinds of `StreamId` possible at all).

The tree removes the entire per-stream relay footprint at once: provides is
32 B regardless of stream count, requires is bounded by the number of
*parachains*, and how many streams a chain runs is nobody's business but
its own—**outbound streams are effectively unbounded and free**.
What we pay, honestly: receivers carry ~300–500 B of tree proof per
consumed stream per consuming block, lifts regain an inclusion
component, consumers poll on the p2p side (one opaque root: a receiver
cannot tell whether its stream moved, so each new root costs one
up-to-dateness request per consumed sender—cheap, the empty response is
a verifiable "nothing new" of well under a kilobyte (tree proof plus
frontier), see
[Fetch Protocol](#fetch-protocol)), and proof-free lag tolerance
denominates in sender blocks rather
than per-stream changes (see
[Relay Chain Matching](#relay-chain-matching) for why that tolerance is
cheap to buy).

### Candidate Commitments (Verified by Relay Chain)

The commitments are minimal—one hash out (the sender's `StreamsRoot`), a
bounded per-source set in (the receiver's dependencies). They are
transported as UMP signals
([#12347](https://github.com/paritytech/polkadot-sdk/issues/12347)) rather than
new fields in `CandidateCommitments`, so no candidate receipt format change is
needed:

```rust
/// Root of a single message stream's MMR: the leaf count bound over the
/// bagged peaks—never the bare bag:
///
///   MmrRoot = H(MMR_ROOT_TAG ‖ leaf_count (8 bytes, LE) ‖ bag(peaks))
///
/// (leaf_count = 0 over the zero bag is the root of the empty stream.)
/// Why the count is in the hash, and the rules that keep it there: see
/// "Stream roots bind their leaf count" below.
///
/// Newtype over `Hash`: roots flow through every layer (tree leaves, wire
/// responses, extension proofs) alongside block hashes, leaf hashes, peaks
/// and StreamsRoots—confusing them must not typecheck. Leaf and inner-node
/// hashes stay bare `Hash`: they are internal to the accumulator code and
/// already domain-separated cryptographically by the hash tags.
/// Normative bytes: the primitives' encoding spec (#12676), locked by the
/// conformance vector suite.
struct MmrRoot(Hash);

/// Canonical, bounded set of (ParaId, StreamsRoot) entries. Manual `Decode`
/// REJECTS input whose ParaIds aren't strictly increasing (no silent
/// normalization): the bytes come from untrusted parachain wasm, so
/// malformed sets (duplicate sources with conflicting roots) must fail
/// loudly at the boundary; canonical bytes make decode∘encode the identity
/// (re-encode-and-hash agrees with the original candidate bytes); and other
/// implementations only need "sorted, unique, bounded" instead of replicating
/// a lenient parser's quirks. Construction is sealed: `try_from_iter` (sorts,
/// rejects duplicates) and `Decode` are the only ways in; no mutable access.
///
/// One entry per SOURCE, not per stream: the StreamsRoot covers all of that
/// source's streams at once, so the set is naturally bounded by the number
/// of parachains a receiver consumes from. `MaxCommitmentEntries` = 256:
/// today's registered-parachain count (~200) rounded up—to be bumped in
/// the unlikely event the network outgrows it (see
/// Practical Limits).
struct RequiresSet(BoundedVec<(ParaId, StreamsRoot), MaxCommitmentEntries>);

enum UMPSignal {
    // ... existing signals ...
    /// Sender side: the root of our stream commitment tree after the
    /// candidate's last block that touched a stream. One hash, constant
    /// size, at most one per candidate. Blocks don't emit the signal: each
    /// block that touched a stream deposits its new StreamsRoot as a header
    /// digest, and the validate_block wrapper emits the last such digest of
    /// the bundle as the candidate's Provides (once, like Requires). Absent
    /// if no block of the bundle touched a stream (an unchanged root would
    /// only push a duplicate into the relay window).
    Provides(StreamsRoot),
    /// Receiver side: (source, expected StreamsRoot) for every source chain
    /// whose streams this candidate depends on.
    ///
    /// NEVER emitted by block execution: blocks produce a consumption
    /// record (they never even see a StreamsRoot), and this signal is
    /// synthesized from it by the validate_block wrapper (PVF) via
    /// POV-carried lifts—see Requires Lifting. Relay semantics: the named
    /// root must be a recently committed StreamsRoot of that source
    /// (window membership). Everything below the hash is proven
    /// parachain-side, in the wrapper; what the dependency *means* per
    /// stream is convention—for channels, that consumed messages are a
    /// *prefix* of the stream root (NOT consumed exactly up to it); for
    /// event streams, that the read leaf is part of it.
    /// Consumption depth is therefore not derivable from requires; don't
    /// build acknowledgement or pruning logic on it—the Register (see
    /// Channels) carries the consumption watermark
    /// instead.
    Requires(RequiresSet),
}
```

**Stream roots bind their leaf count.** Bagged peaks alone do not
commit to the tree's size. Peaks are opaque hashes, so an inner node can
pass as a taller peak, and the same bag is then a valid root for many
leaf counts. A prover can use that to verify genuine leaves under
fabricated positions (worked example:
[Position Forgery](#threat-position-forgery-peak-height-confusion)).
With the count bound, the root commits to one exact sequence of leaves
at fixed positions. Every carried frontier and every count in a proof is
therefore trust-free: a lie yields a root that matches no commitment.
This is a known MMR pitfall with a standard remedy. Grin bags its peaks
together with the MMR size, and ChainSecurity's
[Herodotus case study](https://www.chainsecurity.com/blog/merkle-mountain-range-mmr-the-case-of-herodotus)
documents the inner-node-as-peak forgery and the same two-part fix: bind
the size into the root, and check the peak count against the size (here
`MmrFrontier::from_parts`, see
[Parachain Runtime State](#parachain-runtime-state-internal)).

Two rules keep the binding from being bypassed:

- **Sealing rule.** Bare bagged peaks exist only transiently inside the
  hashing code: they are never stored, transported, or compared. The
  only constructor of an `MmrRoot` from state is `MmrFrontier::root()`,
  which supplies the frontier's own count; all other code only compares
  roots. No comparison site can forget the count, because no code can
  build a root without a frontier.
- **Audit rule** for any change to proofs or verification: every count
  that node positions are derived from must be bound in a root the
  verification path compares. Proofs never carry positions; they may
  carry counts only where the root derived from them is checked against
  a committed one.

The relay chain matches each requires entry against the recent
`StreamsRoot`s of the named source. A parachain block will only be made
available/enacted when all its requires are provided. **Who** requires a
source is unrestricted: a requires entry only constrains the *requiring*
candidate's enactment (atomic groups need mutual requires, i.e. both
parties opting in), so third-party dependencies are self-imposed and
harmless—and unilateral broadcast subscription falls out for free.

Relay chain storage is one small ring per sender—the last W `StreamsRoot`s
(see [Relay Chain Matching](#relay-chain-matching)).

### Parachain Runtime State (Internal)

Each parachain runtime maintains internal state for message tracking.

```rust
/// Index of a message (= leaf) in a stream's MMR, starting at 0.
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
/// equal-height peaks); `root()` binds the count over the bagged peaks
/// (see MmrRoot)—the only way to produce a root from state.
///
/// `from_parts(peaks, leaf_count)` is the only entry for peak sets from
/// untrusted input. Decoding a frontier from an inherent item or a fetch
/// response runs the same checks.
/// It enforces two invariants: exactly one peak per set bit of
/// leaf_count (the count fixes the shape of the tree; a peak set of any
/// other length describes no tree of that count), and
/// leaf_count ≤ MAX_MMR_LEAF_COUNT = 2^48 (node-count arithmetic
/// overflows u64 beyond 2^63). The cap also bounds every MMR proof at 48
/// hashes (see Proof Size Bounds).
///
/// Defined in `cumulus-primitives-spec-messaging` (#12676).
struct MmrFrontier {
    /// Also the position of the next message to append.
    leaf_count: u64,
    /// One peak per set bit of leaf_count; ≤ 48 under MAX_MMR_LEAF_COUNT.
    peaks: BoundedVec<Hash, ConstU32<48>>,
}

// ---- Sender-side (in parachain runtime, see #12350) ----

/// Per-stream MMR *frontiers*: only the O(log n) peaks are stored, not
/// the leaves. The root is computed on demand (`root()`).
OutboundFrontier: StorageMap<StreamId, MmrFrontier>,

/// Messages sent in *this block only*, per stream, so the collator can
/// extract them for off-chain delivery. Payloads are never long-lived
/// consensus state: past blocks' messages live only in node-side archives.
///
/// Message i has position `OutboundFrontier[stream].leaf_count + i`: gaps
/// are unrepresentable, contiguity holds by construction (a
/// `(StreamId, position) → payload` map would need that invariant
/// maintained in code instead). A bare vec (no wrapper struct with a base
/// field) so appends can use the host-side storage `append`/`try_append`
/// (as System::Events does): O(1) per message, instead of decode +
/// re-encode of the whole vec per send, which would be quadratic over a
/// block.
OutboundMessages: StorageMap<StreamId, BoundedVec<BoundedVec<u8, MaxMsgLen>, MaxMessagesPerBlock>>,

// ---- Receiver-side (channel state: see Channels) ----

/// This block's consumption record (see Requires Lifting), written while
/// the messaging inherent is processed: one entry per touched stream, at
/// most MaxTouchedStreams. Per block only—cleared at the next block's
/// initialization, like OutboundMessages. A flat, host-append-only vec
/// (O(1) appends); grouped and sorted at read time by
/// `consumption_record()`.
ConsumptionOutbox: StorageValue<BoundedVec<(ParaId, StreamId, Interval), MaxTouchedStreams>>,

/// The streams this chain consumed or read so far in the current bundle.
/// Written by the messaging inherent. Cleared in `on_initialize` of the
/// bundle's first block: the block whose `BlockBundleInfo` header digest
/// has `index == 0`, or has none (a single-block PoV).
///
/// Used at admission for two things:
/// - Lift reservation. A stream's first touch in the bundle reserves its
///   lift. A later read of it that skips leaves reserves an advance
///   proof (see The Messaging Inherent: PoV weight reservation).
/// - Source cap. A new source chain is refused once the set holds
///   MaxCommitmentEntries distinct ParaIds (see Requires Lifting).
///
/// Only liveness depends on it. If it were wrong, the author's own
/// candidate could be mis-sized or invalid. Soundness does not depend on
/// it, because the PVF checks every block's own consumption record.
BundleStreams: StorageValue<BoundedBTreeSet<(ParaId, StreamId), MaxBundleStreams>>,

/// A broadcast stream: its sender plus the discriminator. Broadcast has
/// no recipient, so unlike the channel ids this is the stream's full,
/// global identity—it means the same stream wherever it is used.
struct BroadcastId { sender: ParaId, domain: u16, subdomain: u8, num: u32 }

impl BroadcastId {
    /// The sender's Broadcast{domain, subdomain, num}, with its owner
    /// (see the helper convention at OutChannelId).
    fn stream(&self) -> (ParaId, StreamId);
}

/// Receiver-side: every broadcast stream this chain has subscribed to.
/// Entries persist across unsubscribe (see "Consumption positions are
/// permanent" below), so resubscribing resumes above everything applied
/// before.
Subscriptions: StorageMap<BroadcastId, Subscription>,

struct Subscription {
    /// Currently consumed: the stream is listed by `consumed_streams()`
    /// and inherent items for it are accepted. Unsubscribe clears it,
    /// resubscribe sets it.
    active: bool,
    /// Position of the last event applied; None = nothing applied yet
    /// (see The Messaging Inherent: the highwater rule).
    highwater: Option<MessagePosition>,
}
```

**Receiver state by stream kind.** Receiver-side state is keyed by what
the consumed stream's id leaves once `recipient` is dropped—it is always
this chain, since a chain consumes what is addressed to it. Each map
therefore admits only its own stream kind:

| Consumed stream | Stored in |
|---|---|
| sender's `Channel{us, d, n}` | `InChannels[InChannelId{sender, d, n}]` (frontier) |
| recipient's `Ack{us, d, n}` | `OutChannels[OutChannelId{recipient, d, n}]` (applied register + ack highwater) |
| sender's `Broadcast{d, s, n}` | `Subscriptions[BroadcastId{sender, d, s, n}]` (highwater) |

The channel maps are defined with the channel protocol (see
[Channels](#channels)).

**Consumption positions are permanent.** A chain's position in a stream
it consumes—the channel frontier, the ack highwater, the broadcast
highwater—is never discarded while the sender exists, just as the
stream itself is never discarded. Closing or unsubscribing are phases
of the entry, not deletions: a position that was dropped
would let anything below it be applied again on resume, and would
leave a channel frontier nothing can be verified against. The one
exception is the sender's offboarding, when the stream itself is gone
(see Threat: ParaId Reuse).

The sender additionally maintains its **stream commitment tree** (see
[The Stream Commitment Tree](#the-stream-commitment-tree)), yielding each
sending block's `StreamsRoot`. With S the sender's number of streams and
k the number this block sent on, implementations should cache the tree's
nodes so a block touches only those k paths, O(k·log S), instead of
reading all S frontiers into the PoV; the cache's layout is
implementation-defined (the primitives currently rebuild, with this as a
TODO—#12676).

Note there is no stored *stream* root anywhere—on either side. The stream
roots that feed the commitment tree and the receiver's checks are computed
on demand from the respective frontiers. Storing one would just cache a
computable value and add an invariant to maintain.

**Sender lifecycle**: the stored frontier is bumped in the same step that
clears `OutboundMessages`—at the *next* block's initialization, appending the
previous block's leaves. During all of block N the stored frontier therefore
reflects the state as of block N−1, so `position = leaf_count + i` holds
unchanged throughout the block, including in its finalization. The stream
roots of block N (stored frontier plus this block's leaves, bagged in
memory) are computed transiently at block end and folded into the
commitment tree, whose new root block N deposits as a header digest—one
tree-path write per touched stream, no mid-block ordering hazard, and
clear + bump are one atomic step. A block that touched no stream folds
nothing and deposits no digest. The messages of block N remain available
at block N—collators extract them via the runtime API below, never by
reading storage directly.

**No sends before initialization.** FRAME runs `on_runtime_upgrade` before
block initialization, so the previous block's sends aren't folded yet; a
leaf sent there would be folded as the previous block's and never reach an
archive. Sends (and the pallet's own signals) are therefore refused until
the block is initialized (`frame_system`'s execution phase is unset until
then: `SendError::NotInitialized`). Upgrade code that needs to send, such
as a migration closing channels or the post-upgrade Hello check, sets a
transient flag instead. The messaging pallet's `on_initialize` then does
the work right after the fold. The Hello check runs only if
`SupportedVersions` changed, as a multi-block pass over `OutChannels`
with a stored cursor and a per-block bound, so the upgrade block's
weight doesn't grow with the number of channels.

```
Block N init:      append block N−1's OutboundMessages to frontiers,
                   clear OutboundMessages            (one atomic step)
Block N execution: sends host-append to OutboundMessages;
                   positions = frontier.leaf_count + i (frontier untouched)
Block N end:       stream roots = bag(frontier + this block's leaves),
                   fold touched roots into tree      (transient, in memory)
                   → deposit the new StreamsRoot as a header digest
Block N+1 init:    ...same cycle; now the frontier catches up to N.
```

#### Runtime API (the Node–Runtime Boundary)

Collators never read messaging storage directly; everything the node side
needs crosses this API. It is also the completeness check for the design:
if authoring or serving needs a fact not listed here, something is missing.

```rust
type Payload = Vec<u8>;

/// Called at a block of this chain: that block's sends, per stream—what a
/// collator extracts for delivery and appends to its archive.
fn outbound_messages() -> Vec<(StreamId, Vec<Payload>)>;

/// Everything the inherent provider should fetch, and from which
/// position on.
///
/// Deliberately unpaginated: this is a node-local call—no PoV, no weight,
/// no consensus limit of any kind. The only bound is wasm memory and call
/// time, i.e. tens of MB at a *million* entries; realistic consumed-stream
/// counts are orders of magnitude below. Pagination would be API surface
/// for a regime that can't occur.
fn consumed_streams() -> ConsumedStreams;

/// `MaxTouchedStreams`: how many streams one inherent may touch. When
/// more streams have data, the node picks which ones go in (node policy,
/// e.g. rotation). The runtime enforces the cap either way.
fn max_touched_streams() -> u32;

/// Field for field the shape of MessagingInherentData, keyed by the same
/// typed ids: the node fetches for each entry and files the result under
/// the same key in the same field—no id reconstruction, and one map per
/// kind keeps kind/discipline mismatches unrepresentable. Every rule for
/// WHAT to fetch lives here, in the runtime; the node only decides how
/// (root choice per source, see Verification; which streams go in when
/// more than `max_touched_streams()` have data).
///
/// All streams of one source chain must lift to the same StreamsRoot, so
/// the node picks one root per source; it groups entries by the chain
/// each id names (`sender`, or `recipient` for registers).
struct ConsumedStreams {
    /// Inbound channels to consume, per the watching rule (see Channels:
    /// State), with the stored frontier: its leaf count is the cursor,
    /// and fetch responses must match it (see Verification). The omission
    /// is how the own collators learn to stop fetching.
    channels: BTreeMap<InChannelId, MmrFrontier>,
    /// Outbound channels whose recipient's register should be read: those
    /// we want open (see Channels: State).
    ///
    /// Cursor = the applied register's position + 1, or 0 before the
    /// first read. The collator fetches the head and includes it only if
    /// its position ≥ cursor: an already-applied head would fail the
    /// highwater rule and invalidate the block.
    registers: BTreeMap<OutChannelId, MessagePosition>,
    /// Active subscriptions, cursor = highwater + 1, or 0 before the
    /// first read. Typically only the head matters; as for registers,
    /// nothing below the cursor may be included.
    events: BTreeMap<BroadcastId, MessagePosition>,
}
// Private-use kinds cannot appear: the standard pallet cannot consume a
// stream whose semantics it doesn't know (see The Messaging Inherent).

/// The block's consumption record (defined in Requires Lifting): touched
/// streams with their intervals. Called by the node for
/// acknowledgement checks (a block's dependencies), lift assembly and
/// diagnostics—and by the validate_block wrapper as a direct in-wasm
/// call, to synthesize the candidate's Requires entries.
fn consumption_record() -> ConsumptionRecord;
```

Inputs flow back into the runtime exclusively through the messaging
inherent—defined next in
[The Messaging Inherent](#the-messaging-inherent). The API and the
inherent are the two halves of one trust boundary, not a trusted channel.

**Recovery from downtime falls out of this split.** All authoring-relevant
state is consensus state behind the API—a returning collator syncs its
chain and resumes: `consumed_streams()` says where fetching continues;
already-consumed messages need no refetch (state reflects them). The one
node-local structure, the sender-side archive, rebuilds during sync if the
node executes the missed blocks (each block's `outbound_messages()`
regenerates in passing); a node syncing without execution instead fetches
the missing range from its own chain's other nodes—the ordinary fetch
protocol, pointed at one's own streams.

#### The Messaging Inherent

All received data enters the runtime through one inherent
([#12531](https://github.com/paritytech/polkadot-sdk/issues/12531)),
carrying payloads and trust-free positioning hints—no roots, no proof
objects, no relay state of any kind (see
[Verification](#verification)). The runtime verifies by
**recomputation into the consumption record, with no exceptions**;
binding the results to committed roots is the `validate_block`
wrapper's job (see
[Requires Lifting](#requires-lifting-pov-proofs)). A tampered input
merely yields an endpoint no lift can bind: node-side pre-verification
(fetching under a root verified at the receiver's tier) protects the
honest collator, the lift protects consensus.

Unlike the fetch protocol, the inherent format is **chain-internal, not
interoperability-normative**: only the resulting commitments are
consensus-visible to anyone else. The shape:

```rust
/// What the inherent provider hands the runtime per block. Built
/// node-side by the fetch pipeline from responses verified under the
/// receiver's tier roots; the runtime re-verifies everything it uses by
/// recomputation. Deliberately no roots and no proofs of any kind.
///
/// Keyed by the typed consumer-side ids, one map per stream kind: a
/// stream's kind fixes its discipline, so a kind/discipline mismatch is
/// unrepresentable, and map keys make "at most one item per stream"
/// structural. The node never translates back from StreamId: it fetches
/// starting from these ids (`consumed_streams()`, same fields) and files
/// each response under the id it fetched for.
///
/// The same type carries the node's offer (inherent data) and the
/// extrinsic `create_inherent` builds from it. Both decode strictly: keys
/// must be strictly increasing in every map, because SCALE's default map
/// decoding would accept unsorted input and silently drop duplicates.
/// Only the extrinsic is bounded: at most MaxTouchedStreams entries
/// across all maps together. The offer may hold more; `create_inherent`
/// trims it (see Caps below).
/// Processed in field order: channels, registers, events (they touch
/// disjoint state—InChannels, OutChannels, Subscriptions—so the order
/// fixes determinism, not semantics).
struct MessagingInherentData {
    /// Prefix discipline: the sender's next messages on the channel, in
    /// order, appended to the `InChannels` entry's frontier. Order and
    /// count are self-enforcing—any deviation yields an endpoint no lift
    /// can bind.
    channels: BTreeMap<InChannelId, Vec<Payload>>,
    /// Register reads on our outbound channels' Ack streams (lossy
    /// discipline). The highwater is `OutChannelState.register.position`.
    registers: BTreeMap<OutChannelId, LossyRead>,
    /// Event reads on active subscriptions (lossy discipline). The
    /// highwater is `Subscription.highwater`.
    events: BTreeMap<BroadcastId, LossyRead>,
}

/// Lossy discipline: the item carries the stream's frontier just before
/// its first payload, standing in for the frontier a lossy consumer
/// deliberately doesn't keep (as served in the fetch response).
/// Trust-free: it decodes through the `MmrFrontier::from_parts` checks,
/// and roots bind their leaf counts (see MmrRoot), so a lie yields an
/// endpoint no lift can bind. An event read carries one or more payloads;
/// a register read exactly one—normally the head, though consensus
/// doesn't require it (see Flow Control: Reading). Both obey the
/// highwater rule below.
struct LossyRead {
    frontier: MmrFrontier,
    payloads: Vec<Payload>,
}
```

**The highwater rule.** Each lossy stream has one highwater: the position
of the last leaf applied (`Subscription.highwater`; for registers,
`OutChannelState.register.position`). A read is admissible only if its
first position (`frontier.leaf_count`) exceeds the highwater. Applying
it moves the highwater to its last position. Positions are trustworthy
because the lift binds the read to a committed root, and roots bind
their leaf count.

- **The highwater moves only by applying a read.** Any other change sets
  a position no committed root vouches for. Set too far, the stream
  becomes permanently unreadable for this chain (see
  [Threat: Position Forgery](#threat-position-forgery-peak-height-confusion)).
  To skip ahead, read later. A consequence the PoV reservation relies on:
  the previous read of a stream ended at `highwater + 1`.
- **It is permanent.** It survives unsubscribing and closing, so nothing
  below it can ever be applied again.
- **It gives monotonicity, not freshness.** Any genuine leaf above it is
  acceptable, including, on a first-ever read, an old one. The runtime
  cannot know a stream's head, so it cannot enforce more.

Private-use stream kinds have no place in this shape: the standard
pallet cannot consume a stream whose semantics it doesn't know, and
`consumed_streams()` cannot list one either. Consuming them would need
an extension of both the runtime API and this inherent—a coordinated
node and runtime upgrade of the consuming chain; not specified here.

No proof object crosses the node–runtime boundary. A read hands the
runtime the fetch response's frontier and payloads, and the runtime
verifies them by recomputation, as it does everything. Binding the
result to a committed root is the job of the candidate's lift.

Dispatch rules:

- **Mandatory, and first in the block** (like all inherents)—consumption
  capacity cannot be crowded out by extrinsics (see
  [Delivery Contract](#delivery-contract-ordered-and-guaranteed)).
- **Absent = consumed nothing.** A block that fetched nothing carries no
  inherent call and records nothing. A candidate whose blocks all record
  nothing emits no `Requires`.
- **At most one item per stream and block**—the consumption record's
  one-interval-per-stream shape, structural (map keys, strict decode).
- **Strict on import: one invalid item invalidates the block.** An item
  is admissible only if all of these hold:
  - its stream is listed by `consumed_streams()` (the watching rule);
  - it is non-empty;
  - its kind's rules accept its payloads (Channels: Violations, the
    highwater rule);
  - it stays within the caps below.

  Inherent items pay no fees, so tolerating invalid ones would let a
  collator pad blocks for free. Strictness also keeps the consumption
  record simple: it records all items.
- **Filtered when building, by the runtime itself.** The node passes the
  runtime everything it fetched and verified against roots. The runtime
  decides what goes into the block: its `create_inherent`, which runs
  during block building, cuts each item just before the first payload it
  doesn't admit and drops items left empty. Honest blocks thus never
  contain an invalid item. This is the established pattern for inherents:
  `paras_inherent` on the relay chain works this way, and so does
  `parachain-system` for inbound DMP/HRMP messages (it drops
  already-processed ones and truncates by size). Three requirements make
  it sound, all ensuring that building and import reach the same verdict:
  1. **Same code.** One admission function decides both: `create_inherent`
     uses it to cut, dispatch to accept or reject. 
  2. **Same state.** `create_inherent` runs before any inherent is
     applied; dispatch runs after the inherents ahead of this one. So
     admission may read only state nothing changes in between: the
     messaging pallet's own storage and its delivery queues. Hence no
     earlier inherent may call the channel API, and delivery only
     enqueues—messages execute in the message queue's servicing hooks,
     before or after this window.
  3. **No budget checks at dispatch.** The block's remaining weight and
     PoV budget depend on the other inherents, so they fail (2). Trimming
     to the budget happens only in `create_inherent` (which can read the
     parachain inherent data); dispatch never refuses a mandatory inherent
     for weight.
- **Caps**—receiver-side constants, checked in both places
  (`create_inherent` trims to each):

  | Cap | Limits |
  |---|---|
  | `MaxTouchedStreams` | streams per block, across all three maps (the node reads it via `max_touched_streams()`) |
  | `MaxInboundMessagesPerItem` | messages per item |
  | the session's `max_message_size` | each message (see Channels) |
  | `MaxQueueFootprint` | a channel's delivery queue |
  | `MaxCommitmentEntries` | distinct sources per bundle. A protocol constant, not chain config: it bounds the `RequiresSet` the relay chain checks (see Requires Lifting) |

  The sender's constants never appear here: the receiver can't know them,
  and one catch-up item spans many sender blocks anyway.

**PoV weight reservation.** Lifts and skip proofs are attached to the
PoV after the block is built (see
[Requires Lifting](#requires-lifting-pov-proofs)). So the STF reserves
their worst case up front, as `proof_size` weight charged before
dispatch:

| Item | Reservation |
|---|---|
| any stream, first touch in the bundle | one lift: the maximum encoded `RequiresLift` without advances (~3.7 KB), with its `advances` length prefix at 2 bytes (a bundle has at most 256 blocks) |
| a source chain, first touch in the bundle | its framing in the lift transport: `ParaId` (4 B) and its `Vec<RequiresLift>` length prefix |
| the bundle, first item | the lift transport's outer length prefix |
| channel, later touch | nothing: a channel never skips, and the candidate needs one lift per stream, not per block |
| lossy read, later in the bundle | exactly the skip proof `stitch` will need: `MMRExtensionProof::encoded_len_for(n1, n2)` with n1 = highwater + 1 (where the previous read ended) and n2 = `frontier.leaf_count`. Zero when n1 = n2. Typically 37–170 B, at most ~1.5 KB |

"First touch" comes from `BundleStreams`. The highwater is the one before
this item applies. The exact skip size is enforced, not estimated:
`verify` accepts a skip proof only with the node count that n1 and n2
determine, and compact encodings are canonical, so every skip proof a
valid candidate can carry for this gap has this size. The item's
frontier is not verified at admission, but it is safe to size by: a
false frontier gives an interval that no chain check or lift accepts.

The reservation covers only material attached *after* authoring; a
`LossyRead` item's frontier rides in the block body and
is simply measured as block bytes. Block building keeps
`block + storage proof ≤ POV limit − reservation`, the caps bound the total
reservation, and a candidate carrying the full worst-case lift set always
fits. Two conditions make that hold, because a mandatory inherent is never
refused for weight: `create_inherent` trims the inherent so that payload
bytes, storage and reservation fit the block's remaining proof budget; and
the inherent's post-dispatch weight never refunds the reservation (weight
reclaim must leave it in place).

#### Relay Runtime API (the Node–Relay Boundary)

The node–runtime boundary has a relay-side counterpart: for the
inclusion tier the collator needs the newest *included* `StreamsRoot`
per source—to fetch under, to build lifts toward, and to verify
responses against. One call:

```rust
/// Newest included StreamsRoot of the given sender: the head of its
/// RecentProvides ring. None if the sender never provided or was
/// offboarded—callers need not distinguish (either way there is
/// nothing to fetch under).
fn newest_included_provides(source: ParaId) -> Option<StreamsRoot>;
```

Head only, deliberately: for on-chain roots authoring policy targets the newest
(see [Verification](#verification)); returning the ring would
harden the window size `W` into API surface, and skipped intermediate
roots cost nothing—there is no obligation to observe every update. An
API rather than a well-known storage key, also deliberately: well-known
keys earn their keep where reads must be *proven* (PVF storage proofs);
here the node queries its own relay client state, and the API keeps the
storage layout private. Polling it on relay block import doubles as the
entire recovery story—the first query after a restart is all the state
a returning collator needs.

Not strictly necessary: the same information is derivable by tracking pending-availability
candidates across their inclusion—but that is a standing node-side
state machine, and it has no recovery path while the sender is idle
(nothing pending, and the latest included head carries no digest if
that block was quiet; bootstrapping would mean walking the sender's
header chain backwards for the last digest). Ten relay-side lines
delete both.

The other tiers need no API at all. **Optimistic (backed / pending
availability)**: the existing
`ParachainHost::candidates_pending_availability` returns the
**committed** candidate receipts—full commitments, `Provides` signals
included (unlike the `CandidateBacked`/`CandidateIncluded` *events*,
which carry only the commitments hash). Newest optimistic root = the
last `Provides` among the pending candidates, falling through to
`newest_included_provides` when none of them provided; matchability of
those roots at the receiver's own inclusion is the pending-availability
window extension (see
[Relay Chain Matching](#relay-chain-matching)). **Speculative**: roots
come from verified header digests, ahead of the relay entirely. As with
the messaging inherent, nothing read through any of these paths enters
a runtime: roots steer fetching and lift targeting, node-side only.

### Off-Chain Communication (Between Collators)

Messages are exchanged off-chain between collators. The relay chain never sees
message contents—only commitments. This section defines the fetch protocol
and how received data is verified. It is normative for interoperability:
the two ends are implemented by different chains' collators.

#### Fetch Protocol

Addressing is by *(stream, message position)* and trust is by *provides
root*—never by block. Per-stream positions are dense (0, 1, 2, ... with no
gaps, by construction), so a receiver resuming after downtime does not
traverse the sender's blocks looking for ones that wrote to its stream.
Every request names the `StreamsRoot` it is willing to depend on—one the
requester verified at its tier (a window entry read off the relay chain,
a verified header digest)—and every response is independently verifiable
against exactly that root: no intermediate state, no trust carried
between responses, a bad peer wastes at most one response.

**Verifiability covers served data, never negatives.** Every proof format
proves presence under a root; no response can prove a stream has *no
entry* under one. This matters in exactly one state: a channel accepted
before the sender's `Hello` (pre-authorization, see
[Channels](#channels)) is consumed-but-unwritten—no leaf, no tree
entry—so every fetch honestly answers `UnknownStream`, indistinguishable
from withholding. The window is no consensus concern: blocks record only
*touched* streams, so an armed-but-empty stream creates no requires, no
lift, no relay matching—nothing ever asserts emptiness, and the first
payload that does arrive is verified like any other. The contract is
node-side: while a cursor in `consumed_streams()` is 0—a channel
accepted with nothing consumed yet, or a register read before the
receiver ever accepted—`UnknownStream` is an *expected* answer: retry
later, no peer penalty, and ask a different peer next time, so a lying
peer can delay a stream's start but never block it. Once the stream's
frontier is nonzero the stream provably exists; from then on
`UnknownStream` is misbehavior, and up-to-dateness itself is always
verifiable (a `Some(cursor)` request's empty extension plus tree proof
proves "nothing new under this root"). A tree *non-inclusion* proof
(neighbor-leaf path showing the absent key's lookup lands elsewhere)
would make even the pending window's emptiness provable—an optional
peer-scoring upgrade, not a correctness requirement.

```rust
/// The protocol's one request. Every use (channel consumption, register
/// and event reads, range reads, lift material) is this request with
/// different parameters; see the table below.
struct MessagesRequest {
    /// A stream of the serving chain.
    stream: StreamId,
    /// The root the response must verify under: one the requester has
    /// already verified at its tier. The server cannot push a newer,
    /// unconfirmed root on it; for fresher data, the requester asks again
    /// under a newer root.
    under: StreamsRoot,
    /// First leaf wanted. `None` means the head under `under`; the
    /// requester only knows the root, so the server resolves the position.
    start: Option<MessagePosition>,
    /// Payload budget. The server returns payloads from `start` until the
    /// budget runs out (it may cap lower); the extension covers the rest.
    /// 0 asks for proofs only.
    max_bytes: u32,
}

struct MessagesResponse {
    /// The stream just before the first leaf served. Its `leaf_count` is
    /// the start position. Decodes through the `MmrFrontier::from_parts`
    /// checks.
    frontier: MmrFrontier,
    /// Payload i sits at position `frontier.leaf_count + i`.
    payloads: Vec<Vec<u8>>,
    /// From the frontier after the payloads to the stream's state under
    /// `under`. Empty when the payloads reach it.
    extension: MMRExtensionProof,
    /// From the stream's root to `under`.
    tree_proof: TreeInclusionProof,
}

enum FetchError {
    UnknownStream,
    UnknownRoot,
    OutsideHorizon { lowest: MessagePosition },
    BeyondHead,
}
```

Every field of a response is trust-free: roots bind their leaf counts
(see MmrRoot), so a wrong frontier or count cannot verify under
`under`. How the requester verifies a response is defined in
[Verification](#verification).

**Head requests.** For `start: None` the response must end exactly one
leaf after its frontier. The end count is `extension.leaf_count`, or
`frontier.leaf_count + payloads.len()` when the extension is empty. A
server therefore cannot pass off an older leaf as the head. The verified
`frontier.leaf_count` tells the requester the head's position. If the
budget was too small for the head's payload, the response proves only the
position, and the requester asks again with `Some(position)`.

**`start` at the count** (one past the last leaf) is valid: it returns no
payload and an empty extension, which proves "nothing new under this
root".

**Errors:**

| Error | Meaning | Peer misbehaved? |
|---|---|---|
| `UnknownStream` | the stream has no entry under `under` | no while the requester's cursor is 0 (see above); yes once the stream provably exists |
| `UnknownRoot` | the server doesn't know `under` (not yet seen, or pruned) | no |
| `OutsideHorizon { lowest }` | `start` is below the server's retention boundary; `lowest` is the first position it can serve | yes if `start` is within the serving obligation (see [Liftability](#liftability)) |
| `BeyondHead` | `start` is past the stream's count under `under` | only if the requester knows the count reaches `start` (e.g. from a verified response under an ancestor root); otherwise ask another peer |

`UnknownStream` takes precedence over `BeyondHead`. Like `UnknownStream`,
`BeyondHead` is a negative and cannot be proven.

**Uses:**

| Use | `start` | `max_bytes` |
|---|---|---|
| Channel consumption | `Some(cursor)` | budget |
| Register or event read (the newest) | `None` | enough for one message; if too small, the response proves the position only and the requester asks again with `Some(position)` |
| Gap leaves for a skip proof (see Liftability) | `Some(previous read's end)` | budget |
| Event range read | `Some(highwater + 1)` | budget |
| Lift material, including resubmission | `Some(endpoint)` | 0 |
| Refilling the own archive after sync without execution | `Some(first missing position)`, then `lowest` after an `OutsideHorizon` | budget |

Everything is bounded by construction: responses are capped by
`max_bytes`.

#### Verification

Two layers verify received data, with a sharp division of labor. The
*runtime* verifies payloads by **recomputation only**—it never sees a
`StreamsRoot` (a tree proof checked in the STF would bind to a hash the
inherent provider chose, verifying nothing; binding to committed roots
happens in the PVF, see
[Requires Lifting](#requires-lifting-pov-proofs)). The *collator*
authenticates every response it accepts, node-side. Nothing on the wire
is taken on faith:

1. Pick the starting frontier. For a channel it is the stored frontier
   from `consumed_streams()`, and the response's `frontier` must equal
   it. For a lossy read it is
   the response's `frontier`.
2. Hash each payload into its leaf (`H(LEAF_TAG ‖ version ‖ payload)`)
   and append it, in order. Order and count need no separate check: any
   other order, or a skipped message, gives a different root. This step
   also runs in the runtime, on the inherent's payloads. It is all the
   runtime ever does.
3. Node-side: apply `extension` to get the stream's root, walk
   `tree_proof` to get a `StreamsRoot`, and compare it with `under`, a
   root the collator had already verified at its tier. For a head
   request, also apply the head rule (see Fetch Protocol).

A mismatch anywhere discards the response and the peer. A match
authenticates the whole response.

Consecutive extension proofs compose—a holder of a proof to root `t` can
splice on a later `t → t'`—which is how lift material is kept current
past the serving horizon (see [Liftability](#liftability)).

**Root choice and consumption boundaries.** The target root—the one the
collator authenticates against node-side and builds the lifts toward—
depends on what kind of root it is (tiers: see Off-Chain Verification):

| Target | Choose | Why |
|---|---|---|
| on-chain (in the stored window) | the **newest** that contains the messages | it slides out of the window last: staleness spends the window's pipeline slack (see [Window Depth](#window-depth)) |
| speculative (pending, or seen off-chain) | the **oldest** that contains the messages | its candidate enacts soonest and carries the fewest transitive dependencies; a newer root would also wait for the sender's later candidates |
| retarget (a lift at resubmission) | an **on-chain** root | resubmission concerns old messages by definition; depending on nothing pending keeps recovery robust when the optimistic path failed |

The one case this cannot prevent (speculative tier): a targeted dependency that
is itself forced to land after us (its own parents depend on us). The block then
can't be included as built and is rebuilt. On Polkadot enforced minimal claim
queue positions will make this robust under normal network conditions, on JAM we
can accumulate across JAM block boundaries and therefore timing matters less
there.

| Consumption boundary | Lift shape |
|---|---|
| Caught up: boundary = the stream's entry under the target root | tree proof only (empty extension) |
| Behind: unconsumed messages remain past the boundary | tree proof + connecting nodes (see [Requires Lifting](#requires-lifting-pov-proofs)) |

The caught-up row is the steady-state hot path, and it is insensitive to
the sender's *other* streams: an unchanged stream root remains that
stream's entry under every newer `StreamsRoot`. The behind row means
exactly one thing: more messages are pending on this stream than this
block consumed (weight/POV budget, downtime being caught up on)—the
extension proof lifts the requires past the unconsumed tail.

#### Leaf Hashing and Domain Separation

```
leaf preimage:
  LEAF_TAG ++ LEAF_VERSION ++ payload
```

The preimage is **transient**: assembled, hashed into the leaf, discarded. It
is never stored and never sent. What exists anywhere is the bare `payload`
(sender storage, wire responses, archives) and hashes (frontiers, proofs). Every
hasher reconstructs the preimage itself: tag is a constant, version fixed
per stream (see below), payload the message.

Every preimage byte has an arguable purpose—and nothing else is included (see
"What is deliberately NOT in the preimage" below):

**Why the domain tag.** Distinct tags (`LEAF_TAG` / `INNER_TAG` / `PEAK_TAG`)
for leaves, inner nodes and peak bagging. Without them, every node is just
`H(bytes)` and the root commits to a web of hash relations, *not* to the
tree's shape—several shapes can be consistent with the same root, and the
prover picks the reading that suits them.

Concretely: without tags a 63-byte
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

**Why a leaf version.** It versions the *preimage layout above*, not the
payload (payloads version themselves, e.g. XCM). A stream's leaf format
is fixed for the stream's lifetime: a new format means new streams, never
a switch inside an existing MMR, which is append-only and can't be
rehashed. The format follows from the stream, so nothing on the wire or
in the inherent names it.

The version byte is defense in depth. It makes formats hash-disjoint, so
a consumer that hashes a stream under the wrong format fails reliably at
the root check rather than misparsing. A channel's payloads don't reveal
their leaf format, so this guard is the only one. It costs one byte per
leaf hash and has no protocol surface. A plausible future v2 field is a
genesis hash for stronger sender-identity binding (see the ParaId-reuse
threat in Security Analysis, currently assessed as unnecessary).

**What is deliberately NOT in the preimage.** Earlier drafts (and the initial
primitives, [#12346](https://github.com/paritytech/polkadot-sdk/issues/12346)/
[PR #12368](https://github.com/paritytech/polkadot-sdk/pull/12368)) also bound
`source`, `destination`, `position` and a length prefix. Removed—each fails
the "name the attack it prevents" test in this architecture:

- **source / destination**: stream context is already bound *structurally*.
  Every stream root lives at its `StreamId`'s key in the sender's
  commitment tree, and every verification walks a tree proof that binds
  that key—a root never exists detached from its stream. A message in A's
  `Channel{C, d, n}` stream lives only under that stream's entry and can
  never verify against the `Channel{B, d, n}` entry, regardless of
  preimage contents. Cross-stream/cross-source replay is impossible
  without any leaf-level binding.
- **position**: order and multiplicity are what the MMR *is*—appending the
  same payloads in a different order or count yields a different root.
  Absolute position is bound one level up: every MmrRoot commits its leaf
  count, from which all node positions are derived, never claimed (see
  MmrRoot). A leaf-level position field would duplicate that.
- **length prefix**: with the payload as the single trailing variable-length
  field, the encoding is already injective.

### Relay Chain Matching

When the relay chain processes candidates for inclusion, it performs commitment
matching. The relay chain only sees the minimal commitments (hashes), not
internal state.

The relay chain maintains **one small ring per sender**—the last W committed
`StreamsRoot`s—and nothing else:

```rust
/// Bounded ring of the last W StreamsRoots of one sender. Pushed on each
/// enactment of a sender candidate that emitted a Provides signal (idle
/// blocks push nothing—an inactive sender's window never expires); oldest
/// root drops out. This is the only historical-commitment storage anywhere
/// in the system—parachain runtimes keep frontiers and tree nodes, which
/// reproduce only the current state.
struct RecentRoots(BoundedVec<StreamsRoot, ConstU32<W>>);

RecentProvides: StorageMap<ParaId, RecentRoots>,
```

Total relay chain state for all of messaging: W × 32 B per registered
sender—**fixed-size, independent of how many streams, channels or
subscribers exist**. Pruned as a whole when the sender offboards; there is
nothing finer-grained to manage.

#### Window Depth

A receiver's block requires a `StreamsRoot` chosen at *authoring* time;
matching happens against relay state at *enactment*. Absorbing that gap is
the window's *sole* purpose: W must cover the sender blocks produced
between the required root's enactment and the receiver candidate's
enactment—the candidate pipeline (authoring → backing → availability →
enactment, ~2–3 relay blocks ≈ 12–18 s in normal operation, longer when a
candidate waits for its dependencies), including bursts—an
elastic-scaling sender may commit several blocks per relay block. The
window is *not* lag tolerance for slow consumers: authoring targets
recent roots (see the root-choice rule under
[Verification](#verification)) and covers consumption lag with POV lifts
instead (see [Requires Lifting](#requires-lifting-pov-proofs)). Since the
window slides one entry per provides-emitting sender block:

| Sender block time | Sender blocks in 18 s | W = 128 covers |
|---|---|---|
| 6 s | 3 | ~13 min |
| 2 s | 9 | ~4 min |
| 500 ms (Low-Latency v2) | 36 | ~64 s |

At W = 128 the cost is 4 KB per sender—about 800 KB relay-wide at 200
parachains—so W can be chosen generously and the
normal pipeline matches proof-free with ample slack (~3.5× even at 500 ms
blocks, 40×+ at 6 s). W does not price the lookup either: entries name
what was the sender's newest root at authoring, so a newest-first linear
scan hits within the pipeline depth (a handful of entries) regardless of
W—the per-source storage read dominates, and no index structure is
warranted. Outrunning the window is not a failure mode: lifts are
regenerated against the current provides at every (re)submission (see
[Requires Lifting](#requires-lifting-pov-proofs)). That relies on the
sender's streams being append-only—only the sender's own runtime enforces
it. A sender that rewrites a stream (a runtime bug, a forced storage
change) can strand lagging receivers' blocks: a liveness failure for
them, never a soundness one, since nothing it didn't commit can verify.

#### Matching Against the Virtually Extended Window

There is only *one* check, and it happens at **enactment**. In each relay
block, the stored windows are **virtually extended** by the `Provides`
roots of all candidates that could enact in this block, and every requires
entry of those candidates is matched against the extended window. A match
against the stored part is settled; a match against the virtual part
holds only if the providing candidate enacts too—so dependent candidates
enact together or not at all. On enactment a root becomes permanent
(pushed into `RecentProvides`).

Nothing is checked at backing. The order in which a sender and its
receiver get backed therefore doesn't matter: whichever comes first waits,
available but not enacted, until the other is available too—exactly as
an available candidate waits today for its predecessor—and then both
enact in the same block. A dependency on a root another candidate provides
is a dependency on the *root*: any candidate providing it satisfies it
(e.g. a resubmission of the same block).

The check is a worklist pass over the candidates that could enact,
removing every candidate whose requires can't be met and propagating each
removal to whatever depended on it:

```rust
/// A pending candidate: its parachain and its position in that
/// parachain's chain of pending candidates (0 = next to enact).
type CandId = (ParaId, usize);

struct PendingCandidate {
    available: bool,                          // availability reached
    provides: Option<StreamsRoot>,            // its Provides signal, if any
    requires: Vec<(ParaId, StreamsRoot)>,     // its Requires entries
}

/// Each relay block: which pending candidates enact.
fn enactable(
    pending: &BTreeMap<ParaId, Vec<PendingCandidate>>, // per para, chain order
    stored: &RecentProvides,
) -> BTreeSet<CandId> {
    // 1. Candidates: per para, the available prefix of its chain
    //    (a candidate enacts only after its predecessor—today's rule).
    let mut alive: BTreeSet<CandId> = BTreeSet::new();
    for (para, chain) in pending {
        for (pos, c) in chain.iter().enumerate() {
            if !c.available { break; }
            alive.insert((*para, pos));
        }
    }

    // 2. The virtual extension: roots provided by candidates still alive.
    let mut provider: BTreeMap<(ParaId, StreamsRoot), CandId> = alive.iter()
        .filter_map(|&id| get(pending, id).provides.map(|r| ((id.0, r), id)))
        .collect();

    // 3. Classify every requires entry once.
    let mut dependents: BTreeMap<(ParaId, StreamsRoot), Vec<CandId>> = BTreeMap::new();
    let mut worklist: Vec<CandId> = Vec::new();
    for &id in &alive {
        for &(source, root) in &get(pending, id).requires {
            if stored.contains(source, root) {
                continue;                                   // settled
            }
            if provider.contains_key(&(source, root)) {
                dependents.entry((source, root)).or_default().push(id); // co-enacting root
            } else {
                worklist.push(id);                          // matches nothing now
                break;
            }
        }
    }

    // 4. Propagate removals.
    while let Some(id) = worklist.pop() {
        if !alive.remove(&id) {
            continue;                                       // already removed
        }
        // Its root leaves the virtual window: its dependents go too.
        if let Some(root) = get(pending, id).provides {
            provider.remove(&(id.0, root));
            worklist.extend(dependents.remove(&(id.0, root)).unwrap_or_default());
        }
        // Its successor on the same para can't enact without it.
        let next = (id.0, id.1 + 1);
        if alive.contains(&next) {
            worklist.push(next);
        }
    }

    alive // enact all, each para's in chain order; push their roots into `stored`
}
```

- **Transitivity** comes from the propagation: if a provider is removed,
  so is everything depending on its root, and on theirs.
- **Cycles** (mutual dependencies) never enter the worklist unless a
  member has an outside hole, so they enact together.
- **Cost** is linear: each requires entry is classified once, each
  candidate removed at most once, each dependency list consumed once—
  O(n + Σ requires) for n pending candidates, at most
  n · `MaxCommitmentEntries`. Every candidate's requires are fixed when
  it's backed, so the weight is known in advance and charged in
  `paras_inherent`, like backed candidates today.
- **Duplicate roots** (two candidates providing the same root) can't arise
  honestly: a parachain's pending candidates form a linear chain, and a
  root only changes when a block sends, so consecutive `Provides` differ.
  Only a broken sender could repeat a root; then `provider` keeps one of
  them, and losing it removes the root early—dropping dependents too
  early, never too late.

A candidate that never gets to enact—its provider never comes, directly or
transitively—stays pending until its availability timeout, then is
resubmitted with its lift retargeted to an on-chain root (see the
root-choice rule under [Verification](#verification)). A candidate whose
requires can never match costs only its own parachain's core until then.

This check at enactment is the relay chain's entire
involvement in messaging. Note what is *absent*: no per-stream anything
(streams are proven parachain-side, below the hash), no check that a
required source is registered (an absent window never matches—pure
self-harm for the requiring candidate, and the lookup happens anyway), and
no restriction on who may require which source (see Candidate
Commitments).

Mutual dependencies (A requires B's provides and vice versa, the Basti block /
super-chain case) match naturally—both entries are in the virtual
extension—and enact together: each is provided by a member of the set
`enactable` keeps (see [Cycle Handling](#cycle-handling)).

One property of the flat scheme is deliberately given up here: under per-stream
relay entries, a quiet stream's stored root never moved, so a receiver *behind*
on it matched proof-free after any lag. Under the single `StreamsRoot`, a
receiver behind on a stream carries a lift in its candidate's POV: one
extension proof and one tree proof, regenerated at every submission. A
lift is typically under a kilobyte and at most ~3.7 KB, plus one skip
proof per skip within the bundle (typically under 200 B), however far
behind the receiver is. In exchange relay state is
independent of stream count, and the per-stream permanence/budget/eviction rules
that per-stream entries would force on the relay chain disappear wholesale.

### Requires Lifting (POV Proofs)

A block's consumption state must, at inclusion time, be tied to a
`StreamsRoot` the relay chain can match—and by then, the roots that were
current when the data was consumed may match nothing:

- **Partial consumption**: more messages pending than the block's
  weight/POV budget—its boundary lies mid-backlog, under no committed
  root's entry at all (offline catch-up, on-demand, congestion).
- **Resubmission**: the block sat sealed while the window slid on; the
  block body can't change (blocks are permanent, candidates
  transient—[PR #11413](https://github.com/paritytech/polkadot-sdk/pull/11413)).
- **Bundling**: data consumed from a source's intermediate,
  never-committed blocks (a source that is itself bundling).

One mechanism covers all cases—including the case of no lag at all:
blocks never emit `Requires` and never see a `StreamsRoot`; they produce
a **consumption record**, and the validate_block wrapper (PVF)
synthesizes the candidate's requires entries from it, one POV-carried
**lift** per recorded stream binding its state to a current committed
root. The relay chain only ever sees entries it can match. (Verification
inside `validate_block`, like Low-Latency v2's scheduling checks; no new
PVF entry point.)

Why the POV is the right place, and why this needs no cooperation from the
original author: a lift is a pure function of *public* data—the source's
streams and its committed roots. It carries no signature and nothing
secret, so anyone can generate it and assemble a valid candidate around
the unaltered block; the resubmission flow assumes exactly this, and every
attempt regenerates the lifts against the then-current provides, so a
block never goes stale no matter how often it is retried. The block itself
stays a pure function of its body and parent state—collators verify and
acknowledge it as-is; completing it for inclusion is recoverable by
anyone. The cost, honestly: proof verification runs in the PVF (validation
budget) rather than as metered block weight.

```rust
/// Extends the verifier's own MMR state (frontier, or peaks
/// carried by a read) to a newer root, with at most ⌊log₂ d⌋ + 1 hashes
/// for d appended leaves (see Proof Size Bounds).
struct MMRExtensionProof {
    /// Leaf count of the extended MMR—a free variable the verifier
    /// cannot derive (not even from the node count: extending 1 leaf to
    /// 3 and to 4 both take two connecting nodes, placed differently).
    /// Together with the verifier's own leaf count it fixes the MMR
    /// shape completely, which is why the nodes below carry no
    /// positions. Trust-free like every count in the protocol:
    /// verification outputs the count-bound root (see MmrRoot), so a
    /// lie yields a root nothing commits to.
    leaf_count: Compact<u64>,
    /// The appended range's summarizing nodes, in the deterministic
    /// order derived from (verifier's leaf count, `leaf_count`).
    connecting_nodes: BoundedVec<Hash, ConstU32<48>>, // bound enforced at decode
}

/// One lift, carried in the POV (never in the block or commitments).
/// Transported grouped per source (`Vec<(ParaId, Vec<RequiresLift>)>`;
/// decode rejects non-strictly-increasing ParaIds, same canonicality
/// discipline as `RequiresSet`) and matched positionally to the
/// consumption record's streams within each source—the record supplies
/// the key, and a mispaired lift cannot verify: the tree walk binds the
/// record's key, so landing on a committed root means being a valid
/// lift for exactly that stream.
struct RequiresLift {
    /// One proof per skip in the stream's interval chain, in order (see
    /// stitch); empty when no block skipped leaves.
    advances: Vec<MMRExtensionProof>,
    /// Extends the chain's endpoint to the stream's current state;
    /// verification *yields* the current stream root. Empty when the
    /// endpoint already is the target root's entry.
    extension: MMRExtensionProof,
    /// Walked from the computed stream root, *yields* the StreamsRoot
    /// the requires entry becomes—validated by the relay chain's window
    /// match.
    tree_proof: TreeInclusionProof,
}
```

#### The Consumption Record and Entry Synthesis

Blocks never emit `Requires` signals. Instead, processing the messaging
inherent writes a **consumption record**—which streams this block touched
is what the inherent *did*, never state inference; untouched streams
appear nowhere and create no dependency:

```rust
/// Per stream and block: consumption entered the block at `start` and
/// left it at `end`.
///
/// Why this exists: the candidate's lift binds only the LAST state to a
/// committed root. The intervals stretch that guarantee back over the
/// whole bundle—each block must start where the previous one ended, or
/// prove the jump moved forward.
///
/// For every stream kind, `start` is the root before the block's leaves
/// of this stream and `end` is the frontier after them. For a channel the
/// leaves are appended to the stored frontier. For a read they are
/// appended to the frontier carried in the item. `end` is a full frontier
/// because the next interval's check, or the lift, extends from it.
///
/// A channel never gaps: every block continues from the stored frontier,
/// so `start` always equals the previous `end`. A register or event read
/// may skip leaves, because a read takes the newest leaf rather than
/// every leaf. A skip is a gap, and it must be proven forward. Without
/// that, a block could act on a fabricated read and hide it behind a
/// later, genuine read. A read that continues exactly where the previous
/// one ended needs no proof. Overlapping reads are rejected, because no
/// extension goes backward.
struct Interval {
    start: MmrRoot,
    end: MmrFrontier,
}

/// Written per block to a transient outbox (the same storage family that
/// carries `UpwardMessages`) and exposed through a single runtime API,
/// `consumption_record()`. Two callers, one definition: the node reads it
/// via ordinary API dispatch (authoring, acknowledgement checks,
/// diagnostics); the validate_block wrapper reads the same data in-wasm
/// after executing each block, through a hook in parachain-system's
/// configuration—as it already assembles `ValidationResult` from pallet
/// storage (see Implementation Considerations: PVF Changes).
struct ConsumptionRecord {
    /// One interval per stream this block touched. Binding to committed
    /// roots is meaningful only where its output is checked against
    /// relay state, so it lives entirely in the wrapper (lifts).
    ///
    /// Grouped by source, mirroring the lifts' transport and the one
    /// requires entry per source it feeds; per source sorted by the
    /// StreamId's canonical encoding (designed to sort—see StreamId;
    /// `Ord` on StreamId is that order) and unique—the messaging
    /// inherent carries at most one item per stream (by structure). This
    /// is the API view of `ConsumptionOutbox` (see Parachain Runtime
    /// State), grouped and sorted at read time.
    entries: BTreeMap<ParaId, Vec<(StreamId, Interval)>>,
}
```

The wrapper collects, per stream, the intervals of all bundle blocks in
order, and **synthesizes** the candidate's `Requires` set under one
uniform invariant, with no per-kind logic anywhere:

> Per stream and candidate, the recorded intervals must form a proven
> chain—each block continues where the previous ended, or a POV-carried
> proof shows the gap is a forward extension—and one lift binds the
> chain's endpoint to a committed root, transitively binding everything
> the candidate consumed or read.

```rust
/// Stitch one stream's intervals (bundle order) into its endpoint.
/// `advances` must contain exactly one proof per gap, in gap order.
fn stitch(
    intervals: &[Interval],
    advances: &[MMRExtensionProof],
) -> Result<MmrFrontier, Error> {
    let (first, rest) = intervals.split_first().ok_or(Error::EmptyRecord)?;
    let mut gaps = advances.iter();
    let mut current = first.end.clone();
    for next in rest {
        // root() binds the leaf count (see MmrRoot), so the gap check
        // also compares positions.
        if next.start != current.root() {
            // A gap must be a proven FORWARD extension of where we
            // ended—structural, relative check; the absolute binding is
            // the lift's job below.
            let proof = gaps.next().ok_or(Error::MissingAdvance)?;
            ensure!(proof.verify(&current)? == next.start,
                Error::BrokenChain);
        }
        current = next.end.clone();
    }
    ensure!(gaps.next().is_none(), Error::StrayAdvance);
    Ok(current)
}

/// The required StreamsRoot for one source's streams—the caller pairs it
/// with the source id it iterates anyway. `streams` iterates in StreamId
/// order (canonical); `lifts` matches it positionally.
fn build_requires_entry(
    streams: &BTreeMap<StreamId, Vec<Interval>>,
    lifts: &[RequiresLift],
) -> Result<StreamsRoot, Error> {
    ensure!(streams.len() == lifts.len(), Error::LiftCountMismatch);
    let mut entry: Option<StreamsRoot> = None;
    for ((stream, intervals), lift) in streams.iter().zip(lifts) {
        let endpoint = stitch(intervals, &lift.advances)?;
        // The endpoint is contained in the stream's current root
        // (computed, not declared)...
        let current = lift.extension.verify(&endpoint)?;
        // ...and the tree walk from it yields the StreamsRoot this
        // stream lifts to. The walk binds the RECORD's stream key, so a
        // mispaired lift cannot verify.
        let root = compute_tree_root(*stream, &current, &lift.tree_proof)?;
        // All streams of a source must lift to one and the same root.
        match entry {
            None => entry = Some(root),
            Some(prev) => ensure!(prev == root, Error::DivergentRoots),
        }
    }
    entry.ok_or(Error::EmptyRecord)
}

/// The candidate's Requires set, from the per-block records (bundle
/// order) and the POV's lifts, accepted exactly as transported (decode
/// already enforces strictly increasing ParaIds—no conversion, no
/// re-sort). Sources must match exactly—recorded sources without lifts,
/// or lifts for unrecorded sources, invalidate the candidate.
///
/// The set holds at most MaxCommitmentEntries sources, and sources merge
/// across the whole bundle, so a per-block cap would not bound it.
/// Admission therefore refuses a new source beyond the cap, counting the
/// sources in `BundleStreams` (see Parachain Runtime State).
fn build_requires(
    records: &[ConsumptionRecord],
    lifts: &[(ParaId, Vec<RequiresLift>)],
) -> Result<RequiresSet, Error> {
    let mut merged: BTreeMap<ParaId, BTreeMap<StreamId, Vec<Interval>>> =
        BTreeMap::new();
    for record in records {
        for (source, entries) in &record.entries {
            let streams = merged.entry(*source).or_default();
            for (stream, interval) in entries {
                streams.entry(*stream).or_default().push(interval.clone());
            }
        }
    }
    ensure!(merged.keys().eq(lifts.iter().map(|(p, _)| p)), Error::LiftSourceMismatch);
    RequiresSet::try_from_iter(
        merged.iter().zip(lifts).map(|((source, streams), (_, lifts))| {
            Ok((*source, build_requires_entry(streams, lifts)?))
        }),
    )
}
```

Everything is a deterministic function of block data and POV, computed
identically by every validator; the relay chain only ever sees entries it
can match against currently-available state. Prefix streams satisfy
`next.start == current` by statehood—equality, zero proofs. On the hot
path—single block, caught up—`stitch` degenerates to the sole interval's
`end`, the lift's `advances` and `extension` are empty, and the lift is a
bare tree proof, ~300 B per stream. Connecting
nodes appear exactly where a state lags its successor or its target: a
mid-backlog boundary, resubmission after the window slid, a read at an
intermediate root of a bundling source, or a read later in the bundle
that skipped leaves. One shape; the proofs' lengths are the only variable.

Two guards share the replay story, doing different jobs: the *chain* is
structural—extension proofs only exist forward, so verified states cannot
regress, and the endpoint lift transitively binds every interval to the
canonical stream history (requires entries only ever match the canonical
relay chain's windows, so all bound roots lie on one history and
positions are globally comparable). The *highwater* is state—it stops
re-acting on old positions even under a legitimately bound older context
(see [Event Streams](#event-streams)).

#### Liftability

A lift must be generatable whenever needed, otherwise a permanent block
could become permanently unincludable. One serving obligation covers
lifts and all reads:

> **The sending chain's nodes serve leaves, frontiers and extensions from
> any position within the horizon, at O(log) work per request plus the
> payload bytes.**
>
> | Stream | Horizon |
> |---|---|
> | channel | every position above the finalized confirmation watermark (see [Archive Pruning](#archive-pruning)) |
> | lossy (event, Ack) | every leaf written in the last 25 hours, plus the stream's head under the newest included root, however old |
>

Leaves are kept per sender fork (see Archive Pruning). O(log) work means
keeping the MMR nodes, not just the leaves; why that suffices is in
Appendix B. For lossy streams the storage is bounded by the sender's own
blockspace over 25 hours. The 25 hours mirror relay-chain availability
retention and its assumption that collators come online at least once a
day. One honest collator suffices.

What the obligation covers:

- **Lifts.** The tree proof needs no history: lifts target a *current*
  committed `StreamsRoot`, and the tree is runtime state. The extension
  is a proofs-only `MessagesRequest` from the recorded endpoint. The
  response's frontier must equal that endpoint. The extension is at most
  48 hashes however far back the endpoint lies, so even a resubmission a
  day later fetches no leaf chain.
- **Skip proofs** (`advances`). A node re-reading a lossy stream within a
  bundle fetches the leaves from where its previous read ended, and
  builds the skip proof from them (see Requires Lifting). If the budget
  reaches the head, the same response also delivers the newest leaf.
  Before including the read, the node checks that the response's
  `frontier` equals its previous read's end. If it doesn't, the root is
  on a different sender fork: the node re-reads under a root on the
  previous read's fork, or not at all in this bundle.
- **Resubmission.** Skip proofs don't depend on the lift's target, so a
  resubmitter reuses them. One that lacks them refetches the gap's
  leaves, which are retained for 25 hours.

Three layers back the guarantee, all covering the serving horizon (25 h
for lossy streams, the watermark for channels): the
receiving chain's collators keep whatever their own not-yet-included
blocks may still need (first line of defense); the sender-side serving
obligation above (the backstop); and, as a last resort, the committed
sender blocks' POVs in relay-chain availability—stateless re-execution
reproduces every send, hashes and payloads alike.

The consumption record every lift is built from needs no extra
retention: it lives in the block's post-state (`consumption_record()` at
the block's hash), and state pruning only ever removes the state of
*finalized* blocks. A parachain block is finalized only after its
inclusion, so its record stays available for as long as resubmissions can
need it.

### Proof Size Considerations

Worst-case sizes, sized against day-scale lag (the same ~24–25 h horizon
that governs Low-Latency v2 relay-parent age, availability retention and
the lift-serving obligation):

#### Commitment Size

- `Provides`: one `StreamsRoot`, 32 B—constant, regardless of how many
  streams were touched. A chain fanning out to 100 destinations in one
  block commits the same 32 B as one sending to a single peer.
- `Requires`: ~36 B per *source* depended on in this block—naturally
  bounded by the number of parachains, independent of how many streams per
  source are consumed.

#### Recurring Tree Proof Size

Each consumed stream carries a tree inclusion proof to its source's
`StreamsRoot`: one `(split bit, hash)` step of 33 B per branch on its
path. On realistic stream sets (a channel and an Ack stream per peer,
sparse ParaIds, a few broadcasts) paths are about log₂(S) + 2 to 4 steps
long, S being the source's stream count:

| S | Steps | Size |
|---|---|---|
| ≈ 200 | ≤ 12 | ≈ 400 B |
| ≈ 2000 | ≤ 15 | ≈ 500 B |

That is per consumed stream and consuming block. Sharing the upper path
between streams of one source is an optional optimization (see The
Stream Commitment Tree). This is the recurring price of the constant-size commitment.
It rides in the PoV, not on the relay chain, and the original
flat-vs-hierarchical analysis
([PR #10449](https://github.com/paritytech/polkadot-sdk/pull/10449))
found it acceptable: sub-percent of the PoV budget.

#### Lift Size

A lift is one MMR extension proof plus one tree proof per stale stream.
The extension depends only on how far it advances: d leaves cost at most
⌊log₂ d⌋ + 1 hashes, often just one. It does not depend on the stream's
total length or on the sender's other streams.

| Behind by | Extension |
|---|---|
| 1000 messages | ≤ 10 hashes (~6 on average), 200–320 B |
| a day at one message per second (~10⁵) | ≤ 17 hashes, ≈ 550 B |
| ~10⁹ messages | ≤ 31 hashes, ≈ 1 KB |

#### Practical Limits

Lifts are added to the PoV after the block is built, so the block must
guarantee at authoring time that the worst case still fits. The worst
case is resubmission, where every touched stream needs a lift: at most
~3.7 KB per stream, plus one skip proof of at most ~1.5 KB per read that
skipped leaves. These ceilings are structural (`KEY_BITS`,
`MAX_MMR_LEAF_COUNT`) and independent of the sender's stream layout, so
the receiver need not trust the sender. Realistic lifts are 0.5 to 1 KB.
The STF knows every quantity involved and charges the reservation as
`proof_size` weight, so a candidate carrying the full worst-case lift set
always fits (see [The Messaging Inherent](#the-messaging-inherent)).

The number of entries in a `RequiresSet` is capped
(`MaxCommitmentEntries`). Since entries are per source, the natural ceiling
is the number of registered parachains (~200)—the cap exists to bound
candidate receipt growth and relay matching work per candidate, not to
constrain topology. Provides needs no cap at all: it is one hash.

Per-destination streams naturally keep proofs small because:
- Receiver only proves against the stream addressed to it
- That stream only contains messages to that specific receiver
- High volume to other chains doesn't affect extension proof size (and
  affects the tree proof only logarithmically)

#### Proof Size Bounds

| Proof | Bound | Max encoded | Typical |
|---|---|---|---|
| MMR extension / skip proof, `n1 → n2`, d = n2 − n1 | ⌊log₂ d⌋ + 1 ≤ 48 hashes | 9 + 1 + 48·32 = 1,546 B | 1–10 hashes |
| Tree inclusion | ≤ `KEY_BITS` = 64 steps | 2 + 64·33 = 2,114 B | 200–500 B |
| `RequiresLift`, no advances | extension + tree proof | 2 + 1,546 + 2,114 = 3,662 B | 0.5–1 KB |

The extension bound is logarithmic in d for two reasons. The verifier
already holds the old peaks, so only siblings inside the new range are
sent, one per height at most. And all peaks entirely past the old range
are bagged into a single hash. The bound was checked against `mmr_lib`
for every leaf-count pair up to 2^11, and for random and power-of-two
pairs up to 2^48 (worst case: 48 hashes, for `1 → 2^48 − 1`).

The tree bound is structural. A sender choosing pathological keys can
push a path toward 64 steps, so reservations use 64, not the realistic
depth.

**Implementation requirements.** These make the bounds hold by
construction rather than by analysis:

1. **Bounds live in the types; sizes come from the encoder.** Each proof
   type is bounded at decode (48 hashes for MMR extensions, `KEY_BITS`
   steps for tree proofs). The first-touch reservation is composed from
   the components' maximum encoded sizes, never a hand-written number. A
   skip proof's reservation is `MMRExtensionProof::encoded_len_for(n1,
   n2)`, exported by the primitives and computed from the same
   node-count function `verify` enforces, plus the codec's compact
   lengths. It never panics: 0 for n1 = n2, and `None` outside
   1 ≤ n1 ≤ n2 ≤ `MAX_MMR_LEAF_COUNT` (admission then reserves the
   maximum). Its cost is part of the inherent's benchmarked weight per
   item.
2. **Decode enforces the bounds.** An oversized proof fails to decode, so
   no valid candidate carries more than was reserved. A bound set too
   tight can only cost liveness, by rejecting an honest proof.
3. **Tests show sizes match.**
   - Every generated skip proof's encoded size equals
     `encoded_len_for(n1, n2)`, and `verify` rejects every other node
     count, including padded and truncated proofs.
   - Over random read sequences (skips, adjacent reads, reads in
     non-consecutive blocks, ranges, cut items), the reserved skip bytes
     equal the encoded `advances` that `stitch` accepts.
   - Lifts and tree proofs, including adversarial tree keys, stay within
     their maximum encoded size.

   All of these run exhaustively for small counts and randomly and at
   power-of-two edges up to `MAX_MMR_LEAF_COUNT`. The fuzzers assert
   the same.
4. **End to end.** A test bundle that pushes every cap and carries
   worst-case lifts and skip proofs fits the PoV.

### Channels and Flow Control

The layers so far move bytes and verify them: streams define what was
sent, commitments and proofs let the relay chain enforce that dependencies
hold. This section defines the **standard reliable convention**—the
interpretation of `Channel` streams, and the HRMP replacement proper: what
a message *is* (message kinds), the delivery contract (ordered,
"guaranteed"), how a communication relationship starts and ends (channels),
how the sender is paced and learns what may be discarded (flow control and
pruning), and the escape hatch when a channel stalls for good (recovery).
A channel is **unidirectional**: one sender data stream
(`Channel { recipient, domain, num }`—ordered data + two signals) plus the
receiver's register stream (`Ack`). Bidirectional communication is simply
two independent channels, one each way. The lossy pub-sub
counterpart—broadcast streams—is specified in
[Event Streams](#event-streams).

Nothing in this section involves the relay chain: channels are a bilateral
affair between the two parachain runtimes, conducted over the very
transport they govern—the sender's signals are ordinary leaves in its
ordered stream, the receiver's voice is its register.

**Terminology.** This section's *confirmations* (the `Register`,
watermarks) are flow-control state between two parachains. They are
unrelated to the collator *acknowledgements* of Low-Latency v2 (signatures
committing to a block becoming canonical, see
[Acknowledgement Extensions](#acknowledgement-extensions)). Distinct words,
used consistently: collators *acknowledge blocks*, parachains *confirm
messages*. (The `Ack{..}` stream-id name is a concession to brevity—it
carries confirmations.)

#### Message Kinds

Every leaf payload on a `Channel` stream is a typed message:

```rust
/// The payload of every MMR leaf on a Channel stream. SCALE-encoded; this
/// encoding is what the leaf preimage's `payload` field contains—the
/// preimage layout (`LEAF_TAG ++ LEAF_VERSION ++ payload`) is untouched.
enum ChannelMessage {
    // ---- Frozen core: parseable at every version, indices fixed ----
    /// Index 0. The sender's answer to an `Accepting` register: opens
    /// session `session` at `version`, which the sender chose from the
    /// range the receiver advertised. Sent only after applying that
    /// register.
    Hello { session: u32, version: u8 },
    /// Index 1. Last leaf of a session the sender took part in: nothing
    /// further until a new session opens.
    CloseChannel { session: u32 },

    // ---- Interpreted under the session's negotiated version ----
    /// Index 2. Userspace payload. The transport delivers the bytes in
    /// order and is blind to their meaning; XCM rides here under a
    /// well-known envelope defined by the XCM integration, not by this
    /// document. Version 1 is the first channel version and defines only
    /// this variant beyond the core.
    Data(Vec<u8>),
}

/// Pallet configuration: runtime constants, changed by runtime upgrade.
/// (MaxCommitmentEntries is a protocol constant, not configuration.)
///
///   Sender
///   MaxMsgLen: u32                         // largest leaf payload
///   MaxMessagesPerBlock: u32               // leaves per stream and block
///   MaxInFlight: u32                       // the in-flight gate, see Flow Control
///
///   Receiver
///   MaxTouchedStreams: u32                 // inherent caps, see
///   MaxInboundMessagesPerItem: u32         //   The Messaging Inherent
///   MaxBundleStreams: u32                  // streams per bundle (BundleStreams)
///   MaxQueueFootprint: QueueFootprint      // per inbound channel's delivery queue
///   SupportedVersions: RangeInclusive<u8>  // channel versions sent and parsed;
///                                          //   advertised when accepting
///   MaxMessageSize: u32                    // advertised when accepting
///
/// Constraints:
/// - 1 ≤ MaxInFlight < MaxMessagesPerBlock. The gate admits at most
///   MaxInFlight leaves per channel and block, plus one CloseChannel, so
///   the per-block cap never stops a channel.
/// - MaxBundleStreams ≥ PoV limit / lift reservation, so the PoV
///   reservation always binds first.
/// The sizes measure the SCALE-encoded `ChannelMessage` leaf payload.

/// Channel API of the messaging pallet, for upper layers (other pallets,
/// governance). domain/num default to 0. Origins are chain policy.
///
/// Sender side:
fn open(recipient: ParaId, domain: u8, num: u16) -> Result<(), ChannelError>;  // want_open = true
fn close(recipient: ParaId, domain: u8, num: u16) -> Result<(), ChannelError>; // want_open = false; closes the session
/// Takes bare userspace bytes and wraps them as `Data`. Signals are sent
/// only by the pallet's lifecycle logic. No hidden queueing: backpressure
/// reaches the caller as an error.
fn send(recipient: ParaId, domain: u8, num: u16, data: Vec<u8>) -> Result<(), SendError>;
/// Broadcast: appends an event to our Broadcast{domain, subdomain, num}.
/// Not gated: nobody confirms lossy consumption.
fn publish(domain: u16, subdomain: u8, num: u32, data: Vec<u8>) -> Result<(), SendError>;

/// Receiver side (`accept_open_channel` is defined in Channels):
fn close_inbound(sender: ParaId, domain: u8, num: u16) -> Result<(), ChannelError>; // closes the session; drains if Opened

enum SendError {
    NotOpen,        // send only: the channel is not open (Channels: Lifecycle)
    TooLarge,       // encoded leaf payload > MaxMsgLen, or (send) > the session's max_message_size
    InFlightLimit,  // send only: the in-flight gate (Flow Control)
    BlockFull,      // publish only: MaxMessagesPerBlock reached
    NotInitialized, // called before block initialization (in on_runtime_upgrade)
}
enum ChannelError {
    InvalidState,   // the lifecycle tables list the call as "error, no effect"
    NotInitialized, // as in SendError: open and close may send a signal
}
```

Design points:

- **One data lane per channel.** Applications needing sub-streams
  multiplex inside `Data`, or open further channels (distinct
  discriminator)—each additional channel replicates the full per-channel
  state through every layer, so the default is one per peer and direction.
- **Versioning is negotiated per session.** Ordered consumption cannot skip
  what it cannot parse, so a receiver must never face a variant it doesn't
  know. The receiver advertises the range it parses (`min..=max`) when
  accepting; the sender picks a version from it and names it in `Hello`;
  that version governs every non-core leaf of the session. The minimum is
  what makes old versions retirable: a sender that only speaks an old
  version never gets a session, and recovers by upgrading. Rules: a
  session's version is fixed for its lifetime (a change is close + accept);
  neither side drops support for a version still in use—the session's
  version once a `Hello` was consumed, and on the receiver side every
  version of an advertised range while still Accepting. This is checked
  before a runtime upgrade is enacted (a try-runtime / pre-upgrade check
  against live channel state), not on-chain; should it be bypassed, the
  channel simply gets stuck at the first leaf the new runtime can't admit
  (see Violations). Three versions exist, only one negotiated:

  | Version | Covers | Set by |
  |---|---|---|
  | Leaf version (`LEAF_VERSION`) | the leaf hashing preimage | protocol, fixed per stream |
  | Channel version (negotiated) | `ChannelMessage` beyond the frozen core | per session |
  | Payload version | the contents of `Data` (e.g. XCM) | the application |

  Downgrade safety needs no extra machinery: the advertised range and the
  chosen version are root-bound leaves, so collators cannot alter them.
  The sender may pick any version in the range; the receiver controls the
  floor through its `min`.

#### Delivery Contract: Ordered and "Guaranteed"

**Ordered delivery is not an added mechanism—it falls out of the existing
machinery, in two layers of different strength.** Order and multiplicity
are *structural*: the receiver appends incoming leaves to its frontier for
that stream, and a reordered, duplicated or fabricated sequence yields a
different root—it cannot match the committed one, no matter what the
runtime does (see
[Off-Chain Communication](#off-chain-communication-between-collators)).
Skipping is different: an extension proof *can* advance a frontier to a
committed root without the payloads in between. No-skip is therefore an
**STF rule**, not a cryptographic impossibility: the runtime computes leaf
hashes from payloads handed to it by the inherent and offers no
payload-free path to advance the frontier—only a runtime upgrade could add
one (see Stall Recovery). Message X+1 is receivable only after X because
the only way forward is through X's payload. The only expressible
misbehavior is a *stall*: stopping consumption entirely.

This yields *guaranteed delivery* in the following sense: a message cannot
be selectively censored or lost while the channel makes progress—either the
whole channel advances past it, or the whole channel stalls at it. Combined
with sender-side archives (one honest collator suffices to serve
retransmits—no availability-system involvement needed), every sent message
remains deliverable for as long as the sender retains it (see
[Archive Pruning](#archive-pruning)). Closing does not weaken this: a
receiver-side close drains everything sent before the sender's
`CloseChannel`.

**Why ordered?** Simplicity and soundness compound:

- **Single-watermark confirmations.** "Received up to position N" is one
  `u64`, cumulative and idempotent—losing or delaying confirmations is
  harmless, the latest supersedes. Unordered delivery would need ranges or
  bitmaps, plus a per-message censorship analysis (which messages may be
  held back, for how long, who tracks the holes).
- **No old-message state.** The frontier position *is* the complete
  consumption state. Nothing tracks individual outstanding messages.
- **Messages always fit.** Incoming messages are processed as part of the
  messaging inherent
  ([#12531](https://github.com/paritytech/polkadot-sdk/issues/12531));
  inherents go first in the block, so consumption capacity cannot be crowded
  out by transactions. How *much* to consume per block remains the
  receiver's choice (weight budget)—POV lifts cover the remainder.

**The cost, owned explicitly:** ordering converts data loss into a permanent
channel stall. If every archive holding an unconfirmed range is lost, the
receiver can never advance past it—by design, it cannot skip. This is
answered by [Stall Recovery](#stall-recovery), not by weakening the
contract. The same holds for protocol violations: they stop the channel
instead of being skipped silently (see Violations, below).

#### Channels

A channel is **unidirectional**: it consists of the sender's data stream
`Channel { recipient, domain, num }` and the receiver's register stream
`Ack { recipient, domain, num }`—same discriminator, kind flipped,
recipient swapped to the other end. The sender speaks through data and two
signals; the receiver's *entire* voice in the channel is its register.
Bidirectional communication is two independent channels—each with its own
discriminator, each in its own sender's namespace, no pairing between them.

```
     One channel = sender's data stream + receiver's register

  A writes:  Channel{recipient: B, d, n}  ── Hello, Data, CloseChannel ──▶  B consumes
  B writes:  Ack{recipient: A, d, n}      ◀────────── Register ──────────  A reads

  (One rule throughout: the recipient field names who READS the stream.)
```

**Accumulator lifetime ≠ channel lifetime.** The channel's streams and
their frontiers are *eternal*: positions never reset, and a frontier
(a few hundred bytes) is never discarded while the peer exists. The
receiver removes its copy only when the peer offboards (see Threat:
ParaId Reuse). The sender's own frontiers and tree entries stay even
then, because the tree has no deletion.
Channel state is layered *on top of* the accumulators, and it is organized
in **sessions**: one session is one accept-to-close cycle with one
negotiation—its parameters are fixed from `Accepting` to `Closed`. Sessions
are labels on the eternal positions, never a reset of them: no replay
across reopen, no stale-frontier failure mode, nothing to re-synchronize.

**The guiding principle: closing exists so both sides can stop watching.**
After a full close the receiver does no work for the channel until it
accepts again; the sender reads the register only while it wants the
channel (to notice the next acceptance).

##### The Register

```rust
/// The receiver's complete voice in the channel, published as a leaf on
/// its Ack stream. Latest-wins: the sender reads only the head, so every
/// register is self-contained (~20 B). Fixed format, strictly decoded,
/// all fields mandatory.
struct Register {
    /// Cumulative watermark: all positions < up_to of the channel's data
    /// stream (data and signals alike—it is a stream position) have been
    /// consumed. Needed in every status, and first so it sits at a fixed
    /// offset.
    up_to: MessagePosition,
    status: RegisterStatus,
}

enum RegisterStatus {
    /// Session `session` accepted; the sender may open it with a `Hello`
    /// at a version in `min..=max`. The parameters are fixed for the
    /// session and appear only here: the sender provably read this
    /// register (its Hello answers it), so they need no repetition.
    Accepting { session: u32, min: u8, max: u8, max_message_size: u32 },
    /// The receiver consumed the session's `Hello`: data may flow. Carries
    /// no session—every `Opened` a sender applies belongs to its current
    /// session (it exists only after that session's Hello, and the
    /// highwater forbids older registers).
    Opened,
    /// Session `session` is closing or closed: the sender sends nothing
    /// further and answers with `CloseChannel{session}` if it took part.
    /// `up_to` reports the drain's progress.
    Closed { session: u32 },
}
```

The handshake runs in this direction—the receiver advertises, the sender
chooses—because it is what lets the parameters appear once: the sender
provably read `Accepting`, whereas nothing guarantees it read any one
later register. A mismatch is visible to the sender immediately (the
ranges don't overlap, so it sends no `Hello`), and the receiver keeps the
authority over the range.

##### Lifecycle

The receiver's phase is its current `status` (see State), with Closed
split by the `draining` flag. Transitions set `status`; it reaches the
sender through the register published at block end (see Flow Control:
Publishing), so several transitions in one block publish only the last.

```
              accept               Hello{s,v}               close
   (none) ───────────▶ Accepting ─────────────▶ Opened ─────────────▶ Draining
                        │  ▲  ↺ accept                 │                  │
                  close │  │ accept      CloseChannel{s}            CloseChannel{s}
                        ▼  │                           ▼                  ▼
                       Closed ◀────────────────────────┴──────────────────┘
```

**Receiver transitions** (consumed leaves and local calls):

| Event | Effect |
|---|---|
| `accept_open_channel()` in (none), Accepting or Closed | `session += 1` (checked); `status = Accepting{session, min, max, max_message_size}` from `SupportedVersions` and `MaxMessageSize`. Accepting again is how the receiver changes its range. |
| consume `Hello{s, v}`, `s == session`, Accepting, `v ∈ min..=max` | `open = {v, max_message_size}`; `status = Opened` |
| consume `Data` while Opened or Draining | encoded size ≤ `open.max_message_size`, parses at `open.version`; enqueue for delivery (see Flow Control: Delivery) |
| consume `CloseChannel{s}`, `s == session`, Opened or Draining | `status = Closed{session}`, `draining = false`, `open = None`. The channel is no longer watched, so the item ends here (any later leaf in it is cut). |
| consume `Hello` or `CloseChannel` with `s < session` | stale: consumed, ignored |
| `close_inbound()` while Accepting | `status = Closed{session}`, no drain: no data can exist (data only follows `Opened`), and the sender may never have opened. A `Hello{session}` or `CloseChannel{session}` the sender may still have sent stays unconsumed until the next acceptance, where it is stale |
| `close_inbound()` while Opened | Draining: `status = Closed{session}`, `draining = true`; keep consuming until `CloseChannel{session}` |
| any other leaf | violation: not admitted (see Violations) |
| any other local call (`accept_open_channel()` while Opened or Draining, `close_inbound()` while Closed or Draining) | error, no effect |

**Sender terms** used by the rules below (fields of `OutChannelState`, see [State](#state); `SupportedVersions` is pallet config, see [Message Kinds](#message-kinds)):

| Term | Means |
|---|---|
| A *took part in* session s | `open.session == s`: A sent `Hello{s}` |
| A *closed* s | `closed_session == Some(s)`: A sent `CloseChannel{s}` |
| A *may send* data | `open` is `Some(o)` with `o.opened`, A hasn't closed `o.session`, and `o.params.version` is in A's `SupportedVersions` |

The last condition holds by the version-in-use rule; should an upgrade
break it anyway, A can't send and has to close and reopen.

**Sender transitions:**

| Event | Effect |
|---|---|
| `open()` | `want_open = true`; evaluate the Hello rule |
| `close()` | `want_open = false`; if A took part in the latest session and hasn't closed it, send `CloseChannel{s}` and set `closed_session` |
| apply a register | store it, and set `session = s` if it is `Accepting{s}` or `Closed{s}`; then by status: |
| … `Accepting{s, …}` | evaluate the Hello rule |
| … `Opened` | `open.opened = true` (A may send, unless it closed the session) |
| … `Closed{s}`, A took part in s and hasn't closed it | send `CloseChannel{s}`, set `closed_session`—whether or not A ever saw `Opened`. `want_open` is unchanged: A reopens on the next `Accepting`. |
| own runtime upgrade (`SupportedVersions` may have changed) | evaluate the Hello rule for every channel with `want_open`—once the block is initialized (see the Hello rule) |

**The Hello rule.** A sends `Hello{s, v}` as soon as all of these hold:
- it wants the channel (`want_open`);
- the latest register it applied is `Accepting{s, min, max, …}`;
- it hasn't taken part in session s yet;
- a version is in both `min..=max` and its own `SupportedVersions`—v is
  the highest such;
- the in-flight gate has room.

It then sets `open = {s, params, opened: false}`. This is a condition on
state, not a reaction to one particular event: A re-checks it at every
event that can change any of its inputs (the table's rows above), so no
order of events can leave a due `Hello` unsent. After a runtime upgrade
the check runs once the block is initialized, since sends are refused
before that (see Parachain Runtime State: sender lifecycle).

Every signal names its session, so a `Hello` or `CloseChannel` from an
older session is stale for the receiver: redundant signals from the sender
are harmless.

**Flows:**

```
Open (pre-authorized or not—same):
  A: open()                    B: accept() → Accepting{s1, min 3, max 27, max_message_size}
  A applies Accepting → Hello{s1, 27}
  B consumes Hello    → Opened
  A applies Opened    → Data …

Close, sender-initiated:
  A: close() → CloseChannel{s1}                    (A stops reading: want_open = false)
  B consumes up to it → Closed{s1}                 (B stops fetching)

Close, receiver-initiated:
  B: close_inbound() → Closed{s1}; keeps consuming (Draining)
  A applies Closed → CloseChannel{s1}; want_open stays
  B consumes it → Closed{s1}                       (B stops fetching)
  A keeps reading only because it wants the channel → waits for the next Accepting

Reopen: B accept() → Accepting{s2, …} → as Open.
```

**Acceptance** is an extrinsic on B: `accept_open_channel(sender, domain,
num)`. The advertised range and size come from B's config. It is the
analog of `hrmp_accept_open_channel`, but entirely on B and priced by B:
transaction fees, plus (per B's policy) a one-time deposit for the
permanent state the first acceptance creates. Later accepts reuse the
entry and need no new deposit. The deposit is released when the entry is
removed, at the sender's offboarding; who gets it back is pallet policy.
Who may submit it is B's policy (the
extrinsic's origin): e.g. anyone paying fees and deposit—typically the
party wanting the channel—or only B's governance. Rejection = never
executing the extrinsic, which costs B nothing.
How A's wish to open gets communicated is outside the transport. Both
orders work: accepting before A wants the channel is pre-authorization
(B fetches A's stream at cursor 0 and gets `UnknownStream` until A's
`Hello`, an expected answer—see Fetch Protocol).

**Unwanted opens cost nothing—on either side.** A writes no leaf before
it has applied an `Accepting`; B tracks no state for channels it never
accepted. No spam surface anywhere, and, unlike HRMP, no relay-chain
deposits or governance in the loop.

**Every remaining indefinite wait is the waiter's own choice.** The
receiver watches while Accepting (it chose to accept) or Draining (the
sender answers every close it took part in). The sender watches only
while it wants the channel. A peer that stops cooperating altogether—dead,
offboarded, or a runtime that ignores the protocol—is exceptional: a
receiver left Draining simply stays there (its collators' fetches find
nothing new) until a runtime upgrade ends the session, if it bothers.

##### Violations

A **violation** is a leaf the channel rules don't admit: data before
`Opened`, a `Hello` outside the advertised range or repeated in the
current session, a `Data` larger than `max_message_size` or unparseable at
the session's version—or a register that fails validation (below). (Leaves
after a consumed `CloseChannel{session}` are not violations: the channel is
then no longer watched, so the item simply ends there.)

A violation is simply **not admitted**, like any other invalid inherent
content (see [The Messaging Inherent](#the-messaging-inherent)): when the
block is built, the runtime cuts the item just before it; on import, an
item containing it makes the block invalid. Nothing about it reaches
consensus state—so there is nothing a collator could fake. The channel
just stops making progress at that position:

- **Receiver side**: the frontier stays before the leaf. `consumed_streams()`
  keeps returning the same cursor; the collators keep offering the leaf and
  the runtime keeps cutting it. The sender sees an unmoving watermark and
  stops at its gate.
- **Sender side** (a register that fails validation): the register is not
  applied; the sender keeps its previous one. A fork-induced register—one
  whose `up_to` exceeds what this fork of the sender has sent—is filtered
  the same way and simply never applied.

**Detection is node-side.** The runtime can't report anything (inherent
construction runs in a rolled-back transaction, and runtime logging is
commonly compiled out of on-chain builds). The collator can: it knows which
payloads it handed to the inherent, so "messages offered, but the
channel's cursor didn't move" is its warning, repeated every block until
resolved. Other reasons a channel's first offered leaf can be cut—the
block's budget or the cap on touched streams—are visible to the node too
(it can compare against the caps), so it warns only when none of them
explains the cut. A node should also remember the stuck position locally
rather than refetch the same leaf every block.

**Resolution** is a runtime upgrade (see Stall Recovery)—typically a bug
fix that makes the leaf admissible, after which it is consumed normally.

Stuck, not discarded: a violation is either a broken sender or a broken
*receiver*—e.g. a runtime upgrade that mis-parses a version it still
claims. Discarding would silently drop valid messages in the second case.
Only the envelope can violate: `Data` payloads are opaque, so an
application-level failure (e.g. an XCM that fails to execute) is handled
above the transport and never blocks the channel.

**Register validation (sender).** A register is admitted only if:
- its `up_to` is ≥ the previously applied `up_to` and ≤ the number of
  leaves this fork has sent;
- `Accepting.session` > `session` (the latest seen), or equal as a
  republish (e.g. `up_to` moved): then the stored register must be
  `Accepting` with the same session and identical parameters; and
  `min ≤ max`;
- `Closed.session` ≥ `session`;
- `Opened` comes while A took part in the latest session.

The highwater rule (see [The Messaging Inherent](#the-messaging-inherent))
stops replay of old leaves; validation stops a peer runtime from publishing nonsense values.

##### State

Channel state is one entry per channel on each side, holding everything
that side knows about the channel. Entries are created on the first open
(sender) or accept (receiver) and **never removed** while the peer exists
(see "Consumption positions are permanent" in
[Parachain Runtime State](#parachain-runtime-state-internal)): closing is a
phase of the entry, not a deletion.

```rust
/// A channel this chain sends on, named by its other end. The full
/// channel identity is (sender, recipient, domain, num); the local
/// chain's own end is implicit. Distinct types per direction: the same
/// (peer, domain, num) names two different channels—A→B and B→A—so one
/// shared type would let an outbound id silently index inbound state.
struct OutChannelId { recipient: ParaId, domain: u8, num: u16 }

/// A channel this chain receives on, named by its other end (see
/// OutChannelId).
struct InChannelId { sender: ParaId, domain: u8, num: u16 }

/// Helper convention: a bare StreamId is one of OUR streams—a key in our
/// own commitment tree and sender-side storage. A foreign stream is
/// returned with its owner, `(ParaId, StreamId)`: that pair is how the
/// sender's tree is addressed (fetch requests, consumption record, lifts),
/// and a foreign StreamId cannot reach own-stream storage without visibly
/// dropping its owner.
impl OutChannelId {
    /// Our data stream: Channel{recipient, domain, num}.
    fn data_stream(&self) -> StreamId;
    /// The recipient's Ack{us, domain, num}, read for its register.
    fn peer_ack_stream(&self) -> (ParaId, StreamId);
}

impl InChannelId {
    /// Our Ack{sender, domain, num}, where we publish the register.
    fn ack_stream(&self) -> StreamId;
    /// The sender's Channel{us, domain, num}, consumed in order.
    fn sender_data_stream(&self) -> (ParaId, StreamId);
}

/// A session's parameters, fixed from Accepting to Closed.
struct SessionParams { version: u8, max_message_size: u32 }

/// Sender side, per outbound channel. The data stream's own frontier is
/// in OutboundFrontier, with all of this chain's streams.
OutChannels: StorageMap<OutChannelId, OutChannelState>,

struct OutChannelState {
    /// Own intent: set by open(), cleared by close(). Survives
    /// receiver-initiated closes, so the sender reopens on the next
    /// Accepting.
    want_open: bool,
    /// Latest session number seen in Accepting/Closed (register validation).
    session: u32,
    /// Latest register applied, with its position on the recipient's Ack
    /// stream: the ack highwater (see The Messaging Inherent: the highwater rule).
    register: Option<AppliedRegister>,
    /// The session we took part in (sent Hello for), its parameters, and
    /// whether we have applied its `Opened`.
    open: Option<OpenSession>,
    /// The session we sent CloseChannel for: no Data in it any more, and
    /// no second CloseChannel. (When a close is fully confirmed matters
    /// only for pruning, which is node-side—see Archive Pruning.)
    closed_session: Option<u32>,
}

struct AppliedRegister { position: MessagePosition, register: Register }
struct OpenSession { session: u32, params: SessionParams, opened: bool }

/// Receiver side, per inbound channel—the id mirrors our Ack stream's
/// fields (see InChannelId::ack_stream).
InChannels: StorageMap<InChannelId, InChannelState>,

struct InChannelState {
    /// Our copy of the sender's channel stream: incoming messages are
    /// appended to it; its leaf count is the position of the next
    /// message to consume, and its `root()` is what consumption is
    /// verified against. Starts empty at the first acceptance.
    frontier: MmrFrontier,
    /// Current session.
    session: u32,
    /// Current status—what the transitions read and write. While
    /// Accepting it holds the advertised range.
    status: RegisterStatus,
    /// The register last published, verbatim (None before the first).
    /// At block end, `Register { up_to: frontier.leaf_count, status }` is
    /// published if it differs from this, and stored here.
    published: Option<Register>,
    /// The session's parameters, from the consumed Hello and our
    /// advertisement: set by the session's Hello, cleared when it closes—
    /// so Some exactly while Opened or Draining.
    open: Option<SessionParams>,
    /// Draining: Closed but still consuming toward CloseChannel{session}.
    draining: bool,
}
```

**Watching** (the normative rule; `consumed_streams()` lists exactly
this):
- The receiver fetches a channel while Accepting, Opened or Draining, and
  its delivery queue has room (see Flow Control: Delivery).
- The sender reads a register while `want_open`.

#### Flow Control

Two needs, both from receiver to sender: **pacing** (the sender must not
outrun what the receiver consumes) and **pruning** (the sender learns which
messages need no longer be retained). The watermark serves both; nothing
else is needed.

**The gate is the sender's own.** The sender's STF refuses a new leaf once

```
in_flight = leaves sent − up_to   ≥   MaxInFlight      (pallet config, ≥ 1)
```

—derivable from `OutboundFrontier`, this block's sends and the applied
register, so it needs no state. `Data` and `Hello` count; only
`CloseChannel` is exempt, so a sender can always close—and since every
new `Hello` needs the gate, at most one `CloseChannel` can exceed it at a
time. Counting `Hello` keeps a receiver that accepts
repeatedly without consuming from growing the sender's stream: at a
session's start no data can be in flight (data only follows `Opened`, and
a session ends drained), so only signals can fill the gate. Backpressure
reaches applications as `SendError::InFlightLimit`; a slow, paused or
dead receiver all look the same—the watermark stops moving and the sender
stops at its limit.

**Delivery (receiver side).** Consuming a `Data` leaf means: verify (it is
appended to the frontier), then **enqueue** the payload for the
application—never execute it. Execution happens later, when the
message queue is serviced (`pallet-message-queue`: in `on_initialize`
before the inherents, and in `on_idle` after the extrinsics), on its own
weight budget; an application failure (e.g. an XCM that fails to execute)
is handled there and never affects consumption. This is how inbound
DMP/HRMP messages are delivered today. The queue's message processor
(e.g. the XCM executor) is chain configuration and not specified here.
Three rules make the queue part of the transport:

- **One queue per inbound channel** (origin = the `InChannelId`), written
  only by the messaging inherent. A backlog on one channel never holds up
  another—the point of having several channels to one receiver. (HRMP
  queues per sending parachain, which is the same thing there: at most one
  channel per pair.)
- **The queue bounds consumption.** A channel is consumed only while its
  queue's footprint is below `MaxQueueFootprint`; the cap is part of
  admission (see [The Messaging Inherent](#the-messaging-inherent)), and a
  full queue drops the channel out of `consumed_streams()`. The backlog
  beyond stays in the sender's archive: the watermark stops, and the sender
  stops at its gate. No suspend signals, and—unlike HRMP's drop
  threshold—nothing is ever dropped.
- **`up_to` means enqueued**, not executed: a message below the watermark
  is in the receiver's state. That is what pruning needs.

Signals (`Hello`, `CloseChannel`) and registers are protocol, not
application: they are processed in the inherent itself.

The receiver limits nothing else: a backlog lives in the sender's archive,
and the receiver pulls at its own pace. Its one advertised limit,
`max_message_size`, exists for liveness—a message the receiver could never
process in one block would stall the ordered channel forever—so the
sender's STF refuses larger `Data` (`SendError::TooLarge`). (The consensus
constant `MaxMsgLen` is the sender's own bound and cannot guarantee
processability at a receiver with a smaller budget.)

**Publishing (receiver side).** At block end, publish one register leaf
`Register { up_to, status }` if it differs from `published`—at most one
per block, whatever happened in it. It is a send on the receiver's own Ack
stream, so it must happen before block end folds this block's stream
roots into the commitment tree (see Parachain Runtime State: sender
lifecycle). No fraction or age cadence: the
sender's in-flight limit is invisible to the receiver, and a sender gated
on a confirmation the receiver hasn't bothered to publish would wait
forever. Each publish is one ~20 B leaf; whether and when to *fetch* it is
the sending collators' decision. Ack streams are lossy streams under the
serving obligation (see [Liftability](#liftability)).

**Reading (sender side).** For each channel in
`consumed_streams().registers`, the sender's collator:

1. **fetches** the recipient's latest register with a head request on
   the Ack stream (`start: None`), under a root `under` it chose;
2. **verifies** the response node-side, like everything it puts into the
   inherent. The head rule guarantees it is the head under `under` (see
   [Fetch Protocol](#fetch-protocol));
3. **includes** it only if it is new, i.e. at or above the channel's
   cursor;
4. **puts** it into the inherent as a `registers` entry: a `LossyRead`
   with the response's `frontier` and the one register (see
   [The Messaging Inherent](#the-messaging-inherent)).

The runtime recomputes the endpoint from it and, as always, sees no
`StreamsRoot`: binding to the committed root is the candidate's ordinary
lift for the Ack stream.

Head-ness is an authoring-time fact, not a consensus guarantee: after
resubmission, a read that was at the head lands under a newer root with
a non-empty lift extension, so neither the STF nor the wrapper can
require "head". What consensus enforces is the highwater rule (see
[The Messaging Inherent](#the-messaging-inherent)), which excludes replay
of superseded registers; register validation (see Violations) excludes
nonsense values.

A collator may apply any register above the highwater, or none. More is
not enforceable, because reading is voluntary and any freshness rule is
circumvented by not reading. It is also not needed, because every
register field is stale-safe:

| Stale field | Effect |
|---|---|
| `up_to` | the sender blocks at its gate early; it never over-prunes (pruning uses only finalized registers) |
| `status` | the sender reacts one read later (opens, stops, or answers a close later) |

Withholding the newest register has the same effects and cannot be
prevented; both are bounded by honest-collator rotation. Applying an
*old* register—the one damage withholding cannot cause—is what the
highwater excludes.

**Which root to read under** is a collator decision (see [Establishing
Trust](#establishing-trust)); optimistic is the default, for register reads as
for data. A read under a not-yet-included root gives the sender's block an
enactment dependency on the receiver's candidate: if that candidate never lands,
the sender's block is dropped—the bet failed. A read under an included root
creates no dependency at all. Which root exactly follows the root-choice rule
under [Verification](#verification).

**The archive bound.** Per channel, the sender retains everything from the
last *finalized* confirmation (see Archive Pruning) up to its last send:

```
retained ≤ MaxInFlight  +  1 (CloseChannel)  +  what was confirmed since finalization
```

That is `MaxInFlight` messages plus the finality lag (bytes:
`MaxInFlight × MaxMsgLen`; a sender wanting a tighter byte bound can keep
the sizes of its in-flight messages locally). The total across channels is
the sender's own choice—it decides which channels it opens.

**Consumption economics.** Inherent-carried messages pay no transaction
fees, and the transport is deliberately blind to payload meaning—so it
defines no fee scheme for consumption. Funding the processing weight is
receiver-chain policy, the usual options being: payload-carried payment
(the XCM `BuyExecution` pattern—parse cheaply, take the fee or drop the
rest unexecuted), a per-channel processing budget funded at acceptance
(the deposit), or plain chain subsidy for traffic the chain wants anyway.
HRMP's inbound processing is unpaid in exactly the same way, so this is
no regression—and unlike there, non-consumption is always legal and
cheap. What the transport *does* supply is the off switch:

**Pausing: the upper layers' off switch.** The receiving chain pauses a
channel's delivery queue, through `pallet-message-queue`'s
`QueuePausedQuery` on the `InChannelId` (Asset Hub pauses suspended XCMP
channels the same way). Execution stops at once. Consumption stops by
itself once the queue reaches `MaxQueueFootprint`. The watermark then
stops moving and the sender blocks at its gate, as for any slow receiver.
Closing is no substitute, because it still delivers up to `MaxInFlight`
more messages.

#### Archive Pruning

The sender-side archive—leaves (payloads) and MMR nodes per stream,
servable under any root of the node's own chain (see
[Fetch Protocol](#fetch-protocol))—is *node-side disk*, not consensus
state—the runtime keeps no payloads beyond the current block (see
[Networking](#networking)). Pruning policy therefore binds no one but the
sender's own nodes. This section covers channel streams. Lossy streams
are retained by time (see [Liftability](#liftability)). The protocol only
defines when pruning is *safe*:

> Prune payloads (and leaf hashes) below watermark W once the receiver
> block that published the register asserting W is **finalized**—included
> in a finalized relay-chain block.

Deliberately conservative: finality is the one point after which the
confirmation can never revert, whatever tier the register was read at.

Sender blocks can revert too, so the same position may hold different
payloads on different forks. W confirms only the fork the receiver
consumed, because its register proves consumption against that fork's
root. Payloads of abandoned sender forks are dropped once the sender's
own blocks finalize.

Pruning is entirely node-side and needs nothing from the sender's own
runtime: the node knows from its own archive which channels still have
unconfirmed tails, and reads their registers itself (collators fetch them
for the inherent anyway; archive nodes that don't author issue the same
head request)—including channels the runtime no longer watches, e.g.
after the sender closed. Whether the sender's runtime *applied* a register is
irrelevant—what proves the consumption is the receiver's block, not the
sender's.

Pruning keeps the MMR serviceable: retain the frontier (peaks) at the
pruning boundary plus every leaf and MMR node above it—that is exactly what appending
and proof generation over the unpruned tail require; nothing below the
boundary is ever needed again (the receiver confirmed it and can never
re-request below its own watermark). A `CloseChannel` the receiver hasn't
consumed sits above the watermark and so stays retrievable.

**Unsafe pruning is policy, not protocol.** A receiver that stops
consuming forever (dead chain) leaves an unconfirmed tail—bounded by
`MaxInFlight`, since the sender stops at its gate. Whether to hold that
forever, apply a generous age-based cutoff, or wait for governance is each
sender chain's local decision: it trades its own disk against breaking
guaranteed delivery for a peer that was dead anyway. No protocol change,
no coordination—the watermark defines *safe*; everything beyond is the
sender's risk appetite.

#### Stall Recovery

A channel stops in two ways: an unconfirmed range is lost from every
archive (all sender-chain nodes pruned unsafely or vanished)—the receiver
cannot skip—or a leaf is not admitted (see Violations). Detection is
node-side: collators see a channel make no progress.

Recovery is a **runtime upgrade**, and no recovery machinery is part of the
protocol. The likely cause of a violation is a bug, so the likely fix is
one: the leaf becomes admissible and is consumed normally. Should skipping
ever be the deliberate choice, the upgrade carries it as one-off logic—
admitting a specific leaf without delivering it, or advancing the frontier
past lost data. Either remains sound because it still runs through the
inherent: a skipped leaf is still appended to the frontier, and a
skip-ahead advances it by an MMR extension proof (Appendix B) recorded as
the block's consumption interval—so the lift binds the result to a
committed root like any consumption. A skipped range is
application-visible data loss, the one breach of the delivery contract,
and should be announced as such by the upgrade that performs it.
### Event Streams

The **standard lossy convention**—the interpretation of
`Broadcast { domain, subdomain, num }` streams (fields elided as
`Broadcast { num }` below; domain/subdomain default 0).
Where channels serve directed, complete, ordered communication,
event streams serve pub-sub: a chain emits notifications, *any* interested
chain consumes them, and only the latest matters. Canonical examples: oracle
prices, state-change notifications, liveness beacons.

The commitment machinery is reused wholesale: the broadcast stream is an
ordinary entry under the sender's `StreamsRoot`; subscribers depending on
it speculatively emit an ordinary `(source, StreamsRoot)` requires entry.
Nothing anywhere scales with audience size: the sender doesn't know its
subscribers, the relay chain doesn't either—horizontal scaling holds
regardless. What changes is entirely the consumption discipline.

#### Consumption: The Head, Not the Prefix

A subscriber does not track a frontier and does not consume leaves 0..N.
It wants the *newest* leaves, typically just the head, and reads them with
a head request (`start: None`, see [Fetch Protocol](#fetch-protocol)).
The response carries the payload and the stream's frontier just before
it, bound to a `StreamsRoot` the subscriber chose. That root is
speculative (freshest, an enactment dependency) or already included
(no dependency), by the same root-choice policy as everywhere else. The
two disciplines side by side:

| | Prefix (channels) | Lossy (event streams) |
|---|---|---|
| Reads | everything up to a point | the newest leaf (or a range above the highwater) |
| Proof bytes per read | none—recomputation is the check | the frontier before the read, ≤ 48 hashes |
| Receiver state | frontier (peaks + count) | one highwater position |
| Must consume | all of it, in order | only what it cares about |
| Gaps | unrepresentable | the normal case |

The carried frontier is the price of lossiness: channels get proof-free
appends *because* they take everything in order. Consuming *every* event
as a separate read would carry a frontier per event versus prefix's zero
proof bytes—"I actually want all of them" is a channel workload wearing a
costume, and belongs there.

**One stream per feed.** A publisher runs one broadcast stream per logical
feed (per asset, per market, ...)—ids are abundant (2⁵⁶ broadcast space),
and "keep the newest" is then trivial: the current value of a feed *is* the
head of its stream, exactly as with ack registers. The spec defines no
sub-stream topic scheme; an application that wants topics multiplexed
inside one stream can build them in its payloads, on its own.

**Replay protection without gap state.** The highwater rule (see
[The Messaging Inherent](#the-messaging-inherent)) is all latest-wins
needs. An old event can never be replayed as the latest, and missed
events are simply never looked at: nothing in the subscriber's state
refers to them. The rule gives monotonicity, not freshness, so payloads
whose meaning depends on age (prices, liveness) carry their own
timestamps, and consumers check them.

**Range reads.** A subscriber may also fetch all events above its
highwater at once, with a request from `highwater + 1`. The response's
`frontier` stands in for the frontier a subscriber doesn't keep, and the
events are verified by recomputation like a channel fetch, applied in
order, and the highwater moves to the last one. Within the 25 h horizon
these events are guaranteed to be served (see
[Liftability](#liftability)). Wanting *every* event, forever, is a
channel workload.

#### No Channel, by Construction

Every piece of channel machinery exists to serve guaranteed delivery, and
lossy renounces the guarantee—so each piece vanishes rather than being
configured away:

- **No confirmations, no back-channel.** There is no delivery
  obligation, so retention is bounded by time rather than by anyone's
  consumption. Pure listeners never need a reverse path.
- **No flow control.** The sender keeps its events for the lossy-stream
  horizon (see [Liftability](#liftability)) and prunes beyond it. A
  subscriber offline for longer misses events. That is the contract, not
  a failure.
- **No handshake**: subscribing is unilateral and invisible to the
  sender—activate a `Subscriptions` entry and start consuming, done.
  Unsubscribing deactivates the entry; the highwater stays. Not
  specified here: the subscription management API (calls, origins,
  deposit) and where applied event payloads go.

The channel protocol's **ack streams** (see [Flow Control](#flow-control))
are exactly this convention with a fixed payload: a lossy stream whose feed
is the confirmation register and whose latest value is the head leaf.
One consumption discipline, two uses.

#### Composing with Channels: Gap Detection and Repair

Lossiness is per the *transport* contract; applications needing occasional
completeness compose the two primitives (the market-data-feed pattern:
lossy stream + reliable retransmission path):

- **Detection**: pure position arithmetic—per-feed streams make positions
  dense per feed, so "highwater p, head p+4" *is* the gap. No sequence
  numbers, no app-level numbering.
- **Repair**: over an ordinary channel to the source—request/response,
  where dedup and ordering are already solved. Note the repair payload is
  usually not the missed events: for latest-wins semantics nothing needs
  repair (the next event supersedes); for completeness semantics the app
  requests the missed *data range* or current state as messages. Old
  events never re-enter through the event path.
- Subscribers wanting repair capability hold both primitives toward the
  source; the channel can be opened lazily on first gap, so pure listeners
  stay back-channel-free until they actually need one.

#### Censorship Profile

Choosing an event stream is choosing a weaker censorship profile, inherent
to lossiness: the subscriber's collators *can* omit individual events
(that is what lossy means)—the monotonic highwater bounds the damage to
delay/omission, never reordering and never stale-as-fresh. Anything
censorship-sensitive belongs in a channel, where per-message omission is
impossible and only whole-channel stalling remains (see
[Delivery Contract](#delivery-contract-ordered-and-guaranteed)).

### Acknowledgement Extensions

For low-latency chains using speculative messaging, the acknowledgement rules
from Low-Latency v2 are extended:

#### Extended Rule for Message Dependencies

> A collator must not acknowledge a block if it depends on speculative messages
  from blocks that are not yet sufficiently confirmed.

"Sufficiently confirmed" = irreversible per
[the tier table](#the-tier-table).

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

### Cycle Handling

**The rule: a candidate may depend on candidates completed at the same
time, never on ones from the future.** Same-time dependencies may form
cycles, and that is fine: they enact together.

Same-time cycles are legitimate and arise in two ways:

- *Bundling*: blocks only ever consume from blocks already seen, yet
  bundled into candidates they can depend on each other mutually (A's
  third block consumed from B's second, B's third from A's second).
- *Super chains*: co-authored blocks of one super-block may depend on each
  other directly, even at the block level.

They are resolved by enactment: the members match through the virtual
window and enact together or not at all—`enactable` keeps a cycle exactly
when all its members are available and their outside dependencies are
satisfied (see
[Matching Against the Virtually Extended Window](#matching-against-the-virtually-extended-window)).

**The timing must hold until enactment, not just at authoring.** Network
latency and pipeline hiccups can reorder candidates on their way to the
relay chain, turning a same-time dependency into a dependency on the
future. Two mechanisms bound this, each specified separately:

- *Polkadot*: the relay chain enforces a minimum claim-queue position at
  backing—a candidate is backed at its declared position or later, never
  earlier ([#12063](https://github.com/paritytech/polkadot-sdk/issues/12063)).
  Backing timing is thereby bounded by protocol parameters, so candidates
  produced together become available together. Only latencies outside
  those parameters break it, and then the affected candidates time out
  and are rebuilt: a liveness cost, never a safety one.
- *JAM*: accumulation is a third phase after inclusion, and it can queue
  work across several blocks. Dependencies can be resolved there even
  when the timing ended up slightly wrong
  ([#13000](https://github.com/paritytech/polkadot-sdk/issues/13000),
  [#12809](https://github.com/paritytech/polkadot-sdk/pull/12809)).

None of this arises in the first iteration: at the optimistic tier, a
candidate depends only on candidates already backed—strictly the past.

### Super Chains

Super chains—multiple parachains run by the same collator set, co-authoring
blocks and exchanging messages within one production cycle—are **future
work**, sketched separately: [Super Chains](super-chains-design.md).

Everything they need from *this* design is already specified and carries no
super-chain special cases:

- **Mutual requires** between co-arriving candidates match through the
  virtually extended window and form atomic enactment groups (see
  [Relay Chain Matching](#matching-against-the-virtually-extended-window)).
- **Candidate-level cycles** are handled, not forbidden (see
  [Cycle Handling](#cycle-handling)).
- **Bundled blocks** settle each source at a committed boundary root, the
  PVF merging per-block requires
  (see [Requires Lifting](#requires-lifting-pov-proofs)).

What remains there—production coordination, super-block acknowledgements,
partial-failure handling—is additive on top of this document and can evolve
without changing it.

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

### The Tier Table

One question decides every trust-sensitive action in this design: **when
is a foreign block irreversible, from my perspective?** This table is the
single, canonical answer—every other section defers to it rather than
restating it:

| Relationship to source | Irreversible when | Backed by |
|---|---|---|
| Same super-chain | co-authored in the same super-block | same collator set ([Super Chains](super-chains-design.md)) |
| Same trust domain | acknowledged by the source's collators | Low-Latency v2 acknowledgement slashing |
| Different trust domain | included on the relay chain | relay chain security |

Two mechanisms consume it, and no other trust rule exists anywhere:

- **Acknowledgement rules**: a collator acknowledges a block only once the
  source blocks it consumed from are irreversible (see
  [Acknowledgement Extensions](#acknowledgement-extensions));
- **Root choice at authoring**: requiring speculative roots buys latency
  at domain trust, backed (optimistic, the default) roots bet on
  availability, included roots are dependency-free—the same policy for
  data and register reads (see [Flow Control](#flow-control)).

Archive pruning deliberately does not use this table: it waits for
finality (see [Archive Pruning](#archive-pruning)).

### Establishing Trust

The per-source tier is **collator configuration**, not runtime state:
every use of it above—which roots to read under, when to acknowledge—is a
collator decision. Nothing in the runtime checks it, and
the requires entries a block produces are valid whichever tier its author
used.

In terms of the consumption tiers (see
[Off-Chain Verification](#off-chain-verification)):

- **Speculative** (same trust domain) follows from configuring collators
  to consider the source's Low-Latency v2 acknowledgements—the only
  reason to do that is speculative consumption from it.
- **Optimistic** (backed roots) is the default.
- **Inclusion** is a collator's own fallback, e.g. when its blocks keep
  failing on an unreliable relay chain.

A "trust domain" is nothing more than a set of chains whose collators
assign each other the speculative tier; no registry of domains exists
anywhere, and nothing is relay-visible. Should collators of one chain
ever need to coordinate their tiers, that is a later, purely additive
step (a runtime-stored default, read via the runtime API).

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
  arrive together—no extension needed, only bare tree-proof lifts

### Mitigation Strategies

#### 1. Domain Size Limits

Limit trust domains to a reasonable size (e.g., 5-10 chains). This bounds the
"blast radius" of cascading delays.

#### 2. Resubmission

If Chain A is censored long enough that Chain B's availability times out, Chain
B simply resubmits. Since both chains are likely resubmitting around the same
time, they'll typically be included together with bare tree-proof lifts,
and extensions are available if necessary, adding robustness.

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
| Latency | 12-18+ seconds | Parachain block time (speculative) or ~2-3 relay blocks (inclusion-based) |
| Scalability | Limited (relay chain state) | High (off-chain, only commitments on-chain) |
| Channels / streams | Scarce: relay-managed deposits, per-channel relay state, governance-mediated | **Effectively unbounded**: no relay-chain deposits, no budget, no relay state per stream—open channels, lanes and topic feeds at will; acceptance priced locally by the receiver |
| Trust | Relay chain only | Relay chain + optional collator acknowledgements |
| Message data | Flows through relay chain | Never touches relay chain |

For the super-chain comparison with parallel-execution runtimes
(Solana-style), see [Super Chains](super-chains-design.md).

---

## Implementation Considerations

### Relay Chain Runtime Changes

1. **New UMP signals**: `Provides(StreamsRoot)` / `Requires(RequiresSet)`,
   both built once per candidate by the PVF wrapper, never emitted by
   blocks (see [Requires Lifting](#requires-lifting-pov-proofs)); the
   relay's cap on signals per candidate rises accordingly
   ([#12347](https://github.com/paritytech/polkadot-sdk/issues/12347)).
   Reusing UMP signals avoids a candidate receipt format change. Rollout:
   older validators reject unknown UMP signals, so the relay-chain changes
   (runtime and node) are released first, and parachains switch on
   emission afterwards—no `node_features` bit
   ([#12349](https://github.com/paritytech/polkadot-sdk/issues/12349)).
2. **Per-sender provides storage**: One ring of the last W `StreamsRoot`s
   per sender, pushed on enactment of provides-emitting candidates; pruned
   as a whole on offboarding. Fixed size (W × 32 B per sender), independent
   of stream count—there is nothing else to store or manage. A parachain
   that offboards without first closing its channels breaks the trust its
   receivers placed in it: their not-yet-included blocks that consumed
   from it can never match again and stall permanently. Proper offboarding
   closes channels first.
3. **Matching at enactment** (`enactable`, see
   [Matching Against the Virtually Extended Window](#matching-against-the-virtually-extended-window)):
   each relay block, the requires of every candidate that could enact are
   matched against the stored windows extended by the roots of the
   candidates enacting with it; a linear worklist pass removes whatever
   can't be met and propagates each removal. Nothing is checked at
   backing, and nothing is stored per candidate beyond its signals.
   Today's inclusion enacts per parachain, each candidate only after all
   its predecessors; enactment has to become dependency-aware, and its
   weight—linear in the pending candidates' requires entries—is charged
   in `paras_inherent`.
4. **Runtime API**: `newest_included_provides(ParaId) ->
   Option<StreamsRoot>`—the ring's head, for inclusion-tier collators;
   the optimistic tier rides the existing
   `candidates_pending_availability` (see
   [Relay Runtime API](#relay-runtime-api-the-noderelay-boundary)).

### PVF Changes

Everything happens inside `validate_block`—no new wasm entry point exists
or is added (the PVF host hardcodes the single `validate_block` call;
Low-Latency v2's scheduling verification likewise runs inside it, fed by a
trailing extension of the validation params). The `validate_block`
wrapper already executes all blocks of a bundle in a loop and reads
pallet storage directly after each block's execution to assemble the
`ValidationResult`
(`cumulus/pallets/parachain-system/src/validate_block/implementation.rs`).

The wrapper also assembles the candidate's UMP signals from those its
blocks emitted. Today these are only scheduling signals (`SelectCore`,
`ApprovedPeer`), with two rules: each may appear once per bundle (a
repeat panics), and signed scheduling info from the collator replaces
them entirely. Messaging signals fit neither rule. Several blocks of a
bundle can send, and the override must not drop them. So the wrapper
builds the messaging signals itself, once per candidate, and appends them
([#12347](https://github.com/paritytech/polkadot-sdk/issues/12347)).
Messaging adds three steps:

1. **Per block, in the existing loop**: take the block's `StreamsRoot`
   digest from its header, if it deposited one (the wrapper has every
   header it executes), and read the block's consumption record,
   collecting each stream's intervals in bundle order (the gap checks need
   every interval—see `stitch`). The wrapper today reads only
   parachain-system's own storage (`UpwardMessages`); reading the messaging
   pallet's record needs a new hook in parachain-system's configuration,
   and it must read only keys the block wrote (`ConsumptionOutbox`), so no
   extra proof nodes are needed.
2. **After the loop**: synthesize the `Requires` entries per source via
   `build_requires_entry` (see
   [Requires Lifting](#requires-lifting-pov-proofs)), verifying lifts
   against the merged record. Emit `Requires` if the bundle consumed
   anything. Emit `Provides` with the bundle's last `StreamsRoot`
   digest if any block deposited one.
3. **Lift transport**: lifts ride in `ParachainBlockData`—the
   collator-supplied, versioned container, following Low-Latency v2's
   exact precedent (its `V2` variant adds `scheduling_proof:
   SchedulingProof` the same way; a further variant adds
   `lifts: Vec<(ParaId, Vec<RequiresLift>)>`)—not in the host-filled
   validation params.

The wrapper cannot judge staleness (it has no relay state; window
matching stays relay-side at inclusion). Its rule is mechanical: one lift
per recorded stream, verified, roots converging per source—anything else
is invalid. Which root the lifts target is submitter policy, as in the
resubmission flow generally.

### Parachain Runtime Changes 

1. **Accumulator maintenance**: Append messages to the per-stream MMR
   frontiers, fold the touched streams' roots into the stream commitment
   tree, deposit the `StreamsRoot` header digest (the wrapper emits
   `Provides`)
2. **Consumption record**: Write the per-block record—touched streams
   with their intervals—to the transient outbox behind
   `consumption_record()`; the runtime never emits a `Requires` signal
   and never sees a `StreamsRoot` (see
   [Requires Lifting](#requires-lifting-pov-proofs))
3. **Message processing**: Consume incoming payloads via the messaging inherent,
   appending to the per-stream frontier
4. **Channel state machine and flow control**: The
   `accept_open_channel` extrinsic (fees + optional local deposit for the
   accepted channel's permanent state), per-channel state
   (`OutChannels`/`InChannels`), lifecycle signal emission/consumption,
   register publishing on the ack stream and register reads via the
   inherent, the `send`-side in-flight gate, one delivery queue per
   inbound channel—see
   [Channels and Flow Control](#channels-and-flow-control)

Note: this document deviates from the initial sketches in
[#12346](https://github.com/paritytech/polkadot-sdk/issues/12346) /
[#12350](https://github.com/paritytech/polkadot-sdk/issues/12350) and the
primitives PR ([#12368](https://github.com/paritytech/polkadot-sdk/pull/12368),
open at the time of writing) in three places:
- `OutboundMessages` is a per-stream vec rather than a
  `(ParaId, position)` map (invalid states unrepresentable, host-side append)
- the off-chain response carries `frontier + payloads` rather than per-message
  `(destination, position, payload)`—positions are derived, not stored
- the leaf preimage is reduced to `LEAF_TAG ++ LEAF_VERSION ++ payload`;
  `source`/`destination`/`position`/length carry no arguable attack in this
  architecture (see Leaf Hashing) and were dropped

The issues and the primitives crate should be updated accordingly.

### Collator Changes

All against the runtime API defined in
[Runtime API](#runtime-api-the-noderuntime-boundary)—no direct storage
reads.

1. **Cross-chain message fetching**: Obtain messages from peer chains
2. **Lift generation**: Create extension proofs for requires lifts. This is
   necessarily node-side: the runtime only stores frontiers (peaks), which is
   enough to *verify* but not to *generate* an ancestry proof. The node
   requests it from the sender's collators: a proofs-only
   `MessagesRequest { start: Some(endpoint), max_bytes: 0, .. }` (see
   [Liftability](#liftability)), for every stream kind—for channels this
   also covers an unconsumed tail beyond what the receiver fetched.
3. **Extended acknowledgement rules**: Verify message dependencies before acknowledging
4. **Super-block production** (if applicable): Coordinate multi-chain block production

### Networking

The request/response fetch protocol is part of the wire specification—see
[Fetch Protocol](#fetch-protocol). What remains here is node-side
infrastructure:

1. **Archives**: sender-side full nodes maintain an off-chain archive of
   each stream's leaves and MMR nodes, built while following their own
   chain (`OutboundMessages` only holds the current block); this is what
   serves fetch requests, under any root of the node's own chain. It is
   kept per sender fork, and its layout is implementation-defined. What
   it must retain is the serving obligation (see
   [Liftability](#liftability)).
2. **Live propagation**: pull on announcement. Receiver collators at the
   speculative tier already follow the sender's headers (off-chain
   verification)—a header announcement *is* the new-root signal, and the
   fetch follows immediately. No push mechanism, no separate notification
   protocol; the hot path costs one request round trip past the header
   announcement.
3. **Acknowledgement propagation**: quick distribution of acknowledgement
   signatures (Low-Latency v2).
4. **Reads and lift material**: everything—channel consumption, register
   and event reads, range reads, gap leaves for skip proofs, lift
   material—is the one `MessagesRequest` (see the table in
   [Fetch Protocol](#fetch-protocol)), `under` per the reader's tier.

### Conformance

Everything on this list must be **bit-identical across every
implementation**—a divergence is not a bug but a consensus split:

- the canonical `StreamId` encoding (trie key derivation);
- tree leaf/inner node byte formats and domain tags, and the canonical
  `tree_hash` construction (see
  [The Stream Commitment Tree](#the-stream-commitment-tree));
- MMR leaf hashing, domain tags and version byte (see
  [Leaf Hashing](#leaf-hashing-and-domain-separation));
- the count-bound MMR root construction
  (`H(MMR_ROOT_TAG ‖ leaf_count ‖ bag(peaks))`, including the empty
  root—see [Candidate Commitments](#candidate-commitments-verified-by-relay-chain)) and the frontier well-formedness invariants
  (popcount, `MAX_MMR_LEAF_COUNT`—see MmrFrontier);
- extension-proof connecting-node set and order, derived from the two
  leaf counts;
- the strict canonical decodes (`RequiresSet`, lift transport,
  `StreamId`: sorted, unique, no redundant encodings—reject, never
  normalize);
- the proof-type bounds enforced at decode (see
  [Proof Size Bounds](#proof-size-bounds)). The PVF decodes lifts, so
  an implementation accepting a larger proof than another diverges;
- the `stitch`/`build_requires` synthesis semantics running in the PVF
  (see [Requires Lifting](#requires-lifting-pov-proofs)).

A **conformance vector suite** in the primitives therefore has the same
normative standing as the encoding rules themselves: id vectors, tree
vectors (including adversarial step orderings that must *fail*),
extension/advance vectors, boundary vectors (the largest valid proof of
each type, and one item past it, which must fail to decode), and full
record→requires synthesis vectors.
An implementation that passes the suite and still diverges is a spec
bug; the suite grows with every such finding.

**Byte values live in the primitives.** This document fixes the
constructions. The reference primitives
(`cumulus-primitives-spec-messaging`,
[#12676](https://github.com/paritytech/polkadot-sdk/pull/12676)) and
their vector suite fix every concrete value:
- tag bytes;
- peak-bagging order and MMR node numbering;
- the digest engine id (`b"SPMS"`);
- UMP signal variant indices and order;
- wire bounds.

### Off-Chain Verification

Consuming messages *before* the sending block's provides commitment is on
chain (the speculative and optimistic tiers) requires verifying the sending
block itself: header lineage from included state, authorship by a currently
valid collator, and acknowledgement confidence. This is a generic subsystem
(shared with Low-Latency v2 ack verification) and specified separately:
[Off-Chain Parachain Block
Verification](offchain-block-verification-design.md).

Interface points with this document:

- The sender commits its `StreamsRoot` into a **header digest**, deposited
  via `frame_system::deposit_log` at block end by every block that touched
  a stream. It is the single source: the `Provides` signal is built from it
  by the validate_block wrapper (see Candidate Commitments). This lets a
  response be verified against a header alone—no candidate required.

  Unlike headers and ack blobs (chain-opaque, judged by the chain's own wasm
  via [Off-Chain Block
  Verification](offchain-block-verification-design.md)), this check is
  performed by the *foreign node directly*—a pure function of header,
  response and proofs, no state involved. The digest format is therefore **protocol
  standard**, not chain-internal:

  - `DigestItem::Consensus(SPMS_ENGINE_ID, streams_root)`, at most one per
    header (`SPMS_ENGINE_ID = b"SPMS"`)—the commitment being a single hash, the
    digest carries it directly;
  - receiver check: recompute the frontier from the payloads → verify
    extension and tree proof from it → compare the resulting root against
    the digest payload.

  A chain deviating from the format self-excludes: receivers cannot verify
  its digests and it simply gets no speculative delivery.

  Freedom removed by this standardization: participating chains must use the
  standard Substrate header layout (this is the one place a foreign node
  parses a header itself—everywhere else headers stay chain-opaque), and
  multiple messaging pallet instances must aggregate into one commitment
  tree per header. Both constraints gate only the speculative/optimistic
  tiers; inclusion-based messaging rides on UMP signals and is header-format
  agnostic. Hash and tree structure were already protocol-fixed at the
  messaging layer, so no chain-internal choice is overridden.
- Tiers, all served by that digest:

| Tier | Provides root source | Trust |
|---|---|---|
| Speculative | Header digest + off-chain verification stack | Trust domain (acks + slashing) |
| Optimistic (backed) | `CandidateBacked` event (`HeadData` incl. digest) or `candidates_pending_availability` API | Backing validity; availability may time out |
| Inclusion-based | `newest_included_provides` (the ring head; see Relay Runtime API) | Relay chain |

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

A dispute can conclude after finality only if approval checking failed.
The invalid candidate's root then stays in the relay window. Evicting it
would undo nothing, because its dependents are final too. This is a
breach of Polkadot's security assumption, handled system-wide by the
dispute freeze and governance, not by messaging.

### Threat: Invalid Extension Proof

**Attack**: A candidate includes a fabricated lift.

**Mitigation**: An extension/tree proof is not checked against a declared
root—verification *computes and returns* the root from the proof (see
[Requires Lifting](#requires-lifting-pov-proofs)), so there is nothing
for a fabrication to assert against: a wrong proof simply yields a
different root, which then fails the relay's window match. The
stitch/forward-only-extension machinery gives the same guarantee across
gaps and bundles—an advance proof can only move a stream's endpoint
forward, never rewrite or rewind it.

### Threat: Position Forgery (Peak-Height Confusion)

**Attack**: Against a stream root that does *not* bind its leaf count (a
bare bag of peaks), positions are forgeable without breaking a single
hash: a peak is an opaque value, so an inner node of height 1 also
passes as a peak of height 47. Example: a stream holds three leaves,
peaks `[merge(l0,l1), l2]`. The inherent item
`LossyRead { frontier: { leaf_count: 2^47, peaks: [merge(l0,l1)] }, payloads: [l2] }`
appends to a peak set byte-identical to the genuine one, so the bare bag
matches the committed entry and the block is valid—yet the consumer's
highwater jumps to 2^47. Every future publish of the stream lies below
that highwater: one malicious block makes the stream permanently
unreadable (register and broadcast consumption both). The same
confusion, applied to a proof-carried leaf count, subverts extension
proofs, the stitch's gap check and skip-ahead.

**Mitigation**: Three layers, each necessary. (1) Roots bind their leaf
counts (see [Candidate Commitments](#candidate-commitments-verified-by-relay-chain)): the forged endpoint above claims count 2^47 + 1
where the commitment binds 3, so its lift fails—and in general the
count pins the peaks' number and heights, and through them every node
position, making the root a commitment to one exact positioned leaf
sequence. (2) Untrusted peak sets enter only through
`MmrFrontier::from_parts`, which rejects any set whose length does not
match the claimed count (see MmrFrontier). (3) Positions are never
carried in proofs, only derived from root-bound counts (the audit rule,
see [Candidate Commitments](#candidate-commitments-verified-by-relay-chain)). What remains is skipping *forward* to genuine
positions—latest-wins semantics on lossy streams, impossible on
channels (prefix discipline); going *backward* is excluded by the
highwater rule (see [The Messaging Inherent](#the-messaging-inherent)). The same attack class
is documented in the wild—ChainSecurity's
[Herodotus case study](https://www.chainsecurity.com/blog/merkle-mountain-range-mmr-the-case-of-herodotus)
(inner node accepted as a peak) and the
[Hyperbridge post-mortem](https://blog.hyperbridge.network/april-13-post-mortem/)
(MMR verifier accepting proofs for nonexistent leaves in the same
library family).

### Threat: Message Replay/Skip

**Attack**: Receiving chain processes messages out of order, skips
messages, or replays old ones.

**Mitigation**: Layered, not a single tracked-state check. Order and
multiplicity are structural: the runtime hashes payloads into leaves and
appends to a frontier, so any reordering or duplication yields an
endpoint no lift can bind (see
[Verification](#verification)). No-skip is an explicit STF rule; only a
runtime upgrade could deliberately skip (see
[Delivery Contract](#delivery-contract-ordered-and-guaranteed)). Replay
of event-stream payloads and ack registers is blocked by the highwater
rule (see [The Messaging Inherent](#the-messaging-inherent)), and cross-block replay within a
bundle by the interval chain's forward-only stitching (see
[Requires Lifting](#requires-lifting-pov-proofs)). This is internal to
the parachain—the relay chain only ever sees the resulting `Requires`
commitment.

### Threat: Acknowledgement Without Verification

**Attack**: Collator acknowledges a block without verifying message
availability.

**Mitigation**: If the block later fails inclusion due to unmet requires, the
acknowledging collator violated Low-Latency v2 rules and is slashable.

### Threat: Cross-Destination Forgery / Replay

**Attack**: A message sent to chain C is replayed to chain B, or a message is
replayed/reordered within a channel.

**Mitigation**: Structural, not leaf-level. Stream context is bound by the
commitment tree: a message to C lives only under the `Channel{C, d, n}` entry
of A's `StreamsRoot`, and the tree proof a receiver verifies names the
`StreamId` explicitly—the same payload cannot verify under the
`Channel{B, d, n}` entry. Order and multiplicity within a stream are what the
MMR structure encodes—replayed or reordered messages yield a different root
(for event streams, cross-stream replay is equally impossible;
*within*-stream replay of an old event as new is blocked by the highwater
rule, see [The Messaging Inherent](#the-messaging-inherent)). Domain tags
prevent inner nodes or roots from being reinterpreted as leaves, in the MMR
and in the commitment tree alike (see
[Leaf Hashing](#leaf-hashing-and-domain-separation), including what is
deliberately *not* in the preimage and why—the same ambiguity attack and
the same fix, RFC 6962-style tagging, apply to the tree's leaf/inner
nodes).

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
prunes `RecentProvides` entries on offboarding, and chains remove their
`InChannels`, `OutChannels` and `Subscriptions` entries for a peer they
observe being offboarded—the one case where channel entries are removed,
since the peer's MMR they protect no longer exists. The inbound
channels' delivery queues are removed with them. How a runtime observes
offboarding is not specified here. Should stronger
identity binding ever become necessary (e.g. genesis hash in the leaf
preimage), a new leaf format can add it, for new streams (see Leaf
Hashing).

### Threat: False Confirmation

**Attack**: A receiving chain's runtime (untrusted code) publishes a
register whose watermark is ahead of what it actually durably consumed.

**Mitigation**: Self-harm only. The sender prunes its archive up to the
watermark; if the receiver never really consumed that range, the receiver
alone is stuck (it cannot skip, and the data it renounced may no longer be
retrievable). No third party is affected, no wrong message ever verifies—the
frontier/root machinery is untouched by confirmations. Confirming *behind*
(or never) merely grows the sender's archive, bounded by its `MaxInFlight`
gate, and is covered by sender-local retention policy (see
[Archive Pruning](#archive-pruning)).

### Threat: Signal / Register Flooding

**Attack**: A malicious peer runtime floods the channel with protocol
messages to impose consumption cost.

**Mitigation**: Nothing to exploit on either lane. On the data stream
there are two signals, each naming its session: older-session signals are
stale and consumed at negligible cost, and anything the receiver's rules
don't admit is simply not admitted (see Channels: Violations)—it stops
only the flooder's own channel. Against a runtime that ignores its
own in-flight gate, consumption being voluntary is the real bound: the
receiver takes what it wants, excess only bloats the flooder's own archive
and tree. Register publishes on the ack stream cannot force work at
all: the reader takes exactly one leaf (the head) per read, so publishing
a million registers only bloats the publisher's own accumulator and
archive. And consumption is always voluntary: the receiver can stop at any
time, keeping only its frontier. For traffic that is well-formed but not
worth its processing weight—garbage *data* rather than protocol
flooding—upper layers pause the channel's delivery queue, and funding
consumption at all is receiver policy (see Flow Control: Consumption economics and
Pausing).

### Threat: Pruning on a Reverted Confirmation

**Attack**: The sender prunes on a register published in a receiver block
that never becomes canonical; the receiver's canonical history then still
needs the pruned range.

**Mitigation**: Prune only once the register-carrying receiver block is
finalized (see [Archive Pruning](#archive-pruning))—a finalized
confirmation cannot revert, whatever tier it was read at. Blocks being
permanent (candidates transient) makes published registers survive
availability timeouts and resubmission. Freeing in-flight room needs no
finality: the sender may act on a speculatively read register (`up_to`),
because its block then depends on the receiver's block and fails with it.

Super-chain-specific threats (collator-set collusion) moved with the
super-chain sketch: see [Super Chains](super-chains-design.md).

---

## Conclusion

Speculative Messaging replaces HRMP: message data never touches the relay
chain—one 32 B commitment per sender block against a fixed-size root
window—while channels, event streams and future semantics are
parachain-layer conventions delivered at parachain block times. Combined
with Low-Latency Parachains v2, this positions Polkadot to offer user
experiences competitive with monolithic chains while preserving
decentralization, security, and horizontal scalability.

---

## Appendix A: Separation of Concerns

Different layers handle different data:

| Layer | Data | Purpose |
|-------|------|---------|
| **UMP Signals** | `Provides(StreamsRoot)` / `Requires` set of `(ParaId, StreamsRoot)`, both built once per candidate by the PVF wrapper (from the blocks' roots and the consumption record) | Relay chain verification |
| **Relay Chain State** | Ring of the last W `StreamsRoot`s per sender—fixed size, nothing per stream | Matching against included blocks |
| **Stream Commitment Tree** | Keyed trie `StreamId → stream MMR root`, maintained by the sender | One-hash commitment over all streams; inclusion proofs verified parachain-side |
| **Messaging Inherent (block body)** | Incoming payloads plus trust-free positioning hints (the frontier for register/event items)—no StreamsRoots, no proof objects, all for declared consumed streams | Consume messages; verification by recomputation into the consumption record |
| **Requires Lifts (POV)** | MMR extension + tree inclusion proofs | Bind recorded states to current roots where the block could not (partial consumption, resubmission, bundles) |
| **Parachain Runtime** | Per-stream MMR frontiers, commitment tree node cache, per-channel state, per-stream highwaters | Internal bookkeeping, flow control, event consumption |
| **Off-Chain (Collators)** | Actual messages | Message delivery |
| **In-Band Signals** | `Hello { session, version }` / `CloseChannel { session }`—the sender side of the channel lifecycle | Channel lifecycle; never seen by the relay chain |
| **Ack Streams** | The latest `Register { up_to, status }`, lossy latest-wins—the receiver's entire channel voice | Acceptance + version range + watermark + close, out-of-band |
| **Stream Conventions** | `Channel{recipient, domain, num}`, `Ack{..}` (mirroring, recipient swapped), `Broadcast{domain, subdomain, num}`, private kinds | Parachain-layer semantics over relay-invisible ids—extensible without relay changes |

## Appendix B: MMR Extension Proof Details

An MMR extension proof demonstrates that a newer MMR root extends an older
one. The structure is defined at first use (see
[Requires Lifting](#requires-lifting-pov-proofs)). The implementation
builds on `polkadot-ckb-merkle-mountain-range`, an established `no_std`
workspace dependency, for node positions, peak helpers and proof
verification ([PR #12676](https://github.com/paritytech/polkadot-sdk/pull/12676)),
but wraps it rather than using its ready-made ancestry proof: that proof
carries node positions and a size the prover chooses, and its root binds
no leaf count. The wrapper derives every position from the two leaf counts
(the proof carries hashes only) and returns the count-bound `MmrRoot`.
Conceptual verification:

```rust
impl MMRExtensionProof {
    /// Extends the verifier's own MMR state (a tracked frontier, or the
    /// frontier carried by a read) and
    /// RETURNS the new root—derived from the proof, never declared
    /// alongside it. No validity checks on `old`: it is the verifier's
    /// state, correct by construction.
    fn verify(&self, old: &MmrFrontier) -> Result<MmrRoot, Error> {
        // Compute the new root, treating the old peaks as opaque, fixed
        // subtrees and merging in the connecting nodes. Their placement
        // is determined by the pair (old.leaf_count, self.leaf_count):
        // an MMR's shape is a pure function of its leaf count, so both
        // ends fix the connecting positions deterministically—which is
        // why the nodes carry none. By construction the result is the
        // root of an MMR of which the old MMR's leaves are a strict
        // prefix. Fails if the connecting nodes are not well-formed for
        // that placement (wrong count for the pair).
        ensure!(self.leaf_count <= MAX_MMR_LEAF_COUNT, Error::Oversized);
        ensure!(self.leaf_count >= old.leaf_count, Error::Regression);
        // The empty extension (no connecting nodes) is valid only as the
        // identity—self.leaf_count == old.leaf_count—and yields old's own
        // root. Any other node count than the pair implies fails.
        // The returned root binds self.leaf_count (see MmrRoot): a
        // lied count yields a root nothing commits to.
        Ok(MmrRoot::from_bag(
            self.leaf_count,
            bag_peaks(&merge_prefix(
                &old.peaks,
                old.leaf_count,
                self.leaf_count,
                &self.connecting_nodes,
            )?),
        ))
    }
}
```

**Serving from any position in O(log).** Appending to an MMR never
changes existing nodes; it only adds new ones. Two consequences:

- The peaks at any count n at or after a retention boundary B are nodes
  of the full MMR. Each is either a peak at B or a node created after B.
- An extension from n consists of nodes created after n.

So a server that keeps the peaks at B plus every node created since
(about two stored hashes per leaf, leaf hash included) answers any
request with lookups: no checkpoints, any starting position. A server
that kept only the leaves would have to rehash from B on every request,
up to a day of leaves.

## Appendix C: Acknowledgement Rule Summary

| Rule | Description |
|------|-------------|
| Base rules | All rules from Low-Latency v2 |
| Message verification | Don't acknowledge until consumed source blocks are irreversible per [the tier table](#the-tier-table) |
| Cycle handling | A block depends only on blocks complete before it was built; candidate-level cycles (super chains, bundled candidates) form atomic enactment groups |

## Appendix D: Comparison of Messaging Modes

| Mode | Latency | Trust | Use Case |
|------|---------|-------|----------|
| Super-chain (intra-block) | < 1 block | Same collator set | Tightly coupled shards |
| Speculative (acknowledged) | ~1-2 blocks | Trust domain collators | Fast cross-chain DeFi |
| Event streams (lossy, broadcast) | tier-dependent (speculative or inclusion) | per tier | Pub-sub: oracle feeds, notifications—latest-wins, no back-channel |
| Inclusion-based | ~2-3 relay blocks | Relay chain only | Cross-domain, untrusted |
| HRMP (legacy) | ~3+ relay blocks | Relay chain only | Deprecated |
