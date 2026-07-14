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
| 0.5 | We bring back root-hash-only commitments to the relay chain (reverting 0.3's flat sets, as its own analysis [PR #10449](https://github.com/paritytech/polkadot-sdk/pull/10449) anticipated), which allows a much more flexible design: parachains can have as many channels as they want, with the semantics they want—relay chain state is a fixed-size ring of recent roots per sender. Purely on the parachain side, the structured `StreamId` takes the place of the previous plain destination `ParaId`. Channels become unidirectional; flow control moves to a lossy acknowledgement stream per channel (the receiver's register, which also carries acceptance and close); and we define lossy broadcast streams (pub-sub). **Large rewrite—re-read**: Message Accumulators and Streams, Candidate Commitments, Relay Chain Matching, Channels and Flow Control, Event Streams. |
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
   - [Catch-Up: Partial Consumption](#catch-up-partial-consumption-in-normal-operation)
   - [Late Block Proofs](#late-block-proofs)
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
  want—no deposits, no relay chain state per channel, no governance in the
  loop
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

For many cross-chain use cases, 12-18+ second messaging latency is prohibitive:

- **DeFi**: Cross-chain arbitrage, liquidations, and atomic swaps require fast
  execution
- **Gaming**: Interactive cross-chain gameplay needs sub-second responses
- **User Experience**: Multi-chain dApps feel sluggish when every cross-chain
  action takes 12–18+ seconds

And beyond latency, HRMP simply lacks primitives the ecosystem keeps asking
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

7. **Richer Primitives**: Make channels abundant (no deposits, no per-channel
   relay state, multiple lanes per peer) and provide native pub-sub event
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
   roots. Receiving chains emit "requires" commitments: one
   `(source, StreamsRoot)` entry per source chain whose streams they depend
   on in this block.

3. **Off-Chain Coordination**: Collators exchange messages directly, without
   relay chain involvement.

4. **Relay Chain Enforcement**: At inclusion time, the relay chain verifies
   that all "requires" are satisfied by corresponding "provides"—a hash
   membership check against a small per-sender window of recent
   `StreamsRoot`s. It never sees streams, positions, or proofs.

5. **Proofs, all parachain-side**: The receiver's own runtime (or the PVF)
   verifies everything below the top hash: tree inclusion proofs connect a
   stream's root to the sender's `StreamsRoot`; MMR extension proofs bridge
   an older consumption point to a current root. Carried in the block body
   (catch-up: a lagging receiver consumes only part of a backlog) or in the
   POV (late block: an already-authored block resubmitted after the window
   moved on).

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
│              Late Block with Proof (Fallback)                       │
├─────────────────────────────────────────────────────────────────────┤
│  Chain A Block N   ...time passes...   Chain A Block N+K            │
│  (provides: T_N)                       (provides: T_{N+K})          │
│                                                                     │
│  Chain B Block M (built against T_N, arrives late)                  │
│  POV includes: proof lifting B's dependency from T_N to T_{N+K}     │
│  PVF verifies it and rewrites B's requires before matching          │
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
/// path the sender used), locked by test vectors in the primitives.
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
- Late block proofs only grow with messages to that specific receiver

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
/// leaves = the streams' MMR roots.
/// Distinct newtype from MmrRoot—the two kinds of root flow through
/// different checks and must not be confusable.
struct StreamsRoot(Hash);

/// Proof that one stream's MMR root is the entry at its StreamId under a
/// given StreamsRoot: the sibling hashes along the id's path through the
/// trie (~log₂(S) of them) plus the path-compression metadata the compact
/// trie structure requires. The verifier reconstructs the path from
/// (StreamId, MmrRoot) upward and compares the result against the
/// StreamsRoot—id and root are both bound by the path, neither is taken on
/// faith.
///
/// The node encoding is CONSENSUS-CRITICAL and protocol-fixed: every
/// implementation must reproduce byte-identical StreamsRoots and every
/// foreign node must verify these proofs. The concrete byte format is
/// specified with the primitives (companion to the #12346 family), under
/// two constraints mandated here: domain-tagged node hashing with no
/// ambiguous parses (same rationale as Leaf Hashing), and injective node
/// encoding.
struct TreeInclusionProof {
    siblings: Vec<Hash>,
    // + compact-trie path metadata
}
```

An example: chain A runs a channel to B (ParaId 2001 = `0x7D1`) with B's
ack register for the reverse channel, a channel to C (`0x7D2`), and one
broadcast stream. Four keys, and the tree they span (binary trie, shared
key prefixes compressed into single edges):

```
Keys (kind ++ recipient ++ domain ++ num, hex):
  Channel{B}    00 000007D1 00 0000
  Channel{C}    00 000007D2 00 0000
  Ack{B}        01 000007D1 00 0000
  Broadcast{0}  02 0000 00 00000000

                     StreamsRoot
                    /           \
         (kinds 0x00,0x01)     Broadcast{0}
           /          \          = MMR root
     (kind 0x00)     Ack{B}
      /       \        = MMR root
 Channel{B}  Channel{C}
 = MMR root  = MMR root
```

The shape follows the key bits: kinds 0x00 and 0x01 differ only in their
last bit, so the channel and ack leaves share a long common edge, while
0x02 splits off at the top. Inclusion proof for Channel{B} = the sibling
hash at each branch on its path: H(Channel{C} leaf), H(Ack{B} leaf),
H(Broadcast{0} leaf) — 3 hashes here.

Everything between branch points is one compressed edge, so the tree has
exactly one branch per *distinguishing* bit among the keys present—proof
length tracks the number of streams, not the key width.

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
  O(k·log S) hashes; node storage O(S). Trivial at realistic stream counts.
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
its own. Read that as the user-facing headline it is: **outbound streams
are effectively unbounded and free**. Where HRMP rations channels through
deposits and relay state, and a flat stream scheme would have needed a
per-sender budget, a chain here opens as many channels, lanes and topic
feeds as it likes—the only party paying for a stream is the chain running
it. What we pay, honestly: receivers carry ~300 B of inclusion proofs
per source per consuming block, late block proofs regain an inclusion
component, and proof-free lag tolerance denominates in sender blocks rather
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
/// batches, extension proofs) alongside block hashes, leaf hashes, peaks
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
/// of parachains a receiver consumes from.
struct RequiresSet(BoundedVec<(ParaId, StreamsRoot), MaxCommitmentEntries>);

enum UMPSignal {
    // ... existing signals ...
    /// Sender side: the root of our stream commitment tree after this
    /// block. One hash, constant size. Emitted only by blocks that touched
    /// at least one stream—an untouched tree has an unchanged root, and
    /// re-emitting it would only push a duplicate into the relay window.
    Provides(StreamsRoot),
    /// Receiver side: (source, expected StreamsRoot) for every source chain
    /// whose streams we depend on in this block.
    ///
    /// Relay semantics: the named root must be a recently committed
    /// StreamsRoot of that source (window membership; the PVF may have
    /// rewritten the entry to a current one via a late block proof—see Late
    /// Block Proofs). Everything below the hash is the receiver's own
    /// business, verified in its STF: tree inclusion proofs connect each
    /// consumed stream's root to this StreamsRoot; what the dependency
    /// *means* per stream is convention—for channels, that consumed
    /// messages are a *prefix* of the stream root (NOT consumed exactly up
    /// to it); for event streams, that inclusion proofs verify against it.
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
(see [Relay Chain Matching](#relay-chain-matching))—so receivers that are a
few sender blocks behind match directly without any proof. Nothing per
stream is stored anywhere on the relay chain (see
[Relay Chain Matching](#relay-chain-matching)).

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

/// Receiver-side: tracking consumed streams (in parachain runtime).
/// Keyed by the full stream key—a chain may consume streams of any sender.
struct IncomingMessageState {
    per_stream: BTreeMap<(ParaId, StreamId), SourceState>,
}

struct SourceState {
    /// Frontier of the stream's MMR as far as we have processed it (channel
    /// consumption; event-stream subscribers keep a single highwater
    /// position instead—see Event Streams). Both the last processed position (= the
    /// frontier's leaf count) and the root we built against (bag the peaks)
    /// are derived from it: we append incoming message leaves and recompute
    /// the root ourselves.
    frontier: MmrFrontier,
}
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
/// This is exactly the consumption state the runtime stores, nothing
/// derived. Two deliberate absences: ack registers carry no resume
/// state—which registers to read follows from the open channels (see
/// `out_channels`; head-ness of a read is pinned by the required root,
/// see Flow Control)—and private kinds cannot appear, since the standard
/// pallet cannot consume a stream whose discipline it doesn't know.
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
```

One deliberate absence: unsolicited channel opens are not listed by any
API—they are node-observed (pushed batches for streams not yet consumed),
not runtime state, and enter as candidate opens through the inherent (see
[Channels: Opening](#channels)).

Inputs flow back into the runtime exclusively through the messaging
inherent ([#12531](https://github.com/paritytech/polkadot-sdk/issues/12531)):
fetched batches with their proofs, the `StreamsRoot`s the collator selected
for the requires entries, catch-up proofs, ack-register reads, and
candidate opens. The runtime verifies everything
(see Verification below)—the API and the inherent are a trust boundary,
not a trusted channel.

**Recovery from downtime falls out of this split.** All authoring-relevant
state is consensus state behind the API—a returning collator syncs its
chain and resumes: `consumed_streams()` says where fetching continues;
already-consumed messages need no refetch (state reflects them). The one
node-local structure, the sender-side archive, rebuilds during sync if the
node executes the missed blocks (each block's `outbound_messages()`
regenerates in passing); a node syncing without execution instead fetches
the missing range from its own chain's other nodes—the ordinary fetch
protocol, pointed at one's own streams.

### Off-Chain Communication (Between Collators)

Messages are exchanged off-chain between collators. The relay chain never sees
message contents—only commitments. This section defines the wire data
(batches), the fetch protocol (how receivers obtain them), and how received
data is verified. It is normative for interoperability: the two ends are
implemented by different chains' collators.

#### Wire Format

```rust
/// What a sender shares with receivers (off-chain)
struct MessageBatch {
    /// Source chain and stream (for the channel protocol: the source's
    /// Channel{recipient, domain, num} stream)
    source: ParaId,
    stream: StreamId,
    /// Source block that produced these messages
    source_block: Hash,
    /// The stream's MMR root after this block. For a candidate-final block
    /// this is the root committed (under the StreamsRoot) by that block.
    /// When several parachain blocks are bundled into one candidate/POV
    /// ("Basti blocks"), only the bundle-final block's state reaches a
    /// commitment—intermediate blocks' roots are never committed and serve
    /// only as integrity checkpoints during fetching.
    root: MmrRoot,
    /// Tree inclusion proof: `root` is this stream's entry under the source
    /// block's committed StreamsRoot (absent for intermediate batched
    /// blocks, whose roots are uncommitted checkpoints). ~log₂(S) hashes.
    tree_proof: Option<TreeInclusionProof>,
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

#### Fetch Protocol

Addressing is always by *(stream, message position)*, never by block.
Per-stream positions are dense (0, 1, 2, ... with no gaps, by construction),
so a receiver resuming after downtime does not traverse the sender's blocks
looking for ones that wrote to its stream—source blocks that wrote nothing
simply don't occupy position space. Blocks appear only as batch boundaries
in responses.

```rust
/// "Give me messages of this stream, from position `start` onward."
/// Served by the sending chain's full nodes from their (stream, position)
/// archive (see Networking for archive maintenance and retention).
struct MessageRangeRequest {
    /// The requested stream of the serving chain.
    stream: StreamId,
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

/// "Where does this stream currently stand?" Lets a resuming receiver size
/// its backlog and pick the root to catch up toward.
struct HeadRequest { stream: StreamId }
struct HeadResponse {
    /// Leaf count of the stream's MMR.
    head: MessagePosition,
    /// Root and source block it was committed in.
    root: MmrRoot,
    source_block: Hash,
}
```

Everything is bounded by construction: a batch is one source block's sends,
a response is capped by `max_bytes`—there is no "everything up to the
current root" request. In the speculative hot path batches are also pushed
to the destination's collators on block production, not only pulled (see
[Networking](#networking)).

#### Verification

Received payloads are verified by *recomputation*; the tree proof then ties
the result to the sender's commitment. Nothing in a batch is taken on
faith:

1. Hash each payload into its leaf (`H(LEAF_TAG ‖ version ‖ payload)`) and
   append to the tracked frontier for this stream, in batch order. Order
   and count need no explicit check: appending in any other order or
   skipping a message yields a different root.
2. Check the recomputed root equals the batch's `root`—a lying `base` or
   version hint cannot redirect anything, it only makes this check fail.
   When fetching a multi-batch backlog, this check runs per batch: garbage
   from a bad peer is caught at the first batch boundary, the peer dropped,
   the range refetched elsewhere.
3. Verify `tree_proof` connects `root` to an observed `StreamsRoot`
   commitment of the source (observed via header digest, backed candidate,
   or relay state—whichever the consumption tier prescribes).

**Root choice and consumption boundaries.** Authoring policy is one rule:
**require the newest `StreamsRoot` available at the chosen tier** (see
Off-Chain Verification for tiers), and add an extension proof exactly
where the block's consumption boundary lags that root's stream entry.
Never require an older root to dodge the extension proof: staleness spends
the window's pipeline slack (see [Window Depth](#window-depth)), and the
extension machinery must exist anyway.

| Consumption boundary | Proofs needed |
|---|---|
| Caught up: boundary = the stream's entry under the required root | tree proof only |
| Behind: unconsumed messages remain past the boundary | tree proof + extension proof (= the catch-up mechanism, see [Catch-Up](#catch-up-partial-consumption-in-normal-operation)) |

The caught-up row is the steady-state hot path, and it is insensitive to
the sender's *other* streams: an unchanged stream root remains that
stream's entry under every newer `StreamsRoot`. The behind row means
exactly one thing: more messages are pending on this stream than this
block consumed (weight/POV budget, downtime being caught up on)—the
extension proof lifts the requires past the unconsumed tail. Batch roots
along a backlog cannot back requires entries; they serve as integrity
checkpoints during fetching (step 2 above).

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

Security must remain arguable: fields without a nameable attack train readers
to stop reasoning ("it's probably needed for something") and mask the fields
that do carry weight. If a future verification mode ever judges leaves by
inclusion proof *without* per-stream root context (light-client evidence,
cross-stream aggregation), the fields it actually needs get added then, with
their argument, via a version bump—that is precisely what `LEAF_VERSION` is
for.

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
extension proofs instead (see
[Catch-Up](#catch-up-partial-consumption-in-normal-operation)). Since the
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
warranted. Outrunning the window is not a failure mode: a
*lagging author* takes the normal catch-up path (its proofs are generated
fresh against the current provides—see Catch-Up); only an *already-sealed*
block whose required root slid out needs the POV-carried late block proof,
as in the resubmission flow generally.

#### Matching Against the Virtually Extended Window

There is only *one* check. Candidates arriving together (live communication)
are not a special case: before checking, the stored windows are **virtually
extended** by the `Provides` roots of all candidates being processed in this
relay chain block. Every requires entry is then matched against the extended
window. On enactment the transient extensions become permanent (pushed into
`RecentRoots`); if a providing candidate doesn't make it, its extensions
evaporate with it.

```rust
fn verify_requires(
    candidates: &[CandidateReceipt],  // all candidates in this relay block
    stored: &BTreeMap<ParaId, RecentRoots>,
) -> Result<(), Error> {
    // Transient: stored windows ∪ provides of the candidates at hand
    let window = VirtualWindow::new(stored, candidates);

    for receiver_candidate in candidates {
        for (source, expected_root) in receiver_candidate.requires().iter() {
            // The receiver's own identity plays no role in the lookup—any
            // para may depend on any sender's commitment.
            if !window.contains(source, expected_root) {
                // Not stored, not provided alongside - needs a late block
                // proof in the POV (resubmission flow)
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
`StreamsRoot`, a receiver behind on a stream carries a catch-up proof (one
extension proof + one tree proof, in-block, generated fresh at
authoring)—about a kilobyte, regardless of how far behind. A receiver
*caught up* on a stream loses nothing: an unchanged stream root remains
that stream's entry under every newer `StreamsRoot`. What is bought: relay
state independent of stream count, and the per-stream
permanence/budget/eviction rules that per-stream entries would force on
the relay chain disappear wholesale.

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
the same inherent carries, per lagging stream, the lift for the unconsumed
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

/// Part of the messaging inherent
struct CatchUpProof {
    /// The stream this lift applies to
    source: ParaId,
    stream: StreamId,
    /// This stream's root under the StreamsRoot the block will require
    /// for this source, ahead of what we consume
    new_root: MmrRoot,
    /// Extension proof: our post-consumption frontier root is a prefix
    /// of new_root
    extension: MMRExtensionProof,
    /// Tree inclusion proof: new_root is this stream's entry under the
    /// required StreamsRoot
    tree_proof: TreeInclusionProof,
}
```

Under block bundling ("Basti blocks") the candidate carries one requires
entry per source, merged by the PVF: blocks emit requires only for
sources they received from, so the PVF collects the entries across the
bundle's blocks; where two blocks name the same source, the later block's
wins. Consuming across blocks like this is ordinary speculative messaging
between parachain blocks—earlier bundle blocks verify against the
source's per-block roots as they appear, including roots that will never
be committed (a source that is itself bundling: its intermediate blocks).
The constraint falls on the **last** consuming block: its required root
must be one the source actually commits toward the relay chain (a
bundle-boundary root—windowed, or a co-arriving candidate's provides),
and it must settle whatever earlier blocks verified to that boundary.
Prefix streams settle via the ordinary catch-up proof—the frontier
carries across the bundle's blocks, so the lift needs no further
consumption; inclusion-proof reads settle by re-proving the read leaf
against the boundary root (append-only: the leaf is still under it). The
relay chain validates only that final entry.

Two regimes, decided by one question—did this block consume the stream up
to the required root's entry?
1. **Caught up**: tree proof only. The steady-state hot path.
2. **Behind**—more messages pending than this block could consume: one
   extension proof (O(log n), ~1 KB) plus the tree proof (O(log S),
   ~0.3 KB), regardless of backlog size; the frontier catches up over
   subsequent blocks.

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
block never goes stale no matter how often it is retried. The mechanism is similar to the scheduling parent
header chain in Low-Latency v2.

#### The Problem

```
Timeline (T_i = A's StreamsRoot after block i; R_i = A's stream root for B):
  Block A_N: sends to B, provides T_N (containing R_N)
  Block A_{N+1}: sends (to B or anyone), provides T_{N+1}
  ... more than W further A blocks ...

  Block B_M: built against A_N's state (requires (A, T_N))

  By the time B_M arrives at the relay chain, A's window has slid past T_N.
  B_M's requires (T_N) doesn't match any stored StreamsRoot.
```

Note the window is deep (see Window Depth): this arises only when B_M's
inclusion was delayed beyond W sender blocks—the resubmission flow, not
normal operation.

#### The Solution

The late block includes a proof in its POV (outside the block itself)
lifting its dependency to the current commitment: an extension proof shows
the messages B processed are a prefix of the stream's current MMR (streams
are append-only), and a tree proof places that current stream root under a
current `StreamsRoot`.

```rust
/// Late block proof included in POV (not in commitments!)
struct LateBlockProof {
    /// The stream this proof is for
    source: ParaId,
    stream: StreamId,

    /// The stream's root under `new_requires`
    new_root: MmrRoot,
    /// A current StreamsRoot of the source—what the block's requires entry
    /// is rewritten to
    new_requires: StreamsRoot,

    /// Proof that the MMR at the old root (what the block consumed to) is
    /// a prefix of the MMR at new_root (defined in Catch-Up above)
    extension: MMRExtensionProof,
    /// Proof that new_root is this stream's entry under new_requires
    tree_proof: TreeInclusionProof,
}
```

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

The proofs are identical in shape to the ones carried in-block by catch-up
proofs—only the transport (POV vs. inherent) and the verifier (PVF
transform vs. STF) differ.

#### Verification

The PVF verifies the late block proofs and **transforms** the block's
original requires entry for that source into one referencing a current
`StreamsRoot`. This way, the relay chain only ever sees a commitment it can
verify against currently-available state. One late block proof is needed
per stream the block consumed from the stale source; all of them lift to
the same `new_requires`, which replaces the entry.

Where the per-stream boundaries come from: the PVF re-executes the block,
and the STF—which verifies each stream's proofs anyway—exposes the
per-stream boundary roots `(stream, consumed-to root)` alongside the
requires entries in its execution output. The POV adds only the proofs:

```rust
fn process_late_block_requires(
    source: ParaId,
    // (stream, root the block consumed to)—from block re-execution: the
    // STF exposes these alongside the requires entries
    consumed: &[(StreamId, MmrRoot)],
    proofs: &[LateBlockProof],  // From POV, one per stream in `consumed`
) -> Result<(ParaId, StreamsRoot), Error> {
    let new_requires = common_new_requires(proofs)?;
    // Every consumed stream must be lifted: a missing proof is an error,
    // never a silent skip (zip-style truncation would let a stream
    // consumed on an abandoned sender fork through unverified).
    ensure!(consumed.len() == proofs.len(), Error::MissingProof);
    for ((stream, old_root), proof) in consumed.iter().zip(proofs) {
        ensure!(*stream == proof.stream, Error::ProofStreamMismatch);
        // The consumed prefix is contained in the stream's current root...
        verify_mmr_extension(old_root, &proof.new_root, &proof.extension)?;
        // ...and that root is this stream's entry under the new
        // required root.
        verify_tree_inclusion(*stream, &proof.new_root, &new_requires, &proof.tree_proof)?;
    }
    // Return UPDATED entry for the candidate's Requires set.
    // The relay chain will verify this against the source's window.
    Ok((source, new_requires))
}
```

Note: The PVF verifies the proofs—the relay chain only sees the transformed
commitment. Message positions, MMR sizes, stream ids and proof details are
all internal to the parachain. The proofs just demonstrate that everything
the receiver consumed is a prefix of the respective stream under the
source's current commitment.

### Proof Size Considerations

With Low-Latency v2 allowing relay parents up to ~14,400 blocks old (24 hours),
we must consider commitment and proof sizes for worst-case scenarios.

#### Commitment Size

- `Provides`: one `StreamsRoot`, 32 B—constant, regardless of how many
  streams were touched. A chain fanning out to 100 destinations in one
  block commits the same 32 B as one sending to a single peer.
- `Requires`: ~36 B per *source* depended on in this block—naturally
  bounded by the number of parachains, independent of how many streams per
  source are consumed.

#### Recurring Tree Proof Size

Each consumed stream carries a tree inclusion proof to its source's
`StreamsRoot`: ~log₂(S) hashes (S = the source's stream count).

- S = 100 streams: ~7 hashes ≈ 224 B, call it ~300 B with structure
  overhead, per source per consuming block; several streams of one source
  share upper path segments.

This is the recurring price of the constant-size commitment—it rides in the
block body/POV, not on the relay chain, and was measured as acceptable in
the original flat-vs-hierarchical analysis
([PR #10449](https://github.com/paritytech/polkadot-sdk/pull/10449): a few
hundred bytes per source per block, sub-percent of the POV budget).

#### Late Block Proof Size

A late block proof is one MMR extension proof plus one tree proof per stale
stream: O(log m) + O(log S), m = messages in that stream.

- Typical (1000 messages to us): ~log₂(1000) ≈ 10 hashes ≈ 320 B, plus
  ~300 B tree proof.
- Worst case (24 hours of messages to one receiver, ~10⁹ leaves): ~30 hashes
  ≈ 960 B, plus the tree proof.

Extension proof size is independent of how much the sender wrote to *other*
streams; the tree proof grows only logarithmically with the sender's stream
count.

#### Practical Limits

Proofs are expected to stay small and should therefore practically fit into any
POV. To be sure, we should nevertheless set aside a few kB (e.g. 50) for not
breaking the late submission opportunity due to the POV getting too large.

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

Nothing in this section involves the relay chain. Channels are a bilateral
affair between the two parachain runtimes, conducted over the very
transport they govern—lifecycle signals are ordinary messages in the
ordered streams, confirmations live in the ack registers. To the relay
chain, all of it is one `StreamsRoot` per sender; chains wanting different
semantics (more lanes per peer, an out-of-band priority lane) can deploy
them as their own convention without anyone's permission—or even knowledge.

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
    /// defined by the XCM integration, not by this document. (XCM is
    /// self-versioning via `VersionedXcm`, so the transport loses nothing
    /// by not knowing about it.)
    Data(Vec<u8>),
}

/// Sending API exposed by the messaging pallet (domain/num default to 0).
/// Fails when the channel is not open or the send would exceed the granted
/// window (see Flow Control)—no hidden queueing; backpressure surfaces to
/// the caller.
fn send(recipient: ParaId, domain: u8, num: u16, msg: SpecMsgKind) -> Result<(), SendError>;
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

It is deliberately *not* guaranteed delivery in the stronger sense of
stalling the *sender's* block production until receipt—the sender sends and
moves on; only the receiver's consumption of that channel can stall.

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
  receiver's choice (weight budget)—catch-up proofs cover the remainder.

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

/// Receiver-granted credit (carried in the Register). Both limits apply
/// simultaneously; message count bounds the receiver's bookkeeping, bytes
/// bound its weight/POV budget per block. `max_message_size` additionally
/// caps individual messages (subject to the global `MaxMsgLen`)—mirrors
/// HRMP's `max_message_size`.
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

**Acceptance = the first register publish.** There is no `AcceptChannel`
signal: B accepts by creating its `Ack { A, d, n }` stream and publishing a
register (initial credit + its version announcement)—a deliberate resource
commitment in B's own commitment tree, which is precisely what consent
should cost. Policy (open to all, allowlist, governance, XCM-negotiated) is
B's runtime's concern, not this document's; the analog of
`hrmp_accept_open_channel`. Rejection is publishing a `closed` register—or
simply never publishing, which costs B literally nothing. Crossing opens
need no special case: two chains opening toward each other have created two
channels, which is exactly what happened.

**How an open reaches B.** An unsolicited `OpenChannel` is, by definition,
not in B's `consumed_streams()`—nothing resumes it. It arrives as a
*candidate open*: A's collators push the batch (see
[Networking](#networking)), and B's inherent provider forwards batches for
`Channel { B, .. }` streams B does not yet consume into the messaging
inherent, verified like any other batch (one leaf, tree proof to a
required `StreamsRoot`). B's acceptance policy then runs in the STF:
accept → publish the register; decline → record nothing. Node-side,
candidate opens are a bounded, low-priority queue the collator may
rate-limit or drop freely—ignoring an open is always sound, and each
attempt costs the opener a leaf in its own tree and archive.

**Unwanted opens cost the receiver nothing—and the relay chain nothing.**
B tracks no state for channels it never engages with—an ignored
`OpenChannel` sits in *A's* archive and *A's* commitment tree, occupying
A's resources only. There is no receiver-side spam surface and no
relay-side one—which is why, unlike HRMP, no deposits are needed anywhere.

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
resume seamlessly. The one obligation this places on the *receiver*:
**retain the consumption frontier after close**—it is the only state whose
loss is unrecoverable by protocol means (a receiver that discarded its
frontier can no longer verify anything against the stream's eternal MMR;
the sender's outbound frontier is consensus state and persists by
construction). Should that ever happen
anyway, the fallback is a bilateral, governance-level reset to a fresh
epoch—an exceptional action explicitly outside the protocol, in the same
category as the ParaId-reuse residual (see Security Analysis): a stuck
channel, never forgery.

Channel state (alongside `OutboundFrontier` / `SourceState`, which stay
exactly as defined)—note the two sides keep *different* state, as befits a
unidirectional protocol:

```rust
/// Channel discriminator; `peer` is the other end of the channel—the
/// recipient for outbound channels, the channel's sender for inbound
/// ones. Also keys the channel views of the Runtime API.
struct ChannelId { peer: ParaId, domain: u8, num: u16 }

/// Sender side, per outbound channel.
OutChannels: StorageMap<ChannelId, OutChannelState>,

struct OutChannelState {
    phase: ChannelPhase,        // Opening | Open | Closed
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
}
```

#### Flow Control

Two needs, one register. Both flow from receiver to sender:

1. **Rate limiting**: the receiver paces the sender to what it is willing to
   consume—bounding its own backlog and the sender's archive growth.
2. **Pruning watermark**: the sender learns which messages are consumed and
   need no longer be retained for retransmission.

Both are served by the **register**—the single object the channel's
receiver publishes on its `Ack` stream, and the receiver's entire voice in
the channel:

```rust
/// The complete receiver-side state of one channel, published as a leaf on
/// the Ack stream. Only the LATEST leaf matters (the stream is consumed
/// lossily, latest-wins); each publish supersedes all earlier ones. Its
/// very existence is the channel acceptance. `up_to` and `version` are
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

**Publishing (receiver side).** The first publish is the acceptance and
carries the initial credit; thereafter, republish when consumption
progressed enough to matter: a fraction of the granted window (e.g. ¼—the
delayed-ACK analog) or an age threshold, whichever first. Each publish is
one small leaf on the Ack stream; publishing every block is sound and
cheap, just usually pointless. The Ack stream's archive retention is
trivial: only the head is ever served.

**Reading (sender side).** The sender's inherent supplies the peer's latest
register leaf plus proofs: an MMR inclusion proof to the Ack stream's root
and a tree proof to the peer's `StreamsRoot`—the ordinary verification
machinery, one code path. Head-ness is verified, not trusted: the required
root fixes the ack stream's leaf count, and the read is the leaf at
count − 1—an old leaf cannot be served as the head. (Reads keep no
position state; the register's own monotonic fields order successive
reads.) What *kind* of read it is follows entirely from
**which `StreamsRoot` the block author requires** (a per-source authoring
policy, not a protocol mode):

- **Newest observed `StreamsRoot`** (from the peer's header digest,
  off-chain): speculative-tier freshness. If that root is not yet
  included, the requires entry matches the virtual window and carries an
  enactment dependency on the peer's candidate—exactly as for data
  consumption, and shared with it: a chain also consuming the peer's data
  emits the *same* entry, so the register read is free at the commitment
  level.
- **Newest *included* `StreamsRoot`** (observed on the relay chain):
  matches *stored* window state, so it creates **no enactment dependency
  whatsoever**—and the read is inclusion-tier *by definition*, i.e.
  pruning-safe with no further condition. A pure unidirectional sender
  reading this way risks nothing on the peer's liveness; its total
  commitment overhead is one ~36 B requires entry.

Flow control tolerates the staleness of the second option easily (window
sizing must absorb round-trip latency anyway), which makes it the sensible
default for register-only reads; the first option exists for chains that
want credit at speculative freshness and accept the coupling. The same
policy generalizes to data consumption: require the newest root *at the
tier you need*—included roots buy dependency-freedom, speculative roots
buy latency (within a tier, newest is always right—see the root-choice
rule under
[Verification](#verification)).

**Enforcement is cooperative, and that suffices.** The sender's own STF
refuses `send` beyond credit—this protects the sender's archive and
surfaces backpressure to its applications, which is who rate limiting is
*for*. The receiver needs no protection from overruns: consumption is
voluntary, and positions make overruns detectable (messages beyond the
granted window)—a violation the receiver may answer by abandoning the
channel. A peer can always stall the sender by simply not publishing—
receiver-side stalling is inherent to any credit scheme and adds no new
threat. Nothing here needs relay-chain enforcement.

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
an MMR extension proof (the catch-up proof structure, Appendix B)—whose
verification yields the new peak set (`merge_prefix(old_peaks,
connecting_nodes)`), i.e. the new frontier directly. Required inputs are the
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

#### No Channel, by Construction

Every piece of channel machinery exists to serve guaranteed delivery, and
lossy renounces the guarantee—so each piece vanishes rather than being
configured away:

- No delivery obligation → no retention obligation → **no confirmations**
  → no back-channel. Pure listeners never need any reverse path.
- No retention pressure → **no flow control**: the sender's event archive
  is prunable freely; the retention window (e.g. 24 h) is a QoS knob, local
  to the sender. A subscriber offline longer misses events—that is the
  contract, not a failure.
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
  (see [Catch-Up](#catch-up-partial-consumption-in-normal-operation)).

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
| Latency | 12-18+ seconds | Parachain block time (speculative) or ~2-3 relay blocks (inclusion-based) |
| Scalability | Limited (relay chain state) | High (off-chain, only commitments on-chain) |
| Channels / streams | Scarce: deposits, per-channel relay state, governance-mediated | **Effectively unbounded**: no deposits, no budget, no relay state per stream—open channels, lanes and topic feeds at will |
| Trust | Relay chain only | Relay chain + optional collator acknowledgements |
| Message data | Flows through relay chain | Never touches relay chain |

For the super-chain comparison with parallel-execution runtimes
(Solana-style), see [Super Chains](super-chains-design.md).

---

## Implementation Considerations

### Relay Chain Runtime Changes

1. **New UMP signals**: `Provides(StreamsRoot)` / `Requires(RequiresSet)`
   ([#12347](https://github.com/paritytech/polkadot-sdk/issues/12347)).
   Reusing UMP signals avoids a candidate receipt format change. Rollout
   caveat: older validators reject unknown UMP signals, so the node-side
   support must be deployed to a supermajority of validators before the
   corresponding `node_features` bit is enabled.
2. **Per-sender provides storage**: One ring of the last W `StreamsRoot`s
   per sender, pushed on enactment of provides-emitting candidates; pruned
   as a whole on offboarding. Fixed size (W × 32 B per sender), independent
   of stream count—there is nothing else to store or manage.
3. **Commitment matching**: At inclusion time, verify each requires entry
   `(source, expected_root)` against the stored windows *virtually extended*
   by the `Provides` roots of the candidates at hand—one unified check;
   extensions become permanent on enactment, and matches against the virtual
   part create atomic enactment dependencies.

Note: The relay chain has no MMR or tree verification logic and does not
track message history. Extension and tree proofs are verified in the
receiving parachain's STF (catch-up, in-block) or in the PVF, which
transforms commitments before the relay chain sees them (late block, POV).
The relay chain only performs simple hash matching.

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

1. **Executes the block**: The block produces its requires entries based on
   the messages it processed (referencing the `StreamsRoot`s it verified
   against)

2. **Processes late block proofs**: For each source whose entry has
   `LateBlockProof`s provided, it verifies them (extension proofs from the
   block's consumption boundaries, tree proofs into `new_requires`—see
   `process_late_block_requires` in Late Block Proofs) and rewrites the
   entry accordingly

3. **Outputs transformed commitments**: The `Requires` set contains the
   (possibly rewritten) entries that the relay chain can verify against
   currently available provides

```rust
fn process_messaging_commitments(
    block_requires: RequiresSet,          // From block execution
    proof_inputs: &MessagingProofInputs,  // From POV
) -> Result<RequiresSet, Error> {
    RequiresSet::try_from_iter(block_requires.iter().map(|(source, root)| {
        match proofs_for_source(&proof_inputs, *source) {
            Some(proofs) =>
                // Rewrite: verify proofs and update to a current root
                process_late_block_requires(*source, consumed_of(*source), proofs),
            None =>
                // No rewrite needed - the block required a StreamsRoot
                // still in the relay chain's window
                Ok((*source, *root)),
        }
    }))
}
```

This follows the same pattern as the scheduling parent header chain in
Low-Latency v2: the PVF verifies proofs and transforms inputs so the relay chain
only sees commitments it can verify against current state.

### Parachain Runtime Changes 

1. **Accumulator maintenance**: Append messages to the per-stream MMR
   frontiers, fold the touched streams' roots into the stream commitment
   tree, emit `Provides(StreamsRoot)`
2. **Requires generation**: Track consumed streams per source, verify tree
   inclusion (and, when lagging, extension) proofs in the STF, emit one
   `Requires` entry per source at the verified `StreamsRoot`
3. **Trust domain configuration**: Define trusted peers for speculative messaging
4. **Message processing**: Consume incoming batches via the messaging inherent,
   appending to the per-stream frontier; verify catch-up proofs in the STF
5. **Channel state machine and flow control**: Per-channel state
   (`OutChannels`/`InChannels`), lifecycle signal emission/consumption,
   register publishing on the ack stream and register reads via the
   inherent, `send`-side window enforcement—see
   [Channels and Flow Control](#channels-and-flow-control)

Note: this document deviates from the initial sketches in
[#12346](https://github.com/paritytech/polkadot-sdk/issues/12346) /
[#12350](https://github.com/paritytech/polkadot-sdk/issues/12350) and the
primitives PR ([#12368](https://github.com/paritytech/polkadot-sdk/pull/12368),
open at the time of writing) in three places:
- `OutboundMessages` is a per-stream vec rather than a
  `(ParaId, position)` map (invalid states unrepresentable, host-side append)
- the off-chain batch carries `base + payloads` rather than per-message
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
2. **MMR proof generation**: Create extension proofs for late blocks. This is
   necessarily node-side: the runtime only stores frontiers (peaks), which is
   enough to *verify* but not to *generate* an ancestry proof. The receiver's
   node has the required data anyway—its old frontier plus all subsequently
   fetched messages for that (source, stream) let it rebuild the MMR segment
   and generate the proof off-chain.
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
   [Archive Pruning](#archive-pruning)), event-stream archives by local
   retention policy (see [Event Streams](#event-streams)).
2. **Live propagation**: push new `MessageBatch`es to (collators of) the
   destination chain on block production—the speculative hot path.
3. **Acknowledgement propagation**: quick distribution of acknowledgement
   signatures (Low-Latency v2).
4. **MMR and tree state sharing**: allow peers to request MMR proofs/peaks
   and stream-commitment-tree inclusion proofs where node-local data
   doesn't suffice.
5. **Event subscription**: per-stream live push for subscribers plus
   head/inclusion-proof requests—nothing beyond the fetch protocol's
   existing shapes, pointed at broadcast streams (see
   [Event Streams](#event-streams)). Ack-register reads use the same
   machinery (a `HeadRequest` on the peer's ack stream plus proofs).

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
  that emits the UMP signal. This lets a batch be verified against a header
  alone—no candidate required.

  Unlike headers and ack blobs (chain-opaque, judged by the chain's own wasm
  via [Off-Chain Block
  Verification](offchain-block-verification-design.md)), this check is
  performed by the *foreign node directly*—a pure function of header, batch
  and proofs, no state involved. The digest format is therefore **protocol
  standard**, not chain-internal:

  - `DigestItem::Consensus(SPMS_ENGINE_ID, streams_root)`, at most one per
    header (engine id value TBD)—the commitment being a single hash, the
    digest carries it directly;
  - receiver check: recompute batch root → verify the batch's tree proof
    from that root to `streams_root` → compare against the digest payload.

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
`SourceState` for a source they observe being offboarded. Should stronger
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
flooder's own credit like any spam, bounded by the grant the receiver
itself extended. Register publishes on the ack stream cannot force work at
all: the reader takes exactly one leaf (the head) per read, so publishing
a million registers only bloats the publisher's own accumulator and
archive. And consumption is always voluntary: the receiver can stop at any
time, keeping only its frontier.

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
- **Self-governing channels**: ordered, guaranteed delivery with TCP-style
  credit flow control (out-of-band confirmation registers) and safe
  pruning—negotiated and enforced bilaterally, with no relay chain
  involvement
- **Unbounded, free streams over a fixed-size relay footprint**: one 32 B
  commitment per sender block, one small root window per sender—independent
  of how many channels, event streams or subscribers exist. Chains open
  channels, lanes and topic feeds at will, no deposits or budgets; the
  semantics (ordered, lossy, or not yet invented) are parachain-layer
  conventions, deployable without relay chain changes
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
| **UMP Signals** | `Provides(StreamsRoot)` / `Requires` set of `(ParaId, StreamsRoot)` | Relay chain verification |
| **Relay Chain State** | Ring of the last W `StreamsRoot`s per sender—fixed size, nothing per stream | Matching against included blocks |
| **Stream Commitment Tree** | Keyed trie `StreamId → stream MMR root`, maintained by the sender | One-hash commitment over all streams; inclusion proofs verified parachain-side |
| **Messaging Inherent (block body)** | Incoming messages, tree inclusion proofs, catch-up extension proofs, ack-register reads, candidate opens | Consume messages; prove streams under the source's required StreamsRoot; lift requires past unconsumed backlog (self-contained) |
| **Late Block Proofs (POV)** | MMR extension + tree inclusion proofs | Rewrite a stale requires entry to a current one (resubmission only) |
| **Parachain Runtime** | Per-stream MMR frontiers, commitment tree nodes, per-channel state, per-stream highwaters | Internal bookkeeping, flow control, event consumption |
| **Off-Chain (Collators)** | Actual messages | Message delivery |
| **In-Band Signals** | `OpenChannel` / `CloseChannel` / `Upgrade`—the sender side of the channel lifecycle, ordinary window-counted messages | Channel lifecycle; never seen by the relay chain |
| **Ack Streams** | The latest `Register { version, up_to, grant, closed }`, lossy latest-wins—the receiver's entire channel voice | Acceptance + flow control + pruning watermark + close, out-of-band |
| **Stream Conventions** | `Channel{recipient, domain, num}`, `Ack{..}` (mirroring, recipient swapped), `Broadcast{domain, subdomain, num}`, private kinds | Parachain-layer semantics over relay-invisible ids—extensible without relay changes |

The relay chain only sees hashes. It verifies that provides/requires match. It
never sees streams, message contents, MMR sizes, or processing
positions—proofs are verified in the receiving runtime's STF (catch-up) or
the PVF (late block).

## Appendix B: MMR Extension Proof Details

An MMR extension proof demonstrates that a newer MMR root extends an older
one. The structure is defined at first use (see
[Catch-Up](#catch-up-partial-consumption-in-normal-operation)); in the
implementation it is covered by `polkadot-ckb-merkle-mountain-range`
(`gen_ancestry_proof` / `verify_incremental`), an established `no_std`
workspace dependency—no hand-rolled accumulator (see
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
    Provides(StreamsRoot),  // constant 32 B, however many streams changed
    Requires(RequiresSet),  // sources we depend on, at their roots
}

// === CATCH-UP PROOF (in the block body, via the messaging inherent) ===
// Normal operation: lift requires past an unconsumed backlog; block stays
// self-contained.

struct CatchUpProof {
    source: ParaId,
    stream: StreamId,
    new_root: MmrRoot,               // stream root under the required root
    extension: MMRExtensionProof,    // consumed boundary ⊑ new_root
    tree_proof: TreeInclusionProof,  // new_root ∈ required StreamsRoot
}

// === LATE BLOCK PROOF (in POV, not commitments) ===
// Resubmission flow only: PVF rewrites the unaltered block's requires.

struct LateBlockProof {
    source: ParaId,
    stream: StreamId,
    new_root: MmrRoot,
    new_requires: StreamsRoot,       // requires entry rewritten to this
    extension: MMRExtensionProof,
    tree_proof: TreeInclusionProof,
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

// MessageBatch: source, stream, source_block, root, tree_proof, base,
// leaf_version, payloads (positions implicit: base + i; verification
// derives them from the receiver's own frontier; base and leaf_version are
// trust-free hints, authenticated by the root check; tree_proof places
// root under the block's committed StreamsRoot / header digest)

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
// latest-wins, read by inclusion proof; first publish = acceptance) ===

struct Register {
    version: u8,               // receiver's announcement, monotonic
    up_to: MessagePosition,    // cumulative watermark, monotonic
    grant: WindowGrant,        // absolute credit beyond up_to, may shrink
    closed: bool,              // receiver-side close / rejection
}

struct WindowGrant { max_messages: u32, max_bytes: u64, max_message_size: u32 }

// === PARACHAIN RUNTIME STATE (channels, in addition to frontiers) ===

// Sender:   OutChannels: ChannelId{peer: recipient, domain, num} → phase,
//           own announcement, latest Register read (credit + watermark +
//           peer version).
// Receiver: InChannels: ChannelId{peer: channel's sender, domain, num} →
//           published Register, sender's announced version.
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
