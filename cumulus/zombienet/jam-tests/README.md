# JAM collator tests

End-to-end tests for `polkadot-omni-node` collating a parachain on a JAM network.

Each test is self-contained. It spawns its own six-validator JAM network with zombienet-sdk from
a genesis that already carries everything the collators need, starts N collators against it, and
asserts the parachain keeps producing and finalizing blocks. Nothing has to be running beforehand,
and nothing outside the test's own temporary work directory is touched.

## Prerequisites

From this repository:

```sh
cargo build --release -p polkadot-omni-node
cargo build --release --bin polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker
```

The `polkadot` binary is not used by the test itself — see "Why a relay chain" below.

The parachain runtime must be built twice: once as PolkaVM (the validation code JAM runs) and once
as WASM (what the collators execute locally). Build both:

```sh
SUBSTRATE_RUNTIME_TARGET=riscv cargo build --release -p parachain-template-runtime
cargo build --release -p parachain-template-runtime
```

The two builds land in *different* directories, and mixing them up is the most common way to
start a run that cannot work:

| build | path | magic |
|---|---|---|
| PolkaVM (`RUNTIME_WASM`) | `target/release/rbuild/parachain-template-runtime/parachain-template-runtime-blob.polkavm` | `PVM\0` |
| WASM (`RUNTIME_AUTHORING_WASM`) | `target/release/wbuild/parachain-template-runtime/parachain_template_runtime.compact.compressed.wasm` | zstd-wrapped `\0asm` |

The suite rejects a WASM blob at `RUNTIME_WASM` or a PolkaVM blob at `RUNTIME_AUTHORING_WASM` at
startup with a named diagnostic.

Rebuild **both** after any runtime change. The collators author with the WASM build while the PVF
validates with the PolkaVM one, so a stale half produces state-root failures that look like
unrelated bugs.

From the polkajam repository: the `polkajam` node binary. It has to do two things, and today they
live on two branches:

* its `gen-spec` has to understand the `services` / `auth_queues` / `assigners` keys this suite
  writes into the chain-spec config. A build that does not ignores them without a word, so the
  run checks the spec it wrote as soon as the network is spawned and fails right there, naming
  the file and `JAM_GENSPEC_BIN`;
* its RPC has to serve `stateValue`, which is how the collator reads the parachain service, the
  authorizer pools and queues, and the availability assignments. A build without it lets the
  network come up and the collators start, and then every collator tick logs `MethodNotFound` and
  the parachain never authors a block.

Until one build does both, set `JAM_GENSPEC_BIN` to a build with the first and `JAM_NODE_BIN` to a
build with the second: only `gen-spec` runs from `JAM_GENSPEC_BIN`, and the generated spec is
portable between the two.

