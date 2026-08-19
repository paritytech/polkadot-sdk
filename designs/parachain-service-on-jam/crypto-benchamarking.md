# Crypto benchmarking for the Parachain Service: hashing & signatures

Comparison of in-PVM vs on-host execution of the hash and signature functions
the Parachain Service / PVF stack needs, to decide which (if any) require
Gray Paper host calls and which can stay PVM guest code (see design doc §4.3).

## What is benchmarked

The PVF-relevant subset of
[`sp_io`](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/primitives/io/src/lib.rs)
host functions (`sp_io::trie` reduces to blake2_256/keccak_256 + plain Rust,
so it is not benchmarked separately):

- `Hashing`: `blake2_128`, `blake2_256`, `keccak_256`,
  `keccak_512`, `sha2_256`, `twox_64`, `twox_128`, `twox_256`
- `Crypto`: `ed25519_verify` (dalek + zebra), `sr25519_verify`,
  `ecdsa_verify` (k256 + libsecp256k1), `secp256k1_ecdsa_recover`
  (k256 + libsecp256k1); intentionally skipped: `*_batch_verify`
  (deprecated, sequential under the hood), `*_prehashed` (delta vs the
  non-prehashed variant is a known blake2 hash), compressed-key variants
  (serialization only)

Implementations match `sp_crypto_hashing`: `blake2b_simd`, `sha2`, `sha3`,
`twox-hash` (1/2/4 seeded XxHash64 passes).

Plus the arkworks elliptic-curve host functions of
[`sp_crypto_ec_utils`](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/primitives/crypto/ec-utils/src/lib.rs),
which are a separate crate from `sp_io` and conditionally enabled per-PVF
(`ExecutorParam::EnabledHostFunction(EccRfc163)`). All 12 of
[`HostFunctionsRfc163`](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/primitives/crypto/ec-utils/src/lib.rs#L69):

- `bls12_381`: `multi_miller_loop`, `final_exponentiation`, `msm_g1`, `msm_g2`,
  `mul_g1`, `mul_g2` — bridges/BEEFY/Ethereum
- `ed_on_bls12_381_bandersnatch`: `msm`, `mul` — JAM Safrole / ring-VRF curve
- `pallas`, `vesta`: `msm`, `mul` each — the Pasta cycle

Measured on the plain `ark-*` 0.5.0 crates, not the `-ext` hook crates — those
are the offload plumbing. Only the compute kernel; the `ArkScale` codec is
excluded, being noise at these magnitudes.

Two extra rows, `bls12_377` and `bw6_761` pairings, show how cost scales with
field width. They are in the crate but **not** in `HostFunctionsRfc163`.

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

Ratios are vs the **best host build per algorithm and machine** —
`target-cpu=native` is not always a win (it pessimizes twox ~1.5× on
machine A), so baselines were checked per cell, not assumed.

Host libraries are built with **stable Rust (1.86)** — the production
configuration; guest blobs use the suite's pinned nightly (required for
`build-std`). The toolchain choice measurably moves several host baselines
in both directions — see *Toolchain impact on host baselines* below.

Two machines, because the ratios turn out to be strongly
microarchitecture-dependent (see *Hardware dependence* below):

- **A** — AMD Ryzen 9 5950X (Zen 3): AVX2, SHA-NI.
- **B** — AMD Ryzen Threadripper PRO 7995WX (Zen 4): AVX2, SHA-NI, AVX-512.

ASLR disabled by benchtool. Numbers are from one representative run out of
five per machine; cross-run spread of the quoted cells is within ~10%
(mostly ~2%).

### Hashing
| algo | size | A best host | A PVM | A ratio | B best host | B PVM | B ratio |
|---|---|---:|---:|---:|---:|---:|---:|
| blake2_256 | 32 B | 101 ns | 287 ns | 2.8× | 86 ns | 124 ns | **1.4×** |
| blake2_256 | 1 MiB | 753.3 µs | 1.71 ms | 2.3× | 655.6 µs | 904.7 µs | **1.4×** |
| blake2_128 | 32 B | 96 ns | 279 ns | 2.9× | 86 ns | 124 ns | **1.4×** |
| blake2_128 | 1 MiB | 740.2 µs | 1.64 ms | 2.2× | 655.3 µs | 903.5 µs | **1.4×** |
| keccak_256 | 32 B | 252 ns | 506 ns | 2.0× | 217 ns | 294 ns | **1.4×** |
| keccak_256 | 1 MiB | 1.77 ms | 2.58 ms | 1.5× | 1.58 ms | 1.89 ms | **1.2×** |
| keccak_512 | 32 B | 251 ns | 485 ns | 1.9× | 211 ns | 285 ns | **1.3×** |
| keccak_512 | 1 MiB | 3.32 ms | 4.82 ms | 1.5× | 2.89 ms | 3.52 ms | **1.2×** |
| sha2_256 | 32 B | 49 ns | 435 ns | 8.8× | 43 ns | 249 ns | **5.8×** |
| sha2_256 | 1 MiB | 470.4 µs | 4.46 ms | 9.5× | 414.5 µs | 2.92 ms | **7.0×** |
| twox_64 | 32 B | 11 ns | 36 ns | 3.2× | 10 ns | 19 ns | **1.9×** |
| twox_64 | 1 MiB | 58.6 µs | 90.3 µs | 1.5× | 39.3 µs | 78.1 µs | **2.0×** |
| twox_128 | 32 B | 27 ns | 50 ns | 1.8× | 20 ns | 20 ns | **1.0×** |
| twox_128 | 1 MiB | 118.9 µs | 180.3 µs | 1.5× | 77.9 µs | 154.5 µs | **2.0×** |
| twox_256 | 32 B | 42 ns | 83 ns | 2.0× | 31 ns | 35 ns | **1.1×** |
| twox_256 | 1 MiB | 234.9 µs | 362.0 µs | 1.5× | 155.7 µs | 308.4 µs | **2.0×** |

### Signature verification & key recovery

One operation per measurement, fixed fixtures. Host times are the
host-portable build; see appendix for the host-native results.

