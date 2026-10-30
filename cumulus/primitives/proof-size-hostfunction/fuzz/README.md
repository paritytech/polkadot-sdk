# Fuzzer for the storage-root PoV size estimation

Notes for PR [#10215](https://github.com/paritytech/polkadot-sdk/pull/10215), written against
head `0732e39745d`.

## What is tested

1. **No overshoot.** After the last estimation, `storage_root` must not add a byte to the proof
   recorder.
2. **Validator agreement.** Weight reclaim writes the estimation values into `BlockWeight`, so
   the validator must obtain the collator's values from the collator's proof, without missing a
   node.

The harness calls the real native host functions (`sp_io::storage`,
`sp_io::default_child_storage`, `storage_proof_size`) on a real `Ext`, overlay, trie backend and
`Recorder`. It does not execute WASM.

## Layout

| File | Content |
|---|---|
| `src/lib.rs` | input types, the three passes, invariants, `shrink`, `pinned_scripts`, `script_from_seed` |
| `tests/smoke.rs` | `pinned_reproductions` and `random_smoke`, plain `cargo test` |
| `fuzz_targets/pov_estimation.rs` | libFuzzer entry point |

In `src/lib.rs`:

- `try_run_script` runs the collator pass with its validator replays, then the truth pass, and
  returns the first `Failure` or a `Report`. `run_script` panics with the script on failure.
- `Ctx::initialize_block`, `Ctx::apply_extrinsic`, `Ctx::finalize_block` are the runtime side
  of a block. Collator and validator call the same functions.
- `BlockEnv` holds recorder, extensions, state and overlay of one block and does the host
  transactions. `BlockWitness` is what the collator hands to `validate_block`: pre- and
  post-block root, proof, estimation values.
- `shrink` removes blocks, extrinsics, initial entries and ops one at a time and keeps every
  removal that preserves the `FailureKind`. It lowers the scale first.

## Passes

1. **Collator pass**: each block as `sc-block-builder` builds it, with estimations.
2. **Validator pass**, per block: the same block as `validate_block` runs it, on the collator's
   proof.
3. **Truth pass**: the collator pass without estimations. Its recorder size after `storage_root`
   is the reference for `IncompleteProof` and the overestimation statistic.

### Collator pass

Every runtime call runs in a host transaction over overlay, recorder and extensions, as
`sp-api` does, and gets a fresh trie backend over the block state, as `contextual_call` does.
Extrinsics with `client_outcome == Rollback` are rolled back by the host and dropped from the
block.

```
block
  host tx { init ops }
  for each extrinsic:
    host tx {
      Estimate                     weight-reclaim validate()          if with_reclaim
      pre ops
      runtime tx { body ops }      with_storage_layer
      commit | rollback            body_outcome
      post ops
      Estimate                     weight-reclaim post_dispatch()     if with_reclaim
    }                              client_outcome
  host tx { finalize ops, Estimate, Estimate, storage_root }
  take proof, validator pass, apply changes to the state; next block with fresh overlay and recorder
```

`Mode::Free` also executes every `Op::Estimate` in the op lists. `Op::Tx` is a nested runtime
transaction, depth up to six. Runtime transactions do not touch the recorder; host transactions
roll recorded nodes back.

### Validator pass

One `Ext` over `TrieBackendBuilder::new(proof.into_memory_db(), pre_root)` with a fresh
`Recorder`, no host transactions. It runs `init`, the committed extrinsics through
`Ctx::apply_extrinsic`, `finalize`, the two final estimations and `storage::root`, then checks
in this order:

- a `trie`/`state` warning or a panic: `ValidatorMissingNode`;
- a different post-block root: `ValidatorRootMismatch`;
- a different sequence of `(EstimatePos, value)`: `ValidatorEstimateMismatch`, detail names the
  first differing position and prints both sequences.

A validator failure is reported before a collator failure of the same block; the collator
failure is appended as "collator also: ...".

The harness uses `Recorder` where production uses `SizeOnlyRecorderProvider`;
`recorder_equivalence_*` in `validate_block/trie_recorder.rs` show they report the same size.

### Inputs

`Key::Map(p, m, i)` is a 50 byte `prefix ++ hash ++ i` key like a `Blake2_128Concat` entry,
`Key::Value(p, m)` its 32 byte prefix, `Key::Sibling` a map key with one nibble flipped,
`Key::Short` a one to three byte key, `Key::Raw` explicit bytes for hand-written scripts.
Indices are reduced modulo the counts of the script's scale. `Val` sizes: empty, inline, exactly
32 bytes, hashed out under V1.

| Scale | Pallets × items × indices | Initial main trie | Per child | Clear-prefix limit |
|---|---|---|---|---|
| `Small` | 4 × 3 × 128 | up to 600 drawn, about 300 distinct | up to 100 | below 8 |
| `Medium` | 16 × 8 × 4096 | up to 5000 | up to 1000 | below 64 |
| `Large` | 16 × 8 × 65536 | up to 50000 | up to 5000 | below 1024 |

Drawn 70 / 25 / 5 percent. `random_smoke` caps at `Small` unless `POV_FUZZ_SCALE` says
otherwise; the libFuzzer target has no cap. Up to three of four child tries start populated;
child `id` has storage key `[0x10 * (id + 1)]`. Scripts also choose backend and root state
version and whether a `LocalTrieCache` is attached.

## Invariants

| Kind | Check |
|---|---|
| `Overshoot` | recorder size after `storage_root` exceeds the last estimation |
| `NotIdempotent` | the two final estimations differ |
| `NotMonotone` | an estimation decreased without a host rollback in between |
| `NegativeReclaim` | `post < pre` for an extrinsic with reclaim |
| `Warning` | a `log` warning with target `trie` or `state` in the collator pass |
| `IncompleteProof` | the collator pass recorded fewer bytes after `storage_root` than the truth pass |
| `Panic` | a collator-pass panic that does not happen without estimation |
| `PanicAlsoWithoutEstimation` | a collator-pass panic that also happens without estimation |
| `ValidatorMissingNode` | validator pass panicked or logged a `trie`/`state` warning |
| `ValidatorRootMismatch` | validator pass computed a different post-block root |
| `ValidatorEstimateMismatch` | validator pass saw a different estimation value |

`IncompleteProof` is the weaker form of `ValidatorMissingNode` and can go once the validator
pass is trusted; the truth pass then only serves the overestimation statistic, which is printed
and never fails a run.

## Running

```sh
# pinned reproductions plus 300 random scripts; stops at the first failure and prints the shrunk script
cargo test --release -p cumulus-proof-size-hostfunction-fuzz -- --nocapture

# all failures of a longer run, grouped by kind, one shrunk example per kind
POV_FUZZ_ITERS=1000 POV_FUZZ_KEEP_GOING=1 cargo test --release -p cumulus-proof-size-hostfunction-fuzz --test smoke random_smoke -- --nocapture

# with medium and large tries
POV_FUZZ_SCALE=large POV_FUZZ_ITERS=200 POV_FUZZ_KEEP_GOING=1 cargo test --release -p cumulus-proof-size-hostfunction-fuzz --test smoke random_smoke -- --nocapture

# replay one seed
POV_FUZZ_SEED=0x5eed0f1022d POV_FUZZ_ITERS=1 cargo test --release -p cumulus-proof-size-hostfunction-fuzz --test smoke random_smoke -- --nocapture

# one pinned script with logs; POV_FUZZ_LOG takes RUST_LOG syntax and also prints pass, runtime
# call and estimation markers
POV_FUZZ_PINNED=review_parent_update_hides_nested_delete POV_FUZZ_LOG=proof_size_tracker=trace,state=trace cargo test --release -p cumulus-proof-size-hostfunction-fuzz --test smoke pinned_reproductions -- --nocapture

# coverage-guided, needs cargo-fuzz and nightly
cd cumulus/primitives/proof-size-hostfunction/fuzz && cargo +nightly fuzz run pov_estimation -- -max_len=16384
```

`POV_FUZZ_SEED` takes decimal or `0x` hex; script `i` uses `base + i`. The libFuzzer binary
builds on stable but has no coverage feedback without `cargo fuzz`.

A failure prints `Kind in block N: detail` and the shrunk script. To pin it, add it to
`pinned_scripts()` with `Key::Raw` keys and the seed. To turn it into an `sp-state-machine`
test, build the `Storage` from `InitialTrie::storage()`, replay the ops on a recording `Ext` as
the tests in `ext.rs` do, and map the estimation points to `ext.record_proof_for_dirty_keys()`.

## Status on head `0732e39745d`

Both tests fail. Pinned scripts:

| Name | Result | Cause |
|---|---|---|
| `review_incremental_main_deletions` | Overshoot, 174 to 209 | deletions spread over several estimations never collapse the branch; each estimation starts from the original root and removes only its own keys (`record_proof_for_dirty_keys`) |
| `review_incremental_child_root_deletions` | Overshoot, 269 to 305 | same, for child root keys |
| `review_parent_update_hides_nested_delete` | Overshoot, 106 to 141 | `take_delta` lets an older layer's `Updated` overwrite the current layer's `Deleted` (unconditional `delta.insert`); both sides agree because the nested runtime transaction is a layer on both |
| `fuzz_client_layer_update_hides_finalize_delete` | ValidatorEstimateMismatch at `Final(0)`, 106 vs 141; collator also Overshoot | same cause with host layers: write in one runtime call, delete in a later one; the validator has both in one layer |
| `fuzz_child_emptied_then_refilled` | Overshoot, 121 to 123 | child emptied and refilled before the estimation; V0 and V1 |
| `fuzz_estimation_hits_trie_db_remove_panic` | Panic | `TrieDBMut::remove` of an absent key ending at a nibbled branch with a non-empty partial panics in `fix` or deletes a neighbouring value (trie-db 0.31.0, `remove_inspector`); pre-existing, now reached by estimating a rolled-back deletion |
| `validator_pre_delete_after_init_write` | ValidatorEstimateMismatch at `Free(0, 0)`, 106 vs 141 | `init` writes, an extrinsic without reclaim deletes and estimates |
| `validator_pre_reinsert_after_init_delete` | ValidatorEstimateMismatch at `Free(0, 0)`, 141 vs 106 | mirror image; no collator-side check fails |
| `validator_nested_estimate_after_delete` | ValidatorEstimateMismatch at `Free(0, 0)`, 106 vs 141 | delete in `pre`, estimation inside a nested runtime transaction |
| `fuzz_child_root_cache_survives_client_rollback` | pass | failed while the harness kept one trie backend per block; production and the harness now use one per runtime call, so the child root cache dies with the rolled-back call |
| `pr_*`, `executive_inserts_and_updates` | pass | happy paths from the PR's own tests |

1000 random scripts at small scale: 103 fail. `Overshoot` 78, `Panic` 1,
`PanicAlsoWithoutEstimation` 8, `ValidatorEstimateMismatch` 9, `ValidatorMissingNode` 7. Every
validator failure is at the `Final` estimation the harness adds after `finalize`, in the
`fuzz_client_layer_update_hides_finalize_delete` shape, and none at `Pre` or `Post`.

The collator and validator diverge when a key is written in one runtime call and changed in a
later one outside a runtime transaction, with no estimation in between and one after the
change. The weight-reclaim pallet estimates in `validate()` before the extrinsic writes, so
with its estimation points the condition is not met. It is met by the deprecated
`StorageWeightReclaim` extension, which estimates in `prepare`, and by any pallet calling
`get_proof_size` after its own writes. Fixing `take_delta` so the newest op per key wins removes
the divergence and the overshoot cases of the same cause.

## Caveats

- The harness estimates after `finalize`; production does not, so `on_finalize` writes are never
  estimated there. Mismatches at `Final(_)` need a pallet that estimates in `on_finalize` to
  matter.
- Warning capture needs `log::set_logger` to succeed; another logger in the same binary silences
  the `Warning` kinds.
- Even at `Large` the key space has 16 pallets and 8 items; real state branches far wider near
  the root and maps are far larger.
- The validator pass models one block per PoV, uses a plain `StorageProof`, and its backend has
  the `std` child root cache that the `no_std` validator lacks.
- No WASM execution: the `#[version(2)]` dispatch and the `ValidationExternalities` guard are not
  covered.
