-- Receipts and signed transactions computed when a block is indexed, served without asking the
-- node again. `receipt` is the JSON encoded receipt with its logs left out: they are read from
-- the `logs` table. `signed_tx` is the RLP encoded signed transaction.
CREATE TABLE IF NOT EXISTS receipts (
	block_hash BLOB NOT NULL,
	transaction_index INTEGER NOT NULL,
	receipt BLOB NOT NULL,
	signed_tx BLOB NOT NULL,
	PRIMARY KEY (block_hash, transaction_index)
);

-- Format version of the block's rows in `receipts`, written together with all of them, possibly
-- none. Blocks indexed before this migration keep 0. Receipts of a block whose version differs
-- from the one eth-rpc writes are rebuilt from the node on request.
ALTER TABLE eth_to_substrate_blocks ADD COLUMN receipts_version INTEGER NOT NULL DEFAULT 0;