| benchmark (implementation) | A host | A PVM | A ratio | B host | B PVM | B ratio |
|---|---:|---:|---:|---:|---:|---:|
| ed25519 (`ed25519-dalek`) | 30.6 µs | 102.9 µs | 3.4× | 29.4 µs | 67.6 µs | 2.3× |
| ed25519 (`ed25519-zebra`, = sp_core) | 32.6 µs | 97.8 µs | 3.0× | 24.6 µs | 63.2 µs | 2.6× |
| sr25519 (`schnorrkel`, = sp_core) | 33.9 µs | 98.0 µs | 2.9× | 24.9 µs | 63.3 µs | 2.5× |
| ecdsa_verify (`k256`, = sp_core no_std) | 73.4 µs | 230.1 µs | 3.1× | 61.4 µs | 170.5 µs | 2.8× |
| ecdsa_verify (`libsecp256k1`) | 128.3 µs | 234.4 µs | 1.8× | 112.2 µs | 161.5 µs | 1.4× |
| secp256k1_ecdsa_recover (`k256`) | 148.9 µs | 459.6 µs | 3.1× | 123.8 µs | 339.0 µs | 2.7× |
| secp256k1_ecdsa_recover (`libsecp256k1`) | 136.7 µs | 256.5 µs | 1.9× | 119.3 µs | 171.2 µs | 1.4× |

### Elliptic curves (`sp_crypto_ec_utils`, RFC-163)

Single run per machine, not five. The host-native build for these rows also
enables `ark-ff/asm`, arkworks' bigint assembly. MSM rows are at n=1024; both host
builds and the full 16/64/256/1024 grids are in the appendix.

| operation | A best host | A PVM | A ratio | B best host | B PVM | B ratio |
|---|---:|---:|---:|---:|---:|---:|
| bls12-381 pairing (miller + final_exp) | 858.72 µs | 2.70 ms | 3.2× | 706.78 µs | 1.68 ms | **2.4×** |
| bls12-381 msm_g1 (n=1024) | 18.22 ms | 45.30 ms | 2.5× | 16.10 ms | 29.41 ms | **1.8×** |
| bls12-381 msm_g2 (n=1024) | 59.69 ms | 166.83 ms | 2.8× | 52.84 ms | 101.40 ms | **1.9×** |
| bls12-381 mul_g1 | 129.20 µs | 322.79 µs | 2.5× | 117.73 µs | 213.73 µs | **1.8×** |
| bls12-381 mul_g2 | 399.68 µs | 1.19 ms | 3.0× | 349.95 µs | 708.19 µs | **2.0×** |
| bandersnatch msm (n=1024) | 10.77 ms | 23.31 ms | 2.2× | 9.40 ms | 17.17 ms | **1.8×** |
| bandersnatch mul | 63.72 µs | 154.66 µs | 2.4× | 55.52 µs | 107.59 µs | **1.9×** |
| pallas msm (n=1024) | 9.79 ms | 20.30 ms | 2.1× | 8.74 ms | 14.46 ms | **1.7×** |
| pallas mul | 62.57 µs | 144.46 µs | 2.3× | 56.17 µs | 97.30 µs | **1.7×** |
| vesta msm (n=1024) | 9.74 ms | 19.91 ms | 2.0× | 8.73 ms | 14.48 ms | **1.7×** |
| vesta mul | 62.57 µs | 143.49 µs | 2.3× | 56.14 µs | 93.18 µs | **1.7×** |

`final_exponentiation` is measured only as part of the full pairing; nothing calls
it without a Miller loop first.

### Hardware dependence

The PVM-vs-host ratios are not a constant of the workload — they depend
heavily on the host microarchitecture:

- **Hashing ratios shrink on the newer core** (blake2 2.3× → 1.4×, keccak
  1.5× → 1.2×). The recompiled code carries ~2× the instructions
  of host code (register spills: PVM has 13 registers; 3-op → 2-op
  translation), and what that costs is decided by the host CPU's front-end
  capacity: Zen 4's larger µop cache absorbs it almost completely. Ratios
  measured on older hardware overstate the PVM penalty on modern
  validator-grade machines.
- **sha2_256 stays an outlier on both** (9.5× / 7.0×): the host baseline is
  SHA-NI silicon; no software — recompiled or not — competes with it.
- **twox bucks the trend** (1.5× → 2.0× bulk): the host side gains more from
  machine B's memory pipeline than the PVM side does. In absolute terms the
  penalty stays trivial (tens of µs per MiB).
- **25519-family ratios improve on machine B** (3.0× → 2.5×): the PVM side
  gains ~1.5× while host times move less — with stable-built hosts. On
  nightly-built hosts Zen 4's AVX-512-IFMA cuts host times 1.5–1.9× and the
  ratios read 3.7–4.4× instead; see *Toolchain impact on host baselines*.

The absolute PVM costs — the numbers the refine-budget question actually
needs — improve on the newer machine across the board (e.g. ed25519 verify
103 µs → 68 µs, blake2 1 MiB 1.71 ms → 0.90 ms).

### Toolchain impact on host baselines

Host numbers are a property of **machine × toolchain**, not machine alone.
Measured explicitly on machine B by rebuilding all host libraries with
`nightly-2026-08-01` vs stable 1.86 (full tables in the appendix; PVM cells
are unaffected — guest blobs always build with the suite's pinned
toolchain). Hashing at 1 MiB, signatures per operation (crypto
native-stable cells: `output_new_01_toaster`):

| algorithm | portable stable | portable nightly | Δ | native stable | native nightly | Δ |
|---|---:|---:|---:|---:|---:|---:|
| blake2_256 (1 MiB) | 726.3 µs | 730.4 µs | ≈ | 655.6 µs | 653.4 µs | ≈ |
| keccak_256 (1 MiB) | 1.85 ms | 1.85 ms | ≈ | 1.58 ms | 3.67 ms | +132% |
| keccak_512 (1 MiB) | 3.55 ms | 3.48 ms | ≈ | 2.89 ms | 6.85 ms | +137% |
| sha2_256 (1 MiB) | 414.7 µs | 414.6 µs | ≈ | 414.5 µs | 413.9 µs | ≈ |
| twox_64 (1 MiB) | 52.0 µs | 51.7 µs | ≈ | 39.3 µs | 45.7 µs | +16% |
| twox_128 (1 MiB) | 117.9 µs | 106.2 µs | -10% | 77.9 µs | 91.2 µs | +17% |
| twox_256 (1 MiB) | 205.5 µs | 205.8 µs | ≈ | 155.7 µs | 181.1 µs | +16% |
| ed25519 (dalek) | 29.4 µs | 15.6 µs | -47% | 24.5 µs | 15.6 µs | -36% |
| ed25519 (zebra) | 24.6 µs | 15.7 µs | -36% | 23.0 µs | 15.8 µs | -32% |
| sr25519 | 24.9 µs | 16.4 µs | -34% | 25.8 µs | 17.2 µs | -33% |
| ecdsa_verify (k256) | 61.4 µs | 61.5 µs | ≈ | 59.6 µs | 61.6 µs | +3% |
| ecdsa_verify (libsecp) | 112.2 µs | 111.2 µs | ≈ | 116.3 µs | 116.5 µs | ≈ |
| recover (k256) | 123.8 µs | 124.0 µs | ≈ | 119.8 µs | 124.8 µs | +4% |
| recover (libsecp) | 119.3 µs | 117.8 µs | ≈ | 124.3 µs | 125.2 µs | ≈ |

