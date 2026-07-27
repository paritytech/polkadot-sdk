# Crypto benchmarking for the Parachain Service: hashing & first signature numbers

Comparison of in-PVM vs native execution of the hash functions the Parachain
Service / PVF stack needs, to decide which (if any) require Gray Paper host
calls and which can stay PVM guest code (see design doc §4.3).

## Results

Setup: PolkaVM **64-bit, recompiler backend, synchronous gas metering** (polkajam's
production configuration). `native-simd` = same code compiled natively with
`-C target-cpu=native` (AVX2, SHA-NI, …); `native` = portable native build.
Times are per hash, including hasher construction; transpilation and
instantiation are excluded (steady-state). All artifacts are built from the
same source and verified to produce identical outputs. Guest artifacts are
built with two build-configuration fixes (see *Build tweaks* below).

Ratios are vs the **best native build per algorithm** — the honest "host
function" baseline: the `target-cpu=native` build for blake2/keccak/sha2, the
portable build for twox (where `target-cpu=native` is a ~1.5× pessimization —
baselines were checked per algorithm, not assumed).

### Hashing
| algo | best native 32 B | PVM 32 B | ratio | best native 1 MiB | PVM 1 MiB | ratio |
|---|---:|---:|---:|---:|---:|---:|
| blake2_256 | 110 ns | 295 ns | 2.7× | 753 µs | 1.49 ms | **2.0×** |
| blake2_128 | 110 ns | 298 ns | 2.7× | 754 µs | 1.47 ms | **1.9×** |
| keccak_256 | 268 ns | 482 ns | 1.8× | 1.80 ms | 2.48 ms | **1.4×** |
| keccak_512 | 260 ns | 465 ns | 1.8× | 3.43 ms | 4.68 ms | **1.4×** |
| sha2_256 | 50 ns | 427 ns | 8.5× | 471 µs | 4.40 ms | **9.3×** |
| twox_64 | 13 ns | 43 ns | 3.3× | 60 µs | 88 µs | **1.5×** |
| twox_128 | 26 ns | 65 ns | 2.5× | 119 µs | 177 µs | **1.5×** |
| twox_256 | 47 ns | 129 ns | 2.7× | 239 µs | 355 µs | **1.5×** |


### Signature verification: ed25519 (preliminary)


| configuration | per verification | vs native |
|---|---:|---:|
| native | 30.7 µs | 1.0× |
| PVM 64-bit, compiler, sync gas | 102.3 µs | **3.3×** |

Measured with the memcpy fix only (not yet re-run with the unaligned-load
fix; see *Build tweaks*). Pending: sr25519 and ecdsa/secp256k1-recover
equivalents, batch variants, and crate parity with `sp_io`
(`ed25519-zebra`) — same methodology as bench-hash.

## Host-call overhead (not yet measured)

A PVF runs in a **nested PVM** (`machine`/`invoke`); its host calls are not
handled natively but bounce through the service's refine wrapper (guest
code) — at minimum **three host-boundary round trips** plus copying the
arguments out of inner memory:

```
1. PVF: ecalli N          inner VM exits, control returns to the outer PVM
2. service wrapper dispatches on N
3.   peek(handle, …)      copy arguments out of inner memory
4.   invoke(handle, …)    resume inner VM
```

