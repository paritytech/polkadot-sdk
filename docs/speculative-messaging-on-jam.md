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

## Changes at MVP Tier

We are releasing with the Enacted tier (HRMP parity) from day 0. The changes needed from PS:

## 1. `Provides` and `Requires` fields on the PS work digest

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
the sender's settlement ring (`ring[A]`).

It costs 33 bytes when present.

### Producing Receiver `Requires`

The collator node picks the target `Provides` depending on the speculation tier and
consumes a prefix of messages. The runtime consumes the messages and records a `ConsumptionRecord`.
The collator then generate the Lift proofs and packages them into the PoV.

In the PS Refine, the guarantoors execute the PVF. Then, the `validate_block` wrapper reads the
`ConsumptionRecords` from the pallet storage with the Lifts from the PoV. It stitches consumption gaps
to produce a final `Requires` that is set via `set_requires_root`.

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
> Moving the verification into the PS Refine wrapper doens't change the security guarantees.
> Instead, PS Refine would simply check the collator provided consumption record against
> the collator provided PoV lifts.

At 36 bytes each, full fan-in (32 entries) costs ~1.1 KiB of the 48 KiB report.

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
  let mut cursor = spec_msg_cursor.get(para);

  // Eviction
  while cursor.head.wrapping_sub(cursor.tail) >= W_MAX {
    let to_evict = spec_msg_queue.get(&(para, cursor.tail));  // 75 B read
    let entry = spec_msg_member.get(&(para, to_evict));       // 75 B read
    if entry.seq == cursor.tail {
      // Head submitted again more recently.
      spec_msg_member.remove(&(para, to_evict));
    }
    spec_msg_queue.remove(&(para, cursor.tail));              // 75 B write
    spec_msg_member.insert((para, root), MemberEntry { seq: cursor.head}); // 75 B write 
    cursor.head = cursor.head.wrapping_add(1);
    spec_msg_cursor.set(para, c); // 47 B write
  }
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

## Buffering

A candidate that passes every check except settlement (ie, `Provides` entry
is not yet in the ring) is rejected today. The slot is burned and the work must be redone.
Buffering stores the work digest and applies it later, once the root arrives.

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

Buffering never holds up the canonical chain. An entry is never replaced or refreshed.

**Gas Model**

The buffer walk happens before the new work item that brought the gas, but it can only spend the leftover
gas that the item doesn't need for itself. Collators need to pad the work item to cover this walk.
Because the buffer is in consensus state, they can read the exact cost at their anchor and fund it perfectly.
If the gas only covers part of the buffer, the walk settles what i can and pauses. Nothing is destroyed,
and the remainder waits for the next item.
After the walk, the new item is processed. It is either applied directly if it sits on the enacted head, or
buffered as a chain extension if it builds on the tip.
Ultimately, an under gassed item will never cause the buffer to be dropped, and the buffer walk will never starve the item that paid for it.

Parachains can also push the buffer forward without submitting a new candidate. A settlement-only package carries
no work digest and no parent head. Because it has no parent head, it can't fail the parent-head check,
guaranteeing it reaches the walk phase and delivers gas.

Like normal work items, they are autorizer-gated produced by active collators . They require a core slot, refine gas and the
walk's declared gas (which is cheeper than rebuilding).

