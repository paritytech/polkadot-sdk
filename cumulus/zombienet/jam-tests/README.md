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

The demo runs the same code path with no assertion, until it is killed:

```sh
NUM_COLLATORS=2 cumulus/scripts/jam-collator-demo.sh
```

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

## Two things worth knowing about the chain spec

The parachain template's `development` preset has no `aura.authorities` — pallet-session drives
pallet-aura, so the authority set comes from `session.keys` and `collatorSelection.invulnerables`.
The harness rewrites both to exactly the collators it is about to start: an authority with no
running collator costs a full six-second slot of block production every time its turn comes round.

The parachain uses para id 0. Under the dev-genesis null authorizer parasim falls back to
`ParaId(0)`, so the collator has to build, submit and watch para 0 for the loop to close.

## The zombienet-sdk dependency

This crate pins zombienet-sdk to a revision of the unmerged `jam-integration` branch
([PR #573](https://github.com/paritytech/zombienet-sdk/pull/573)), which is what adds JAM networks
to the SDK. It deliberately does not share the `zombienet-sdk` version the rest of the workspace
uses, so `cumulus-zombienet-sdk-tests` and every existing zombienet test keep their released pin.
Revisit the pin when #573 merges.

### Why a relay chain

At the pinned revision a JAM-only network is not yet possible: the orchestrator unwraps the relay
chain config unconditionally, so building a network without one panics at spawn time. The harness
therefore starts a single idle relay validator that nothing in the test uses, which is the only
reason a `polkadot` binary is needed.

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
