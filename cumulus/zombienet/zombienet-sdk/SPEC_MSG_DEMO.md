# Speculative messaging demo

`spec_msg_delivers_xcm_without_hrmp` (`tests/zombie_ci/spec_msg.rs`) sends one XCM between two penpal chains
(2000 and 2001) over spec-msg, with no HRMP channel. The network boots in about 4 minutes. The channel open, the
send and the checkpoints take about 1.5 more minutes.

## 1. Build

Build once. The runtimes are inside the binaries, so build again after a runtime or node change.

```
cargo build --release -p polkadot -p polkadot-parachain-bin
```

## 2. Run

The test needs this environment:

```
export NO_PROXY=127.0.0.1,localhost RUST_LOG=info ZOMBIE_PROVIDER=native PATH=$PWD/target/release:$PATH
```

Set `RUST_LOG=info`. An inherited value such as `RUST_LOG=xcm=trace` overrides the test's default and hides the
checkpoint lines. `NO_PROXY` is needed only behind a system proxy.

A run that boots a network also needs a new base directory: an old one keeps its chain state.

```
export ZOMBIENET_SDK_BASE_DIR=$(mktemp -d)
```

**One command** boots the network and runs the test:

```
cargo test --release -p cumulus-zombienet-sdk-tests --features zombie-ci spec_msg_delivers -- --nocapture
```

**Two steps**, for a talk:

1. Warmup, about 4 minutes. It boots the network, waits until both parachains finalize a block, and then waits.
   Ctrl-C stops the nodes.

   ```
   SPEC_MSG_STANDBY=1 cargo test --release -p cumulus-zombienet-sdk-tests --features zombie-ci spec_msg_delivers -- --nocapture
   ```

2. Demo, about 1.5 minutes, in a second terminal. It connects to the running nodes, opens and accepts the channel,
   sends the XCM, and prints the six checkpoints. Run the environment `export` again. Step 2 starts no nodes, so
   it does not use `ZOMBIENET_SDK_BASE_DIR`: only the log commands in section 3 do.

   ```
   SPEC_MSG_ATTACH=1 cargo test --release -p cumulus-zombienet-sdk-tests --features zombie-ci spec_msg_delivers -- --nocapture
   ```

- Step 2 can run again on the same network: it skips the open and sends another message. Restart step 1 before
  a talk, so the audience sees the channel open. After a test-only change, keep step 1 running.
- Run the `cargo test` command once before the talk, so the test crate is compiled.

The test prints six checkpoints. Each one is read from chain state:

| # | Checkpoint | What it shows |
|---|---|---|
| 1 | channel | 2001 consumed 2000's stream, which carries the open signal. 2000 read 2001's `Ack` register: the channel is `Open`. |
| 2 | send | A 2000 block holds `PolkadotXcm::Sent`. Its digest commits a `StreamsRoot`: the `Provides`. |
| 3 | relay | `newest_included_provides(2000)` is that root. The relay recorded it. |
| 4 | consume | A 2001 block consumed the channel up to a leaf. This record is the candidate's `Requires`. |
| 5 | backing | That 2001 block is finalized, so the relay matched its `Requires` against 2000's provides. |
| 6 | execute | `MessageQueue::Processed` with origin `SpecMsg ((2000))` and success `true`. |

## 3. Follow the node logs

Open another terminal about 40 s after the start (with two steps: before step 2). Set `ZOMBIENET_SDK_BASE_DIR` to the
directory of the network (with two steps: the one of step 1). charlie is the sender collator (2000), dave the
receiver collator (2001), alice a relay validator.

```
cd $ZOMBIENET_SDK_BASE_DIR
tail -F charlie/charlie.log dave/dave.log alice/alice.log | grep --line-buffered -E \
  "committed streams root|Emitting the|Archived block|New root|Served a spec-msg|fetch settled|Messaging inherent|consumed|Collation carries|Recording provides|requires found"
```

## 4. One message in the logs

This is one run of the two-step flow. `Block N` is the parachain block. Roots are shortened. Part A is the channel
open and accept (checkpoint 1). Part B is the message of checkpoints 2 to 6.

**A. Open and accept**