```rust
/// Refine output handled to Accumulate.
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

## Speculation Tiers

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
    - 1. Node side timed work package submission
    - 2. Parachain Service topological sort of dependencies
    - 3. (Optional): `prerequisites` fields (capped at `J = 8`)
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
    - 2. Parachain Service Buffering 

- **Tier 3: InCore Imported**: delivery via in-core segment import
  - B's report names A's segment root. JAM parks the report in the ready queue until the segement dependency is resolved
  - B can't accumulate before A, the ring still guards cases when `A` is rejected
  - If `A` segments never become available, `B` package isn't refined and the slot isn't burned
  - Needs to a segment framing to export the speculative messages, which can land at a later time
  - latency: same as `Tier 2: Announced`
  - race condition: doesn't apply since `B`'s report has a prerequisite on `A`

- **Tier 4: Fused / SuperChains**
  - Both candidates are bundled in the same work package, utilizing the same core
  - Ordering is resolved while creating the work package
  - Parachain Service must ensure that if multiple candidates are bundled in the same work package
  the whole group is declined if one of them fails
  - If candidates are built by different collators, they must negotiate via p2p protocols which one is trusted with the package

**Solutions to reduce the race condition**

1. Node side timed work package submission

The receiver sees A's report in the guarantees extrinsic and reads the work package hash, core and
`StreamsRoot` from `spec_msg_provides`.
The payloads are immediately fetched from p2p layer and verified locally. The receiver `B` work package is created but not submitted.

The `B` package is submitted once count assurance bits for A's core are near 2/3. This gives a higher chance for `A`'s report
to Accumulate within 1-2 slot delay until `B` accumulates.

- If `A` dies before `B` package is submitted, then `B` rebuilds against the latest T0 enacted root without burning the slot
- If `A` dies after `B` package is submitted, then `B` burns the slot and `B` rebuilds against T0
- If `B`'s head doesn't advance after its package accumulates, then `B` burns the slot and conservatively rebuilds against T0

2. Parachain Service topological sort of dependencies

Within one JAM block, the PS `Accumulate` processes digests in the provided order from JAM (sorted by core order).
If sender A and consumer B land in the same block, but B core is first, then B `Requires` check fails.

To ensure we are not relying on JAM provided order, PS reorders the blocks's digest.
The solution is to build a depedency graph based on the speculative `Provides` and `Requires`.

If B's `spec_msg_requires` contains `(A, root)` and A's `spec_msg_provides` is `Some(root)`, add an
edge from `A -> B`. The source `ParaId` selects A and root equality confirms the dependency. For a para
with multiple candidates in the block, also add an implicit edge from each candidate to its successor, such
that the chain order is a hard constraint of the graph.

The reorder is part of the consensus logic. Therefore, the sort must be terministic. This is done using
the Khan's algorithm. Between the nodes with in-degree zero, the digest with the smallest index in JAM's original
core order is picked. Digests not ordered by any edge keep their JAM relative order.

If a cycle is detected, PS continues with the original order from JAM. The ring will reject the candidates naturally.
Cycles are not expected since that would imply that both sides are consuming from each other at the same time.

3. Parachain Service Buffering

Instead of rejecting `B`'s candidate when the requires check fails, the PS will buffer the work digest and settles it later
once A's root is written in the ring. 
At most one digest is buffered per parachain at any given time to ensure the state can't grow.
A buffered digest expires after `K = 2` slots (TBD needs real data to size).

The storage digest is charged from B's balance and refunded on settlement or expiry.

Running the retries consume gas:
- option 1 (depends on gas usage): Retries run in the `always-accumulate` phase, paid by the protocol's gas grant and capped per block.
  Therefore, can never consume gas registered by the block's own work packages.
- option 2: B's next package reserves double the gas

## Transport and Discovery

We have 2 delivery paths for messages:
- Primary offchain req-resp protocol `/spec-msg/exchange` which holds the messages in a node side archive. This outlives the DA window and gives unbounded catch-up.
- Secondary via PVF exports of payloads to DA as framed segments add from `Tier 3` onward.

The DA exports are added from `Tier 3` and offers a bootnode agnostic fallback to fetch messages.
64 KiB of messages per block would cost 0.6% of the budget. The actual cost is erasure-coding bandwidth across validators.

**Discovery**

The para's runtime maintains its list in the existing KV store (tag `0x08`):

```rust
/// Full storage key: 0x08 ++ SCALE((para_id, BOOTNODES_KEY)).
const BOOTNODES_KEY: &[u8] = b"bootnodes/v1";

/// Ephemeral record of a parachain's active bootnodes.
struct AddressRecord {
    /// The para ID this record belongs to.
    para_id: u32,
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
    /// `("bootnode-pop-v1", jam_genesis_hash, AddressRecord::para_id, Bootnode::seq, Bootnode::addr)`
    /// This proves the node owner consents to being a bootnode for this specific broadcast.
    sig: Ed25519Signature,
}
```

The parachain service doesn't verify the `AddressRecord` fields. The admission of a new
address into the book is part of the Parachain runtime logic.

Initially the chain operator will setup the network via an explicit `--bootnodes` CLI flag. Once the chain
is registered new collators and nodes can join by reading the `AddressRecord` under the storage key.

> Note: KV writes are charged and skipped on **insufficient balance**.

**Flow**

Parachain A talks with Parachain B:

1. `B` node watches enacted heads from `A`.
  When `A` enacts a new `StreamRootA`, the PS pushes it into `spec_msg_recent_provides[A]`

2. B reads A's bootnodes from `0x08 ++ SCALE((A, "bootnodes/v1"))` and connects to an archive node.

3. B fetches messages with their MMR extension and proofs from `/spec-msg/exchange` req-response.
  Then it verifies locally the messages against `StreamsRootA`.

4. B consumes a prefix of messages and declares `Requires(A, StreamsRootA)`.
  During `Accumulate, the PS accepts B only if the root is still in A's ring.

