# Statement Store Latency Benchmark

CLI tool for benchmarking statement store latency at scale. Clients form a ring topology where each subscribes to statements from the next client, measuring propagation latency across the network.

## Building

```bash
cargo build --release -p statement-latency-bench
```

## Usage

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

## CLI Arguments

| Argument               | Description                                         | Default |
| ---------------------- | --------------------------------------------------- | ------- |
| `--rpc-endpoints`      | Comma-separated WebSocket URLs (required)           | -       |
| `--num-clients`        | Number of clients to spawn                          | 100     |
| `--messages-pattern`   | Message pattern "count:size" (e.g., "5:512,3:1024") | "5:512" |
| `--num-rounds`         | Number of benchmark rounds                          | 1       |
| `--interval-ms`        | Interval between rounds (ms)                        | 10000   |
| `--receive-timeout-ms` | Timeout for receiving messages (ms)                 | 5000    |

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
Benchmark Results: send_min=0.045s send_avg=0.123s send_max=0.234s receive_min=2.134s receive_avg=3.456s receive_max=5.678s latency_min=2.234s latency_avg=3.567s latency_max=5.789s
```
