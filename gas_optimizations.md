# JAM refine gas: transaction cost optimizations

Measured with `cumulus/jam/tx-gas-benchmark`: `parachain-template-runtime` built for PolkaVM with
`--cfg jam`, blocks of 50 signed `Balances::transfer_keep_alive`, refined on polkavm 0.36 with the
`Full(L2Hit)` cost model. The reported gas is the inner PVM's (the `invoke` part of a refine); every
run declares the built head and parent head, so signatures and the authorizer seal were checked.

## Gas model

polkavm charges each basic block what an out-of-order core simulation needs to run it on its own
(`polkavm-common` `simulator.rs`): loads and stores cost 25 cycles of latency, there are 4 load and
4 store slots, 1 multiply slot and a 32 entry reorder buffer. Every iteration of a loop pays the full
latency of its loads again, while independent loads inside one block overlap. Short loops over
memory are therefore expensive (the `compiler_builtins` `memcmp` costs 47 gas per byte), and so is
every heap allocation (`picoalloc` about 780 gas per `alloc`, 415 per `free`).

## Done (uncommitted)

| Step | Change | Per tx | Empty block |
|---|---|---|---|
| Baseline | | 5,701,974 | 375,800,672 |
| 1 | `sp-io` `src/mem_riscv.rs`: word-wise `memcmp`/`bcmp` (the `compiler_builtins` symbols are weak); `sp-io` jam storage: exchangeable `host_get`, overridden in `validate_block` with `ext.storage(key)` (no zeroed 256 byte scratch buffer, no second copy) | 5,016,558 | 375,915,772 |
| 2 | `memcpy` and `memcmp` move four words per basic block | 4,597,091 | 370,489,314 |
| 3 | Conditional `u32`/`u16`/`u8` tails; `Blake2_128Concat`/`Twox64Concat::hash` build the key with `concat` instead of collecting it byte by byte | 4,429,486 | 370,274,636 |
| 4 | `wasm-builder`: `-C target-feature=+unaligned-scalar-mem` for PolkaVM runtimes (PolkaVM loads and stores any address; without it every value not provably aligned is assembled from byte loads) | 4,271,544 | 343,908,717 |
| 5 | FRAME map, double map and n-map final keys use the precomputed `prefix_hash()` instead of two `twox_128` per access | **4,006,620** | **343,791,519** |

Total: -1,695,354 gas per transaction (-29.7%), -32.0M per block.

Building `polkajam2` and `parachain-service` into the SDK needed `[patch.crates-io]` in both
workspaces (`cumulus-aura-authorizer-utils`, `sp-io`, `jam-types`, `jam-std-common`) and a copy of
`cumulus/jam/aura-authorizer-utils`. Because that makes `parachain-service-core`'s `sp-io`
features apply to the SDK `sp-io`, `cumulus-jam-state-reader` now pulls `parachain-service-core`
only for `cfg(any(jam, not(substrate_runtime)))`; otherwise relay PolkaVM runtimes lose their
allocator and panic handler.

## Where the 4,006,620 gas per transaction go

| Part | Gas | Share |
|---|---|---|
| curve25519 (sr25519 verification) | 2,110,690 | 52.7% |
| Everything else (FRAME, storage, decoding) | 596,815 | 14.9% |
| Allocator (`picoalloc`) | 367,503 | 9.2% |
| `memcpy` | 363,544 | 9.1% |
| `memcmp` | 214,587 | 5.4% |
| blake2b | 196,137 | 4.9% |
| merlin / keccak / schnorrkel | 85,054 | 2.1% |
| `memset` / `memmove` | 61,321 | 1.5% |
| twox | 10,969 | 0.3% |

Signature verification is out of scope here.

## Possible optimizations

"Measured" numbers come from profiles of this benchmark, "estimate" numbers do not.

### Per transaction

1. **Build `RuntimeBlockWeights` once.** `parameter_types!` rebuilds and validates the whole
   `BlockWeights` on every `get()` (`templates/parachain/runtime/src/configs/mod.rs:95`), and the
   transaction payment and weight checks call it several times per transaction. About 57k per
   transaction (measured on an earlier profile). Low risk.

2. **Fewer heap allocations** (allocator 367k measured, about 307 allocations per transaction).
   1. Build storage keys on the stack. `Blake2_128Concat::hash` returns a `Vec`
      (`substrate/frame/support/src/hash.rs:125`) and the final key is a second `Vec`
      (`storage/generator/map.rs`). Prefix (32) + hash (16) + `AccountId32` (32) fits a stack
      buffer. Estimate 60-100k.
   2. Cheaper allocator in `polkajam2`. `jam-pvm-common` wraps `picoalloc` in a `Mutex`
      (`crates/jam-pvm-common/src/mem.rs:47`), which compiles to emulated atomics in a
      single-threaded guest, and `alloc_impl` is a chain of dependent loads (about 780 gas). An
      allocator with size classes and free lists suits the gas model better. Estimate 100-200k per
      transaction, plus some per block.

3. **Unaligned word access in `memcpy`/`memcmp`.** The word paths need both pointers equally aligned
   (`substrate/primitives/io/src/mem_riscv.rs:49`, `:126`); about 20% of `memcpy` calls and many key
   comparisons still take the byte loop. With `+unaligned-scalar-mem` they can load words from any
   address. Estimate 50-150k.

