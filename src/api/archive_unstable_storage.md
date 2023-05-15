# archive_unstable_storage

**Parameters**:

- `hash`: String containing an hexadecimal-encoded hash of the header of the block whose storage to fetch.
- `key`: String containing the hexadecimal-encoded key to fetch in the storage.
- `childTrie`: `null` for main storage look-ups, or a string containing the hexadecimal-encoded key of the child trie of the "default" namespace.
- `includeDescendants`: Boolean indicating whether the key-values of all the descendants of the `key` should be returned as well.

**Return value**: If no block with that `hash` exists, `null`. Otherwise, a map of `String => String`, where the keys are hexadecimal-encoded storage keys and the values are hexadecimal-encoded storage values.

The JSON-RPC server must obtain the value of the entry with the given `key` from the storage, either from the main trie of from `childTrie`. If `includeDescendants` is `true`, then the values of all the descendants must be obtained as well.

If `includeDescendants` is `false`, then the returned array contains either zero entry if there is no entry in the storage with that key, or one entry if there is one. That one entry must have the same key as the `key` parameter.
If `includeDescendants` is `true`, then the returned array contains the requested `key` and all the descendants.

If the block was previously returned by `archive_unstable_hashByHeight` at a height inferior or equal to the current finalized block height (as indicated by `archive_unstable_finalizedHeight`), then calling this method multiple times is guaranteed to always return non-null and always the same result.

If the block was previously returned by `archive_unstable_hashByHeight` at a height strictly superior to the current finalized block height (as indicated by `archive_unstable_finalizedHeight`), then the block might "disappear" and calling this function might return `null` at any point.
