# Price Oracle – Local Zombienet Test

## Prerequisites

1. **Build the `polkadot` binary** (includes the Westend runtime with the price oracle pallet):

```bash
cargo build -p polkadot --release
```

This produces three binaries in `target/release/`:
- `polkadot`
- `polkadot-prepare-worker`
- `polkadot-execute-worker`

2. **Install zombienet** (if not already):

Download the latest binary for your platform from
https://github.com/paritytech/zombienet/releases and place it in your `PATH`.

```bash
# Example for macOS arm64
curl -L -o zombienet https://github.com/paritytech/zombienet/releases/latest/download/zombienet-macos-arm64
chmod +x zombienet
sudo mv zombienet /usr/local/bin/
```

## Launch the Network

From the repo root:

```bash
zombienet --provider native spawn polkadot/zombienet_tests/price-oracle/price-oracle.toml
```

This starts **6 validators** (alice, bob, charlie, dave, eve, ferdie) running
`westend-local` with price oracle debug logging enabled.

## What to Observe

### Terminal output

Zombienet prints the RPC endpoints for each node. Alice is pinned to `ws://127.0.0.1:9944`,
Bob to `ws://127.0.0.1:9955`.

### Node logs (look for these lines)

```
🔮 Price oracle service started on protocol /…/price-oracle/1
Fetched DOT/USD price: 4.206000000000000000
Gossipped nudge to 5 peers
Block author: onchain=0, cached=4.206…, direction=Up, needed=420, selected=5
Price oracle: 5 ups, 0 downs, price 0 -> 0.050000000000000000
```

The price starts at 0 and climbs toward the real DOT/USD price over several blocks.

### Query the on-chain price via RPC

```bash
./polkadot/zombienet_tests/price-oracle/check-price.sh http://127.0.0.1:9944
```

Or use Polkadot.js Apps:
1. Open https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:9944
2. Developer → Chain State → PriceOracle → currentPrice

### Run the automated zombienet assertions

```bash
zombienet --provider native test polkadot/zombienet_tests/price-oracle/price-oracle.zndsl
```

## Configuration

The zombienet config (`price-oracle.toml`) sets:

| Parameter | Value | Meaning |
|-----------|-------|---------|
| chain | `westend-local` | Westend testnet runtime with the price oracle pallet |
| validators | 6 | alice, bob, charlie, dave, eve, ferdie |
| log filters | `runtime::price-oracle=debug, price-oracle=debug` | Debug logs for both the pallet (runtime side) and the gossip service (node side) |
| alice RPC | `9944` | Direct RPC access for querying state |
| bob RPC | `9955` | Second RPC endpoint for comparison |

## Runtime Parameters

These are set in `polkadot/runtime/westend/src/lib.rs`:

| Parameter | Value | Meaning |
|-----------|-------|---------|
| `Epsilon` | `0.01` | Each net nudge changes the price by $0.01 |
| `MinNudges` | `0` | Blocks are valid even without oracle inherents |
| `NudgeValidity` | `10` slots | Nudges older than 10 BABE slots are pruned |

## Troubleshooting

**"polkadot: command not found"** → Make sure `target/release/` is in your `PATH`:
```bash
export PATH="$PWD/target/release:$PATH"
```

**Price stays at 0** → Check logs for "Failed to fetch price". The node needs
internet access to reach Binance/CoinLore/CryptoCompare APIs.

**"Notification stream ended"** → The gossip protocol disconnected. Check that all
validators are peered (look for "Peer connected" log lines).
