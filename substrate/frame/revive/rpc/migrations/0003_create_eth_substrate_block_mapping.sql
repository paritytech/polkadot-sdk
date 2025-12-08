CREATE TABLE IF NOT EXISTS eth_to_substrate_blocks (
	ethereum_block_hash BLOB NOT NULL PRIMARY KEY,
	substrate_block_hash BLOB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_substrate_block_hash ON eth_to_substrate_blocks (
	substrate_block_hash
);
