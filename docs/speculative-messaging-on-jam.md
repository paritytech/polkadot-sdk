# Speculative Messaging on JAM

The [Parachain Service](https://github.com/paritytech/polkadot-sdk/blob/afe236db6a21ac4ca4bef93a2e7375002a068585/designs/parachain-service-on-jam/parachain-service-on-jam.md#3-the-parachain-service) needs a robust messaging system to replace the older HRMP-style payloads, and [Speculative Messaging](https://github.com/paritytech/polkadot-sdk/blob/9d0a0daee40e6e350209aaf4b3e3bdf1fb9a8793/docs/speculative-messaging-design.md) is our primary candidate for JAM.

Here is a breakdown of exactly what needs to change to make Speculative Messaging work smoothly on JAM, ensuring every Polkadot feature has a clear, explicit JAM backport.

> For MVP, chains speculate at `Enactment` tier. Sender parachain `A` writes its `Provides/StreamsRoot` entry
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

```
anchor super-peak → belt leaf → (PS, heads_root) → Leaf { para_id, head_hash }
  → header preimage → SPMS digest → StreamsRootA
```


## Changes at Accumulation Tier MVP

We are releasing SpecMsg with Accumulation Tier (HRMP parity) from day 0. The changes needed:

1. Sender header digest: `DigestItem::Consensus(SPMS_ENGINE_ID, enum SpmsDigest)`.

This payload is the leaf that every enactment proof walk terminates on. To ensure we can change the
digest layout in the future while having coexisting versions, the `SpmsDigest` is a versioned enum. 

For bundled PoV the Parachain Service commits the head a parachain ended the block with. Therefore, only the
final inner block's root is ever provable and intermediate roots can never be consumed or settled.

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

The field is set via `set_requires_root` host call at most once per `Refine`. Each entry names the
consumed root itself, so settlement can check it against the source's ring.

At 36 bytes each, full fan-in costs ~2.3 KiB of the 48 KiB report.

This must be a digest field and not UpwardMessage, because UMP replays after the head write (so it can't gate enactment).

3. Settlement: ring `spec_msg_recent_provides` and `Requires` check

The settlement ring represents the last `W` enacted roots per para, keyed at `0x09 ++ para_id`.

It is check and ring is delivered by the MVP implementation. If the enacted head carries an SPMS digest
its root is pushed into the senders ring only if different from the newest entry. The ring evicts the
oldest entry beyond `W`.

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

> Note: A forced rollback (`ParachainSetHead` from the Coretime chain) isn't applied instantly. Its an upward message,
replayed at step 7 when the Coretime chain's own package accumulates. Packages accumulate in order within a block,
so if B's package sits before Coretime's in that order: B's settlement finds A's root still in the ring and B enacts.
Moments later in the same round, Coretime's replay rolls A back and clears the ring. B consumed a history that died within the same block.
Since this represents a tiny window of exposure relying on accumulation ordering and only reachable via Coretime intervention, at MVP
this is accepted rather than solved (ie draining Coretime's commands before any settlement runs).

## Speculation Tiers

- **T0 Accumulate / Consume enacted roots (MVP)**: Parity with HRMP
  - 1 or 2 slots from guarantee to accumulate, plus 1 or 2 for anchor lag
  - root verification in-core via the output-log proof; the requires entry carries
    `(ParaId, StreamsRoot)` and settlement checks the root against the source's ring — the
    rollback brake, since a forced rollback clears the ring

- **T1a Backed / PrefetchHint**: prefetch on backed with zero consensus change.
  - node reads guaranteed but not yet accumulated reports to warm cache
  - still consumes T0 enacted roots

- **T1b Backed / Active**
  - saves 1 or 2 slots
  - needs no consensus change
  - `A` guaranteed report can die and `B` burns its slot (ie `A` must land first or `B` burns a slot)
  - Same block `A -> B` is possible since Accumulation happens in order and A's ring push lands before B's settlement runs.

- **T2 Best Block**
  - saves 2 to 4 slots
  - needs offchain announce protocol, offchain verification and header-digest read path for package boundary roots
  - Recommended: high-fan-in consumption (64 sources like every tier up to here) at the lowest latency; T3 matches
    the latency but caps fan-in at 8. Needs LLv2 ACK/Slashing on JAM
  - Optional `prerequisites` for ordering: B's package declares A's hash. `J = 8` caps prerequisites plus segment
    lookups per report, not consumed sources, so fan-in stays at 64. The 64 to 8 drop only happens at Tier 3.
  - Work package hash is distributed via `/spec-msg/announce` notification protocol.

- **T3 InCore Tier**
  - B imports A's exported segment
  - delivery becomes in-core and deterministic with `import_segments()`
  - we drop the maximum communication sources from 64 to 8 (`J` constant), but keep the same latency as *T2 Best Block*
  - Recommended: low-fan-in (8 consumed sources), latency critical (parity with T2) and segment import is straightforward without LLv2 ACK/Slashing

- **T4 SuperChains**
  - Both A and B candidates are bundled in the same package
  - Needs a PS service rule that if any member fails the whole group is declined.

## Verification

B's PoV carries per consumed source an `EnactmentProof`:

```rust
struct EnactmentProof {
    /// Path from the anchor's super-peak to the enacting block's entry in the
    /// accumulation-output log.
    /// 
    /// TODO-GrayPaper: We need the exact format and hash function.
    belt_path: Vec<u8>,

    /// PS's per-block accumulation output for that block.
    heads_root: Hash,

    /// keccak path in the changed-heads tree to `Leaf { para_id, head_hash }`.
    heads_path: Vec<Hash>,

    /// Full header preimage; must hash to `head_hash`.
    /// The SPMS digest `streams_root` is extracted out of it.
    header: Vec<u8>,
}
```

Verification occurs in the Parachain Service Refine wrapper via
`verify_enacted_root(para_id, proof) -> Option<StreamsRoot>` and not in the guest code.
Malformed proofs abort Refine with `RefineLog::InvalidEnactmentProof`.

Then the message Lifts verify against the root as on Polkadot. Settlement (§5.1 step 6) re-checks the
named root against the source's ring — never message verification, which
is inherently guest-side. A guest that skips lift verification is broken either way.

The anchor must be matched against the last 8 JAM blocks at guarantee time. The 6s slots needs roughly 3-4 slots out of the
8 for state root lag, building, distribution and inclusion. This leaves approximately 4-5 slots to match against. Missing the
window means the re-anchoring swaps the proof envelope, not the block. Since MMR peaks are prefetched, lift generation also
remains local. This needs to take into account AURA authorizer which makes the anchor double as the slot claim, which could 
shrink the collator window from 8 to its own rotation run.

PoV cost is about 4KiB per touched stream plus, per source, the belt path (log2 of chain length), the heads
path (<= log2 of core count) and A's full header preimage. For example, 128 streams and 64 sources stays well
under 1 MiB out of the ~13MiB budget.

> Unknown: The verification gas is unknown, and now needs `keccak_256` for the belt and
> heads paths, BLAKE2b for the lifts. Needs to fit inside the Refine 5 * 10^9.
> The fallback is hashing host calls for the child PVM, capped by a `MAX_VERIFICATION_HASHES` constant.

## Transport and Discovery

We have 2 delivery paths for messages:
- Primary offchain req-resp protocol `/spec-msg/exchange` which holds the messages in a node side archive. This outlives the DA window and gives unbounded catch-up.
- Secondary via PVF exports of payloads to DA as framed segments.

Since segments cannot be exported retroactively, the DA exports are used since day 0. This offers a bootnode agnostic fallback to fetch messages and
Tier 3 (in-core tier) relies on the segments. 64 KiB of messages per block would cost 0.6% of the budget.
The actual cost is erasure-coding bandwidth across validators.

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

1. B's JAM follower watches PS's per-block head commitments (§5.5) for A's leaf. A changed head carries a new
`StreamsRootA` in A's header digest: A added new outbound messages.

2. B finds A's network via JAM state keys. A published the bootnodes under `0x08 ++ SCALE((A, "bootnodes/v1"))`. 
The record contains up to 8 multiaddresses (managed by parachain itself).

3. B dials bootnodes and discovers A's archive nodes. Messages plus MMR extension proof are fetched from `/spec-msg/exchange` req-resp protocol.

4. Extension is verified against `StreamsRootA` locally and a prefix of the messages is consumed in the next block. The PoV carries the enactment proof for A's root.

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
chain's ring persists — so a dead chain's last messages remain drainable at T0 as long as its ring
stands. A `parachain_clean_up` removes the ring and ends drainability with it.

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
pending-provides hint (dormant until Tier 1a).

## 6. Trust Model

| Property | Polkadot | JAM |
|---|---|---|
| Message authenticity | PVF | PVF — unchanged |
| Root authenticity | relay consensus check | wrapper output-log proof in-core; ring root check at §5.1 step 6 |
| Provides commitment | UMP signal | header digest + §5.5 enactment commitment |
| Stream monotonicity | not enforced | not enforced — self-harm, consumer-visible |

Polkadot capabilities: same-block A -> B deferred to Tier 1b.

> **Why not JAM's `prerequisites`?**
>
> (1) Wrong granularity: a package hash orders, a StreamsRoot is content. The root must be carried regardless.
>
> (2) Report is not enactment: steps 3/5 can decline after accumulation, so compare-and-reject survives anyway.
> 
> (3) `J = 8` is too small and shared with Tier 3's segment lookups; fan-in targets 64.
>
> (4) The 8-block reporting gate makes it strictly worse than proving enacted state.
>
> **Right solution at Tier 3: segment roots are content-addressed.**

## 10. Open Questions

1. **PVM hashing throughput + metered gas ceiling**
  Verification now spans keccak_256 (belt + heads paths) and blake2 (lifts) in Refine, under a max 5 billion
  gas ceiling shared with the parachain's own execution. If guest hashing doesn't fit, the fallback is
  hashing host calls for the child PVM.

2. **`W`** at best-block depth. Now a day-0 sizing input.

  The ring is live and read by settlement from day 0, so `W` must be fixed before launch. It must be
  sized for the best-block pipeline (announce to enact), not the backed one, since the higher tiers
  reuse the same ring without a consensus change.

3. **Silent ready-queue expiry**
  If B declares a prerequisite on A and A never accumulates, B's report is dropped
  after up to an epoch with zero on-chain trace. The node must detect the drop itself and rebuild-and-retry.

4. **ParaId reuse** The ring is dropped at clean-up and recreated empty on re-registration, so a
  reused id's stale roots are never consumable. That a reused `ParaId` is a *different chain* is a
  Coretime guarantee (registration policy), not a consensus check — consumers must treat channel
  identity as managed by Coretime.

5. **Acknowledgement/slashing on JAM**
  Tier 2's security on Polkadot rests on LLv2 ACK slashing. JAM has no equivalent

6. **CE 129**
  Can a collator without a JAM node ask another node for state? Recent-history reads are fine, but the
  enactment proof needs output-log peaks and bootnode records need hash-leaf preimages.

7. **Pin the rest** 
  JAM DA retention period is not pinned. For message recovery we'd need a real number. Similar to key derivation.

8. **Settlement gas at worst-case fan-in**
  At MVP settlement reads one ring per source (up to 64 per candidate, `W` × 32 bytes each), and
  the head write adds a ring push. We don't have JAM gas per read byte estimates. 
  
  Running out of gas in Accumulate silently drops later packages, so this design raises PS's required `min_item_gas`.

9. **Authorizer anchor policy**: AURA uses anchor to prove it's our turn.
  The same anchor is also the block all message proofs verify against.
  The current collator cannot freely pick between the 8 recent blocks anymore (only ones in the current rotation). 
  Needs to make the slot claim independent of anchor choice.

10. **Output-log citations (GP)**
  For the EnactmentProof we need: 
  - The super-peak in the refine context is validated against recent history at report inclusion.
  - Accumulation output log have a proof format and hash function
  - Newest recent history entry carries a usable super-peak or gets it one block late (decides minimum anchor age). 

## 12. Super Chains

A superchain pair consumes each other's speculative output every step (A consumes B in this block and B consumes an earlier A block).
Because of how it's designed, this loop works safely on JAM.

Solving the work package ordering:

> Why not mutual JAM `prerequisites`?
> An honest author cannot even encode the cycle. `P_A` would need `hash(P_B)` which needs `hash(P_A)`
> 
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
