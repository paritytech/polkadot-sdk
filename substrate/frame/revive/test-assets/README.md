# Test Assets

This directory contains test data files used for verifying the correctness of transaction and receipt root calculations in the Revive pallet.

## Test Data Sources

### Frontier RPC Node (Local Development)

Test data was collected from a local Frontier node with test transactions. This provides controlled test scenarios with known transaction types.

#### Setup Instructions

1. **Clone and build Frontier:**
   ```bash
   git clone https://github.com/polkadot-evm/frontier/
   cd frontier
   cargo build --release --bin frontier-template-node
   ```

2. **Launch Frontier node:**
   ```bash
   ./target/release/frontier-template-node \
     --chain=dev \
     --validator \
     --execution=Native \
     --no-telemetry \
     --no-prometheus \
     --sealing=instant \
     --no-grandpa \
     --force-authoring \
     -linfo,rpc=trace \
     --port=19931 \
     --rpc-port=8545 \
     --frontier-backend-type=key-value \
     --tmp \
     --unsafe-force-node-key-generation
   ```

3. **Submit test transactions:**
   ```bash
   # From polkadot-sdk repository
   cargo run -p pallet-revive-eth-rpc --example tx-types
   ```

4. **Collect test data:**
   ```bash
   # Collect minimal data (RLP only)
   cargo run -p pallet-revive-eth-rpc --example collect-test-data --block-number 1
   cargo run -p pallet-revive-eth-rpc --example collect-test-data --block-number 2

   # Collect full data including transaction and receipt details
   cargo run -p pallet-revive-eth-rpc --example collect-test-data --block-number 3 --with-transactions --with-receipts
   ```

### Ethereum networks

Test data was also collected from Ethereum Mainnet and Sepolia to verify compatibility with real-world data.

#### Collection Commands

```bash
# Historical blocks with different transaction types
cargo run -p pallet-revive-eth-rpc --example collect-test-data \
  --rpc-url https://ethereum-rpc.publicnode.com \
  --block-number 5094851

cargo run -p pallet-revive-eth-rpc --example collect-test-data \
  --rpc-url https://ethereum-rpc.publicnode.com \
  --block-number 22094877

cargo run -p pallet-revive-eth-rpc --example collect-test-data \
  --rpc-url https://eth-sepolia.public.blastapi.io \
  --block-number 8867251
```

## File Structure

Test data files follow the naming convention: `test_data_block_{block_number}.json`

Each file contains:
- **info**: Metadata about data collection (RPC URL, timestamp, requested block)
- **block_number**: The actual block number
- **block_hash**: Block hash
- **transactions_rlp**: Array of RLP-encoded transactions (always included)
- **transactions_root**: Expected transactions Merkle root
- **receipts_rlp**: Array of RLP-encoded receipts (always included)
- **receipts_root**: Expected receipts Merkle root
- **transactions**: Full transaction objects (optional, only with `--with-transactions`)
- **receipts**: Full receipt objects (optional, only with `--with-receipts`)

## Usage in Tests

The test data is used in `substrate/frame/revive/src/tests/trie_roots.rs` to verify:
1. Transaction root calculation using `EthBlockBuilder::compute_trie_root()`
2. Receipt root calculation using the same method
3. Compatibility between our implementation and Ethereum's trie structure

## Notes

- **File Size**: Using `--with-transactions --with-receipts` creates larger files but provides useful debugging information
- **RLP Data**: The RLP-encoded arrays are the essential data for root verification tests
- **Compatibility**: Test data from both Frontier and Ethereum mainnet ensures broad compatibility testing
