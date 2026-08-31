CREATE TABLE IF NOT EXISTS sync_state (
    label TEXT NOT NULL PRIMARY KEY,
    block_number INTEGER NOT NULL,
    block_hash BLOB
);
