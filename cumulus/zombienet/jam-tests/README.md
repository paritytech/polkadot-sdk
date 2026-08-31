# JAM collator tests

End-to-end tests for `polkadot-omni-node` collating a parachain on a JAM network.

Each test is self-contained. It spawns its own six-validator JAM network with zombienet-sdk,
registers the parasim service on it, starts N collators against it, and asserts the parachain
keeps producing and finalizing blocks. Nothing has to be running beforehand, and nothing outside
the test's own temporary work directory is touched.

## Prerequisites

From this repository:

```sh
cargo build --release -p polkadot-omni-node -p parachain-template-runtime
cargo build --release --bin polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker
```

The `polkadot` binary is not used by the test itself — see "Why a relay chain" below.

From the polkajam repository: the `polkajam` node binary and the `jamt` CLI.
From the parachain-service repository: the compiled `parasim-service.jam` blob.

## Running

```sh
export JAM_NODE_BIN=/path/to/polkajam/target/release/polkajam
export JAMT_BIN=/path/to/polkajam/target/release/jamt
export PARASIM_BLOB=/path/to/parachain-service/.../parasim-service.jam

cargo test -p cumulus-jam-zombienet-tests --features jam-ci --test tests \
	-- --test-threads 1 --nocapture jam::collator_progress
```

`--test-threads 1` is required: each test spawns seven JAM nodes plus its collators, and running
them concurrently would fight over CPU and make the six-second slot budget unrealistic.

`OMNI_NODE_BIN`, `RUNTIME_WASM` and `RELAY_NODE_BIN` override the `target/release` defaults. If
any artifact is missing the tests print what they need and pass without running — they never fail
for a reason unrelated to the collator.

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
	jam-parachain-spec.json   the collators' patched chain spec
	parasim-service.jam       the copy of the blob that was registered
	alice.log, bob.log, ...   one log per collator
	alice/, bob/, ...         one base path per collator
	zombienet/                the network zombienet spawned: jam0..jam5, jam-or, relay-filler
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
| `tests/jam/network.rs` | spawns the JAM network and registers parasim on it |
| `tests/jam/chain_spec.rs` | builds and patches the collators' chain spec |
| `tests/jam/collators.rs` | starts, supervises and tears down the collator processes |
| `tests/jam/rpc.rs` | the JAM node and collator RPC clients |
| `tests/jam/harness.rs` | one run: network, parasim, collators, assertions |
| `tests/jam/collator_progress.rs` | the 2, 3 and 6 collator tests |
| `tests/jam/demo.rs` | the same run with no assertion and no end |
| `demo.sh` | shell entry point for that demo |

## Two things worth knowing about the chain spec

The parachain template's `development` preset has no `aura.authorities` — pallet-session drives
pallet-aura, so the authority set comes from `session.keys` and `collatorSelection.invulnerables`.
The harness rewrites both to exactly the collators it is about to start: an authority with no
running collator costs a full six-second slot of block production every time its turn comes round.

The parachain uses para id 0. Under the dev-genesis null authorizer parasim falls back to
`ParaId(0)`, so the collator has to build, submit and watch para 0 for the loop to close.

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

### Current status of the three tests

All three pass against a zombienet-spawned network, once the SDK carries the genesis address fix:

| test | result | wall clock | effective block rate |
| --- | --- | --- | --- |
| `two_jam_collators_build_blocks` | best 30 / finalized 28 | 325s | 6s median |
| `three_jam_collators_build_blocks` | best 30 / finalized 26 | 301s | 6s median |
| `six_jam_collators_build_blocks` | best 30 / finalized 27 | 312s | 6s median |

`three_` and `six_` used to stall part-way: all collators submit to the same core, so they
multiplied the work-package contention that the genesis address bug had already made unreliable.
With the mesh intact they are no slower than the two-collator case. Across all three runs every
validator held `6 peers (5 vals)` for the entire run, where before it decayed to `4 peers
(3 vals)` within a few minutes and never recovered.

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
