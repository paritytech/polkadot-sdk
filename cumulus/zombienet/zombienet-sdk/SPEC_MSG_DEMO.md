# Speculative messaging demo

`spec_msg_delivers_xcm_without_hrmp` (`tests/zombie_ci/spec_msg.rs`) sends one XCM between two penpal chains
(2000 and 2001) over spec-msg. The chains have no HRMP channel. The run takes about 4 minutes.

## 1. Build

Build the binaries once. The penpal and relay runtimes are inside them, so build again after a runtime change.

```
cargo build --release -p polkadot -p polkadot-parachain-bin
```

## 2. Run

Use a new base directory for every run. An old directory keeps its chain state (for example an HRMP channel from
the latency test), and the first check then fails.

```
export ZOMBIENET_SDK_BASE_DIR=$(mktemp -d)
export NO_PROXY=127.0.0.1,localhost RUST_LOG=info ZOMBIE_PROVIDER=native PATH=$PWD/target/release:$PATH
```

- `RUST_LOG=info` is needed. A value such as `RUST_LOG=xcm=trace` hides the test output.
- `NO_PROXY` is needed only behind a system proxy.

### One command

```
cargo test --release -p cumulus-zombienet-sdk-tests --features zombie-ci spec_msg_delivers -- --nocapture
```

### Two steps, for a live presentation

Step 1 is the warmup. It boots the relay chain and both parachains, and waits until both parachains finalize a
block. This takes about 4 minutes. Start it before the talk, in a first terminal:

```
SPEC_MSG_STANDBY=1 cargo test --release -p cumulus-zombienet-sdk-tests --features zombie-ci spec_msg_delivers -- --nocapture
```

The test prints `Standby ready: 2000 and 2001 are producing blocks` and then waits. Ctrl-C stops the nodes.

Step 2 is the demo. It connects to the running nodes (RPC ports 30510 for alice, 30511 for 2000, 30512 for 2001),
opens and accepts the channel, sends the XCM, and prints the six checkpoints. This takes about 2 minutes. Run it in
a second terminal with the same `export` lines:

```
SPEC_MSG_ATTACH=1 cargo test --release -p cumulus-zombienet-sdk-tests --features zombie-ci spec_msg_delivers -- --nocapture
```

- Use the same `ZOMBIENET_SDK_BASE_DIR` in both terminals, so the node logs are in a known place (section 3).
- One standby network serves one step 2. A second run fails at the channel open, because the channel exists.
  Stop and restart step 1 before each run.
- Run the `cargo test` command once before the talk, so the test crate is already compiled.

The test prints six checkpoints. Each one is read from chain state:

| # | Checkpoint | What it shows |
|---|---|---|
| 1 | channel | 2001 consumed `OpenChannel`. 2000 read 2001's `Ack` register: the channel is `Open`. |
| 2 | send | A 2000 block holds `PolkadotXcm::Sent`. Its digest commits a `StreamsRoot`: the `Provides`. |
| 3 | relay | `newest_included_provides(2000)` is that root. The relay recorded it. |
| 4 | consume | A 2001 block consumed the channel up to a leaf. This record is the candidate's `Requires`. |
| 5 | backing | That 2001 block is finalized, so the relay matched its `Requires` against 2000's provides. |
| 6 | execute | `MessageQueue::Processed` with origin `SpecMsg ((2000))` and success `true`. |

## 3. Follow the node logs

Open another terminal in the same shell about 40 s after the start (in two steps: before step 2). The node log files
appear then.
charlie is the sender collator (2000), dave the receiver collator (2001), alice a relay validator.

```
cd $ZOMBIENET_SDK_BASE_DIR
tail -F charlie/charlie.log dave/dave.log alice/alice.log | grep --line-buffered -E \
  "committed streams root|Emitting the|Archived block|New root|fetch settled|Messaging inherent|consumed|Collation carries|Recording provides|requires found"
```

## 4. One message in the logs

This is the message of checkpoints 2 to 6 in one run. `Block N` is the parachain block.

| Time | Node | Log | Step |
|---|---|---|---|
| 15:32:48.050 | charlie | `runtime::spec-msg: Block 16: committed streams root 0x6329…` | Send. The root is checkpoint 2. |
| 15:32:48.050 | charlie | `runtime::parachain-system: Emitting the Provides signal for root 0x6329…` | The root leaves as a `Provides` signal. |
| 15:32:48.056 | charlie | `spec-msg: Archived block #16 …: 1 sends, committing …` | The archive can serve the message. |
| 15:33:00.089 | alice | `runtime::inclusion: Recording provides root 0x6329… of para 2000` | Inclusion (checkpoint 3). 12 s after the send. |
| 15:33:00.119 | dave | `spec-msg: New root 0x6329… of 2000: fetching` | The receiver sees the root. |
| 15:33:00.128 | dave | `spec-msg: Root 0x6329… of 2000: fetch settled in 9.7ms` | Pull fetch from charlie's archive. |
| 15:33:00.128 | charlie | `spec-msg: Served a spec-msg exchange request from <peer>: 86 bytes` | The server side of that fetch. |
| 15:33:00.136 | dave | `spec-msg: Messaging inherent: 1 items` | The collator plans the inherent. |
| 15:33:00.148 | dave | `runtime::spec-msg: Block 17: consumed 1 payloads from 2000 on Channel { recipient: 2001, … }` | Consume (checkpoint 4). |
| 15:33:00.176 | dave | `cumulus-collator: Collation carries a Requires over the consumption of 1 blocks. consuming=1 signals=4` | The collator adds `Requires` to the candidate. |
| 15:33:06.016 | alice | `runtime::inclusion-inherent: Candidate 0x93f4… for para 2001: requires found in the provides windows` | The relay matches `Requires` (checkpoint 5). |

- Send to consume takes 12 s: two relay blocks. The fetch takes 1 to 10 ms.
- The same runtime line can appear several times for one block: the runtime runs on author and on import.
  Other 2000 and 2001 blocks in the log (blocks 8, 10, 19) are the channel open and accept messages.
- The relay line names the candidate by hash. The candidate `0x93f4…` is dave's block 17 candidate: the relay
  log shows it seconded at 15:33:00.453, and the match comes 6 s later.
- The register `Ack` consumption is logged at `trace`, because every block republishes it.
- If delivery stalls, grep the relay logs for `Dropping candidate`. It names the missing root.

## Log targets

The test sets the filters in `build_network_config`. Collators: `spec-msg`, `cumulus-collator`,
`runtime::spec-msg`, `runtime::parachain-system`. Relay validators: `runtime::inclusion`,
`runtime::inclusion-inherent`.

## Latency against HRMP

`spec_msg_vs_hrmp_latency -- --ignored --nocapture` replaces `spec_msg_delivers -- --nocapture`. It compares
spec-msg with HRMP on the same chains.
