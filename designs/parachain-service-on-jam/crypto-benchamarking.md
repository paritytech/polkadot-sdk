# Crypto benchmarking for the Parachain Service: hashing & signatures

Comparison of in-PVM vs on-host execution of the hash and signature functions
the Parachain Service / PVF stack needs, to decide which (if any) require
Gray Paper host calls and which can stay PVM guest code (see design doc §4.3).

## Results

Setup: PolkaVM **64-bit, recompiler backend, synchronous gas metering** (polkajam's
production configuration). Two host builds of the same code:

- **host native** — `-C target-cpu=native`: every ISA extension of the build
  machine enabled unconditionally (AVX2, SHA-NI, …). Not portable: crashes
  (SIGILL) on CPUs lacking any of those extensions, so it must be rebuilt on
  every benchmark machine.
- **host portable** — plain build; runs on any x86-64. Crates may still
  runtime-dispatch to fast paths (e.g. sha2 uses SHA-NI where available),
  which makes this the closest analogue of a production `sp_io` host
  function.

Times are per hash, including hasher construction. Transpilation and
instantiation are excluded (steady-state). All artifacts are built from the
same source and verified to produce identical outputs. Guest artifacts are
built with two build-configuration fixes (see *Build tweaks* below).

Ratios are vs the **best host build per algorithm**: host native for
blake2/keccak/sha2, host portable for twox (where `target-cpu=native` is a
~1.5× pessimization — baselines were checked per algorithm, not assumed).

Two machines, because the ratios turn out to be strongly
microarchitecture-dependent (see *Hardware dependence* below):

- **A** — AMD Ryzen 9 5950X (Zen 3): AVX2, SHA-NI.
- **B** — AMD Ryzen Threadripper PRO 7995WX (Zen 4): AVX2, SHA-NI, AVX-512.

ASLR disabled by benchtool. Result checksums match across all runs and
both machines.

### Hashing
| algo | size | A best host | A PVM | A ratio | B best host | B PVM | B ratio |
|---|---|---:|---:|---:|---:|---:|---:|
| blake2_256 | 32 B | 108 ns | 297 ns | 2.8× | 96 ns | 129 ns | **1.3×** |
| blake2_256 | 1 MiB | 749.4 µs | 1.47 ms | 2.0× | 655.7 µs | 891.8 µs | **1.4×** |
| blake2_128 | 32 B | 106 ns | 293 ns | 2.8× | 95 ns | 127 ns | **1.3×** |
| blake2_128 | 1 MiB | 725.7 µs | 1.47 ms | 2.0× | 661.9 µs | 890.2 µs | **1.3×** |
| keccak_256 | 32 B | 244 ns | 506 ns | 2.1× | 265 ns | 293 ns | **1.1×** |
| keccak_256 | 1 MiB | 1.69 ms | 2.52 ms | 1.5× | 1.85 ms | 1.88 ms | **1.0×** |
| keccak_512 | 32 B | 244 ns | 481 ns | 2.0× | 267 ns | 286 ns | **1.1×** |
| keccak_512 | 1 MiB | 3.20 ms | 4.73 ms | 1.5× | 3.48 ms | 3.52 ms | **1.0×** |
| sha2_256 | 32 B | 49 ns | 432 ns | 8.9× | 51 ns | 253 ns | **5.0×** |
| sha2_256 | 1 MiB | 456.6 µs | 4.50 ms | 9.8× | 414.4 µs | 2.93 ms | **7.1×** |
| twox_64 | 32 B | 12 ns | 42 ns | 3.4× | 11 ns | 19 ns | **1.7×** |
| twox_64 | 1 MiB | 57.4 µs | 89.2 µs | 1.6× | 38.7 µs | 76.9 µs | **2.0×** |
| twox_128 | 32 B | 25 ns | 65 ns | 2.6× | 22 ns | 22 ns | **1.0×** |
| twox_128 | 1 MiB | 113.3 µs | 179.7 µs | 1.6× | 77.6 µs | 153.8 µs | **2.0×** |
| twox_256 | 32 B | 44 ns | 121 ns | 2.8× | 36 ns | 40 ns | **1.1×** |
| twox_256 | 1 MiB | 231.1 µs | 355.9 µs | 1.5× | 154.4 µs | 307.6 µs | **2.0×** |