A host call pays off when `PVM_time − native_time > n × crossing + copy(len)`.
Crossing and copy costs are not measured yet (benchtool `bench-ecalli`,
polkajam's host-call benchmarks) — no winners can be declared. Calls made by
the service's own refine code pay a single crossing.

## What is benchmarked

The PVF-relevant subset of
[`sp_io`](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/primitives/io/src/lib.rs)
host functions (`sp_io::trie` reduces to blake2_256/keccak_256 + plain Rust,
so it is not benchmarked separately):

- `Hashing` (this report): `blake2_128`, `blake2_256`, `keccak_256`,
  `keccak_512`, `sha2_256`, `twox_64`, `twox_128`, `twox_256`
- `Crypto`: `ed25519_verify` (preliminary, via `ed25519_dalek` — see above);
  pending: crate-matched ed25519, `sr25519_verify`, `ecdsa_verify`,
  `ecdsa_verify_prehashed`, `secp256k1_ecdsa_recover(_compressed)` and the
  batch variants

Implementations match `sp_crypto_hashing`: `blake2b_simd`, `sha2`, `sha3`,
`twox-hash` (1/2/4 seeded XxHash64 passes).

## Build tweaks

- **Linker-provided `memcpy`/`memset`** (`builtins-mem` feature, default for
  bench-hash): the suite's shared `bench-common.rs` defines naive
  byte-at-a-time `memcpy`/`memset` that shadow the weak, word-wise
  `compiler_builtins` versions — the ones real guests (e.g. polkajam
  services) link. Impact of the fix: up to −58% on small-input hashing
  (keccak_256 @ 32 B), −21% on ed25519 under sync gas.
- **Unaligned scalar loads** (`+unaligned-scalar-mem` in the target JSON):
  PVM allows unaligned loads, but the stock target does not tell LLVM, which
  therefore assembles every unaligned `u64` read out of 8 single-byte loads.
  With the flag they become single loads. Impact: blake2 −24%, twox −64%,
  keccak/sha2 −9% (bulk sizes).

## Tooling & reproduction

Extension of the [polkavm](https://github.com/paritytech/polkavm) benchmark
suite, on the
[`mku-bench-hash`](https://github.com/paritytech/polkavm/tree/mku-bench-hash)
branch (not yet upstreamed): a
[`guest-programs/bench-hash`](https://github.com/paritytech/polkavm/tree/mku-bench-hash/guest-programs/bench-hash)
crate exporting
`benchmark_<algo>(len, times) -> u64` for each hash function, and a benchtool
`bench-hash` subcommand that measures those exports over a size grid on every
discovered artifact (PVM blob, portable native `.so`, `*_simd.so` variants)
and prints raw per-iteration times; comparison/ratios are post-processing.

Methodology: each measurement hashes a pre-filled buffer `times` times with
the output chained back into the input (prevents hoisting/DCE, measures
latency — matching trie-style chained hashing); buffer setup and a warmup call
happen outside the timer; each export returns the first 8 bytes of the final
digest, cross-checked across artifacts so all builds provably do the same
work.

```
cd guest-programs && ./build-benchmarks.sh && ./build-hash-native.sh
cd ../tools/benchtool
cargo run --release -- bench-hash --csv blake2_128 blake2_256 keccak_256 \
    keccak_512 sha2_256 twox_64 twox_128 twox_256 > results.csv
./process-results.sh results.csv > result-table.md
```

The ed25519 numbers come from the suite's pre-existing `bench-ed25519` guest
via the generic harness (`runtime` variant = steady-state execution), built
with the same mem-function configuration as bench-hash
(`--features builtins-mem`):

```
cargo run --release -- benchmark ed25519
```

## Appendix: full tables

Ratios in these tables are vs `native-simd` (i.e. the `target-cpu=native`
build — note the twox rows, where the portable `native` column is the faster
baseline; the summary table above accounts for this). Machine: x86-64 with
AVX2 + SHA-NI; ASLR disabled by benchtool.

### blake2_128

| size | native-simd | PVM | ratio | native | ratio |
|---:|---:|---:|---:|---:|---:|
| 32 B | 110 ns | 298 ns | 2.71× | 112 ns | 1.02× |
| 128 B | 104 ns | 285 ns | 2.73× | 112 ns | 1.07× |
| 512 B | 383 ns | 839 ns | 2.19× | 413 ns | 1.08× |
| 4 KiB | 2.96 µs | 5.95 µs | 2.01× | 3.21 µs | 1.08× |
| 64 KiB | 47.05 µs | 92.87 µs | 1.97× | 51.13 µs | 1.09× |
| 1 MiB | 753.89 µs | 1.47 ms | 1.94× | 818.69 µs | 1.09× |

### blake2_256

| size | native-simd | PVM | ratio | native | ratio |
|---:|---:|---:|---:|---:|---:|
| 32 B | 110 ns | 295 ns | 2.69× | 110 ns | 1.00× |
| 128 B | 105 ns | 278 ns | 2.66× | 110 ns | 1.05× |
| 512 B | 380 ns | 845 ns | 2.22× | 405 ns | 1.06× |
| 4 KiB | 2.95 µs | 5.89 µs | 2.00× | 3.14 µs | 1.06× |
| 64 KiB | 47.06 µs | 91.84 µs | 1.95× | 50.45 µs | 1.07× |
| 1 MiB | 752.72 µs | 1.49 ms | 1.98× | 804.56 µs | 1.07× |

### keccak_256

| size | native-simd | PVM | ratio | native | ratio |
|---:|---:|---:|---:|---:|---:|
| 32 B | 268 ns | 482 ns | 1.80× | 302 ns | 1.13× |
| 128 B | 259 ns | 474 ns | 1.83× | 290 ns | 1.12× |
| 512 B | 957 ns | 1.44 µs | 1.50× | 1.10 µs | 1.15× |
| 4 KiB | 7.26 µs | 10.12 µs | 1.39× | 8.71 µs | 1.20× |
| 64 KiB | 112.62 µs | 155.38 µs | 1.38× | 134.84 µs | 1.20× |
| 1 MiB | 1.80 ms | 2.48 ms | 1.38× | 2.17 ms | 1.21× |

### keccak_512

| size | native-simd | PVM | ratio | native | ratio |
|---:|---:|---:|---:|---:|---:|
| 32 B | 260 ns | 465 ns | 1.79× | 303 ns | 1.17× |
| 128 B | 490 ns | 782 ns | 1.60× | 583 ns | 1.19× |
| 512 B | 1.89 µs | 2.72 µs | 1.44× | 2.32 µs | 1.23× |
| 4 KiB | 13.40 µs | 18.22 µs | 1.36× | 16.02 µs | 1.20× |
| 64 KiB | 214.35 µs | 288.62 µs | 1.35× | 254.87 µs | 1.19× |
| 1 MiB | 3.43 ms | 4.68 ms | 1.37× | 4.09 ms | 1.19× |

### sha2_256

| size | native-simd | PVM | ratio | native | ratio |
|---:|---:|---:|---:|---:|---:|
| 32 B | 50 ns | 427 ns | 8.49× | 76 ns | 1.50× |
| 128 B | 109 ns | 1.02 µs | 9.32× | 182 ns | 1.67× |
| 512 B | 281 ns | 2.61 µs | 9.27× | 539 ns | 1.92× |
| 4 KiB | 1.89 µs | 17.71 µs | 9.38× | 3.57 µs | 1.89× |
| 64 KiB | 29.44 µs | 274.35 µs | 9.32× | 56.20 µs | 1.91× |
| 1 MiB | 471.38 µs | 4.40 ms | 9.34× | 898.10 µs | 1.91× |

### twox_64

| size | native-simd | PVM | ratio | native | ratio |
|---:|---:|---:|---:|---:|---:|
| 32 B | 20 ns | 43 ns | 2.14× | 13 ns | 0.63× |
| 128 B | 29 ns | 44 ns | 1.49× | 17 ns | 0.57× |
| 512 B | 62 ns | 72 ns | 1.17× | 38 ns | 0.61× |
| 4 KiB | 373 ns | 378 ns | 1.01× | 246 ns | 0.66× |
| 64 KiB | 5.69 µs | 5.55 µs | 0.97× | 3.71 µs | 0.65× |
| 1 MiB | 91.27 µs | 88.45 µs | 0.97× | 59.84 µs | 0.66× |

### twox_128

| size | native-simd | PVM | ratio | native | ratio |
|---:|---:|---:|---:|---:|---:|
| 32 B | 34 ns | 65 ns | 1.92× | 26 ns | 0.76× |
| 128 B | 41 ns | 67 ns | 1.62× | 35 ns | 0.84× |
| 512 B | 109 ns | 125 ns | 1.15× | 77 ns | 0.71× |
| 4 KiB | 727 ns | 740 ns | 1.02× | 488 ns | 0.67× |
| 64 KiB | 11.38 µs | 11.12 µs | 0.98× | 7.50 µs | 0.66× |
| 1 MiB | 182.51 µs | 177.33 µs | 0.97× | 119.39 µs | 0.65× |

### twox_256

| size | native-simd | PVM | ratio | native | ratio |
|---:|---:|---:|---:|---:|---:|
| 32 B | 52 ns | 129 ns | 2.50× | 47 ns | 0.91× |
| 128 B | 70 ns | 124 ns | 1.77× | 68 ns | 0.97× |
| 512 B | 194 ns | 239 ns | 1.23× | 156 ns | 0.80× |
| 4 KiB | 1.43 µs | 1.47 µs | 1.02× | 968 ns | 0.68× |
| 64 KiB | 22.73 µs | 22.20 µs | 0.98× | 14.86 µs | 0.65× |
| 1 MiB | 365.15 µs | 355.03 µs | 0.97× | 238.81 µs | 0.65× |
