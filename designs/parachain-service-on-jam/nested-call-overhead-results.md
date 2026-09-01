# Nested host-call overhead — results

Companion to [nested-call-overhead-handoff.md](nested-call-overhead-handoff.md),
which owns the scope and the decisions. This is the lab notebook: the full
story, the diagrams, and the reasoning. The one-paragraph version lives in
[crypto-benchamarking.md](crypto-benchamarking.md) under *Host-call overhead*.

**Status: complete. Numbers of record, machine B, 2026-08-13.**

## Executive summary

**What one host call costs, and who pays it:**

| caller | service↔node crossings | guest-memory accesses | µs/call |
|---|---:|---:|---:|
| **parachain code** (nested, packed args) | 4 | 8 | **6.0** |
| parachain code, one `peek` per argument | 6 | 12 | 11.4 |
| **the service's own refine code** | 1 | 2 | **1.9** |

**The verdict it was built to settle — the host-call route wins for 25519:**

| route (one ed25519 verification) | wall |
|---|---:|
| in-guest, after the wide-arithmetic work, zero crossings | 38.58 µs |
| **host call from parachain code** (23.4 native host + 6.0) | **29.4 µs** |
| host call from the service's own code (23.4 + 1.9) | 25.3 µs |

The budget was **< 15 µs of overhead**; the measurement came in at **6.0 µs**,
less than half of it. The result survives every pessimisation available: even
with un-packed arguments *and* the wrong paging configuration (11.4 µs), the
total is 34.8 µs, still under the in-guest route.

**The surprise, and the reason the old model was wrong:** the cost is not the
VM boundary crossings. A crossing is ~0.11 µs. The cost is **reaching guest
memory** — 0.87 µs per access, because the guests run in separate processes and
the host reaches them with a `process_vm_readv`/`writev` syscall. A mediated
call needs eight such accesses. Six of them hit *service* memory, which the
node never runs with dynamic paging, so they cannot be optimised away by
configuration alone.

**Secondary results:**

- **Hashing must stay in guest code**, at every size and from either caller —
  blake2b's entire PVM-vs-host gap is smaller than the copies a host call
  would need to pay for. Worked through below.
- **`instantiate_nested` is worth nothing** (7738 vs 7754 ns, inside the noise
  floor) — a clean negative result that closes an open question about
  polkajam's `NestedEngine`.
- **Nesting is free per instruction, not free per entry**: 234 µs of pure
  compute costs the same flat or nested, but each `invoke` sequence costs
  ~2.7 µs to enter.

## What is being decided, and why this number decides it

In JAM, parachain code runs in an **inner PVM** that a parachain service
(itself a PVM guest) creates and runs. The inner PVM **cannot reach the node**:
every host call it makes returns control from `invoke` to the service, which is
a mandatory mediator. So a hypothetical `ed25519_verify` available to parachain
code is not one host call — it is a protocol.

The competing route is doing the crypto in guest code, which the
wide-arithmetic PoC brought to 38.58 µs per verification
([wide-arith-results.md](wide-arith-results.md)) against a 23.4 µs native host
verify. Hence the criterion:

> the host-call route wins on wall clock iff the mediation overhead at a
> 96–128 B payload is below ~15 µs.

## The protocol, and where the time goes

One mediated call. Three processes: the host (the node), and one forked worker
per VM. The two guests never touch each other — every arrow crosses the host.

```
HOST (node)                      SERVICE VM (outer)        PARACHAIN CODE (inner)
                                 ── worker B ──            ── worker C ──
                                 run(): loop {
   ◄─── ecalli invoke ─────────── invoke(handle,&args) (1)
 rd 112 B  ← service   [syscall]
 set inner regs + gas
 inner.run() ──────────────────────────────────────────►  ...executes...
   ◄────────────── ecalli stub ──────────────────────────  ed25519_verify(…) (2)
 wr 112 B  → service   [syscall]     ↑ the host only REPORTS this;
 A0 = HOST(3), A1 = index              it never serves an inner VM's host call
 resume ───────────────────────► branches on the HOST fault
   ◄─── ecalli peek ───────────── peek(h,buf,ptr,len)  (3)
 rd N B    ← inner     [syscall, or memcpy if the inner VM has dynamic paging]
 wr N B    → service   [syscall]
 resume ───────────────────────►
   ◄─── ecalli stub ───────────── ed25519_verify(…)    (4)
 rd N B    ← service   [syscall]
 the crypto happens here, natively
 wr R B    → service   [syscall]
 resume ───────────────────────►
   ◄─── ecalli poke ───────────── poke(h,res,dst,R)    (5)
 rd R B    ← service   [syscall]
 wr R B    → inner     [syscall, or memcpy if the inner VM has dynamic paging]
 resume ───────────────────────► }  loops back to invoke
 inner.run() ──────────────────────────────────────────►  resumes after its ecalli,
                                                          reads the result from dst
```

