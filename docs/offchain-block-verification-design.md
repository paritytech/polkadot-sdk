# Off-Chain Parachain Block Verification

## Design Document

| Field | Value |
|-------|-------|
| **Authors** | eskimor |
| **Status** | Draft |
| **Version** | 0.1 |
| **Related Designs** | [Speculative Messaging](speculative-messaging-design.md), [Low-Latency Parachains v2](low-latency-v2-design.md) |

### Version History

| Version | Area | Changes |
|---------|------|---------|
| 0.1 | | Initial version. |

---

## Table of Contents

1. [Introduction](#introduction)
2. [Motivation](#motivation)
3. [Goals](#goals)
4. [Non-Goals](#non-goals)
5. [Solution Overview](#solution-overview)
6. [Trust Anchors](#trust-anchors)
7. [The Verification Runtime API](#the-verification-runtime-api)
8. [Executing Foreign Wasm Against Proven State](#executing-foreign-wasm-against-proven-state)
9. [Code Management and Runtime Upgrades](#code-management-and-runtime-upgrades)
10. [Network Protocols](#network-protocols)
11. [End-to-End Flows](#end-to-end-flows)
12. [Overhead](#overhead)
13. [Security Analysis](#security-analysis)

---

## Introduction

A node that does not follow parachain A sometimes needs to verify facts about
A's *unincluded* blocks: that a header chain is real, that its blocks were
authored by A's current collators, and that A's collators have committed to it
becoming canonical. This document specifies a generic mechanism—working for
any parachain, under any consensus—by which a foreign node performs these
checks using only:

- its own view of the relay chain (which every collator follows anyway), and
- data served by the source chain's collators, all of it verifiable.

The primary consumer is [Speculative
Messaging](speculative-messaging-design.md): receiving messages *before* the
sending block's provides commitment appears on chain requires exactly these
checks. [Low-Latency v2](low-latency-v2-design.md) acknowledgement
verification by foreign parties is the second consumer.

---

## Motivation

For speculative (pre-inclusion) message consumption, a receiver must verify:

1. **The message was sent by a block**: the block commits to its provides
   roots and the batch's root is among them.
2. **The block is legit**: authored by a collator currently valid for that
   slot, on a lineage connecting to the chain's included state (an eligible
   collator could otherwise craft an *unconnected* block—a fork that can never
   become canonical).
3. **The block will become canonical**: acknowledgement signatures
   (Low-Latency v2) from the source chain's collators.

Checks (2) and (3) share a problem: they require knowing the *current
collator set* of a foreign chain—and consensus rules, session logic and slot
math are chain-specific. Hardcoding per-consensus verifiers in every node does
not scale and breaks on every consensus innovation.

---

## Goals

1. **Generic**: works for any parachain regardless of consensus (Aura, Babe,
   custom), session handling, or collator-selection mechanism.
2. **Trust-anchored in the relay chain**: nothing about the source chain is
   trusted except what the relay chain already vouches for.
3. **Cheap hot path**: per-block verification cost comparable to light-client
   header verification—parent link plus a few signature checks, no block
   execution.
4. **Graceful degradation**: any failure (unsupported source, stale state,
   resource exhaustion) degrades that source to inclusion-based messaging,
   never to accepting unverified data.

## Non-Goals

- **Verifying state transitions of unincluded blocks.** Content validity is
  enforced at inclusion (PVF execution); off-chain we verify authorship,
  lineage and commitment—deception at the content level is priced by the
  trust-domain/slashing model.
- **Replacing inclusion-based verification.** This is an additional, faster
  tier; the inclusion tier remains the floor.

---

## Solution Overview

**Standardize the interface, delegate the logic to the source chain's own
wasm.** The only party that authoritatively knows a chain's consensus rules is
that chain's runtime. The receiver obtains exactly that code trustlessly: for
cumulus chains the validation code *is* the runtime wasm, registered on the
relay chain and identified by `CurrentCodeHash`. The receiver's node executes
this wasm—stateless-client style, against storage proofs anchored at an
*included* head's state root—calling standardized verification entry points.

The three checks then stack (for speculative messaging):

| Tier | Provides root source | Verification |
|---|---|---|
| Speculative | Header digest | full stack: code + lineage + authorship + acks |
| Optimistic (backed) | `CandidateBacked` event (`HeadData` incl. digest) or `candidates_pending_availability` runtime API | backing group validity; availability may still time out |
| Inclusion-based | `CandidateIncluded` / `paras::Heads` | relay chain |

One code path serves all tiers, differing only in how much of the stack runs.

---

## Trust Anchors

Everything reduces to data the receiver's node obtains from its own relay
chain view:

- **Included heads**: `paras::Heads` (latest full head data = encoded header
  for cumulus chains) and `CandidateIncluded` events. Note
  `CandidateDescriptor::para_head` is the hash of the head data, which for
  cumulus chains *equals the parachain block hash*—so "became canonical" has
  a crisp relay-observable meaning.
- **Code identity and blob**: `CurrentCodeHash` / `FutureCodeHash` (paras
  pallet). The blob itself is *relay chain state* (`CodeByHash`)—and since
  collators typically run relay chain full nodes, it is usually already in
  the local database, no fetch at all. Otherwise: any relay full node
  (`ParachainHost::validation_code_by_hash`); fetching from source collators
  is possible (verified by hash, so trust-neutral) but likely not needed at all.
- **Upgrade timeline**: `FutureCodeUpgrades` (`expected_at`), go-ahead signal
  timing—see [Code Management](#code-management-and-runtime-upgrades).

Two kinds of anchors, deliberately distinct:

- **Linkage anchors**: any *previously verified* header. The receiver keeps a
  per-source **verified frontier**; new header chains only need to connect to
  it (incremental verification).
- **Proof anchors**: state roots of *included* heads ONLY. An unincluded
  header's state root is author-claimed and unvalidated until inclusion;
  anchoring storage proofs at it would let the author fabricate the very
  state (e.g. authority sets) being consulted.

  This rule is **coupled to tip-only acknowledgement checking** (see
  [Ancestry transitivity](#ancestry-transitivity)); relaxing it is not just
  risky but breaks the slashing argument entirely. Concrete attack under
  unincluded anchoring: a single malicious collator authors invalid block H1
  whose fabricated state swaps in attacker-controlled *ack* keys while
  keeping the real *author* keys in place. Nobody acks H1—irrelevant, only
  the tip is checked. H2 on top (authored with a retained real key) is acked
  by the fake keys and verified against H1's fabricated state: everything
  checks out at `Max` confidence. No real collator ever signed an ack, and
  authoring invalid blocks that die at backing is not slashable—**total
  attacker exposure: zero**. With included-only anchors, the state judging
  the acks is relay-validated and every deception step costs real, staked
  signatures.

  *Considered alternative*: allow unincluded state roots as anchors, but
  require acknowledgement verification for the **entire unincluded segment**,
  each block's acks judged against its *parent's* state. This restores
  linear pricing inductively (any fabricated state transition first needs
  real-set acks on an invalid block—slashable), at the cost of k ack checks
  instead of one. Rejected for now: unincluded segments are short in
  practice and queued keys already cover the one session boundary; revisit
  only if >1-session unincluded segments ever matter.

The proof-anchor restriction costs nothing in practice: `pallet-session`
exposes the *queued* keys for the next session, so authority sets are known
one session ahead from included state. Any unincluded segment spanning at most
one session boundary verifies entirely from included-state proofs. Beyond that
(long-idle on-demand chains with time-driven sessions) the source degrades to
inclusion-based until its next block is included—a startup latency penalty,
then full speed again. Chains with block-driven sessions (`PeriodicSessions`
by block number) cannot rotate while idle at all, so their set proofs never
stale.

---

## The Verification Runtime API

Declared via `decl_runtime_apis!` in a primitives crate, implemented by any
participating parachain runtime via `impl_runtime_apis!` (precedent for
node-facing parachain runtime APIs: `AuraUnincludedSegmentApi`,
`CollectCollationInfo`). Everything chain-specific is opaque; only the
signatures and the `Confidence` order are protocol:

```rust
/// Implemented by the parachain runtime; executed by FOREIGN nodes against a
/// proof-backed state view anchored at an included head of this chain.
trait OffchainVerify {
    /// Verify that `headers` is a valid chain from `anchor` (exclusive) to
    /// its last element: parent hashes link, and every header is authored by
    /// a collator valid at its slot per THIS chain's own consensus and
    /// session rules (read through the proof-backed state).
    ///
    /// `anchor` is supplied by the caller from its own trust base (an
    /// included head or its verified frontier)—never peer-supplied.
    /// Headers are opaque bytes: the chain knows its own header format.
    fn verify_header_chain(
        anchor: Hash,
        headers: Vec<Vec<u8>>,
    ) -> Result<VerifiedTarget, VerifyError>;

    /// Verify an acknowledgement blob for `target` (which must already be
    /// verified via `verify_header_chain`). Blob format is chain-internal;
    /// only this signature and `Confidence` are protocol.
    fn verify_acks(
        target: Hash,
        blob: Vec<u8>,
    ) -> Result<Confidence, VerifyError>;
}
```

### Confidence

Acknowledgement confidence measures **consecutive-slot-author coverage**: the
parties able to fork at the target's parent are precisely the authors of the
slots following it, so their signatures are the meaningful commitments.

```rust
/// Totally ordered (SlotChain(0) < SlotChain(1) < ... < Max), so requests
/// can state a minimum and responses compare. One logical level, one
/// representation—no overlapping constructors.
///
/// Anything below `MIN` is NOT a confidence level: `verify_acks` errors
/// (e.g. `InsufficientAcks`). Below it, the *next* slot author—the party
/// who single-handedly decides whether the block gets extended—has not
/// committed, so there is no meaningful confidence to report.
enum Confidence {
    /// The mandatory base—authors of the previous, current and next slot—
    /// plus this many further consecutive slot authors.
    SlotChain(u32),
    /// The entire current collator set signed.
    Max,
}

impl Confidence {
    /// Base level: previous, current and next slot authors signed.
    const MIN: Self = Self::SlotChain(0);
}
```

(Names to be finalized; the total order is the requirement.)

Small-set/wraparound note: k consecutive *slots* may map to fewer *distinct*
signers (one collator's ack covers all its slots in the window)—the economic
meaning is unchanged, since deceptive passivity must be sustained for k slots
regardless of who owns them. Implementations compare *coverage* and return
`Max` whenever the entire current set has signed; for sets of ≤ 3 collators
the base (`MIN`) window already spans the whole rotation, so every valid blob is
`Max`.

### Ancestry transitivity

Acknowledgement commitments are **transitive over ancestry**: an ack on X
commits to X becoming canonical, which requires X's entire ancestry to become
canonical—so a tip ack economically stakes on the whole verified chain.
Low-Latency v2's ack rules reinforce this from the honest side (rule 4: a
collator only acks a block whose whole ancestry is acknowledged or accepted
by the relay chain).

Enforcement is entirely Low-Latency v2's: slot-gated relay submission means
abandoning an acked block requires a *subsequent slot author* to build
around it—if that author acked, the ack plus their conflicting block form a
slashable offense pair; if they didn't ack, they were never part of the
confidence count. The remaining residual—all acked authors staying passive—
is deliberately unslashed (it would punish honest outages) and is exactly
why confidence below `Max` is not a guarantee. No additional slashing
machinery is needed in this design.

Consequences:

- `verify_acks` is called for the **tip** only; no per-ancestor blobs are
  needed. Consuming data from an ancestor of a C-confidence tip rides on the
  tip's confidence—every tip-acker staked on that ancestor.
- Tip-measured confidence is also the *relevant* measure: fork risk lives at
  the tip (future slot authors choosing what to extend); descended ancestors
  can only be forked by forking everything above them.
- **Coupling**: tip-only checking is sound only because acks are judged
  against relay-validated (included) state—see the proof-anchor rule in
  [Trust Anchors](#trust-anchors), including the attack that materializes if
  either half of that pair is relaxed without strengthening the other.

### Implementation note: session boundaries

The *data* for next-session authorship is in place today: `pallet_session`
stores `QueuedKeys` (the full next-session key sets, Aura keys included) one
session ahead, so a proof against an old-session included anchor can answer
authorship for the following session. The *logic* is not: existing APIs
(`AuraApi::authorities`, aura-ext) only ever expose the current set.
"Authorities for the session containing block X, switching to queued keys
when X lies past the boundary—including a boundary crossing mid-header-chain"
is new logic each chain implements inside `OffchainVerify`. A **reference
implementation for the default cumulus stack** (Aura +
`pallet-session`/`PeriodicSessions` + collator-selection) should ship with
the primitives, so chains on the standard stack get session-boundary
handling correct by construction rather than each hand-rolling it.

### API discovery

`RuntimeVersion.apis` is readable from the wasm blob's custom section without
execution (`sc_executor::read_embedded_version`). A source whose runtime does
not expose `OffchainVerify` (or only an unsupported version) is degraded to
inclusion-based messaging. Standard runtime API versioning covers evolution.

---

## Executing Foreign Wasm Against Proven State

The machinery is the existing remote-call-with-proof pattern from
`sp-state-machine`:

- **Source collator** (serving a request): `prove_execution`—runs the call on
  its full state with a recording backend, returns `(result, StorageProof)`
  containing every touched key. The prover does not choose what to prove;
  execution does.
- **Receiver**: `execution_proof_check`—rebuilds a `MemoryDB` trie backend
  from `(anchor_state_root, proof)` and *re-executes the call itself* with
  its own `WasmExecutor` and the hash-verified code. The peer's claimed
  result is never trusted; only the receiver's own execution counts.

The wasm reads whatever *it* needs through the backend (session keys, queued
keys, slot configuration)—no well-known storage keys, no per-chain
conventions. A missing key fails the execution cleanly, signalling the
receiver to re-request a wider proof (or that the anchor is too stale).

### Resource bounds

The receiver voluntarily executes *foreign* wasm. Hash verification proves
identity, not benignity: the code can loop or over-allocate. Verification
calls therefore run under PVF-style resource bounds (execution timeout +
memory cap). A source whose wasm exhausts them is degraded to inclusion-based
messaging. Without this, a malicious trust-domain member gets a free DoS on
every receiver.

### Caching

- **Compiled wasm instance**: per code hash; invalidated only by upgrades.
- **Proof-backed backend**: per (source, latest included anchor)—the anchor
  advances at most once per relay chain block, so one small proof (a few KB:
  authority set, session data, slot config) serves all verifications within
  that window. This is the amortization that matters: chains producing
  blocks at millisecond cadence (Basti blocks) verify many blocks against
  one cached backend. No session bookkeeping: the anchor is always fresh
  enough that the current (plus queued) session data is simply what its
  state contains; a missing-key error remains the generic
  re-request-fresher/wider-proof signal.
- **Verified frontier**: per source; advances with each verified chain,
  trimmed to the included head's ancestry on inclusion.

---

## Code Management and Runtime Upgrades

Verification semantics for a header are defined by the code active *at that
block*. Exact upgrade semantics (verified against `paras/mod.rs`):

1. **Scheduling**: an upgrade is announced on the relay chain
   (`FutureCodeHash`, `FutureCodeUpgrades[para] = expected_at`) sessions in
   advance. The receiver can prefetch the new blob the moment it is
   scheduled—at any time there are **at most two candidate codes** per
   source.
2. **Arming**: at relay block `expected_at`, a *timer* in the relay
   initializer sets `UpgradeGoAheadSignal = GoAhead`
   (`process_scheduled_upgrade_changes`). No inclusion is involved. The
   signal alone changes nothing about which code is needed.
3. **Trigger**: the first para block whose relay parent shows the signal—call
   it B_apply—executes under the **old** code, applies the pending code as
   its last act, and carries `DigestItem::RuntimeEnvironmentUpdated` in its
   header. Its *children* run the new code. No para block ⇒ no transition:
   an idle chain never needs new code, and its first block back is still
   old-code (it merely applies).
4. **Relay bookkeeping**: `CurrentCodeHash` swaps at *inclusion* of the first
   candidate with relay parent ≥ `expected_at` (`note_new_head` →
   `set_current_code`). This gates relay-side candidate validation, not
   off-chain verification. Since candidates are included sequentially per
   para, old-code blocks are never included after new-code ones—the verified
   frontier never crosses a code boundary backwards.

### Receiver rule: degrade across the upgrade, don't straddle

Speculative verification **pauses** for the upgrade window and resumes on the
other side. Deterministic, keyed entirely to the receiver's own relay view:

- **Before `expected_at`**: nothing changes—no block can apply the upgrade
  before the signal arms, so speculation continues untouched.
- **Degrade per block once the signal arms**: a block is affected only if
  its *relay parent* is at or past `expected_at`. Every cumulus block
  carries its relay parent in a header digest, deposited unconditionally by
  parachain-system (`CumulusDigestItem::RelayParent`, or the RPSR digest
  with storage root + number; `find_relay_block_identifier` decodes both).
  Blocks whose digest-claimed relay parent predates the signal remain
  old-code and stay speculatively verifiable; from the first block at or
  past it, the source drops to inclusion-based. The digest is
  author-claimed for unincluded blocks, but lying is fail-safe in both
  directions: claiming "older" while secretly applying the upgrade makes
  descendants fail old-wasm verification (fail-closed degrade); claiming
  "newer" only censors the author's own chain's latency. Missing/unresolvable
  digest → conservative degrade at `expected_at`.
- **Resume** at the first included head whose candidate descriptor's
  `validation_code_hash` equals the new code hash. That head's state was
  produced *and relay-validated* under the new code, migrations included—the
  first coherent proof anchor for the new wasm.

Two subtleties this rule deliberately sidesteps:

- **The marker is not reliable off-chain.** `RuntimeEnvironmentUpdated` is
  produced by execution, which the receiver does not perform for unincluded
  blocks—a malicious author can serve a marker-less header (an invalid block
  that dies at backing, but indistinguishable off-chain). Keying degradation
  to the relay timer instead of the marker removes any reliance on it.
- **B_apply's included head is NOT a valid proof anchor**, even though it is
  included: upgrades force a candidate boundary (enforced by the PVF—
  `validate_block` panics on more than one block per PoV when applying an
  upgrade), so B_apply's candidate is validated under the *old* code, and its
  state is pre-migration (`on_runtime_upgrade` runs in its child, under new
  code). Anchoring the new wasm there would read old-layout state. Hence the
  resume condition checks the *descriptor's code hash*, not merely "some
  inclusion happened".

Cost: an inclusion-latency window (seconds to ~a minute) around
operator-planned events that occur on the order of months. `FutureCodeHash`
prefetch remains a mild optimization for resume speed.

**Rejected alternative—straddled verification** (verify the old-code prefix
with the old wasm, the new-code suffix with the new wasm): the new wasm would
read *pre-migration* state through the proof backend—reading state with a
non-matching runtime is unsound (migrations haven't run; layouts and
semantics may differ arbitrarily). Only new-wasm ×
new-code-produced-included-state is a coherent pairing; the degrade/resume
rule uses exactly and only that.

An upgrade that drops or breaks the `OffchainVerify` API degrades that source
to inclusion-based messaging until fixed—deployment-checklist item for
participating chains.

---

## Network Protocols

All request/response with the source chain's collators. Every response is
verifiable; lying peers can only waste bounded work.

```rust
/// Fetch the validation code blob (hash known from the relay chain).
/// Usually unnecessary: the blob is relay state, locally available to any
/// collator running a relay full node. This request could be introduced for relay
/// light-client setups; either way, verify blake2_256(code) == hash.
struct CodeRequest { code_hash: ValidationCodeHash }
struct CodeResponse { code: Vec<u8> }

/// Header ranges: from the receiver's verified frontier towards a target.
/// Plain ranges—upgrades never split them: speculative verification pauses
/// across upgrade windows (see Code Management), so a range is always
/// single-code.
struct HeaderRangeRequest {
    from: Hash,
    to: Hash,
    max_bytes: u32,
    /// Included head (from the REQUESTER's own relay view—the requester
    /// decides the anchor, never the server) to generate the storage proof
    /// against. `None`: requester has a valid session proof cached, no
    /// proof wanted.
    proof_anchor: Option<Hash>,
}
struct HeaderRangeResponse {
    headers: Vec<Vec<u8>>,
    /// Storage proof backing `verify_header_chain` over these headers
    /// against the requested `proof_anchor`; `None` iff none was requested.
    proof: Option<StorageProof>,
}

/// Acknowledgement blob at a desired confidence.
struct AckRequest {
    block: Hash,
    min_confidence: Confidence,
    /// Included head to prove against; `None` = cached session proof, no
    /// proof wanted. Requester-chosen, as in `HeaderRangeRequest`.
    proof_anchor: Option<Hash>,
}
struct AckResponse {
    /// Chain-opaque ack data—the *argument* to `verify_acks`, judged only
    /// by it.
    blob: Vec<u8>,
    /// Storage proof backing the *execution* of `verify_acks(block, blob)`
    /// against the requested `proof_anchor`: the server ran
    /// `prove_execution` of exactly that call. Distinct from the blob: the
    /// proof feeds the proof-backed backend, not the wasm's arguments.
    /// `None` iff none was requested.
    proof: Option<StorageProof>,
}

/// Storage proof for a verification call against an included anchor.
///
/// Normally not needed as a separate request: header/ack responses come
/// with the proof attached—the serving collator runs `prove_execution` of
/// the corresponding verify call on its own data and includes the recorded
/// proof. This standalone request exists for re-requests (e.g. the cached
/// proof went stale, e.g. its anchor was superseded): the receiver states
/// the exact call it wants proven, arguments inline.
struct VerificationProofRequest {
    /// Included head to anchor the proof at.
    anchor: Hash,
    /// The entry point and its full arguments, as the receiver will execute
    /// them. The prover runs exactly this call with a proof recorder.
    call: VerifyCall,
}
struct VerificationProofResponse { proof: StorageProof }

enum VerifyCall {
    VerifyHeaderChain { anchor: Hash, headers: Vec<Vec<u8>> },
    VerifyAcks { target: Hash, blob: Vec<u8> },
}
```

Message batches and position-addressed range requests are specified in
[Speculative Messaging /
Networking](speculative-messaging-design.md#networking); the ack blob request
composes with them (one round trip can carry batch + headers + acks + proof).

---

## End-to-End Flows

### Setup per source (once, and per code change)

1. Read `CurrentCodeHash` (+ scheduled `FutureCodeHash`) from the relay.
2. Obtain the blob: local relay state (`CodeByHash`) in the common
   collator-with-relay-full-node setup; otherwise any relay full node, or
   source collators for relay-light-client setups. Verify hash regardless.
3. `read_embedded_version` → require `OffchainVerify` at a supported version.
4. Compile and cache.

### Continuous (per relay block)

Track the source's included heads (anchors) and its
code/upgrade timeline.

### Hot path (per source block / batch)

1. Gap = verified frontier tip → target. One round trip: headers + ack blob
   (at desired confidence) + storage proof (+ message batch, for messaging).
2. `execution_proof_check` against the included anchor's state root:
   `verify_header_chain`, then `verify_acks`. The segment is single-code by
   construction: blocks whose relay parent is at or past an armed upgrade
   signal are excluded from speculation until a new-code included anchor
   exists (see Code Management).
3. Messaging-specific: check the target's header digest (provides-set hash;
   format protocol-standardized—node-side pure check, no wasm involved, see
   the messaging design),
   batch root ∈ set, recompute root from payloads.
4. Accept at the returned `Confidence`; advance the verified frontier.

### Degradation ladder

Unsupported API / stale anchor beyond one session / resource exhaustion /
verification failure → the source drops to inclusion-based messaging. Never
"accept less-verified data".

---

## Overhead

| When | What | Cost |
|---|---|---|
| Per upgrade (rare) | blob (usually local relay state) + compile | ~seconds compile, cached; network only for relay-light-client setups |
| Per relay block per source | storage proof (piggybacked on header/ack responses) + trie verification | few KB, µs–ms |
| Per block (hot path) | 1 parent-hash + 1 authorship sig + k ack sigs + root recomputation | comparable to light-client header verification; no block execution |
| Failure retries | wider-proof re-request | bounded |
| Per upgrade | speculative pause until first new-code inclusion | seconds to ~a minute, monthly-order events |

---

## Security Analysis

### Threat: Phantom (unconnected) block

**Attack**: An eligible collator signs a slot-valid block on a fabricated
lineage; colluding collators ack it.

**Mitigation**: the receiver lineage walk—header chains must connect to the
verified frontier, rooted at included heads—refuses phantoms instantly; no
protocol-following receiver accepts one. Abandoning a *connected* acked
block instead requires a subsequent slot author to build around it, which is
either a slashable offense pair (they acked) or outside the confidence count
(they didn't)—covered by Low-Latency v2's existing offenses (see [Ancestry
transitivity](#ancestry-transitivity)). The floor: on-chain safety is
unaffected regardless—a receiver block built on phantom provides can never
match anything committed and dies at inclusion.

### Threat: Fabricated authority sets

**Attack**: A peer serves storage proofs claiming a different collator set.

**Mitigation**: Proofs verify against an *included* head's state root, which
the receiver takes from its own relay view. Unincluded state roots are never
proof anchors (they are author-claimed): combined with tip-only ack checking,
anchoring at them would admit a zero-slashing-exposure deception—see the
proof-anchor rule and its considered alternative in
[Trust Anchors](#trust-anchors). A wrong proof simply fails trie
verification.

### Threat: Malicious verification wasm

**Attack**: A chain's runtime loops or over-allocates in the verify entry
points, DoSing receivers.

**Mitigation**: PVF-style resource bounds on all foreign-wasm execution;
exhaustion degrades the source to inclusion-based. Identity of the code is
relay-verified (hash), so peers cannot substitute wasm.

### Threat: Wrong-code verification around upgrades

**Attack**: A peer misrepresents where the code boundary lies (e.g. serves a
marker-less header for the applying block), hoping the receiver verifies
new-code blocks with the old wasm or against pre-migration state.

**Mitigation**: The receiver never verifies across the boundary at all:
speculation pauses when the upgrade signal arms (a fact of the receiver's own
relay view—peers have no say) and resumes only at an included head whose
candidate descriptor carries the new code hash. There is no window in which a
peer-influenced boundary decision exists (see [Code
Management](#code-management-and-runtime-upgrades)).

### Threat: Stale sets after long idleness

**Attack**: Speculative acceptance based on an outdated collator set.

**Mitigation**: Set knowledge is valid for at most one session past the
included anchor (queued keys); beyond that, verification fails closed and the
source degrades to inclusion-based until its next inclusion. Block-driven
session chains cannot rotate while idle and are unaffected.
