# Fuzzer for the storage-root PoV size estimation

Hand-off notes for PR [#10215](https://github.com/paritytech/polkadot-sdk/pull/10215)
("PoV size reclaim fix"). Written against head `0732e39745d` on 2026-10-16. Nothing in this
crate is committed yet; it lives in the working tree of the `mku-pov-reclaim-fix-1` branch.

## What is being tested

The PR adds version 2 of the `storage_proof_size` host function. Before returning the proof
size it calls `Externalities::record_proof_for_dirty_keys`, which walks the trie for every key
written since the previous call and records the nodes that the later `storage_root` will touch.
Weight reclaim reads the proof size before and after each extrinsic, so if the estimation misses
nodes, the block's real PoV is larger than what the runtime accounted for.

The property under test: **after the last estimation, `storage_root` must not add a single byte
to the proof recorder.**

Call chain on the production side:

```
storage_proof_size::storage_proof_size()  v2    cumulus/primitives/proof-size-hostfunction/src/lib.rs
  Ext::record_proof_for_dirty_keys              substrate/primitives/state-machine/src/ext.rs
  OverlayedChanges::record_proof_for_dirty_keys substrate/primitives/state-machine/src/overlayed_changes/mod.rs
    StorageKeyDeltaTracker::take_delta          .../overlayed_changes/storage_key_delta_tracker.rs
  TrieBackendEssence::record_proof_for_dirty_keys       .../trie_backend_essence.rs
    read_trie_keys_from_delta   (TrieDB::get_hash)      substrate/primitives/trie/src/lib.rs
    remove_trie_keys_from_delta (TrieDBMut::remove, no commit)
  TrieBackendEssence::record_proof_for_child_dirty_keys
```

The harness drives all of this through the real native host functions (`sp_io::storage`,
`sp_io::default_child_storage`, `storage_proof_size`) with a real `Ext`, overlay, trie backend
and `Recorder`. Nothing is mocked. It does not execute WASM.

## Layout

| File | Content |
|---|---|
| `src/lib.rs` | Input types (`Script`, `Block`, `Xt`, `Op`, `Key`, `Val`), `run_script` / `try_run_script`, invariants, `shrink`, `pinned_scripts`, `script_from_seed` |
| `tests/smoke.rs` | Deterministic runner: `pinned_reproductions` and `random_smoke`. Works with plain `cargo test`. |
| `fuzz_targets/pov_estimation.rs` | libFuzzer entry point for `cargo fuzz`. |

Key functions in `src/lib.rs`:

- `execute(script, estimate)` runs one script once. With `estimate == false` every estimation
  call is skipped; that run is the "truth".
- `try_run_script` runs with estimation, then without, and compares. Returns `Err(Failure)` on
  the first hard invariant violation, otherwise a `Report` with per-block numbers.
- `BlockEnv` holds the recorder, extensions, trie backend and overlay of one block and
  implements the client-side transaction handling.
- `Ctx` executes ops inside the externalities and tracks estimation values.
- `shrink` is a greedy minimiser: it drops blocks, extrinsics, initial entries and single ops
  while the same `FailureKind` persists.

## How a script executes

Each block is executed the way `sc-block-builder`, `sp-api` and `frame-executive` execute it.
`sp-api` wraps every runtime call at depth zero in a host transaction that starts the overlay
transaction, the recorder transaction and `Extensions::start_transaction(Host)` together
(`substrate/primitives/api/proc-macro/src/impl_runtime_apis.rs`, `start_transaction`).
`BlockEnv::client_start` does the same three calls.

```
block
  host tx { initialize_block: init ops }                         committed
  for each extrinsic:
    host tx {
      Estimate                     weight-reclaim validate()          if with_reclaim
      pre ops                      nonce, fee withdrawal
      runtime tx { body ops }      with_storage_layer around the call
      commit | rollback            body_outcome
      post ops                     events append, fee refund
      Estimate                     weight-reclaim post_dispatch()     if with_reclaim
    }                              client_outcome: commit, or rollback for an invalid extrinsic
  host tx {
    finalize_block: finalize ops
    Estimate, Estimate             final estimations (see caveats)
    storage_root
  }
  drain overlay into the backend, next block starts with a fresh overlay and recorder
```

`Mode::Free` additionally executes every `Op::Estimate` wherever it appears in an op list. This
covers callers outside the executive; `get_proof_size` is public and any pallet may call it.
In `Mode::Executive` those ops do nothing.

Op lists may contain `Op::Tx`, a nested runtime transaction, up to depth six. Runtime
transactions do not affect the recorder; the `ProofSizeProvider` transaction hooks are no-ops
for `Recorder`. Only the host transactions roll recorded nodes back.

Keys are described symbolically so that `Debug` output stays short and libFuzzer mutations stay
meaningful. `Key::Map(p, m, i)` is a 50 byte `prefix ++ hash ++ i` key like a `Blake2_128Concat`
map entry, `Key::Value(p, m)` the 32 byte prefix of those (a branch node with a value),
`Key::Sibling` a map key with one nibble changed at a random position, `Key::Short` a one to
three byte key near the root, `Key::Raw` explicit bytes for hand-written scripts. The indices are
reduced modulo the pallet, item and index counts of the script's scale. `Val` has four size
classes: empty, inline, exactly 32 bytes, and hashed out under `StateVersion::V1`.

### Scale

`Config::scale` sets the key space and the initial trie size:

| Scale | Pallets × items × indices | Initial main trie | Per child | Clear-prefix limit | Cost |
|---|---|---|---|---|---|
| `Small` | 4 × 3 × 128 = 1536 map keys | up to 600 generated keys (about 300 distinct) | up to 100 | below 8 | milliseconds |
| `Medium` | 16 × 8 × 4096 | up to 5000 | up to 1000 | below 64 | seconds in a debug build |
| `Large` | 16 × 8 × 65536 | up to 50000 | up to 5000 | below 1024 | about a minute in a debug build |

Every script runs twice (with and without estimation), and a failing script is shrunk by
re-running it many times. A 40-script run with all classes allowed took eight minutes as a debug
build; use `--release` for anything beyond a handful of medium or large scripts.

Generation draws `Small` 70%, `Medium` 25%, `Large` 5%. The shrinker lowers the scale first, so
failing cases end up small. `random_smoke` caps the scale at `Small` unless `POV_FUZZ_SCALE` is
set; the libFuzzer target has no cap.

Why scale matters for this invariant: the recorder deduplicates nodes. In a 300-key trie almost
every operation shares nodes with every other one, so a node the estimation misses may already
have been recorded by an unrelated read and the check passes. In a production-sized trie that
read touches different nodes and the block overshoots. The truth run deduplicates the same way
and cannot catch this; only a larger key space reduces the accidental overlap. On the other
hand, `Key::Sibling` makes small tries deep: a lookup path crosses up to fifteen nodes because
the flipped nibbles split the shared prefixes at many positions.

The initial trie also contains explicit entries and up to three of the four addressable child
tries. Child `id` has storage key `[0x10 * (id + 1)]`. Scripts further choose backend and root
state version, and whether a `LocalTrieCache` is attached.

## Invariants

`FailureKind` in `src/lib.rs`:

| Kind | Check |
|---|---|
| `Overshoot` | recorder size after `storage_root` is larger than the last estimation. This is the property the PR has to guarantee. |
| `NotIdempotent` | two consecutive final estimations differ |
| `NotMonotone` | an estimation is smaller than the previous one without a host rollback in between |
| `NegativeReclaim` | `post < pre` for an extrinsic with reclaim; weight reclaim would saturate |
| `Warning` | a `log` warning with target `trie` or `state` was emitted |
| `IncompleteProof` | the run with estimations recorded fewer bytes after `storage_root` than the run without; the proof misses nodes a validator reads |
| `Panic` | a panic that does not happen without estimation |
| `PanicAlsoWithoutEstimation` | a panic that also happens without estimation, so a pre-existing bug |

The soft statistic `overestimation = e_last - truth` is printed by `random_smoke` and never
fails a run. The documented source of overestimation is a child trie that may become empty:
the estimation assumes its root key is removed from the main trie.

## Running

From the repository root:

```sh
# pinned reproductions plus 300 random scripts; stops at the first failure and prints the shrunk script
cargo test -p cumulus-proof-size-hostfunction-fuzz -- --nocapture

# collect all failures of a longer run, grouped by kind, one shrunk example per kind (about two minutes)
POV_FUZZ_ITERS=3000 POV_FUZZ_KEEP_GOING=1 cargo test -p cumulus-proof-size-hostfunction-fuzz --test smoke random_smoke -- --nocapture

# the same with medium and large tries allowed; release build, since large tries are slow in debug
POV_FUZZ_SCALE=large POV_FUZZ_ITERS=200 POV_FUZZ_KEEP_GOING=1 cargo test --release -p cumulus-proof-size-hostfunction-fuzz --test smoke random_smoke -- --nocapture

# replay a single seed
POV_FUZZ_SEED=0x5eed0f1022d POV_FUZZ_ITERS=1 cargo test -p cumulus-proof-size-hostfunction-fuzz --test smoke random_smoke -- --nocapture

# coverage-guided, needs `cargo install cargo-fuzz` and a nightly toolchain
cd cumulus/primitives/proof-size-hostfunction/fuzz && cargo +nightly fuzz run pov_estimation -- -max_len=16384
```

Environment variables of `random_smoke`: `POV_FUZZ_ITERS` (default 300), `POV_FUZZ_SEED` (base
seed, decimal or `0x` hex; script `i` uses `base + i`), `POV_FUZZ_KEEP_GOING=1`,
`POV_FUZZ_SCALE` (`small` by default, `medium` or `large` to allow the bigger classes; larger
generated scales are reduced to the cap, including their trie sizes).

The libFuzzer binary builds on stable (`cargo build -p cumulus-proof-size-hostfunction-fuzz
--bins`) but gets no coverage feedback without `cargo fuzz`. `cargo-fuzz` and `cargo-hfuzz`
are not installed in the development environment used so far.

## Reading a failure

A failure prints as `Kind in block N: detail`, followed by the shrunk script in `Debug` form.
To turn a shrunk script into a unit test in `sp-state-machine`:

1. Build the initial `Storage` from `InitialTrie::storage()` or by hand; `Key::bytes()` and
   `Val::bytes()` give the concrete bytes.
2. Replay the ops on an `Ext` with a recording backend, as the tests at the end of
   `substrate/primitives/state-machine/src/ext.rs` do. `Op::Estimate` and the fixed estimation
   points map to `ext.record_proof_for_dirty_keys()`.
3. To pin the case in this crate, add it to `pinned_scripts()` with `Key::Raw` keys and a
   comment stating the seed it came from.

`shrink` is greedy and recomputes all candidates after every successful removal. For scripts
with several hundred ops it takes seconds to a minute.

## Status on head `0732e39745d`

Both tests fail on purpose. The failures are real defects, not harness bugs; each pinned script
was minimised by hand and the two trie-level ones were reproduced outside the harness.

Pinned scripts (`pinned_reproductions`):

| Name | Result | Cause |
|---|---|---|
| `review_incremental_main_deletions` | Overshoot, 174 to 209 | deletions spread over several estimations never collapse the branch; each estimation starts from the original root and removes only its own keys (`trie_backend_essence.rs`, `record_proof_for_dirty_keys`) |
| `review_incremental_child_root_deletions` | Overshoot, 269 to 305 | same, for child root keys in the main trie |
| `review_parent_update_hides_nested_delete` | Overshoot, 106 to 141 | `take_delta` lets an older layer's `Updated` overwrite the current layer's `Deleted` (`storage_key_delta_tracker.rs`, `take_delta`, unconditional `delta.insert`) |
| `fuzz_client_layer_update_hides_finalize_delete` | Overshoot, 106 to 141 | same root cause; a write in one runtime call and a delete in a later one is enough, because each runtime call is its own host transaction layer |
| `fuzz_child_emptied_then_refilled` | Overshoot, 121 to 123 | a child emptied and refilled before the estimation; V0 and V1 |
| `fuzz_estimation_hits_trie_db_remove_panic` | Panic | `TrieDBMut::remove` of an absent key that ends exactly at a nibbled branch with a non-empty partial panics in `fix` or deletes a neighbouring value (trie-db 0.31.0, `remove_inspector`, arm `(NibbledBranch(n, ..), true)`). Pre-existing trie-db bug; the estimation now runs `remove` for deletions inside rolled-back extrinsics, so it panics where plain execution does not |
| `fuzz_child_root_cache_survives_client_rollback` | IncompleteProof, 101 recorded vs 264 needed | `TrieBackendEssence::child_root` caches the child root outside the recorder's transactions. An estimation inside a rolled-back host transaction leaves the root cached while the recorder drops the top-trie path; a later child read hits the cache and never records the path. Pre-existing mechanism, new trigger |
| `pr_main_trie_changes`, `pr_child_is_created`, `pr_child_is_emptied`, `executive_inserts_and_updates` | pass | happy paths from the PR's own tests |

A 3000-script `random_smoke` run fails in roughly eight percent of the scripts. By kind:
`Overshoot` dominates and is almost always the branch-collapse case in executive shape, often
through `ClearPrefix`. `IncompleteProof`, `Panic` and `PanicAlsoWithoutEstimation` are the
trie-db and cache cases above. Among passing scripts the overestimation per block averages
about 40 bytes with outliers around 2 KiB.

The three `review_*` cases match the byte counts in the local review notes
(`review-notes/pr-10215.md`, not part of the repository) exactly.

## Caveats of the model

- **Final estimation.** The harness estimates once more after `finalize` ops and before
  `storage_root`. Production does not; the last real estimation is the post-dispatch read of the
  last extrinsic, so writes in `on_finalize` are never estimated. The invariant checked here is
  therefore stricter than what production can rely on. The tracker bugs found are real
  independent of this.
- **Warning capture** relies on `log::set_logger` succeeding. If another logger is installed
  first (for example by `sp_tracing` in the same test binary) the `Warning` kind is silent.
- **Real keys** are `twox128 ++ twox128 ++ hash`. The symbolic keys approximate that shape, but
  even at `Large` there are 16 pallets and 8 items, so the first-level branching of a real state
  with thousands of prefixes is not reproduced, and maps stay far below production sizes.
- **Overestimation** has no principled bound and is informational only.
- **No WASM execution.** The `#[version(2)]` dispatch and the `ValidationExternalities` panic
  guard are not covered. A tiny runtime executed through `sc_executor::WasmExecutor` would be
  the next layer; not started.
- The harness cannot see validation-side overshoot; there the proof is given.

## Picking this up

1. Keep the oracle as is. When the PR is fixed, the seven failing pinned scripts must pass; do
   not relax `after <= e_last`.
2. For every fix iteration, run the default smoke test first, then the 3000-script keep-going
   run, and pin any new shrunk example with its seed.
3. The trie-db removal bug needs an upstream fix in `paritytech/trie` regardless of this PR.
   The estimation should also not run `remove` for keys that were never read, or should catch
   the error; either way the host function must not panic on a rolled-back deletion.
4. The child-root cache needs to participate in the recorder's host transactions, or the
   estimation must bypass it. Check whether production block building can hit it: it needs a
   host rollback after a child-root lookup, that is an extrinsic that fails after dispatch
   or reads a child trie before failing validation.
5. Once `cargo-fuzz` is available, run the libFuzzer target per configuration for a few hours
   each and feed the crash corpus back into `pinned_scripts`.
6. Candidates for more coverage: `storage::read` with offsets, `next_key` loops over child
   tries, `Op::ChildRoot` followed by further child writes in the same extrinsic, and larger
   initial tries with more than four pallet prefixes.
