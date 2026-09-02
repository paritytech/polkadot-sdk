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
From the parachain-service repository: the compiled `parasim-service.jam` and
`parachain-authorizer-sr25519.jam` blobs, and the `parasim-tool` CLI.

There is one authorizer blob per signature scheme, and which one a para needs is decided by its
runtime's `AuraId`. The parachain template is sr25519, so that is the blob this suite runs and the
scheme it passes `parasim-tool`. Nothing can check the pairing — a blob's scheme is not visible in
its bytes — so a mismatch shows up only as a core no collator ever authorizes on.

## Running

```sh
export JAM_NODE_BIN=/path/to/polkajam/target/release/polkajam
export JAMT_BIN=/path/to/polkajam/target/release/jamt
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
| `JAMT_BIN` | the `jamt` CLI, used once to register parasim |
| `PARASIM_TOOL_BIN` | the `parasim-tool` CLI, used to host the authorizer and assign cores |
| `PARASIM_BLOB` | `parasim-service.jam`, the service the collators talk to |
| `AUTHORIZER_BLOB` | `parachain-authorizer-sr25519.jam`, the AURA authorizer the cores run |
| `OMNI_NODE_BIN`, `RUNTIME_WASM`, `RELAY_NODE_BIN` | override the `target/release` defaults |
| `JAM_TEST_BASE_DIR` | keep every run's work dir under this directory |
| `NUM_COLLATORS` | how many collators the demo runs (default 1) |

Both blobs are copied into the run's work dir before their hash goes on chain: PVM builds are not
byte-deterministic, so a rebuild during a run would strand the registered hash without a
resolvable preimage. The collators are pointed at the authorizer *copy*, because an authorizer
hash is a hash of exactly those bytes.

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
	parasim-service.jam        the copy of the blob that was registered
	parachain-authorizer.jam   the copy of the blob whose hash the cores were assigned from
	alice.log, bob.log, ...    one log per collator
	alice/, bob/, ...          one base path per collator
	zombienet/                 the network zombienet spawned: jam0..jam5, jam-or, relay-filler
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
| `tests/jam/network.rs` | spawns the JAM network, registers parasim, hosts the authorizer, assigns the cores |
| `tests/jam/chain_spec.rs` | builds and patches one para's chain spec |
| `tests/jam/collators.rs` | starts, supervises and tears down one para's collator processes |
| `tests/jam/rpc.rs` | the JAM node and collator RPC clients |
| `tests/jam/harness.rs` | one run: network, parasim, cores, collators, assertions |
| `tests/jam/collator_progress.rs` | the 1, 2, 3 and 6 collator tests |
| `tests/jam/core_assignment.rs` | two paras at once, and cores taken away or moved mid-run |
| `tests/jam/demo.rs` | the same run with no assertion and no end |
| `demo.sh` | shell entry point for that demo |

## Three things worth knowing about the collators

The parachain template's `development` preset has no `aura.authorities` — pallet-session drives
pallet-aura, so the authority set comes from `session.keys` and `collatorSelection.invulnerables`.
The harness rewrites both to exactly the collators it is about to start: an authority with no
running collator costs a full six-second slot of block production every time its turn comes round.

The para id is the harness's, not the preset's. It is the id `parasim-tool assign-core <para>
<core>` writes into the authorizer config the core commits to, and the collator reads its own id
straight out of this spec, so the two have to agree — otherwise the collator computes an
authorizer hash no core holds and never finds a core to submit to. The existing tests all run
para 0.

A collator needs no key of its own for JAM. It signs work packages with the aura session key it
already claims slots with — `--alice` puts that in the keystore in memory — and it learns the
collator set from `AuraApi::authorities()` at startup, which is the same set the harness wrote
into `session.keys` above. So the only thing that has to be kept in step is `parasim-tool
--collators` and `--scheme`: they build the collator-set trie the authorizer hash commits to, and
naming a different set, a different order or a different curve installs a hash no collator will
ever match.

## What the JAM side is set up with, and in what order

`network.rs` does four things to the freshly spawned chain, and the order is not free. What fixes
it is one scarce resource: a **bootstrap instruction only rides a core that still holds the
genesis authorizer**, and that supply only ever shrinks. Assignment to parasim is one-way, and
`free-core` parks a core under the AURA authorizer rather than handing it back.

1. **`jamt create-service`** registers parasim under a fixed id. `jamt` builds its packages under
   the genesis authorizer, so this — and any `jamt` call added later — has to happen before any
   core is pointed at a para. It passes `--force-core 0` rather than letting `jamt` pick a core at
   random, which on a two-core chain is a coin flip.
2. **`parasim-tool deploy-authorizer`** solicits the AURA authorizer blob into the bootstrap
   service and provides it. Validators fetch authorizer code by preimage lookup, so a core pointed
   at a code hash nobody hosts authorizes nothing and says nothing about why. The tool waits until
   the preimage is readable at a finalized block, so the deploy is complete before step 3 starts.
3. **`parasim-tool assign-core`, for the first para only**, riding the very core it assigns.
   Something has to travel the bootstrap lane before any AURA core exists, and this is it.
4. **`parasim-tool grant-assigner <core>`, for every para's core**, handing each core's assigner
   privilege to parasim — which is what lets a later `free-core` or re-assignment travel the
   control lane inside an AURA package. A grant is a bootstrap instruction that picks its own
   carrier (it has no `--via-*` flags), so it must run while cores still holding the genesis
   authorizer are left to carry it. Granting does not assign: the queue is written back unchanged,
   so a core granted here still holds the genesis authorizer and can still carry the grants after
   it.
5. **`parasim-tool assign-core --via-core …`, for the remaining paras.** Their cores answer to
   parasim now, so these take the control lane and need a carrier running an AURA authorizer —
   the first para's core, named with `--via-core`/`--via-para`/`--via-collators`.

A tiny network has exactly two cores: polkajam ties `core_count` to the validator count (six
validators, three per core) and the next step up is 78 validators. So two paras is the most this
harness can run. Granting every core before the last one is assigned is what lets it do so with
no core left stranded under service 0 — the earlier assign/grant/assign/grant order ran the
genesis lane dry and left the last core unfreeable for the rest of the run.

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
`--via-core`/`--via-para`/`--via-collators` are for: the setup's step 5 above, and nothing in the
tests. The tool checks the carrier it builds against the hash the carrier core actually holds and
refuses to submit on a mismatch, naming both hashes — so a wrong carrier is a loud failure, not a
core that quietly authorizes nothing.

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

### Current status of the core tests

All three pass, and the two runs of the stall test reached the same head numbers at the same
points, so the timings below are what a healthy stack does rather than one lucky run:

| test | wall clock | what it observed |
| --- | --- | --- |
| `two_paras_on_two_cores_build_blocks` | 328s | both paras best 30 / finalized 27, JAM head #28 each, neither collator knowing the other's head |
| `freeing_the_core_freezes_the_para_head_until_it_is_assigned_again` | 386s | six more heads drained through after the free, then frozen at #12 for 90s while nine blocks were authored and six re-rooted; #13 within 72s of the re-assign |
| `moving_the_para_to_the_other_core_keeps_its_head_moving` | 356s | head #5 to #28 with no pause; the collator saw both cores and stayed on core 0, and submitted to core 1 42s (seven JAM slots, the pool drain) after core 0 was freed |

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
