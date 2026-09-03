# JAM collator tests

End-to-end tests for `polkadot-omni-node` collating a parachain on a JAM network.

Each test is self-contained. It spawns its own six-validator JAM network with zombienet-sdk from
a genesis that already carries everything the collators need, starts N collators against it, and
asserts the parachain keeps producing and finalizing blocks. Nothing has to be running beforehand,
and nothing outside the test's own temporary work directory is touched.

## Prerequisites

From this repository:

```sh
cargo build --release -p polkadot-omni-node -p parachain-template-runtime
cargo build --release --bin polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker
```

The `polkadot` binary is not used by the test itself — see "Why a relay chain" below.

From the polkajam repository: the `polkajam` node binary. It has to do two things, and today they
live on two branches:

* its `gen-spec` has to understand the `services` / `auth_queues` / `assigners` keys this suite
  writes into the chain-spec config. A build that does not ignores them without a word, and the
  run then fails at its first readiness check, naming the generated spec;
* its RPC has to serve `stateValue`, which is how the collator reads the parachain service, the
  authorizer pools and queues, and the availability assignments. A build without it lets the
  network come up and the collators start, and then every collator tick logs `MethodNotFound` and
  the parachain never authors a block.

Until one build does both, set `JAM_GENSPEC_BIN` to a build with the first and `JAM_NODE_BIN` to a
build with the second: only `gen-spec` runs from `JAM_GENSPEC_BIN`, and the generated spec is
portable between the two.

From the parachain-service repository: the compiled `parasim-service.jam` and
`parachain-authorizer-sr25519.jam` blobs, and the `parasim-tool` CLI, which the para-head reads
and the dynamic-core tests use.

There is one authorizer blob per signature scheme, and which one a para needs is decided by its
runtime's `AuraId`. The parachain template is sr25519, so that is the blob this suite puts on the
chain. Nothing can check the pairing — a blob's scheme is not visible in its bytes — so a mismatch
shows up only as a core no collator ever authorizes on.

## Running

```sh
export JAM_NODE_BIN=/path/to/polkajam/target/release/polkajam
# Only while gen-spec and the stateValue RPC are on different polkajam branches:
export JAM_GENSPEC_BIN=/path/to/a/polkajam/whose/gen-spec/reads/the/genesis/keys
export PARASIM_BLOB=/path/to/parachain-service/.../parasim-service.jam
export AUTHORIZER_BLOB=/path/to/parachain-service/.../parachain-authorizer-sr25519.jam
export PARASIM_TOOL_BIN=/path/to/parachain-service/target/release/parasim-tool

cargo test -p cumulus-jam-zombienet-tests --features jam-ci --test tests \
	-- --test-threads 1 --nocapture jam::collator_progress
```

### Environment

| variable | what it points at |
| --- | --- |
| `JAM_NODE_BIN` | the polkajam node binary zombienet spawns for every JAM node |
| `JAM_GENSPEC_BIN` | the polkajam build that runs `gen-spec`, when it is not `JAM_NODE_BIN` |
| `PARASIM_TOOL_BIN` | the `parasim-tool` CLI, used to read para heads and to move cores mid-run |
| `PARASIM_BLOB` | `parasim-service.jam`, the service genesis creates and the collators talk to |
| `AUTHORIZER_BLOB` | `parachain-authorizer-sr25519.jam`, the AURA authorizer the cores run |
| `OMNI_NODE_BIN`, `RUNTIME_WASM`, `RELAY_NODE_BIN` | override the `target/release` defaults |
| `JAM_TEST_BASE_DIR` | keep every run's work dir under this directory |
| `NUM_COLLATORS` | how many collators the demo runs (default 1) |

Both blobs are copied into the run's work dir before genesis names them: PVM builds are not
byte-deterministic, so a rebuild during a run would strand a hash on the chain with no resolvable
preimage. The collators are pointed at the authorizer *copy*, because an authorizer hash is a hash
of exactly those bytes — and the copy is what genesis hosts.

`--test-threads 1` is required: each test spawns seven JAM nodes plus its collators, and running
them concurrently would fight over CPU and make the six-second slot budget unrealistic.

If any artifact is missing the tests print what they need and pass without running — they never
fail for a reason unrelated to the collator.

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
| `tests/jam/network.rs` | spawns the JAM network from a genesis carrying parasim, the authorizers and the cores |
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

## Running a runtime other than the template

`RUNTIME_WASM` chooses the parachain runtime. Two have been run: the parachain template (the
default) and Asset Hub Rococo, which is the first real chain's runtime on this stack.

```sh
cargo build --release -p asset-hub-rococo-runtime
export RUNTIME_WASM=target/release/wbuild/asset-hub-rococo-runtime/\
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

* **parasim as service 5** (`network::PARASIM_SERVICE_ID`), created from the copied-aside
  `parasim-service.jam` with a balance of 10^15, and **hosting the AURA authorizer blob's
  preimage**. That is where a guarantor resolves the authorizer code from, because a collator's
  work package names the parachain service as its `auth_code_host`.
* **each para's core queued for that para's authorizer hash**, derived by `tests/jam/genesis.rs`
  exactly as the collator derives it — the blob's code hash, and a config naming the para id, the
  service, the collator-set root, the set size and the slot duration.
* **each of those cores' assigner privilege held by parasim**, which is what lets a later
  `free-core` or re-assignment travel the control lane inside an AURA package.

So a run goes straight from "the network finalized a block" to starting collators. `JamNetwork`
reads the service list back once as a check: parasim is genesis state, so a chain without it means
the generated spec never reached the nodes, and the error names `zombienet/jam_spec.json` and the
`jam_config.json` beside it. `Run::start` then waits for every collator's startup line and fails
unless the authorizer it derived is the one genesis queued.

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
head parasim has stored for the para, read back with `parasim-tool display-key parahead`; the
harness exposes it as `JamNetwork::para_head` and every phase wait is written against it. The two
readings together are the assertion: a frozen head with a climbing local best is a stall, and both
climbing is a healthy para.

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

All four pass. Figures from the full-suite run of 2026-09-02 (sr25519 throughout, collator
identity read from the runtime):

| test | result | wall clock | effective block rate |
| --- | --- | --- | --- |
| `one_jam_collator_builds_blocks` | best 30 / finalized 27 | 311s | 6s median |
| `two_jam_collators_build_blocks` | best 30 / finalized 27 | 307s | 6s median |
| `three_jam_collators_build_blocks` | best 30 / finalized 26 | 310s | 6s median |
| `six_jam_collators_build_blocks` | best 30 / finalized 27 | 320s | 6s median |

`three_` and `six_` used to stall part-way: all collators submit to the same core, so they
multiplied the work-package contention that the genesis address bug had already made unreliable.
With the mesh intact they are no slower than the two-collator case. Every validator held
`6 peers (5 vals)` for the entire run, where before it decayed to `4 peers (3 vals)` within a few
minutes and never recovered.

The multi-collator three were also the ones that caught the authority-order bug above: they are
the only progress tests where the runtime's order of the set differs from the harness's, so they
sat at best 3 until `--collators` was given the runtime's order.

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
