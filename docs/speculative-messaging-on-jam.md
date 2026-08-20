# Speculative Messaging on JAM

The [Parachain Service](../designs/parachain-service-on-jam/parachain-service-on-jam.md#3-the-parachain-service)
needs a messaging system to replace HRMP-style payloads;
[Speculative Messaging](https://github.com/paritytech/polkadot-sdk/blob/9d0a0daee40e6e350209aaf4b3e3bdf1fb9a8793/docs/speculative-messaging-design.md)
is the candidate. This document is the protocol spec: consensus delta, verification,
transport, tiers, asks.

**Normativity:** the PS design is normative for the service delta (digest fields, tags,
accumulate steps, host functions); this document for the protocol. One home per constant.

Pinned against PolkaJAM `84f86225a704` (GP 0.7.x): `W_B = 13,791,360`, report budget 48 KiB,
`H = 8`, `U = 5`, `E = 600`, `max_refine_gas = 5×10⁹`, `max_exports = 3,072`, segment
4,104 B. Open: DA retention period (ask 17), GP equation refs.

## 0. Design Summary

> **If a root was already enacted by B's anchor, B just proves it and the chain never hears
> about it.** (ie no settlement needed)
>
> B picks a recent JAM block as its anchor and ships a Merkle proof in its PoV:
> "in the state *after* this block executed, A's cell holds `StreamsRootA`." 
> Validators check that in-core. A root not yet enacted at the anchor cannot be
> proven this way and has to be checked on-chain instead.
>
> The latency tiers decide when to use the in-core cell proof or the settlement ring.

This is safe because JAM only accepts B's report if the anchor is an ancestor of the block that
accumulates it (it must be in `β`), and enacted state never un-happens along a fork. So, whatever B
proved at the anchor is still true when B lands.

Two exceptions: forced recovery (§1.6) and co-packaged sibling roots (§12). And when a root *does* have to
be checked on-chain (Tier 1b+), Accumulate needs something on-chain to check it against (settlement ring).

MVP delta: `set_provides_root` + digest field (33 B), provides cell (81 balance units),
ring slot reserved-not-written (1,586). **No settlement step, no requires field, no
`yield`.** Baseline bumped: 69,847 → **71,514**.

## 1. Consensus and State Changes

### 1.1 The Provides Digest Field

Only **provides** reaches consensus at MVP:

```rust
enum ParachainWorkDigest {
    Ok {
        // .. existing fields
        /// Set via `set_provides_root`. `None` = root unchanged.
        spec_msg_provides: Option<StreamsRoot>,
    }
}
```

- `set_provides_root(root)`: optional host call, at-most-once per Refine, wrapper-enforced. Second
  call fails with `RefineLog::DuplicateProvidesDeclaration`.
- **Bundled PoVs:** called once per bundle, with the **final** inner block's root (the
  guest aggregates). Sound because stream MMRs are append-only and the final root commits to
  every inner block's messages.
- **Normative rule:** the `set_provides_root` value and the final inner block's header
  digest (`DigestItem::Consensus(SPMS_ENGINE_ID, root)`) are one computation,
  byte-identical. This is guest-asserted by aborting Refine on mismatch. A wrong but internally
  consistent root is unenforceable self harm.
- 33 B against the 48 KiB report budget; PS §4.1 step 8's size check must include it.
- `ParachainWorkDigest` field, not an `UpwardMessage` since UMP replays after the step-6 head write, so it
  cannot gate enactment.

### 1.2 The Provides Cell

```rust
/// Keyed `0x09 ++ SCALE(para_id)`.
/// - ParaId as fixed 4-byte LE u32, NOT Compact.
/// - Absent means never sent or force-recovered or not re-enacted or offboarded (OQ 4).
/// Written only by Accumulate, on enactment (PS §5.1 step 6).
spec_msg_provides: Map<ParaId, StreamsRoot>
```

Step 6 runs after the registration/parent-head/code checks and a rejected candidate never
moves the cell. `parachain_clean_up` drops it.

- **Why a bare 32-byte value:** values <= 32 bytes are placed in the JAM trie leaf, so the proof is a
  pure Merkle path, no preimage.
  Costs 81 units vs +33 in `ParaInfo`, but saves ~4 KiB of proof per source per block.

- Accumulate writes whatever the digest carries, JAM will not verify MMR extension. This means
  that consumers are protected against forged messages, **not against a source rolling back its own stream**.


### 1.3 The Settlement Ring — reserved at MVP, written from Tier 1b

```rust
/// Keyed `0x0a ++ SCALE(para_id)`.
/// 
/// From Tier 1b: step 6 pushes the enacted root iff != newest entry, evicts oldest.
/// Read only by settlement (§7.2). NEVER Merkle-proved — so width costs nothing in proof size.
spec_msg_recent_provides: Map<ParaId, BoundedVec<StreamsRoot, W>>
```

| | Cell (`0x09`) | Ring (`0x0a`) |
|---|---|---|
| Consumer | in-core verifier (guest) | Accumulate settlement |
| Access | Merkle proof in PoV | Direct state read |
| Size | <= 32 B hard | unconstrained |
| Live at | every tier | Tier 1b+ |

**Sizing:** `W = max_elastic_cores * (eviction_window + pipeline_depth)`.
The core rate multiplies the whole exposure span. For 3 cores, 3 * (8 + 8) = **48**. Choose at *best-block*
depth before the baseline freezes.

`Compact(W)` is 1 byte only for W <= 63.

### 1.4 Baseline Footprint

PS §6.1 prices an entry at `44 + |value| + |key|` — **balance units, not bytes**:
cell = 44 + 32 + 5 = **81**; ring at W = 48 = 44 + 1,537 + 5 = **1,586**;
baseline bumps 69,847 to **71,514**.

Both are baseline-covered: writes take no balance check ("the write cannot fail"), 
and **deletions refund nothing**, unlike `kv_remove`, the deletes in `parachain_clean_up`
and `parachain_set_head` must not touch `used_state_balance`, or PS §6.4's exact-equality check bricks every deregistration.

**Land before the first parachain registers (ask 7) — plan of record.** Any later `BASELINE_FOOTPRINT` change is a live migration.

### 1.5 Bundling and Granularity

Provides is per block semantically, per work package in consensus. A bundle declares the
**final** root and intermediates stay in their headers. Consequences:

- Event streams coarsen to the bundle (serving obligation, §3.3).
- A bundling receiver merges per-block requires into one set.
- **Bundle-intermediate roots are unsettleable at every tier**: only the digest root
  reaches cell and ring. Tier 1b is immune (binds to bundle-final digests by construction);
  Tier 2 must bind only to declared package-boundary roots (§7.3).

Cycles: never between blocks (SpecMsg's invariant), never at Tier 0 (only enacted roots
consumable).

From Tier 1b, mutual requires between co-arriving candidates are handled by co-packaging or
enactment groups (and not by mutual **prerequisites** (§12)). This means that no 
viratual window is needed. The Accumulate processes packages in order and step 6 writes before
the next package's setlement runs. Therefore, we can achive same-block A->B communication and
ordering (one-directional prerequisite as Tier 2).

### 1.6 Forced Recovery

`parachain_set_head` (PS §6.3) **rolls back enacted history**. Two costs:

1. **Channel desync (liveness):** A's next root won't extend the old one; every consumer
   stalls.
2. **Economic divergence (safety):** deliveries from rolled-back enacted blocks are
   irreversible. A burns X, emits "mint X", enacts, B delivers, recovery abandons the
   block: re-executed txs double-mint, non-re-executed leave an unbacked mint. **B's supply
   is permanently inflated**; no messaging-layer mechanism can undo it.

Handling:

- **Service:** `parachain_set_head` also deletes `0x09`/`0x0a` (balance-neutral) and recovery
  becomes observable via the absent cell.
  Consequence: **any mid-life `set_head` is channel-severing by construction**.
  Coretime runbooks must treat it as "reset all inbound channels" (ask 19). No-op at registration-time use.

- **Consumer:** on cell deletion or non-extending root will freeze the channel, discard
  prefetched material. Resume **only** on a fresh `open_channel`, resetting the inbound
  frontier to the new stream state. The re-delivered overlap is genuinely new messages
  (Cost 2's residue), not a dedup bug. Delivered messages stand.

- **Sender (reconciliation gate, ask 19):** before reopening, enumerate messages delivered
  under abandoned roots (archive/DA) and compensate — suppress re-emission or account the
  drift. Skipping this makes the para an inflation source for its counterparties.

**In-flight tail, accepted and bounded:** packages anchored pre-deletion still deliver
abandoned-branch messages for up to 8 slots + accumulation lag.

At Tier 1b+ in-flight settlements naming the deleted ring burn those consumers' slots.
Robust form: a stream-generation marker in the leaf preimage (OQ 4) covers recovery,
offboarding and ParaId reuse in one mechanism.

## 2. In-Core Verification of Requires

### 2.1 State Proofs Against the Anchor

B's PoV carries, per consumed source A with root `StreamsRootA`, a Merkle proof that
`0x09 ++ SCALE(A) = StreamsRootA` at B's anchor which is verified in-core against
`RefineContext.anchor.state_root` (fetched by the child PVM via
`work_package_context()`, no wrapper involvement).
Then Lifts are verified against `StreamsRootA` as on Polkadot. Nothing reaches Accumulate.

**State-key derivation** (a mismatch is a silent, total failure, §8):

1. `h = BLAKE2b-256( 0xffffffff ++ storage_key )`.
2. 31-byte key: bytes 0–7 interleave service-id LE bytes with the hash
   `[id₀,h₀,id₁,h₁,id₂,h₂,id₃,h₃]`; bytes 8–30 = `h[4..27]`.

The guest reads the service id from `work_items_summary()`, never hardcoded. The anchor
state-root is part of the package — deterministic for every guarantor. `RefineContext` has
**no anchor timeslot**, so no in-core freshness policy is expressible.

### 2.2 The Anchor Budget

The anchor must be within the last **8** JAM blocks (`H`) at guarantee inclusion.

- **Anchor on the best imported block, not finalized.** A dropped fork fails closed
  (rebuild) — the exposure every work package has. Finality gates delivery confidence and
  pruning instead (§3.3).
- Budget at 6 s slots: state-root lag 1 (headers carry the *prior* root — freshest anchor
  is **tip − 1**), build < 1, distribution 1–2, inclusion 1 → **3–4 of 8 used**, 4–5
  reserve.
- **The anchor is double-booked (ask 18).** PS §7.1's AURA authorizer makes the anchor the
  slot claim, shrinking a collator's eligible anchors from 8 to its own rotation run —
  degrading the reserve, the envelope cache, and re-anchoring locality.
  
  Either authorizers for message-consuming paras claim slots without binding anchor choice (OQ 9), or this
  budget is re-derived at the constrained count. The unstated interaction is the only
  unacceptable outcome.

### 2.3 Re-anchoring

Missing the window is cheap:
- 1. Re-anchoring swaps the ~KiB proof envelope, never the block
- 2. MMR peaks are prefetched continuously, so regenerating lifts is local
- 3. keep a small cache of envelopes against recent eligible anchors.

From Tier 1b the ring lets B name an older root against a newer anchor, ending forced lift regeneration.

### 2.4 Verification Lives in Guest Code

The proof rides the PoV in the `ParachainBlockData` framing and is verified
via `validate_block` in cumulus guest code — **the framing needs a slot for the anchor
proof** (V4 or a V3 field, same code for both targets, ask 14).

The wrapper's requires-related delta is **zero**. All hashing is BLAKE2b-256 (state trie and message
trees); the existing `sp_io`-on-PVM shim covers it — subject to §2.6.

This implies a trust cost: root authenticity moves from a relay-consensus check to per-parachain guest code.
A buggy runtime can disable inbound authentication with no on-chain evidence. The alternative is a
requires field with a settlement at every tier.

### 2.5 Budgets and PoV Reservation

The bound is on **touched streams**, not sources (~4 KiB per touched stream worst case
+ ~2 KiB per read-context gap; one source = many streams). Reserve in the STF as
`proof_size`:

```
pov_reserve = Σ_touched_streams (4 KiB + gaps × 2 KiB)   # lifts
            + Σ_consumed_sources (2 KiB)                 # state proofs (additive)
```

Aproximation: 128 streams + 64 sources = 640 KiB, well inside `W_B` ~13.15 MiB.
A single-key proof is `32 * depth + 64` ~ 0.7–1 KiB; 2 KiB is conservative.
Depth tracks *total* JAM state. Encode the 64 paths as a **multiproof** (~20% saving).

`MAX_REQUIRES_SOURCES = 64` and `MAX_TOUCHED_STREAMS` are **guest-side, advisory at
Tier 0** (requires is off-chain and nothing checks them).

From Tier 1b the speculative subset is wrapper-bounded by `N` (§7.2).
On-chain cost of requires at Tier 0: **zero bytes**.

### 2.6 Gas — the highest-risk unknown

Refine gas is a hard consensus limit. The design adds ~1,700 BLAKE2b invocations (64 paths
× 20–30 compressions) plus lift verification, on an unmeasured transpilation baseline.

Three gates: throughput (informs core time), **metered ceiling (go/no-go, before spec
freeze**, denominator `max_refine_gas = 5×10⁹`), and accumulate settlement gas (before
Tier 1b, §7.2).

**Fallback: a BLAKE2b-256 host call for the child PVM (ask 10)**.

## 3. Transport and Discovery

### 3.1 Transport — dual, DA default-on

- **Primary:** `/spec-msg/exchange` request-response for low latency. The payloads are retained in the sender's
  node-side archive per §3.3 (outlives DA windows with unbounded catch-up).

- **DA export, default-on:** the PVF `export()`s outbound payloads as framed segments.
  Segments **cannot be retro-exported**, so deferring DA is irreversible for every block
  produced meanwhile. Enabling unneeded DA is cheap. Also the zero-discovery fallback and
  the cold-start path (signal leaves + register events in segment framing means no pre-existing
  channel needed). Tier 3 requires it outright.

- Cost: a 64 KiB/block para uses 17 segments ~ 0.6% of the export budget; the real cost is
  erasure-coding bandwidth across the validator set (needs double checking before we decide to go with p2p only).

### 3.2 Discovery — parachain-managed KV bootnodes

JAM state keys are hashed and non-enumerable, so the key must be well-known. The para's
runtime maintains its list in the existing KV store (tag `0x07`):

```rust
/// Full storage key: 0x07 ++ SCALE((para_id, SPEC_MSG_BOOTNODES_KEY)).
const SPEC_MSG_BOOTNODES_KEY: &[u8] = b"spec-msg/bootnodes/v1";

struct BootnodeRecord {
    /// bump = publish under .../v2
    version: u8,  
    /// Reachable bootnodes into the para's network, not the authoring node.
    addrs: BoundedVec<Multiaddr /* <=128 B */, ConstU32<8>>,
}
```

Requires no changes for Parachain Service. The parachain must handle rotation and anti-spam policies.
Note: KV writes are charged and skipped on **insufficient balance**. This is accepted since we
have DA as independent path to fetch messsages.

Note: discovery may be self-attested (KV), but Provides must be consensus written.

### 3.3 Irreversibility, Pruning, Serving

| SM trust row | Polkadot | JAM |
|---|---|---|
| Different trust domain | relay inclusion | **enacted in Accumulate** (Tier 0), sanctioned exceptions per §0 |
| Same trust domain | Low-Latency v2 ack slashing | **no equivalent** — gates Tier 2 (§7.3, OQ 5) |
| Same super-chain | co-authored super-block | **co-packaged work package** or enactment group (§12) |

Prune payloads below watermark `W` once the receiver block asserting `W` is irreversible
per the row above (at MVP **enacted**).

Material for §1.6-frozen channels is retained until reconciliation (it's the evidence).
Serving oblication by the archive node side:
- extension proofs from any block boundary in the last **25 h**
- payloads for every stream under any root in the last `W` ring entries
- anchor window at Tier 0

## 4. Execution: End to End

1. **A:** runtime appends outbound messages to per-destination stream MMRs

   - Output: one `StreamsRoot`; the same computation feeds the header digest and `set_provides_root`
   (guest-asserted identical).
   
   - PVF `export()`s payloads and node-side archives them. Wrapper emits
   `spec_msg_provides: Some(root)` — once per bundle for the final block.

2. **JAM:** report guaranteed -> available -> accumulated
    -  step 6 writes `cell[A]` (and from Tier 1b pushes `ring[A]`).

3. **B:** node reads `cell[A]` at recent blocks, fetches payloads (p2p exchange preffered for low latency, or DA)

   - selects an anchor (best imported block where all consumed roots match, within its
   authorizer's eligible set)
   - authors a block consuming stream prefixes, reserving the proof envelope as `proof_size`
   - in-core (and in the pre-submission self-check) the guest verifies state proofs against `anchor.state_root` and lifts against each source's
   `StreamsRoot`. Calls into `report_error` with a structured payload on failure. Nothing further on-chain.

Handshakes (`open_channel`/`accept_open_channel`) and flow control port unchanged 
(in-band, stream-ordered, parachain-side). **PS §8.2's open point answered: no host functions
needed for channel management.**

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
| Root authenticity | relay consensus check | guest state proof — per-parachain enforcement (§2.4) |
| Provides commitment | UMP signal | digest field — unchanged guarantees |
| Stream monotonicity | not enforced | not enforced — self-harm, consumer-visible |
| Sender-history irreversibility | governance-grade exception | Coretime-grade (§1.6) — more routine, hence reconciliation-gated |

Polkadot capabilities: same-block A→B **deferred to Tier 1b** (free via in-order
accumulation); atomic enactment groups, candidate cycles and super chains — **service-level
/ handled at authoring**, see §12.


> **Why not JAM's `prerequisites`?**
>
> (1) Wrong granularity: a package hash orders, a StreamsRoot is content. The root must be carried regardless.
>
> (2) Report is not enactment: steps 3/5 can decline after accumulation, so compare-and-reject survives anyway.
> 
> (3) `J = 8` is too small and shared with Tier 3's segment lookups; fan-in targets 64.
> 
> (4) The 8-block reporting gate makes it strictly worse than proving enacted state.
> Compact form:**available exactly where they buy nothing (Tier 0), unusable exactly where they'd help
> (mutual dependencies never clear the ready queue, §7.3).
> 
> **Right solution at Tier 3: segment roots *are* content-addressed.**

## 7. Latency Tiers

Cumulative — each tier carries everything below it.

| | Consumes | Requires settles | Ring | New GP deps |
|---|---|---|---|---|
| **T0** Accumulate | enacted roots | in-core | reserved, unwritten | none |
| **T1a** Backed (hint) | enacted roots | in-core | reserved, unwritten | none |
| **T1b** Backed (active) | guaranteed roots | **on-chain** | **written + read** | none |
| **T2** Best block | announced roots | on-chain | written + read, deeper | optional prerequisites |
| **T3** In-core import | exported segments | on-chain | written + read | **`J = 8` binds** |

### T0 — Accumulate (MVP)

HRMP latency parity (guarantee->accumulate 1–2 slots + 1–2 anchor lag).

Implementation Delta:
- digest field
- host fn + `DuplicateProvidesDeclaration` 
- step-8 budget check
- cell `0x09` + reserved ring `0x0a`, baseline 71,514, all writes/deletes balance-neutral
- step-6 cell write, no settlement; clean-up and recovery drop both tags
- guest = trie path per source + lifts + digest/header assertion + structured `report_error`
- PoV = proofs + lifts in the `proof_size` reservation, multiproof-encoded; framing slot for the anchor proof
- node = cell reads at best blocks, embedded follower preferred, exchange + DA, KV bootnodes, telemetry from day zero. 

Contingency: BLAKE2b host call (ask 10).

### T1a — Backed, prefetch hint. Zero consensus delta.

`pending_provides()` reads guaranteed-not-accumulated reports (needs block-body visibility,
ask 13). Prefetch payloads, warm MMR peaks — still consume only enacted roots. Ship first,
measure.

### T1b — Backed, active

**Gate: guarantee->accumulate survival >= 99% in a 2-slot window, read chain-weighted.**

A guaranteed report can still die (availability timeout `U = 5`, step-3/5 decline).

- Digest: `spec_msg_requires: BoundedVec<(ParaId, StreamsRoot), N>`, **N = 16
  provisional**, speculative subset only, unique by ParaId, wrapper-enforced via
  `set_requires_speculative` (at-most-once). Enacted-at-anchor roots stay in-core at zero
  cost. `N` is sized by settlement **gas**, not digest bytes — fix before the digest
  freezes (ask 11).
- Ring `0x0a` written at step 6 + read by the **new settlement step at the 5/6 seam**:
  every `(src, streams_root)` must be in `ring[src]`, newest-first. Miss = reject like a parent-head
  mismatch, log `RequiresUnmet { src, root }`, no state change. Same-block A->B needs
  nothing (in-order processing); ordering misses just retry.
- Guest split: enacted-at-anchor -> prove in-core; speculative -> emit to digest.
- Node: rebuild-and-retry on `RequiresUnmet`.

**Settlement gas is the tier's real gate:** up to `N` rings × 1,537 B, newest-first, inside
`min_item_gas` — and reports are **not replayed** after out-of-gas, so under-sizing
silently drops *later* packages in the block (innocent paras lose slots). Size at
worst-case fan-in; let gas bound `N` (ask 12).

**Economics:** a settlement miss burns B's slot *and* withholds B's own provides, so misses
cascade — a 5-deep chain at 99% survival clears ≈ 95%. Read the gate at the depth actually
built; consume speculatively from sources, not from speculative consumers of sources.
Bundling immunity: 1b binds to bundle-final digests, so §1.5's intermediate hazard cannot
occur here. Bonus: the ring ends forced lift regeneration on re-anchor (§2.3).

A must land first or B burns a slot.

### T2 — Best block

Consensus surface unchanged from 1b. New, node-side: `/spec-msg/announce`; off-chain block
verification (header lineage, valid collator — needs its own definition once PS §7.1 is
repaired, ask 9); the **header-digest read path**, correct only for **declared
package-boundary roots** — announce carries A's intended boundary and B binds speculative
requires only to boundary roots (§1.5); ready-queue expiry detection (OQ 3). Ring needs the
deeper best-block `pipeline_depth` — already sized in from Tier 0, **no second migration**.

- **Boundary violation is adversarial:** announcing a boundary then re-bundling inflicts
  `RequiresUnmet` + a burned slot on every bound consumer, free and repeatable. Interim:
  per-source backoff (violations are publicly observable). Systemic: the slashing design
  (OQ 5).
- **Optional prerequisites for ordering:** B's package declares A's hash; JAM orders A
  first, same-block included. Caveats: report ≠ enactment (compare-and-reject stays); the
  **8-block reporting gate binds at every stage** (never name a package reported > 8 blocks
  ago — the report becomes unreportable; ξ's epoch depth is duplicate-rejection, not
  eligibility); **silent expiry** (dropped from the ready queue after ≤ 1 epoch, no
  on-chain trace — node detection, OQ 3); **no mutual prerequisites** — honest packages
  can't encode them (the field hashes under the package hash) and fabricated reports never
  clear the ready queue; super chains use co-packaging or one-directional prerequisites
  (§12).
- **The unfunded dependency:** SM's speculative tier is backed by acknowledgement slashing;
  no JAM equivalent exists. Tier 2 cannot ship on its stated security basis until one does
  (OQ 5) — the gating item, on no other list.

### T3 — In-core import

B imports A's exported segments; delivery becomes in-core and deterministic. No new
consensus (settlement and ring still required); `import_segments()` exists; PoV shrinks;
tight coupling to A's export timing and the DA window. **`J = 8` binds** — fan-in drops
from 64 to 8. The only tier with a genuine `J`-bump motive; still wait for measured demand.

### Decide before the baseline freezes

1. `W` at best-block depth (§1.3). 2. Ring slot reserved at Tier 0 — **yes**. 3. Land
before first registration (ask 7). 4. Super-chain parked reserve: **opt-in delta-charged,
never baseline** (§12).

## 9. Asks on the Parachain Service

| # | Ask | Tier | Acceptance |
|---|---|---|---|
| 1 | `spec_msg_provides` digest field | T0 | 33 B counted in the 48 KiB budget |
| 2 | `set_provides_root(root)` host fn | T0 | optional, at-most-once, wrapper-enforced |
| 3 | `RefineLog::DuplicateProvidesDeclaration` | T0 | second call fails Refine |
| 4 | Output-size check includes the field | T0 | `RefineOutputTooLarge` accounts for it |
| 5 | Tag `0x09` + step-6 write | T0 | written only after steps 1/3/5; balance-neutral |
| 6 | Tag `0x0a`, reserved unwritten | T0 | baseline-charged at W = 48 |
| 7 | Baseline 69,847 → 71,514 | T0 | **lands pre-first-registration**; fallback priced (§1.4), never preferred |
| 8 | `parachain_clean_up` drops `0x09`/`0x0a` | T0 | reused ParaId inherits nothing; deletions refund nothing |
| 9 | `RefineContext` doc fix | T0 | records `beefy_root`; removes fields GP lacks; flags §7.1's slot claim; constrained by ask 18 |
| 10 | BLAKE2b-256 host call for the child PVM *(contingent)* | T0 | only if §2.6's gas gate fails |
| 11 | `spec_msg_requires` + `set_requires_speculative` + log variant | T1b | speculative subset, unique by ParaId; `N` fixed pre-freeze, sized by gas |
| 12 | Ring write + settlement step + `RequiresUnmet` | T1b | push iff ≠ newest, balance-neutral; check after earlier same-block step 6; gas sized at worst-case fan-in in `min_item_gas` |
| 13 | Guarantees-extrinsic visibility (node) | T1a | `ProvidesSource` reads guaranteed-not-accumulated reports |
| 14 | `ParachainBlockData` anchor-proof slot (ours) | T0 | V4 or V3 field; both targets; guest digest/host-call assertion, fixture-backed |
| 15 | `parachain_set_head` deletes `0x09`/`0x0a` | T0 | recovery observable; balance-neutral; no-op at registration |
| 16 | Confirm CE 129 serving envelope (node) | T0 | non-validator peers; load; state at anchor − 8; hash-leaf preimages |
| 17 | Pin the DA retention period | T0 | §3.3's backstop and cold-start have a number |
| 18 | Anchor-freedom constraint on the §7.1 repair | T0 | slot claims don't bind anchor choice (OQ 9), or §2.2 re-derived |
| 19 | Recovery runbook: reconciliation-gated re-open | T0 | `set_head` = "resets all inbound channels"; reconcile before re-opening; consumers resume only on fresh `open_channel` |

## 10. Open Questions

1. **PVM BLAKE2b throughput + metered gas ceiling** (§2.6) — go/no-go, fallback ask 10.
   Highest-risk item.
2. **`W`** at best-block depth, before the baseline freezes. Worked 48; `Compact` boundary
   at 63.
3. **Silent ready-queue expiry** — node-side detection + rebuild-and-retry (also covers
   §12's S1.5). Liveness, not soundness.
4. **ParaId reuse** — re-run SM's threat assessment against Coretime-on-JAM's id policy; a
   stream-generation marker in the leaf preimage covers reuse, offboarding *and* recovery
   in one mechanism and makes §1.6's re-open gate structural.
5. **Acknowledgement/slashing on JAM** — Tier 2's trust row is empty; owner needed; the
   §7.3 grief vector is required input.
6. **CE 129's four confirmations** (ask 16), esp. hash-leaf preimage retrieval.
7. **Pin the rest:** DA retention (ask 17), GP equation refs for key derivation and report
   checks.
8. **Settlement gas at worst-case fan-in** — sizes `min_item_gas` and `N` (asks 11, 12);
   before Tier 1b.
9. **Authorizer anchor policy** (ask 18) — resolve with, not after, ask 9's §7.1 repair.

## 12. Super Chains

A superchain pair consumes each other's speculative output every step (A consumes B
same-step; B consumes an earlier A block), all lifts valid by construction.

The validated ladder:

- **S0 — Fusion** *(the plan)*: both candidates as work items of one package on one core —
  one report, one invocation, availability atomic. Needs one service-level group-enactment
  rule (any member fails means decline all). Zero JAM changes.
  
  This is §0's second sanctioned exception.

- **S1.5 — Ordered pair**: two cores; B declares a one-directional prerequisite on A, A
  imports B's exports via a `Direct` segment root.
  
  Zero changes; only "A enacted, B missing" remains.

- **S1 — Asymmetric parking**: closes that residual service-side (opt-in delta-charged
  record, never baseline). Zero JAM changes.
- **S2 — GP package groups**: format-breaking; gated on measured demand (fusion overflow ∧
  parking pain ∧ burn cost).

The fixpoint dies at authoring (fixed intra-superstep order); mutual same-step consumption
is a two-phase-collation STF profile, not an impossibility.