From the parachain-service repository: the real `parachain-service.jam` blob and the
`parachain-authorizer-sr25519.jam` blob. Nothing builds them as a side effect of `cargo build`
any more — ask for them by name, and each crate's `[package.metadata.jam]` says how it wants
building (the authorizer at `production-authorizer`, to fit JAM's 64 kB `C_maxauthcodesize`):

```sh
cargo build --release -p cargo-jam-build
./target/release/cargo-jam-build -p parachain-service -p parachain-authorizer-sr25519
```

They land under `target/jam/<target>/<profile>/`, at a path that does not move between builds:

```
target/jam/riscv64emac-unknown-none-polkavm/production/parachain-service.jam
target/jam/riscv64emac-unknown-none-polkavm/production-authorizer/parachain-authorizer-sr25519.jam
```

The `parasim-tool` CLI is needed only by the two dynamic-core tests, which are the only ones that
move a core mid-run; without it they skip and everything else runs. The `parasim-service.jam` blob
is no longer needed for the progress tests (the real service is used instead), but is retained for
the dynamic-core tests and the README's toy runs.

There is one authorizer blob per signature scheme, and which one a para needs is decided by its
runtime's `AuraId`. The parachain template is sr25519, so that is the blob this suite puts on the
chain. Nothing can check the pairing — a blob's scheme is not visible in its bytes — so a mismatch
shows up only as a core no collator ever authorizes on.

## Running

```sh
export JAM_NODE_BIN=/path/to/polkajam/target/release/polkajam
# Only while gen-spec and the stateValue RPC are on different polkajam branches:
export JAM_GENSPEC_BIN=/path/to/a/polkajam/whose/gen-spec/reads/the/genesis/keys
export PARACHAIN_SERVICE_BLOB=/path/to/parachain-service/target/jam/riscv64emac-unknown-none-polkavm/\
production/parachain-service.jam
export AUTHORIZER_BLOB=/path/to/parachain-service/target/jam/riscv64emac-unknown-none-polkavm/\
production-authorizer/parachain-authorizer-sr25519.jam
export RUNTIME_WASM=/path/to/polkadot-sdk3/target/release/rbuild/parachain-template-runtime/\
parachain-template-runtime-blob.polkavm
export RUNTIME_AUTHORING_WASM=/path/to/polkadot-sdk3/target/release/wbuild/parachain-template-runtime/\
parachain_template_runtime.compact.compressed.wasm
# Only for `jam::core_assignment`'s two dynamic-core tests:
export PARASIM_TOOL_BIN=/path/to/parachain-service/target/release/parasim-tool

cargo test -p cumulus-jam-zombienet-tests --features jam-ci --test tests \
	-- --test-threads 1 --nocapture jam::collator_progress
```

### Environment

| variable | what it points at |
| --- | --- |
| `JAM_NODE_BIN` | the polkajam node binary zombienet spawns for every JAM node |
| `JAM_GENSPEC_BIN` | the polkajam build that runs `gen-spec`, when it is not `JAM_NODE_BIN` |
| `PARACHAIN_SERVICE_BLOB` | the real parachain-service `.jam` blob, which genesis creates the service from |
| `AUTHORIZER_BLOB` | `parachain-authorizer-sr25519.jam`, the AURA authorizer the cores run |
| `RUNTIME_WASM` | the PolkaVM build of the parachain runtime (`PVM\0` magic), used as the para's JAM validation code. The name is misleading (it is not WASM); a future rename to `RUNTIME_PVF` is deferred. |
| `RUNTIME_AUTHORING_WASM` | the WASM build of the parachain runtime, supplied to collators via `--wasm-runtime-overrides` for execution |
| `PARASIM_BLOB` | optional: `parasim-service.jam`, only needed for the dynamic-core tests and toy runs |
| `PARASIM_TOOL_BIN` | optional: the `parasim-tool` CLI, required only by the dynamic-core tests, which move cores mid-run |
| `OMNI_NODE_BIN`, `RELAY_NODE_BIN` | override the `target/release` defaults |
| `JAM_TEST_BASE_DIR` | keep every run's work dir under this directory |
| `NUM_COLLATORS` | how many collators the demo runs (default 1) |

All blobs are copied into the run's work dir before genesis names them: PVM builds are not
byte-deterministic, so a rebuild during a run would strand a hash on the chain with no resolvable
preimage. The collators are pointed at the authorizer *copy*, because an authorizer hash is a hash
of exactly those bytes — and the copy is what genesis hosts.

The two runtime blobs serve different roles. The PolkaVM blob is the para's JAM validation code
and is registered in the chain spec's `:code`; the collator declares this hash via `code_at`. The
WASM blob is supplied via `--wasm-runtime-overrides` and changes only what the collator *executes*
— the declared hash stays the PolkaVM blob's. This asymmetry exists because a PolkaVM runtime
cannot author: its `sp_io` re-exports crypto from `native::crypto`, whose key-generation functions
panic by design (they need node-side state), so a PolkaVM runtime traps in `SessionKeys_generate_session_keys`.

`--test-threads 1` is required: each test spawns seven JAM nodes plus its collators, and running
them concurrently would fight over CPU and make the six-second slot budget unrealistic.

If any artifact is missing the tests print what they need and pass without running — they never
fail for a reason unrelated to the collator. `PARASIM_TOOL_BIN` skips only the two dynamic-core
tests; every other variable skips the whole suite.

### Keeping the logs

By default a run works in a temporary directory that is deleted when it ends. Set
`JAM_TEST_BASE_DIR` to keep it instead:

```sh
export JAM_TEST_BASE_DIR=/home/miszka/parity/46-jam-cummulus-side-2/xxx-logs
```

Each run then gets its own `$JAM_TEST_BASE_DIR/jam-collator-test-<test>-<YYYYmmdd-HHMMSS>/`, which
survives whether the test passed or failed. Everything one run produces is inside it:

```
jam-collator-test-two_jam_collators_build_blocks-20260831-141233/
	jam-parachain-0-spec.json  the patched chain spec of para 0, one file per para
	parasim-service.jam        the copy of the blob genesis created the service from
	parachain-authorizer.jam   the copy of the blob whose hash genesis queued on the cores
	alice.log, bob.log, ...    one log per collator
	alice/, bob/, ...          one base path per collator
	zombienet/                 the network zombienet spawned: jam0..jam5, jam-or, relay-filler
	zombienet/jam_config.json  what the chain spec was generated from, genesis section included
	zombienet/jam_spec.json    the generated chain spec every JAM node started on
```

The zombienet network is given `zombienet/` as its base directory, so its nodes' logs are part of
the same tree rather than somewhere under `/tmp`. The harness logs the resolved path as
`work dir: ...` as soon as the run starts.

The demo honours the same variable.

### Reading a PVF failure

When the para head does not move, the reason is in the *guarantors'* logs
(`zombienet/jam0/jam0.log` … `jam5`), not the collator's. The validation code runs there, as a child
PVM inside the parachain service, and the service narrates it at `@<core>#<service>`:

```
INFO  @0#5 PVF dispatch probe: call=200 …     a host call the guest made (200 = set_parent_head_hash,
                                              201 = set_head, 203 = report_error, 204 = log)
INFO  @0#5 PVF invoke probe: host|halt|panic  how the child came back from one invoke
ERROR @0#5 PVF [runtime] panicked at …        the guest's own panic, with file, line and message
```

A refine that reaches `call=200` but never `call=201` produced no head, so the work result is an
error — and `accumulate/package.rs` skips error results silently, which is why a dead pipeline looks
like a quiet one from the JAM side.

The `PVF [runtime] panicked` line exists because `sp-io`'s PolkaVM panic handler forwards to the JAM
`log` host call (`substrate/primitives/io/src/native/logging.rs`, import index 204, dispatched by
the service). Without it a guest panic arrives as nothing but `Trap at <pc>: explicit trap`. Only
the panic and OOM handlers emit: `native::logging::max_level()` is `Off`, so the runtime's ordinary
`log::` calls stay silent inside refine.

Service-side panics are still opaque — the service guest is built with panic messages compiled out,
so its own `panic!("… §4.2 whole-refine failure")` shows up only as a `Trap at <pc>` with no text.

The demo runs the same code path with no assertion, until it is killed:

```sh
NUM_COLLATORS=2 cumulus/zombienet/jam-tests/demo.sh
```

To run collators against a JAM testnet you already have running, rather than one spawned here,
use `cumulus/scripts/jam-collator-demo.sh` instead.

## Layout

| file | what it does |
| --- | --- |
| `tests/jam/env.rs` | resolves the binaries, or explains what is missing |
| `tests/jam/network.rs` | builds the genesis override — the real parachain service, the authorizers, the cores, and the validation code — and spawns the JAM network from it |
| `tests/jam/genesis.rs` | derives a para's authorizer hash, the way the collator derives it |
| `tests/jam/chain_spec.rs` | builds and patches one para's chain spec |
| `tests/jam/collators.rs` | starts, supervises and tears down one para's collator processes |
| `tests/jam/rpc.rs` | the JAM node and collator RPC clients |
| `tests/jam/harness.rs` | one run: network, collators, the authorizer-agreement check, assertions |
| `tests/jam/collator_progress.rs` | the 1, 2, 3 and 6 collator tests |
| `tests/jam/core_assignment.rs` | two paras at once, and cores taken away or moved mid-run |
| `tests/jam/demo.rs` | the same run with no assertion and no end |
| `demo.sh` | shell entry point for that demo |

## Three things worth knowing about the collators

The parachain template's `development` preset has no `aura.authorities` — pallet-session drives
pallet-aura, so the authority set comes from `session.keys` and `collatorSelection.invulnerables`.
The harness rewrites both to exactly the collators it is about to start: an authority with no
running collator costs a full six-second slot of block production every time its turn comes round.

The para id is the harness's, not the preset's. It is the id that goes into the authorizer config
genesis commits the core to, and the collator reads its own id straight out of this spec, so the
two have to agree — otherwise the collator computes an authorizer hash no core holds and never
finds a core to submit to. The existing tests all run para 0.

A collator needs no key of its own for JAM. It signs work packages with the aura session key it
already claims slots with — `--alice` puts that in the keystore in memory — and it learns the
collator set from `AuraApi::authorities()` at startup, which is the same set the harness wrote
into `session.keys` above. So the only thing that has to be kept in step is the set `genesis.rs`
hashes: it builds the collator-set trie the authorizer hash commits to, and a different set, a
different order or a different curve is a hash no collator will ever match. `Run::start` checks
that the two agree against what every collator logs at startup, so a mismatch fails in the first
minute rather than as a head that never moves.

**The runtime does not return that set in the order genesis names it.** Collator-selection keeps
its invulnerables sorted by account id and pallet-session builds the aura authorities from that,
so `alice,bob` comes back as `bob,alice`. A leaf's position in the collator trie *is* the collator
index, so the order is part of the hash: `chain_spec::in_authority_order` is what both
`genesis.rs` and every `parasim-tool --collators` string go through. A single-collator run cannot
see any of this, which is how it stayed broken while one test kept passing.

## Why the runtime is built twice

One blob cannot serve both roles. The plan originally had a single blob be both the collator's
authoring runtime and the para's JAM validation code. That is impossible: on riscv, `sp_io`
re-exports crypto from `native::crypto`, whose key-generation functions panic by design ("needs
node-side state and has no in-blob implementation"), so a PolkaVM runtime traps in
`SessionKeys_generate_session_keys` and cannot author. Nothing in this repo authors with a PolkaVM
runtime.

The two roles separate via an asymmetry in the collator. `code_at` delegates to
`code_at_ignoring_overrides`, which returns the on-chain `:code`; `--wasm-runtime-overrides`
changes only what is *executed*. So the chain spec keeps the PolkaVM blob (the collator therefore
*declares* the hash genesis registered) while the collator *executes* a WASM override. The
declared hash stays the PolkaVM blob's, because `code_at` ignores overrides.

This looks wrong until you know why it is right: the chain spec's `:code` is what JAM validates
with, and the collator must declare that same hash so the service can look up the code preimage at
refine time. The WASM override is purely local — it never reaches the chain, never reaches JAM,
and never reaches the preimage store. It is only what the collator executes, and it is safe to
override because the collator's own crypto (signing work packages) uses the host's `sp_io`, not the
runtime's.

## Running a runtime other than the template

`RUNTIME_WASM` (the PolkaVM build) and `RUNTIME_AUTHORING_WASM` (the WASM build) choose the
parachain runtime. Two have been run: the parachain template (the default) and Asset Hub Rococo,
which is the first real chain's runtime on this stack.

```sh
SUBSTRATE_RUNTIME_TARGET=riscv cargo build --release -p asset-hub-rococo-runtime
cargo build --release -p asset-hub-rococo-runtime
export RUNTIME_WASM=target/release/wbuild/asset-hub-rococo-runtime/\
asset_hub_rococo_runtime.compact.compressed.wasm
export RUNTIME_AUTHORING_WASM=target/release/wbuild/asset-hub-rococo-runtime/\
asset_hub_rococo_runtime.compact.compressed.wasm
```

What `chain_spec.rs` needs from that runtime's `development` preset, and checks before it patches:

* **`session.keys` entries shaped `[account, account, { aura: key }]`.** The harness replaces the
  whole list with one entry per collator it is about to start, so a runtime whose `SessionKeys`
  has a second field beside `aura` would have it dropped and produce a genesis the runtime cannot
  decode. The check is on that shape only — *not* on which collators the preset names, because
  every runtime names its own (the template two, Asset Hub one).
* **`balances`, `collatorSelection.invulnerables`, `parachainInfo`.** A collator the preset does
  not endow is topped up with the preset's own endowment, so no amount is hardcoded per runtime.
  Asset Hub Rococo's preset funds only Alice and Bob, so anything past two collators needs this;
  the template funds all six.
* **An sr25519 `AuraId` matching `AUTHORIZER_BLOB`.** Both of these runtimes use
  `parachains_common::AuraId`. Nothing can check the pairing — see the note above.

**The para id stays the harness's, and 0 is fine for a real runtime.** Asset Hub Rococo's preset
pins 1000; the harness overwrites it with the id whose core genesis names, exactly as it does
for the template. Nothing on the collator's path carries a para id of its own: it reads the id from
the runtime (`GetParachainInfo`) and threads that same value into the mocked relay state proof, so
the proof's para-keyed entries and the runtime's `SelfParaId` cannot disagree whatever the id is.
Asset Hub uses its id only to build XCM locations, which a run that sends no XCM never evaluates.

Asset Hub Rococo is a six-second chain like the template — `SLOT_DURATION` and
`RELAY_CHAIN_SLOT_DURATION_MILLIS` are both 6000, which is what the mocked relay slot assumes — and
its async-backing limits are looser, not tighter (velocity 12 and unincluded-segment capacity 36,
against the template's 1 and 3). So `--jam-slot-duration` stays at its default.

Measured 2026-09-02, para 0: `two_jam_collators_build_blocks` best 30 / finalized 27 in 310s and
`three_jam_collators_build_blocks` the same in 309s — the six-second median the template gives,
with no runtime warning of its own in any collator log. The three-collator run is the one that
exercises the endowment top-up, Charlie being the first collator Asset Hub's preset does not fund.
The demo ran two collators to best 42 / finalized 39 and stopped cleanly on Ctrl-C.

## What genesis carries, and what is left to do afterwards

Nothing. The chain spec `polkajam gen-spec` generates for a run already holds:

* **the real parachain service as service 5** (`network::PARASIM_SERVICE_ID`), created from the
  copied-aside `parachain-service.jam` with a balance of 10^15, and **hosting the AURA authorizer
  blob's preimage**. That is where a guarantor resolves the authorizer code from, because a
  collator's work package names the parachain service as its `auth_code_host`.
* **each para's core queued for that para's authorizer hash**, derived by `tests/jam/genesis.rs`
  exactly as the collator derives it — the blob's code hash, and a config naming the para id, the
  service, the collator-set root, the set size and the slot duration.
* **each of those cores' assigner privilege held by the parachain service**, which is what lets a
  later `free-core` or re-assignment travel the control lane inside an AURA package.
* **the para's validation code as a PolkaVM blob**, registered in the service's `ParaInfo` and
  hosted as a preimage so the service can resolve it at refine time.

All of that reaches `gen-spec` as one JSON object. zombienet-sdk knows nothing about these keys:
`JamNetwork::spawn` hands it the object through `with_genesis_overrides`, and it is merged as is
into the `jam_config.json` zombienet generates, next to the `id` and `genesis_validators` it
writes itself. For a single para on core 0 the object is:

```json
{
  "services": {
    "5": {
      "code": "<work dir>/parachain-service.jam",
      "balance": 1000000000000000,
      "preimages": ["<work dir>/parachain-authorizer-sr25519.jam", "<work dir>/runtime.polkavm"]
    }
  },
  "auth_queues": { "0": "<the para's authorizer hash, bare hex>" },
  "assigners": { "0": 5 }
}
```

`services` is an object keyed by service id, which is what `jam-chainspec` declares
(`BTreeMap<ServiceId, GenesisService>`) — not an array of `{ "id": ..., ... }` objects.

`network::genesis_overrides` is the one place the harness spells that schema, and its unit test
pins the keys; the schema's owner is polkajam's `jam-chainspec` crate. A balance above 2^53 is
written as a decimal string, because `gen-spec` refuses a JSON number it cannot read back exactly.

So a run goes straight from "the network finalized a block" to starting collators, after two
checks that the genesis is really the one described. First, before anything is asked of the
nodes, `zombienet/jam_spec.json` has to hold service 5's record in its `genesis_state` — the key
`ff05000000000000` followed by 23 zero bytes. A `gen-spec` that does not know the keys drops them
without a word, so this fails at once and says to point `JAM_GENSPEC_BIN` at a build that does.
Second, once the ordinary node answers, `listServices` has to include 5: the service is genesis
state, so a chain without it means the nodes started from some other spec than the one just
checked; the error names `zombienet/jam_spec.json` and the `jam_config.json` beside it.
`Run::start` then waits for every collator's startup line and fails unless the authorizer it
derived is the one genesis queued.

A tiny network has exactly two cores: polkajam ties `core_count` to the validator count (six
validators, three per core) and the next step up is 78 validators. So two paras is the most this
harness can run, and a single-para run leaves core 1 untouched — still under the null authorizer,
still with service 0 as its assigner, which is the bootstrap lane the reassignment test rides.

### The one step that is left, and only for two tests

`parasim-tool deploy-authorizer` hosts the AURA blob in the **bootstrap service** as well.
Nothing the collators do needs that any more, but `parasim-tool` builds its own control packages
with `auth_code_host: 0`, so a guarantor asked to authorize an `assign-core` or `free-core`
command looks the code up in service 0. Genesis cannot be asked to host a preimage in service 0 —
the config has no way to add one to the bootstrap service — so the two dynamic-core tests call
`JamNetwork::host_authorizer_for_control_packages` before their first core change, and nothing
else does. It is idempotent ("already available; nothing to do") and it rides an unassigned core,
which is why the reassignment test has to run it *before* it assigns core 1.

That call disappears the day `parasim-tool` names `--service` in `auth_code_host` instead of 0.

## Taking cores away and moving them, mid-run

`tests/jam/core_assignment.rs` changes the core layout while the paras are running, which the
progress tests never do. Two things about it are worth knowing before adding a test there.

**A test asserts on the accumulated head, not on the collator's height.** A collator authors
whether or not anything works, so its own height proves nothing about JAM. What proves it is the
head parasim has stored for the para, read the way the collator reads it: `serviceValue` at the
best block, under the key the parachain service files a para's `ParaInfo` at, whose `head_data` is
the para's header. The harness exposes it as `JamNetwork::para_head` and every phase wait is
written against it. The two readings together are the assertion: a frozen head with a climbing
local best is a stall, and both climbing is a healthy para.

The key and both decodes come from the crates that wrote them — `para_info_key` and `ParaInfo`
from the facade, the header from the runtime's own type — so nothing in the harness holds a byte
offset that could drift out of step with the service.

**Freeing a core parks it; it does not empty it.** `free-core` installs the same authorizer code
under a config naming no para, so the para's hash drains out of the pool and the core stops
carrying parachain work — but the core still accepts *control* packages, because the code that
authorizes them is still there. So a re-`assign-core` rides the parked core itself, and a para
that has lost its only core can be given it straight back. That is what the stall test does, and
it is why these tests need no spare core to recover.

**A carrier is only needed to reach a core that can not carry the command itself.** That is what
`--via-core`/`--via-para`/`--via-collators` are for, and no test here needs them: every core these
tests touch can carry its own command. The tool checks the carrier it builds against the hash the
carrier core actually holds and refuses to submit on a mismatch, naming both hashes — so a wrong
carrier is a loud failure, not a core that quietly authorizes nothing.

## The zombienet-sdk dependency

This crate depends on the unmerged `jam-integration` branch
([PR #573](https://github.com/paritytech/zombienet-sdk/pull/573)), which is what adds JAM networks
to the SDK. It deliberately does not share the `zombienet-sdk` version the rest of the workspace
uses, so `cumulus-zombienet-sdk-tests` and every existing zombienet test keep their released pin.

**The dependency currently points at a local checkout, not at a git revision.** The pinned rev
`74a1d56` carries the genesis address bug described below; the fix for it is not upstream yet, so
`Cargo.toml` uses a `path` dependency on a sibling `zombienet-sdk` working copy that has it
applied. The git line it replaces is kept, commented out, right above it. Restore that line — and
bump the rev — once the fix lands in #573, and drop the dependency entirely when #573 merges.

### Why a relay chain

At the pinned revision a JAM-only network is not yet possible: the orchestrator unwraps the relay
chain config unconditionally, so building a network without one panics at spawn time. The harness
therefore starts a single idle relay validator that nothing in the test uses, which is the only
reason a `polkadot` binary is needed.

### The SDK bug that made these tests slow (fixed in the local checkout)

`jam_config.rs` recorded each validator's address in JAM genesis as `127.0.0.1:{rpc_port}`, but
starts the node with `--port={p2p_port}` — a different, randomly chosen port. In polkajam the
genesis validator metadata *is* the address book and it overrides `--bootnode` addresses, so the
network forms (the bootnode dials happen before a node learns it is a validator) but cannot
recover: every validator observed here drops from five validator peers to three within a few
minutes and never reconnects. Work packages whose guarantor set has just rotated then miss their
report deadline, and each miss costs three rebuilt parachain blocks. The measured block rate was
~22s instead of the 6s a healthy JAM network gives, which is why the deadline is 25 minutes — it
is now far larger than any run needs, and is left as headroom rather than tuned to these numbers.
There was no workaround from the test side: `JamNodeConfigBuilder` has `with_rpc_port` but no
`with_p2p_port`. The fix is one line — write `n.port` into `net_addr` instead of `n.rpc_port` —
and it is in the local checkout this crate now builds against.

### Current status of the collator-progress tests

`one_jam_collator_builds_blocks` passes against the real parachain service: **para 0 best 30 /
finalized 27 in ~199s**, which is the 6s median a healthy JAM network gives and matches what the
`parasim` mock used to reach in ~310s.

Measured 2026-09-11 over all six guarantors: 258 refines, 258 `set_head` declarations, **0 guest
panics, 0 traps, 0 `report_error` aborts**, and no `SKIP` line.

Getting there took three fixes, each of which had silently capped the run at best 3 blocks before:

* **`validate_validation_data` was asserting a relay-chain invariant on the JAM path.** It compares
  `parent_header` against the block's own `set_validation_data` inherent, and the JAM collator's
  mocked relay state never fills `parent_head`
  (`cumulus/client/parachain-inherent/src/mock.rs:238` hardcodes `Default::default()`). It is now
  relay-only *by structure*: the shared core takes an `on_block_validated` hook, the polkadot layer
  passes the real check and the JAM layer passes a no-op. No `cfg(target_arch)` anywhere.
* **The PVF had no way to learn the head it was actually built on.** The anchor state proof proves
  the *accumulated* head, which is wrong the moment the collator pipelines ahead: the PoV's storage
  proof is taken against the real parent's state root, not the accumulated one. The parent header
  now travels untrusted in `ParachainBlockData::V4`; `verify_blocks_form_chain` binds it to
  `blocks[0]`, and the service binds it to the canonical chain by comparing the hash the PVF
  declares against its own stored head at accumulate. The anchor proof is gone.
* **Guest panics were invisible.** `sp-io`'s PolkaVM panic handler formatted the message and dropped
  it, so every failure arrived as `Trap at <pc>: explicit trap`. It now forwards to the JAM `log`
  host call, which is what made the first bullet diagnosable at all.

### Current status of the core tests

All three pass. Two consecutive full runs of the stall test reached the same head numbers at the
same points, so the figures below are what a healthy stack does rather than one lucky run:

| test | wall clock | what it observed |
| --- | --- | --- |
| `two_paras_on_two_cores_build_blocks` | ~345s | para 0 best 30 / finalized 27, para 1 best 31 / finalized 28, neither collator knowing the other's head |
| `freeing_the_core_freezes_the_para_head_until_it_is_assigned_again` | 401s | healthy at head #5, parked at #6, seven more heads drained through, then frozen at #13 for 90s while 18 blocks were authored and 13 re-rooted; #14 within 72s of the re-assign onto the parked core |
| `moving_the_para_to_the_other_core_keeps_its_head_moving` | 372s | head #5 to #28 with no pause; the collator saw both cores and stayed on core 0, then submitted 11 packages to core 1 once core 0 was parked |

The stall test heals on **core 0 itself**, the core it just lost. That is the parked-core
property: the core keeps the authorizer code and so keeps taking the control package that puts a
para back on it. Before parking, this test had to escape to the spare core, and a single-para
network that lost its only core could not be recovered at all.

### What upstream support should replace

* The relay chain filler node, once a jamchain can be spawned on its own.
* `network.rs`'s hand-rolled six-validator topology, once `with_tiny_jamchain()` accepts per-node
  environment variables. It is hand-rolled only because the JAM nodes need
  `POLKAVM_BACKEND=interpreter` and `POLKAVM_ALLOW_INSECURE=1` in sandboxes without userfaultfd,
  and the native provider clears the environment before spawning.
* The pinned JAM RPC port, once JAM nodes appear in the `Network` handle and their URL can be read
  back with `get_node("jam-or")`.
* All of `collators.rs`, once the SDK can express a parachain whose relay chain is a JAM network:
  the collators would become ordinary zombienet nodes with the usual metric-based assertions.
