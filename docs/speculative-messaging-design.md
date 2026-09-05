# Speculative Messaging

## Design Document

| Field | Value |
|-------|-------|
| **Authors** | eskimor |
| **Status** | Ready for review |
| **Version** | 0.5 |
| **Related Designs** | [Low-Latency Parachains v2](low-latency-v2-design.md), [Off-Chain Block Verification](offchain-block-verification-design.md), [Super Chains](super-chains-design.md) |

### Version History

Each entry says what changed and which sections to (re)read; small
changesets can be absorbed from here alone, large ones (like 0.5) mean
re-reading the listed sections.

| Version | Changes |
|---------|---------|
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
/// it grants credit to. Broadcast streams have no addressee, hence no
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
  recipient swapped to the other end: acceptance, watermark, credit and
  close in one latest-wins blob, read out-of-band (see
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

/// Bit width of the canonical 8-byte StreamId encoding; a path cannot
/// have more branches than the key has bits.
const KEY_BITS: u8 = 64;

/// One branch on the path. `split_bit` is the branch's bit position,
/// counted as offset from the key's first (most significant) bit.
struct TreeStep {
    split_bit: u8,
    sibling: Hash,
}

struct TreeInclusionProof {
    /// Leaf to root: split-bit offsets strictly decreasing (deeper
    /// branches split on later bits). Structurally impossible in a
    /// well-formed trie to be otherwise—the verifier rejects any other
    /// ordering as early garbage (uniqueness itself rests on
    /// constraints 3 and 4 above).
    steps: BoundedVec<TreeStep, KEY_BITS>,
}
```

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
            let b = lowest bit offset at which entries' keys differ;
            let (zeros, ones) = entries.split_at(first key with bit b set);
            H(TREE_INNER_TAG ‖ b ‖ tree_hash(zeros) ‖ tree_hash(ones))
        }
    }
}
```

A prover holding all entries extracts the proof for key K by recording,
at each recursion step on K's side, `(b, tree_hash(other side))`—then
reversing into leaf-to-root order. The sender never rebuilds from
scratch, though: it caches the node hashes and updates the k touched
paths per block (see below).

Properties the tree must have, and why a keyed binary trie:

- **Inclusion proofs O(log S)** (S = the sender's stream count): a receiver
  proves "stream X has root R under StreamsRoot T" with ~log₂(S) hashes
  (~300 B at S=100, sibling paths shared when proving several streams of
  one sender at once).
- **Stable under insertion**: adding a stream perturbs only the insert
  path—it does not reposition other leaves (a *positional* Merkle tree over
  a sorted list would; that instability was a correct objection against the
  hierarchical scheme in the 0.3 analysis, and it is an artifact of the
  positional structure, not of hierarchy).
- **Cheap maintenance**: the sender updates k touched leaves per block in
  O(k·log S) hashes; node storage O(S). Trivial at realistic stream
  counts. The stored nodes are a rebuildable cache (the frontiers
  determine the whole tree), kept because incremental path updates need
  the sibling hashes—no invariant lives in them.
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
What we pay, honestly: receivers carry ~300 B of inclusion proofs
per source per consuming block, lifts regain an inclusion
component, consumers poll on the p2p side (one opaque root: a receiver
cannot tell whether its stream moved, so each new root costs one
up-to-dateness request per consumed sender—cheap, the empty response is
a ~300 B verifiable "nothing new", see
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
/// Root of a single message stream's MMR (bagged peaks).
///
/// Newtype over `Hash`: roots flow through every layer (tree leaves, wire
/// responses, extension proofs) alongside block hashes, leaf hashes, peaks
/// and StreamsRoots—confusing them must not typecheck. Leaf and inner-node
/// hashes stay bare `Hash`: they are internal to the accumulator code and
/// already domain-separated cryptographically by the hash tags.
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
/// [Practical Limits](#practical-limits)).
struct RequiresSet(BoundedVec<(ParaId, StreamsRoot), MaxCommitmentEntries>);

enum UMPSignal {
    // ... existing signals ...
    /// Sender side: the root of our stream commitment tree after this
    /// block. One hash, constant size. Emitted only by blocks that touched
    /// at least one stream—an untouched tree has an unchanged root, and
    /// re-emitting it would only push a duplicate into the relay window.
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
    /// event streams, that inclusion proofs verify against it.
    /// Consumption depth is therefore not derivable from requires; don't
    /// build acknowledgement or pruning logic on it—the Register of
    /// [Flow Control](#flow-control) carries the consumption watermark
    /// instead.
    Requires(RequiresSet),
}
```

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

/// Per-stream MMR *frontiers*: only the O(log n) peaks are stored, not
/// the leaves. The root is computed on demand by bagging the peaks.
OutboundFrontier: StorageMap<StreamId, MmrFrontier>,

/// Messages sent in *this block only*, per stream, so the collator can
/// extract them for off-chain delivery. The MMR frontier is the only
/// long-lived sender state.
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

/// Receiver-side: the consumption frontier per consumed channel stream,
/// keyed by the full stream key—a chain may consume streams of any
/// sender. Position (= leaf count) and the root built against (bag the
/// peaks) are both derived from it: incoming leaves are appended and the
/// root recomputed. Event-stream subscribers keep a single highwater
/// position instead (see Event Streams); channel-protocol state lives in
/// OutChannels/InChannels (see Channels and Flow Control).
InboundFrontier: StorageMap<(ParaId, StreamId), MmrFrontier>,
```

The sender additionally maintains its **stream commitment tree** (see
[The Stream Commitment Tree](#the-stream-commitment-tree)): node storage
O(S), updated along the paths of the k streams touched per block—
O(k·log S) hashes—yielding the block's `StreamsRoot`.

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
commitment tree, whose new root is committed as block N's `Provides`—one
tree-path write per touched stream, no mid-block ordering hazard, and
clear + bump are one atomic step. A block that touched no stream folds
nothing and emits no `Provides`. The messages of block N remain available
at block N—collators extract them via the runtime API below, never by
reading storage directly.

```
Block N init:      append block N−1's OutboundMessages to frontiers,
                   clear OutboundMessages            (one atomic step)
Block N execution: sends host-append to OutboundMessages;
                   positions = frontier.leaf_count + i (frontier untouched)
Block N end:       stream roots = bag(frontier + this block's leaves),
                   fold touched roots into tree      (transient, in memory)
                   → emit Provides(new StreamsRoot), deposit header digest
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

/// Everything this chain currently consumes, grouped by source chain:
/// what the inherent provider must fetch, and from which position on.
/// The grouping mirrors the commitment structure—one requires entry, one
/// required StreamsRoot, per source.
///
/// Deliberately unpaginated: this is a node-local call—no PoV, no weight,
/// no consensus limit of any kind. The only bound is wasm memory and call
/// time, i.e. tens of MB at a *million* entries; realistic consumed-stream
/// counts are orders of magnitude below. Pagination would be API surface
/// for a regime that can't occur.
fn consumed_streams() -> BTreeMap<ParaId, Vec<ConsumedStream>>;

/// One consumed stream of the given source, with resume state—id and
/// discipline in one enum, so kind/discipline mismatches are
/// unrepresentable. The full StreamId is reconstructible: `recipient` is
/// always this chain (the uniform addressing rule—you consume what is
/// addressed to you; broadcast has no addressee).
///
/// This is exactly the consumption state the runtime stores, reshaped
/// for fetching: the Channel arm projects `InboundFrontier` (cursor =
/// leaf count), the Broadcast arm the highwater map (cursor = highwater
/// + 1)—storage keeps frontiers because verification needs peaks, the
/// API returns positions because fetching addresses by position.
/// Three deliberate absences: ack registers carry no resume
/// state—which registers to read follows from the open channels (see
/// `out_channels`; head-ness of a read is pinned by the required root,
/// see Flow Control); private kinds cannot appear, since the standard
/// pallet cannot consume a stream whose discipline it doesn't know; and
/// suspended channels are omitted—the omission is how the own collators
/// learn to stop fetching (see Flow Control: Suspension).
///
/// `from` is the fetch cursor: positions >= from are wanted.
enum ConsumedStream {
    /// The source's Channel{us, domain, num}: ordered prefix consumption;
    /// from = the tracked frontier's leaf count.
    Channel   { domain: u8, num: u16, from: MessagePosition },
    /// The source's Broadcast{domain, subdomain, num}: lossy latest-wins;
    /// from = highwater + 1 (typically only the head matters).
    Broadcast { domain: u16, subdomain: u8, num: u32, from: MessagePosition },
}

/// Channel views, both directions—credit and watermark standing, phases,
/// which ack registers to read (outbound) and which are due a publish
/// check (inbound). For authoring decisions and diagnostics.
/// (ChannelId, OutChannelState and InChannelState are defined in Channels
/// and Flow Control, where the channel protocol is specified.)
fn out_channels() -> BTreeMap<ChannelId, OutChannelState>;
fn in_channels() -> BTreeMap<ChannelId, InChannelState>;

/// The block's consumption record (defined in Requires Lifting): touched
/// streams with their intervals. Called by the node for
/// acknowledgement checks (a block's dependencies), lift assembly and
/// diagnostics—and by the validate_block wrapper as a direct in-wasm
/// call, to synthesize the candidate's Requires entries.
fn consumption_record() -> ConsumptionRecord;
```

The API is *complete*: everything the inherent carries is for a stream
listed by `consumed_streams()`—channel acceptance happens via extrinsic
(see [Channels](#channels)), so no unsolicited-data path exists that the
API would have to leave unlisted.

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
struct MessagingInherentData {
    items: Vec<(ParaId, StreamId, ConsumeItem)>,
}

/// One stream's consumption this block. Both arms verify identically—
/// hash payloads into leaves, append to a frontier, let the lift bind
/// the endpoint—differing only in WHERE the frontier comes from
/// (mirroring the two consumption disciplines, see Event Streams).
enum ConsumeItem {
    /// Prefix discipline: the frontier is consensus state
    /// (`InboundFrontier`); payloads are the stream's next messages, in
    /// order. Order and count are self-enforcing—any deviation yields
    /// an endpoint no lift can bind.
    Channel { payloads: Vec<Payload> },
    /// Inclusion discipline (registers, event streams): the frontier
    /// arrives in the item—`start_peaks` is the stream's peak set at
    /// `base`, standing in for the frontier a lossy consumer
    /// deliberately doesn't keep. len ≥ 1; a register or head read is
    /// the len-1 case at the head (head-ness falls out: an empty lift
    /// extension means the endpoint IS the committed entry—see Flow
    /// Control). `base` and `start_peaks` are trust-free hints: the
    /// appended structure is placed by `base`, so a lie in either
    /// yields a root no lift can bind (the MMRExtensionProof
    /// `leaf_count` argument). Replay-guarded by the highwater rule:
    /// require `base > highwater`, consume ascending, bump highwater
    /// to `base + len − 1` (see Event Streams).
    Events {
        base: MessagePosition,
        start_peaks: Vec<Hash>,
        payloads: Vec<Payload>,
    },
}
```

`MmrInclusionProof` thus never crosses the node–runtime boundary—it
remains a wire object (`EventResponse`), where the *node* verifies it.
The runtime's single verification discipline is recomputation, at no
byte cost: an inclusion proof is internally sibling path + peaks
(~2 log n hashes); the recomputation form is peaks + lift extension,
the same two O(log n) components—the bytes merely move from a proof
object in the block body to the candidate's ordinary lift, already
covered by the per-stream reservation.

Dispatch rules:

- **Mandatory, and first in the block** (like all inherents)—consumption
  capacity cannot be crowded out by extrinsics (see
  [Delivery Contract](#delivery-contract-ordered-and-guaranteed)).
- **Absent = consumed nothing.** Empty data produces no inherent call at
  all; a block that fetched nothing carries none, touches no stream, and
  emits no requires. Consuming nothing costs nothing.
- **At most one item per stream and block**—the consumption record's
  one-interval-per-stream shape, enforced at dispatch.
- **Strict on import: one invalid item invalidates the block.** Any
  invalid item (undeclared stream, cap violation, kind/discipline
  mismatch, `base ≤ highwater`) errors the mandatory dispatch—the
  standard inherent pattern (cf. `paras_inherent`): filter when
  *building* (the inherent provider simply doesn't include bad items),
  hard error on *import*. Every check is deterministic against parent
  state and node-verifiable before building, so an honest author never
  fails; and since inherent items pay no fees, tolerance would let a
  collator pad blocks with garbage nobody pays for. Strictness also
  keeps the consumption record trivial: record = all items, no
  rejected-item bookkeeping.
- **Caps**: `MaxTouchedStreams` streams per block (necessarily
  ≤ `MaxCommitmentEntries`, since every touched stream's source becomes
  a synthesized requires entry) and `MaxContextGaps` read-context gaps;
  per-stream volume is bounded by the sender-side
  `MaxMessagesPerBlock`/`MaxMsgLen` (see Flow Control).

**PoV weight reservation—receiving costs a little extra, so lift space
always exists.** Lifts and advance proofs are attached by the submitter
*outside* block execution (see
[Requires Lifting](#requires-lifting-pov-proofs)), so the STF charges
their worst case up front as `proof_size` weight, pre-dispatch:

- per touched stream: the worst-case encoded lift (~4.2 KB ceiling, a
  structural constant—see [Lift Size](#lift-size));
- per read-context gap—an `Events` item pins its context freely and,
  being one contiguous interval, can open at most one gap: the
  worst-case advance proof (~2.1 KB).

The reservation covers only material attached *after* authoring; an
`Events` item's `base`/`start_peaks` hints ride in the block body and
are simply measured as block bytes. Block building then keeps
`block + storage proof ≤ POV limit − reservation` by construction, the
caps bound the total reservation, and a candidate carrying the full
worst-case lift set always fits—enforced by the same mechanism as all
PoV accounting, not by collator discipline.

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

Head only, deliberately: the newest root is all authoring policy ever
targets (see [Verification](#verification)); returning the ring would
harden the window size `W` into API surface, and skipped intermediate
roots cost nothing—there is no obligation to observe every update. An
API rather than a well-known storage key, also deliberately: well-known
keys earn their keep where reads must be *proven* (PVF storage proofs);
here the node queries its own relay client state, and the API keeps the
storage layout private. Polling it on relay block import doubles as the
entire recovery story—the first query after a restart is all the state
a returning collator needs.

Not strictly necessary, and worth being honest why it exists anyway:
the same information is derivable by tracking pending-availability
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
before the sender's `OpenChannel` (pre-authorization, see
[Channels](#channels)) is consumed-but-unwritten—no leaf, no tree
entry—so every fetch honestly answers `UnknownStream`, indistinguishable
from withholding. The window is no consensus concern: blocks record only
*touched* streams, so an armed-but-empty stream creates no requires, no
lift, no relay matching—nothing ever asserts emptiness, and the first
payload that does arrive is verified like any other. The contract is
node-side: while `in_channels` shows a channel accepted with its
`OpenChannel` not yet consumed, `UnknownStream` at cursor 0 is an
*expected* answer—retry later, no peer penalty. Once the stream's
frontier is nonzero the stream provably exists; from then on
`UnknownStream` is misbehavior, and up-to-dateness itself is always
verifiable (a payload-free response's empty extension plus tree proof
proves "nothing new under this root"). A tree *non-inclusion* proof
(neighbor-leaf path showing the absent key's lookup lands elsewhere)
would make even the pending window's emptiness provable—an optional
peer-scoring upgrade, not a correctness requirement. Alternative
considered and rejected: acceptance carrying the `OpenChannel` payload
itself (acceptance as first consumption, through the standard
recomputation-plus-lift path—no new verification machinery). It closes
the window but forces open-before-accept, killing pre-authorization, and
the honest lagging-peer case needs the retry tolerance anyway.

```rust
/// "Give me messages of this stream from `start` on, verifiable under
/// this provides root." One request serves ordered fetching AND pure
/// lift material: with max_bytes = 0 the response is payload-free—
/// exactly the extension + tree proof pair a requires lift carries
/// (empty extension = the tree-proof-only case). Served by the sending
/// chain's full nodes (see Networking). For event streams the *normative*
/// obligation covers payload-free requests from block boundaries within
/// the serving horizon (see Liftability); payload-carrying event ranges
/// (see Event Streams: Range reads) are QoS—servable from any position
/// where payloads are still retained.
struct MessagesRequest {
    /// The requested stream of the serving chain.
    stream: StreamId,
    /// Typically the receiver's frontier leaf count.
    start: MessagePosition,
    /// The StreamsRoot the response must verify under—the requester's
    /// chosen dependency (newest, or newest *included*, per its tier
    /// policy). It can never be handed a dependency on a newer,
    /// possibly unconfirmed root; freshness is its own job: re-request
    /// under a newer root once that root is verified.
    under: StreamsRoot,
    /// Response size bound (the server may cap harder)—fetching stays
    /// chunked and resumable no matter how large the backlog.
    max_bytes: u32,
}

/// Payloads from `base` on, plus the proofs binding them—and everything
/// before them—to the requested root.
struct MessagesResponse {
    /// Position of the first payload. Trust-free hint (a lie only fails
    /// the proofs).
    base: MessagePosition,
    /// Leaf-format version for these payloads—a trust-free hint like
    /// `base`: versions are hash-disjoint domains, so only the correct
    /// one can reproduce a committed root. A response never spans a
    /// version change; the server splits there (versions change only at
    /// sender block boundaries, and rarely).
    leaf_version: u8,
    /// The payloads (XCM or other data), in MMR order. Payload i has
    /// position base + i: gaps are unrepresentable, mirroring the
    /// sender's storage layout.
    payloads: Vec<Vec<u8>>,
    /// The stream's peak set at `base`—always present (≤ 64 hashes,
    /// noise next to the payloads; cheap for any server that can serve
    /// the range at all: roll the retention-boundary frontier forward).
    /// Necessary for consumers holding no frontier (event subscribers
    /// fetching a range—see Event Streams); for channel receivers a
    /// work-saving cross-check: compare against the own frontier before
    /// hashing the payloads. Trust-free either way: fabricated peaks
    /// cannot extend to a committed root, the end-to-end check
    /// authenticates them (leaf count = `base`).
    start_peaks: Vec<Hash>,
    /// From the frontier recomputed over `payloads` to the stream's
    /// entry under `under`; empty when the payloads reach it.
    extension: MMRExtensionProof,
    /// Yields, walked from the extension's output, the StreamsRoot the
    /// response verifies under—compared against the requested root.
    tree_proof: TreeInclusionProof,
}

/// "Give me one event of this stream, with proofs, under this provides
/// root." The lossy consumer's request: subscribers verify by inclusion
/// proof, not recomputation—no frontier needed. Ack-register reads are
/// exactly this request pointed at the peer's Ack stream. Deliberately
/// single-leaf: ranges go through MessagesRequest + start_peaks (see
/// Event Streams: Range reads).
struct EventRequest {
    stream: StreamId,
    /// The StreamsRoot the response must prove against. Always explicit:
    /// the receiver names what it is willing to depend on—a root
    /// verified at its tier, which for the inclusion tier is a window
    /// entry read straight off the relay chain. The receiver can never be handed a dependency on a
    /// newer, possibly unconfirmed root; freshness is its job:
    /// re-request under a newer root once that root is verified.
    under: StreamsRoot,
    /// Specific position, or None for the head as of `under`.
    /// The request is a pure function of (stream, under, at): it either
    /// serves or fails (root unknown, or outside the serving horizon—see
    /// Liftability); nothing is resolved server-side.
    at: Option<MessagePosition>,
}
struct EventResponse {
    /// The event payload; its position is proven by the inclusion path's
    /// shape.
    payload: Vec<u8>,
    /// MMR inclusion proof to the stream's root (mmr_lib's MerkleProof:
    /// sibling path plus the other peaks for root bagging).
    inclusion: MmrInclusionProof,
    /// Places the stream's root (computed from `inclusion`) under
    /// `under`.
    tree_proof: TreeInclusionProof,
}

```
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

1. Hash each payload into its leaf (`H(LEAF_TAG ‖ version ‖ payload)`) and
   append to the tracked frontier for this stream, in order. Order and
   count need no explicit check: appending in any other order or skipping
   a message yields a different root. (This step also runs in the
   runtime, on the inherent's payloads—it is all the runtime ever does.)
2. Node-side: verify `extension` from the recomputed frontier (yielding
   the stream's root under the target), walk `tree_proof` from that
   (yielding a `StreamsRoot`), and compare against the root the request
   named—one the collator had already verified at its tier (relay window
   entry, verified header digest). A mismatch anywhere discards the
   response and the peer; a match authenticates the entire response.

Consecutive extension proofs compose—a holder of a proof to root `t` can
splice on a later `t → t'`—which is how lift material is kept current
past the serving horizon (see [Liftability](#liftability)).

**Root choice and consumption boundaries.** Authoring policy is one rule:
**target the newest `StreamsRoot` available at the chosen tier** (see
Off-Chain Verification for tiers)—the root the collator authenticated
against node-side and will build the lifts toward. Never target an older
root to make a boundary line up: staleness spends the window's pipeline
slack (see [Window Depth](#window-depth)), and the lift machinery is
engaged either way.

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
hasher reconstructs the preimage itself: tag is a constant, version a
trust-free hint, payload the message.

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
  Nothing is left for a position field to prevent.
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

A receiver's block requires the newest `StreamsRoot` observed at
*authoring* time; matching happens against relay state at *inclusion*.
Absorbing that gap is the window's *sole* purpose: W must cover the sender
blocks produced during the candidate pipeline (authoring → backing →
inclusion, ~2–3 relay blocks ≈ 12–18 s in normal operation), including
bursts—an elastic-scaling sender may commit several blocks per relay
block. The window is *not* lag tolerance for slow consumers: authoring
always requires the newest available root and covers consumption lag with
POV lifts instead (see
[Requires Lifting](#requires-lifting-pov-proofs)). Since the
window slides one entry per provides-emitting sender block:

| Sender block time | Sender blocks in 18 s | W = 128 covers |
|---|---|---|
| 6 s | 3 | ~13 min |
| 2 s | 9 | ~4 min |
| 500 ms (Low-Latency v2) | 36 | ~64 s |

At W = 128 the cost is 4 KB per sender—about 800 KB relay-wide at 200
parachains—so W can be chosen generously (governance-adjustable) and the
normal pipeline matches proof-free with ample slack (~3.5× even at 500 ms
blocks, 40×+ at 6 s). W does not price the lookup either: entries name
what was the sender's newest root at authoring, so a newest-first linear
scan hits within the pipeline depth (a handful of entries) regardless of
W—the per-source storage read dominates, and no index structure is
warranted. Outrunning the window is not a failure mode: lifts are
regenerated against the current provides at every (re)submission (see
[Requires Lifting](#requires-lifting-pov-proofs)).

#### Matching Against the Virtually Extended Window

There is only *one* check. Candidates arriving together (live communication)
are not a special case: before checking, the stored windows are **virtually
extended** by the `Provides` roots of all candidates being processed in this
relay chain block **and of all candidates pending availability**. Every
requires entry is then matched against the extended window. On enactment the
transient extensions become permanent (pushed into `RecentRoots`); if a
providing candidate doesn't make it (dropped, or availability times out),
its extensions evaporate with it.

The pending-availability part is not an optimization but the speculative
tier's steady state: a fast sender's newest roots are *always* in flight
between backing and inclusion—a receiver backed one relay block after its
sender would otherwise never match on first submission and churn through
resubmissions exactly on the hot path. The relay already holds these
candidates; the window gains one more transient source, under the same
rule as same-block matches: a match against a pending candidate's provides
is an enactment dependency on that candidate.

```rust
fn verify_requires(
    candidates: &[CandidateReceipt],  // all candidates in this relay block
    pending: &[CandidateReceipt],     // candidates pending availability
    stored: &BTreeMap<ParaId, RecentRoots>,
) -> Result<(), Error> {
    // Transient: stored windows ∪ provides of the candidates at hand
    // ∪ provides of candidates pending availability. Matches against
    // either transient part = enactment dependency on that candidate.
    let window = VirtualWindow::new(stored, candidates, pending);

    for receiver_candidate in candidates {
        for (source, expected_root) in receiver_candidate.requires().iter() {
            // The receiver's own identity plays no role in the lookup—any
            // para may depend on any sender's commitment. Scan newest
            // first: entries name near-newest roots by authoring policy.
            if !window.contains(source, expected_root) {
                // Not stored, not provided alongside - needs a lift in
                // the POV
                return Err(Error::RequiresProof);
            }
        }
    }
    Ok(())
}
```

This is the relay chain's entire involvement in messaging. Note what is
*absent*: no per-stream anything (streams are proven parachain-side, below
the hash), no check that a required source is registered (an absent window
never matches—pure self-harm for the requiring candidate, and the lookup
happens anyway), and no restriction on who may require which source (see
Candidate Commitments).

Mutual dependencies (A requires B's provides and vice versa, the Basti block /
super-chain case) match naturally—both entries are in the virtual extension.
The price is that matches against the virtual part create **enactment
dependencies**: a candidate whose requires matched another candidate's
transient provides can only enact if that candidate enacts. Dependent groups
become available/enacted atomically—all or nothing (see
[Cycle Handling](#cycle-handling)).

One property of the flat scheme is deliberately given up here: under
per-stream relay entries, a quiet stream's stored root never moved, so a
receiver *behind* on it matched proof-free after any lag. Under the single
`StreamsRoot`, a receiver behind on a stream carries a lift in its
candidate's POV (one extension proof + one tree proof, regenerated at
every submission)—about a kilobyte, regardless of how far behind. A receiver
*caught up* on a stream loses nothing: an unchanged stream root remains
that stream's entry under every newer `StreamsRoot`. What is bought: relay
state independent of stream count, and the per-stream
permanence/budget/eviction rules that per-stream entries would force on
the relay chain disappear wholesale.

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
/// reconstructed from an in-block inclusion proof) to a newer root:
/// O(log n) hashes summarizing the appended range.
struct MMRExtensionProof {
    /// Leaf count of the extended MMR—a free variable the verifier
    /// cannot derive (not even from the node count: extending 1 leaf to
    /// 3 and to 4 both take two connecting nodes, placed differently).
    /// Together with the verifier's own leaf count it fixes the MMR
    /// shape completely, which is why the nodes below carry no
    /// positions. Trust-free like every count in the protocol: a lie
    /// yields a root nothing commits to.
    leaf_count: Compact<u64>,
    /// The appended range's summarizing nodes, in the deterministic
    /// order derived from (verifier's leaf count, `leaf_count`).
    connecting_nodes: Vec<Hash>,
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
    /// One proof per gap in the stream's interval chain, in gap order
    /// (see stitch); empty for prefix streams and single-context reads.
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
/// Channel streams cannot gap: consumption is a stored frontier every
/// block continues from, so start == previous end holds by construction
/// and the check is free. Register/event reads are where this bites:
/// each block picks its read context freely (a fresher root mid-bundle
/// is the point), so contexts CAN jump—and without the chain, a block
/// could act on reads against a fabricated context and hide behind a
/// later, genuine one. The fabricated context breaks the chain instead.
///
/// Channels: the frontier's root before / the frontier after the
/// block's incoming messages. Reads: `end` = the context the block's
/// reads were verified against, `start` = its root (nothing advances).
/// `end` is a full frontier because the next gap check, or the lift,
/// extends from it.
struct Interval {
    start: MmrRoot,
    end: MmrFrontier,
}

/// Written per block to a transient outbox (the same storage family that
/// carries `UpwardMessages`) and exposed through a single runtime API,
/// `consumption_record()`. Two callers, one definition: the node reads it
/// via ordinary API dispatch (authoring, acknowledgement checks,
/// diagnostics); the validate_block wrapper calls the API's
/// implementation directly in-wasm after executing each block—the
/// established pattern by which `ValidationResult` is already assembled
/// from pallet storage.
struct ConsumptionRecord {
    /// One interval per stream this block touched. Binding to committed
    /// roots is meaningful only where its output is checked against
    /// relay state, so it lives entirely in the wrapper (lifts).
    ///
    /// Grouped by source, mirroring the lifts' transport and the one
    /// requires entry per source it feeds; per source sorted by the
    /// StreamId's canonical encoding (designed to sort—see StreamId;
    /// `Ord` on StreamId is that order) and unique—the messaging
    /// inherent carries at most one item per stream, the STF rejects
    /// duplicates. This is the API view: the underlying storage
    /// (`ConsumptionOutbox`—distinct from the sender's
    /// `OutboundMessages`) stays a flat, host-append-only vec for the
    /// same O(1)-append reason, grouped and sorted at read time.
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
        if next.start != MmrRoot(bag_peaks(&current.peaks)) {
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
bare tree proof, ~300 B per stream (paths shared per source). Connecting
nodes appear exactly where a state lags its successor or its target: a
mid-backlog boundary, resubmission after the window slid, a read at an
intermediate root of a bundling source, or a fresher read context later
in the bundle. One shape; the proofs' lengths are the only variable.

Two guards share the replay story, doing different jobs: the *chain* is
structural—extension proofs only exist forward, so verified states cannot
regress, and the endpoint lift transitively binds every interval to the
canonical stream history (requires entries only ever match the canonical
relay chain's windows, so all bound roots lie on one history and
positions are globally comparable). The *highwater* is state—it stops
re-acting on old positions even under a legitimately bound older context
(see [Event Streams](#event-streams)).

#### Liftability

A lift must be generatable whenever needed—otherwise a permanent block
could become permanently unincludable. Its two halves have very different
needs. The tree proof needs no history at all: lifts target a *current*
committed `StreamsRoot`, and the commitment tree is runtime state, so any
synced full node of the source serves it live. The extension and advance
proofs are built from the source stream's *leaf hashes* between the
recorded states and the present—payloads are never needed—and their
availability is guaranteed per convention:

- **Channel streams**: already covered by the pruning rule—the sender
  retains everything above the confirmation watermark, and the watermark
  never passes the receiver's boundary. The receiver's own nodes hold the
  range too, having fetched it to consume.
- **Event streams**: no watermark exists, so the obligation is bounded in
  time instead: **the sending chain's collators must be able to serve
  extension proofs from any block boundary of the stream within the last
  25 hours** (a payload-free `MessagesRequest`, see
  [Fetch Protocol](#fetch-protocol)). Block boundaries suffice because
  event reads only ever verify against per-block roots. The 25 hours
  mirror relay-chain availability retention and its operating assumption
  that collators come online at least once a day; one honest collator
  suffices, and the storage is megabytes even for busy feeds. Payloads
  must be served for every event that is the stream's head *under some
  servable root*: any `StreamsRoot` still in the source's relay window
  (measured at the server's best relay head) or newer—so every tier can
  read the feed's value under a root it is willing to depend on. That is
  at most one payload per such root, in practice one or two per stream.
  Payloads no longer reachable as any such head are pure QoS:
  nothing in the protocol ever needs them again (repair goes through
  channels; re-validation reads payloads from the block body).

Three layers back the guarantee, all covering the same ~25 h window: the
receiving chain's collators keep whatever their own not-yet-included
blocks may still need (first line of defense); the sender-side serving
obligation above (the backstop); and, as a last resort, the committed
sender blocks' POVs in relay-chain availability—stateless re-execution
reproduces every send, hashes and payloads alike.

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
`StreamsRoot`: ~log₂(S) `(split bit, hash)` steps (S = the source's
stream count).

- S = 100 streams: ~7 steps ≈ 231 B, call it ~300 B with structure
  overhead, per source per consuming block; several streams of one source
  share upper path segments.

This is the recurring price of the constant-size commitment—it rides in the
block body/POV, not on the relay chain, and was measured as acceptable in
the original flat-vs-hierarchical analysis
([PR #10449](https://github.com/paritytech/polkadot-sdk/pull/10449): a few
hundred bytes per source per block, sub-percent of the POV budget).

#### Lift Size

A lift is one MMR extension proof plus one tree proof per stale
stream: O(log m) + O(log S), m = messages in that stream.

- Typical (1000 messages to us): ~log₂(1000) ≈ 10 hashes ≈ 320 B, plus
  ~300 B tree proof.
- Worst case (24 hours of messages to one receiver, ~10⁹ leaves): ~30 hashes
  ≈ 960 B, plus the tree proof.

Extension proof size is independent of how much the sender wrote to *other*
streams; the tree proof grows only logarithmically with the sender's stream
count.

#### Practical Limits

Lifts are added to the POV by the submitter, outside block execution—so
the block must guarantee at authoring time that the worst case still
fits. The worst case is resubmission, where *every* touched stream needs
a lift: one extension proof (≤ 64 connecting nodes: ~2.1 KB encoded)
plus one tree proof (≤ `KEY_BITS` steps of `(bit, hash)`: ~2.1 KB), so a
hard per-stream ceiling of **~4.2 KB encoded**—plus one advance proof
(≤ ~2.1 KB) per read-context gap in the stream's interval chain, bounded
by the bundle's block count. These ceilings are structural constants
(`KEY_BITS`, the 64-peak MMR bound), independent of the sender's stream
layout—a receiver need not trust the sender to keep its own reservation
correct; realistic occupancy is far lower (~30 connecting nodes at a day
of backlog, ~11 tree steps at 1024 streams: ~2.8 KB). All quantities are
STF-known (touched streams, gaps), and the reservation is charged in the
STF as `proof_size` weight, so a candidate carrying the full worst-case
lift set always fits—the charging rule lives with the inherent, see
[The Messaging Inherent](#the-messaging-inherent).

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

### Channels and Flow Control

The layers so far move bytes and verify them: streams define what was
sent, commitments and proofs let the relay chain enforce that dependencies
hold. This section defines the **standard reliable convention**—the
interpretation of `Channel` streams, and the HRMP replacement proper: what
a message *is* (message kinds), the delivery contract (ordered,
"guaranteed"), how a communication relationship starts and ends (channels),
how the receiver paces the sender and tells it what may be discarded (flow
control and pruning), and the escape hatch when a channel stalls for good
(recovery). A channel is **unidirectional**: one sender data stream
(`Channel { recipient, domain, num }`—ordered data + lifecycle signals) plus the
receiver's mirrored confirmation register (`Ack`). Bidirectional
communication is simply two independent channels, one each way. The lossy
pub-sub counterpart—broadcast streams—is specified in
[Event Streams](#event-streams).

Nothing in this section involves the relay chain: channels are a bilateral
affair between the two parachain runtimes, conducted over the very
transport they govern—lifecycle signals are ordinary messages in the
ordered streams, confirmations live in the ack registers.

**Terminology.** This section's *confirmations* (the `Register`,
watermarks) are flow-control state between two parachains. They are
unrelated to the collator *acknowledgements* of Low-Latency v2 (signatures
committing to a block becoming canonical, see
[Acknowledgement Extensions](#acknowledgement-extensions)). Distinct words,
used consistently: collators *acknowledge blocks*, parachains *confirm
messages*. (The `Ack{..}` stream-id name is a concession to brevity—it
carries confirmations.)

#### Message Kinds

Every leaf payload is a typed message:

```rust
/// The payload of every MMR leaf. SCALE-encoded; this encoding is what the
/// leaf preimage's `payload` field contains—the preimage layout itself
/// (`LEAF_TAG ++ LEAF_VERSION ++ payload`) is untouched, `LEAF_VERSION`
/// governs the preimage framing only and stays opaque to payload contents.
/// No leaf-format version bump is needed for this structure or any future
/// evolution of it.
enum SpecMsgKind {
    /// Protocol-level signalling: the sender-side channel lifecycle
    /// (open/close/upgrade). The receiver's side of the channel—acceptance,
    /// flow control, close—lives entirely out-of-band in its register (see
    /// Flow Control). Emitted and consumed by the messaging pallet itself,
    /// never by applications. Ordinary messages in every respect:
    /// window-counted, ordered, confirmed by the watermark like any other
    /// position.
    Signal(SpecMsgSignal),
    /// Userspace payload. The transport delivers the bytes in order and is
    /// deliberately blind to their meaning—the only distinction it ever
    /// acts on is Signal vs. Data (window accounting, pallet-internal
    /// consumption). Demultiplexing among userspace protocols is an
    /// upper-layer convention; XCM rides here under a well-known envelope
    /// defined by the XCM integration, not by this document. 
    Data(Vec<u8>),
}

/// Sending API exposed by the messaging pallet (domain/num default to 0).
/// Takes bare userspace bytes—the pallet wraps them as `Data`; `Signal`
/// leaves are emitted by the pallet's own lifecycle logic only and are
/// not constructible through this API. Fails when the channel is not open
/// or the send would exceed the granted window (see Flow Control)—no
/// hidden queueing; backpressure surfaces to the caller.
fn send(recipient: ParaId, domain: u8, num: u16, data: Vec<u8>) -> Result<(), SendError>;
```

Design points:

- **One data lane per channel.** Signals and data interleave in the single
  `Channel` stream; ordering across kinds is preserved by construction.
  Applications needing sub-streams multiplex inside `Data`, or open further
  channels (distinct discriminator)—each additional channel replicates the
  full per-channel state (frontier, watermark, window) through every layer,
  so the default is one per peer and direction.
- **Versioning is per channel, not per message.** Ordered consumption cannot
  skip what it cannot parse—a receiver facing an unknown enum variant has no
  sound option (skipping violates the delivery contract, stalling bricks the
  channel). So a receiver must never face one. The mechanism is
  **monotonic version announcements**: each party announces the highest
  protocol version it supports—the sender in-band (`OpenChannel` initially,
  `Upgrade` later), the receiver in its register; the effective version is
  the min of the two latest announcements; and the sender may use a variant
  only after *reading* the register announcement permitting it (the enabling
  announcement is thereby known before every use). Two supporting rules:
  announcements never decrease, and a party never drops parse support for a
  version it announced while the channel is open (parsing old variants is
  cheap to keep; a genuine downgrade is the one case that still requires
  close + reopen). A **frozen core** is parseable at every version: the
  signals `OpenChannel` (variant index 0—it must parse before any
  announcement exists), `CloseChannel` and `Upgrade`, and the `Register`
  format itself (it is the machinery the receiver's announcements ride on);
  only variants beyond the core are version-gated.

#### Delivery Contract: Ordered and "Guaranteed"

**Ordered delivery is not an added mechanism—it falls out of the existing
machinery, in two layers of different strength.** Order and multiplicity
are *structural*: the receiver appends incoming leaves to its frontier for
that stream, and a reordered, duplicated or fabricated sequence yields a
different root—it cannot match the committed one, no matter what the
runtime does (see
[Off-Chain Communication](#off-chain-communication-between-collators)).
Skipping is different: an extension proof *can* advance a frontier to a
committed root without the payloads in between (that is precisely what
skip-ahead recovery does, below). No-skip is therefore an **STF rule**, not
a cryptographic impossibility: the runtime computes leaf hashes from
payloads handed to it by the inherent and offers no payload-free path to
advance the frontier—except the explicitly gated skip-ahead. Message X+1 is
receivable only after X because the only ungated way forward is through X's
payload. The only expressible misbehavior is a *stall*: stopping
consumption entirely.

This yields *guaranteed delivery* in the following sense: a message cannot
be selectively censored or lost while the channel makes progress—either the
whole channel advances past it, or the whole channel stalls at it. Combined
with sender-side archives (one honest collator suffices to serve
retransmits—no availability-system involvement needed), every sent message
remains deliverable for as long as the sender retains it (see
[Archive Pruning](#archive-pruning)).

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
contract.

#### Channels

A channel is **unidirectional**: it consists of the sender's data stream
`Channel { recipient, domain, num }` and the receiver's register stream
`Ack { recipient, domain, num }`—same discriminator, kind flipped,
recipient swapped to the other end. The sender speaks through data and
lifecycle signals; the receiver's *entire* voice in the channel is its
register. Bidirectional communication is two independent channels—each
with its own discriminator, each in its own sender's namespace, no pairing
between them.

```
     One channel = sender's data stream + receiver's register

  A writes:  Channel{recipient: B, d, n}  ── data + signals ──▶  B consumes
  B writes:  Ack{recipient: A, d, n}      ◀── register ────────  A reads

  (One rule throughout: the recipient field names who READS the stream.
   Collision-free in B's own stream set—the kind tag separates the
   register from B's channels, the recipient field from registers for
   other senders' channels.)
```

**Accumulator lifetime ≠ channel lifetime.** The channel's streams and
their frontiers are *eternal*: positions never reset, frontiers are never
discarded (O(log n) ≈ a few hundred bytes per stream, kept forever). A channel
is state layered *on top of* the accumulators—opening, closing and reopening
manipulate that layer only. This single decision deletes an entire problem
class: no channel epochs, no replay-across-reopen analysis, no
stale-frontier-after-reopen failure mode (a receiver that slept through a
close/reopen still holds a frontier that matches), and no need to forbid
closing while messages are unconfirmed—an unconfirmed tail simply remains
deliverable after reopen.

```rust
/// Lifecycle signals—the SENDER side of the channel; in-band, ordered,
/// window-counted messages on the Channel stream. The receiver side
/// (acceptance, credit, watermark, close) lives in its Register—see Flow
/// Control.
enum SpecMsgSignal {
    /// Request/announce a channel. Variant index 0, frozen across all
    /// versions. `version` is the sender's initial version announcement
    /// (highest supported).
    OpenChannel { version: u8 },
    /// Half-close: the sender sends nothing further after this leaf (until
    /// a later reopen).
    CloseChannel,
    /// Raise the sender's version announcement mid-channel (monotonic;
    /// lower-than-current values are invalid). New variants are usable
    /// only after reading the register announcement permitting them—see
    /// Message Kinds. Genuine downgrades require close + reopen.
    Upgrade { version: u8 },
}

/// Receiver-granted credit (carried in the Register)—**advice, not
/// enforcement**. Registers are lossy and read with delay, so a sender
/// may legitimately act on an older grant; and on an ordered stream the
/// receiver cannot reject what arrives without stalling the channel.
/// Honoring the grant is the sender's own interest: beyond it, the
/// receiver may be unable or unwilling to process, and its recourses
/// (deprioritize, stall, abandon) hurt the sender. The sender's own STF
/// turns the advice into its local gate (see Flow Control).
///
/// Both limits apply simultaneously, mirroring weight's two dimensions:
/// message count bounds per-item processing, bytes the per-block
/// weight/POV budget.
/// `max_message_size` mirrors HRMP's—likewise advice; the *hard* size
/// bound is the consensus constant `MaxMsgLen`, enforced in the sender's
/// STF, which is what guarantees any message is processable at all.
struct WindowGrant {
    max_messages: u32,
    max_bytes: u64,
    max_message_size: u32,
}
```

**Opening.** A opens by sending `OpenChannel` on the data stream—at
whatever the current position is (0 on first contact, the resume position
on reopen). The `OpenChannel` leaf is the one message sendable without
credit—necessarily, since no register exists yet. Everything after it is
gated by the flow-control machinery itself: until A has *read* a register
for this channel (see Flow Control), it holds no credit and may send
nothing further. Toward a dead or unwilling peer, the sender's total
exposure is one tiny leaf in its own archive and tree.

**Acceptance = an extrinsic on B.** There is no `AcceptChannel` signal
and no push path: an `accept_open_channel(sender, domain, num, ...)`
extrinsic on B—the analog of `hrmp_accept_open_channel`, but entirely on
B, priced by B: transaction fees plus (per B's policy) a deposit for the
permanent state acceptance creates (the `Ack { A, d, n }` stream and its
frontier, the `InChannels` entry). Anyone with funds on B can submit
it—typically the party wanting the channel; whether an unprivileged
origin suffices is B's policy. On execution, B adds the stream to its
consumed set (it now appears in `consumed_streams()`, fetched like any
other stream) and publishes the initial register—the *sender-visible*
acceptance: A's collators, holding the channel in Opening phase, poll
`Ack { A, d, n }`'s head and find it. Rejection = never executing the
extrinsic, which costs B nothing. Crossing opens need no special case:
two chains opening toward each other have created two channels.

Both orders work: extrinsic first is pre-authorization (stream consumed
but empty until A opens); open first leaves A's `OpenChannel` leaf
sitting in A's stream until acceptance. Either way, everything entering
B's inherent is for a stream B's runtime declared it consumes—how A's
wish to open gets communicated is outside the transport.

**Unwanted opens cost the receiver nothing.** B tracks no state—consensus
or node-side—for channels it never accepted: an ignored `OpenChannel`
occupies *A's* archive and tree only, and no collator queue or inherent
slot ever carries it. No spam surface anywhere, and, unlike HRMP, no
relay-chain deposits or governance in the loop—acceptance cost is B's
local, self-priced concern.

**Closing.** From the sender: `CloseChannel` in-band—it stops sending; the
receiver drains what it wants, publishes its final watermark, and may drop
channel bookkeeping. From the receiver: a register with `closed` set (its
watermark tells the sender what was consumed). Close is *advisory resource
release* either way—the relay chain has no per-channel state to begin
with—and because frontiers survive, it is safe at any time: no
all-confirmed precondition, no draining protocol. Neither side even needs
the polite form: a receiver can simply stop consuming and drop channel
state (*abandonment*), a sender facing a never-publishing receiver stops at
the window by construction and can do the same—the frontier is all either
must keep.

**Reopening.** `OpenChannel` again, at the current position; frontiers
resume seamlessly. A receiver that closed or abandoned re-accepts the
same way it accepted—the acceptance extrinsic, charging whatever its
policy charges (a receiver that merely half-closed *its own* sending has
nothing to redo). The one obligation reopening places on the *receiver*:
**retain the consumption frontier after close**—without it, nothing can
be verified against the stream's eternal MMR (the sender's side is
consensus state and persists by construction). Even that loss is
recoverable unilaterally: the receiver adopts the stream's *current*
state—claimed peaks whose bag is the stream's entry under a committed
root are genuine, the `start_peaks` argument—at the cost of skipping
every unconsumed message. That is exactly the
[Stall Recovery](#stall-recovery) breach, behind the same receiver-side
governance gate; the sender is not involved at all.

Channel state (alongside `OutboundFrontier` / `InboundFrontier`, which
stay exactly as defined)—note the two sides keep *different* state, as befits a
unidirectional protocol:

```rust
/// Channel discriminator; `peer` is the other end of the channel—the
/// recipient for outbound channels, the channel's sender for inbound
/// ones. Also keys the channel views of the Runtime API.
struct ChannelId { peer: ParaId, domain: u8, num: u16 }

/// Sender side, per outbound channel.
OutChannels: StorageMap<ChannelId, OutChannelState>,

struct OutChannelState {
    /// The one channel-phase bit not derivable from `register`: whether
    /// WE sent CloseChannel (the peer's close arrives in the register).
    /// The phases are views: Opening = register is None; Open = Some,
    /// neither side closed; Closed = closed_by_us or register.closed.
    closed_by_us: bool,
    /// Our latest in-band version announcement.
    announced_version: u8,
    /// Latest register read: peer's watermark for our stream (determines
    /// remaining credit together with locally known sizes of messages
    /// beyond it), credit, peer's version announcement, closed flag.
    register: Option<Register>,
}

/// Receiver side, per inbound channel—the id mirrors our Ack stream's
/// fields (peer = the channel's sender).
InChannels: StorageMap<ChannelId, InChannelState>,

struct InChannelState {
    /// The register we last published on our Ack stream—our entire channel
    /// state as far as the peer is concerned; also decides when the next
    /// publish is due.
    published: Register,
    /// Sender's latest in-band version announcement (from consumed
    /// OpenChannel/Upgrade signals).
    peer_version: u8,
    /// Upper-layer consumption switch (see Flow Control: Suspension).
    /// While set: the STF refuses this channel's messages,
    /// `consumed_streams()` omits the stream, and published registers
    /// grant zero. Pause, not close—all state persists.
    suspended: bool,
}
```

#### Flow Control

Two needs, one register. Both flow from receiver to sender:

1. **Rate limiting**: the sender caps its unconfirmed tail anyway, by
   local policy, to protect its own collators' archives—but bytes are the
   one dimension it can reason about. What a backlog *means in time*
   depends entirely on the receiver's resources (block cadence, weight
   budget—it might run on an on-demand core), which only the receiver
   knows. The grant supplies exactly that missing information: the
   receiver's throughput, translated into numbers the sender's `send`
   gate can act on. Information, not enforcement.
2. **Pruning watermark**: the sender learns which messages are consumed and
   need no longer be retained for retransmission.

Both are served by the **register**—the single object the channel's
receiver publishes on its `Ack` stream, and the receiver's entire voice in
the channel:

```rust
/// The complete receiver-side state of one channel, published as a leaf on
/// the Ack stream. Only the LATEST leaf matters (the stream is consumed
/// lossily, latest-wins); each publish supersedes all earlier ones. Its
/// very existence is the sender-visible channel acceptance (produced by
/// the receiver's accept extrinsic—see Channels). `up_to` and `version` are
/// monotonic—a register that regresses either is a protocol violation:
/// the sender ignores the regressed leaf and keeps its previous read
/// (monotonic fields make a stale read harmless), and may treat it as
/// grounds for close or abandonment.
/// `grant` is NOT: the receiver may shrink it at any time (shrinking only
/// gates new sends, see Window accounting).
struct Register {
    /// Receiver's version announcement (see Message Kinds).
    version: u8,
    /// Cumulative watermark: all positions < up_to of the channel's data
    /// stream (messages AND lifecycle signals—it is a stream position)
    /// have been consumed.
    up_to: MessagePosition,
    /// Absolute credit for messages beyond `up_to`.
    grant: WindowGrant,
    /// Receiver-side close (see Closing). A closed register's grant is
    /// void; `up_to` still reports what was consumed.
    closed: bool,
}
```

This is a classic credit window (TCP-style), and ordered delivery is what
makes it one `u64` + one grant: cumulative, idempotent, latest supersedes.
Losing or delaying a register read is harmless—the next read catches up.

**Why out-of-band?** The register is the canonical latest-wins object, and
carrying it *inside* the ordered stream—the design's earlier shape—forced a
cluster of special cases that all vanish with the move:

- *Window exemption*: in-band confirms had to be exempt from the credit
  window, else deadlock (both directions full → the unlock signal itself
  cannot enter the stream). Out-of-band there is no gate to exempt from;
  lifecycle signals, no longer needing exemption cover, become ordinary
  window-counted messages.
- *Signal-rate caps*: exemption needed its own bound against flooding by
  the peer's (untrusted) runtime. An ack stream cannot be used to force
  consumption work—the reader takes one leaf, the head; flooding only
  bloats the flooder's own accumulator.
- *Confirm-of-confirm regress*: an in-band confirm is itself a message
  needing (careful non-)confirmation. A lossy register is never confirmed,
  by construction—nothing to terminate.
- *The coupling caveat*: in-band confirms queued behind bulk payload were
  learned only after consuming the bulk, entangling one direction's credit
  with the other direction's congestion. The register is readable at any
  time, at proof cost O(log n), regardless of what stands in any queue.

The price, honestly: reading the register costs an inclusion proof
(~20–30 hashes) where in-band consumption was proof-free—the standard
lossy-vs-ordered trade (see [Event Streams](#event-streams)), spent here on
a few hundred bytes per channel per consuming block.

**Window accounting.** Everything on the data stream counts against the
window—`Data` and `Signal` alike (signals are few and tiny; special-casing
them buys nothing anymore). A sender's in-flight amount is the count and
byte sum of its messages at positions ≥ the peer's watermark; sending
requires in-flight < grant (both limits). Shrinking a grant never
invalidates already-sent messages—it only gates new sends.

**Publishing (receiver side).** The first publish follows the acceptance
extrinsic (it is what the sender sees of it) and
carries the initial credit; thereafter, republish when consumption
progressed enough to matter: a fraction of the granted window (e.g. ¼—the
delayed-ACK analog) or an age threshold, whichever first. Each publish is
one small leaf on the Ack stream; publishing every block is sound and
cheap, just usually pointless. The Ack stream's archive retention is
trivial: only the head is ever served.

**Reading (sender side).** The collator fetches the peer's latest register
via `EventRequest { at: None, under }` and authenticates the response
node-side, like everything it feeds the inherent; head-ness comes with
that: `under` fixes the ack stream's leaf count, the head is the leaf at
count − 1—an old leaf cannot be served as the head under that root. The
inherent then carries the register leaf as an `Events` item (len 1 at
the head, `start_peaks` = the peaks at its position; see
[The Messaging Inherent](#the-messaging-inherent)); the runtime
recomputes the endpoint from it and, as always, sees no
`StreamsRoot`—the binding to the committed root is the candidate's
ordinary lift for the ack stream, whose empty extension is the
head-ness check. (Reads keep no position state; the register's own monotonic
fields order successive reads.) What *kind* of read it is follows
entirely from **which root the collator reads under and targets with the
lift** (a per-source policy, not a protocol mode):

- **Newest observed `StreamsRoot`** (from the peer's header digest,
  off-chain): speculative-tier freshness. If that root is not yet
  included, the synthesized requires entry matches the virtual window and
  carries an enactment dependency on the peer's candidate—exactly as for
  data consumption, and shared with it: a chain also consuming the peer's
  data gets the *same* entry, so the register read is free at the
  commitment level.
- **Newest *included* `StreamsRoot`** (a window entry read off the relay
  chain): matches *stored* window state, so it creates **no enactment
  dependency whatsoever**—and the read is inclusion-tier *by definition*,
  i.e. pruning-safe with no further condition. A pure unidirectional
  sender reading this way risks nothing on the peer's liveness; its total
  commitment overhead is one ~36 B requires entry.

Flow control tolerates the staleness of the second option easily (window
sizing must absorb round-trip latency anyway), which makes it the sensible
default for register-only reads; the first option exists for chains that
want credit at speculative freshness and accept the coupling. The same
policy generalizes to data consumption: read under the newest root *at
the tier you need*—included roots buy dependency-freedom, speculative
roots buy latency (within a tier, newest is always right—see the
root-choice rule under [Verification](#verification)).

**Enforcement is cooperative, and that suffices.** The sender's own STF
refuses `send` beyond credit—this protects the sender's archive and
surfaces backpressure to its applications, which is who rate limiting is
*for*. The receiver, by contrast, enforces nothing, because it cannot:
registers are read with delay, so it cannot even reliably distinguish an
overrun from compliance with an older grant it itself published—and
rejecting an arrived message on an ordered stream would stall the
channel. It doesn't need to: consumption is voluntary, and its responses
to a sender it dislikes are the same whether or not a grant was
technically exceeded—consume anyway, deprioritize, or abandon. A peer can
always stall the sender by simply not publishing—receiver-side stalling
is inherent to any credit scheme and adds no new threat. Nothing here
needs relay-chain enforcement.

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

**Suspension—the upper layers' off switch.** A channel whose traffic
stops paying its way (by whatever economics the chain chose) must be
stoppable *by the receiver's own machinery*, not merely signaled to the
sender. The pallet exposes suspend/resume per inbound channel to upper
layers; suspension does three things at once:

- **Gates the STF**: a suspended channel's messages are not consumable—an
  inherent carrying them is invalid. (The one refinement to "the receiver
  enforces nothing": suspension binds the receiver's *own* blocks, never
  the sender's.)
- **Instructs the own collators**: suspended channels drop out of
  `consumed_streams()`, so the inherent provider stops fetching—the same
  state that drives fetching drives stopping.
- **Informs the sender**: the next register publish carries a zero grant
  (shrinking is always allowed); an honest sender stops at the window.

Suspension is pause, not close: frontier, register and channel state all
persist, resume is simply republishing a real grant—no reopen protocol,
no sender involvement.

**Trust tiers.** Credit updates may act on speculatively read registers—
the worst case of a register that never lands is wrongly generous or
wrongly stingy throttling, both recoverable. **Pruning may not**: prune
only once the register-carrying block is irreversible per
[the tier table](#the-tier-table); see next.

#### Archive Pruning

The sender-side (destination, position) → payload archive is *node-side
disk*, not consensus state—the runtime keeps only frontiers (see
[Networking](#networking)). Pruning policy therefore binds no one but the
sender's own nodes. The protocol's job is only to define when pruning is
*safe*:

> Prune payloads (and leaf hashes) below watermark W once the receiver
> block that published the register asserting W is irreversible from the
> sender's perspective, per [the tier table](#the-tier-table)—the same
> rule that governs consumption trust. (Cross-domain, requiring an
> included `StreamsRoot` for the register read yields this by definition.)

This adds **no new trust assumption**: a sender that consumes speculatively
from a peer already trusts that peer's acknowledged blocks won't revert
(backed by Low-Latency v2 slashing); pruning on the same tier is the same
bet. The block/candidate split makes the rule robust—blocks are permanent,
candidates transient, so a published register survives availability
timeouts and resubmission unchanged.

Pruning keeps the MMR serviceable: retain the frontier (peaks) at the
pruning boundary plus everything above it—that is exactly what appending
and proof generation over the unpruned tail require; nothing below the
boundary is ever needed again (the receiver confirmed it and can never
re-request below its own watermark).

**Unsafe pruning is policy, not protocol.** A receiver that stops publishing
forever (dead chain, abandoned channel) leaves the sender's archive growing
with every further send—but sending to a non-confirming peer stops at the
window anyway, so the exposure is bounded by in-flight + unconfirmed tail.
Whether to hold that forever, apply a generous age-based cutoff, or wait for
governance is each sender chain's local decision: it trades its own disk
against breaking guaranteed delivery for a peer that was dead anyway. No
protocol change, no coordination—the watermark defines *safe*; everything
beyond is the sender's risk appetite.

#### Stall Recovery

The delivery contract's failure mode: an unconfirmed range is lost from
every archive (all sender-chain nodes pruned unsafely or vanished), the
receiver cannot skip, the channel is stalled permanently. Recovery is a
**skip-ahead**, reusing existing machinery rather than adding a proof type:

The receiver advances its frontier for the stalled stream from its current state to some
newer committed root *without executing the skipped payloads*, by verifying
an MMR extension proof (the lift's proof structure, Appendix B)—whose
verification, applied to the runtime's own frontier, yields the new peak
set, i.e. the new frontier directly. Required inputs are the
connecting node *hashes* only—served via MMR state sharing (see
[Networking](#networking))—never the lost payloads. The runtime emits an
event naming the skipped range so applications can react (a skipped range is
application-visible data loss, the one and only breach of the delivery
contract, undertaken deliberately).

Because this deliberately breaks guaranteed delivery, it is gated: by
governance origin by default; chains that prefer automation can configure a
per-channel age policy (skip ranges older than T with no delivery progress).
Default off—stalls should be loud.

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

#### Consumption: Inclusion Proofs, Not Prefix

A subscriber does not track a frontier and does not consume leaves 0..N—it
wants the *newest* leaves, typically just the head. Verification is by
**MMR inclusion proof**: payload plus the O(log n) sibling path to the
event stream's root, plus the tree proof placing that root under a
`StreamsRoot` the subscriber requires—a speculative one (enactment
dependency, freshest) or an already-included one (dependency-free,
inclusion tier), the same root-choice policy as everywhere else. The two
disciplines, side by side:

| | Prefix (channels) | Inclusion (event streams) |
|---|---|---|
| Verifies | everything up to a point | one leaf, any position |
| Proof bytes | none—recomputation is the check | O(log n) hashes per leaf |
| Receiver state | frontier (peaks + count) | one highwater position |
| Must consume | all of it, in order | only what it cares about |
| Gaps | unrepresentable | the normal case |

The inclusion proof is the price of lossiness: channels get proof-free
appends *because* they take everything in order. Consuming *every* event by
inclusion proof would be O(n log n) versus prefix's O(n) with zero proof
bytes—"I actually want all of them" is a channel workload wearing a
costume, and belongs there.

**One stream per feed.** A publisher runs one broadcast stream per logical
feed (per asset, per market, ...)—ids are abundant (2⁵⁶ broadcast space),
and "keep the newest" is then trivial: the current value of a feed *is* the
head of its stream, exactly as with ack registers. The spec defines no
sub-stream topic scheme; an application that wants topics multiplexed
inside one stream can build them in its payloads, on its own.

**Replay protection without gap state**: the subscriber keeps one
**monotonic highwater position** per consumed stream—accept an event only
if its position (proven by the path shape) exceeds the highwater. Latest-
wins needs exactly this and nothing more: an old event can never be
replayed as the latest; missed events are simply *never looked at*—nothing
in subscriber state refers to them. The highwater is absolute: no backfill
through the event path (that would resurrect which-positions-did-I-process
tracking, exactly what latest-wins deleted). Repair goes through channels
(below).

**Range reads.** A subscriber may also fetch *all* available events above
its highwater in one go: a `MessagesRequest` from `highwater + 1`, with
the response's `start_peaks` standing in for the frontier a subscriber
deliberately doesn't keep—verified by recomputation like a channel fetch,
consumed ascending, highwater bumped to the last position. This is
strictly above-highwater, so the no-backfill rule is untouched, and
availability is the sender's payload-retention QoS ("as many as still
available" is the contract). Head-only reads stay `EventRequest`;
wanting *every* event, forever, stays a channel workload. Should range
reads prove needless in practice, they reduce away cleanly: event
consumption becomes single-read only, `start_peaks` remains as the
channel receivers' cross-check—nothing else depends on it.

#### No Channel, by Construction

Every piece of channel machinery exists to serve guaranteed delivery, and
lossy renounces the guarantee—so each piece vanishes rather than being
configured away:

- No delivery obligation → no retention obligation → **no confirmations**
  → no back-channel. Pure listeners never need any reverse path.
- No retention pressure → **no flow control**: the sender's event archive
  is prunable freely; the retention window (e.g. 24 h) is a QoS knob, local
  to the sender. A subscriber offline longer misses events—that is the
  contract, not a failure. (Only payloads *behind the head* are a knob:
  the windowed heads' payloads and 25 h of extension-proof material are
  mandatory—see [Liftability](#liftability).)
- **No handshake**: subscribing is unilateral and invisible to the
  sender—start consuming (and, for the speculative tier, requiring), done.
  Enabled precisely by the relay's indifference to who requires a source.

The channel protocol's **ack streams** (see [Flow Control](#flow-control))
are exactly this convention with a fixed payload: a lossy stream whose feed
is the confirmation register, latest = the head leaf, retention = keep the
head. One consumption discipline, two uses.

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

When two chains exchange messages speculatively in the same production
cycle, acknowledgements risk deadlock: each side's collators wait for the
other block to be sufficiently confirmed before acknowledging their own.

The invariant that prevents it: **a block may only depend on blocks that
were complete before it was built.** The acknowledgement timing above
enforces exactly this—messages from block `A` are consumed only once the
entire block `A` has been seen (t=1). Applied by both sides, block `A`
cannot depend on the concurrently produced block `B`, because `B` did not
exist when `A` was authored. Dependencies therefore always point strictly
backward in time; no cycle between blocks can form. The argument is
pairwise-free and holds for arbitrary multi-party communication.

Bundled candidates ("Basti blocks") and super chains do produce cycles *at
the candidate level* (mutual requires between co-arriving candidates).
Those are handled, not forbidden: they match through the virtual window and
form atomic enactment-dependency groups—available and enacted all or
nothing (see
[Matching Against the Virtually Extended Window](#matching-against-the-virtually-extended-window)).

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

Three mechanisms consume it, and no other trust rule exists anywhere:

- **Acknowledgement rules**: a collator acknowledges a block only once the
  source blocks it consumed from are irreversible (see
  [Acknowledgement Extensions](#acknowledgement-extensions));
- **Pruning safety**: prune below a watermark only once the
  register-carrying block is irreversible (see
  [Archive Pruning](#archive-pruning));
- **Root choice at authoring**: requiring speculative roots buys latency
  at domain trust, requiring included roots is dependency-free—the same
  policy for data and register reads (see [Flow Control](#flow-control)).

### Establishing Trust

The per-source tier is parachain runtime configuration—the chain's own
storage, updatable by its governance, never relay-visible. A "trust
domain" is nothing more than a set of chains that assign each other the
speculative tier; no registry of domains exists anywhere.

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
  arrive together—no lift needed

### Mitigation Strategies

#### 1. Domain Size Limits

Limit trust domains to a reasonable size (e.g., 5-10 chains). This bounds the
"blast radius" of cascading delays.

#### 2. Resubmission

If Chain A is censored long enough that Chain B's availability times out, Chain
B simply resubmits. Since both chains are likely resubmitting around the same
time, they'll typically be included together without needing lifts,
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

1. **New UMP signals**: `Provides(StreamsRoot)` (block-emitted) /
   `Requires(RequiresSet)` (PVF-synthesized, never block-emitted—see
   [Requires Lifting](#requires-lifting-pov-proofs))
   ([#12347](https://github.com/paritytech/polkadot-sdk/issues/12347)).
   Reusing UMP signals avoids a candidate receipt format change. Rollout
   caveat: older validators reject unknown UMP signals, so senders may
   only start emitting them once node-side support is deployed to a
   sufficient share of validators—the binary rollout itself is the gate,
   no `node_features` bit needed.
2. **Per-sender provides storage**: One ring of the last W `StreamsRoot`s
   per sender, pushed on enactment of provides-emitting candidates; pruned
   as a whole on offboarding. Fixed size (W × 32 B per sender), independent
   of stream count—there is nothing else to store or manage.
3. **Commitment matching**: At inclusion time, verify each requires entry
   `(source, expected_root)` against the stored windows *virtually extended*
   by the `Provides` roots of the candidates at hand and of candidates
   pending availability—one unified check; extensions become permanent on
   enactment, and matches against the virtual part create atomic enactment
   dependencies.
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
wrapper already executes all blocks of a bundle in a loop, already reads
pallet storage directly after each block's execution to assemble the
`ValidationResult`, and already gathers, deduplicates and
consistency-checks UMP signals across the bundle
(`cumulus/pallets/parachain-system/src/validate_block/implementation.rs`).
Messaging adds three steps in exactly those places:

1. **Per block, in the existing loop**: call the `consumption_record()`
   API implementation directly in-wasm (the same post-execution read that
   collects `UpwardMessages`), collecting each stream's intervals in
   bundle order (the gap checks need every interval—see `stitch`).
2. **After the loop**: synthesize the `Requires` entries per source via
   `build_requires_entry` (see
   [Requires Lifting](#requires-lifting-pov-proofs)), verifying lifts
   against the merged record, and append the resulting signal at the
   existing signal re-assembly point.
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
   tree, emit `Provides(StreamsRoot)`
2. **Consumption record**: Write the per-block record—touched streams
   with their intervals—to the transient outbox behind
   `consumption_record()`; the runtime never emits a `Requires` signal
   and never sees a `StreamsRoot` (see
   [Requires Lifting](#requires-lifting-pov-proofs))
3. **Trust domain configuration**: Define trusted peers for speculative messaging
4. **Message processing**: Consume incoming payloads via the messaging inherent,
   appending to the per-stream frontier
5. **Channel state machine and flow control**: The
   `accept_open_channel` extrinsic (fees + optional local deposit for the
   accepted channel's permanent state), per-channel state
   (`OutChannels`/`InChannels`), lifecycle signal emission/consumption,
   register publishing on the ack stream and register reads via the
   inherent, `send`-side window enforcement, upper-layer suspend/resume
   per inbound channel—see
   [Channels and Flow Control](#channels-and-flow-control)

Note: this document deviates from the initial sketches in
[#12346](https://github.com/paritytech/polkadot-sdk/issues/12346) /
[#12350](https://github.com/paritytech/polkadot-sdk/issues/12350) and the
primitives PR ([#12368](https://github.com/paritytech/polkadot-sdk/pull/12368),
open at the time of writing) in three places:
- `OutboundMessages` is a per-stream vec rather than a
  `(ParaId, position)` map (invalid states unrepresentable, host-side append)
- the off-chain response carries `base + payloads` rather than per-message
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
   enough to *verify* but not to *generate* an ancestry proof. For channel
   streams the receiver's node has the data anyway—its old frontier plus all
   subsequently fetched messages rebuild the MMR segment; for event
   streams it requests the proof from the sender's collators and keeps it
   current by splicing (payload-free `MessagesRequest`, normative 25 h
   serving horizon—see [Liftability](#liftability)).
3. **Extended acknowledgement rules**: Verify message dependencies before acknowledging
4. **Super-block production** (if applicable): Coordinate multi-chain block production

### Networking

The request/response fetch protocol is part of the wire specification—see
[Fetch Protocol](#fetch-protocol). What remains here is node-side
infrastructure:

1. **Archives**: sender-side full nodes maintain an off-chain
   (stream, position) → message archive, built while following their own
   chain (`OutboundMessages` only holds the current block); this is what
   serves fetch requests. Channel-stream archives are prunable below the
   per-channel confirmation watermark (see
   [Archive Pruning](#archive-pruning)); event-stream payloads *behind
   the head* by local retention policy (see
   [Event Streams](#event-streams)), while the windowed heads' payloads
   and 25 h of extension-proof material are mandatory (see
   [Liftability](#liftability)).
2. **Live propagation**: pull on announcement. Receiver collators at the
   speculative tier already follow the sender's headers (off-chain
   verification)—a header announcement *is* the new-root signal, and the
   fetch follows immediately. No push mechanism, no separate notification
   protocol; the hot path costs one request round trip past the header
   announcement.
3. **Acknowledgement propagation**: quick distribution of acknowledgement
   signatures (Low-Latency v2).
4. **Lift material**: serve payload-free `MessagesRequest`s
   (`max_bytes = 0`, see [Fetch Protocol](#fetch-protocol)) where a
   peer's node-local data doesn't suffice to build lifts itself.
5. **Event subscription**: subscribers fetch on observed head change via
   the fetch protocol's `EventRequest` (proof-carrying reads), pointed at
   broadcast streams (see [Event Streams](#event-streams)). Ack-register
   reads are an `EventRequest { at: None, under }` on the peer's ack
   stream—`under` per the reader's tier (see Flow Control).

### Conformance

Everything on this list must be **bit-identical across every
implementation**—a divergence is not a bug but a consensus split:

- the canonical `StreamId` encoding (trie key derivation);
- tree leaf/inner node byte formats and domain tags, and the canonical
  `tree_hash` construction (see
  [The Stream Commitment Tree](#the-stream-commitment-tree));
- MMR leaf hashing, domain tags and version byte (see
  [Leaf Hashing](#leaf-hashing-and-domain-separation));
- extension-proof connecting-node set and order, derived from the two
  leaf counts;
- the strict canonical decodes (`RequiresSet`, lift transport,
  `StreamId`: sorted, unique, no redundant encodings—reject, never
  normalize);
- the `stitch`/`build_requires` synthesis semantics running in the PVF
  (see [Requires Lifting](#requires-lifting-pov-proofs)).

A **conformance vector suite** in the primitives therefore has the same
normative standing as the encoding rules themselves: id vectors, tree
vectors (including adversarial step orderings that must *fail*),
extension/advance vectors, and full record→requires synthesis vectors.
An implementation that passes the suite and still diverges is a spec
bug; the suite grows with every such finding.

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
  via `frame_system::deposit_log` at block end from the same computation
  that emits the UMP signal. This lets a response be verified against a
  header alone—no candidate required.

  Unlike headers and ack blobs (chain-opaque, judged by the chain's own wasm
  via [Off-Chain Block
  Verification](offchain-block-verification-design.md)), this check is
  performed by the *foreign node directly*—a pure function of header,
  response and proofs, no state involved. The digest format is therefore **protocol
  standard**, not chain-internal:

  - `DigestItem::Consensus(SPMS_ENGINE_ID, streams_root)`, at most one per
    header (engine id value TBD)—the commitment being a single hash, the
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

**Attack**: A candidate includes a fabricated lift.

**Mitigation**: An extension/tree proof is not checked against a declared
root—verification *computes and returns* the root from the proof (see
[Requires Lifting](#requires-lifting-pov-proofs)), so there is nothing
for a fabrication to assert against: a wrong proof simply yields a
different root, which then fails the relay's window match. The
stitch/forward-only-extension machinery gives the same guarantee across
gaps and bundles—an advance proof can only move a stream's endpoint
forward, never rewrite or rewind it.

### Threat: Message Replay/Skip

**Attack**: Receiving chain processes messages out of order, skips
messages, or replays old ones.

**Mitigation**: Layered, not a single tracked-state check. Order and
multiplicity are structural: the runtime hashes payloads into leaves and
appends to a frontier, so any reordering or duplication yields an
endpoint no lift can bind (see
[Verification](#verification)). No-skip is an explicit STF rule, with
the one gated exception being deliberate skip-ahead (see
[Delivery Contract](#delivery-contract-ordered-and-guaranteed)). Replay
of event-stream payloads is blocked by the monotonic highwater rule (see
[Event Streams](#event-streams)), and cross-block replay within a
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
*within*-stream replay of an old event as new is blocked by the monotonic
highwater, position being proven by the inclusion path). Domain tags
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
prunes `RecentProvides` entries on offboarding, and receivers reset their
`InboundFrontier` entries for a source they observe being offboarded. Should stronger
identity binding ever become necessary (e.g. genesis hash in the leaf
preimage), the leaf-format version byte allows adding it.

### Threat: False Confirmation

**Attack**: A receiving chain's runtime (untrusted code) publishes a
register whose watermark is ahead of what it actually durably consumed.

**Mitigation**: Self-harm only. The sender prunes its archive up to the
watermark; if the receiver never really consumed that range, the receiver
alone is stuck (it cannot skip, and the data it renounced may no longer be
retrievable). No third party is affected, no wrong message ever verifies—the
frontier/root machinery is untouched by confirmations. Confirming *behind*
(or never) merely grows the sender's archive, bounded by the window plus the
unconfirmed tail, and is covered by sender-local retention policy (see
[Archive Pruning](#archive-pruning)).

### Threat: Signal / Register Flooding

**Attack**: A malicious peer runtime floods the channel with protocol
messages to impose consumption cost.

**Mitigation**: Nothing to exploit on either lane. Lifecycle signals on the
data stream are ordinary window-counted messages—flooding them exhausts the
flooder's own credit like any spam; and against a runtime that ignores its
own gate (the grant is advisory—see Flow Control), consumption being
voluntary is the real bound: the receiver takes what it wants, excess only
bloats the flooder's own archive and tree. Register publishes on the ack stream cannot force work at
all: the reader takes exactly one leaf (the head) per read, so publishing
a million registers only bloats the publisher's own accumulator and
archive. And consumption is always voluntary: the receiver can stop at any
time, keeping only its frontier. For traffic that is well-formed but not
worth its processing weight—garbage *data* rather than protocol
flooding—the same lever is exposed to upper layers as per-channel
suspension, and funding consumption at all is receiver policy (see Flow
Control: Consumption economics and Suspension).

### Threat: Pruning on a Reverted Confirmation

**Attack**: The sender prunes on a register published in a receiver block
that never becomes canonical; the receiver's canonical history then still
needs the pruned range.

**Mitigation**: The pruning tier rule: prune only once the register-carrying
block is irreversible per [the tier table](#the-tier-table).
This is the same bet the sender already makes when consuming speculatively
from that peer—pruning adds no new trust assumption. Blocks being permanent
(candidates transient) makes published registers survive availability
timeouts and resubmission. Credit updates, whose failure mode is merely
wrong throttling, may act on speculatively read registers without this
restriction.

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
| **UMP Signals** | `Provides(StreamsRoot)` (block-emitted) / `Requires` set of `(ParaId, StreamsRoot)` (PVF-synthesized from the consumption record) | Relay chain verification |
| **Relay Chain State** | Ring of the last W `StreamsRoot`s per sender—fixed size, nothing per stream | Matching against included blocks |
| **Stream Commitment Tree** | Keyed trie `StreamId → stream MMR root`, maintained by the sender | One-hash commitment over all streams; inclusion proofs verified parachain-side |
| **Messaging Inherent (block body)** | Incoming payloads plus trust-free positioning hints (`base`/`start_peaks` for register/event items)—no StreamsRoots, no proof objects, all for declared consumed streams | Consume messages; verification by recomputation into the consumption record |
| **Requires Lifts (POV)** | MMR extension + tree inclusion proofs | Bind recorded states to current roots where the block could not (partial consumption, resubmission, bundles) |
| **Parachain Runtime** | Per-stream MMR frontiers, commitment tree nodes, per-channel state, per-stream highwaters | Internal bookkeeping, flow control, event consumption |
| **Off-Chain (Collators)** | Actual messages | Message delivery |
| **In-Band Signals** | `OpenChannel` / `CloseChannel` / `Upgrade`—the sender side of the channel lifecycle, ordinary window-counted messages | Channel lifecycle; never seen by the relay chain |
| **Ack Streams** | The latest `Register { version, up_to, grant, closed }`, lossy latest-wins—the receiver's entire channel voice | Acceptance + flow control + pruning watermark + close, out-of-band |
| **Stream Conventions** | `Channel{recipient, domain, num}`, `Ack{..}` (mirroring, recipient swapped), `Broadcast{domain, subdomain, num}`, private kinds | Parachain-layer semantics over relay-invisible ids—extensible without relay changes |

## Appendix B: MMR Extension Proof Details

An MMR extension proof demonstrates that a newer MMR root extends an older
one. The structure is defined at first use (see
[Requires Lifting](#requires-lifting-pov-proofs)); in the
implementation it is covered by `polkadot-ckb-merkle-mountain-range`
(`gen_ancestry_proof` / `verify_incremental`), an established `no_std`
workspace dependency—no hand-rolled accumulator (see
[PR #12368](https://github.com/paritytech/polkadot-sdk/pull/12368)).
Conceptual verification:

```rust
impl MMRExtensionProof {
    /// Extends the verifier's own MMR state (a tracked frontier, or the
    /// peak set reconstructed from an in-block inclusion proof) and
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
        ensure!(self.leaf_count >= old.leaf_count, Error::Regression);
        Ok(MmrRoot(bag_peaks(&merge_prefix(
            &old.peaks,
            old.leaf_count,
            self.leaf_count,
            &self.connecting_nodes,
        )?)))
    }
}
```

## Appendix C: Acknowledgement Rule Summary

| Rule | Description |
|------|-------------|
| Base rules | All rules from Low-Latency v2 |
| Message verification | Don't acknowledge until consumed source blocks are irreversible per [the tier table](#the-tier-table) |
| Cycle handling | A block depends only on blocks complete before it was built; candidate-level cycles (super chains, bundled candidates) form atomic enactment groups |

## Appendix D: Commitment Schema Summary

```rust
// === STREAMS ===

// Sender-scoped stream identifier; full key = (sender, StreamId).
// Relay-invisible, parachain-structured. Manual canonical SCALE encoding
// (8 bytes: kind byte ++ big-endian fields; consensus-critical, frozen,
// test-vector-locked)—used as wire format AND as the commitment-tree key;
// decode rejects reserved kinds. Domain/subdomain: reserved delegation
// fields, 0 by default.
// Uniform rule: `recipient` names the chain the stream is addressed to
// (who reads it); broadcast has no addressee.
enum StreamId {
    Channel   { recipient: ParaId, domain: u8, num: u16 }, // kind 0x00
    Ack       { recipient: ParaId, domain: u8, num: u16 }, // kind 0x01: same
                        // discriminator as its channel, recipient swapped
    Broadcast { domain: u16, subdomain: u8, num: u32 },    // kind 0x02
    Private   { kind: u8, body: [u8; 7] },                 // kind 0x80..=0xFF
    // kinds 0x03..=0x7F reserved for future standard kinds
}

// === COMMITMENTS (UMP signals, verified by relay chain) ===

// Root of the sender's stream commitment tree (keyed binary trie,
// StreamId → stream MMR root). ONE hash commits all streams.
struct StreamsRoot(Hash);

// Canonical sorted set, strictly increasing ParaIds enforced at decode;
// one entry per SOURCE (bounded by paras), covering all its streams.
struct RequiresSet(BoundedVec<(ParaId, StreamsRoot), MaxCommitmentEntries>);

enum UMPSignal {
    // ...
    Provides(StreamsRoot),  // block-emitted; constant 32 B
    Requires(RequiresSet),  // NEVER block-emitted: synthesized by the
                            //   validate_block wrapper from the
                            //   consumption record (+ lifts)
}

// === CONSUMPTION RECORD (per-block outbox, runtime API) ===
// Per touched stream one Interval{start, end}—where the block's
// consumption began and ended—grouped by source (like the lifts); no
// StreamsRoots, the runtime never sees one. The PVF stitches intervals
// across the bundle (equal or advance-proven), lifts each chain's
// endpoint; the node reads it for a block's dependencies.

// === REQUIRES LIFT (in POV, never in the block or commitments) ===
// Binds a recorded state to a current committed root where the block
// could not: partial consumption, resubmission, bundles—one mechanism.
// Generatable by anyone from public data.

// Transported grouped per source (Vec<(ParaId, Vec<RequiresLift>)>),
// positionally matched to the record's streams within each source.
struct RequiresLift {
    advances: Vec<MMRExtensionProof>, // one per interval-chain gap
    extension: MMRExtensionProof,    // chain endpoint ⊑ current; yields
                                     //   the current stream root
    tree_proof: TreeInclusionProof,  // walked from it, yields the
                                     //   StreamsRoot the entry becomes
}

// === RELAY CHAIN STATE (all of it) ===

// RecentProvides: ParaId → ring of last W StreamsRoots. Fixed size,
// nothing per stream; pruned wholesale on sender offboarding.

// === PARACHAIN RUNTIME STATE (internal, not on relay chain) ===

// Sender tracks: per-stream MMR frontiers (roots computed on demand) +
//   commitment tree nodes (StreamsRoot computed by folding touched roots)
// Channel receiver tracks: per-stream MMR frontier (position and root
//   derived from it)
// Event subscriber tracks: one monotonic highwater per consumed stream

// === OFF-CHAIN (between collators) ===

// MessagesRequest{stream, start, under: StreamsRoot, max_bytes} →
// MessagesResponse{base, leaf_version, payloads, start_peaks,
//                  extension, tree_proof}:
// every response independently verifiable against the requester-chosen
// provides root (recompute frontier from start_peaks → extension →
// tree walk → compare).
// max_bytes = 0 serves pure lift material. No block hashes anywhere on
// the wire—trust is keyed by root; positions implicit: base + i.

// === CHANNEL-STREAM PAYLOADS (leaf payload = SCALE(SpecMsgKind);
// preimage framing and LEAF_VERSION untouched) ===

enum SpecMsgKind {
    Signal(SpecMsgSignal),  // lifecycle only; ordinary window-counted msgs
    Data(Vec<u8>),          // userspace; demux (incl. XCM envelope) upper-layer
}

// Sender side of the channel lifecycle. Versioned by monotonic per-side
// announcements (sender in-band, receiver in the Register; effective =
// min); the listed variants + the Register format are the frozen core,
// parseable at every version.
enum SpecMsgSignal {
    OpenChannel { version: u8 },   // index 0, initial announcement
    CloseChannel,                  // sender half-close
    Upgrade { version: u8 },       // raise announcement mid-channel
}

// === ACK-STREAM PAYLOAD (the receiver's entire channel voice; lossy,
// latest-wins, read by inclusion proof; first publish = sender-visible
// acceptance, produced by the receiver's accept extrinsic) ===

struct Register {
    version: u8,               // receiver's announcement, monotonic
    up_to: MessagePosition,    // cumulative watermark, monotonic
    grant: WindowGrant,        // advisory credit beyond up_to, may shrink
    closed: bool,              // receiver-side close / rejection
}

struct WindowGrant { max_messages: u32, max_bytes: u64, max_message_size: u32 }

// === PARACHAIN RUNTIME STATE (channels, in addition to frontiers) ===

// Sender:   OutChannels: ChannelId{peer: recipient, domain, num} →
//           closed_by_us, own announcement, latest Register read (credit
//           + watermark + peer version; phase derives from these).
// Receiver: InChannels: ChannelId{peer: channel's sender, domain, num} →
//           published Register, sender's announced version, suspended
//           (upper-layer off switch: STF refuses consumption, stream
//           omitted from consumed_streams(), registers grant zero).
// Frontiers are eternal—channel close/reopen never touches them.
```

## Appendix E: Comparison of Messaging Modes

| Mode | Latency | Trust | Use Case |
|------|---------|-------|----------|
| Super-chain (intra-block) | < 1 block | Same collator set | Tightly coupled shards |
| Speculative (acknowledged) | ~1-2 blocks | Trust domain collators | Fast cross-chain DeFi |
| Event streams (lossy, broadcast) | tier-dependent (speculative or inclusion) | per tier | Pub-sub: oracle feeds, notifications—latest-wins, no back-channel |
| Inclusion-based | ~2-3 relay blocks | Relay chain only | Cross-domain, untrusted |
| HRMP (legacy) | ~3+ relay blocks | Relay chain only | Deprecated |
