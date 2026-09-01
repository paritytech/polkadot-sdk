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

### 1.1 sc-executor-polkavm V2 entry point support

`substrate/client/executor/polkavm/src/lib.rs` has been updated to support RFC-145 V2 entry points.
The executor was previously stuck on the V1 calling convention (host writes payload into guest memory
and passes pointer + length), so no post-RFC-145 riscv runtime blob could execute. Changes include:
adding a `HostState { input_data: Option<Vec<u8>> }` type to hold the call payload, threading it
through `InstancePre`, `Instance`, `Context`, and the linker, implementing `FunctionContext::take_input_data`
(was `todo!()`), and rewriting the call path to invoke the entry point with a single argument
(the input length), allowing the guest to pull the payload via `sp_io::input::read`. This also
eliminated a memory leak in the input payload handling.

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

### 5.1 riscv compile/link smoke test — RESOLVED

**Goal.** A test that the `native/` implementations at least compile — ideally that all 100 are
*linked* into a real riscv blob, proving none of them drags in a host import.

**Intended host.** `substrate-test-runtime`.

**Status.** The blocker has been resolved. `substrate-test-runtime` can now build for riscv with
`SUBSTRATE_ENABLE_POLKAVM=1`. The PolkaVM executor previously did not implement `take_input_data`,
which is required by RFC-145 V2 entry points; this has been fixed. Riscv blobs now execute end-to-end,
as verified by the metadata hashing step in the build process.

**Note on existing coverage.** Compiling any runtime for riscv already type-checks *all* of
`sp-io`'s native fns even when uncalled — a broken `native/offchain.rs` previously failed the
`minimal-template-runtime` riscv build although that runtime never calls offchain. So CI's riscv
job already guards "it compiles"; the smoke test is now unblocked and provides link-time coverage
and an explicit guard.

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

This is not specific to logging: it applies to *any* indexed import, and therefore to the JAM
`validate_block` entry itself — see 5.5.

**Linker blocker resolved** by the index scheme in 5.4 — but logging is still not enabled, for two
reasons that replace it:

1. **Index 100 is not `log` for a PVF.** The spec gives the parachain service's native calls
   100 upwards, so in the child's table 100 is `set_parent_head_hash`. `log = 100` is a
   `jam-pvm-common` convention for a *service*, and the child does not inherit a service's table.
   Routing `logging::log` to 100 would call `set_parent_head_hash` with garbage. A PVF-visible
   `log` needs its own index in the service's table (104 is free) plus a `dispatch` arm.
2. **An undefined import only traps when called** — and logging *is* called, unlike the JAM entry's
   imports under Substrate's executor (5.5). So it cannot be added speculatively on one side.

**Decision.** Keep `logging`/`misc::print_*` as native **no-ops** on riscv.

Consequence today: a JAM parachain produces no log output and runtime panic messages are not
surfaced. `report_error` (103) remains the only channel by which a PVF records a failure reason.

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

The service mediates every nested-PVF host call: `refine::invoke` surfaces it as
`InvokeOutcome::HostCallFault(index)` and `ExecutorState::dispatch` resolves it
(`service/src/pvf/pvm.rs`, `service/src/pvf/executor.rs`). The child reaches JAM only through
calls the service chooses to forward — nothing is automatic.

**The spec now fixes the PVF's own table (§4.3)**: JAM host calls are forwarded at their Gray
Paper index (`gas` 0, `grow_heap` 1, `fetch` 2, `historical_lookup` 7, `export` 8), and the
service's native calls start at 100 (`set_parent_head_hash` 100, `set_head` 101,
`send_upward_message` 102, `report_error` 103). `jam_implementation.rs` matches this.

**The Substrate half is done** — the numbers are declared, so the spec can adopt them rather than
invent them. `#[runtime_interface]` accepts `#[polkavm_index(N)]` per function, and the table lives
in `substrate/primitives/io/src/host_functions/mod.rs`:

