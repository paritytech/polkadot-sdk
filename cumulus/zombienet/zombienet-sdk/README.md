# How to run locally

As a prerequisite, the `test-parachain` and `polkadot` binaries need to be installed or available under `$PATH`.

The following commands need to be run from the repository root:
```
# install test-parachain
cargo install --path ./cumulus/test/service --locked
# install polkadot
cargo install --path ./polkadot --locked
```

The following command launches the tests:

```
ZOMBIE_PROVIDER=native cargo test --release -p cumulus-zombienet-sdk-tests --features zombie-ci
```

You can also just use `run.sh` that setups everything for you and runs the tests.

In addition, you can specify a base directory with `ZOMBIENET_SDK_BASE_DIR=/my/dir/of/choice`. All chain files and logs
will be placed in that directory.

## JAM network tests

The `jam` feature roots the network in a JAM chain instead of a relay chain. The JAM harness is the
`jam` module of `cumulus-zombienet-sdk-helpers` (`cumulus/zombienet/zombienet-sdk-helpers/src/jam/`),
which the same feature enables: `jam::spawn` is the one place a JAM test builds its network (the JAM
chain, one parachain per `jam::para::Para`, the collators), and the other modules are the genesis,
chain spec, RPC, control-lane and dropping-proxy pieces it composes. The JAM-only tests are this
crate's `tests/jam/` (`core_assignment`, `demo`, `polkavm_authoring`, `resubmission`).

The `jam` feature gates the `elastic_scaling` and `block_bundling` test suites to run on a JAM network. On JAM they run
`elastic_scaling::{slot_based_authoring, upgrade_to_3_cores}` and
`block_bundling::{full_core_usage_scenarios, runtime_upgrade, three_cores_glutton}`. The tests with no JAM counterpart
are compiled out, each with its reason in its `mod.rs`: `elastic_scaling::{asset_hub_westend, pov_recovery,
slot_based_rp_offset}` and `block_bundling::{basic, pov_recovery, relay_parent_offset, tracing_block, warp_sync}`.
`block_bundling::basic` needs five cores and HRMP; a tiny JAM network has exactly two cores (six validators, three per
core) and no HRMP.

Some of these suites run their para on a `cumulus-test-runtime` feature flavor from the start, and
others upgrade to one mid-run. Those flavors are built as PolkaVM blobs by building the test crate
**without** `SKIP_WASM_BUILD` and with
`SUBSTRATE_RUNTIME_TARGET=riscv WASM_BUILD_RUSTFLAGS=" --cfg jam"`: under the riscv target the
wasm-builder emits each flavor's PVM blob under the same `WASM_BINARY` const the test reads, so no
separate `.polkavm` path is needed. This is a heavy build — every flavor is compiled for riscv — and
it dominates the test run.

JAM networks are supported on the native provider only.

### Prerequisites

- This repository's collator: `cargo build --release -p polkadot-omni-node`.
- A polkajam node binary whose RPC serves `stateValue` and whose `gen-spec` reads the `services` /
  `auth_queues` / `assigners` keys the harness writes into the chain-spec config. If those are on
  two builds, point `JAM_GENSPEC_BIN` at the one with `gen-spec` support; only `gen-spec` runs from
  it.
- From the parachain-service repository, the `parachain-service.jam` and
  `parachain-authorizer-sr25519.jam` blobs (`cargo build --release -p cargo-jam-build` and
  `./target/release/cargo-jam-build -p parachain-service -p parachain-authorizer-sr25519`; they land
  under `target/pvm/riscv64emac-unknown-none-polkavm/{production,production-authorizer}/`). The
  authorizer's signature scheme has to match the runtime's `AuraId`, which is sr25519 for the
  parachain template; nothing can check the pairing, so a mismatch shows up only as a core no
  collator ever authorizes on.
- Only for `jam::core_assignment`: the `parasim-tool` CLI and polkajam's
  `jam-bootstrap-service.jam`.

### Running

Build the parachain runtime, and the Coretime chain's runtime that
`elastic_scaling::upgrade_to_3_cores` runs as para 1, as PolkaVM blobs with `--cfg jam`. Rebuild
after every runtime change. Then run a suite by its test filter:

