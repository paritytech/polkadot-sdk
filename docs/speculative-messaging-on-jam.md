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

The settlement ring is used for monitoring rollbacks. When a `parachain_set_head` overwrites a live
head, it will also clear out the parachain's ring. Abandoned roots are no longer consumed and recovery
(resetting the channels, compensate for delivered messages) is a `Coretime` runbook procedure, not
a consensus mechanims Speculative Messaging or PS provides.

## Changes at MVP Tier

We are releasing with the Enacted tier (HRMP parity) from day 0. The changes needed:

1. Sender header digest: `DigestItem::Consensus(SPMS_ENGINE_ID, enum SpmsDigest)`.

This payload is the leaf that every enactment proof walk terminates on. To ensure we can change the
digest layout in the future while having coexisting versions, the `SpmsDigest` is a versioned enum. 

For bundled PoV, only the candidate-boundary StreamsRoot is added to the settlement ring.
Messages from inner blocks can still be consumed, but the receiver PoV must lift the stream state
to the final root. Inner blocks cannot be settled directly.

This is the sender's consensus commitment relying only on 33 bytes in its header.

```rust
/// SCALE-encoded payload of `DigestItem::Consensus(SPMS_ENGINE_ID, ..)`: 33 bytes.
enum SpmsDigest {
    /// Encodes to u8.
    V0 {
        /// The root of the sender's outbound message streams.
        streams_root: H256
    },
}
```

2. Digest fields: `spec_msg_requires: BoundedVec<(ParaId, StreamsRoot), 64>` on the PS work digest

The `spec_msg_requires` must not be chosen by the parachain block. Otherwise it can state any consumption.
The block records what is consumed, then the PS Refine wrapper verifies the PoV carried lift,
stitches bundle intervals and gap proofs, and synthesizes a `(ParaId, StreamsRoot)` per source.

The PS Refine should write `spec_msg_requires` directly after verifying the consumption records and PoV lifts.

To ensure canonical encoding and uniqueness, the PS Refine wrapper produces unique entries sorted by `ParaId`.
The Refine phase rejects malformed input.

