# Speculative Messaging on JAM

This document highlights the changes needed to support faster speculation tiers.

The settlement ring has been introduced at the `Enactment` tier 0, since it is required for all speculation tiers.
At the `Enactment` tier, the settlement ring is not strictly needed. To see an alternative verification method,
which outlives any settlement window, please see [Alternative Verification](#5-alternative-verification).


Table of Contents

- [1. Race Conditions](#1-race-conditions)
  - [1.1 PS Topological Sort](#11-ps-topological-sort)
  - [1.2 Buffering](#12-buffering)
  - [1.3 Node Side Timing](#13-node-side-timing)
- [2. Speculation Tiers](#2-speculation-tiers)
- [3. Forced Recovery](#3-forced-recovery)
- [4. Super Chains](#4-super-chains)
- [5. Alternative Verification](#5-alternative-verification)
  - [5.1 Proof Format](#51-proof-format)
  - [5.2 Costs](#52-costs)
  - [5.3 Trust Model](#53-trust-model)
  - [5.4 Why not walk to the header](#54-why-not-walk-to-the-header)
  - [5.5 Known Limitations](#55-known-limitations)

## 1. Race Conditions

1. `Provides` is evicted from the settlement ring by the time `Requires` reaches the `Accumulate` phase

This issue could happen at `Enactment` tier 0 with a low probability.

The consumer block reaches the `Accumulate` phase, after the `Provides` it targets has been evicted from
the settlement ring. This is an unlikely scenario to happen under normal operations, with a ring sized at 64
entires and production parachains operating on up to 3 cores.

Since the probability of the issue happening is low, the code complexity of introducing the
[Alternative Verification](#5-alternative-verification) is not justified for the MVP.

The most straight foward solution is to accept the risk and let the consumer block burn its slot. Then,
monitor the production chains and move to higher speculation tiers if the issue is observed in practice.

2. `Requires` reaches the `Accumuulate` phase before `Provides` is pushed into the settlement ring

The issue is present from Tier 1 and above.

Parachains build their block speculatively. It is possible that the producer block carrying the `Provides`
is delayed in the pipeline between building the block and the `Accumulate` phase.
If the consumer block reaches the `Accumulate` phase, before the `Provides` was enacted and written to the ring,
the settlement check will fail.

### 1.1 PS Topological Sort

This solution ensures that the same-block `Provides` and `Requires` are resolved in dependency order,
so that the `Provides` is always pushed into the settlement ring before the `Requires` check runs.
This is achieved using Kahn's algorithm to sort the block's digests into a topological order based on the
speculative `Provides` and `Requires`.

Within one JAM block, the `Accumulate` will process digests by core order, plus ready queue releases, which
causes a settlement mismatch if the `Requires` check runs before the `Provides` is pushed into the settlement ring.

If the consumer block `B` contains a dependency on `(A, root)` and the producer block `A` provides the same root,
a dependency edge is added from `A -> B`. The source `ParaId` selects A and root equality confirms the dependency.
If the parachain produced multiuple candidates in the same block, an implicit edge is added from each candidate to its successor.
Among the nodes with in-degree zero, the digest with the smallest index in JAM's original operand order is picked.
Digests with no incident edges keep their relative order.

If cycles are detected, the remaining nodes are appended in JAM's original order.
Only the cyclic digests lose the reorder benefit, while the rest of the block is unaffected.
Naturally, the cyclic digests will miss settlement.

### 1.2 Buffering

The buffering ensures that at most 16 digests per fork lane are buffered, with a maximum of 2 fork lanes.
The candidate is buffered once it passes every check except the settlement. A buffered chain is kept for
a maximum of `K = 4` slots.

The first buffered entry must target the parachain's enacted head and all other
entries must form a valid chain. If a collator wants to abandon a branch,
they can offer a competing one immediately, or produce a digest that gets enacted immediately (via Tier 0 fallback).

Whichever fork settles first (evaluated FIFO by lane) invalidates the other. Forks are designed to be an edge case,
not the standard operating model.

The parachain is billed for the buffered digests and the exposure is bounded by `MAX_BUFFERED_BYTES = 1.6 MiB`.
A digest carries up to 4 KiB `head_data` plus the 40 KiB upward-message budget, so roughtly ~1.4MiB for worst case.
Please note that a full upward message budget is not common in practice.

Before any work items are applied, the buffered chain is walked from the front. The root parent must still
be the enacted head and the front entry must not be expired. For each entry, all Accumulate checks are reexecuted.
If the entry settles, it is applied and removed. Otherwise, the entry remains buffered if the settlement check fails.
Any other failure invalidates the buffered chain.

**Gas Model**

The buffer walk happens before the new work item that brought the gas, but it can only spend the leftover
gas that the item doesn't need for itself. Collators must ensure sufficient gas for the buffer walk.

Collators can estimate how much gas the buffer walk will consume by reading the buffer state at their anchor.
Note that the buffer might have changed since the anchor. The next item will cover the buffered walk gas.

A `RefineOutput::SettleOnly` package carries no work digest, no parent header and PS guarantees it reaches
the buffer walk phase. This is a package that carries only gas for the buffered walk.

Each work item has associated a `gas_limit`. This is the total budget for the Accumulation phase
and the buffered walk. The buffered walk remaining gas is `walk_gas = gas_limit - gas_used_by_work_item`.

- `RefineOutput::SettleOnly` carries no extra work and `gas_used_by_work_item = 0`
- `RefineOutput::Candidate` carries an actual candidate and `gas_used_by_work_item` is the cost of base 
checks, plus per `Requires` ring check, plus per upward message replay, plus the KV write if the candidate
is buffered

The `walk_gas` is the budget for the buffered walk. First, the Accumulate phase drains as many
buffered digests as possible, until the `walk_gas` is exhausted. Then, the work item is processed.
If the candidate is a compeating fork at the front of the buffer, it is applied directly and the buffer walk
is skipped.

To ensure that any candidate can drop the buffered chain, the PS `min_item_gas` can take into account
the average buffered walk gas (not the worst case with maximum upward messages).

```rust
/// Refine output handed to Accumulate.
enum RefineOutput {
    /// Full work digest.
    Candidate(ParachainWorkDigest),
    /// Settlement only to run the buffer walk with this item's declared gas.
    SettleOnly,
}

/// Depth of buffered chain (per fork lane).
pub const MAX_CHAIN_SIZE: u8 = 16;
/// Maximum number of fork lanes.
///
/// This supports two forks which are common on polkadot
/// with elastic scaling where 1 chain doesn't get finalized
/// and a collator starts building a different chain:
///
/// ```ignore
///  A -> B -> C -> D
///    -> B' -> D'
/// ```
///
/// However it doesn't support more compeating forks.
pub const MAX_FORKS: u8 = 2;
/// Total buffered amount per para (includes everything buffering related).
pub const MAX_BUFFERED_BYTES: u32 = 1.6 * 1024 * 1024;

Map<ParaId, struct BufferCursor {
    /// The start positon.
    start: u8,
    /// Number of elements buffered.
    /// Supports two lane of forks.
    len: [u8; 2],
    /// Position where the chain diverge.
    /// The position is <= `BufferCursor::len[0]`. The lane 1
    /// exists only in `[fork_at, len[1])`
    fork_at: u8,
    /// Head hash of last entry. Ensures next buffered
    /// digest forms a valid chain.
    tip_head: [H256; 2],
    /// If the last work digest carries a code upgrade signal
    /// this must be last entry in the buffered chain. Every buffered
    /// entry carry a code_hash gainst which it was validated. Any
    /// successor would be validated against the old code.
    tip_terminal: [bool; 2],
    /// The parent the first entry(s), neede for making sure the buffered
    /// element is still based on the canonical chain.
    root_parent: H256,
}>

struct BufferedDigestKey {
  para_id: ParaId,
  /// Position within the buffered chain. The front entry is at position 0.
  position: u8,
  /// Fork lane 0 or 1. The front entry is always in lane 0.
  fork_lane: u8,
}

Map<BufferedDigestKey, struct BufferedDigest {
    /// The stored work digest.
    digest: ParachainWorkDigest,
    /// The slot in which this buffered entry was added.
    /// The full fork dies when the front entry expires
    /// at `buffered_at + K`.
    buffered_at: TimeSlot,
}
```

### 1.3 Node Side Timing

This is an heuristic to minimize the race condition from the node size, which is simple to implement and doesn't
require any changes to the runtime.

The receiver sees A's report in the guarantees extrinsic and reads the work package hash and core from the
report and the `StreamsRoot` from `spec_msg_provides`. The payloads are immediately fetched from p2p layer
and verified locally. The receiver `B` work package is created but not yet submitted.

The `B` package is submitted once count assurance bits for A's core are near 2/3. This gives a higher chance for `A`'s report
to Accumulate within 1-2 slot delay until `B` accumulates.

- If `A` dies before `B` package is submitted, then `B` rebuilds against the latest T0 enacted root without burning the slot
- If `A` dies after `B` package is submited, `B` remains buffered. `B`'s buffered digest can never settle
  and just expires at `buffered_at + K`. Then, `B` forks immediately at T0 tier and rebuilds on the second buffered lane.
- If `B`'s head doesn't advance after its package accumulates, then `B` burns the slot and conservatively rebuilds against T0


## 2. Speculation Tiers

Each tier is named after the sender-side event the consumer trusts:
**Enacted, then Guaranteed, then Announced, then Imported and Fused**.

- **Tier 0: Enacted (Safe Baseline MVP)**:
  - consume enacted roots with HRMP parity. The requires entry carries `(ParaId, StreamsRoot)` and settlement checks the root against the ring
  - latency: 1 or 2 slots from guarantee to accumulate, plus receiver build and submission time (between 12 and 24+ seconds)
  - optimization: node-side fetches `StreamsRoot` from guanranteed but not yet accumulated reports

- **Tier 1: Guaranteed (A little faster)**
  - consume guaranteed but not yet enacted roots (acts on optimization from Tier 0)
  - latency: saves 1 or 2 slots
  - risk: if settlement fails `B` burns its slot (`A` can die or `A` lands later)
  - solution for race condition:
    - 1. Node side timed work package submission heuristic
    - 2. PS topological sort of dependencies
    - 3. PS Buffering
    - 4. (Optional): `prerequisites` fields (capped at `J = 8`)
  - In theory, same block `A -> B` is possible 

- **Tier 2: Announced (High Speed Communication)**
  - consume best-block roots advertised via `/spec-msg/announce` notification protocol
  - Announced roots represent fetching hints. Before `Refine`, the package builder must retarget
  the lifts to the final candidate root. An intermediate announced root must never be writted directly into `Requires`.
  The `/spec-msg/announce` carries the exact `work_package_hash` and the final `StreamsRoot`.
  - latency: saves 2 or 4 slots
  - risk: if settlement fails `B` burns its slot (`A` can die or `A` lands later)
  - relies on llv2 ack / slashing since building block `A` is cheap
  - solution for race condition:
    - 1. `prerequisites` fields: work package hash is distributed offchain via `/spec-msg/announce` (capped at `J = 8`)
    - 2. PS Buffering 

**Tiers with lower implementation priority**

- **Tier 3: InCore Imported/DA Layer**: delivery via in-core segment import
  - B's report names A's segment root. JAM parks the report in the ready queue until the segement dependency is resolved
  - B can't accumulate before A, the ring still guards cases when `A` is rejected
  - If `A` segments never become available, `B` package isn't refined and the slot isn't burned
  - Needs to a segment framing to export the speculative messages, which can land at a later time
  - latency: **Same as T0/T1**
  - race condition: doesn't apply since `B`'s report has a prerequisite on `A`

- **Tier 4: Fused / SuperChains**
  - Both candidates are bundled in the same work package, utilizing the same core
  - Ordering is resolved while creating the work package
  - Parachain Service must ensure that if multiple candidates are bundled in the same work package
  the whole group is declined if one of them fails
  - If candidates are built by different collators, they must negotiate via p2p protocols which one is trusted with the package

## 3. Forced Recovery

`parachain_set_head` rolls back A's enacted history causing two side effects:
- Every consumer channel desyncs. The sender's next root will not extend the roots consumers are already following.
- Deliveries from rolled-back blocks cannot be undone. If A enacts "burn, then mint X on B" and B delivers it before
  the rollback, B's supply is permanently inflated. No layer can retract a delivered message.

A `parachain_set_head` that overwrites a live chan head will clears A's settlement ring.
If any candidate attempts to consume the abandoned history, it will reference a root that is no longer in the ring.
Because it cannot settle, it is buffered and simply expires after `K` slots.

Everything beyond this is handled via off-chain policy. Forced recovery is a runbook procedure, and Coretime ensures
an abandoned history is never silently resumed. The runbook treats a forced `set_head` as a trigger to reset all inbound channels:
1. Consumers freeze a channel upon seeing a ring clear or a non-extending root.
2. Prefetched messages are discarded.
3. The channel resumes only through an explicit reopen

Before reopening the channel, the sender must reconcile what was delivered under the abandoned roots and compensate its counterparties.
Without this, every recovery turns the sender into an inflation source. When operation resumes, re-sent messages may overlap with ones
already delivered, but previously delivered messages always stand.

> Note: A dormant chain's ring persists, meaning its final messages remain drainable at the Enacted tier.
The `parachain_clean_up` routine removes the ring, permanently ending drainability.

## 4. Super Chains

A superchain pair consumes each other's speculative output every step (A consumes B in this block and B consumes an earlier A block).
Because of how it's designed, this loop works safely on JAM.

> Why not mutual JAM `prerequisites`?
> An honest author cannot even encode the cycle. `P_A` would need `hash(P_B)` which needs `hash(P_A)`
> Colluding guarantors *can* put mutually-referencing reports on chain, but those park in the ready queue,
> never release, and are silently discarded at the epoch boundary.

- **Solution 0: Fusion** *(the plan)*: both candidates as work items of one package on one core.

  The core idea is to bundle both candidates into a single work package and run them on the same core.
  The PS must guarantee all or nothing, either all succeed or none succeed.

  To prevent cycles, we enforce strict orders: B goes first, then A.
  - B runs and looks at A's previous state
  - A runs and looks at B's current state (by reading B's header SPMG)

  Since they share the same core, they are also capped in terms of resources. Both candidates
  must share the ~13.8 MiB and 5 billion gas limit. The most likely bottleneck would be the tiny 48 KiB limit for the result.

  Securing the link between A and B inside the core:
  - in core: We put B's header at a fixed offset. A reads and checks the `R_B` hash from the header digest. 
  - backup: Add a new digest field that carries A's claimed `(sibling, root)` and rely on Accumulate to double check it
  against B's `spec_msg_provides`.

- **Solution 1: Ordered pair**
  If a single core limit becomes an issue, we can separate them onto two cores but force synchronization by `prerequisites`.

  B adds A's work hash as prerequisite and will never accumulate before A. Then, A can import the B's exports.
  The timing here is a bit fragile, B must follow A in the exact slot or at most within last 8 slots. If B misses the
  report is silently discarded at epoch boundary. 

- **Solution 2: Asymmetric parking**

  This is a fallback logic for when things get out of sync.
  If A is enacted, but B fails to enact, the system won't enact A until B can enact.
  A is parked for a timeout and if B takes too long or never arrives, A is dropped as well.


## 5. Alternative Verification

It applies only to the `Enacted` tier and is not part of the MVP.

The RefineContext carries the anchor's accumulation-output-log super peak, authenticated by JAM.
Verification happens in-core, in the receiver parachain's Refine. Since speculation happens at
enactment, the super peak commits to what PS accumulated at the sender's enacting block.

During Accumulate, at the same point where a `Provides` root is pushed into the sender's ring,
PS also emits `Leaf { para_id, streams_root }` into a per-block provides tree. Its root is
committed next to the changed-heads root in PS's accumulation output:

```
output = keccak("ps-out-v1" ++ heads_root ++ provides_root)
```

A provides leaf exists only for a candidate that enacted, and it carries exactly the value the
sender reported via `set_provides_root`: the same value ring settlement checks. No header is
involved anywhere.

The changed-heads *tree* stays byte identical, but the output commitment changes shape: every
existing belt consumer (light clients, bridges) that walked `belt leaf → heads_root` directly
must be upgraded to supply the `provides_root` sibling and the version tag. This is a
coordinated format migration, not a drop-in change.

**Tree construction (consensus rules).** The provides tree is a binary keccak tree with
mandatory domain separation:

- leaf: `keccak(0x00 ++ para_id ++ streams_root)`
- interior node: `keccak(0x01 ++ left ++ right)`

Leaves are sorted by `para_id`; a para enacting multiple candidates in one block contributes
multiple leaves, ordered by accumulation order. A single-leaf tree's root is the leaf hash. A
block in which no para provides commits the constant
`EMPTY_PROVIDES_ROOT = keccak("ps-provides-empty-v1")`, so the output shape never varies and a
heads-only commitment can never be reinterpreted. The in-block `(service, output)` pairs tree
uses the same leaf/interior tags. The tags are not optional: without them a 64-byte leaf
encoding is byte identical to an interior preimage and interior-as-leaf forgeries open up.
Sorted leaves also make non-membership provable ("para A provided nothing at this block"),
usable for the §8 channel-freeze policy.

Walking the super-peak:

```
anchor super-peak → belt leaf → (PS, output) pair → provides_root → Leaf { para_id, streams_root }
```

### 5.1 Proof Format

Every path carries its position; side bits at each level derive from the index, so hashing is
never commutative and every leaf is position bound.

```rust
/// Depth caps, enforced before any hashing. An oversize or malformed proof
/// aborts Refine with `RefineLog`; no panics, checked index arithmetic only.
const MAX_BELT_DEPTH: usize = 64;     // u64 leaf index bounds the MMR
const MAX_PAIRS_DEPTH: usize = 16;    // services accumulating per block
const MAX_PROVIDES_DEPTH: usize = 16; // provides leaves per block

struct EnactmentProof {
    /// Position of the enacting block's belt leaf in the accumulation-output log.
    belt_leaf_index: u64,
    /// Intra-peak siblings, `<= MAX_BELT_DEPTH`.
    belt_peak_path: Vec<Hash>,
    /// Remaining peak bag, folded per `belt_leaf_index`, `<= MAX_BELT_DEPTH`.
    belt_peaks: Vec<Hash>,
    /// Position of the `(PS, output)` pair in the block's pairs tree.
    pairs_index: u16,
    /// Siblings in the pairs tree, `<= MAX_PAIRS_DEPTH`.
    pairs_path: Vec<Hash>,
    /// PS's changed-heads root for that block, the sibling of `provides_root`
    /// in the output commitment.
    heads_root: Hash,
    /// Position of the provides leaf.
    provides_index: u16,
    /// Siblings in the provides tree, `<= MAX_PROVIDES_DEPTH`.
    provides_path: Vec<Hash>,
}
```

Verification, in the receiver's Refine:

1. Enforce the depth caps and decode; failure aborts with `RefineLog` before any hashing.
2. Recompute `leaf = keccak(0x00 ++ para_id ++ streams_root)` from the verifier's own
   `Requires` target. Nothing semantic is ever extracted from the proof.
3. Walk `provides_path` (sides from `provides_index`) up to `provides_root`.
4. Recombine `output = keccak("ps-out-v1" ++ heads_root ++ provides_root)`.
5. Recompute the pair leaf from the verifier's own PS service id constant and `output`, walk
   `pairs_path` (sides from `pairs_index`) up to the block's belt leaf.
6. Walk `belt_peak_path` and fold `belt_peaks` per `belt_leaf_index` up to the anchor super
   peak; compare against the RefineContext value.

Every step is a hash membership check on values the receiver already holds; a proof cannot
settle any root other than the one being asked about.

**Service id binding is the security of the scheme.** Every service fully controls its own
output bytes, so any attacker can register a service and emit
`keccak("ps-out-v1" ++ x ++ fake_provides_root)` where the fake tree contains
`Leaf { victim_para, evil_root }`. The only thing separating that from a universal forgery is
step 5: the pair leaf is recomputed from the PS service id the verifier already knows, never
read from the proof.

**Replay.** Settlement (ring or proof) only ever answers "was this root enacted". Which
messages get consumed is guarded solely by the receiver's consumption frontier, enforced by
its registered PVF: stream positions are append-only, and a `parachain_set_head` reverts the
frontier and the consumption effects atomically, so double delivery is impossible while both
endpoint histories are continuous. The discontinuous cases are Known Limitations below.

### 5.2 Costs

Proofs ride in the work item payload next to the Lift proofs, are consumed in Refine and
discarded. Only the resulting `(ParaId, StreamsRoot)` entries (~36 B each) reach the digest;
Accumulate performs zero settlement reads. The bytes are paid in package size, DA erasure
coding and audit re-execution, not in the 48 KiB report.

Path length grows with chain age, not activity. At ~2^26 belt leaves (about a decade of 6 s
blocks) a proof is ~2.5 KiB per consumed source; the depth caps bound it at ~5.2 KiB. Naive
full fan-in (32 sources, §1) is ~80 KiB, ~165 KiB at the caps (~1.2% of the package budget).
Sources enacted in the same block share `belt_*`, `pairs_*` and `heads_root`, so the wire
format groups proofs by enacting block: each co-enacted source adds only its provides path
(~450 B), bringing full fan-in to ~16 KiB in the common case. Verify gas is at most ~161
keccaks of 64 B per source.

Proofs verify against the anchor super peak, so they are anchor specific: a resubmission that
re-anchors (§2 note) needs freshly generated proofs. Collator tooling must not cache them
across anchors.

### 5.3 Trust Model

Ring settlement trusts an Accumulate state read. Here the load-bearing check runs in Refine,
so the trust root moves to ELVES-audited in-core execution. The verification must live in
PS's own Refine logic, not in the §1 wrapper checks that are explicitly collator-vs-collator.
The digest carries a PS-attested flag marking entries as proof-settled, and Accumulate skips
the ring read exactly for those: while both mechanisms coexist, every `Requires` entry is
settled by exactly one of them, never neither.

### 5.4 Why not walk to the header

An earlier form of this proof walked `belt leaf → heads_root → Leaf { para_id, head_hash }
→ header preimage → SPMS digest → StreamsRoot`, carrying the full sender header in the PoV.
It had provable defects:

- **Unbound root.** The ring is fed from `set_provides_root` (pallet storage); the SPMS header
  digest is written separately by the sender runtime. PS is header agnostic and never checks
  they agree, so a buggy or malicious runtime can settle one root on-chain while its header
  proves a different one in-core. The two settlement paths diverge silently and consumers can
  be made to settle a root that never entered the settlement system.
- **Bundles break the walk.** For bundled PoVs the wrapper carries the last produced `Provides`
  forward when later inner blocks send nothing. Only the candidate-boundary head enters the
  changed-heads tree, so the enacted header's digest need not contain the settled root at all.
  For such candidates the proof simply has no path to the root: verification fails on honest
  history.
- **Opaque preimage.** `head_hash` commits to opaque head-data bytes. Decoding them as a header
  with a known digest layout is a format assumption PS nowhere enforces, and every proof hauls
  up to 4 KiB of header into the PoV just to read 32 bytes out of it.

Pushing the root as its own PS-authored leaf removes all three: the leaf is created by PS
itself, at enactment, from the host-call value, for the effective candidate-boundary root.

### 5.5 Known Limitations

**1. Canonicality.** The accumulation-output log is append-only, so this proof shows a root
*was* enacted, permanently and with unbounded reach (no `W_MAX` window). It cannot show the
enacting history is still canonical: `parachain_set_head` (§8) clears the ring precisely so
abandoned roots stop settling, and no rollback can retract a belt leaf. As a full replacement
for ring settlement this scheme must be paired with a per-para generation key: the receiver's
Accumulate checks the sender's current generation at settlement, and the generation bumps on
every `parachain_set_head` that overwrites a live head, on `parachain_clean_up`, and on para
id (re)registration. Bumping is coarse: it also invalidates proofs for the still-canonical
prefix. That matches ring semantics, since a ring clear drops prefix roots too, and §8's
runbook already freezes and explicitly reopens every channel after recovery. Alone, without
the generation key, this scheme is only safe for histories Forced Recovery has never touched.

**2. Generation TOCTOU.** The proof verifies in Refine against an anchor up to `C_H = 8`
slots stale; the generation is checked at Accumulate. A recovery landing in that window makes
a candidate fail on-chain after passing in-core: the slot is burned and, unlike ring
settlement, §3 buffering cannot rescue it, because the verification is already fixed at
Refine. Same acceptance class as the Coretime-ordering window noted in §2.

**3. Para id reuse.** `parachain_clean_up` removes the ring and permanently ends drainability
(§8); a belt leaf can never be removed. Without the generation bump on cleanup and
re-registration above, a para id re-registered to a new chain inherits provable
`Leaf { para_id, root }` history from its predecessor, and receivers can be made to consume
the dead chain's messages as the new one's.

**4. Enacted tier only, no buffering.** A root missing at Refine fails the candidate in-core:
there is nothing for §3 buffering to wait on, and §4's sort cannot help. The scheme cannot
serve Tiers 1-2, which settle at Accumulate time by construction. "Full replacement" is only
meaningful if those tiers keep the ring or are dropped.

**5. Proof-data liveness.** The ring needed only current state; generating a proof for an old
root requires reconstructing that block's provides tree from chain history. Unless an indexer
retains them, old roots stay theoretically provable but practically unsettleable.