```sh
SUBSTRATE_RUNTIME_TARGET=riscv WASM_BUILD_RUSTFLAGS=" --cfg jam" \
	cargo build --release -p parachain-template-runtime -p coretime-westend-runtime

export JAM_NODE_BIN=/path/to/polkajam/target/release/polkajam
export PARACHAIN_SERVICE_BLOB=/path/to/parachain-service.jam
export AUTHORIZER_BLOB=/path/to/parachain-authorizer-sr25519.jam
export RUNTIME_WASM=$PWD/target/release/rbuild/parachain-template-runtime/parachain-template-runtime-blob.polkavm
export CORETIME_RUNTIME_WASM=$PWD/target/release/rbuild/coretime-westend-runtime/fast_runtime_binary.rs.polkavm

SUBSTRATE_RUNTIME_TARGET=riscv WASM_BUILD_RUSTFLAGS=" --cfg jam" ZOMBIE_PROVIDER=native \
SUBSTRATE_ENABLE_POLKAVM=1 POLKAVM_BACKEND=interpreter POLKAVM_ALLOW_INSECURE=1 \
	cargo test -p cumulus-zombienet-sdk-tests --features jam,zombie-ci --test tests \
	-- --test-threads 1 --nocapture zombie_ci::elastic_scaling
```

| filter | what runs |
| --- | --- |
| `zombie_ci::elastic_scaling` | `slot_based_authoring` and `upgrade_to_3_cores`; the standard smoke run |
| `zombie_ci::block_bundling` | `full_core_usage_scenarios`, `runtime_upgrade` and `three_cores_glutton` |
| `jam::resubmission` | the two dropping-proxy tests: lost work packages whose blocks land in new ones |
| `jam::core_assignment` | the dynamic-core tests; they need `PARASIM_TOOL_BIN` and `BOOTSTRAP_SERVICE_BLOB` |
| `jam::polkavm_authoring` | a dev node authoring on the PolkaVM runtime blob |
| `jam::demo` | add `--ignored`: the harness with no assertion, until it is killed |

The process environment of the `cargo test` line is part of the run, not decoration:

- `SUBSTRATE_RUNTIME_TARGET=riscv` and `WASM_BUILD_RUSTFLAGS=" --cfg jam"` build the test crate's
  `cumulus-test-runtime` flavors as PolkaVM blobs with the JAM code compiled in. The value is the
  same string as in the runtime build, leading space included, so the two builds share one
  fingerprint.
- `ZOMBIE_PROVIDER=native`: the default provider (docker) cannot run a JAM network.
- `POLKAVM_BACKEND=interpreter` and `POLKAVM_ALLOW_INSECURE=1`: zombienet converts each para's
  chain spec to raw by invoking the collator binary itself, and that invocation inherits this
  process's environment rather than the per-node one the harness sets; without them PolkaVM
  cannot start without its Linux sandbox.
- `SUBSTRATE_ENABLE_POLKAVM=1`: zombienet-sdk builds each chain spec in-process with the published
  `sc-executor`, which only loads a PolkaVM runtime blob with this set.
- `--test-threads 1`: every test spawns the same six-validator network plus its collators, and two
  at once would make the six-second slot budget unrealistic.

If an artifact is missing the test fails at its start and names what it needs, for example
`missing artifacts: AUTHORIZER_BLOB (parachain-authorizer-sr25519.jam, ...)` or
`CORETIME_RUNTIME_WASM must point at the Coretime chain's PolkaVM blob`.

### Environment

