CREATE TABLE IF NOT EXISTS eth_to_substrate_blocks (
	ethereum_block_hash BLOB NOT NULL PRIMARY KEY,
	substrate_block_hash BLOB NOT NULL,
	block_number INTEGER NOT NULL,
	gas_limit BLOB NOT NULL,
	block_author BLOB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_substrate_block_hash ON eth_to_substrate_blocks (
	substrate_block_hash
);

CREATE INDEX IF NOT EXISTS idx_block_number ON eth_to_substrate_blocks (
	block_number
);