### Signature verification & key recovery

One operation per measurement, fixed fixtures. Host times are the
host-portable build; see appendix for the host-native results.

| benchmark (implementation) | A host | A PVM | A ratio | B host | B PVM | B ratio |
|---|---:|---:|---:|---:|---:|---:|
| ed25519 (`ed25519-dalek`) | 31.3 µs | 102.9 µs | 3.3× | 16.1 µs | 68.1 µs | 4.2× |
| ed25519 (`ed25519-zebra`, = sp_core) | 34.4 µs | 101.5 µs | 3.0× | 15.8 µs | 69.6 µs | 4.4× |
| sr25519 (`schnorrkel`, = sp_core) | 32.7 µs | 99.5 µs | 3.0× | 16.7 µs | 64.4 µs | 3.8× |
| ecdsa_verify (`k256`, = sp_core no_std) | 74.3 µs | 232.4 µs | 3.1× | 61.5 µs | 169.3 µs | 2.8× |
| ecdsa_verify (`libsecp256k1`) | 124.5 µs | 241.4 µs | 1.9× | 111.6 µs | 161.5 µs | 1.4× |
| secp256k1_ecdsa_recover (`k256`) | 143.5 µs | 443.4 µs | 3.1× | 124.1 µs | 338.5 µs | 2.7× |
| secp256k1_ecdsa_recover (`libsecp256k1`) | 131.8 µs | 240.0 µs | 1.8× | 118.6 µs | 170.6 µs | 1.4× |

- Not benchmarked: `*_batch_verify` 

### Hardware dependence

The PVM-vs-host ratios are not a constant of the workload — they depend
heavily on the host microarchitecture:

- **Hashing ratios shrink on the newer core** (blake2 2.0× → 1.4×, keccak
  1.5× → ~1.0× — parity). The recompiled code carries ~2× the instructions
  of host code (register spills: PVM has 13 registers; 3-op → 2-op
  translation), and what that costs is decided by the host CPU's front-end
  capacity: Zen 4's larger µop cache absorbs it almost completely. Ratios
  measured on older hardware overstate the PVM penalty on modern
  validator-grade machines.
- **sha2_256 stays an outlier on both** (9.8× / 7.1×): the host baseline is
  SHA-NI silicon; no software — recompiled or not — competes with it.
- **twox bucks the trend** (1.6× → 2.0× bulk): the host side gains more from
  machine B's memory pipeline than the PVM side does. In absolute terms the
  penalty stays trivial (tens of µs per MiB).
- **25519-family host times halve on machine B** (e.g. ed25519-zebra 34 µs →
  16 µs), which *worsens* the PVM ratios (3.0× → 4.4×) even though the PVM
  side also improved ~1.5×. Whether this is purely Zen 4 (BMI2/ADX-heavy
  field arithmetic profiting from the wider core) or partly a toolchain
  difference between the machines is not yet verified.

The absolute PVM costs — the numbers the refine-budget question actually
needs — improve on the newer machine across the board (e.g. ed25519 verify
103 µs → 68 µs, blake2 1 MiB 1.47 ms → 0.89 ms).

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

A host call pays off when `PVM_time − host_time > n × crossing + copy(len)`.
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
- `Crypto` (this report): `ed25519_verify` (dalek + zebra), `sr25519_verify`,
  `ecdsa_verify` (k256 + libsecp256k1), `secp256k1_ecdsa_recover`
  (k256 + libsecp256k1); prehashed/compressed/batch variants intentionally
  skipped (see above)

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
discovered artifact (PVM blob, host-portable `.so`, `*_native.so` variants)
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

