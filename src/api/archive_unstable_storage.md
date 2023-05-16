# archive_unstable_storage

**Parameters**:

- `hash`: String containing an hexadecimal-encoded hash of the header of the block whose storage to fetch.
- `key`: String containing the hexadecimal-encoded key to fetch in the storage.
- `childTrie`: `null` for main storage look-ups, or a string containing the hexadecimal-encoded key of the child trie of the "default" namespace.
- `includeDescendants`: Boolean indicating whether the key-values of all the descendants of the `key` should be returned as well.

**Return value**: String containing an opaque value representing the operation, or `null` if no block with that `hash` exists.

The JSON-RPC server must obtain the value of the entry with the given `key` from the storage, either from the main trie of from `childTrie`. If `includeDescendants` is `true`, then the values of all the descendants must be obtained as well.

If the block was previously returned by `archive_unstable_hashByHeight` at a height inferior or equal to the current finalized block height (as indicated by `archive_unstable_finalizedHeight`), then calling this method multiple times is guaranteed to always return non-null and always the same results.

If the block was previously returned by `archive_unstable_hashByHeight` at a height strictly superior to the current finalized block height (as indicated by `archive_unstable_finalizedHeight`), then the block might "disappear" and calling this function might return `null` at any point.

## Notifications format

This function will later generate notifications in the following format:

```json
{
    "jsonrpc": "2.0",
    "method": "archive_unstable_storageEvent",
    "params": {
        "subscription": "...",
        "result": ...
    }
}
```

Where `subscription` is equal to the value returned by this function, and `result` can be one of:

### item

```json
{
    "event": "item",
    "key": "0x0000000...",
    "value": "0x0000000...",
}
```

Yields an item that was found in the storage.

The `key` field is a string containing the hexadecimal-encoded key of the value that was found.
If the `includeDescendants` parameter was `true`, this `key` is guaranteed to start with the `key` provided as parameter.
If the `includeDescendants` parameter was `false`, then it is also guaranteed to be equal to the `key` provided as parameter.

The `value` field is a string containing the hexadecimal-encoded value of the storage item.

### waiting-for-continue

```json
{
    "event": "waiting-for-continue"
}
```

The `waiting-for-continue` event is generated after at least one `"item"` event has been generated, and indicates that the JSON-RPC client must call `archive_unstable_storageContinue` before more events are generated.

This event only ever happens if the `includeDescendants` parameter was `true`.

### done

```json
{
    "event": "done"
}
```

The `done` event indicates that everything went well and all values have been provided through `item` events in the past.

If no `item` event was yielded, then the storage doesn't contain a value at the given key.

No more event will be generated with this `subscription`.
