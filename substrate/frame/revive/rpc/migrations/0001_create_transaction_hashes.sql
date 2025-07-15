-- Useful commands:
--
-- Set DATABASE_URL environment variable.
-- export DATABASE_URL=sqlite:///$HOME/eth_rpc.db
--
-- Create DB:
-- cargo sqlx database create
--
-- Run migration manually:
-- cargo sqlx migrate run
--
-- Update compile time artifacts:
-- cargo sqlx prepare
CREATE TABLE IF NOT EXISTS transaction_hashes (
  transaction_hash BLOB NOT NULL PRIMARY KEY,
  transaction_index INTEGER NOT NULL,
  block_hash BLOB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_block_hash ON transaction_hashes (
	block_hash
);
