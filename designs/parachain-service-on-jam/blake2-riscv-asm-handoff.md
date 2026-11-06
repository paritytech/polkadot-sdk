# Hand-written RISC-V blake2b for PVM — hand-off

Companion to [crypto-benchamarking.md](crypto-benchamarking.md). Documents the
experiment that closed most of the PVM-vs-host gap for blake2b-256 by
hand-writing the compression function in RISC-V assembly, the performance
model behind it, and the exact tooling/CLI flow so the analysis can be
reproduced or extended to other algorithms.

Everything below was measured on machine B (TR PRO 7995WX, Zen 4); the
assembly itself is machine-independent.

## What was made

- `polkavm/guest-programs/bench-hash/src/blake2b_compress.S` — generated,
  fully unrolled 12-round blake2b compression **loop** (2 036 instructions;
  ~1 889 per block), processing N blocks per call.
- `polkavm/guest-programs/bench-hash/gen_blake2b_asm.py` — the generator.
  Edit this, not the `.S`. Rerun: `python3 gen_blake2b_asm.py`.
- `polkavm/guest-programs/bench-hash/src/asm_blake2b.rs` — Rust driver +
  `global_asm!` include. riscv64-only; other targets (host `.so`s, riscv32)
  fall back to `compact_blake2b.rs` so the export exists everywhere.
- New export `benchmark_blake2_256_asm` in `bench-hash`; checksums must equal
  `blake2_256`'s.
- `benchtool bench-hash` gained `--aslr` (run without the ASLR-disabling
  re-exec, e.g. inside containers, with `POLKAVM_BACKEND=interpreter`).

### Results (1 MiB, machine B)

| build | PVM time | vs host portable | vs host native (AVX2) |
|---|---:|---:|---:|
| Rust (`blake2b_simd` portable) | 885 µs | 1.25x | 1.35x |
| asm, per-block function | 810 µs | 1.15x | 1.23x |
| asm, inline block loop | **772 µs** | **1.09x** | **1.17x** |

- 32 B (single block, setup-dominated): 111 ns vs 96 ns host = 1.16x.
- PVM instructions per 2 rounds: **504 (LLVM) → 304 (asm)** — under gas
  metering the asm build is ~40% cheaper regardless of wall-clock.
- Interpreter: 63.3 → 32.7 ms/MiB.
- Caveat when reading `process-results.sh` tables: for the `blake2_256_asm`
  row the host columns are the *compact fallback*, not real host blake2.
  Always compare asm-PVM against the `blake2_256` row's host cells.
- "Host portable" is scalar only because the bench builds `no_std`
  (`default-features = false` drops `blake2b_simd`'s `std` feature and with
  it runtime CPU dispatch). A production host (`sp_core::hashing`, std)
  runtime-dispatches to AVX2, so **the production-realistic ratio is 1.17x**
  (vs the native column). Verified: `libbench_hash_native.so` contains 422
  `vpaddq` (AVX2 path), `libbench_hash.so` zero. AVX2 buys only ~8% over
  scalar on Zen 4.

## Bottleneck model (the assumptions, and which held)

Starting point: recompiled PVM blake2b was 1.25x slower than host. Prior
side-by-side disassembly showed the recompiled loop carries ~2x the
instructions of native x86, from two sources:

1. ~92 register-copy `mov`s per 2 rounds — RISC-V is 3-operand, x86 is
   2-operand; the 1:1 recompiler inserts a `mov` whenever `dst != src1`.
2. ~161 spill loads/stores — LLVM's allocation for the 13-register rv64e
   target.