- **25519 signatures: host verification takes 1.5–1.9× less time on
  nightly** (ed25519-dalek 29.4 → 15.6 µs = 1.9×, ed25519-zebra 24.6 →
  15.7 µs = 1.6×; the corresponding PVM ratios worsen from 2.3–2.6× to
  3.7–4.4×). Dalek's larger factor is partly its own handicapped stable
  baseline (missing precomputed tables — see `x86-sig-investigation/`);
  under nightly all three crates converge at ~15.6–16.4 µs. `curve25519-dalek` compiles its SIMD backends only
  on the nightly *channel*
  ([unstable-feature gate](https://github.com/dalek-cryptography/curve25519-dalek/blob/curve25519-4.1.3/curve25519-dalek/src/lib.rs#L13-L24),
  enabled by [channel detection in build.rs](https://github.com/dalek-cryptography/curve25519-dalek/blob/curve25519-4.1.3/curve25519-dalek/build.rs#L44-L50))
  and runtime-dispatches to AVX-512-IFMA on Zen 4+; `target-cpu` flags are
  irrelevant, and no stable release changes this (1.97 tested) until the
  crate drops its gate.
- **keccak host-native: ~2.3× slower on nightly** (1.58 → 3.67 ms): modern
  LLVM auto-vectorizes the permutation to AVX-512 under `target-cpu=native`
  on Zen 4; stable 1.86 does not. Best host flips to portable. Likely an
  LLVM-version behavior, so future stable releases may inherit it.
- **twox host-native: +11–17% slower on nightly** at ≥512 B (worse znver4
  scalar codegen); twox_128 *portable* is 10–23% faster — pure loop-codegen
  churn, in both directions.
- **sha2: clean on modern nightly** (= stable, 414 µs); the suite's
  previously-pinned nightly-2025-05-10 had a ~2× SHA-NI register-spill
  regression, since fixed upstream.
- **blake2 (including the asm PoC comparisons) and secp256k1:
  toolchain-insensitive** (±3%).

The main tables therefore use stable-built hosts (the production
configuration), host-native baselines are revalidated per toolchain (best
host per cell, never assumed), and host numbers should always be quoted
with their toolchain.

### Hand-written assembly PoC (blake2b) — for LLM values of "hand"

A proof-of-concept to check how much of the PVM penalty is code generation
rather than the VM itself: the blake2b compression loop was hand-rolled in
RISC-V assembly (by an LLM — no human hands were harmed): a/b state rows
pinned in registers, strict 2-operand form (no recompiler `mov`s), message
schedule as constant offsets, block loop inlined. Lives in its own guest
crate (`guest-programs/bench-blake2-256-asm`);
details, the performance model, and the full analysis/verification flow are
in [blake2-riscv-asm-handoff.md](blake2-riscv-asm-handoff.md).

Results (machine B; "host" = host native, AVX2 — the best host build for
blake2). All cells from the same representative run as the machine-B
tables above:

| size | PVM `blake2b_simd` | vs host | PVM asm | vs host | asm vs simd |
|---:|---:|---:|---:|---:|---:|
| 32 B | 124 ns | 1.45× | **114 ns** | **1.33×** | 0.92× |
| 128 B | 121 ns | 1.44× | **118 ns** | **1.40×** | 0.97× |
| 512 B | 451 ns | 1.39× | **404 ns** | **1.24×** | 0.90× |
| 4 KiB | 3.54 µs | 1.38× | **3.06 µs** | **1.19×** | 0.87× |
| 64 KiB | 56.38 µs | 1.38× | **48.64 µs** | **1.19×** | 0.86× |
| 1 MiB | 904.74 µs | 1.38× | **773.26 µs** | **1.18×** | 0.85× |

- Takeaway: roughly half of blake2's PVM gap was LLVM's code generation for
  the 13-register target (504 → 304 PVM instructions per 2 rounds); the
  rest (1.06× vs the *scalar* host-portable build at bulk sizes, not shown
  above) is structural (state doesn't fit in registers, the dependency
  chain crosses memory) and is the floor for this ISA. Small sizes keep
  ~15 ns of fixed per-call setup, hence the higher 32–128 B ratios.
- The ~40% instruction-count cut also lowers metered gas by a similar
  amount regardless of wall-clock (gas is charged per instruction executed,
  summed per basic block).
- This is a per-primitive ceiling check, not a proposal to hand-write
  production crypto; it shows guest-code blake2 gets within 1.2–1.4× of a
  production host at every size, strengthening the guest-code path.
- The asm code is **not production ready**: benchmark-grade only — no
  fuzzing, no review. 

## Host-call overhead — measured

A PVF runs in a **nested PVM** (`machine`/`invoke`); its host calls cannot
reach the node. Each one returns control from `invoke` to the service, which
serves it with host calls of its own:

```
1. PVF: ecalli N          inner VM exits; invoke() returns HOST + N to the service
2.   peek(handle, …)      copy the arguments out of inner memory
3.   ecalli N (node)      the service performs the call natively
4.   poke(handle, …)      copy the result back into inner memory
5.   invoke(handle, …)    resume the inner VM
```

Measured on machine B with a dedicated harness — `polkavm/tools/nested-call`,
branch `mku-nested-call`; **full write-up, decomposition and diagrams in
[nested-call-overhead-results.md](nested-call-overhead-results.md)**.

**Method.** No crypto in the loop: the node-side stub reads an `N`-byte
argument out of guest memory, folds it to a `u64` and writes the result back,
so what is measured is the mediation and nothing else. Two guest programs play
the parachain-code and parachain-service roles, in the node's own
configuration (recompiler, Linux sandbox, sync gas both VMs, inner VM with
dynamic paging as `NestedEngine::spawn` creates it). Per-call figures are the
slope of a calls-per-run sweep, which cancels per-run entry cost; medians of 3
interleaved rounds, measured noise floor ±0.75%. End-to-end checksums verify
every mediated byte.

| who makes the call | service↔node crossings | µs of overhead |
|---|---:|---:|
| **parachain code** (nested, arguments packed) | 4 | **6.0** |
| parachain code, one `peek` per argument | 6 | 11.4 |
| **the service's own refine code** | 1 | **1.9** |

- **Crossings are cheap; reaching guest memory is not.** A crossing costs
  ~0.11 µs (spin resume, no syscall). The cost is the **8 guest-memory
  accesses** a mediated call needs — on a VM without dynamic paging each is a
  `process_vm_readv`/`writev` syscall into the worker process, **~0.87 µs
  regardless of length**. Six of the eight hit *service* memory, which the node
  never runs with dynamic paging. The old "n × crossing" model had the
  dominant term wrong.
- Length is nearly free to ~1 KiB, then copy-bound: **+0.215 ns/B** mediated,
  +0.104 ns/B for the service's own call (the mediated route moves each byte
  twice). At 64 KiB: 21.8 µs and 8.7 µs.
- Entering a nested VM costs ~2.7 µs once per `invoke` sequence and **nothing
  per instruction** — 234 µs of pure compute costs the same flat or nested.

**A host call therefore pays off iff `PVM_time − host_time` exceeds ~6.0 µs +
0.215 ns/B (from parachain code) or ~1.9 µs + 0.104 ns/B (from the service).**
Applying that to the tables above:

- **25519 signatures: host call wins.** In-guest is 38.58 µs after the
  wide-arithmetic work ([wide-arith-results.md](wide-arith-results.md)) against
  a host verify of 23.4 µs native / 24.7 µs portable — a 14–15 µs delta against
  6.0 µs of overhead, so 29.4 µs (30.7 portable) vs 38.58 µs, **20–24% better
  even from parachain code**. On IFMA hardware the host side is ~16 µs (nightly
  `curve25519-dalek`, see *Toolchain impact on host baselines*), widening it to
  22 µs vs 38.58 µs.
- **Hashing: guest code wins at every size.** blake2b's whole PVM-vs-host gap
  is 1.19–1.45×, which at 64 KiB is 7.8 µs — less than the 21.8 µs a mediated
  call costs at that length, and less than the 8.7 µs the service's own call
  costs. At 32 B the gap is 28 ns against 1.9 µs. A hashing host call cannot
  pay for its own copies.
- Not measured, and the obvious escape hatch if a case ever falls the wrong
  way: **batching** N operations behind one crossing.

## Build tweaks

- **Linker-provided `memcpy`/`memset`** (`builtins-mem` feature, default
  for the hash benchmarks): the suite's shared `bench-common.rs` defines naive
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
branch (not yet upstreamed): one guest crate per hash function
(`guest-programs/bench-<algo>`), driven by the generic `benchmark` harness
across all discovered artifacts (PVM blob, host-portable `.so`,
`*_native.so` variants). The benchmarks export an optional
`benchmark_set_size(size)` alongside the standard `initialize`/`run`; the
harness's `--size 32,512,…` option sweeps them over input sizes (benchmarks
without the export simply run unparameterized), and sized rows carry the
size as an extra output segment: `runtime/blake2-256/native/512: …`.

Methodology: `run()` hashes a pre-filled buffer with the digest chained back
into the input (prevents hoisting/DCE, measures latency — matching trie-style
chained hashing), looping `max(1, 64 KiB / size)` hashes per call so the
harness's per-call overhead is amortized at small sizes;
`process-hash-results.sh` divides by the same factor to report per-hash times
(per-row ratios are unaffected).

```
cd guest-programs && ./build-benchmarks.sh && ./build-hash-native.sh
cd ../tools/benchtool
./run-hash-benches > hash.txt
./process-hash-results.sh hash.txt > hash-table.md
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

Raw data for both machines: `tools/benchtool/output_new_00` (machine A),
`tools/benchtool/output_new_00_toaster` (machine B) and
`tools/benchtool/output_new_nightly_20260801_00_toaster` (machine B, nightly
host builds) and `tools/benchtool/output_new_01_toaster` (machine B, stable
host builds incl. signature `*-native` variants) on the `mku-bench-hash` branch (5 runs each; tables use one
representative run). Host libraries are stable-1.86 builds unless the
heading says otherwise.

ec-utils raw data (single run each): `tools/benchtool/ec-utils-06-full-machine-a.md`
(machine A) and `all-results/ec-utils-06-full.md` (machine B).

### Signatures, host-portable build — machine A
CPU: AMD Ryzen 9 5950X (Zen 3): AVX2, SHA-NI.

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
| ed25519 | 29.38 µs | 67.58 µs | 2.30× |
| ed25519-zebra | 24.57 µs | 63.21 µs | 2.57× |
| sr25519 | 24.93 µs | 63.28 µs | 2.54× |
| ecdsa-k256 | 61.39 µs | 170.54 µs | 2.78× |
| ecdsa-libsecp | 112.17 µs | 161.47 µs | 1.44× |
| recover-k256 | 123.84 µs | 339.04 µs | 2.74× |
| recover-libsecp | 119.27 µs | 171.24 µs | 1.44× |

### Signatures, host-native build — machine A

From an earlier run with the pre-refactor (nightly) host toolchain —
indicative only; `target-cpu=native` gains little for this scalar code.

| benchmark | host native | PVM (64-bit, sync gas) | ratio |
|---|---:|---:|---:|
| ed25519 | 29.01 µs | 100.27 µs | 3.46× |
| ed25519-zebra | 28.38 µs | 95.07 µs | 3.35× |
| sr25519 | 29.45 µs | 98.39 µs | 3.34× |
| ecdsa-k256 | 69.67 µs | 225.51 µs | 3.24× |
| ecdsa-libsecp | 129.20 µs | 241.51 µs | 1.87× |
| recover-k256 | 140.69 µs | 453.49 µs | 3.22× |
| recover-libsecp | 145.47 µs | 252.30 µs | 1.73× |

### Signatures, host-native build — machine B

Stable 1.86 host builds. For the 25519 family native ≈ portable (the crates
runtime-dispatch their backend, so `target-cpu` flags change little — see
`x86-sig-investigation/`); the portable/PVM cells of this run agree with the
host-portable table above within ±1%.

| benchmark | host native | PVM (64-bit, sync gas) | ratio |
|---|---:|---:|---:|
| ed25519 | 24.47 µs | 68.05 µs | 2.78× |
| ed25519-zebra | 23.04 µs | 63.89 µs | 2.77× |
| sr25519 | 25.76 µs | 63.72 µs | 2.47× |
| ecdsa-k256 | 59.58 µs | 169.73 µs | 2.85× |
| ecdsa-libsecp | 116.27 µs | 161.06 µs | 1.39× |
| recover-k256 | 119.81 µs | 339.07 µs | 2.83× |
| recover-libsecp | 124.30 µs | 170.49 µs | 1.37× |

### Signatures, nightly host builds — machine B

Host libraries built with `nightly-2026-08-01` (see *Toolchain impact on
host baselines*): curve25519-dalek's runtime-dispatched IFMA backend makes
the 25519 rows 1.5–1.9× faster than any stable build; portable ≈ native
because the dispatch is at runtime.

| benchmark | host portable | host native | PVM (64-bit, sync gas) | pvm/portable | pvm/native |
|---|---:|---:|---:|---:|---:|
| ed25519 | 15.58 µs | 15.57 µs | 68.67 µs | 4.41× | 4.41× |
| ed25519-zebra | 15.71 µs | 15.76 µs | 64.40 µs | 4.10× | 4.09× |
| sr25519 | 16.37 µs | 17.22 µs | 63.57 µs | 3.88× | 3.69× |
| ecdsa-k256 | 61.55 µs | 61.63 µs | 169.63 µs | 2.76× | 2.75× |
| ecdsa-libsecp | 111.21 µs | 116.52 µs | 162.10 µs | 1.46× | 1.39× |
| recover-k256 | 124.00 µs | 124.78 µs | 339.19 µs | 2.74× | 2.72× |
| recover-libsecp | 117.83 µs | 125.17 µs | 171.02 µs | 1.45× | 1.37× |

### Hashing — machine A

*pvm* = PVM guest blob (recompiler, sync gas) · *native* = host native
build (`-C target-cpu=native`, build machine only) · *portable* = host
portable build (runs on any x86-64; crates may runtime-dispatch, e.g.
sha2 uses SHA-NI where available)

#### blake2_128

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 279 ns | 96 ns | 108 ns | 2.90x | 2.59x | 0.89x |
| 128 B | 260 ns | 94 ns | 105 ns | 2.78x | 2.47x | 0.89x |
| 512 B | 866 ns | 354 ns | 409 ns | 2.45x | 2.12x | 0.87x |
| 4 KiB | 6.42 µs | 2.79 µs | 3.21 µs | 2.30x | 2.00x | 0.87x |
| 64 KiB | 100.42 µs | 45.17 µs | 51.19 µs | 2.22x | 1.96x | 0.88x |
| 1 MiB | 1.64 ms | 740.21 µs | 814.50 µs | 2.22x | 2.01x | 0.91x |

#### blake2_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 287 ns | 101 ns | 116 ns | 2.84x | 2.48x | 0.87x |
| 128 B | 267 ns | 99 ns | 112 ns | 2.70x | 2.38x | 0.88x |
| 512 B | 913 ns | 376 ns | 427 ns | 2.43x | 2.14x | 0.88x |
| 4 KiB | 6.68 µs | 2.95 µs | 3.36 µs | 2.26x | 1.99x | 0.88x |
| 64 KiB | 105.39 µs | 46.91 µs | 53.83 µs | 2.25x | 1.96x | 0.87x |
| 1 MiB | 1.71 ms | 753.31 µs | 858.94 µs | 2.27x | 1.99x | 0.88x |

#### keccak_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 506 ns | 252 ns | 315 ns | 2.00x | 1.61x | 0.80x |
| 128 B | 491 ns | 252 ns | 305 ns | 1.94x | 1.61x | 0.83x |
| 512 B | 1.49 µs | 937 ns | 1.15 µs | 1.60x | 1.30x | 0.82x |
| 4 KiB | 10.46 µs | 7.12 µs | 8.77 µs | 1.47x | 1.19x | 0.81x |
| 64 KiB | 160.59 µs | 110.27 µs | 135.48 µs | 1.46x | 1.19x | 0.81x |
| 1 MiB | 2.58 ms | 1.77 ms | 2.18 ms | 1.46x | 1.18x | 0.81x |

#### keccak_512

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 485 ns | 251 ns | 307 ns | 1.93x | 1.58x | 0.82x |
| 128 B | 808 ns | 474 ns | 586 ns | 1.71x | 1.38x | 0.81x |
| 512 B | 2.79 µs | 1.84 µs | 2.31 µs | 1.52x | 1.21x | 0.80x |
| 4 KiB | 18.86 µs | 12.99 µs | 16.23 µs | 1.45x | 1.16x | 0.80x |
| 64 KiB | 299.37 µs | 207.36 µs | 259.29 µs | 1.44x | 1.15x | 0.80x |
| 1 MiB | 4.82 ms | 3.32 ms | 4.15 ms | 1.45x | 1.16x | 0.80x |

#### sha2_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 435 ns | 51 ns | 49 ns | 8.53x | 8.80x | 1.03x |
| 128 B | 1.04 µs | 115 ns | 105 ns | 8.99x | 9.88x | 1.10x |
| 512 B | 2.65 µs | 288 ns | 277 ns | 9.19x | 9.56x | 1.04x |
| 4 KiB | 18.01 µs | 1.91 µs | 1.89 µs | 9.42x | 9.53x | 1.01x |
| 64 KiB | 279.92 µs | 29.54 µs | 29.53 µs | 9.48x | 9.48x | 1.00x |
| 1 MiB | 4.46 ms | 472.38 µs | 470.42 µs | 9.44x | 9.48x | 1.00x |

#### twox_64

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 36 ns | 19 ns | 11 ns | 1.89x | 3.20x | 1.69x |
| 128 B | 42 ns | 27 ns | 16 ns | 1.53x | 2.65x | 1.73x |
| 512 B | 75 ns | 60 ns | 37 ns | 1.24x | 2.01x | 1.62x |
| 4 KiB | 428 ns | 373 ns | 237 ns | 1.15x | 1.81x | 1.57x |
| 64 KiB | 6.45 µs | 5.64 µs | 3.62 µs | 1.14x | 1.78x | 1.56x |
| 1 MiB | 90.32 µs | 90.92 µs | 58.59 µs | 0.99x | 1.54x | 1.55x |

#### twox_128

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 50 ns | 27 ns | 27 ns | 1.83x | 1.85x | 1.01x |
| 128 B | 61 ns | 37 ns | 36 ns | 1.64x | 1.67x | 1.02x |
| 512 B | 125 ns | 106 ns | 87 ns | 1.18x | 1.44x | 1.22x |
| 4 KiB | 758 ns | 732 ns | 557 ns | 1.04x | 1.36x | 1.31x |
| 64 KiB | 11.62 µs | 11.42 µs | 8.48 µs | 1.02x | 1.37x | 1.35x |
| 1 MiB | 180.28 µs | 182.21 µs | 118.87 µs | 0.99x | 1.52x | 1.53x |

#### twox_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 83 ns | 46 ns | 42 ns | 1.82x | 1.97x | 1.08x |
| 128 B | 98 ns | 68 ns | 61 ns | 1.45x | 1.61x | 1.11x |
| 512 B | 227 ns | 206 ns | 147 ns | 1.10x | 1.54x | 1.40x |
| 4 KiB | 1.50 µs | 1.61 µs | 948 ns | 0.93x | 1.58x | 1.70x |
| 64 KiB | 23.74 µs | 25.80 µs | 14.81 µs | 0.92x | 1.60x | 1.74x |
| 1 MiB | 361.99 µs | 410.52 µs | 234.91 µs | 0.88x | 1.54x | 1.75x |

### Hashing — machine B

#### blake2_128

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 124 ns | 86 ns | 96 ns | 1.45x | 1.30x | 0.90x |
| 128 B | 121 ns | 84 ns | 93 ns | 1.44x | 1.30x | 0.90x |
| 512 B | 451 ns | 324 ns | 358 ns | 1.39x | 1.26x | 0.90x |
| 4 KiB | 3.54 µs | 2.56 µs | 2.82 µs | 1.38x | 1.25x | 0.91x |
| 64 KiB | 56.47 µs | 41.11 µs | 45.16 µs | 1.37x | 1.25x | 0.91x |
| 1 MiB | 903.47 µs | 655.29 µs | 721.39 µs | 1.38x | 1.25x | 0.91x |

#### blake2_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 124 ns | 86 ns | 97 ns | 1.45x | 1.28x | 0.88x |
| 128 B | 121 ns | 84 ns | 95 ns | 1.44x | 1.27x | 0.89x |
| 512 B | 451 ns | 325 ns | 362 ns | 1.39x | 1.25x | 0.90x |
| 4 KiB | 3.54 µs | 2.57 µs | 2.85 µs | 1.38x | 1.24x | 0.90x |
| 64 KiB | 56.38 µs | 40.94 µs | 45.47 µs | 1.38x | 1.24x | 0.90x |
| 1 MiB | 904.74 µs | 655.63 µs | 726.31 µs | 1.38x | 1.25x | 0.90x |

#### keccak_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 294 ns | 217 ns | 265 ns | 1.36x | 1.11x | 0.82x |
| 128 B | 293 ns | 219 ns | 256 ns | 1.34x | 1.14x | 0.85x |
| 512 B | 1.03 µs | 812 ns | 980 ns | 1.27x | 1.05x | 0.83x |
| 4 KiB | 7.68 µs | 6.25 µs | 7.46 µs | 1.23x | 1.03x | 0.84x |
| 64 KiB | 118.64 µs | 97.00 µs | 115.48 µs | 1.22x | 1.03x | 0.84x |
| 1 MiB | 1.89 ms | 1.58 ms | 1.85 ms | 1.20x | 1.02x | 0.85x |

#### keccak_512

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 285 ns | 211 ns | 269 ns | 1.35x | 1.06x | 0.79x |
| 128 B | 523 ns | 406 ns | 510 ns | 1.29x | 1.03x | 0.80x |
| 512 B | 1.98 µs | 1.60 µs | 1.97 µs | 1.24x | 1.00x | 0.81x |
| 4 KiB | 13.87 µs | 11.33 µs | 13.91 µs | 1.22x | 1.00x | 0.81x |
| 64 KiB | 220.40 µs | 180.65 µs | 222.14 µs | 1.22x | 0.99x | 0.81x |
| 1 MiB | 3.52 ms | 2.89 ms | 3.55 ms | 1.22x | 0.99x | 0.81x |

#### sha2_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 249 ns | 44 ns | 43 ns | 5.64x | 5.76x | 1.02x |
| 128 B | 723 ns | 100 ns | 91 ns | 7.25x | 7.94x | 1.10x |
| 512 B | 1.86 µs | 255 ns | 243 ns | 7.29x | 7.64x | 1.05x |
| 4 KiB | 11.87 µs | 1.68 µs | 1.66 µs | 7.09x | 7.16x | 1.01x |
| 64 KiB | 183.47 µs | 26.01 µs | 26.05 µs | 7.05x | 7.04x | 1.00x |
| 1 MiB | 2.92 ms | 414.46 µs | 414.67 µs | 7.04x | 7.04x | 1.00x |

#### twox_64

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 19 ns | 16 ns | 10 ns | 1.18x | 1.86x | 1.57x |
| 128 B | 24 ns | 19 ns | 13 ns | 1.27x | 1.86x | 1.47x |
| 512 B | 55 ns | 33 ns | 30 ns | 1.64x | 1.81x | 1.11x |
| 4 KiB | 333 ns | 167 ns | 208 ns | 1.99x | 1.60x | 0.81x |
| 64 KiB | 5.09 µs | 2.44 µs | 3.24 µs | 2.09x | 1.57x | 0.75x |
| 1 MiB | 78.11 µs | 39.30 µs | 51.97 µs | 1.99x | 1.50x | 0.76x |

#### twox_128

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 20 ns | 22 ns | 20 ns | 0.89x | 1.01x | 1.13x |
| 128 B | 34 ns | 27 ns | 30 ns | 1.28x | 1.15x | 0.90x |
| 512 B | 92 ns | 42 ns | 75 ns | 2.21x | 1.23x | 0.56x |
| 4 KiB | 636 ns | 307 ns | 517 ns | 2.07x | 1.23x | 0.59x |
| 64 KiB | 9.97 µs | 4.96 µs | 8.37 µs | 2.01x | 1.19x | 0.59x |
| 1 MiB | 154.49 µs | 77.94 µs | 117.86 µs | 1.98x | 1.31x | 0.66x |

#### twox_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 35 ns | 35 ns | 31 ns | 1.01x | 1.14x | 1.13x |
| 128 B | 64 ns | 43 ns | 48 ns | 1.50x | 1.33x | 0.88x |
| 512 B | 178 ns | 74 ns | 124 ns | 2.42x | 1.43x | 0.59x |
| 4 KiB | 1.25 µs | 586 ns | 828 ns | 2.13x | 1.51x | 0.71x |
| 64 KiB | 19.52 µs | 9.68 µs | 12.87 µs | 2.02x | 1.52x | 0.75x |
| 1 MiB | 308.43 µs | 155.65 µs | 205.52 µs | 1.98x | 1.50x | 0.76x |
### Hashing — machine B, nightly host builds

Host libraries built with `nightly-2026-08-01`; the pvm column is the same
guest blob as above (guests always build with the suite's pinned
toolchain). Note keccak's native column (~2.3× worse than stable-built —
AVX-512 auto-vectorization) and the twox native columns (+11–17%).

#### blake2_128

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 123 ns | 86 ns | 93 ns | 1.44x | 1.33x | 0.92x |
| 128 B | 121 ns | 84 ns | 91 ns | 1.43x | 1.33x | 0.93x |
| 512 B | 452 ns | 325 ns | 356 ns | 1.39x | 1.27x | 0.91x |
| 4 KiB | 3.54 µs | 2.57 µs | 2.82 µs | 1.38x | 1.26x | 0.91x |
| 64 KiB | 56.39 µs | 40.97 µs | 45.14 µs | 1.38x | 1.25x | 0.91x |
| 1 MiB | 903.32 µs | 654.10 µs | 722.57 µs | 1.38x | 1.25x | 0.91x |

#### blake2_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 124 ns | 85 ns | 95 ns | 1.45x | 1.31x | 0.90x |
| 128 B | 121 ns | 84 ns | 94 ns | 1.45x | 1.29x | 0.89x |
| 512 B | 457 ns | 323 ns | 352 ns | 1.41x | 1.30x | 0.92x |
| 4 KiB | 3.54 µs | 2.57 µs | 2.79 µs | 1.38x | 1.27x | 0.92x |
| 64 KiB | 56.47 µs | 40.91 µs | 44.59 µs | 1.38x | 1.27x | 0.92x |
| 1 MiB | 902.44 µs | 653.37 µs | 730.35 µs | 1.38x | 1.24x | 0.89x |

#### keccak_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 294 ns | 492 ns | 265 ns | 0.60x | 1.11x | 1.85x |
| 128 B | 294 ns | 493 ns | 257 ns | 0.60x | 1.14x | 1.92x |
| 512 B | 1.03 µs | 1.92 µs | 983 ns | 0.54x | 1.05x | 1.96x |
| 4 KiB | 7.68 µs | 15.13 µs | 7.45 µs | 0.51x | 1.03x | 2.03x |
| 64 KiB | 118.69 µs | 229.09 µs | 115.48 µs | 0.52x | 1.03x | 1.98x |
| 1 MiB | 1.89 ms | 3.67 ms | 1.85 ms | 0.51x | 1.02x | 1.99x |

#### keccak_512

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 285 ns | 489 ns | 263 ns | 0.58x | 1.08x | 1.86x |
| 128 B | 524 ns | 960 ns | 500 ns | 0.55x | 1.05x | 1.92x |
| 512 B | 1.98 µs | 3.78 µs | 1.94 µs | 0.52x | 1.02x | 1.96x |
| 4 KiB | 13.84 µs | 26.83 µs | 13.65 µs | 0.52x | 1.01x | 1.97x |
| 64 KiB | 220.49 µs | 428.59 µs | 217.88 µs | 0.51x | 1.01x | 1.97x |
| 1 MiB | 3.52 ms | 6.85 ms | 3.48 ms | 0.51x | 1.01x | 1.97x |

#### sha2_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 249 ns | 39 ns | 43 ns | 6.37x | 5.76x | 0.90x |
| 128 B | 734 ns | 96 ns | 91 ns | 7.68x | 8.07x | 1.05x |
| 512 B | 1.86 µs | 246 ns | 243 ns | 7.54x | 7.65x | 1.01x |
| 4 KiB | 11.86 µs | 1.66 µs | 1.66 µs | 7.15x | 7.13x | 1.00x |
| 64 KiB | 183.44 µs | 26.00 µs | 26.00 µs | 7.06x | 7.06x | 1.00x |
| 1 MiB | 2.92 ms | 413.87 µs | 414.61 µs | 7.07x | 7.05x | 1.00x |

#### twox_64

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 19 ns | 16 ns | 10 ns | 1.19x | 1.88x | 1.58x |
| 128 B | 24 ns | 19 ns | 13 ns | 1.27x | 1.83x | 1.43x |
| 512 B | 54 ns | 36 ns | 39 ns | 1.52x | 1.40x | 0.92x |
| 4 KiB | 330 ns | 191 ns | 280 ns | 1.73x | 1.18x | 0.68x |
| 64 KiB | 5.12 µs | 2.84 µs | 3.25 µs | 1.80x | 1.57x | 0.87x |
| 1 MiB | 79.07 µs | 45.73 µs | 51.74 µs | 1.73x | 1.53x | 0.88x |

#### twox_128

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 20 ns | 23 ns | 19 ns | 0.88x | 1.05x | 1.19x |
| 128 B | 35 ns | 27 ns | 27 ns | 1.32x | 1.28x | 0.97x |
| 512 B | 93 ns | 46 ns | 63 ns | 2.00x | 1.46x | 0.73x |
| 4 KiB | 632 ns | 357 ns | 419 ns | 1.77x | 1.51x | 0.85x |
| 64 KiB | 9.98 µs | 5.67 µs | 6.43 µs | 1.76x | 1.55x | 0.88x |
| 1 MiB | 154.23 µs | 91.25 µs | 106.25 µs | 1.69x | 1.45x | 0.86x |

#### twox_256

| size | pvm | native | portable | pvm/native | pvm/portable | native/portable |
|---:|---:|---:|---:|---:|---:|---:|
| 32 B | 35 ns | 34 ns | 31 ns | 1.05x | 1.15x | 1.09x |
| 128 B | 64 ns | 41 ns | 50 ns | 1.56x | 1.28x | 0.82x |
| 512 B | 178 ns | 82 ns | 124 ns | 2.17x | 1.43x | 0.66x |
| 4 KiB | 1.24 µs | 681 ns | 825 ns | 1.82x | 1.51x | 0.83x |
| 64 KiB | 19.51 µs | 11.25 µs | 12.88 µs | 1.73x | 1.51x | 0.87x |
| 1 MiB | 307.89 µs | 181.08 µs | 205.81 µs | 1.70x | 1.50x | 0.88x |

### Elliptic curves (ec-utils) — machine A
CPU: AMD Ryzen 9 5950X (Zen 3): AVX2, SHA-NI. Single run.
MSM rows carry the number of bases in brackets. *host native* here is
`target-cpu=native` **+ `ark-ff/asm`**.

| benchmark | host portable | host native | PVM (64-bit, sync gas) | pvm/portable | pvm/native |
|---|---:|---:|---:|---:|---:|
| bls381-pairing | 960.60 µs | 858.72 µs | 2.70 ms | 2.81× | 3.15× |
| bls381-msm-g1 (16) | 848.71 µs | 786.69 µs | 2.06 ms | 2.43× | 2.62× |
| bls381-msm-g1 (64) | 2.44 ms | 2.29 ms | 5.85 ms | 2.40× | 2.55× |
| bls381-msm-g1 (256) | 6.53 ms | 6.15 ms | 15.46 ms | 2.37× | 2.51× |
| bls381-msm-g1 (1024) | 19.36 ms | 18.22 ms | 45.30 ms | 2.34× | 2.49× |
| bls381-msm-g2 (16) | 2.89 ms | 2.67 ms | 7.48 ms | 2.59× | 2.80× |
| bls381-msm-g2 (64) | 8.30 ms | 7.79 ms | 21.35 ms | 2.57× | 2.74× |
| bls381-msm-g2 (256) | 22.18 ms | 20.49 ms | 56.64 ms | 2.55× | 2.76× |
| bls381-msm-g2 (1024) | 65.28 ms | 59.69 ms | 166.83 ms | 2.56× | 2.79× |
| bls381-mul-g1 | 143.52 µs | 129.20 µs | 322.79 µs | 2.25× | 2.50× |
| bls381-mul-g2 | 444.89 µs | 399.68 µs | 1.19 ms | 2.67× | 2.98× |
| bander-msm (16) | 568.10 µs | 566.63 µs | 1.29 ms | 2.27× | 2.28× |
| bander-msm (64) | 1.75 ms | 1.72 ms | 3.91 ms | 2.23× | 2.28× |
| bander-msm (256) | 4.02 ms | 3.94 ms | 8.90 ms | 2.21× | 2.26× |
| bander-msm (1024) | 10.77 ms | 10.78 ms | 23.31 ms | 2.16× | 2.16× |
| bander-mul | 67.71 µs | 63.72 µs | 154.66 µs | 2.28× | 2.43× |
| pallas-msm (16) | 424.46 µs | 375.76 µs | 914.68 µs | 2.15× | 2.43× |
| pallas-msm (64) | 1.32 ms | 1.18 ms | 2.58 ms | 1.95× | 2.19× |
| pallas-msm (256) | 3.64 ms | 3.25 ms | 7.02 ms | 1.93× | 2.16× |
| pallas-msm (1024) | 10.88 ms | 9.79 ms | 20.30 ms | 1.87× | 2.07× |
| pallas-mul | 70.40 µs | 62.57 µs | 144.46 µs | 2.05× | 2.31× |
| vesta-msm (16) | 431.13 µs | 373.68 µs | 924.80 µs | 2.15× | 2.47× |
| vesta-msm (64) | 1.31 ms | 1.18 ms | 2.61 ms | 2.00× | 2.21× |
| vesta-msm (256) | 3.60 ms | 3.26 ms | 6.80 ms | 1.89× | 2.08× |
| vesta-msm (1024) | 10.78 ms | 9.74 ms | 19.91 ms | 1.85× | 2.04× |
| vesta-mul | 70.08 µs | 62.57 µs | 143.49 µs | 2.05× | 2.29× |
| bls377-pairing *(not RFC-163)* | 1.10 ms | 936.64 µs | 3.04 ms | 2.76× | 3.24× |
| bw6761-pairing *(not RFC-163)* | 3.85 ms | 3.32 ms | 14.87 ms | 3.86× | 4.48× |

### Elliptic curves (ec-utils) — machine B
Single run.

| benchmark | host portable | host native | PVM (64-bit, sync gas) | pvm/portable | pvm/native |
|---|---:|---:|---:|---:|---:|
| bls381-pairing | 797.01 µs | 706.78 µs | 1.68 ms | 2.11× | 2.38× |
| bls381-msm-g1 (16) | 713.59 µs | 678.18 µs | 1.29 ms | 1.81× | 1.90× |
| bls381-msm-g1 (64) | 2.10 ms | 1.97 ms | 3.75 ms | 1.79× | 1.91× |
| bls381-msm-g1 (256) | 5.70 ms | 5.43 ms | 10.06 ms | 1.76× | 1.85× |
| bls381-msm-g1 (1024) | 16.93 ms | 16.10 ms | 29.41 ms | 1.74× | 1.83× |
| bls381-msm-g2 (16) | 2.47 ms | 2.29 ms | 4.60 ms | 1.86× | 2.00× |
| bls381-msm-g2 (64) | 7.10 ms | 6.76 ms | 13.16 ms | 1.85× | 1.95× |
| bls381-msm-g2 (256) | 18.97 ms | 18.01 ms | 34.79 ms | 1.83× | 1.93× |
| bls381-msm-g2 (1024) | 55.77 ms | 52.84 ms | 101.40 ms | 1.82× | 1.92× |
| bls381-mul-g1 | 124.42 µs | 117.73 µs | 213.73 µs | 1.72× | 1.82× |
| bls381-mul-g2 | 379.49 µs | 349.95 µs | 708.19 µs | 1.87× | 2.02× |
| bander-msm (16) | 478.08 µs | 477.65 µs | 919.49 µs | 1.92× | 1.93× |
| bander-msm (64) | 1.47 ms | 1.48 ms | 2.73 ms | 1.86× | 1.84× |
| bander-msm (256) | 3.54 ms | 3.52 ms | 6.52 ms | 1.84× | 1.85× |
| bander-msm (1024) | 9.47 ms | 9.40 ms | 17.17 ms | 1.81× | 1.83× |
| bander-mul | 57.64 µs | 55.52 µs | 107.59 µs | 1.87× | 1.94× |
| pallas-msm (16) | 359.67 µs | 340.87 µs | 603.28 µs | 1.68× | 1.77× |
| pallas-msm (64) | 1.02 ms | 950.88 µs | 1.74 ms | 1.70× | 1.83× |
| pallas-msm (256) | 3.11 ms | 2.90 ms | 4.94 ms | 1.59× | 1.70× |
| pallas-msm (1024) | 9.39 ms | 8.74 ms | 14.46 ms | 1.54× | 1.65× |
| pallas-mul | 60.95 µs | 56.17 µs | 97.30 µs | 1.60× | 1.73× |
| vesta-msm (16) | 360.01 µs | 335.60 µs | 601.78 µs | 1.67× | 1.79× |
| vesta-msm (64) | 1.02 ms | 944.35 µs | 1.73 ms | 1.69× | 1.83× |
| vesta-msm (256) | 3.12 ms | 2.90 ms | 4.94 ms | 1.59× | 1.70× |
| vesta-msm (1024) | 9.40 ms | 8.73 ms | 14.48 ms | 1.54× | 1.66× |
| vesta-mul | 60.54 µs | 56.14 µs | 93.18 µs | 1.54× | 1.66× |
| bls377-pairing *(not RFC-163)* | 933.31 µs | 746.25 µs | 1.94 ms | 2.08× | 2.60× |
| bw6761-pairing *(not RFC-163)* | 3.09 ms | 2.74 ms | 10.82 ms | 3.50× | 3.95× |

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
