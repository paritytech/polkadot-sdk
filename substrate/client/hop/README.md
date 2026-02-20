# HOP Service

HOP (Hand-Off Protocol) is a standalone service crate for Substrate nodes that provides ephemeral data storage with an RPC for submitting data and Bitswap protocol for retrieval.

## Overview

HOP enables peer-to-peer data sharing when recipients are offline by providing:

- **In-memory data pool** with configurable size and retention period
- **Content-addressed storage** using Blake2-256 hashes
- **RPC** for data submission
- **Bitswap protocol** support for retrieval

## Use Cases

- Chat attachments when recipient is offline
- Collaborative document auto-save
- Temporary cache for blockchain data
- P2P data exchange before permanent storage

## Integration into Substrate Nodes

### 1. Add Dependency

Add to your node's `Cargo.toml`:

```toml
[dependencies]
hop-service = { path = "../hop-service" }
```

Or from the bulletin-chain workspace:

```toml
[dependencies]
hop-service = { workspace = true }
```

### 2. CLI Integration

Add HOP parameters to your node CLI:

```rust
use hop_service::HopParams;

#[derive(Debug, clap::Parser)]
pub struct Cli {
    #[clap(subcommand)]
    pub subcommand: Option<Subcommand>,

    #[clap(flatten)]
    pub run: sc_cli::RunCmd,

    #[clap(flatten)]
    pub hop: HopParams,
}
```

### 3. Service Initialization

Initialize the HOP pool in your service builder:

```rust
use hop_service::HopDataPool;
use std::sync::Arc;

// Initialize pool if enabled
let hop_pool = if hop_params.enable_hop {
    Some(Arc::new(HopDataPool::new(
        hop_params.hop_max_pool_size * 1024 * 1024,  // Convert MiB to bytes
        hop_params.hop_retention_blocks,
    )?))
} else {
    None
};

// Alternative: Use SDK-style .then() pattern for consistency with polkadot-omni-node
let hop_pool = hop_params.enable_hop.then(|| {
    HopDataPool::new(
        hop_params.hop_max_pool_size * 1024 * 1024,
        hop_params.hop_retention_blocks,
    )
    .map(Arc::new)
    .map_err(|e| ServiceError::Other(format!("Failed to create HOP pool: {}", e)))
}).transpose()?;
```

### 4. RPC Registration

Register HOP RPC methods:

```rust
use hop_service::{HopApiServer, HopRpcServer};

pub struct FullDeps<C, P, SC, B> {
    pub client: Arc<C>,
    pub pool: Arc<P>,
    pub hop_pool: Option<Arc<HopDataPool>>,
    // ... other fields
}

// In your RPC builder
if let Some(hop_pool) = deps.hop_pool {
    module.merge(HopRpcServer::new(hop_pool, deps.client.clone()).into_rpc())?;
}
```

## RPC Methods

### `hop_submit`

Submit data to the pool.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "hop_submit",
  "params": ["0x68656c6c6f"],
  "id": 1
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": "0x324dcf027dd4a30a932c441f365a25e86b173defa4b8e58948253471b81b72cf",
  "id": 1
}
```

### `hop_get` (only for v0, until we have Bitswap retrieval)

Retrieve data by hash (deletes after retrieval).

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "hop_get",
  "params": ["0x324dcf027dd4a30a932c441f365a25e86b173defa4b8e58948253471b81b72cf"],
  "id": 1
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": "0x68656c6c6f",
  "id": 1
}
```

### `hop_has`

Check if data exists in the pool.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "hop_has",
  "params": ["0x324dcf027dd4a30a932c441f365a25e86b173defa4b8e58948253471b81b72cf"],
  "id": 1
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": true,
  "id": 1
}
```

### `hop_poolStatus`

Get pool statistics.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "hop_poolStatus",
  "params": [],
  "id": 1
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "entryCount": 42,
    "totalBytes": 1048576,
    "maxBytes": 10737418240
  },
  "id": 1
}
```

## CLI Flags

When running your node with HOP enabled:

```bash
# Enable HOP with default settings (10 GiB pool, 24h retention)
./your-node --enable-hop

# Custom pool size (1 GiB)
./your-node --enable-hop --hop-max-pool-size 1024

# Custom retention (12 hours at 6s blocks = 7200 blocks)
./your-node --enable-hop --hop-retention-blocks 7200

# All options
./your-node \
  --enable-hop \
  --hop-max-pool-size 2048 \
  --hop-retention-blocks 14400 \
  --hop-check-interval 60
```

## Configuration

### Default Values

| Parameter | Default | Description |
|-----------|---------|-------------|
| `enable_hop` | `false` | Enable HOP service |
| `hop_max_pool_size` | `10240` MiB (10 GiB) | Maximum pool size |
| `hop_retention_blocks` | `14400` blocks (24h @ 6s) | Retention period |
| `hop_check_interval` | `60` seconds | Promotion check interval |

### Limits

- **Maximum data size**: 8 MiB (matches transaction-storage pallet)
- **Hash algorithm**: Blake2-256
- **Storage**: In-memory only (not persisted)

## Error Codes

| Code | Error | Description |
|------|-------|-------------|
| 1001 | DataTooLarge | Data exceeds 8 MiB limit |
| 1002 | PoolFull | Pool has reached capacity |
| 1003 | DuplicateEntry | Data already exists in pool |
| 1004 | NotFound | Data not found in pool |
| 1005 | EmptyData | Data cannot be empty |
| 1006 | Encoding | Encoding/decoding error |
| 1008 | InvalidHash | Hash must be 32 bytes |

## Architecture

### Components

- **`HopDataPool`**: Core in-memory storage with thread-safe access
- **`HopParams`**: CLI configuration parameters
- **`HopRpcServer`**: RPC interface implementation
- **`HopPoolEntry`**: Data structure for stored entries
- **`PoolStatus`**: Statistics and monitoring

### Thread Safety

The data pool uses `Arc<RwLock<HashMap>>` for thread-safe concurrent access:
- Multiple readers can access simultaneously
- Writers get exclusive access
- Atomic counters track pool size

### Data Flow

1. Client submits data via `hop_submit` RPC
2. Data is validated (size, duplicates, capacity)
3. Blake2-256 hash is computed
4. Entry stored in pool with expiration metadata
5. Hash returned to client
6. Client retrieves data via `hop_get` (deletes after retrieval)

## Omni-Node Integration

This crate is designed for easy integration into `polkadot-omni-node`. The integration pattern follows the SDK's Statement Store pattern:

```rust
// In omni-node NodeExtraArgs
pub struct NodeExtraArgs {
    #[command(flatten)]
    pub hop_params: hop_service::HopParams,
    // ... other fields
}

// In service builder
let hop_pool = node_extra_args.hop_params.enable_hop.then(|| {
    Arc::new(hop_service::HopDataPool::new(
        node_extra_args.hop_params.hop_max_pool_size * 1024 * 1024,
        node_extra_args.hop_params.hop_retention_blocks,
    ))
}).transpose()?;
```

## Testing

Run the test suite:

```bash
cargo test -p hop-service
```

Test with a running node:

```bash
# Start node with HOP enabled
./your-node --dev --enable-hop

# Submit data
curl -H "Content-Type: application/json" \
  -d '{"id":1, "jsonrpc":"2.0", "method":"hop_submit", "params":["0x68656c6c6f"]}' \
  http://localhost:9944

# Check status
curl -H "Content-Type: application/json" \
  -d '{"id":1, "jsonrpc":"2.0", "method":"hop_poolStatus"}' \
  http://localhost:9944
```

## License

GPL-3.0-or-later WITH Classpath-exception-2.0