Two things in that picture are easy to get wrong:

- **The inner VM's `ecalli` is reported, not dispatched.** The host is running
  `inner.run()` from inside its implementation of the service's `invoke`, so
  the trap necessarily lands on the host — but the host turns it into
  `invoke`'s *return value* (outcome `HOST` + the index) and hands it to the
  service, which decides what to do. The node never executes an inner VM's host
  call (`polkajam/crates/node/src/chain/exec/vm/host.rs:1791`; the service side
  of the same contract in a real service:
  `crates/corevm-engine/src/co_engine.rs:213`).
- **`invoke` resumes, it does not spawn.** `machine` creates the instance once;
  every `invoke` sets gas and the 13 registers, runs the existing instance, and
  reads them back. K crypto calls need **K+1 invokes** — the first starts the
  inner VM, the last collects its `HALT`. The extra one is startup, and the
  calls-per-run regression removes it (it *is* the 2.79 µs intercept).

### Why guest-memory access costs 0.87 µs

Each VM runs in a separate process (a worker running the zygote), so "read the
service's memory" is not a memcpy unless the host happens to have that memory
mapped. In `polkavm/crates/polkavm/src/sandbox/linux.rs`:

| path | mechanism | cost |
|---|---|---:|
| no dynamic paging (the default, and what services use) | `process_vm_readv` / `process_vm_writev` — `:2304`, `:2370` | **~0.87 µs** |
| dynamic paging enabled | memcpy into the shared mmap — `:2320`, `:2385` | ~0.01 µs |
| write into the aux-data region | memcpy fast path — `:2344` | ~0.01 µs |

The syscall cost is **independent of length** at these sizes, which is why
`peek` (128 B), `poke` (8 B) and the stub (128 B in, 8 B out) all measured
within 10 ns of each other.

### Phase decomposition, and a cost model that predicts every row

Per mediated call, 128 B, K = 32 (`--phases`; instrumented, so the total is
~5% above the clean run):

| phase | count/call | ns | what it is |
|---|---:|---:|---|
| outer run (crossing + guest) | 4.06 | 445 | the 4 crossings, ~110 ns each |
| invoke | 1.03 | 2075 | 2 accesses + 13 register stores |
| — of which inner execution | 1.03 | 139 | the parachain code itself |
| peek | 1.00 | 1736 | 2 accesses |
| stub | 1.00 | 1727 | 2 accesses |
| poke | 1.00 | 1729 | 2 accesses |
| **sum** | | **7711** | vs 7754 measured unperturbed — 99.4% accounted |

That gives **~0.865 µs per guest-memory access** and **~0.11 µs per crossing**.
The model then predicts the rest of the matrix without further fitting:

| configuration | predicted | measured | error |
|---|---:|---:|---:|
| one-jump = 1 crossing + 2 accesses | 1.84 µs | 1.90 | +3% |
| per-arg = +2 peeks (+4 accesses, +2 crossings) | 11.43 µs | 11.44 | +0.1% |
| inner dynamic paging = −2 syscalls | 6.02 µs | 6.08 | +1% |

Three independent predictions inside 3%. The mechanism is not in doubt.

## Numbers of record

Machine B (Threadripper PRO 7995WX, Zen 4), bare metal, Linux sandbox,
recompiler, sync gas on both VMs, `taskset -c 2-4`. Medians of 3 interleaved
rounds with the configuration order reversed on even rounds. **Noise floor:
the unchanged 128 B packed configuration re-measured across rounds gave
7748 / 7806 / 7754 ns → ±0.75%**, matching machine B's established floor.

**Per mediated call, 128 B payload, 8 B result, K = 32:**

