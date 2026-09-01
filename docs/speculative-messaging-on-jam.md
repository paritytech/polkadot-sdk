# Speculative Messaging on JAM

The [Parachain Service](https://github.com/paritytech/polkadot-sdk/blob/afe236db6a21ac4ca4bef93a2e7375002a068585/designs/parachain-service-on-jam/parachain-service-on-jam.md#3-the-parachain-service) needs a robust messaging system to replace the older HRMP-style payloads, and [Speculative Messaging](https://github.com/paritytech/polkadot-sdk/blob/9d0a0daee40e6e350209aaf4b3e3bdf1fb9a8793/docs/speculative-messaging-design.md) is our primary candidate for JAM.

Here is a breakdown of exactly what needs to change to make Speculative Messaging work smoothly on JAM, ensuring every Polkadot feature has a clear, explicit JAM backport.

> For MVP, chains speculate at the `Enacted` tier. Sender parachain `A` writes its `Provides/StreamsRoot` entry
> inside a `recent_provides` settlement ring (bounded vector). Receiver `B` monitors the ring, fetches messages from `A` and targets its
> `Requires` field agains the latest `recent_provides` entry. In the `Accumulate` phase, the PS ensures that
> all `Requires` are matched against their respective `recent_provides` entries.

Therefore, we introduce on-chain settlement from the MVP. The sender `StreamsRoot` is pushed to the ring,
the consumer `Requires` field (ie `(ParaId, StreamsRoot)`) is checked against the ring. The same
mechanism handles settlement for all speculation tiers.

## Contents

1. [`Provides` and `Requires`](#1-provides-and-requires)
2. [Settlement Ring](#2-settlement-ring)
3. [Buffering](#3-buffering)
4. [PS Topological Sort](#4-ps-topological-sort)
5. [Bootnodes Discovery via Para-Owned KV](#5-bootnodes-discovery-via-para-owned-kv)
6. [Speculation Tiers](#6-speculation-tiers)
7. [Execution: End to End](#7-execution-end-to-end)
8. [Forced Recovery](#8-forced-recovery)
9. [Super Chains](#9-super-chains)
- [Appendix](#appendix)

## Changes at MVP Tier

We are releasing with the Enacted tier (HRMP parity) from day 0. The changes needed from PS:

## 1. `Provides` and `Requires`

The Parachain Service work digest gains two fields:

```rust
struct ParachainWorkDigestOk {
  /// ...
  spec_msg_provides: Option<StreamsRoot>,
  spec_msg_requires: BoundedVec<(ParaId, StreamsRoot), 32>,
}
```

### Producing Sender `Provides`

The sender runtime maintains its stream MMR frontiers and the commitment tree.
When producing a block, the runtime writes the `Provides` root directly into pallet storage and the header digest.

During the PS Refine phase, guarantors execute the PVF. Once execution completes, the `validate_block` wrapper
reads the root from pallet storage and reports it via the `set_provides_root` host call.

For bundled PoVs, the wrapper carries the last produced `Provides` forward to the call, even when later inner blocks send nothing.
Intermediate roots are not settlement entries. A consumer that used an intermediate boundary must lift its endpoint
to the candidate boundary root before it can settle.

During Accumulate, the candidate's head is written for enactment. If a `Provides` root is present, it is pushed into
the sender's settlement ring (`ring[A]`). Therefore, a root enters the ring only for a candidate that enacted.

It costs 33 bytes when present.

### Producing Receiver `Requires`

The collator node picks the target `Provides` depending on the speculation tier and
consumes a prefix of messages. The runtime consumes the messages and records a `ConsumptionRecord`.
The collator then generate the Lift proofs and packages them into the PoV.

In the PS Refine, the guarantors execute the PVF. Then, the `validate_block` wrapper reads the
`ConsumptionRecords` from the pallet storage with the Lifts from the PoV. It stitches consumption gaps
to produce a final `Requires` that is set via `set_requires_root`.

For a bundle, the wrapper unions the per-block consumption records into one entry per source,
keeping the latest root per source.

PS Refine then enforces that all source parachains in the `Requires` set are unique and bounded to 32 entries.

If the runtime is buggy or verifications are disabled, PS does not guarantee that the PoV lifts or stitches
actually result in the provided `Requires`. The system relies on the Accumulation phase to verify that the
`Requires` are present in the settlement ring.

```rust
/// Declares the candidate's requires entries, one per consumed source.
/// One call carries the whole set. May be called at most once per Refine.
fn set_requires_root(entries: &[(ParaId, StreamsRoot)]) -> ();
```

During Accumulate, the system checks the settlement ring. It verifies that each declared `(ParaId, StreamsRoot)` is actually
present in the source's ring (`ring[ParaID]`). The block is enacted only if this check passes.

> PS doesn't enforce the validity of `Requires`. It remains header agnostic (ie, no decoding) and strictly
> moves opaque 32byte roots.
> Moving the verification into the PS Refine wrapper doesn't change the security guarantees.
> Instead, PS Refine would simply check the collator provided consumption record against
> the collator provided PoV lifts.

At 36 bytes each, full fan-in (32 entries) costs ~1.1 KiB of the 48 KiB report.

```rust
/// Declares the candidate's provides root
/// May be called at most once per Refine. Omitted when the candidate sent nothing.
fn set_provides_root(root: StreamsRoot) -> ();
```

## 2. Settlement Ring

> Note: On polkadot missing the settlement check emplies that the lifts are regenerated and candidate
> is resubmitted. On JAM, the re anchoring changes the package hash and the cotretime slot is burned.

Every `Requires` source must exist and its root must be present in the ring.

The settlement ring holds the last `W_MAX = 64` enacted roots per para.

The settlement ring represents the last `W` enacted roots per para, keyed at `0x09 ++ para_id`.

```rust
/// Tracks order and capacity.
///
/// Key: `0x09 ++ para_id`
///
/// Billed: 47 B (never read during settlement)
spec_msg_cursor: Map<ParaId, Cursor {
  /// Sequence of next push (wrapping).
  head: u32,
  /// Sequence of the oldest entry.
  tail: u32,
}>

/// Maps the sequence to the streams root.
/// 
/// Key: `0x0b ++ para_id ++ seq`
///
/// Billed: 75 B (Read on capacity eviction and lifecycle).
spec_msg_queue: Map<(ParaId, u32), StreamsRoot>

/// Ensures the `StreamsRoot` is present in the ring.
///
/// Key: `0x0a ++ para_id ++ root`
///
/// Billed: 75 B (Read only by settlement check).
spec_msg_member: Map<(ParaId, StreamsRoot), MemberEntry {
  /// The queue position ensuring duplicate guard and consumer hints about
  /// live chain sets.
  seq: u32
}>
```

The ring capacity is per parachain and strictly position based. A root is only evicted after the parachain pushes `W_MAX (64)`
new roots. Because eviction requires new messages, a block with no new messages pushes nothing.
After 64 roots, the ring reaches a maximum capacity of 9747 B.

**Push Logic**

If a root is repeated, the `MemberEntry` is updated with the new position, leaving the old entry in the
`spec_msg_queue`.

```rust
fn push(para: ParaId, root: StreamsRoot) {
  let mut cursor = spec_msg_cursor.get(para);                              // 47 B read

  // Eviction
  while cursor.head.wrapping_sub(cursor.tail) >= W_MAX {
    let to_evict = spec_msg_queue.get(&(para, cursor.tail));               // 75 B read
    spec_msg_queue.remove(&(para, cursor.tail));                           // delete

    let entry = spec_msg_member.get(&(para, to_evict));                    // 75 B read
    if entry.seq == cursor.tail {
      // Tail slot is this root's latest occurrence: membership expires.
      spec_msg_member.remove(&(para, to_evict));                           // delete
    }
    // else: root was re-pushed since; the newer slot keeps it alive.
    cursor.tail = cursor.tail.wrapping_add(1);
  }

  spec_msg_queue.insert((para, cursor.head), root);                        // 75 B write
  spec_msg_member.insert((para, root), MemberEntry { seq: cursor.head });  // 75 B write
  cursor.head = cursor.head.wrapping_add(1);
  spec_msg_cursor.set(para, cursor);                                       // 47 B write
}
```

| per para                | one-item ring (W = 64)          | this layout (point reads)          |
|-------------------------|---------------------------------|------------------------------------|
| settle, per `Requires`  | 2,345 B read                    | 75 B read                          |
| push                    | 2,345 B read + 2,345 B write    | 197 B read + 197 B write, 2 dels   |
| teardown                | 1 delete                        | 65 reads (~4.85 KB) + 129 deletes  |
| resident footprint      | 2,345 B, self-pruning           | 9,647 B, ratchets until teardown   |


When the `parachain_set_head` overwrites a live head, or if `parachain_clean_up` is called, the ring is cleared.
The teardown process reads the cursor and walks backwards from `head - 1` down to `tail`.
If the process runs out of gas, the revert leaves the cursor intact. This acts as a sentinel to detect incomplete
teardown.

> Note: A forced rollback (`ParachainSetHead` from the Coretime chain) isn't applied instantly. Its an upward message,
replayed at step 7 when the Coretime chain's own package accumulates. Packages accumulate in order within a block,
so if B's package sits before Coretime's in that order: B's settlement finds A's root still in the ring and B enacts.
Moments later in the same round, Coretime's replay rolls A back and clears the ring. B consumed a history that died within the same block.
Since this represents a tiny window of exposure relying on accumulation ordering and only reachable via Coretime intervention, at MVP
this is accepted rather than solved (ie draining Coretime's commands before any settlement runs).

## 3. Buffering

A candidate that passes every check except settlement (ie, `Provides` entry
is not yet in the ring) is rejected today. The slot is burned and the work must be redone.
Buffering stores the work digest and applies it later, once the root arrives.

> Note: #11883 §5.1's rejection path mentiones that "a candidate rejected at any
> step changes nothing at all". A candidate that fails only settlement will be buffered.
> The exposure is bounded by `MAX_BUFFERED_BYTES` and billed to the para balance at §6.1 rates.
> A digest carries up to 4 KiB `head_data` plus the 40 KiB upward-message budget,
> so `MAX_CHAIN_SIZE (16) * MAX_FORKS (2)` digests would be ~1.4 MiB per parachain.

The first buffered entry must target the parachain's enacted head and all other
entries must form a valid chain. We allow a maximum of two fork lanes. If a collator wants to abandon a branch,
they can offer a competing one immediately without waiting for the first to expire.
These forks can either process different messages (different `Requires`) or build a different head from the
exact same messages (same `Requires`).

Whichever fork settles first (evaluated FIFO by lane) invalidates the other. Forks are designed to be an edge case,
not the standard operating model.

Before any work items are applied, the buffered chain is walked from the front. The root parent
must still be the enacted head and the front entry must not be expired (either failure will drop the chain).
Then each entry's Accumulate is rerun against the current state. An entry that settles is applied and removed.
An entry still missing its root stays buffered and ends the walk. Any other failure drops the chain.

A chain front entry that is older than `buffered_at + K slots` is dropped when the next work item
is accumulated. Buffered bytes are billed to the para's balance and are capped at `MAX_BUFFERED_BYTES` per parachain.
An entry is never replaced or refreshed.

Buffering never holds up the canonical chain.

**Gas Model**

The buffer walk happens before the new work item that brought the gas, but it can only spend the leftover
gas that the item doesn't need for itself. Collators need to pad the work item to cover this walk.

> Note: JAM combines all the Accumulate gas for a service's work items into one total
> for the block. To charge this walk to a single item, we need a way to track individual
> gas usage inside that total pool. This is the same attribution problem flagged in #11883 §5.1
> for deferred `TransferOut` gas. The walk has to stay within the limit set by the item paying
> for it, so it will depend on how we solve attribution there.

Because the buffer is in consensus state, collators can read the walk's cost at their anchor.
The anchor is up to `C_H = 8` slots stale and the buffer can move in between, so this is an
estimate to over-provision against, not an exact figure.

If the gas only covers part of the buffer, the walk settles what it can and pauses. Nothing is destroyed,
and the remainder waits for the next item.
After the walk, the new item is processed. It is either applied directly if it sits on the enacted head, or
buffered as a chain extension if it builds on the tip.
Ultimately, an under gassed item will never cause the buffer to be dropped, and the buffer walk will never starve the item that paid for it.

Parachains can also push the buffer forward without submitting a new candidate. A settlement-only package carries
no work digest and no parent head, so no check tied to a candidate can reject it,
guaranteeing it reaches the walk phase and delivers gas.

`SettleOnly` is a new kind of payload that doesn't include a candidate body. Refine skips the PVF completely,
forgets about head declarations, and just hands back `RefineOutput::SettleOnly`. Once it hits Accumulate,
the system sees what it is and sends it straight to the buffer walk. It bypasses all the usual candidate checks.

> Note: #11883 needs adjusting around:
> - §4.1 runs the PVF no matter what, so Refine needs that early exit.
> - §4.2 insists on set_parent_head_hash and set_head every single time.
> - §3.2 assumes every work item is a full parachain candidate.


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
pub const MAX_BUFFERED_BYTES: u32 = TBD;

Map<ParaId, Cursor {
    /// The start positon.
    start: u8,
    /// Number of elements buffered.
    /// Supports two lane of forks.
    len: [u8; 2],
    /// Position where the chain diverge.
    /// The position is <= `Cursor::len[0]`. The lane 1
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
}
Map<(ParaId, u8 /*position*/, u8 /*fork lane 0/1*/), Held {
    /// The stored work digest.
    digest: ParachainWorkDigest,
    /// Validation code this entry was refined against.
    code_hash: H256,
    /// The head of the entry. Ensures a valid link is formed.
    head_hash: H256,
    /// The slot in which this buffered entry was added.
    /// The full fork dies when the front entry expires
    /// at `buffered_at + K`.
    buffered_at: TimeSlot,
}
```

## 4. PS Topological Sort

Within one JAM block, the PS `Accumulate` processes digests in the order JAM provides (core order for
newly available reports, plus ready-queue releases). If sender A and consumer B land in the same block
but B's digest comes first, B's `Requires` check misses the ring and the digest is buffered, costing
a slot of latency.

To ensure we are not relying on JAM provided order, PS reorders the block's digests.
The solution is to build a dependency graph based on the speculative `Provides` and `Requires`.

If B's `spec_msg_requires` contains `(A, root)` and A's `spec_msg_provides` is `Some(root)`, add an
edge from `A -> B`. The source `ParaId` selects A and root equality confirms the dependency. For a para
with multiple candidates in the block, also add an implicit edge from each candidate to its successor, such
that the chain order is a hard constraint of the graph.

The reorder is part of the consensus logic, therefore the sort must be deterministic. This is done using
Kahn's algorithm. Among the nodes with in-degree zero, the digest with the smallest index in JAM's
original operand order is picked. This yields the lexicographically smallest topological order:
deterministic, but not stable. A digest blocked behind a dependency lets later unrelated digests move
ahead of it. Digests with no incident edges keep their relative order

Cycles need no separate detection pass. When Kahn's zero in-degree set empties with nodes remaining,
the acyclic part is already emitted in sorted order and the remaining nodes are appended in JAM's original
order. Only the cyclic digests lose the reorder benefit, while the rest of the block is unaffected.
The cyclic digests themselves miss settlement, are buffered, deadlock on each other and expire after `K` slots.

## 5. Bootnodes Discovery via Para-Owned KV

Speculative messaging relies on two mechanisms to fetch messages:
- request-response `/spec-msg/exchange` protocol (same as v0.5 SpecMsg design)
- (from Tier 3) PVF exported payloads to DA as framed segments
  The DA exports are added from `Tier 3` and offers a bootnode agnostic fallback to fetch messages.

Therefore, the parachain must discover bootnodes before communicating on the p2p layer.
The para's runtime maintains its list in the existing KV store (tag `0x08`):

```rust
/// Full storage key: 0x08 ++ SCALE((para_id, BOOTNODES_KEY)).
const BOOTNODES_KEY: &[u8] = b"bootnodes/v1";

/// Ephemeral record of a parachain's active bootnodes.
struct AddressRecord {
    /// The timeslot after which this entire record is invalid and should be dropped.
    expires_at: Timeslot,
    /// Up to 4 active bootnodes.
    bootnodes: BoundedVec<Bootnode, 4>,
}

struct Bootnode {
    /// The network address (must NOT contain a peer ID).
    addr: Multiaddr,
    /// The public key from which the node's peer ID is derived.
    node_key: Ed25519Public,
    /// A monotonic sequence number to prevent replay attacks.
    seq: u64,
    /// Proof of possession. The node must sign:
    /// `("bootnode-pop-v1", jam_genesis_hash, para_id from the key, Bootnode::seq, Bootnode::addr)`
    /// This proves the node owner consents to being a bootnode for this specific broadcast.
    sig: Ed25519Signature,
}
```

The parachain service doesn't verify the `AddressRecord` fields. The admission of a new
address into the book is part of the Parachain runtime logic.

Initially the chain operator will setup the network via an explicit `--bootnodes` CLI flag. Once the chain
is registered new collators and nodes can join by reading the `AddressRecord` under the storage key.

> Note: KV writes are charged and skipped on **insufficient balance**.

## 6. Speculation Tiers

Each tier is named after the sender-side event the consumer trusts:
**Enacted, then Guaranteed, then Announced, then Imported and Fused**.

> Speculation risk: a race condition between A's ring push and B's requires check against the ring.
Since both candidates are independent, they have their own travel time through guaranteed, then availability,
then the accumulate pipeline. And either can stall or get rejected. If B report accumulates first, the
settlement check reads A's root before the update and B is rejected.

- **Tier 0: Enacted (Safe Baseline MVP)**:
  - consume enacted roots with HRMP parity. The requires entry carries `(ParaId, StreamsRoot)` and settlement checks the root against the ring
  - latency: 1 or 2 slots from guarantee to accumulate, plus receiver build and submission time (between 12 and 24+ seconds)
  - optimization: node-side fetches `StreamsRoot` from guanranteed but not yet accumulated reports
  - race condition: structurally impossible since `A` is already enacted. The only remaining risk is not matching the `Provides`
    against the `W` settlement ring size.

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

**Node side timed work package submission**

The receiver sees A's report in the guarantees extrinsic and reads the work package hash and core from the
report and the `StreamsRoot` from `spec_msg_provides`.
The payloads are immediately fetched from p2p layer and verified locally. The receiver `B` work package is created but not submitted.

The `B` package is submitted once count assurance bits for A's core are near 2/3. This gives a higher chance for `A`'s report
to Accumulate within 1-2 slot delay until `B` accumulates.

- If `A` dies before `B` package is submitted, then `B` rebuilds against the latest T0 enacted root without burning the slot
- If `A` dies after `B` package is submited, `B` remains buffered. `B`'s buffered digest can never settle
  and just expires at `buffered_at + K`. Then, `B` forks immediately at T0 tier and rebuilds on the second buffered lane.
- If `B`'s head doesn't advance after its package accumulates, then `B` burns the slot and conservatively rebuilds against T0

## 7. Execution: End to End

1. **A:** runtime appends outbound messages to per-destination stream MMRs
   - Output: The runtime writes `StreamsRoot` into the pallet storage (and header digest). During the PS
   Refine, after PVF execution the `validate_block` wrapper reads the root from the pallet and reports
   it via `set_provides_root`. PS never decodes the header.
   - (Tier InCore) PVF `export()`s payloads and node-side archives them.

2. **JAM:** report guaranteed -> available -> accumulated
    - step 6 writes A's head and pushes its `spec_msg_provides` root into `ring[A]`.

3. **B:** node follows A's enacted heads, fetches payloads (p2p exchange preferred for low latency, or DA)
   - verifies fetched messages and their stream proofs against each source's `StreamsRoot`
   - authors a block consuming stream prefixes and records the consumption
   - in-core the validation wrapper verifies the PoV-carried lifts, stitches bundle intervals and gap
   proofs, and synthesizes each source's `(ParaId, StreamsRoot)` via `set_requires_root`. Failures
   abort with `RefineLog`.
   - on-chain, the settlement check (§2) verifies each named root is in its source's ring, then B enacts.

## 8. Forced Recovery

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

## 9. Super Chains

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

## Appendix

This appendix records an unscoped alternative to on-chain ring settlement. It applies only
to the Enacted tier and is not part of the MVP or the design above.

### Alternative Verification

The RefineContext carries the anchor's accumulation-output-log super peak. The verification
can happen in-core of the receiver parachain. Since speculation happens at enactment, the
super peak contains the sender parachain header hash. The receiver node passes in its PoV
an enactment proof which walks the super peak to the sender header hash, plus the full sender header.
The receiver then extracts from the SPMS digest the `StreamsRoot` and compares against its `Provides`.

Walking the super-peak:

```
anchor super-peak → belt leaf → (PS, heads_root) → Leaf { para_id, head_hash }
  → header preimage → SPMS digest → StreamsRootA
```

```rust
struct EnactmentProof {
    /// Path from the anchor's super-peak to the enacting block's entry.
    belt_path: Vec<u8>,
    /// PS's per-block accumulation output for that block.
    heads_root: Hash,
    /// keccak path in the changed-heads tree to `Leaf { para_id, head_hash }`.
    heads_path: Vec<Hash>,
    /// Full header preimage that must match to `head_hash`.
    header: Vec<u8>,
}
```