The signature numbers come from individual guest crates in the same suite —
the pre-existing `bench-ed25519` plus `bench-ed25519-zebra`, `bench-sr25519`,
`bench-ecdsa-{k256,libsecp}`, `bench-recover-{k256,libsecp}` — one fixed,
cross-validated fixture and one operation per `run()`, measured by the
generic harness (`runtime` variant = steady-state execution):

```
./run-crypto-benches > crypto.txt
./process-crypto-results.sh crypto.txt
```

## Appendix: full tables

Raw data for both machines: `tools/benchtool/output_00` (machine A) and
`tools/benchtool/output_00_toaster` (machine B) on the `mku-bench-hash`
branch.

### Signatures, host-portable build — machine A

| benchmark | host portable | PVM (64-bit, sync gas) | ratio |
|---|---:|---:|---:|
| ed25519 | 31.31 µs | 102.94 µs | 3.29× |
| ed25519-zebra | 34.39 µs | 101.54 µs | 2.95× |
| sr25519 | 32.73 µs | 99.45 µs | 3.04× |
| ecdsa-k256 | 74.35 µs | 232.45 µs | 3.13× |
| ecdsa-libsecp | 124.53 µs | 241.40 µs | 1.94× |
| recover-k256 | 143.53 µs | 443.42 µs | 3.09× |
| recover-libsecp | 131.83 µs | 239.99 µs | 1.82× |

### Signatures, host-portable build — machine B

| benchmark | host portable | PVM (64-bit, sync gas) | ratio |
|---|---:|---:|---:|
| ed25519 | 16.05 µs | 68.09 µs | 4.24× |
| ed25519-zebra | 15.79 µs | 69.63 µs | 4.41× |
| sr25519 | 16.72 µs | 64.37 µs | 3.85× |
| ecdsa-k256 | 61.52 µs | 169.34 µs | 2.75× |
| ecdsa-libsecp | 111.59 µs | 161.48 µs | 1.45× |
| recover-k256 | 124.11 µs | 338.46 µs | 2.73× |
| recover-libsecp | 118.64 µs | 170.56 µs | 1.44× |

### Signatures, host-native build

| benchmark | host native | PVM (64-bit, sync gas) | ratio |
|---|---:|---:|---:|
| ed25519 | 29.01 µs | 100.27 µs | 3.46× |
| ed25519-zebra | 28.38 µs | 95.07 µs | 3.35× |
| sr25519 | 29.45 µs | 98.39 µs | 3.34× |
| ecdsa-k256 | 69.67 µs | 225.51 µs | 3.24× |
| ecdsa-libsecp | 129.20 µs | 241.51 µs | 1.87× |
| recover-k256 | 140.69 µs | 453.49 µs | 3.22× |
| recover-libsecp | 145.47 µs | 252.30 µs | 1.73× |

*pvm* = PVM guest blob (recompiler, sync gas) · *native* = host native
build (`-C target-cpu=native`, build machine only) · *portable* = host
portable build (runs on any x86-64; crates may runtime-dispatch, e.g.
sha2 uses SHA-NI where available)

This table is from machine A.

### Hashing — machine A

*pvm* = PVM guest blob (recompiler, sync gas) · *native* = host native
build (`-C target-cpu=native`, build machine only) · *portable* = host
portable build (runs on any x86-64; crates may runtime-dispatch, e.g.
sha2 uses SHA-NI where available)

#### blake2_128

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 293 ns | 106 ns | 113 ns | 2.77x | 2.59x | 0.94x |
| 128 B | 274 ns | 100 ns | 111 ns | 2.73x | 2.47x | 0.90x |
| 512 B | 848 ns | 366 ns | 409 ns | 2.32x | 2.07x | 0.89x |
| 4 KiB | 5.88 µs | 2.84 µs | 3.22 µs | 2.07x | 1.83x | 0.88x |
| 64 KiB | 92.19 µs | 45.02 µs | 51.20 µs | 2.05x | 1.80x | 0.88x |
| 1 MiB | 1.47 ms | 725.70 µs | 817.14 µs | 2.02x | 1.80x | 0.89x |