| Range | Owner |
|---|---|
| 0, 1, 2, 7, 8 | JAM host calls forwarded at their Gray Paper index (spec §4.3) |
| 100+ | parachain-service native calls; 100-103 taken (spec §4.3) |
| 200-216 | `sp_io::storage` |
| 220-227 | `sp_io::default_child_storage` |
| 240 | `sp_io::input` |
| 241 | `cumulus_primitives_proof_size_hostfunction` |
| 242-243 | `sp_additional_data` |

24 imports are indexed — the exact set a parachain runtime blob emits, enumerated from the built
ELF, not guessed. The 200+ block deliberately clears the service's 100+ growth. What the service
still has to do is *dispatch* them: `ExecutorState::dispatch` needs arms for 200-243, forwarding to
the nested PVF's storage. Indices are transparent to Substrate's own executor, which resolves
imports by name — verified by the PVM tests in 5.7 still passing after the change.

Only the function versions a runtime actually calls carry an index. An older, unindexed version
would fail the link with `import without a specified index`, which is the signal to add it.

Note the index ceiling: the linker pads index holes with dummy imports, so the blob's import table
is `max_index + 1` entries and `VM_MAXIMUM_IMPORT_COUNT` is 1024. Our highest index (243) costs a
244-entry table; nothing may exceed 1023.

**Nested-PVF stack size — no spec change needed.** `parse_pvf` builds the child's memory map with
`.stack_size(parts.stack_size)`, i.e. straight from the blob's own `.polkavm_min_stack_size`
section, so the 2 MiB declared in 5.6 is already honoured.

**Still unspecified.** `ValidationResult`'s `upward_messages` (XCM), `horizontal_messages`,
`processed_downward_messages` and `hrmp_watermark` have no ABI yet: the spec's `UpwardMessage` enum
carries service-level control messages only, and full XCMP over `export` is a §8 proposal. Those
four fields are therefore still dropped on the JAM path — see 5.8.

### 5.5 One blob carries both entry points — RESOLVED, no build flag

`jam_implementation`'s child-PVM imports are explicitly indexed (2, 9, 12, 13, 14, 24 — spec §4.3),
so while `sp-io`'s imports were unindexed the 5.2 constraint kept them out of the same blob:

```
Linking error: import without a specified index: 'ext_storage_proof_size_storage_proof_size_version_1'
```

**Resolved by the Substrate-side index scheme (see 5.4).** With every import indexed, the ordinary
riscv build — no extra flags — links and exports *both* entry points:

```
$ strings target/debug/rbuild/cumulus-test-runtime/cumulus-test-runtime-blob.polkavm | grep validate_block
jam_validate_block
validate_block
```

An intermediate `--cfg jam_service` gate was removed once measured: PolkaVM only traps on an
undefined import when it is *called*, so the JAM host calls sit inert in a blob run by Substrate's
executor (which never calls `jam_validate_block`). Verified by the 5.7 tests passing against a blob
that contains them. A parachain therefore ships **one** blob usable by both hosts.

### 5.6 `validate_block` needs a 2 MiB guest stack

PolkaVM defaults to an 8 KiB stack; `validate_block` recurses through trie proof verification and
block execution and overflows it. The failure surfaces as a bare `trap` with no message, because
riscv logging is a no-op (5.2) — `RUST_LOG=polkavm=debug` is what identifies it:

```
Store of 8 bytes to 0xfffddfe8 failed: trap! (pc = 878335)
Current stack range: 0xfffde000-0xfffe0000
Hint: try increasing your stack size with: 'polkavm_derive::min_stack_size'
```

`cumulus-test-runtime` declares `polkavm_derive::min_stack_size!(2 * 1024 * 1024)`. Note that
`rococo-runtime` pushes the `.polkavm_min_stack_size` section by hand with a TODO to use the macro;
that TODO is stale — the macro works directly, it only needs the `polkavm-derive` dependency and a
`#[cfg(all(any(target_arch = "riscv32", target_arch = "riscv64"), target_feature = "e"))]` gate on
the invocation, since the macro *definition* is itself riscv-gated.

Any parachain runtime on JAM will hit the same limit, hence the note in 5.4.

### 5.7 PVM coverage of the validation path