**Pruning**

A sender may prune payloads below a watermark once the receiver block acknowledges it.
Archives serve extension proofs from any block boundary within the last 25h. The payload serving
horizon is archive policy, while settlement remains limited to roots still present in the ring.

## Forced Recovery

The `parachain_set_head` rolls back enacted history:
- each consumer channel desyncs since A's next root won't extend the old one.
- deliveries from rolled-back blocks are irreversible
    - A burned tokens and emitted "mint X" and enacted
    - B delivered before the recovery abandoned that block
    - B's supply is permanently inflated and no layer can undo it.

To mitigate this, a `set_head` call that overwrites a live head clears the para's settlement ring.
Every consumer's requires entry names the root it consumed, and the requires check (§5.1 step 6)
rejects any candidate still consuming the abandoned history — its roots are no longer in the ring.
Everything beyond that brake is Coretime's responsibility: forced recovery is a runbook procedure,
and Coretime ensures an abandoned history is never silently resumed.

The Coretime runbook must treat a forced `set_head` as "reset all inbound channels". Consumers freeze a
channel on a ring clear or a non-extending root (one that does not extend the consumer's inbound
frontier), discard prefetched messages and resume only on a fresh `open_channel`.

The delivered messages stand and the re-delivered overlap after the restart. Before reopening, the recovering chain
must communicate: what was delivered under the abandoned roots and compensate or it becomes an inflation source
for the counterparties.

> Note: a dormant chain's ring persists. The dead chain's last messages remain drainable at the
Enacted tier as long as its ring stands. A `parachain_clean_up` removes the ring and ends
drainability with it.

## 4. Execution: End to End

1. **A:** runtime appends outbound messages to per-destination stream MMRs

   - Output: one `StreamsRoot`, committed as `SpmsDigest::V0 { streams_root }` in A's own header digest.
   No host call is needed. PS Refine decodes the header and writes the root to `spec_msg_provides`.

   - PVF `export()`s payloads and node-side archives them.

2. **JAM:** report guaranteed -> available -> accumulated
    - step 6 writes A's head and pushes its `spec_msg_provides` root into `ring[A]`.

3. **B:** node follows A's enacted heads, fetches payloads (p2p exchange preferred for low latency, or DA)

   - verifies fetched messages and their stream proofs against each source's `StreamsRoot`
   - authors a block consuming stream prefixes and records the consumption
   - in-core the validation wrapper verifies the PoV-carried lifts, stitches bundle intervals and gap
   proofs, and synthesizes each source's `(ParaId, StreamsRoot)` via `set_requires_root`. Failures
   abort with `RefineLog`.
   - on-chain, §5.1 step 6 checks each named root is in its source's ring, then B enacts.

## 5. Node Stack

**Topology**, in preference order:

(1) **embedded JAM follower with state**: follows PS head commitments and settlement rings,
with no third-party liveness in the critical path (MVP production target)

(2) **CE 129 `StateRequest`** against a trusted node. Plausible pending four confirmations
(ask 16): serves non-validators, with capacity and retention requirements still to be confirmed.

(3) **RPC / light-follower fallback**.

**`ProvidesSource`:** reads A's enacted heads at imported blocks — **best, not finalized**. Block
stream, startup tip, sync gate, ring read at a recent hash,
pending-provides hint (dormant until Prefetch mode).

## 10. Open Questions

1. **W at the Enacted pipeline**
  The ring is live and read by settlement from day 0, so `W` must be fixed before launch.
  It must exceed the maximum number of distinct roots a sender can enact between the receiver
  selected a root and the accumulation phase (including elastic scaling). A sender using k cores
  may advance the ring up to k times per JAM block.

  Since we read the read for settlement, we can read up to `(num digests) * (64 candidates) * W * 32 B`.
  With hundreads of digets per block, that could reach 10+ MiB of reads.
  Then the Accumulation step might run out of gas and silently drop later packages.

2. **Silent ready-queue expiry**
  If B declares a prerequisite on A and A never accumulates, B's report is dropped
  after up to an epoch with zero on-chain trace. The node must detect the drop itself and rebuild-and-retry.

3. **ParaId reuse**
  The ring is dropped at clean-up and recreated empty on re-registration.

4. **Acknowledgement/slashing on JAM**
  The Announced tier's security on Polkadot rests on LLv2 ACK slashing. JAM has no equivalent

## 12. Super Chains

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
