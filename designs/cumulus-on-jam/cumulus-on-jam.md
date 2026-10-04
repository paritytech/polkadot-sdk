## Cumulus on JAM — collator-side implementation scope

The parachain-service document [\[1\]](#ref-1) describes the in-core (Refine/PVF) and on-chain (Accumulate)
logic of the Parachain Service, the PoV/work-item format and how parachain state lives in JAM
state. It does not cover the collator side.

Purpose of this document:
- identify the components that need to be adjusted,
- present the high-level flows between cumulus and the JAM node and ideas how to implement them,
- present the scope of changes and define phases.

### Work domains

#### 1. **JAM node interface** (counterpart of `RelayChainInterface`)
- Add a `JamNodeInterface` alongside `RelayChainInterface` (parallel backends through the
  transition, see 12), backed by **JIP-2 Node RPC** [\[4\]](#ref-4) (implementation-agnostic standard;
  polkajam's `NodeInterface` [\[2\]](#ref-2) implements it): `bestBlock`/`finalizedBlock` + subscriptions,
  `parent`, `stateRoot`, `serviceValue`/subscription (→ para head & service state),
  `servicePreimage`, `submitWorkPackage(Bundle)`, `workPackageStatus`/subscription,
  `workReport`, `fetchSegments`, `submitPreimage`, `parameters` (all GP constants),
  `syncState`.
- Cumulus uses ~25 `RelayChainInterface` methods; most map to the above, the rest (claim
  queue, availability cores, PVD, state proofs) have no JAM counterpart or become
  service-state reads. JIP-2 has no state-proof or guarantor-assignment queries — by design
  (in-core reads; node-side submission routing).

#### 2. **Scheduling: core / slot / anchor selection**
- Claim-queue polling, `determine_core` and the core-selector machinery (incl. the `CoreInfo`
  header digest) have no JAM counterpart — cores come from coretime assignment. Multi-core /
  elastic scaling works as today: one WP per assigned core, cross-core ordering via GP-native
  work-package prerequisites (mirrors candidate chaining). The parent-search ancestry depth
  (today `scheduling_lookahead`) survives as the recent-anchor window
  (`parameters().recent_block_count`).
- New: track coretime assignment + authorizer pool/queue per core; pick an anchor among recent
  JAM blocks whose timeslot maps to our collator index; drive the build timer from JAM's fixed
  6s timeslots. Based on [\[1\]](#ref-1) §7.1 — explicitly a demonstration sketch, so anchor/slot-claiming
  rules are open design pending authorizer finalization.
- AURA remap (runtime + node in lockstep): `can_build_upon`/`AuraUnincludedSegmentApi` and
  aura-ext `FixedVelocityConsensusHook` compare against the relay slot — must compare against
  the JAM timeslot. The AURA pre-digest/seal itself is unchanged (no relay dependency).
- `RelayParentOffset` (anchor N blocks behind the tip to reduce para forks; used by live
  parachains): the policy comes for free on JAM — older anchors within the recent-anchor
  window are authenticated by the refine context, no descendant proofs needed. Whether the
  exact offset must be *enforced* in-core (today: in-PVF BABE verification of descendants)
  is part of the §7.1 authorizer open design.
- slot_based becomes the default authoring path (lookahead — build per imported relay block —
  has no natural JAM analog).

#### 3. **Work package building & submission**
- WP builder module: assemble work item (validation_code_hash + PoV as extrinsic data
  [\[1\]](#ref-1) §3.2) + authorization token; import/export segments only if messaging uses DA (see 16).
- No candidate receipt on JAM: descriptor, commitments and erasure coding are not collator
  concerns — the availability spec (erasure/segment roots) is computed by guarantors when
  building the work report; the collator submits only the bundle.
- PoV compression: zstd today — decide if the WP extrinsic carries compressed PoV and who
  decompresses (Refine gas cost).
- Submission: JIP-2 `submitWorkPackage(core, package, extrinsics)`; the node routes to the
  assigned guarantors over JAMNP (CE-133 style; polkajam also has proxy fallback [\[3\]](#ref-3)) — phase
  1 = submit via RPC to a (trusted) JAM node; direct JAMNP submission from the collator later.
- Post-submission tracking: JIP-2 `workPackageStatus` (Reportable → Reported → Ready; no
  Accumulated status by design — accumulation observed via `subscribeServiceValue`) replaces
  the collator-protocol "seconded" signal. Drives: announcement, unincluded-segment
  advancement, and **resubmission** — anchors expire with the recent-history window, a stuck
  WP must be rebuilt on a fresh anchor; the `Failed` status enumerates the cases (anchor on
  other fork / not reported / not available in time). No cumulus analog today.
- Decide whether WPs can also be built from imported (not self-authored) blocks —
  `SlotBasedBlockImport` provides a block+proof channel for this (unimplemented consumer,
  [polkadot-sdk#6495](https://github.com/paritytech/polkadot-sdk/issues/6495)).

#### 4. **Authorization token & collator keys**
- `CollatorPair` is replaced by a key in the collator-set trie committed on the Coretime chain:
  membership Merkle proof + signature over the WP hash per submission. Token/config shapes
  follow [\[1\]](#ref-1) §7.1 — a demonstration sketch; open design pending authorizer finalization.
- Authoritative set + root are owned by the parachain runtime + Coretime chain; the node only
  reconstructs the trie locally to produce its own proof. Keystore integration needed (today
  the collator key is generated ad hoc by the relay-node builder, not the keystore).

#### 5. **Parent selection & pipelining** (replaces `find_parent_for_building`)
- Today: included head from `persisted_validation_data` + pending-availability candidates.
  New: para head from service state + in-flight WPs (reported/ready, not yet accumulated) —
  extra RPC/state need beyond "para head at block".
- Unincluded segment semantics must be mapped to JAM's guarantee→availability→accumulate
  latency (block velocity / pipelining — open design question).

#### 6. **Consensus driving: fork choice & finality following**
- Parachain best/finalized blocks are set purely from relay streams (`run_parachain_consensus`,
  `ParachainBlockImport` forced fork choice, `LevelMonitor`). Re-derive from JAM: best = head
  at JAM best block, finalized = head at JAM finalized block; define behavior across JAM
  forks/reorgs. Warp sync survives unchanged — its target is a trusted `WithTarget` block
  derived from the finalized head (not finality-proof based).
- Every para header carries a relay-parent digest (`rpsr_digest` /
  `CumulusDigestItem::RelayParent`; written by parachain-system, read by the node) → a
  JAM-anchor identifier digest, reworked on both sides.

#### 7. **Parachain networking: consequences of no collator protocol**
- Collations are not advertised/fetched; validators get data via WP submission + JAM DA.
  There is also no validator→collator feedback channel — collation fate is tracked purely by
  chain observation (see 3).
- Guarantor pre-connect (direct-JAMNP phase): the gap is guarantor-identity → JAMNP address
  discovery (today backing-group connections ride authority-discovery DHT).
- `ApprovedPeer` (para approves its collator peer-id; emitted by parachain-system, carried via
  inherent + UMP signal) — remove or replace; touches both networking and the inherent (8).
- **pov-recovery**: recovery of withheld blocks from relay availability chunks — replace by
  recovering the WP bundle from JAM DA (`fetchSegments` / audit DA), or explicitly accept
  degraded full-node sync in phase 1.
- **bootnodes**: RFC-0008 discovery via relay DHT → JAM-DHT equivalent or static bootnodes.

#### 8. **Inherent & host functions (inputs)**
- Drop the relay-state-proof inherent; the PVF reads its inputs in-core. **Open [\[1\]](#ref-1) gap**:
  §4.3 exposes only preimage-by-hash lookups and the refine context — there is no generic
  service-state read (para head, KV) host call; [\[1\]](#ref-1) says state arrives "through the
  validation inputs" without defining the mechanism. Needs resolution on the [\[1\]](#ref-1) side.
- PVD remap: `relay_parent_number`/`storage_root` checks → refine-context anchor timeslot +
  posterior state root (thin adapter in `validate_validation_data`).
- Code-upgrade signaling: go-ahead/restriction read from the relay proof → [\[1\]](#ref-1) §5.2 lifecycle
  (parachain-system change).
- The inherent also carries DMP/HRMP message contents — inbound payloads move off-chain,
  transport per 16; follows the messaging decision.

#### 9. **Runtime time base** (relay block number / slot as the clock)
- One new JAM-timeslot `BlockNumberProvider` (mirror of `RelaychainDataProvider`) + mechanical
  `type BlockNumberProvider` swaps in parachain runtime configs (vesting, scheduler,
  governance, proxy, staking-async, broker...; mostly downstream work — relay block number and
  JAM timeslot are both u32).
- Timestamp inherent is derived from the relay slot → JAM timeslot, incl. its in-PVF check and
  the import-queue `AuraVerifier` inherent checks on non-authoring nodes;
  `CheckAssociatedRelayNumber` (monotonic relay-parent ordering) → anchor-timeslot ordering.

#### 10. **Block building: limits**
- Building itself unchanged; max PoV size stays. The real work is PVM gas vs.
  weights (opens the benchmarking pandora jar).

#### 11. **PVM executor & tooling**
- Soft proposal: keep the WASM executor for block building in phase 1 — technically the PoV
  generated by WASM and PVM shall be exactly the same, so this could save some time when
  crafting the PoC. PVM executor for collation building then lands in phase 2.
- PVF blob in PVM format: polkajam has `jam-pvm-build(er)` (services/authorizers) and
  polkadot-sdk `wasm-builder` has an experimental `riscv`/polkavm runtime target — gap is
  JAM-service-blob output for a parachain PVF, not greenfield tooling.
- `validate_block` rework: the entry point no longer returns a `ValidationResult` — outputs
  (head data, upward messages, code upgrade...) are emitted via host calls during Refine
  ([\[1\]](#ref-1) §4.2/§4.3; `set_parent_head_hash`/`set_head` mandatory). Plus the host-function
  override stack: trie recorder + proof-size accounting (the `storage_proof_size` host fn /
  `StorageWeightReclaim` survive — proof size stays the DA weight dimension under PVM), and
  the hashmap-randomness seed (today the relay storage root → anchor state root).
- smoldot: needs PVM integration (light clients).

#### 12. **Node CLI & configuration**
- Relay chain and JAM must be supported **in parallel** in the same binary — long transition
  period, no separate branch. JAM is a new backend selected via config/flags ("point at a
  JAM node") next to the existing relay machinery (`RelayChainCli`, `-- [relay-chain-args]`,
  `--relay-chain-rpc-urls`), not a replacement.

#### 13. **Onboarding / genesis tooling**
- `export-genesis-head`/`export-genesis-wasm` produce artifacts for the relay registrar today;
  define the operator-side JAM flow (genesis head + PVF preimage → Coretime/service
  registration, [\[1\]](#ref-1) §6.2). solo-to-para cutover pallet rides the same signals.

#### 14. **Dev & test environment**
- Dev mode fabricates relay state (`MockValidationDataInherentDataProvider`, synthesized
  go-ahead) → mock-JAM-state provider; `cumulus/test/service` boots a real polkadot node →
  JAM test harness; validate-block benches build on `RelayStateSproofBuilder`; zombienet
  topologies are all relay+para → JAM-network equivalents.

#### 15. **Observability**
- Parachain informant + prometheus metrics derive from relay `candidate_events`
  (backed/included/timed-out) — re-derive from WP status/accumulation, incl. metric semantics
  (`parachain_block_backed_duration` etc.).

#### 16. **Messaging**
- Soft proposal: phase 1 without messaging — solo-chain semantics (no DMP/HRMP/XCMP).
  [\[1\]](#ref-1) §8 leaves the messaging host functions unspecified anyway; consequence:
  phase-1 targets are chains that can live without XCM, with static collator sets
  (no rotation-via-Coretime, [\[1\]](#ref-1) §7.1). Decision pending.
- Messaging on JAM is an open design. **Speculative Messaging** [\[5\]](#ref-5) (relay-era HRMP
  replacement; the design [\[1\]](#ref-1) §8.2 references) prepares the ground: pallet, XCM router,
  para p2p payload transport and runtime verification survive a port.
- Its relay-anchored layer must be redesigned for CRJA: commitment signals → Refine side
  effects + service state, settlement → Accumulate-time check, and the latency-tier model
  re-evaluated (no same-block settlement on JAM).

### Open questions toward the parachain-service design
- Service-state read mechanism for the PVF ("validation inputs" — undefined; see 8).
- Authorizer design finalization (§7.1 is a demonstration sketch; blocks 2 and 4).
- Whether an anchor-offset policy (`RelayParentOffset` analog) needs in-core enforcement,
  and if so where — authorizer or PVF (see 2).
- Multi-core / elastic scaling: confirm Accumulate assumes nothing about ≤1 candidate per
  para per block; ordering via WP prerequisites.

### Phases (draft)
- **Phase 1**: WASM PVF; JAM node via JIP-2 RPC (interface, submission, status tracking);
  scheduling/token/parent-selection/consensus-following reworked; runtime time base;
  CLI/config + onboarding flow + minimal dev/test env; no messaging; substrate p2p sync only
  (no DA recovery, static bootnodes).
- **Phase 2**: PVM executor + PVF-as-PVM tooling; direct JAMNP submission + guarantor
  pre-connect; DA-based recovery/sync; observability polish; smoldot.
- **Phase 3**: messaging (see 16).

### refs

- <a id="ref-1"></a>\[1\] [parachain-service-on-jam.md](https://github.com/paritytech/polkadot-sdk/blob/mku-cumulus-on-jam-doc/designs/parachain-service-on-jam/parachain-service-on-jam.md)
- <a id="ref-2"></a>\[2\] polkajam [`NodeInterface`](https://github.com/paritytech/polkajam/blob/main/crates/node/src/node/interface/mod.rs), [wp_status_tracker.rs](https://github.com/paritytech/polkajam/blob/main/crates/node/src/node/interface/wp_status_tracker.rs)
- <a id="ref-3"></a>\[3\] polkajam [work_package_submitter.rs](https://github.com/paritytech/polkajam/blob/main/crates/node/src/chain/guarantors/work_package_submitter.rs)
- <a id="ref-4"></a>\[4\] [JIP-2 (Node RPC)](https://github.com/polkadot-fellows/JIPs/blob/main/JIP-2.md)
- <a id="ref-5"></a>\[5\] [Speculative Messaging: End To End Flow](https://docs.google.com/document/d/1P5d9lcvahDQproAGSEP55PdhKjIGBEoPtjXumYc_fCU); polkadot-sdk PRs [#10449](https://github.com/paritytech/polkadot-sdk/pull/10449), [#12226](https://github.com/paritytech/polkadot-sdk/pull/12226)