4. **Fewer copies in the storage path.**
   - `Ext::storage` copies an overlay hit with `to_vec`
     (`substrate/primitives/state-machine/src/ext.rs:145`).
   - `OverlayedMap::set` clones keys it already owns (`overlayed_changes/changeset.rs:865`).
   - `host_storage_set` copies key and value
     (`cumulus/pallets/parachain-system/src/validate_block/host_functions.rs:235`).

   Estimate 50-100k.

5. **blake2b** (196k measured, 36 compressions per transaction, about 5.4k gas each). Hash less, by
   reusing the hashed `System::Account` keys that nonce check, fee payment and transfer each compute
   again; or hash cheaper, with a compression function whose rounds are unrolled so more independent
   work sits in each block. Estimate 50-100k.

6. **Fewer storage reads per transaction.** One transfer makes 74 `Ext::storage` calls (109k
   measured, plus about 57k in `environmental::with`), many of them repeats (`BlockWeight` read and
   written by each extension, account reads). FRAME-level change. Estimate 100-200k.

7. **Drop proof-size recording and `StorageWeightReclaim` on JAM.** The recorder
   (`validate_block_core.rs:193-197`) and the `storage_proof_size` host call cost 14,468 gas per
   transaction (measured), the reclaim logic itself an estimated 5-30k more; about 0.8M per block.
   It needs a different `TxExtension` for JAM runtimes, because the reclaimed weight ends up in the
   state root. Small win for a semantic change.

8. **Production build profile** (`WASM_BUILD_TYPE=production`: fat LTO, one codegen unit).
   -89k per transaction (measured), no code change.

### Per block

9. **Check the authorizer seal without the embedded interpreter.** An empty block spends most of its
   gas running the authorizer blob in an embedded polkavm interpreter (331.9M of 385.6M in an
   earlier profile) through `parachain-service`'s `core/src/authorize_block.rs:62`, called from
   `cumulus/pallets/aura-ext/src/jam.rs:67`. Running the same check natively in the runtime would
   leave the signature check itself as it is and remove most of about 300M per block.

10. **Verify the JAM state proof once.** `JamProofReader::read` re-verifies the whole proof on every
    read (`cumulus/jam/state-reader/src/jam_proof.rs:62`, `parachain-service` `core/src/proof.rs:77`),
    hashing every node into a fresh map. Verifying once in `new()` saves one full proof verification
    for every JAM state read after the first in a block.

### Tried and rejected

| Experiment | Per tx |
|---|---|
| curve25519 `fiat` backend | 4,384,276 |
| curve25519 32-bit backend | 8,459,515 |
| `memcpy` eight words, `memcmp` two words per block | 4,283,635 |

## Transactions per block

Capacity = (refine gas limit - fixed cost per block) / gas per transaction. The fixed cost is the
empty block plus about 85M the service spends outside the runtime (from an earlier zombienet
measurement). The last two rows rest on the estimates above.

| Case | Per tx | Fixed | 1e9 limit (testnet) | 5e9 limit (Gray Paper) |
|---|---|---|---|---|
| Baseline | 5.70M | ~461M | ~94 | ~796 |
| Now | 4.01M | ~429M | ~142 | ~1,140 |
| + items 1-8 | 3.0-3.45M | ~429M | ~165-190 | ~1,325-1,525 |
| + item 9 | 3.0-3.45M | ~130M | ~250-290 | ~1,410-1,625 |

At the 1e9 limit the fixed cost per block weighs most, so item 9 is worth more than all per
transaction items together. At 5e9 the gas per transaction decides, and signature verification is
more than half of it.

The next limit is authoring time, not gas: in an earlier zombienet flood the collator hit its 2 s
proposal deadline at about 137-146 transfers under the PolkaVM interpreter. Beyond about 140
transactions per block the collator needs a faster PolkaVM backend or a longer proposal window.

## Reproducing

```sh
SUBSTRATE_RUNTIME_TARGET=riscv WASM_BUILD_RUSTFLAGS=" --cfg jam" \
	cargo build --release -p parachain-template-runtime
# in ../parachain-service
./target/release/cargo-jam-build jam-build -p parachain-authorizer-sr25519
SKIP_WASM_BUILD=1 cargo run --release -p cumulus-jam-tx-gas-benchmark -- \
	--runtime target/release/rbuild/parachain-template-runtime/parachain-template-runtime-blob.polkavm \
	--authorizer ../parachain-service/target/pvm/riscv64emac-unknown-none-polkavm/production-authorizer/parachain-authorizer-sr25519.jam \
	--transactions 50
```

For a profile, add `--profile <dir>` and set `JAM_TX_GAS_PROFILE_DIR=<dir>`. `wasm-builder` strips
the blob, so to name functions, link the runtime ELF
(`target/release/rbuild/parachain-template-runtime/target/riscv64emac-unknown-none-polkavm/release/parachain-template-runtime-blob.polkavm.elf`)
again with `polkatool` 0.35 (`cargo install polkatool --version 0.35.0`) without `--strip`: the code
is identical, so profile program counters map one to one.