| variable | what it points at |
| --- | --- |
| `JAM_NODE_BIN` | the polkajam node binary zombienet spawns for every JAM node |
| `JAM_GENSPEC_BIN` | optional: the polkajam build that runs `gen-spec`, when it is not `JAM_NODE_BIN` |
| `PARACHAIN_SERVICE_BLOB` | the real parachain-service `.jam` blob, which genesis creates the service from |
| `AUTHORIZER_BLOB` | `parachain-authorizer-sr25519.jam`, the AURA authorizer the cores run |
| `RUNTIME_WASM` | the PolkaVM build of the parachain runtime (`PVM\0` magic, despite the name), the para's JAM validation code *and* the runtime the collators execute; built with `--cfg jam` as above. A WASM blob is rejected at start-up |
| `CORETIME_RUNTIME_WASM` | the Coretime chain's PolkaVM blob, needed by `elastic_scaling::upgrade_to_3_cores` only; the `fast-runtime` flavor assigns a force-reserved core within a 20-slot timeslice instead of 80 |
| `OMNI_NODE_BIN` | optional: the collator binary, `target/release/polkadot-omni-node` by default |
| `PARASIM_TOOL_BIN` | optional: the `parasim-tool` CLI, for the dynamic-core tests only |
| `BOOTSTRAP_SERVICE_BLOB` | optional: polkajam's `jam-bootstrap-service.jam`, registered at genesis as service 0; for the dynamic-core tests only |
| `JAM_TEST_BASE_DIR` | optional: keep every run's work dir under this directory |
| `JAM_TEST_DROP_SEED` | optional: replay `jam::resubmission`'s random drops; a run logs the seed it used |
| `NUM_COLLATORS` | optional: how many collators `jam::demo` runs (default 1) |
| `POLKAVM_SANDBOX` | optional: `generic` runs the JAM nodes' recompiler in the generic sandbox (polkajam built with polkavm's `generic-sandbox`); otherwise they use the recompiler only when `vm.unprivileged_userfaultfd = 1` and fall back to the slow interpreter |

All blobs are copied into the run's work dir before genesis names them: PVM builds are not
byte-deterministic, so a rebuild during a run would strand a hash on the chain with no resolvable
preimage.

### Keeping the logs

By default a run works in a temporary directory that is deleted when it ends. With
`JAM_TEST_BASE_DIR` set, each run gets its own `$JAM_TEST_BASE_DIR/jam-collator-test-<test>-<stamp>-<n>/`,
which survives whether the test passed or failed: the patched chain spec of every para, the copied
blobs, and `zombienet/`, the network's base dir with one directory and log per JAM node
(`jam0`..`jamN`, `jam-or`) and per collator, plus `jam_config.json` and the generated
`jam_spec.json`.

### Reading a PVF failure

When the para head does not move, the reason is in the *guarantors'* logs
(`zombienet/jam0/jam0.log` and the other validators), not the collator's. The validation code runs
there, as a child PVM inside the parachain service, and the service narrates it at
`@<core>#<service>`:

```
INFO  @0#5 PVF dispatch probe: call=200 …     a host call the guest made (200 = set_parent_head_hash,
                                              201 = set_head, 203 = report_error, 204 = log)
INFO  @0#5 PVF invoke probe: host|halt|panic  how the child came back from one invoke
ERROR @0#5 PVF [runtime] panicked at …        the guest's own panic, with file, line and message
```

A refine that reaches `call=200` but never `call=201` produced no head, so the work result is an
error, and the service skips error results at accumulate without a word: a dead pipeline looks like
a quiet one from the JAM side.

A refine that ends in `call=203` (`report_error`) and `PVF run probe: Err RefineLog` rejected a
candidate that lies about JAM data, a fault its author can be slashed for. The service records the
payload in the para's log as `RefineLog::Opaque`; it is a SCALE-encoded
`cumulus_jam_primitives::RefineError`, one byte (`a1=0x1` in the dispatch probe):

| payload | `RefineError` | the candidate |
| --- | --- | --- |
| `0x00` | `JamParentMismatch` | a block of a package built at authoring time names JAM anchors other than the refine context's in its `JamParent` digest |
| `0x01` | `JamParentUnproven` | a block of a resubmitted package names an anchor or lookup anchor the package's anchor chain does not prove at the claimed slot |
| `0x02` | `AnchorChainInvalid` | the anchor chain does not lead from the context anchor through linked JAM headers |
| `0x03` | `JamProofRootMismatch` | a block's carried JAM state proof is rooted elsewhere than the state root derived for its anchor |

Every other rejected PoV (no PoV extrinsic, a PoV that does not decode, a pre-V4 PoV, a block
without a `JamParent` digest) is malformed input: the guest panics, so the log shows
`PVF [runtime] panicked at …` with the message, and the para's log gets no entry.