| configuration | µs/call |
|---|---:|
| **two-jump, node-faithful (inner VM with dynamic paging)** | **6.085** |
| two-jump, packed peek | 7.754 |
| two-jump, per-arg peeks | 11.443 |
| two-jump, `instantiate()` instead of `instantiate_nested()` | 7.738 |
| one-jump (the service calling the node itself) | 1.900 |

**Calls-per-run sweep** (two-jump, 128 B) — the decomposition that cancels
per-run entry cost:

| K | 1 | 2 | 4 | 8 | 16 | 32 | 64 |
|---|---:|---:|---:|---:|---:|---:|---:|
| ns/call | 10553 | 9096 | 8398 | 8043 | 7851 | 7764 | 7707 |

> fit: **7660 ns per mediated call + 2792 ns fixed per run**

The intercept is itself a check: one extra `invoke` (2075 ns) plus the inner
program-counter reset and the outer entry ≈ 2.8 µs. The regression and the
phase decomposition agree without being told to.

**Payload sweep, ns per call:**

| payload | 32 B | 96 B | 128 B | 1 KiB | 4 KiB | 64 KiB | slope |
|---|---:|---:|---:|---:|---:|---:|---:|
| two-jump | 7739 | 7745 | 7767 | 7836 | 8729 | 21825 | **0.2152 ns/B** |
| one-jump | 1895 | 1902 | 1901 | 1939 | 2357 | 8725 | **0.1040 ns/B** |

Flat to ~1 KiB — at signature sizes the length is free, only the access count
matters. Beyond that it is copy-bound, and the mediated slope is exactly 2× the
direct one because the protocol moves every byte one extra time.

**Nesting tax** (a fixed 234 µs pure-compute loop, flat vs as an inner VM):
+2675 ns, **+1.14%**, identical results both ways. That absolute figure is one
`invoke` round trip — a constant per entry, nothing per instruction.

## The configuration that is 1:1 with polkajam

The node does not configure both VMs the same way:

| | polkajam | the harness |
|---|---|---|
| service VM | no dynamic paging (`vm/mod.rs:302-306`) | same |
| inner machines | **dynamic paging on** (`vm/mod.rs:83`, `NestedEngine::spawn`) | `--inner-dynamic-paging` |
| gas metering | sync, both | same |
| backend | recompiler | same |

So **6.085 µs is the number of record** and 7.754 µs is the harness being
unfaithful on one axis. The difference is exactly the two inner-side accesses
turning from syscall into memcpy.

Two consequences worth noting for anyone designing a service:

- Six of the eight accesses hit *service* memory, and services do not get
  dynamic paging — so the floor for a mediated call is ~5.2 µs of syscalls no
  matter how the service is written.
- **Packing arguments is worth 3.7 µs per call** (7.75 → 11.44 when peeked
  per-argument). One `peek` of a contiguous argument block, not three.
- Writes into the **aux-data region** take the memcpy fast path even without
  dynamic paging (`linux.rs:2344`), and the node gives services a 16 MiB aux
  region. Untested here; a plausible way to shave the service-side write costs.

## Applying it: which primitives should be host calls

