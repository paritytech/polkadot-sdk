# Speculative messaging demo

`spec_msg_delivers_xcm_without_hrmp` (`tests/zombie_ci/spec_msg.rs`) sends one XCM between two penpal chains
(2000 and 2001) over spec-msg, with no HRMP channel. The network boots in about 2 minutes. The channel open, the
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

1. Warmup, about 2 minutes. It boots the network, waits until both parachains finalize a block, and then waits.
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
| 1 | channel | 2001 consumed 2000's stream, which carries the open signal. 2000 read 2001's `Ack` register: the channel is open. The log shows both watermarks, the register's position and the session. |
| 2 | send | A 2000 block holds `PolkadotXcm::Sent` with a message id. Its digest commits a `StreamsRoot`. |
| 3 | relay | `newest_included_provides(2000)` is that root. The relay recorded it. |
| 4 | consume | A 2001 block consumed up to the stream root the sent `StreamsRoot` commits: its `Requires`. |
| 5 | backing | That 2001 block is finalized, so the relay matched its `Requires` against 2000's provides. |
| 6 | execute | `MessageQueue::Processed` with the sent message id, origin `SpecMsg ((2000))` and success `true`. |

## 3. Follow the node logs

Open another terminal about 40 s after the start (with two steps: before step 2). Set `ZOMBIENET_SDK_BASE_DIR` to the
directory of the network (with two steps: the one of step 1). charlie is the sender collator (2000), dave the
receiver collator (2001), alice a relay validator.

```
cd $ZOMBIENET_SDK_BASE_DIR
tail -F charlie/charlie.log dave/dave.log alice/alice.log | grep --line-buffered -E \
  "committed streams root|Archived block|New root|Served a spec-msg|fetch settled|Messaging inherent|Collation carries|Recording provides|Dropping candidate"
```

## 4. One message in the logs

This is one run of the one-command flow. Node logs use local time. `Block N` is the parachain block. Roots are
shortened. Part A is the channel open (checkpoint 1). Part B is the message of checkpoints 2 to 6.

**A. Open**

The open takes two register reads: 2001 publishes `Accepting`, 2000 reads it and sends `Hello`, 2001 consumes
`Hello` and publishes `Opened`, and 2000's read of `Opened` opens the channel. `open_channel` sends nothing.

| Time | Node | Log | Step |
|---|---|---|---|
| 21:35:30.017 | dave | `runtime::spec-msg: Block 7: committed streams root 0xedb0… over 1 touched streams` | `accept_open_channel` on 2001: `Accepting` on its `Ack` stream. |
| 21:35:48.003 | alice | `runtime::inclusion: Recording provides root 0xedb0… of para 2001` | The relay records the root. |
| 21:35:48.011 | charlie | `spec-msg: Root 0xedb0… of 2001: fetch settled in 516µs` | 2000 reads the register (dave: `Served a spec-msg fetch request …: 34 bytes`). |
| 21:35:48.016 | charlie | `runtime::spec-msg: Block 10: committed streams root 0x5df3… over 1 touched streams` | 2000 applies `Accepting` and sends `Hello`. |
| 21:36:06.003 | alice | `runtime::inclusion: Recording provides root 0x5df3… of para 2000` | Inclusion of the `Hello` block. |
| 21:36:06.013 | dave | `spec-msg: Root 0x5df3… of 2000: fetch settled in 1.2ms` | 2001 fetches `Hello` (charlie: 21 bytes served). |
| 21:36:06.018 | dave | `runtime::spec-msg: Block 11: committed streams root 0xe114… over 1 touched streams` | 2001 consumes `Hello` and publishes `Opened`. |
| 21:36:18.004 | alice | `runtime::inclusion: Recording provides root 0xe114… of para 2001` | Inclusion of the `Opened` block. |
| 21:36:18.013 | charlie | `spec-msg: Root 0xe114… of 2001: fetch settled in 719µs` | 2000 reads `Opened`. |
| 21:36:18.020 | charlie | `cumulus-collator: Collation carries a Requires over the consumption of 1 blocks. consuming=1 signals=3` | 2000's block applies `Opened`: the channel is open (checkpoint 1). It sends nothing, so no `Provides`. |

**B. One message**

| Time | Node | Log | Step |
|---|---|---|---|
| 21:36:24.017 | charlie | `runtime::spec-msg: Block 14: committed streams root 0xe02a… over 1 touched streams` | Send. The root is checkpoint 2. |
| 21:36:24.019 | charlie | `spec-msg: Archived block #14 …: 1 sends, committing …` | The archive can serve the message. |
| 21:36:36.003 | alice | `runtime::inclusion: Recording provides root 0xe02a… of para 2000` | Inclusion (checkpoint 3), 12 s after the send. |
| 21:36:36.011 | dave | `spec-msg: New root 0xe02a… of 2000: fetching` | The receiver sees the root. |
| 21:36:36.012 | charlie | `spec-msg: Served a spec-msg fetch request from <peer>: 85 bytes` | The server side of the fetch, over `/spec-msg/fetch/1`. |
| 21:36:36.012 | dave | `spec-msg: Root 0xe02a… of 2000: fetch settled in 1.3ms` | Pull fetch from charlie's archive. |
| 21:36:36.013 | dave | `spec-msg: Messaging inherent: 1 streams` | The collator plans the inherent. |
| 21:36:36.017 | dave | `runtime::spec-msg: Block 16: committed streams root 0xd726… over 1 touched streams` | Consume (checkpoint 4). The root is 2001's updated `Ack` register. |
| 21:36:36.020 | dave | `cumulus-collator: Collation carries a Requires over the consumption of 1 blocks. consuming=1 signals=4` | The collator adds `Requires` to the candidate (checkpoints 5 and 6 follow from finality). |

- Send to consume takes 12 s: two relay blocks. A fetch takes under 2 ms.
- The relay logs nothing when `Requires` matches; it logs `Dropping candidate … not in source …'s provides
  window` when it does not. If delivery stalls, grep the relay logs for `Dropping candidate`.
- `signals=4` is the scheduling signals, `Provides` and `Requires`; `signals=3` has no `Provides`, because that block
  sent nothing.
- A runtime line can appear twice for one block: the runtime runs on author and on import, or the collator builds
  the block again on the same parent.
- Consumption itself is not logged at `debug`. The committed root and the collator's `Requires` line show it.
- Every consuming block publishes a new `Ack` register, so a second wave of lines follows in the other direction.
  It is not a second message. 2000's Block 15 is one more leaf on the channel, consumed by 2001's Block 17.
- Every few seconds dave serves a small request to charlie that is not in the tables: 2000's archive reads 2001's
  register under a finalized root, to prune what 2001 confirmed.

## Log targets

The test sets the node log filters in `build_network_config`, at `debug` level. Collators: `spec-msg`,
`cumulus-collator`, `runtime::spec-msg`, `runtime::parachain-system`. Relay validators: `runtime::inclusion`,
`runtime::inclusion-inherent`.

## Latency against HRMP

`spec_msg_vs_hrmp_latency -- --ignored --nocapture` replaces `spec_msg_delivers -- --nocapture`. It compares
spec-msg with HRMP on the same chains. It takes 10 samples per path by default: set `SPEC_MSG_SAMPLES=2` for a
short check (about 8 minutes). It boots its own network, so it does not use the two-step flow.
