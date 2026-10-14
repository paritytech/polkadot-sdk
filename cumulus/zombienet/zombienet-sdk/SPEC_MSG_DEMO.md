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
NO_PROXY=127.0.0.1,localhost RUST_LOG=info ZOMBIE_PROVIDER=native PATH=$PWD/target/release:$PATH \
  cargo test --release -p cumulus-zombienet-sdk-tests --features zombie-ci spec_msg_delivers -- --nocapture
```

- `RUST_LOG=info` is needed. A value such as `RUST_LOG=xcm=trace` hides the test output.
- `NO_PROXY` is needed only behind a system proxy.

The test prints five checkpoints. Each one is read from chain state:

| # | Checkpoint | What it shows |
|---|---|---|
| 1 | send | A 2000 block holds `PolkadotXcm::Sent`. Its digest commits a `StreamsRoot`: the `Provides`. |
| 2 | relay | `newest_included_provides(2000)` is that root. The relay recorded it. |
| 3 | consume | A 2001 block consumed the channel up to a leaf. This record is the candidate's `Requires`. |
| 4 | backing | That 2001 block is finalized, so the relay matched its `Requires` against 2000's provides. |
| 5 | execute | `MessageQueue::Processed` with origin `SpecMsg ((2000))` and success `true`. |

## 3. Follow the node logs

Open a second terminal in the same shell about 40 s after the start. The node log files appear then.
charlie is the sender collator (2000), dave the receiver collator (2001), alice a relay validator.

```
cd $ZOMBIENET_SDK_BASE_DIR
tail -F charlie/charlie.log dave/dave.log alice/alice.log | grep --line-buffered -E \
  "committed streams root|Emitting the|Archived block|New root|fetch settled|Messaging inherent|consumed|Collation carries|Recording provides|requires found"
```

## 4. One message in the logs

This is the message of checkpoints 1 to 5 in one run. `Block N` is the parachain block.

| Time | Node | Log | Step |
|---|---|---|---|
| 15:32:48.050 | charlie | `runtime::spec-msg: Block 16: committed streams root 0x6329…` | Send. The root is checkpoint 1. |
| 15:32:48.050 | charlie | `runtime::parachain-system: Emitting the Provides signal for root 0x6329…` | The root leaves as a `Provides` signal. |
| 15:32:48.056 | charlie | `spec-msg: Archived block #16 …: 1 sends, committing …` | The archive can serve the message. |
| 15:33:00.089 | alice | `runtime::inclusion: Recording provides root 0x6329… of para 2000` | Inclusion (checkpoint 2). 12 s after the send. |
| 15:33:00.119 | dave | `spec-msg: New root 0x6329… of 2000: fetching` | The receiver sees the root. |
| 15:33:00.128 | dave | `spec-msg: Root 0x6329… of 2000: fetch settled in 9.7ms` | Pull fetch from charlie's archive. |
| 15:33:00.128 | charlie | `spec-msg: Served a spec-msg exchange request from <peer>: 86 bytes` | The server side of that fetch. |
| 15:33:00.136 | dave | `spec-msg: Messaging inherent: 1 items` | The collator plans the inherent. |
| 15:33:00.148 | dave | `runtime::spec-msg: Block 17: consumed 1 payloads from 2000 on Channel { recipient: 2001, … }` | Consume (checkpoint 3). |
| 15:33:00.176 | dave | `cumulus-collator: Collation carries a Requires over the consumption of 1 blocks. consuming=1 signals=4` | The collator adds `Requires` to the candidate. |
| 15:33:06.016 | alice | `runtime::inclusion-inherent: Candidate 0x93f4… for para 2001: requires found in the provides windows` | The relay matches `Requires` (checkpoint 4). |

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
