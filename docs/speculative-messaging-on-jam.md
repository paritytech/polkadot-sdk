# Speculative Messaging on JAM

 
The [Parachain Service](https://github.com/paritytech/polkadot-sdk/blob/afe236db6a21ac4ca4bef93a2e7375002a068585/designs/parachain-service-on-jam/parachain-service-on-jam.md#3-the-parachain-service) needs a robust messaging system to replace the older HRMP-style payloads, and [Speculative Messaging](https://github.com/paritytech/polkadot-sdk/blob/9d0a0daee40e6e350209aaf4b3e3bdf1fb9a8793/docs/speculative-messaging-design.md) is our primary candidate for JAM.

Here is a breakdown of exactly what needs to change to make Speculative Messaging work smoothly on JAM, ensuring every Polkadot feature has a clear, explicit JAM backport.

## Core Idea

> For MVP, if a root has already been enacted by B's anchor, B can simply prove it locally. 
> The chain never needs to be involved, meaning **no on-chain settlement step is required**.

B selects a recent JAM block to act as anchor, and includes a Merkle Proof in its PoV. 
The PoV proof attests that A's cell holds `StreamsRootA`. Validators then verify this in-core.

If the root wasn't enacted at the anchor, it cannot be proven and must be verified on-chain instead.
Ultimately, the system latency tiers dictate when to use this lightweight in-core proof or
fallback to the settlement ring.

## Changes at Accumulation Tier MVP

We are releasing SpecMsg with Accumulation Tier (HRMP parity) from day 0. The changes needed:

1. Digest fields: `spec_msg_provides: Option<StreamsRoot>` on the PS work digest.

The field is set via `set_provides_root` at most once per `Refine` and needs 33 bytes out of the 48 KiB report budget.
This must be a digest field and not UpwardMessage, because UMP replays after the head write (so it can't gate enactment).

The same `StreamsRoot` value must be byte-identical to the header digest (`DigestItem::Consensus(SPMS_ENGINE_ID, root)`).
For bundled PoVs, the call is made once with the final inner block's root. This is valid since MMRs are append-only.


2. A provides cell: one 32 byte value per parachain keyed at `0x09 ++ para_id`. 

The cell is written only by Accumulate on enactment. Since it's 32 bytes it fits directly into a JAM trie leaf,
so proving it is a pure Merkle path without a preimage, saving ~4KiB of proof per source per block vs a field inside
`ParaInfo`. The cell's storage footprint is 81 balance units (vs +33 for the in-`ParaInfo` placement). The proof size
wins over the 48 units.

Note: Accumulate writes what the digest carries without verifying the MMR extensions. Consumers are protected against
forged messages, not against a source rolling back its stream.


3. Settlement ring: The last `W` enacted roots per para keyed at `0x0a ++ para_id`. This is reserved only and priced from
day 0, but not written until Tier 1b.

Sized at `W = 48` (derived from `3 elastic cores * (8 slot eviction window + 8 slot pipeline depth)`). This number must be
fixed before baseline freezes.

```rust
/// Keyed `0x0a ++ SCALE(para_id)`.
/// 
/// From Tier 1b: step 6 pushes the enacted root iff != newest entry, evicts oldest.
/// Read only by settlement, never proved.
spec_msg_recent_provides: Map<ParaId, BoundedVec<StreamsRoot, W>>
```

The baseline footprint goes from 69,847 to 71,514 balance units. Every write and delete is balance neutral. All of this has to land before
the first parachain registers to avoid a live migration.

> Note: Only the PoV bundle final root reaches the cell and the ring. Intermediate roots stay in headers and can never be settled on chain.


Deferred changes:
- Digest field `spec_msg_requires: BoundedVec<(ParaId, StreamsRoot), N>` where `N = 64` consumed changes
- `set_requires_root` host call to set the `spec_msg_requires`

## Speculation Tiers

- **T0 Accumulate / Consume enacted roots (MVP)**: Parity with HRMP
  - 1 or 2 slots from guarantee to accumulate, plus 1 or 2 for anchor lag
  - no settlement and no requires field, verification happens with merkle proof in PoV and guest code

- **T1a Backed / PrefetchHint**: prefetch on backed with zero consensus change.
  - node reads guaranteed but not yet accumulated reports to warm cache
  - still consumes T0 enacted roots

- **T1b Backed / Active**
  - saves 1 or 2 slots
  - needs a new digest field `spec_msg_requires: BoundedVec<(ParaId, StreamsRoot), N>`
  - settlement ring gets active and settlement step consumes gas (N must be sized for gas limits)
  - `A` guaranteed report can die and `B` burns its slot (ie `A` must land first or `B` burns a slot)
  - Same block `A -> B` is possible since Accumulation happens in order and A's cell write lands before B's settlement runs.

- **T2 Best Block**
  - saves 2 to 4 slots
  - needs offchain announce protocol, offchain verification and header-digest read path for package boundary roots
  - Announcing a boundary then rebundling is adversarial resulting in `RequiresUnmet` and burned slot, which needs slashing to solve
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

B's PoV carries per consumed source a Merkle proof of the `0x09` cell at the anchor. This is verified in guest code
(ie `validate_block`) against the anchor's state root which the child PVM reads from `work_package_context()`.

**State key derivation**

The guest computes the cell's 31 byte state key itself. A mismatch results in a silent failure, therefore the construction matters:

1. `h = BLAKE2b-256(0xffffffff ++ storage_key)`. The 4 byte domain prefix marks storage values

2. Bytes 0-7 of the state key interleave the service id's little-endian bytes with the hash
(ie `[id0, h0, id1, h1, id2, h2, id3, h3]`), bytes 8-30 are `h[4..27]` taken linearly.
The interleave covers only the first eight bytes.

Two inputs the guest must source correctly:
- The PS service id is read from `work_items_summary()`
- The storage key is `0x09` followed by the ParaId as a fixed 4 byte little-endian u32, not `Compact`.

Then the message Lifts verify against the root as on Polkadot. This means that root authenticity is now enforced by
parachain guest code rather than consensus. A buggy runtime can disable the inbound authentication with no chain evidence.
The alternative is to introduce the requires field settlement at every tier.

The settlement only replaces the recent root proof, not message verification. Since message verification is done by Lifts
which verifies against `StreamRoot` by the guest in-core, a buggy guest that skips lift verification is equally broken.

The anchor must be matched against the last 8 JAM blocks at guarantee time. The 6s slots needs roughly 3-4 slots out of the
8 for state root lag, building, distribution and inclusion. This leaves approximately 4-5 slots to match against. Missing the
window means the re-anchoring swaps the proof envelope, not the block. Since MMR peaks are prefetched, lift generation also
remains local. This needs to take into account AURA authorizer which makes the anchor double as the slot claim, which could 
shrink the collator window from 8 to its own rotation run.

PoV cost is about 4KiB per touched stream plus ~2KiB per source proof. A case of 128 streams and 64 sources consumes ~640KiB
out of the ~13MiB budget.

> Unknown: The verification gas is unknown. Verification adds ~1700 blake2 calls inside Refine under a 5 * 10^9 ceiling.
> This needs to be properly measured and the fallback is a blake2b-256 host call for the child PVM.


## Transport and Discovery

We have 2 delivery paths for messages:
- Primary offchain req-resp protocol `/spec-msg/exchange` which holds the messages in a node side archive. This outlives the DA window and gives unbounded catch-up.
- Secondary via PVF exports of payloads to DA as framed segments.

Since segments cannot be exported retroactively, the DA exports are used since day 0. This offers a bootnode agnostic fallback to fetch messages and
Tier 3 (in-core tier) relies on the segments. 64 KiB of messages per block would cost 0.6% of the budget.
The actual cost is erasure-coding bandwidth across validators.

**Discovery**

For bootnode discovery, each parachain publishes its bootnodes under a well-known key in the existing KV store.
The para's runtime maintains its list in the existing KV store (tag `0x08`):

```rust
/// Full storage key: 0x08 ++ SCALE((para_id, SPEC_MSG_BOOTNODES_KEY)).
const SPEC_MSG_BOOTNODES_KEY: &[u8] = b"spec-msg/bootnodes/v1";

struct BootnodeRecord {
    /// bump = publish under .../v2
    version: u8,  
    /// Reachable bootnodes into the para's network, not the authoring node.
    addrs: BoundedVec<Multiaddr /* <=128 B */, ConstU32<8>>,
}
```

> Note: KV writes are charged and skipped on **insufficient balance**. This is accepted since we have DA as independent path to fetch messages.

**Flow**

Parachain A talks with Parachain B:

1. B's JAM follower watches A's provides cell (`0x09++A`). When it changes to a new `StreamsRootA`, A added new outbound messages.

2. B finds A's network via JAM state keys. A published the bootnodes under `0x08 ++ SCALE((A, "spec-msg/bootnodes/v1"))`. 
The record contains up to 8 multiaddresses (managed by parachain itself).

3. B dials bootnodes and discovers A's archive nodes. Messages plus MMR extension proof are fetched from `/spec-msg/exchange` req-resp protocol.

4. Extension is verified against `StreamsRootA` locally and a prefix of the messages is consumed in the next block. The PoV carries the anchor state proof of A's cell.

**Pruning**

A sender may prune payloads below a watermark once the receiver block acknowledges it. Archives serve extension proofs from any block
boundary within the last 25h, and payloads for every root still in the anchor window at Tier 0, or in the last `W` ring
entries from Tier 1b (the ring is unwritten before that), or newer.

## Forced Recovery

The `parachain_set_head` rolls back enacted history:
- each consumer channel desyncs since A's next root won't extend the old one.
- deliveries from rolled-back blocks are irreversible
    - A burned tokens and emitted "mint X" and enacted
    - B delivered before the recovery abandoned that block
    - B's supply is permanently inflated and no layer can undo it.

To mitigate this the `set_head` must delete both cells, which makes recovery observable.
The Coretime runbook must treat it as "reset all inbound channels". Consumers freeze a channel on a deleted cell
or a non-extending root (one that does not extend the consumer's inbound frontier), discard prefetched messages
and resume only on a fresh `open_channel`.

The delivered messages stand and the re-delivered overlap after the restart. Before reopening, the recovering chain
must communicate: what was delivered under the abandoned roots and compensate or it becomes an inflation source
for the counterparties.

> Note: A stream-generation marker in the leaf preimage could cover recovery (ie `(generation, StreamsRoot)`). 
The generation id is bumped on every lifetime discontinuity (registration, set_head and re-onboarding).
Then everything binds to `(paraID, generation)` instead of `paraId`

## 4. Execution: End to End

1. **A:** runtime appends outbound messages to per-destination stream MMRs

   - Output: one `StreamsRoot`; the same computation feeds the header digest and `set_provides_root`
   (guest-asserted identical).
   
   - PVF `export()`s payloads and node-side archives them. Wrapper emits
   `spec_msg_provides: Some(root)` — once per bundle for the final block.

2. **JAM:** report guaranteed -> available -> accumulated
    -  step 6 writes `cell[A]` (and from Tier 1b pushes `ring[A]`).

3. **B:** node reads `cell[A]` at recent blocks, fetches payloads (p2p exchange preferred for low latency, or DA)

   - selects an anchor (best imported block where all consumed roots match, within its
   authorizer's eligible set)
   - authors a block consuming stream prefixes, reserving the proof envelope as `proof_size`
   - in-core (and in the pre-submission self-check) the guest verifies state proofs against `anchor.state_root` and lifts against each source's
   `StreamsRoot`. Calls into `report_error` with a structured payload on failure. Nothing further on-chain.

## 5. Node Stack

**Topology**, in preference order:

(1) **embedded JAM follower with state**: reads the
cell and generates proofs locally, no third-party liveness in the critical path (MVP
production target)

(2) **CE 129 `StateRequest`** against a trusted node. Plausible pending
four confirmations (ask 16): serves non-validators, 64 reads/slot/consumer load, state at
anchor − 8 retained, hash-leaf preimages for `BootnodeRecord` (3) RPC / light-follower
fallback.

**`ProvidesSource`:** reads the cell at imported blocks — **best, not finalized**. Block
stream, startup tip, sync gate, ring read at a recent hash, pending-provides hint (dormant
until Tier 1a).

## 6. Trust Model

| Property | Polkadot | JAM |
|---|---|---|
| Message authenticity | PVF | PVF — unchanged |
| Root authenticity | relay consensus check | guest state proof per-parachain enforcement |
| Provides commitment | UMP signal | digest field - unchanged guarantees |
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

1. **PVM BLAKE2b throughput + metered gas ceiling** 
  Verification adds 1.7k blake2 calls in Refine (64 trie paths + lifts). Refine can consume max 5 billion gas.
  If in guest blake2 doesn't fit, the fallback is a blake2 host call for the child PVM.

2. **`W`** at best-block depth, before the baseline freezes.
  The ring must be sized for the best block pipeline (announce to enact), not the backed one. Raising W later
  is another migration. If we increase W to 64 or `Compact(W)` grows a byte the baseline pricing breaks

3. **Silent ready-queue expiry**
  If B declares a prerequisite on A and A never accumulates, B's report is dropped
  after up to an epoch with zero on-chain trace. The node must detect the drop itself and rebuild-and-retry.

4. **ParaId reuse**
  Coretime doesn't guarantee ParaId uniqueness on JAM. 
  Fix: a generation number in the stream leaf preimage, bumped on register/set_head/offboard.

5. **Acknowledgement/slashing on JAM**
  Tier 2's security on Polkadot rests on LLv2 ACK slashing. JAM has no equivalent

6. **CE 129**
  Can a collator without a JAM node ask another node for state? Should be ok for the cell, but breaks for bootnodes.

7. **Pin the rest** 
  JAM DA retention period is not pinned. For message recovery we'd need a real number. Similar to key derivation.

8. **Settlement gas at worst-case fan-in**
  For Tier 1B the settlement scans up to N rings per candidate on chain. Running out of gas in Accumulate silently
  drops later packages.

9. **Authorizer anchor policy**: AURA uses anchor to prove it's our turn.
  The same anchor is also the block all message proofs verify against.
  The current collator cannot freely pick between the 8 recent blocks anymore (only ones in the current rotation). 
  Needs to make the slot claim independent of anchor choice.

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
  against `spec_msg_provides` of B.

- **Solution 1: Ordered pair**
  If a single core limit becomes an issue, we can separate them onto two cores but force synchronization by `prerequisites`.

  B adds A's work hash as prerequisite and will never accumulate before A. Then, A can import the B's exports.
  The timing here is a bit fragile, B must follow A in the exact slot or at most within last 8 slots. If B misses the
  report is silently discarded at epoch boundary. 

- **Solution 2: Asymmetric parking**

  This is a fallback logic for when things get out of sync.
  If A is enacted, but B fails to enact, the system won't enact A until B can enact.
  A is parked for a timeout and if B takes too long or never arrives, A is dropped as well.
