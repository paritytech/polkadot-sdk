# Statement Store Latency Benchmark

CLI tool for benchmarking statement store latency at scale. Clients form a ring topology where each subscribes
to statements from the next client, measuring propagation latency across the network.

This crate produces two binaries:
- **`setup-allowances`** — one-shot provisioning of on-chain statement allowances via Sudo
- **`statement-latency-bench`** — the actual latency benchmark (can be re-run without re-provisioning)

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