| Time | Node | Log | Step |
|---|---|---|---|
| 18:24:18.049 | charlie | `runtime::spec-msg: Block 10: committed streams root 0xed86… over 1 streams` | `open_channel` on 2000: the open signal is the first leaf of the stream. |
| 18:24:18.049 | dave | `runtime::spec-msg: Block 10: committed streams root 0xb892… over 1 streams` | `accept_open_channel` on 2001: its first `Ack` register. |
| 18:24:42.018 | alice | `runtime::inclusion: Recording provides root 0xed86… of para 2000` | The relay records both roots (0xb892… of para 2001 too). |
| 18:24:42.034 | dave | `spec-msg: New root 0xed86… of 2000: fetching` | 2001 sees the open signal and fetches it. |
| 18:24:42.036 | charlie | `spec-msg: New root 0xb892… of 2001: fetching` | 2000 fetches the register. |
| 18:24:42.047 | dave | `runtime::spec-msg: Block 12: consumed 1 payloads from 2000 on Channel { recipient: 2001, … }` | 2001 consumes the open signal. |
| 18:24:42.052 | charlie | `cumulus-collator: Collation carries a Requires over the consumption of 1 blocks. consuming=1 signals=3` | 2000 consumed the register (no `consumed` line, see below). The channel is `Open` (checkpoint 1). |
| 18:24:48.018 | alice | `runtime::inclusion-inherent: Candidate 0xbdba… for para 2001: requires found in the provides windows` | The relay matches both candidates (para 2000 too). |

**B. One message**

| Time | Node | Log | Step |
|---|---|---|---|
| 18:24:48.041 | charlie | `runtime::spec-msg: Block 13: committed streams root 0x45a3… over 1 streams` | Send. The root is checkpoint 2. |
| 18:24:48.041 | charlie | `runtime::parachain-system: Emitting the Provides signal for root 0x45a3…` | The root leaves as a `Provides` signal. |
| 18:24:48.044 | charlie | `spec-msg: Archived block #13 …: 1 sends, committing …` | The archive can serve the message. |
| 18:25:00.018 | alice | `runtime::inclusion: Recording provides root 0x45a3… of para 2000` | Inclusion (checkpoint 3). 12 s after the send. |
| 18:25:00.030 | dave | `spec-msg: New root 0x45a3… of 2000: fetching` | The receiver sees the root. |
| 18:25:00.031 | charlie | `spec-msg: Served a spec-msg exchange request from <peer>: 86 bytes` | The server side of the fetch. |
| 18:25:00.032 | dave | `spec-msg: Root 0x45a3… of 2000: fetch settled in 2.1ms` | Pull fetch from charlie's archive. |
| 18:25:00.036 | dave | `spec-msg: Messaging inherent: 1 items` | The collator plans the inherent. |
| 18:25:00.041 | dave | `runtime::spec-msg: Block 15: consumed 1 payloads from 2000 on Channel { recipient: 2001, … }` | Consume (checkpoint 4). |
| 18:25:00.046 | dave | `cumulus-collator: Collation carries a Requires over the consumption of 1 blocks. consuming=1 signals=4` | The collator adds `Requires` to the candidate. |
| 18:25:06.005 | alice | `runtime::inclusion-inherent: Candidate 0xcb6d… for para 2001: requires found in the provides windows` | The relay matches `Requires` (checkpoint 5). |

- Send to consume takes 12 s: two relay blocks. The fetch takes 1 to 10 ms. Consume to match takes 6 s: one relay
  block.
- A runtime line can appear twice for one block: the runtime runs on author and on import, or the collator builds
  the block again on the same parent. Here both Block 10 lines (open and accept) came twice, 12 s apart, so
  their inclusion took 24 s instead of 12 s.
- The relay line names the candidate by hash. The collator logs do not show the hash.
- The register `Ack` consumption is logged at `trace`, because every block republishes it. So 2000's consumption
  of the accept register has no `consumed` line.
- Every consuming block also publishes a new `Ack` register root (Block 15 commits `0x2b12…`). A second wave of
  the same lines follows 12 s later, in the other direction. It is not a second message.
- If delivery stalls, grep the relay logs for `Dropping candidate`. It names the missing root.

## Log targets

The test sets the node log filters in `build_network_config`, at `debug` level. Collators: `spec-msg`,
`cumulus-collator`, `runtime::spec-msg`, `runtime::parachain-system`. Relay validators: `runtime::inclusion`,
`runtime::inclusion-inherent`.

## Latency against HRMP

`spec_msg_vs_hrmp_latency -- --ignored --nocapture` replaces `spec_msg_delivers -- --nocapture`. It compares
spec-msg with HRMP on the same chains. It takes 10 samples per path by default: set `SPEC_MSG_SAMPLES=2` for a
short check (about 8 minutes). It boots its own network, so it does not use the two-step flow.