`spec_msg_requires` is a digest field, because UMP signals are replayed after the head write (so it can't gate enactment).
At 36 bytes each, full fan-in costs ~2.3 KiB of the 48 KiB report.

3. Settlement: ring `spec_msg_recent_provides` and `Requires` check

The settlement ring represents the last `W` enacted roots per para, keyed at `0x09 ++ para_id`.

It is check and ring is delivered by the MVP implementation. If the enacted head carries an SPMS digest
its root is pushed into the senders ring only if different from the newest entry. The ring evicts the
oldest entry beyond `W`.

> `W` is no longer extra pipeline headroom. It is the window against which `Requires` must match.
If a package `Requires` is not in the ring, that slot is lost. On polkadot, a stale candidate
can retry for free. On JAM, each retry costs another slot.

A forced `parachain_set_head` that overwrites a live head will also clear the ring.

```rust
/// Keyed `0x09 ++ SCALE(para_id)`.
/// 
/// Step 6 pushes the enacted root at the head write iff != newest entry, evicts oldest.
/// Cleared when a forced `parachain_set_head` overwrites a live head.
/// Read by settlement, never proved.
spec_msg_recent_provides: Map<ParaId, BoundedVec<StreamsRoot, W>>
```

The ring is charged from the baseline of the parachain. Passing the settlement ring check implies that
the candidate will enact. Every requires entry source must exist and the named root must be present in the ring.
The candidate is rejected silently and the receiver `B` monitors offchain this behaviour, similar to
`parent-head` or `check-code` failures. Otherwise, letting rejected candidates append `AccumulateLogs` would let junk
candidates push out valuable entries.

 Before processing a digest, PS charges its worst-case ring-read cost against that item’s declared accumulation gas. The
 `W` should be reasonable bounded and the scan reads the newest entries first stopping on the first match.

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
  - latency: 1 or 2 slots from guarantee to accumulate, plus 1 or 2 for anchor lag (between 12 and 24+ seconds)
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

The receiver sees A's reportin the guarantees extrinsic and fetch the work package hash, core and `StreamsRoot` from the header digest.
The payloads are immediately fetched from p2p layer and verified locally. The receiver `B` work package is created but not submitted.

The `B` package is submitted once count assurance bits for A's core are near 2/3. This gives a higher chance for `A`'s report
to Accumulate within 1-2 slot delay until `B` accumulates.

- If `A` dies before `B` package is submitted, then `B` reanchors against T0 latest enacted root without burning the slot
- If `A` dies after `B` package is submitted, then `B` burns the slot and `B` rebuilds against T0
- If `B`'s head doesn't advance after its package accumulates, then `B` burns the slot and conservatively rebuilds against T0

2. Parachain Service topological sort of dependencies

Within one JAM block, the PS `Accumulate` processes digests in the provided order from JAM (sorted by core order).
If sender A and consumer B land in the same block, but B core is first, then B `Requires` check fails.

To ensure we are not relying on JAM provided order, PS reorders the blocks's digest.
The solution is to build a depedency graph based on the speculative `Provides` and `Requires`.

If B digest contains `spec_msg_requires` towards `A` and `A` has the same `StreamsRoot` as B's `Requires`, the we have
an edge from `A -> B`. Edges are created by `ParaID` only. Then we run topological sort on the edges. Everything
unrelated to speculative messaging remains unchanged.

If a cycle is detected, PS continues with the original order from JAM. The ring will reject the candidates naturally.

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
Archives serve extension proofs from any block boundary within the last 25h. Since any enacted root is provable regardless of age,
the payload serving horizon is archive policy rather than a protocol window.

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

> Note: abandoned enactments stay provable forever since the output log is append-only, and a dormant
chain's ring persists. The dead chain's last messages remain drainable at the Enacted tier as long
as its ring stands. A `parachain_clean_up` removes the ring and ends drainability with it.

## 4. Execution: End to End

1. **A:** runtime appends outbound messages to per-destination stream MMRs

   - Output: one `StreamsRoot`, committed as `SpmsDigest::V0 { streams_root }` in A's own header digest.
   Nothing else — no host call, no work-digest field.

   - PVF `export()`s payloads and node-side archives them.

2. **JAM:** report guaranteed -> available -> accumulated
    - step 6 writes A's head and pushes the enacted root into `ring[A]`. The output log carries the
      head under every later super-peak.

3. **B:** node follows A's enacted heads, fetches payloads (p2p exchange preferred for low latency, or DA)

   - selects an anchor (best imported block, within its authorizer's eligible set — any anchor at or after
   each source's enacting block works)
   - authors a block consuming stream prefixes, reserving the proof envelope as `proof_size`, and names each
   source's `(ParaId, StreamsRoot)` via `set_requires_root`
   - in-core the wrapper walks each enactment proof from the anchor's
   super-peak and the guest verifies lifts against each source's `StreamsRoot`. Failures abort with `RefineLog`.
   - on-chain, §5.1 step 6 checks each named root is in its source's ring, then B enacts.

## 5. Node Stack

**Topology**, in preference order:

(1) **embedded JAM follower with state**: follows the output log and PS head commitments,
generates enactment proofs locally, no third-party liveness in the critical path (MVP
production target)

(2) **CE 129 `StateRequest`** against a trusted node. Plausible pending
four confirmations (ask 16): serves non-validators, 64 reads/slot/consumer load, state at
anchor − 8 retained, hash-leaf preimages for `BootnodeRecord` (3) RPC / light-follower
fallback.

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
  against the SPMS digest in B's new head data.

- **Solution 1: Ordered pair**
  If a single core limit becomes an issue, we can separate them onto two cores but force synchronization by `prerequisites`.

  B adds A's work hash as prerequisite and will never accumulate before A. Then, A can import the B's exports.
  The timing here is a bit fragile, B must follow A in the exact slot or at most within last 8 slots. If B misses the
  report is silently discarded at epoch boundary. 

- **Solution 2: Asymmetric parking**

  This is a fallback logic for when things get out of sync.
  If A is enacted, but B fails to enact, the system won't enact A until B can enact.
  A is parked for a timeout and if B takes too long or never arrives, A is dropped as well.

## Apendix

This sections highlights an alternative to the onchain ring settlement that is valid only
for the Enacted tier (MVP).

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
