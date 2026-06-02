-- #6790 pt 3: `transaction_index` in `transaction_hashes` / `logs` used to hold the
-- substrate extrinsic index; it now holds the 0-based EVM index. Hard cutoff: wipe receipt
-- caches plus the block mapping and sync checkpoints that gate re-extraction
-- (`insert_into_db` short-circuits on existing `eth_to_substrate_blocks` rows). One-time
-- backfill on next start; downstream indexers should re-sync transaction-indexed data.
DELETE FROM transaction_hashes;
DELETE FROM logs;
DELETE FROM eth_to_substrate_blocks;
DELETE FROM sync_state;
