-- Create DB:
-- DATABASE_URL="..." cargo sqlx database create
--
-- Run migration:
-- DATABASE_URL="..." cargo sqlx migrate run
--
-- Update compile time artifacts:
-- DATABASE_URL="..." cargo sqlx prepare
CREATE TABLE transaction_hashes (
  transaction_hash CHAR(64) NOT NULL PRIMARY KEY,
  transaction_index INTEGER NOT NULL,
  block_hash CHAR(64) NOT NULL
);

CREATE INDEX idx_block_hash ON transaction_hashes (block_hash);
