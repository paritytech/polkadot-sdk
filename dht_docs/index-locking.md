# Index locking refactor

Issue #10910. v1 protects the whole index with one `RwLock<Index>`, so writers
block readers and vice-versa, and the entire index must live in RAM (a hard
scaling ceiling). Reads and writes touch disjoint fields, so they can be split.

- **Stage 1** — separate `WriteIndex` (accounts, entries, counters) from
  `ReadIndex` (`by_topic`, `by_dec_key`) under independent locks; the write path
  notifies a background worker to update the read index after the DB commit.
- **Stage 2** — move the read index to disk with a bounded LRU cache, and
  replace the full `accounts` map with a summary + LRU. The read path becomes
  lock-free and memory stops being the limiting factor.

Enables the higher storage volumes that [[store-limits]] assumes.
