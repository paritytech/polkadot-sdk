CREATE TABLE IF NOT EXISTS logs (
	block_hash BLOB NOT NULL,
	transaction_index INTEGER NOT NULL,
	log_index INTEGER NOT NULL,
    address BLOB NOT NULL,
    block_number INTEGER NOT NULL,
	transaction_hash BLOB NOT NULL,
    topic_0 BLOB,
    topic_1 BLOB,
    topic_2 BLOB,
    topic_3 BLOB,
    data BLOB,
	PRIMARY KEY (block_hash, transaction_index, log_index)
);

CREATE INDEX IF NOT EXISTS idx_block_number_address_topics ON logs (
	block_number,
	address,
	topic_0,
	topic_1,
	topic_2,
	topic_3
);

CREATE INDEX IF NOT EXISTS idx_block_hash ON logs (
	block_hash
);

