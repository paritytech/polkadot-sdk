# Nested host-call overhead — results

Companion to [nested-call-overhead-handoff.md](nested-call-overhead-handoff.md),
which owns the scope and the decisions. This document is the lab notebook.

**Status, 2026-08-12: the harness is built, validated, and pre-screened in the
container. No numbers of record yet — those need machine B with `--linux`, and
the pre-screen cannot substitute for them (see "Why the container cannot answer
this").** The commands to produce them are at the bottom.

## What is being decided

- A hypothetical `ed25519_verify` host call invoked from *parachain code* must
  cross the service↔node boundary several times, because the inner PVM cannot
  reach the node — the parachain service is a mandatory mediator.
- Machine B numbers of record from the wide-arith work: native host verify
  **23.4 µs**; the in-guest wide-arith route, **zero crossings**, **38.58 µs**
  ([wide-arith-results.md](wide-arith-results.md)).
- Therefore the host-call route wins on wall clock **iff the two-jump overhead
  at a 96–128 B payload is below ~15 µs**. This harness measures exactly that
  left-hand side, with no crypto in it.

## Measured, machine-independent: the protocol costs 4 + 1 crossings per call

Counted by the harness (`--phases` reports crossings per mediated call; at
K = 32 it reads 4.06, i.e. `(4·32 + 2)/32`):

| crossing | what crosses |
|---|---|
| `peek` | inner→service copy of the N-byte argument (two memcpys node-side) |
| stub | the node does the work; N bytes in, R bytes out |
| `poke` | service→inner copy of the R-byte result (two memcpys node-side) |
| `invoke` | resumes the inner VM; 112-byte register/gas block each way |

That is **4 service↔node crossings per logical call**, plus the inner VM's own
`ecalli` exit and resume, plus one extra `invoke` per inner run to collect the
halt. It confirms the hand-off's "≈ 5–6 crossings" estimate as a count, and it
is a property of the protocol, not of the machine.

Two copies per `peek` and per `poke` is what the node actually does
(`polkajam/crates/node/src/chain/exec/vm/host.rs:1588`, `:1631`: read into a
fresh `Vec`, then write); the harness mirrors it rather than optimising it.

## Why the container cannot answer this

- The **Linux sandbox does not run in the dev container** (`clone`, errno 38) —
  confirmed again here.
- The fallback, the **generic sandbox, runs the guest in-process**: a host call
  is a longjmp-style exit, not an IPC handshake
  (`polkavm/crates/polkavm/src/sandbox/generic.rs`). It has no crossing cost
  worth the name.
- On Linux the crossing is the futex/spin machinery: after an `ecalli` the host
  enters a low-latency mode where resuming is a store plus a spin, but the
  worker still spins `20× sched_yield` before sleeping
  (`polkavm/crates/polkavm-zygote/src/main.rs:803`), and any path that touches
  the program counter pays a longjmp handshake plus `futex_wake`
  (`sandbox/linux.rs:2640`, `:2957`).
- So the container numbers below are **shape, not size**: they validate the
  harness, the linearity, the copy slope and the relative cost of the peek
  discipline. Every crossing figure will change by an order of magnitude on the
  rig. This is the same class of error as the store-traffic finding in
  wide-arith-results.md — the container is not a proxy for save/sync traffic.

## Container pre-screen (machine A container, generic sandbox, `taskset -c 2-4`)

Medians of 3 interleaved rounds, order reversed on even rounds; each figure is
best-of-30-batches of 200 runs.

**Per mediated call, 128 B payload, 8 B result, K = 32:**

| configuration | ns/call |
|---|---:|
| one-jump (single VM, node serves it directly) | 64.5 |
| two-jump, packed peek | 321.4 |
| two-jump, per-arg peeks (3 instead of 1) | 476.5 |
| two-jump, `instantiate()` instead of `instantiate_nested()` | 323.6 |
| two-jump, inner VM with dynamic paging | 353.0 |

**Decomposition by regression** (the method that cancels every per-run fixed
cost):

| quantity | value |
|---|---:|
| per mediated call, from the K-sweep slope | **328.1 ns** |
| fixed per inner run, from the K-sweep intercept | 194 ns |
| per byte, two-jump, whole payload sweep | 0.0995 ns/B |
| per byte, one-jump, whole payload sweep | 0.0703 ns/B |
| fixed per call, one-jump (32–128 B fit) | 54.0 ns |

**Payload sweep, two-jump, ns per mediated call:** 32 B → 329.8, 96 B → 333.0,
128 B → 332.9, 1 KiB → 406.5, 4 KiB → 753.7, 64 KiB → 7002.5.

**Noise floor:** the unchanged 128 B packed configuration re-measured across the
three rounds gave 334.2 / 318.8 / 321.4 ns → **±2.4%**. Any container delta
smaller than that is not a result.

### What the pre-screen already establishes

- **Everything is linear and the two decomposition methods agree.** The K-sweep
  slope (328.1 ns) matches the directly measured per-call cost at large K
  (331.7 ns at K = 64), well inside the noise floor.
- **Packing arguments is worth ~48%** of the mediated call at 128 B (321 → 477
  ns, i.e. ~78 ns per extra `peek`). A service that peeks once instead of three
  times is not micro-optimising. This ratio is expected to transfer to the rig
  in relative terms; the absolute delta will grow with the crossing cost.
- **`instantiate_nested` shows nothing in the container** (323.6 vs 321.4 ns,
  inside the noise floor) — as expected, because its whole effect is worker
  core/CCX co-placement (`polkavm/crates/polkavm/src/api.rs:735`,
  `sandbox/linux.rs:1718–1755`), and the generic sandbox has no workers. **This
  is a rig-only question**, and it matters beyond this harness: the node spawns
  inner VMs with plain `instantiate()`
  (`polkajam/crates/node/src/chain/exec/vm/mod.rs:66`), so if the rig shows a
  gap, that is a free win for polkajam.
- **Dynamic paging on the inner VM costs ~10%** in the container (353.0 vs
  321.4 ns). The node enables it for the machines a service spawns, so this is a
  fidelity knob, not a measured mode — and on Linux it is userfaultfd, a
  different mechanism from the generic sandbox's, so the rig number is the only
  one that means anything.
- **The nesting tax is ~0.26% on a 262 µs pure-compute run** (671 ns absolute),
  and that absolute figure is one `invoke` round trip, not a per-instruction
  tax: the flat and nested runs execute identical code and produce identical
  results. **Nesting is free for compute** — the structural premise the in-guest
  crypto route rests on. Worth restating because other documents depend on it.
- **The copy cost is ~0.1 ns/B mediated, ~0.07 ns/B direct** — the mediated
  route moves each byte roughly one extra time, which is what the protocol says
  it should. At 64 KiB the mediated call is 7 µs and copy-dominated; that curve
  is the input to the "host call for *hashing*" question, which is a different
  decision from the signature one.

## The arithmetic that produces the verdict

Once the rig gives `C` = per-mediated-call cost at 128 B, two-jump, `--linux`:

- **`C` < 15 µs** ⇒ the host-call route beats the in-guest route (38.58 µs)
  on wall clock, since the native verify itself is 23.4 µs.
- **`C` > 15 µs** ⇒ the in-guest wide-arith route wins, and the follow-up that
  could overturn it is batch amortisation (N signatures per crossing), which
  this experiment deliberately does not build.

The container gives no guidance on which side of 15 µs `C` lands: 4 crossings
at anywhere from 0.5 to 3 µs each spans 2–12 µs, all of it below the budget,
while a pessimistic crossing cost puts it above. **The rig run decides it.**

## Reproducing

```bash
# in polkavm/, on branch mku-nested-call
cargo build -p nested-call --release
guest-programs/build-nested-call.sh

# checksums first — this is also the stale-blob tell
./target/release/nested-call --selftest --linux \
    guest-programs/target/riscv64emac-unknown-none-polkavm/release/bench-nested-caller.polkavm \
    guest-programs/target/riscv64emac-unknown-none-polkavm/release/bench-nested-mediator.polkavm

# the matrix: 3 interleaved rounds, order reversed on even rounds
./tools/nested-call/run-matrix.sh          # --linux, the rig configuration
./tools/nested-call/run-matrix.sh --generic 3   # the container pre-screen
```

Rig requirements: bare metal, quiet, **at least three free cores**
(`NESTED_CALL_CPUS`, default `2-4`) — the host thread and both VMs' worker
threads all spin during a crossing. Take medians across rounds; re-run one
unchanged configuration to measure the noise floor rather than assuming it.

## What the harness is

- `polkavm/tools/nested-call` — the host side: implements `machine`, `peek`,
  `poke`, `invoke` and the node stub for the outer guest, with the node's copy
  semantics (two copies per peek/poke, 112-byte invoke argument block) and its
  configuration (recompiler, sync gas on both VMs). Modes `one-jump`,
  `two-jump`, `nesting-tax`; sweeps over payload and calls-per-run with a
  least-squares fit; `--phases` for the direct decomposition.
- `polkavm/guest-programs/bench-nested-caller` — the parachain-code role: K stub
  calls per `run()`, plus a pure-compute export for the nesting-tax mode.
- `polkavm/guest-programs/bench-nested-mediator` — the parachain-service role:
  `invoke` → peek → node call → poke → resume, until the inner VM halts.
- Nothing in the VM changed. No new instructions, no semantics, no polkajam
  code imported — only the protocol shape.

**Correctness**: every mediated byte is verified. The guest folds each returned
result into a running value; the host recomputes the identical fold
independently and asserts equality, at every payload size, both peek
strategies, both result sizes, both modes — 96 configurations in `--selftest`,
all passing. Packed and per-arg peeks must produce the same checksum, which is
what proves the three-peek path reassembles the argument correctly.

## Deliberate deviations from the node, and why they are free

- **`machine` ignores the code pointer** and instantiates the blob the tool was
  given. Spawning happens once, in `initialize`, outside every measured loop.
- **The inner VM's program counter is reset once per outer run**, which is the
  one place per iteration that pays the sandbox's expensive re-entry path
  instead of the low-latency resume. It is constant in K, so the K-sweep slope —
  the figure of record — cancels it exactly. The mediation loop itself never
  touches a program counter.
- **Page faults under `--inner-dynamic-paging` are handled host-side** rather
  than by the service through the `pages` host call, so that mode measures
  dynamic paging's cost, not the cost of a page-fault protocol.

## Open items

1. **The rig run.** Everything above is scaffolding until machine B produces
   `C`; that is the deliverable.
2. **`instantiate_nested` vs `instantiate` on the rig.** If it moves, polkajam's
   `NestedEngine` should adopt it — a one-line change with no downside.
3. **Batch amortisation** (out of scope by decision): the follow-up that could
   overturn a "host calls lose" verdict.
4. **A one-point end-to-end validation on the real node stack** with actual
   crypto, to confirm the prediction composes. Its own small task.