The rule that falls out: **a host call pays iff `PVM_time − host_time` exceeds
6.0 µs + 0.215 ns/B (from parachain code), or 1.9 µs + 0.104 ns/B (from the
service's own code).**

- **25519 signatures — host call, clearly.** In-guest 38.58 µs against a host
  verify of 23.4 µs native / 24.7 µs portable (the 1.66×/1.56× ratios in
  wide-arith-results): a 14–15 µs delta against 6.0 µs of overhead. **29.4 µs
  (30.7 portable) vs 38.58 µs — 20–24% better even from parachain code**; on
  IFMA hardware the host side is ~16 µs, widening it to 22 µs.
- **Hashing — guest code, at every size.** blake2b's whole PVM-vs-host gap is
  1.19–1.45×: at 64 KiB that is 7.8 µs, against 21.8 µs for a mediated call at
  that length (and 8.7 µs for the service's own). At 32 B the gap is 28 ns
  against 1.9 µs. **A hashing host call cannot pay for its own copies** — the
  thing that makes hashing cheap to do natively also makes it cheap to do in
  the guest, while the copies scale with the same input the hash does.

## Corrections to earlier claims in this workstream

- **"≈ 5–6 crossings per call"** (hand-off): the count is right — 4
  service↔node crossings plus the inner VM's exit and resume — but the
  inference was wrong. Crossings are 6% of the cost. The hand-off's
  `n × crossing + copy(len)` model omits the term that dominates.
- **"Nesting tax ≈ 0"**: true per instruction, false per entry. The container
  pre-screen said 671 ns; the rig says 2675 ns, which is one `invoke`. State it
  as "constant per entry, zero per instruction".
- **`instantiate_nested` as a free win for polkajam**: withdrawn. It moves
  nothing on the rig (7738 vs 7754 ns, inside ±0.75%). Its effect is worker
  core/CCX co-placement, which evidently does not matter when the cost is
  syscall-bound rather than latency-bound.

## Container pre-screen vs the rig — how badly the container lied

The dev container cannot run the Linux sandbox (`clone`, errno 38); its
fallback, the generic sandbox, runs the guest **in-process**, so it has neither
crossings nor `process_vm_*` syscalls.

| quantity | container (pre-screen) | machine B (record) | ratio |
|---|---:|---:|---:|
| two-jump, 128 B | 0.321 µs | 7.754 µs | 24× |
| one-jump, 128 B | 0.064 µs | 1.900 µs | 30× |
| per-arg vs packed | +48% | +48% | 1.0× |
| mediated byte slope | 0.0995 ns/B | 0.2152 ns/B | 2.2× |
| nesting tax | 671 ns | 2675 ns | 4× |

Absolute costs were wrong by 24–30×, and the *shape* survived: the per-arg
penalty transferred to the percent, and the crossings-are-cheap conclusion was
invisible in the container precisely because everything was cheap there. Treat
container runs as a correctness and linearity check only — the same lesson as
the store-traffic finding in [wide-arith-results.md](wide-arith-results.md).

## Reproducing

```bash
# polkavm, branch mku-nested-call
cargo build -p nested-call --release
./guest-programs/build-nested-call.sh          # 64-bit blobs

./tools/nested-call/run-matrix.sh              # --linux; 3 interleaved rounds
./tools/nested-call/run-matrix.sh --generic 3  # container pre-screen
```

Rig requirements: bare metal, quiet, **≥3 free cores** (`NESTED_CALL_CPUS`,
default `2-4`) — the host thread and both workers spin during a crossing.

Raw output of the run of record: `polkavm/all-results/host-call-benchmarks-00.txt`.

**What the harness is.** `polkavm/tools/nested-call` implements `machine`,
`peek`, `poke`, `invoke` and the node stub for the outer guest with the node's
copy semantics (two copies per peek and per poke, 112-byte invoke argument
block), mirroring `polkajam/.../host.rs` without importing it.
`guest-programs/bench-nested-caller` is the parachain-code role (K stub calls
per run, plus a pure-compute export for the nesting-tax mode);
`bench-nested-mediator` is the service role (`invoke` → `peek` → node call →
`poke` → resume). Nothing in the VM changed — no new instructions, no
semantics.

**Correctness.** Every mediated byte is verified: the guest folds each returned
result into a running value and the host recomputes the identical fold
independently, at every payload size, both peek strategies, both result sizes
and both modes — 96 configurations in `--selftest`, all passing. Packed and
per-arg peeks must produce the same checksum, which is what proves the
three-peek path reassembles the argument correctly.

**Deliberate deviations, and why they are free.** `machine` ignores the code
pointer and instantiates the blob given on the command line (spawning happens
once, in `initialize`, outside every measured loop). The inner VM's program
counter is reset once per outer run — the one place per iteration that pays the
sandbox's expensive re-entry path — which is constant in K and therefore
cancels out of the slope. The mediator does not switch on the reported host-call
index, it assumes the stub; a real service switches (corevm's handler table),
which is a compare and a branch inside the guest.

## Open items

1. **Batch amortisation** — out of scope by decision, and the only lever that
   would move the hashing verdict: N operations behind one crossing amortise
   the eight accesses, but not the copies, which is exactly what hashing is
   bound by. Signatures would benefit more (fixed 96–128 B payloads).
2. **Aux-data-region buffers** as a way to dodge the service-side write
   syscalls (`linux.rs:2344`). Untested.
3. **A one-point end-to-end validation on the real node stack** with actual
   crypto, to confirm the prediction composes. Its own small task.
4. **Spawn cost** (`machine` + `expunge`) is unmeasured by construction. A real
   refine call pays it once per parachain block, amortised over every host call
   in that block; worth a number if that amortisation ever looks thin.
