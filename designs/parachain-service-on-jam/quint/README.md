# Parachain Service on JAM — Quint Spec

Formal model of the [Parachain Service on JAM](../parachain-service-on-jam.md)
design, written in [Quint](https://quint-lang.org).

## File layout

| File | Models | Source §  |
|---|---|---|
| [`types.qnt`](./types.qnt) | Foundational types (`ParaId`, `Hash`, `ValidatorKey`, …) and protocol constants (`ValCount`, `MaxReportVarSize`, …) | — |
| [`messages.qnt`](./messages.qnt) | `UpwardMessage`, `ParachainWorkDigest`, `RefineLog`, `AccumulateLog` | §3.3, §4 |
| [`state.qnt`](./state.qnt) | `ParaInfo`, `PreimageEntry`, `ParachainServiceState` | §3.1 |
| [`state_balance.qnt`](./state_balance.qnt) | `baseline_footprint`, `used`/`total_state_balance` accounting, preimage-registry referencer sharing | §6.1 |
| [`code_upgrades.qnt`](./code_upgrades.qnt) | Pending-upgrade lifecycle: request, transition, activation, timeout | §5.2 |
| [`validator_keys.qnt`](./validator_keys.qnt) | Chunked `designate`: `set_validator_keys`, `staged_validator_keys`, finalize/abort | §5.3 |
| [`head_commitment.qnt`](./head_commitment.qnt) | Binary Merkle tree over the block's changed parachain heads; the root `accumulate` returns | §5.5 |
| [`refine.qnt`](./refine.qnt) | Refine per work-item, abstract PVF execution, `ParachainWorkDigest` assembly | §4 |
| [`accumulate.qnt`](./accumulate.qnt) | Per-work-package and always-accumulate work, upward-message replay | §5.1 |
| [`management.qnt`](./management.qnt) | Coretime-chain-only host calls: `parachain_set_state_balance`, `_set_head`, `_set_validation_code`, `_clean_up` | §6 |
| [`state_vars.qnt`](./state_vars.qnt) | Shared `var` declarations: `svc`, `now`, `jamStagingSet`, `coretimeChain`, `assetHub`, ghost `solicitedSet` — imported by `main.qnt` and `invariants.qnt` so both modules refer to the same state | — |
| [`invariants.qnt`](./invariants.qnt) | Nullary `val: bool` catalogue of every state invariant (balance, preimage-registry, designate, code-upgrade, staged-keys, solicit-bookkeeping) + composite `invariants` | — |
| [`main.qnt`](./main.qnt) | Composition: `init`, `step`, refine-input generators. State variables live in `state_vars.qnt`; invariants in `invariants.qnt`. | — |
| [`tests.qnt`](./tests.qnt) | `run` tests (e.g. `testBounceOnFull` — `incoming_transfers` at capacity bounces the next ingress). | — |

## Scope

**Modeled:**
- All Parachain Service state per §3.1.
- The full upward-message vocabulary the PVF can emit during Refine.
- Accumulate replay semantics: parent-head check, code-upgrade activation,
 preimage-registry referencer sharing, state-balance accounting,
 chunked `designate`.
- Coretime-chain-only host calls for registration, forced updates, clean-up.
- The pending-authorizer-queue cache flush on the always-accumulate path.
- The §5.5 head commitment: which heads a block changed, leaf ordering, and the
  tree's shape.

**Abstracted:**
- Cryptography (signatures, hashes, Merkle proofs) — opaque opaque types. The
  §5.5 commitment tree is built structurally, but `keccak_256` is modelled as an
  injective mix that keeps leaf and node images disjoint; encoded element sizes
  (37/65 octets) are not modelled.
- PVF execution — modelled as a nondeterministic choice of result + upward-message
 list, constrained by §4.1 invariants only.
- D3L segment storage and DA — out of scope (D3L is the §8 messaging design,
 still TBD in the design doc itself).
- The AURA authorizer's per-slot collator logic (§7.1) — we only model the
 `assigners`/`assign` calls and the queue draining/flush rule.

**Not modeled (matches design TBDs):**
- Cross-chain messaging (XCMP) — §8 is itself a future design.
- Anchor-timeslot exposure and lookup-anchor posterior state-root access — §9.

**Assumption where the design is silent:** §5.5 fixes neither how leaves are
paired nor what happens to an odd element at a level. `pairUp` hashes adjacent
pairs and promotes a trailing odd element to the next level unchanged (matching
`binary-merkle-tree`) rather than duplicating it. Both choices change the root,
so §5.5 needs to state one before an implementation can interoperate.

## Invariants

The top-level invariants live in [`main.qnt`](./main.qnt). The key ones:

- **`balance_invariant`**: for every live para, `0 ≤ used_state_balance ≤
 total_state_balance`.
- **`preimage_referencer_consistency`**: a `ParaId` appears in
 `preimage_registry[h].referencers` iff its `ParaInfo` accounts for that
 preimage's footprint in `used_state_balance`.
- **`head_commitment_matches_changed_heads`**: the hash `accumulate` returned is
 exactly the commitment over the heads that changed in the step — `None` when
 none did.
- **`designate_only_on_is_last`**: JAM `designate` is invoked iff Accumulate
 just processed a `SetValidatorKeys { is_last: true }` message that passed
 the `valcount` check.
- **`staged_keys_owner`**: `staged_validator_keys` is mutated only on a
 successful replay of a `SetValidatorKeys` message from Asset Hub.
- **`no_pending_when_code_matches`**: if `ParaInfo.pending_upgrade ==
 Some((h, _))` then `ParaInfo.validation_code_hash != h`.
- **`work_result_parent_continuity`**: every accumulated work result's
 `parent_head_hash` equals the hash of the para's previous `head_data`.

## Running the spec

```sh
quint typecheck main.qnt                       # static checks
quint run main.qnt                             # randomized exploration
quint test --backend=typescript tests.qnt      # unit tests
quint verify main.qnt                          # symbolic model checking (via Apalache)
```