Model: on Zen 4 the recompiled loop is **issue-bound** (504 instr / 6-wide
≈ 84–93 cycles per 2 rounds) while the host is **chain-bound** (~74 cycles,
blake2's a→d→c→b dependency chain). Crossing point: get under ~444
instructions per 2 rounds and the guest becomes chain-bound too ⇒ parity.

Assembly design that gets there (304/2 rounds, 19 per G):

- Pin the state's a/b rows (`v0..v7`) in `a0..a5,s0,s1` — every G's a and b
  operands are register-resident, in columns *and* diagonals.
- Stream the c/d rows through a stack frame (`sp+0..56`) via `t1`/`t2`;
  `ra` is saved and used as the third temp (message word).
- Strict 2-operand form (`d = d op s`) ⇒ the recompiler emits zero `mov`s.
- Message schedule baked into constant load offsets off `t0` (no sigma
  table, no index arithmetic); unaligned `ld` is fine
  (`+unaligned-scalar-mem` is in the target json).
- Inline multi-block loop: `h` stays in the pinned registers across blocks
  (the fold result *is* the next block's `v0..v7`); per-block ABI/reload
  overhead eliminated. Loop-back branch spans ~7.6 KB > `bnez`'s ±4 KiB
  range ⇒ `beqz` + `j` pair.

What held: prediction 770–780 µs for the loop version; measured 772 µs.

What was learned (the residual 9%): with 13 registers, ≥6 of the 16 state
words must live in memory, and blake2's critical chain visits all four rows
per G ⇒ ~2 store-to-load forwards per round stay **on the chain** (~7–8
cycles each on Zen 4, vs ~0 in a register). llvm-mca does not model
store-forwarding (it predicted 65 cycles/2 rounds; measured ~81) — trust it
for issue-bound analysis, not for memory-chain latency. Rejected ideas, all
for the same reason (the critical path is a *max* over braided chains, and
some chain always crosses memory):

- pinning c/d instead of a/b — symmetric, same crossing count;
- pinning 3 rows — needs 15 registers, PVM has 13 (sp included);
- pinning 10 words — the unpinned words' chains still bound;
- reordering/software-pipelining Gs — a round (152 instr) fits in the
  ~320-entry ROB, the scheduler already sees everything;
- parallel chains — blake2 feeds `h` forward, blocks are sequential.

**~1.09x vs scalar host is the floor for this ISA.** The asm exercise also
decomposed the original gap: ~0.11 of the 1.25x was LLVM register
allocation + 2-operand translation (fixable, see upstream note), ~0.1 is the
structural memory-chain tax.

## Tooling: inspecting what actually executes

Three layers, three tools. The `.S` is RISC-V; polkatool-link translates it
~1:1 to PVM bytecode; the recompiler translates that ~1:1 to x86-64.

### 1. PVM bytecode — `polkatool disassemble`

```sh
cd polkavm
cargo run -q -p polkatool disassemble \
    guest-programs/target/riscv64emac-unknown-none-polkavm/release/bench-hash.polkavm > /tmp/disasm.txt
```

Find the compress code by its signature rotations (`>>r 0x20/0x18/0x10/0x3f`
per G). Round r starts at the `t1 = u64 [t0 + 8*SIGMA[r%10][0]]` load. Used
to confirm the linker kept the code 1:1 (19 instr/G, 304 per 2 rounds) and
to get the PVM byte offsets of round boundaries for step 2.

### 2. Recompiled x86-64 — the `jitdump` helper

The recompiler's output is reachable via public API:
`Module::machine_code()` + `Module::program_counter_to_machine_code_offset()`.
A ~30-line binary (not committed; recreate as needed) with
`polkavm = { path = ".../crates/polkavm", features = ["generic-sandbox"] }`:

```rust
use polkavm::{Config, Engine, Module, ModuleConfig, ProgramBlob};

fn main() {
    let mut args = std::env::args().skip(1);
    let blob = ProgramBlob::parse(std::fs::read(args.next().unwrap()).unwrap().into()).unwrap();
    let out = args.next().unwrap();
    let mut config = Config::from_env().unwrap();
    config.set_backend(Some(polkavm::BackendKind::Compiler));
    config.set_sandbox(Some(polkavm::SandboxKind::Generic)); // works unprivileged/in containers
    config.set_worker_count(0);
    let engine = Engine::new(&config).unwrap();
    let module = Module::from_blob(&engine, &ModuleConfig::default(), blob).unwrap();
    std::fs::write(&out, module.machine_code().unwrap()).unwrap();
    let map = module.program_counter_to_machine_code_offset().unwrap();
    for arg in args {
        let pvm: u32 = arg.parse().unwrap(); // PVM byte offset from step 1
        let i = map.partition_point(|&(pc, _)| pc.0 < pvm as u64);
        println!("pvm {} -> mc {}", pvm, map[i].1);
    }
}
```

```sh
POLKAVM_ALLOW_EXPERIMENTAL=1 jitdump bench-hash.polkavm mc.bin <pvm_off_round0> <pvm_off_round1> ...
objdump -D -b binary -m i386:x86-64 -M intel \
    --start-address=<mc0> --stop-address=<mc1> mc.bin | less
```

Sandbox caveat: the generic sandbox emits `lea ecx,[reg+off]` + `mov ...,[r13+rcx]`
per memory access (32-bit address wrap); the **Linux sandbox — what benchmarks
actually run — emits a single `mov` with `[reg+off]` addressing** (see
`load_store_operand!` in `crates/polkavm/src/compiler/amd64.rs`). Instruction
counts from a generic-sandbox dump overestimate by one `lea` per memory op.

### 3. Pipeline prediction — `llvm-mca`

```sh
objdump -D -b binary -m i386:x86-64 --start-address=<mc0> --stop-address=<mc1> mc.bin \
    | sed -n '8,$p' | sed 's/^[^\t]*\t[^\t]*\t//' > rounds.s
llvm-mca-14 -mcpu=znver3 -iterations=100 rounds.s | head -12
```

- llvm-mca 14 has no znver4 model; znver3 is close enough (µop-cache size
  differs: 4K vs 6.9K µops — matters for the LLVM build, not the asm one).
- Reliable for: issue-width limits, IPC, port pressure.
- Blind to: store-to-load forwarding, front-end/µop-cache effects. Its 65
  cycles/2 rounds vs measured ~81 is entirely the forwarding gap.

## Build / verify / measure flow

```sh
# 1. regenerate the .S after editing the generator
cd polkavm/guest-programs/bench-hash && python3 gen_blake2b_asm.py

# 2. build guests (riscv64 + riscv32 + host libs) and link the blobs
cd .. && ./build-benchmarks.sh && ./build-hash-native.sh
# (manual 64-bit-only equivalent:
#   RUSTFLAGS="" cargo build -Z build-std=core,alloc \
#     --target "$PWD/../crates/polkavm-linker/targets/legacy/riscv64emac-unknown-none-polkavm.json" \
#     --release --bin bench-hash -p bench-hash
#   cd .. && cargo run -q -p polkatool link \
#     guest-programs/target/riscv64emac-unknown-none-polkavm/release/bench-hash \
#     -o guest-programs/target/riscv64emac-unknown-none-polkavm/release/bench-hash.polkavm )

# 3. correctness: interpreter backend, no sandbox/ASLR privileges needed.
#    The checksum column must be identical between blake2_256 and
#    blake2_256_asm on every row. Sizes cover: sub-block, exact block,
#    block+1 (padding/counter edges), multi-block, bulk.
cd tools/benchtool
POLKAVM_BACKEND=interpreter cargo run -q --release -- bench-hash --aslr --csv \
    --sizes 1,32,127,128,129,256,4096,1048576 blake2_256 blake2_256_asm

# 4. timing: needs the compiler backend + Linux sandbox (bare metal, not a
#    restricted container); ASLR re-exec on by default
cargo run --release -- bench-hash --csv blake2_256 blake2_256_asm
./process-results.sh <csv...>

# 5. optional: confirm chain-bound with hardware counters
#    (expect IPC ~3.6 for the asm build; the issue-bound LLVM build shows ~5+)
perf stat -e cycles,instructions cargo run --release -- bench-hash \
    --sizes 1048576 blake2_256_asm
```

## Open threads

- Upstream signal (for koute): 504 → 304 instructions per 2 rounds with zero
  semantic change means LLVM's rv64e allocation + the 2-operand translation
  leave ~40% on the table for register-starved kernels. A polkavm-linker
  regalloc/copy-elision pass could recover much of this for *compiled* code;
  the earlier load-op fusion experiment (measured ~0% wall-clock on Zen 4)
  is not the mechanism — instruction *count* still matters for gas and for
  narrower cores.
- Same treatment would work for keccak (already ~1.0x, not worth it) and
  sha2 (pointless on x86 hosts: SHA-NI silicon is 7x, unreachable by any
  guest code). twox (~2x) is a plausible next candidate.
- The asm is maintained by generator; if `bench-hash`'s frame/ABI
  assumptions change (rv64e callee-saved set, sp alignment), regenerate and
  re-run step 3 — the checksum grid is the safety net.