#### blake2_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 297 ns | 108 ns | 113 ns | 2.75x | 2.64x | 0.96x |
| 128 B | 279 ns | 104 ns | 112 ns | 2.69x | 2.49x | 0.93x |
| 512 B | 848 ns | 378 ns | 412 ns | 2.25x | 2.06x | 0.92x |
| 4 KiB | 5.89 µs | 2.93 µs | 3.23 µs | 2.01x | 1.82x | 0.91x |
| 64 KiB | 91.72 µs | 46.71 µs | 51.43 µs | 1.96x | 1.78x | 0.91x |
| 1 MiB | 1.47 ms | 749.38 µs | 826.63 µs | 1.96x | 1.77x | 0.91x |

#### keccak_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 506 ns | 244 ns | 312 ns | 2.07x | 1.62x | 0.78x |
| 128 B | 497 ns | 244 ns | 303 ns | 2.03x | 1.64x | 0.81x |
| 512 B | 1.48 µs | 898 ns | 1.13 µs | 1.65x | 1.31x | 0.80x |
| 4 KiB | 10.41 µs | 6.82 µs | 8.52 µs | 1.53x | 1.22x | 0.80x |
| 64 KiB | 158.69 µs | 105.67 µs | 132.59 µs | 1.50x | 1.20x | 0.80x |
| 1 MiB | 2.52 ms | 1.69 ms | 2.12 ms | 1.49x | 1.19x | 0.80x |

#### keccak_512

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 481 ns | 244 ns | 296 ns | 1.98x | 1.62x | 0.82x |
| 128 B | 802 ns | 459 ns | 564 ns | 1.75x | 1.42x | 0.81x |
| 512 B | 2.77 µs | 1.79 µs | 2.22 µs | 1.55x | 1.25x | 0.81x |
| 4 KiB | 18.61 µs | 12.54 µs | 15.52 µs | 1.48x | 1.20x | 0.81x |
| 64 KiB | 295.80 µs | 200.22 µs | 250.83 µs | 1.48x | 1.18x | 0.80x |
| 1 MiB | 4.73 ms | 3.20 ms | 3.97 ms | 1.48x | 1.19x | 0.81x |

#### sha2_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 432 ns | 49 ns | 73 ns | 8.88x | 5.92x | 0.67x |
| 128 B | 1.03 µs | 104 ns | 176 ns | 9.87x | 5.86x | 0.59x |
| 512 B | 2.66 µs | 267 ns | 494 ns | 9.97x | 5.39x | 0.54x |
| 4 KiB | 18.03 µs | 1.82 µs | 3.45 µs | 9.91x | 5.23x | 0.53x |
| 64 KiB | 280.92 µs | 28.40 µs | 53.79 µs | 9.89x | 5.22x | 0.53x |
| 1 MiB | 4.50 ms | 456.62 µs | 849.87 µs | 9.84x | 5.29x | 0.54x |

#### twox_64

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 42 ns | 20 ns | 12 ns | 2.15x | 3.42x | 1.59x |
| 128 B | 44 ns | 28 ns | 16 ns | 1.57x | 2.82x | 1.80x |
| 512 B | 73 ns | 60 ns | 36 ns | 1.23x | 2.05x | 1.67x |
| 4 KiB | 380 ns | 359 ns | 232 ns | 1.06x | 1.64x | 1.55x |
| 64 KiB | 5.58 µs | 5.53 µs | 3.58 µs | 1.01x | 1.56x | 1.55x |
| 1 MiB | 89.21 µs | 87.76 µs | 57.40 µs | 1.02x | 1.55x | 1.53x |