`enable_pvm()` on `cumulus-test-runtime`'s default build emits `PVM_BINARY` next to `WASM_BINARY`,
and three tests in `cumulus/pallets/parachain-system/src/validate_block/tests.rs` run the real
validation path through `sc-executor-polkavm`: `validate_block_works_on_pvm`,
`validate_block_with_extra_extrinsics_on_pvm`, `validate_block_returns_custom_head_data_on_pvm`.

They re-exec themselves in a subprocess with `SUBSTRATE_ENABLE_POLKAVM=1` (the executor reads it
from the environment, which cannot be set in-process while other tests run in parallel).

This covers the polkadot `validate_block` entry on PolkaVM. The JAM entry (`jam_validate_block`)
still has no execution coverage — it needs 5.4.

### 5.8 Four `ValidationResult` fields have no JAM ABI yet

`jam_validate_block` sinks `head_data` (`set_head`) and `new_validation_code`
(`send_upward_message` carrying `UpwardMessage::RequestCodeUpgrade`). The remaining four fields of
`ValidationResult` are **silently dropped**:

| Field | Status |
|---|---|
| `upward_messages` | The spec's `UpwardMessage` enum is service-level control messages only (`RequestCodeUpgrade`, `SetKV`, `TransferOut`, …); no variant carries an opaque relay-bound XCM blob. |
| `horizontal_messages` | Intended to travel as Data Lake segments via `export` (8), but the encoding is part of the §8.2 *full XCMP* proposal, not yet specified. |
| `processed_downward_messages` | No digest field. |
| `hrmp_watermark` | No digest field. |

A parachain block that sends XCM therefore validates successfully on JAM while its messages
vanish. This is the largest remaining functional gap between the JAM and polkadot paths, and it is
blocked on the spec, not on this repo.

## 6. Reproducing the builds

```bash
# host / std
SKIP_WASM_BUILD=1 SKIP_PALLET_REVIVE_FIXTURES=1 cargo check -p sp-io
SKIP_WASM_BUILD=1 SKIP_PALLET_REVIVE_FIXTURES=1 cargo test  -p sp-io

# wasm runtime
SKIP_PALLET_REVIVE_FIXTURES=1 cargo check -p minimal-template-runtime

# riscv / PolkaVM (CI's set)
SUBSTRATE_RUNTIME_TARGET=riscv cargo check -p minimal-template-runtime -p westend-runtime -p polkadot-test-runtime

# riscv smoke test (executes the blob at build time for metadata hashing)
SUBSTRATE_ENABLE_POLKAVM=1 SUBSTRATE_RUNTIME_TARGET=riscv cargo check -p substrate-test-runtime

# validate_block on PolkaVM (builds both blobs via enable_pvm, then executes the riscv one)
SKIP_PALLET_REVIVE_FIXTURES=1 cargo test -p cumulus-pallet-parachain-system --lib _on_pvm

# compile-check the macro-generated JAM entry (a pallet-only check does NOT cover it, since
# `register_validate_block!` is invoked by the runtime)
RUSTC_BOOTSTRAP=1 SKIP_WASM_BUILD=1 RUSTFLAGS="--cfg substrate_runtime" \
  cargo check -p cumulus-test-runtime --no-default-features \
  --target ~/.cache/.polkavm-linker/0.35.0/1_91/riscv64emac-unknown-none-polkavm.json \
  -Z build-std=core,alloc -Z json-target-spec

# formatting (always scope with -p)
cargo +nightly fmt -p sp-io
```

Notes: the riscv target json is fetched/generated by wasm-builder under
`~/.cache/.polkavm-linker/<ver>/`; builds are cached per runtime under `target/debug/rbuild/<crate>/`
so `touch substrate/primitives/io/src/lib.rs` to force a genuine relink. Build-script stdout is
hidden by cargo unless the script fails, so a successful riscv `cargo check` prints no link output.
The `SUBSTRATE_ENABLE_POLKAVM=1` environment variable is mandatory for the smoke test; without it,
the executor refuses the PolkaVM blob.
