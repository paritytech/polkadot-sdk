# JAM collator PoC — status

Four repos under `~/projects/parity/`. The PoC runs; one blocker and one pre-existing test
failure are outstanding.

| repo | branch | state |
|---|---|---|
| `parachain-service` | `mku-jam-collator-poc-2` | committed — `3f90b52 Remove pvm-builder` |
| `polkajam2` | `mku-jam-collator-poc-2` | committed — `3ecd9ba0 Export some stuff` |
| `polkadot-sdk3` | `parchain-service` | **4 files uncommitted** |
| `zombienet-sdk` | `jam-integration` | committed — `c15eb16 Fix version` |

## Blocker: the submodule pin is orphaned and the API moved

`parachain-service` pins `vendor/polkajam` at **`abefc71c`**, which no branch contains. The object
still exists locally, so builds work here and nowhere else.

Repointing to `3ecd9ba0` is **not** a drop-in: `cargo_jam_build::program()` and the `Blob.program`
field were removed there, and `service/bin/src/lib.rs` calls `program("frameless")` in
`frameless_pvf()`. The frameless PVF is the *raw linked program*, not the `.jam` container —
`parse_pvf` reads it with `polkavm::ProgramParts::from_bytes`, so handing it a container fails at
runtime, not at build time.

`link.rs:31` still writes `{name}.polkavm` beside the container, so the remedy is to derive it
from `Blob::path`:

```rust
let built = cargo_jam_build::build_with("frameless", None, &Options::default())?;
std::fs::read(built.path.with_extension("polkavm"))
```

Everything else `parachain-service` uses survived at `3ecd9ba0`: `blob()`, `hash()`, the `Generic`
blob type, and all five manifest keys (`type`, `name`, `profile`, `cargo-args`, `rustflags`).

## What changed

**`parachain-service` (committed).** `tools/pvm-builder` deleted (747 lines), along with every
`build.rs`, the three blob-only `*-bin` crates, and all `jam-pvm-builder`/`pvm_binary!` use. Blobs
now come from `cargo-jam-build` on demand, cached per process, via plain accessor functions in
`service/bin/src/lib.rs`. Each guest crate declares its own build in `[package.metadata.jam]` —
authorizers pin `profile = "production-authorizer"`, frameless carries
`cargo-args`/`rustflags`.

Blob output moved to stable paths, so nothing has to hunt for a build-hash directory:

```
target/jam/riscv64emac-unknown-none-polkavm/production/parasim-service.jam
target/jam/riscv64emac-unknown-none-polkavm/production-authorizer/parachain-authorizer-sr25519.jam
```

**`polkadot-sdk3` (uncommitted).** `jam-tests`: dropped the relay filler so the orchestrator takes
its `spawn_jam` path (upstream made the dispatch either/or, so a filler silently shadowed the
jamchain and `jam_spec.json` was never written); dropped `relay_node` and the PVF-worker gate from
`env.rs`; README updated for `cargo jam-build` and the new paths. Plus a `Cargo.lock` bump for
zombienet 0.4.18.

**`zombienet-sdk`.** Moved from `mku-chainspec` to `jam-integration` — the former's work was
already upstream as PR #585, byte-identical. A merge had left `[workspace.package].version` at
`0.4.17` while the internal deps wanted `0.4.18`, so nothing resolved; fixed.

## Verification

- `cargo build --workspace --all-targets` clean in `parachain-service` and `polkajam2`
  (`corevm-monitor` needs `bun`, environmental).
- `cargo test --workspace` in `parachain-service`: **112 passed, 1 failed** (below).
- PoC `jam::collator_progress::one_jam_collator_builds_blocks`: **best 30 / finalized 26, 201 s**.
  All four progress tests passed earlier at 815 s total.
- Blobs byte-identical to the pre-migration `pvm-builder` output — sr25519 authorizer
  `f829d976…`, ed25519 `348b45ba…`, frameless PVF `ba730cc9…`. `cargo-jam-build` is also
  reproducible across runs and target dirs.

