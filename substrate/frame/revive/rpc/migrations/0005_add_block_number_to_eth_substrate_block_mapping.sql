-- Rows written before this migration keep a NULL block number.
ALTER TABLE eth_to_substrate_blocks ADD COLUMN block_number INTEGER;

CREATE INDEX IF NOT EXISTS idx_eth_to_substrate_blocks_block_number ON eth_to_substrate_blocks (
	block_number
);
