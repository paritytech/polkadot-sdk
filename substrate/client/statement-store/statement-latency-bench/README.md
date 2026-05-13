# Statement Store Latency Benchmark

CLI tools for benchmarking statement store latency at scale. The ring-topology binary
(`statement-latency-bench`) measures aggregate cohort latency; the per-node binary
(`statement-ops-bench`) measures individual RPC operations on specific nodes.

This crate produces three binaries:
- **`setup-allowances`** — one-shot provisioning of on-chain statement allowances via Sudo
- **`statement-latency-bench`** — cohort/ring-topology latency benchmark
- **`statement-ops-bench`** — per-node operation benchmark (submit / propagation / subscribe / loop)

## Building

```bash
cargo build --release -p statement-latency-bench
```

## Setup: Statement Allowances

Before running the benchmark, each account needs an on-chain statement allowance. Run this once
(or whenever you change `--num-clients`):

```bash
setup-allowances \
  --rpc-endpoints ws://localhost:9944 \
  --sudo-seed "//Alice" \
  --num-clients 100
```

This submits `Sudo(batch_all(set_storage(...)))` transactions to write allowances for all
deterministic benchmark accounts, then verifies each allowance exists at the finalized block.

### setup-allowances Arguments

| Argument                 | Description                                      | Default   |
| ------------------------ | ------------------------------------------------ | --------- |
| `--rpc-endpoints`        | Comma-separated WebSocket URLs (required)        | -         |
| `--sudo-seed`            | Sudo seed/SURI, e.g. "//Alice" (required)        | -         |
| `--num-clients`          | Number of accounts to provision                  | 100       |
| `--allowance-batch-size` | Accounts per `set_storage` call                  | 100       |
| `--allowance-max-count`  | Max statements allowed per account               | 100000    |
| `--allowance-max-size`   | Max total statement bytes per account            | 1000000   |
| `--max-batch-calls`      | Max calls per `batch_all` transaction            | 100       |

## Running the Benchmark

Basic example:

```bash
statement-latency-bench \
  --rpc-endpoints ws://localhost:9944,ws://localhost:9945 \
  --num-clients 10 \
  --messages-pattern "5:512"
```

Multi-round with custom settings:

```bash
statement-latency-bench \
  --rpc-endpoints ws://node1:9944,ws://node2:9944 \
  --num-clients 100 \
  --num-rounds 10 \
  --interval-ms 5000 \
  --messages-pattern "5:512,1:5120"
```

### statement-latency-bench Arguments

| Argument                | Description                                         | Default |
| ----------------------- | --------------------------------------------------- | ------- |
| `--rpc-endpoints`       | Comma-separated WebSocket URLs (required)           | -       |
| `--num-clients`         | Number of clients to spawn                          | 100     |
| `--messages-pattern`    | Message pattern "count:size" (e.g., "5:512,3:1024") | "5:512" |
| `--num-rounds`          | Number of benchmark rounds                          | 1       |
| `--interval-ms`         | Interval between rounds (ms)                        | 10000   |
| `--receive-timeout-ms`  | Timeout for receiving messages (ms)                 | 5000    |
| `--statement-expiry-ms` | Statement expiry time (ms)                          | 600000  |
| `--skip-sync`           | Skip time synchronization (for local testing)       | false   |

## How It Works

1. Clients are distributed round-robin across RPC endpoints
2. Each client sends statements with unique topics
3. Each client subscribes to statements from the next client in the ring
4. Latency is measured from submission to receipt via subscription

## Output

Results are logged with min/avg/max statistics for:
- Send duration
- Receive duration
- Full latency

Example output:
```
Benchmark Results: send_min=0.045s send_avg=0.123s send_max=0.234s receive_min=2.134s receive_avg=3.456s
receive_max=5.678s latency_min=2.234s latency_avg=3.567s latency_max=5.789s
```

## Per-Node Operation Benchmark (`statement-ops-bench`)

`statement-ops-bench` measures individual statement-store RPC operations on specific nodes.

Each subcommand requires a signing key with an on-chain statement allowance.

### `submit` — per-node submit duration

Sequentially submits N statements to each endpoint, with a distinct channel per statement
and a strictly increasing expiry. Reports per-node min/avg/max.

```bash
statement-ops-bench submit \
  --rpc-endpoints ws://node1:9944,ws://node2:9944 \
  --iterations 100 \
  --message-size 512 \
  --seed "your account seed"
```

### `propagation` — submit→subscribe latency for each pair

For every (submit-endpoint, subscribe-endpoint) pair in the Cartesian product, opens
**two separate** ws connections (one per side), submits a statement, and measures the
time until the subscription receives it. Same-node pairs (submit and subscribe on the
same node, on independent connections) are included.

```bash
statement-ops-bench propagation \
  --submit-endpoints ws://node1:9944,ws://node2:9944 \
  --subscribe-endpoints ws://node1:9944,ws://node3:9944 \
  --iterations 10 \
  --message-size 512 \
  --seed "your account seed"
```

### `subscribe` — per-node retrieval latency

For each endpoint, ensures a matching seed statement exists and then opens
`--reads-per-node` subscriptions filtered to its topic. Latency is measured from
subscribe-open to receipt of the initial dump containing the seed.

Seed handling:
- **No `--topic`**: the topic is derived per run and is guaranteed unique, so
  the seed is always submitted.
- **With `--topic`**: the seed step is **skipped** (`seed=NotSeeded` in the log);
  the read step then succeeds only if a matching statement is already in the
  store (or arrives live within the drain timeout), and otherwise times out
  cleanly with `first_error="Timed out waiting..."`.

```bash
statement-ops-bench subscribe \
  --rpc-endpoints ws://node1:9944,ws://node2:9944 \
  --reads-per-node 10 \
  --message-size 512 \
  --seed "your account seed"

# Read an existing statement under a known topic without writing a new one
# (no seed is ever submitted when --topic is set; reads time out if nothing
# matches):
statement-ops-bench subscribe \
  --rpc-endpoints ws://node1:9944 \
  --reads-per-node 10 \
  --topic 0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef \
  --seed "your account seed"
```

### `loop` — periodic execution

Periodically runs `submit`, `propagation`, and `subscribe` across the provided
endpoints (used as both submit and subscribe sides for propagation). Stops when
`--iterations`, `--duration-secs`, or Ctrl-C arrives first.

```bash
statement-ops-bench loop \
  --rpc-endpoints ws://node1:9944,ws://node2:9944 \
  --interval-secs 30 \
  --iterations 10 \
  --submit-iterations 50 \
  --propagation-iterations 5 \
  --reads-per-node 5 \
  --seed "your account seed"
```

### Output format

Per-endpoint and per-pair lines are logged at INFO level. Examples:
```
submit endpoint=ws://node1:9944 ok=100 fail=0 min=0.0042s avg=0.0061s max=0.0123s n=100
propagation submit_endpoint=ws://node1:9944 subscribe_endpoint=ws://node3:9944 ok=10 fail=0 prop_min=0.012s prop_avg=0.034s prop_max=0.087s n=10 submit_min=0.004s submit_avg=0.005s submit_max=0.007s submit_n=10
subscribe endpoint=ws://node1:9944 ok=10 fail=0 min=0.003s avg=0.005s max=0.011s n=10 seed=Submitted
```
