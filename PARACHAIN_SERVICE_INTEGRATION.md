# Parachain Service Integration (parachains on JAM)

Working notes for the `parchain-service` branch: adapting the Polkadot SDK so a parachain
runtime can execute as a JAM service on PolkaVM.

This document records the **current state**, the **design decisions and why they were forced**,
the **verification evidence**, and the **open blockers**. It is a handoff record, not a spec.

- Branch: `parchain-service`
- HEAD: `6831c144e2f` (merge of PR #8641) on top of `28e8b899567` ("Adds additional data")
- Authoritative design decision: [parachain-service#13](https://github.com/paritytech/parachain-service/issues/13)
- Spec/design docs live outside this repo, in `polkadot-sdk2` branch `bkchr-parachain-service-doc`
  (`designs/parachain-service-on-jam/`) and the PoC in the `parachain-service` repo.

## 1. Merged upstream work

PR [#8641](https://github.com/paritytech/polkadot-sdk/pull/8641) — *RFC-145: remove the host-side
runtime memory allocator* — is merged into this branch (107 files, +5086/-1129). It is still open
upstream; it was merged here because it rewrites the `sp-io` host-function signatures wholesale
(`PassFatPointerAndRead` / `PassPointerAndWrite` / `AllocateAndReturnByCodec`) and adds the `Input`
host function, so doing the riscv split on top of it avoids a painful rebase.

RFC-145 also established the file-per-target pattern this work follows: it replaced
`sp-io/src/global_alloc.rs` with `global_alloc_wasm.rs` + `global_alloc_riscv.rs`, the latter using
a `grow_heap` PolkaVM host call.

## 2. The decision that drives everything

From issue #13, after crypto benchmarking on validator-grade hardware (bkchr):

> Decision: We gonna move ahead without any special host functions.
> We assume that in the future there will be extra instructions to speed up the execution of
> certain actions.

Measured in-blob PVM cost vs native host was **1.2x-7x** (hashing 1.2-2.0x; `sha2_256` worst at
5.8-7.0x because the host baseline uses SHA-NI; signatures/recovery 1.4-2.8x; elliptic curves
1.7-2.4x). A mediated host call from parachain code costs ~6.0 us. That penalty was accepted.

Consequence for `sp-io`: on riscv, **only the storage-related host functions remain**; everything
else must be computed inside the blob.

## 3. What was done to `sp-io`

`substrate/primitives/io/src/lib.rs` went from 4303 to **980** lines. Its 14
`#[runtime_interface]` traits were moved out **byte-for-byte** (verified) into one file per trait,
and a parallel native implementation tree was added.

```
substrate/primitives/io/src/
  lib.rs                    <- crate docs, types, extensions, panic/oom, re-exports
  host_functions/           <- 14 files, one per #[runtime_interface] trait (verbatim moves)
  native/                   <- 11 modules of in-blob implementations, riscv only
  global_alloc_wasm.rs      <- from RFC-145
  global_alloc_riscv.rs     <- from RFC-145 (grow_heap)
```

### 3.1 Why it is built this way (the macro could not be changed)

The `#[runtime_interface]` proc macro must not be modified (hard constraint). But it emits a host
call for **both** wasm and riscv:

- `proc-macro/src/runtime_interface/bare_function_interface.rs`, `function_no_std_impl` emits
  `#[cfg(substrate_runtime)] pub fn f(..) { host_f.get()(..) }` — unconditionally a host call.
- `proc-macro/src/runtime_interface/host_function_interface.rs:147` turns that into
  `#[cfg_attr(target_arch = "riscv64", polkavm_import(abi = polkavm_abi))]`.
- `substrate_runtime` is set by wasm-builder for **both** wasm and riscv.

There is no cfg path where a `substrate_runtime` build calls the plain Rust method body. So the only
way to get native calls on riscv without touching the macro is to **not compile those traits on
riscv** and supply a parallel native module — exactly the `global_alloc_{wasm,riscv}.rs` pattern.

**The move is ABI-neutral.** The extern symbol name is built in
`proc-macro/src/utils.rs:213` as `format!("ext_{}_{}_version_{}", trait_name.to_snake_case(), name, version)`
— derived from the *trait name*, never the module path. Moving a trait between modules cannot
change a single exported/imported symbol.

**The blob-visible surface is smaller than it looks.** `function_std_impl` emits the `_version_N`
functions as `#[cfg(not(substrate_runtime))]` *private* fns, i.e. host-side only. So a runtime blob
only ever sees: the bare latest-callable-version fn (named `f` or `f__raw` for `#[raw_api]`), plus
the `#[wrapper]` convenience fns (which are already in-blob code calling `f__raw`). That is
**100 functions** across the 11 non-storage modules.

### 3.2 Gating

In `host_functions/mod.rs`:

- `storage`, `default_child_storage`, `input` — **no target gate**; host functions on every target.
- the other 11 — `#[cfg(any(not(substrate_runtime), target_family = "wasm"))]`, i.e. compiled for
  the host side (needed by `SubstrateHostFunctions`) and for wasm runtimes, but **not** for riscv.

In `lib.rs`, `mod native;` and its re-exports are gated
`#[cfg(all(substrate_runtime, any(target_arch = "riscv32", target_arch = "riscv64")))]`.
Every public path (`sp_io::hashing`, `sp_io::crypto`, ...) resolves on all three configurations.

### 3.3 Per-module classification

| module | wasm | riscv (PolkaVM/JAM) |
|---|---|---|
| `storage`, `default_child_storage` | host fn | **host fn** (retained per #13) |
| `input` | host fn | **host fn** (a guest cannot synthesise its own input) |
| `hashing` (16 fns) | host fn | **native**, bodies lifted verbatim (`sp_crypto_hashing`) |
| `trie` (10 fns) | host fn | **native** via `sp-trie` `LayoutV0/V1` + local `Blake2Hasher`/`KeccakHasher` |
| `crypto` verify (`ed25519`/`sr25519`/`ecdsa`/`ecdsa_prehashed`) | host fn | **native** (`sp-core`) |
| `crypto::secp256k1_ecdsa_recover(_compressed)` | host fn | **native** via `libsecp256k1` |
| `logging`, `misc::print_*` | host fn | **native no-op** (see blocker 5.2) |
| `wasm_tracing` | host fn | **native no-op** |
| `crypto` keystore (`*_generate`, `*_sign`, `*_public_keys`) | host fn | **native, panics** |
| `offchain`, `offchain_index`, `transaction_index` | host fn | **native, panics** |
| `misc::runtime_version`, `misc::last_cursor` | host fn | **native, panics** |
| `allocator`, `panic_handler` | host fn | **native, panics / traps** (riscv allocates via `grow_heap`) |

The panicking ones need node-side state that cannot exist inside a guest blob (a keystore, a
network stack, the node's transaction index, a nested VM). Panicking means the blob emits **no host
import** for them, which is the property #13 asks for, while still failing loudly if a runtime
wrongly uses them on JAM.

Native counts per module: `crypto` 33, `offchain` 22, `hashing` 16, `trie` 10, `misc` 6,
`wasm_tracing` 4, `allocator`/`logging`/`offchain_index`/`transaction_index` 2 each,
`panic_handler` 1.

### 3.4 Dependencies added

Only for the riscv runtime target, in `substrate/primitives/io/Cargo.toml`:

```toml
[target.'cfg(all(any(target_arch = "riscv32", target_arch = "riscv64"), substrate_runtime))'.dependencies]
hash-db = { workspace = true }
hash256-std-hasher = { workspace = true }
polkavm-derive = { workspace = true }
libsecp256k1 = { workspace = true, default-features = false, features = ["static-context"] }
sp-trie = { workspace = true, default-features = false }
```

`hash-db` + `hash256-std-hasher` are needed because `sp_core::Blake2Hasher` / `KeccakHasher` are
`#[cfg(not(substrate_runtime))]`, so `native/trie.rs` defines its own equivalents over
`sp_crypto_hashing`.

`libsecp256k1` (pure Rust) is used instead of the `secp256k1` C crate because `secp256k1-sys`'s
build script cannot cross-compile C for the `riscv64emac-unknown-none-polkavm` target.

`prdoc/pr_12900.prdoc` documents the change.

## 4. Verification evidence

All green, zero `sp-io` warnings:

```
SKIP_WASM_BUILD=1 SKIP_PALLET_REVIVE_FIXTURES=1 cargo check -p sp-io
SKIP_WASM_BUILD=1 SKIP_PALLET_REVIVE_FIXTURES=1 cargo test  -p sp-io        # 6 unit + 2 doc
SKIP_WASM_BUILD=1 SKIP_PALLET_REVIVE_FIXTURES=1 cargo check -p sp-io --no-default-features
SKIP_PALLET_REVIVE_FIXTURES=1 cargo check -p minimal-template-runtime        # wasm
SUBSTRATE_RUNTIME_TARGET=riscv cargo check -p minimal-template-runtime -p westend-runtime -p polkadot-test-runtime
SKIP_WASM_BUILD=1 SKIP_PALLET_REVIVE_FIXTURES=1 cargo check -p sc-executor -p frame-support -p sp-state-machine -p sp-statement-store -p substrate-test-runtime
RUSTDOCFLAGS="-D warnings" cargo doc -p sp-io --no-deps                      # intra-doc links intact
```

The last riscv command is exactly CI's set (`.github/workflows/build-misc.yml`, `SUBSTRATE_RUNTIME_TARGET: riscv`).

**The decisive check is at the binary level.** The `westend-runtime` riscv blob's import table
contains only:

```
ext_default_child_storage_clear_version_1      ext_storage_clear_prefix_version_4
ext_default_child_storage_next_key_version_2   ext_storage_clear_version_1
ext_default_child_storage_read_version_2       ext_storage_commit_transaction_version_1
ext_default_child_storage_set_version_1        ext_storage_exists_version_1
ext_input_read_version_1                       ext_storage_next_key_version_2
ext_storage_append_version_1                   ext_storage_read_version_2
grow_heap                                      ext_storage_rollback_transaction_version_1
                                               ext_storage_root_version_3
                                               ext_storage_set_version_1
                                               ext_storage_start_transaction_version_1
```

No `ext_crypto_*`, `ext_hashing_*`, `ext_trie_*`, `ext_misc_*`, `ext_logging_*`, `ext_offchain_*`,
`ext_allocator_*`, `ext_wasm_tracing_*`, `ext_transaction_index_*`, and no `host_*` getters.

Reproduce with:

```bash
B=$(find target/debug/rbuild -name "westend-runtime-blob.polkavm" | head -1)
strings "$B" | grep -oE 'ext_[a-z_0-9]+' | sort -u
```

Beware: `strings` yields false positives from embedded metadata and runtime-API names
(e.g. `ext_epoch` inside `BabeApi_next_epoch`, `ext_authority_set_proof` inside
`BeefyMmrApi_next_authority_set_proof`). Confirm real imports against the contiguous import block.

## 5. Open items and blockers

### 5.1 riscv compile/link smoke test — POSTPONED (blocked)

**Goal.** A test that the `native/` implementations at least compile — ideally that all 100 are
*linked* into a real riscv blob, proving none of them drags in a host import.

**Intended host.** `substrate-test-runtime`.

**Blocker.** `substrate-test-runtime` cannot currently build for riscv. It is the only one of these
runtimes that *executes* its own blob at build time: `substrate/test-utils/runtime/build.rs:28`
calls `.enable_metadata_hash("TOKEN", 10)`, which runs the blob to call
`Metadata_metadata_at_version`. Under PolkaVM that panics:

```
SUBSTRATE_RUNTIME_TARGET=riscv cargo check -p substrate-test-runtime
# -> expected a WASM runtime blob, found a PolkaVM runtime blob;
#    set the 'SUBSTRATE_ENABLE_POLKAVM' environment variable ...

SUBSTRATE_ENABLE_POLKAVM=1 SUBSTRATE_RUNTIME_TARGET=riscv cargo check -p substrate-test-runtime
# -> panicked at substrate/client/executor/polkavm/src/lib.rs:184
#    `Metadata::metadata_at_version` should exist.: RuntimeConstruction(Instantiation("panic in call to get runtime version"))
```

**Root cause** — `substrate/client/executor/polkavm/src/lib.rs:184`:

```rust
fn take_input_data(&mut self) -> sp_wasm_interface::Result<Vec<u8>> {
    todo!("Implement 'take_input_data' for PolkaVM");
}
```

RFC-145 introduced the `Input` host function (`input::read` -> `take_input_data`), which the
PolkaVM executor has never implemented. Any riscv blob that reads its input therefore cannot be
executed by `sc-executor-polkavm`. Note `grow_heap` *is* implemented (same file, ~line 312), so the
allocator side is fine.

**Prerequisite to unblock.** Implement `take_input_data` in `sc-executor-polkavm`. That is a small,
self-contained change but it is out of scope for the sp-io split and is a genuine gap in the
experimental PolkaVM executor under RFC-145.

`minimal-template-runtime`, `westend-runtime` and `polkadot-test-runtime` build for riscv precisely
because they never execute their blob during the build.

**Note on existing coverage.** Compiling any runtime for riscv already type-checks *all* of
`sp-io`'s native fns even when uncalled — a broken `native/offchain.rs` previously failed the
`minimal-template-runtime` riscv build although that runtime never calls offchain. So CI's riscv
job already guards "it compiles"; what is missing is link-time coverage and an explicit guard.

### 5.2 JAM logging cannot be enabled yet

The intent was to route `logging::log` and `misc::print_*` to JAM's `log` host call
(`jam-pvm-common/src/imports.rs`, `#[polkavm_import(index = 100)]`, explicitly "NOT part of the
GP"). Log levels map 1:1 with `sp_core::RuntimeInterfaceLogLevel` (0=Error .. 4=Trace), so no
translation is needed.

**Blocker.** The polkavm linker requires imports to be *uniformly* indexed or unindexed. `sp-io`'s
macro-generated imports carry no index, so adding one explicitly-indexed import fails the link:

```
Linking error: import without a specified index: 'ext_storage_clear_prefix_version_4'
```

Verified by bisection: baseline (all unindexed) links; adding `index = 100` breaks it; removing the
index fixes it.

**Decision.** Keep `logging`/`misc::print_*` as native **no-ops** on riscv for now, and enable JAM
logging later as part of a coordinated scheme that assigns explicit indices to *all* riscv host
imports (storage, input, grow_heap included). Since JAM dispatches host calls by index, that scheme
is needed for the storage imports anyway — see 5.4.

Consequence today: a JAM parachain produces no log output and runtime panic messages are not
surfaced.

### 5.3 No behavioural tests for the native implementations

The natives are verified structurally (they compile for riscv; the blob imports only storage) but
**not** behaviourally. There are no tests asserting the native results equal the host results. The
6 unit + 2 doc tests in `sp-io` are pre-existing and only exercise the std/host path; `mod native`
is riscv-gated so `cargo test` never compiles it.

Highest-risk gap: **implementation divergence**. `secp256k1_ecdsa_recover(_compressed)` uses
`libsecp256k1` (`Signature::parse_overflowing_slice`) on riscv, whereas the wasm/host path's latest
version uses the `secp256k1` C crate (`RecoverableSignature::from_compact`). `parse_overflowing_slice`
is deliberately lenient where `from_compact` is not. If they disagree on any edge case, a JAM
parachain and a wasm parachain would compute different results from identical input, i.e. consensus
divergence. Also unasserted: the `pubkey.serialize()[1..65]` / `serialize_compressed()` slicing, and
that the local trie hashers plus the `StateVersion -> LayoutV0/V1` mapping reproduce byte-identical
roots.

Proposed fix: host-side equivalence tests that compile `native` additionally under `cfg(test)` and
assert native output == host output (hashing incl. empty/1 MiB inputs; trie roots and proof
round-trips for both `StateVersion`s; signature verify valid/tampered/wrong-key; ecdsa recover over
known vectors *and* the overflowing R/S edge case).

### 5.4 parachain-service side (other repo)

- **Host-call indices.** JAM dispatches by index and the parachain service mediates the nested PVF's
  host calls (`service/src/pvf/pvm.rs`, `ExecutorState::dispatch`; unhandled indices become
  `InvokeOutcome::HostCallFault`). The `ext_storage_*` / `ext_input_read_*` / `grow_heap` imports
  emitted by `sp-io` are unindexed and are not currently forwarded. A mapping has to be agreed and
  implemented; it is the same work item that unblocks 5.2.
- **`log` forwarding.** `pvm.rs` has no arm for index 100.
- For reference the PoC `frameless` runtime bypasses `sp-io` entirely, declaring its own
  `#[polkavm_import(index = N)]` block for indices 0-29 matching the `HostCall` enum in
  `service-interface/src/host_call.rs`.

## 6. Reproducing the builds

```bash
# host / std
SKIP_WASM_BUILD=1 SKIP_PALLET_REVIVE_FIXTURES=1 cargo check -p sp-io
SKIP_WASM_BUILD=1 SKIP_PALLET_REVIVE_FIXTURES=1 cargo test  -p sp-io

# wasm runtime
SKIP_PALLET_REVIVE_FIXTURES=1 cargo check -p minimal-template-runtime

# riscv / PolkaVM (CI's set)
SUBSTRATE_RUNTIME_TARGET=riscv cargo check -p minimal-template-runtime -p westend-runtime -p polkadot-test-runtime

# formatting (always scope with -p)
cargo +nightly fmt -p sp-io
```

Notes: the riscv target json is fetched/generated by wasm-builder under
`~/.cache/.polkavm-linker/<ver>/`; builds are cached per runtime under `target/debug/rbuild/<crate>/`
so `touch substrate/primitives/io/src/lib.rs` to force a genuine relink. Build-script stdout is
hidden by cargo unless the script fails, so a successful riscv `cargo check` prints no link output.