### Gas fixtures re-baselined

Seven pins in `service/bin/tests/accumulate_gas.rs` were stale. They were measured on 2026-08-19
(`293cad6`) against a single **101,073 B** combined authorizer; `90d6e74` later split it
per-scheme and rewrote `is_authorized.rs` (75 lines), so the measured program became a different
**61,230 B** one. Nothing re-measured because the test binary would not build (below), so the
drift went unnoticed.

`worst_case_margin_works` — the safety gate, untouched — re-validates every new pin, and all 9 gas
tests pass. Two numbers are tight:

- `SOLICIT_FLOOD` leaves 21.65 % of Ga free against a 20 % floor: **165,232 gas of slack**, down
  from 232,629.
- the sr25519 authorizer is **96.6 % of the 64 kB `C_maxauthcodesize`** (2,205 B spare), with
  `opt-level = "z"` already the smallest setting. Nothing asserts this limit today.

## Open items

1. **Repoint `vendor/polkajam`** off the orphaned `abefc71c`, reworking `frameless_pvf()` as above.
2. **Commit the `polkadot-sdk3` changes** (4 files).
3. **`accumulate_preimages::shared_referencer_leaves_works` fails** — pre-existing, not from this
   work: the file is untouched, the test was unbuildable before, and this branch's preimage code
   diverges ~152 lines from `parachain-service-poc` across `preimage_registry.rs`,
   `accumulate/{mod,package}.rs`, `state_balance.rs`. Likely wants that branch's fix ported.
4. **Consider a size assertion** on the authorizer blobs, given the 2,205 B headroom.
5. `jam::core_assignment`'s two dynamic-core tests need `PARASIM_TOOL_BIN`
   (`cargo build --release -p parasim-tool`); never run here.

## Why the tests could not build (context for 3)

Two independent pre-existing failures, reproduced from a pristine worktree at the previous state:

- `executor` — `unresolved imports jam_node::vm::{RefineCallContext, RefineCallContextOwned}`.
  polkajam `de24a605` had gated `vm::testing` on `hazmat` *specifically so `executor` could
  build*; `af6b216` reverted that to `#[cfg(test)]`. Fixed by re-exporting through
  `vm_for_tests`.
- `jam-bootstrap-service-bin`'s build script — `` `.json` target specs require
  -Zjson-target-spec ``, because the vendored `jam-pvm-builder` never passes it on cargo ≥ 1.95
  (here 1.97.1). This is also why the crates.io `jam-pvm-builder` route taken on
  `parachain-service-poc` (`48dd151`) does not work on this toolchain.

`service/bin`'s dev-dependency enables its own `test-utils`, which pulls `dep:executor`, so
`cargo test -p parachain-service-bin` exited 101 and no test in that crate ran.

## Running the PoC

```sh
# parachain-service
cargo build --release -p cargo-jam-build
./target/release/cargo-jam-build -p parasim-service -p parachain-authorizer-sr25519

# polkadot-sdk3
export JAM_NODE_BIN=../polkajam2/target/release/polkajam
export PARASIM_BLOB=../parachain-service/target/jam/riscv64emac-unknown-none-polkavm/\
production/parasim-service.jam
export AUTHORIZER_BLOB=../parachain-service/target/jam/riscv64emac-unknown-none-polkavm/\
production-authorizer/parachain-authorizer-sr25519.jam
cargo test --release -p cumulus-jam-zombienet-tests --features jam-ci --test tests \
	-- --test-threads 1 --nocapture jam::collator_progress
```

`JAM_GENSPEC_BIN` stays unset — one `polkajam` binary serves both `gen-spec` and the `stateValue`
RPC. Submodule URLs in `parachain-service/.gitmodules` are HTTPS and fail auth; local `git config
submodule.<path>.url` overrides point at SSH or a local clone.
