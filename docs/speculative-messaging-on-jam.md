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
  spec_msg_requires: BoundedVec<(ParaId, StreamsRoot), 64>,
}
```

### Producing Sender `Provides`

The sender runtime maintains its stream MMR frontiers and the commitment tree. After all inner blocks in
a candidate have executed, the validation wrapper reports the final changed root through
`set_provides_root`. It may do so once per `Refine`.

It costs 33 bytes when present.

For a bundle, the wrapper carries the last changed root forward even when later inner blocks send nothing.
Intermediate roots are not settlement entries. A consumer that used an intermediate boundary must lift
its endpoint to the candidate-boundary root before it can settle.

### Producing Receiver `Requires`

The validation wrapper verifies the PoV lifts and declares one `(ParaId, StreamsRoot)` per source parachain,
via `set_requires_root` host call in Refine. PS Refine copies the entries into `spec_msg_requires` and
ensures all sources are unique and bounded.

If the runtime is buggy or verifications are disabled, PS does not guarantee that the PoV lifts or stitches
actually result in the provided `Requires`. The system relies on the Accumulation phase to verify that the
`Requires` are present in the settlement ring.

```rust
/// Declares the candidate's requires entries, one per consumed source.
/// One call carries the whole set. May be called at most once per Refine.
fn set_requires_root(entries: &[(ParaId, StreamsRoot)]) -> ();
```

> PS doesn't enforce the validity of `Requires`. It remains header agnostic (ie, no decoding) and strictly
> moves opaque 32byte roots.
>
> Moving the verification into the PS Refine wrapper doens't change the security guarantees.
> Instead, PS Refine would simply check the collator provided consumption record against
> the collator provided PoV lifts.

At 36 bytes each, full fan-in costs ~2.3 KiB of the 48 KiB report.

## 2. Settlement Ring

> Note: On polkadot missing the settlement check emplies that the lifts are regenerated and candidate
> is resubmitted. On JAM, the re anchoring changes the package hash and the cotretime slot is burned.

Every `Requires` source must exist and its root must be present in the ring.

The settlement ring represents the last `W` enacted roots per para, keyed at `0x09 ++ para_id`.

```rust
/// Keyed by `0x09 ++ SCALE(para_id)`. Appended newest-last, ensuring strict slot-ordering by construction.
///
/// A root is evicted `D` slots after the parachain pushes its *subsequent* root. Until then, 
/// it remains in the ring buffer. This dynamic retention guarantees that the latest root of an 
/// idle or on-demand parachain never expires, while allowing elastic scaling to adjust the buffer's size.
/// 
/// A block with no new messages pushes nothing.
/// The `parachain_clean_up` delets the ring as well.
spec_msg_recent_provides: Map<ParaId, BoundedVec<(StreamsRoot, Slot), W_MAX>>

/// The upper bound on the number of slots between a lift's retarget (package build) 
/// and its accumulation.
///
/// Represents the sum of:
///  `C_H` (anchor recency) + Availability timeout (`U`) + consumer pipeline depth +
///   censorship margin.
///
/// Calculation: `D = 8 + 5 + 9 + 8 = 30` (provisional on `U` with 2 margin slots).
pub const D: u32 = 32;
/// Assumed ceiling on concurent cores per parachain. This is not enforced. However,
/// JAM has 341 cores total.
///
/// If Coretime allocates more than `MAX_CORES` to a parachain, the ring will hit `W_MAX`
/// and safely fall back to oldest-first eviction.
pub const MAX_CORES: u32 = 20;

/// The absolute maximum capacity of the ring buffer for a single parachain.
///
/// This size accommodates a parachain pushing one root per core, per slot, for `D` slots.
pub const W_MAX: u32 = MAX_CORES * D;
```

Under stable block production, the size used is roughtly the number of cores the parachain owns.
Evection doesn't happen after a fixed amount to account for on-demand parachains.

If the buffer contains `[(root A, slot 10)]`, `root A` remains valid indefinitely.
The eviction only begins when the parachain pushes its next root. When `root B` lands at `slot 50`,
root A is scheduled for eviction at `slot 50 + D`.

> Note: A forced rollback (`ParachainSetHead` from the Coretime chain) isn't applied instantly. Its an upward message,
replayed at step 7 when the Coretime chain's own package accumulates. Packages accumulate in order within a block,
so if B's package sits before Coretime's in that order: B's settlement finds A's root still in the ring and B enacts.
Moments later in the same round, Coretime's replay rolls A back and clears the ring. B consumed a history that died within the same block.
Since this represents a tiny window of exposure relying on accumulation ordering and only reachable via Coretime intervention, at MVP
this is accepted rather than solved (ie draining Coretime's commands before any settlement runs).

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