#### twox_128

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 65 ns | 33 ns | 25 ns | 1.98x | 2.59x | 1.31x |
| 128 B | 67 ns | 40 ns | 33 ns | 1.68x | 2.05x | 1.22x |
| 512 B | 125 ns | 103 ns | 72 ns | 1.21x | 1.73x | 1.43x |
| 4 KiB | 747 ns | 702 ns | 462 ns | 1.06x | 1.62x | 1.52x |
| 64 KiB | 11.21 µs | 10.97 µs | 7.07 µs | 1.02x | 1.59x | 1.55x |
| 1 MiB | 179.72 µs | 176.40 µs | 113.33 µs | 1.02x | 1.59x | 1.56x |

#### twox_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 121 ns | 50 ns | 44 ns | 2.41x | 2.76x | 1.15x |
| 128 B | 124 ns | 68 ns | 64 ns | 1.82x | 1.92x | 1.05x |
| 512 B | 240 ns | 190 ns | 149 ns | 1.26x | 1.61x | 1.28x |
| 4 KiB | 1.47 µs | 1.42 µs | 922 ns | 1.03x | 1.60x | 1.54x |
| 64 KiB | 22.25 µs | 22.55 µs | 14.41 µs | 0.99x | 1.54x | 1.56x |
| 1 MiB | 355.95 µs | 361.37 µs | 231.07 µs | 0.98x | 1.54x | 1.56x |

### Hashing — machine B

#### blake2_128

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 127 ns | 95 ns | 95 ns | 1.33x | 1.33x | 1.00x |
| 128 B | 125 ns | 91 ns | 95 ns | 1.37x | 1.31x | 0.96x |
| 512 B | 450 ns | 331 ns | 352 ns | 1.36x | 1.28x | 0.94x |
| 4 KiB | 3.48 µs | 2.57 µs | 2.77 µs | 1.36x | 1.26x | 0.93x |
| 64 KiB | 55.43 µs | 40.99 µs | 44.13 µs | 1.35x | 1.26x | 0.93x |
| 1 MiB | 890.16 µs | 661.92 µs | 707.82 µs | 1.34x | 1.26x | 0.94x |

#### blake2_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 129 ns | 97 ns | 96 ns | 1.33x | 1.34x | 1.01x |
| 128 B | 125 ns | 91 ns | 96 ns | 1.38x | 1.31x | 0.95x |
| 512 B | 450 ns | 331 ns | 353 ns | 1.36x | 1.27x | 0.94x |
| 4 KiB | 3.48 µs | 2.58 µs | 2.77 µs | 1.35x | 1.26x | 0.93x |
| 64 KiB | 55.48 µs | 40.96 µs | 44.12 µs | 1.35x | 1.26x | 0.93x |
| 1 MiB | 891.81 µs | 655.66 µs | 708.78 µs | 1.36x | 1.26x | 0.93x |

#### keccak_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 293 ns | 436 ns | 265 ns | 0.67x | 1.10x | 1.65x |
| 128 B | 291 ns | 435 ns | 258 ns | 0.67x | 1.13x | 1.69x |
| 512 B | 1.02 µs | 1.68 µs | 987 ns | 0.61x | 1.03x | 1.70x |
| 4 KiB | 7.60 µs | 12.94 µs | 7.44 µs | 0.59x | 1.02x | 1.74x |
| 64 KiB | 117.50 µs | 200.59 µs | 115.48 µs | 0.59x | 1.02x | 1.74x |
| 1 MiB | 1.88 ms | 3.21 ms | 1.85 ms | 0.59x | 1.02x | 1.74x |

#### keccak_512

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 286 ns | 433 ns | 267 ns | 0.66x | 1.07x | 1.62x |
| 128 B | 524 ns | 847 ns | 502 ns | 0.62x | 1.04x | 1.69x |
| 512 B | 1.98 µs | 3.34 µs | 1.93 µs | 0.59x | 1.02x | 1.73x |
| 4 KiB | 13.80 µs | 23.64 µs | 13.64 µs | 0.58x | 1.01x | 1.73x |
| 64 KiB | 219.80 µs | 377.61 µs | 217.96 µs | 0.58x | 1.01x | 1.73x |
| 1 MiB | 3.52 ms | 6.03 ms | 3.48 ms | 0.58x | 1.01x | 1.73x |

