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
same source and verified to produce identical outputs.

Ratios are vs the **best native build per algorithm** — the honest "host
function" baseline: the `target-cpu=native` build for blake2/keccak/sha2, the
portable build for twox (where `target-cpu=native` is a ~1.5× pessimization —
baselines were checked per algorithm, not assumed).

### Hashing
| algo | best native 32 B | PVM 32 B | ratio | best native 1 MiB | PVM 1 MiB | ratio |
|---|---:|---:|---:|---:|---:|---:|
| blake2_256 | 109 ns | 346 ns | 3.2× | 752 µs | 1.97 ms | **2.6×** |
| blake2_128 | 109 ns | 337 ns | 3.1× | 750 µs | 1.98 ms | **2.6×** |
| keccak_256 | 253 ns | 497 ns | 2.0× | 1.77 ms | 2.74 ms | **1.5×** |
| keccak_512 | 252 ns | 480 ns | 1.9× | 3.36 ms | 4.79 ms | **1.4×** |
| sha2_256 | 50 ns | 432 ns | 8.6× | 467 µs | 4.81 ms | **10.3×** |
| twox_64 | 13 ns | 43 ns | 3.3× | 59 µs | 248 µs | **4.2×** |
| twox_128 | 26 ns | 67 ns | 2.6× | 120 µs | 498 µs | **4.2×** |
| twox_256 | 47 ns | 121 ns | 2.6× | 236 µs | 996 µs | **4.2×** |


### Signature verification: ed25519 (preliminary)


| configuration | per verification | vs native |
|---|---:|---:|
| native | 30.7 µs | 1.0× |
| PVM 64-bit, compiler, sync gas | 102.3 µs | **3.3×** |

Pending: sr25519 and ecdsa/secp256k1-recover equivalents, batch variants,
and crate parity with `sp_io` (`ed25519-zebra`) — same methodology as
bench-hash.

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
| 32 B | 109 ns | 337 ns | 3.08× | 111 ns | 1.01× |
| 128 B | 104 ns | 319 ns | 3.06× | 108 ns | 1.04× |
| 512 B | 380 ns | 1.03 µs | 2.71× | 407 ns | 1.07× |
| 4 KiB | 2.96 µs | 7.76 µs | 2.62× | 3.16 µs | 1.07× |
| 64 KiB | 48.22 µs | 122.95 µs | 2.55× | 50.07 µs | 1.04× |
| 1 MiB | 749.53 µs | 1.98 ms | 2.65× | 806.25 µs | 1.08× |

### blake2_256

| size | native-simd | PVM | ratio | native | ratio |
|---:|---:|---:|---:|---:|---:|
| 32 B | 109 ns | 346 ns | 3.17× | 111 ns | 1.01× |
| 128 B | 105 ns | 330 ns | 3.15× | 109 ns | 1.04× |
| 512 B | 383 ns | 1.07 µs | 2.79× | 406 ns | 1.06× |
| 4 KiB | 2.97 µs | 7.80 µs | 2.63× | 3.16 µs | 1.06× |
| 64 KiB | 46.97 µs | 123.14 µs | 2.62× | 50.48 µs | 1.07× |
| 1 MiB | 751.82 µs | 1.97 ms | 2.62× | 822.75 µs | 1.09× |

### keccak_256

| size | native-simd | PVM | ratio | native | ratio |
|---:|---:|---:|---:|---:|---:|
| 32 B | 253 ns | 497 ns | 1.97× | 299 ns | 1.18× |
| 128 B | 253 ns | 494 ns | 1.96× | 294 ns | 1.16× |
| 512 B | 928 ns | 1.58 µs | 1.70× | 1.09 µs | 1.18× |
| 4 KiB | 7.13 µs | 11.09 µs | 1.56× | 8.36 µs | 1.17× |
| 64 KiB | 110.01 µs | 170.16 µs | 1.55× | 129.69 µs | 1.18× |
| 1 MiB | 1.77 ms | 2.74 ms | 1.55× | 2.08 ms | 1.17× |

### keccak_512

| size | native-simd | PVM | ratio | native | ratio |
|---:|---:|---:|---:|---:|---:|
| 32 B | 252 ns | 480 ns | 1.90× | 289 ns | 1.15× |
| 128 B | 474 ns | 825 ns | 1.74× | 554 ns | 1.17× |
| 512 B | 1.85 µs | 2.82 µs | 1.52× | 2.17 µs | 1.18× |
| 4 KiB | 12.96 µs | 18.91 µs | 1.46× | 15.33 µs | 1.18× |
| 64 KiB | 208.13 µs | 299.74 µs | 1.44× | 244.88 µs | 1.18× |
| 1 MiB | 3.36 ms | 4.79 ms | 1.43× | 3.92 ms | 1.17× |

### sha2_256

| size | native-simd | PVM | ratio | native | ratio |
|---:|---:|---:|---:|---:|---:|
| 32 B | 50 ns | 432 ns | 8.59× | 73 ns | 1.45× |
| 128 B | 111 ns | 1.02 µs | 9.20× | 176 ns | 1.59× |
| 512 B | 282 ns | 2.77 µs | 9.83× | 501 ns | 1.77× |
| 4 KiB | 1.87 µs | 19.06 µs | 10.18× | 3.49 µs | 1.86× |
| 64 KiB | 29.28 µs | 298.85 µs | 10.21× | 55.09 µs | 1.88× |
| 1 MiB | 466.92 µs | 4.81 ms | 10.31× | 882.49 µs | 1.89× |

### twox_64

| size | native-simd | PVM | ratio | native | ratio |
|---:|---:|---:|---:|---:|---:|
| 32 B | 20 ns | 43 ns | 2.12× | 13 ns | 0.63× |
| 128 B | 29 ns | 66 ns | 2.28× | 16 ns | 0.57× |
| 512 B | 61 ns | 158 ns | 2.58× | 37 ns | 0.61× |
| 4 KiB | 369 ns | 1.01 µs | 2.73× | 244 ns | 0.66× |
| 64 KiB | 5.65 µs | 15.49 µs | 2.74× | 3.68 µs | 0.65× |
| 1 MiB | 90.33 µs | 248.16 µs | 2.75× | 59.39 µs | 0.66× |

### twox_128

| size | native-simd | PVM | ratio | native | ratio |
|---:|---:|---:|---:|---:|---:|
| 32 B | 34 ns | 67 ns | 1.98× | 26 ns | 0.78× |
| 128 B | 41 ns | 114 ns | 2.75× | 34 ns | 0.83× |
| 512 B | 108 ns | 298 ns | 2.76× | 77 ns | 0.71× |
| 4 KiB | 735 ns | 2.00 µs | 2.73× | 484 ns | 0.66× |
| 64 KiB | 11.38 µs | 31.02 µs | 2.73× | 7.39 µs | 0.65× |
| 1 MiB | 182.77 µs | 497.63 µs | 2.72× | 119.74 µs | 0.66× |

### twox_256

| size | native-simd | PVM | ratio | native | ratio |
|---:|---:|---:|---:|---:|---:|
| 32 B | 50 ns | 121 ns | 2.40× | 47 ns | 0.92× |
| 128 B | 68 ns | 214 ns | 3.15× | 68 ns | 1.00× |
| 512 B | 189 ns | 580 ns | 3.08× | 153 ns | 0.81× |
| 4 KiB | 1.42 µs | 4.02 µs | 2.83× | 964 ns | 0.68× |
| 64 KiB | 22.80 µs | 62.15 µs | 2.73× | 14.73 µs | 0.65× |
| 1 MiB | 366.32 µs | 995.53 µs | 2.72× | 235.65 µs | 0.64× |