#### sha2_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 253 ns | 51 ns | 77 ns | 5.00x | 3.28x | 0.66x |
| 128 B | 729 ns | 108 ns | 159 ns | 6.74x | 4.60x | 0.68x |
| 512 B | 1.87 µs | 261 ns | 445 ns | 7.16x | 4.20x | 0.59x |
| 4 KiB | 11.85 µs | 1.67 µs | 3.11 µs | 7.08x | 3.81x | 0.54x |
| 64 KiB | 183.28 µs | 25.93 µs | 48.84 µs | 7.07x | 3.75x | 0.53x |
| 1 MiB | 2.93 ms | 414.38 µs | 781.30 µs | 7.06x | 3.74x | 0.53x |

#### twox_64

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 19 ns | 16 ns | 11 ns | 1.14x | 1.71x | 1.50x |
| 128 B | 23 ns | 20 ns | 15 ns | 1.16x | 1.59x | 1.37x |
| 512 B | 51 ns | 34 ns | 32 ns | 1.50x | 1.61x | 1.07x |
| 4 KiB | 313 ns | 167 ns | 209 ns | 1.88x | 1.50x | 0.80x |
| 64 KiB | 4.81 µs | 2.42 µs | 3.21 µs | 1.99x | 1.50x | 0.75x |
| 1 MiB | 76.89 µs | 38.68 µs | 51.37 µs | 1.99x | 1.50x | 0.75x |

#### twox_128

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 22 ns | 25 ns | 22 ns | 0.87x | 0.99x | 1.14x |
| 128 B | 34 ns | 28 ns | 29 ns | 1.19x | 1.18x | 0.99x |
| 512 B | 90 ns | 43 ns | 64 ns | 2.10x | 1.40x | 0.67x |
| 4 KiB | 622 ns | 302 ns | 422 ns | 2.06x | 1.47x | 0.72x |
| 64 KiB | 9.62 µs | 4.82 µs | 6.42 µs | 2.00x | 1.50x | 0.75x |
| 1 MiB | 153.78 µs | 77.65 µs | 102.55 µs | 1.98x | 1.50x | 0.76x |

#### twox_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 40 ns | 38 ns | 36 ns | 1.06x | 1.10x | 1.04x |
| 128 B | 65 ns | 44 ns | 55 ns | 1.47x | 1.17x | 0.80x |
| 512 B | 177 ns | 75 ns | 125 ns | 2.37x | 1.41x | 0.60x |
| 4 KiB | 1.24 µs | 569 ns | 828 ns | 2.19x | 1.50x | 0.69x |
| 64 KiB | 19.24 µs | 9.59 µs | 12.85 µs | 2.01x | 1.50x | 0.75x |
| 1 MiB | 307.64 µs | 154.36 µs | 206.15 µs | 1.99x | 1.49x | 0.75x |

## Appendix: usage recap

Where each algorithm shows up in the Substrate/Polkadot stack, and which
table rows that makes relevant:

| algo | used for | typical input |
|---|---|---|
| blake2_256 | state-trie nodes (`sp-trie`; the PVF hot path), header/extrinsic/block hashes, signing payloads ≥ 256 B, code & PoV hashes | trie nodes ~100–550 B; headers ~150–300 B; code/PoV MB-scale |
| blake2_128 | `Blake2_128Concat` storage-key hasher | 4–32 B |
| twox_128 / twox_64 | storage prefixes (pallet/storage names; mostly precomputed), `Twox64Concat` keys | 10–30 B |
| keccak_256 | Ethereum compat only: Frontier tries, bridges, BEEFY | ~32–550 B |
| sha2_256 | essentially unused in Substrate core (Bitcoin-style bridges) | — |
| sr25519 / ed25519 verify | transaction signatures | per-op (payload < 256 B, else blake2-hashed first) |
| ecdsa verify / recover | Ethereum-compat transactions | per-op (32-B prehash) |

Consequently the most important hashing rows are **blake2 at 128–512 B** (trie nodes).
