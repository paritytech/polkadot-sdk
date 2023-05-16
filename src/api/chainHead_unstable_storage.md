# chainHead_unstable_storage

**Parameters**:

- `followSubscription`: An opaque string that was returned by `chainHead_unstable_follow`.
- `hash`: String containing an hexadecimal-encoded hash of the header of the block whose storage to fetch.
- `key`: String containing the hexadecimal-encoded key to fetch in the storage.
- `childTrie`: `null` for main storage look-ups, or a string containing the hexadecimal-encoded key of the child trie of the "default" namespace.
- `type`: String equal to one of: `value`, `hash`, `closest-ancestor-merkle-value`, `descendants-values`, `descendants-hashes`.
- `networkConfig` (optional): Object containing the configuration of the networking part of the function. See [here](./api.md) for details. Ignored if the JSON-RPC server doesn't need to perform a network request. Sensible defaults are used if not provided.

**Return value**: String containing an opaque value representing the operation.

The JSON-RPC server must start obtaining the value of the entry with the given `key` from the storage, either from the main trie of from `childTrie`. If `type` is `descendants-value` or `descendants-hashes`, then it must also obtain the values of all the descendants of the entry.

The operation will continue even if the given block is unpinned while it is in progress.

This function should be seen as a complement to `chainHead_unstable_follow`, allowing the JSON-RPC client to retrieve more information about a block that has been reported. Use `archive_unstable_storage` if instead you want to retrieve the storage of an arbitrary block.

For optimization purposes, the JSON-RPC server is allowed to wait a little bit (e.g. up to 100ms) before starting to try fulfill the storage request, in order to batch multiple storage requests together.

One `{"event": "item"}` notification will be generated for each value found in the storage. If `type` is `value` or `hash`, then either 0 or 1 `"item"` notification will be generated. If `type` is `closest-ancestor-merkle-value` then exactly 1 `"item"` notification will be generated. If `type` is `descendants-values` or `descendants-hashes`, then one `"item"` notifications that will be generated for each descendant of the `key` (including the `key` itself).

If `type` is `hash` or `descendants-hashes`, then the cryptographic hash of each item is provided rather than the full value. The hashing algorithm used is the one of the chain, which is typically blake2b. This can lead to significantly less bandwidth usage and can be used in order to compare the value of an item with a known hash and querying the full value only if it differs.

If `type` is `closest-ancestor-merkle-value`, then the so-called trie Merkle value of the `key` is provided. If `key` doesn't exist in the trie, then the Merkle value of the closest ancestor of `key` is provided. Contrary to `hash`, a `closest-ancestor-merkle-value` always exists for every `key`. The Merkle value is similar to a hash of the value and all of its descendants together.

If a `{"event": "waiting-for-continue"}` notification is generated, the subscription will not generate any more notification unless the JSON-RPC client calls the `chainHead_unstable_storageContinue` JSON-RPC function. The JSON-RPC server is encouraged to generate this event after having sent a certain number of bytes to the JSON-RPC client in order to avoid head-of-line-blocking issues.

## Notifications format

This function will later generate notifications in the following format:

```json
{
    "jsonrpc": "2.0",
    "method": "chainHead_unstable_storageEvent",
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
    "hash": "0x0000000...",
    "merkle-value": "0x000000...",
}
```

Yields an item that was found in the storage.

The `key` field is a string containing the hexadecimal-encoded key of the value that was found.
If the `type` parameter was `"value"`, `"hash"`, `"descendants-values"` or `"descendants-hashes"`, this `key` is guaranteed to start with the `key` provided as parameter.
If the `type` parameter was `"value"` or `"hash"`, then it is also guaranteed to be equal to the `key` provided as parameter.
If the `type` parameter was `"closest-ancestor-merkle-value"`, then the `key` provided as parameter is guaranteed to start with the value in the `key` field.

If the `type` parameter was `"value"` or `"descendants-values"`, then the `value` field is set. The `value` field a string containing the hexadecimal-encoded value of the storage entry.

If the `type` parameter was `"hash"` or `"descendants-hashes"`, then the `hash` field is set. The `hash` field a string containing the hexadecimal-encoded hash of the storage entry.

If the `type` parameter was `"closest-ancestor-merkle-value"`, then the `merkle-value` field is set and the `key` field indicates which closest ancestor has been found. The `merkle-value` field a string containing the hexadecimal-encoded Merkle value of the storage entry or its closest ancestor.

Only one of `value`, `hash` or `merkle-value` are set at any given time.

### waiting-for-continue

```json
{
    "event": "waiting-for-continue"
}
```

The `waiting-for-continue` event is generated after at least one `"item"` event has been generated, and indicates that the JSON-RPC client must call `chainHead_unstable_storageContinue` before more events are generated.

This event only ever happens if the `type` parameter was `descendants-values` or `descendants-hashes`.

### done

```json
{
    "event": "done"
}
```

The `done` event indicates that everything went well and all values have been provided through `item` events in the past.

If no `item` event was yielded, then the storage doesn't contain a value at the given key.

No more event will be generated with this `subscription`.

### inaccessible

```json
{
    "event": "inaccessible"
}
```

The `inaccessible` event indicates that the storage value has failed to be retrieved from the network.

No more event will be generated with this `subscription`.

### error

```json
{
    "event": "error",
    "error": "..."
}
```

The `error` event indicates a problem during the storage access, such as failing to parse the block header to obtain the state root hash or the trie being empty when `type` was `closest-ancestor-merkle-value`.

Contrary to the `inaccessible` event, querying the same storage value again in the future will not succeed.

`error` is a human-readable error message indicating why the call has failed. This string isn't meant to be shown to end users, but is for developers to understand the problem.

This can only be the first event generated by this subscription.
No other event will be generated with this subscription.

### disjoint

```json
{
    "event": "disjoint"
}
```

The `disjoint` event indicates that the provided `followSubscription` is invalid or stale.

This can only be the first event generated by this subscription.
No other event will be generated with this subscription.

## Possible errors

- A JSON-RPC error is generated if `type` isn't one of the allowed values (similarly to a missing parameter or an invalid parameter type).
- If the networking part of the behaviour fails, then a `{"event": "inaccessible"}` notification is generated (as explained above).
- If the `followSubscription` is invalid or stale, then a `{"event": "disjoint"}` notification is generated (as explained above).
- A JSON-RPC error is generated if the block hash passed as parameter doesn't correspond to any block that has been reported by `chainHead_unstable_follow`.
- A JSON-RPC error is generated if the `followSubscription` is valid but the block hash passed as parameter has already been unpinned.
- If the trie is empty and `type` is `closest-ancestor-merkle-value`, then a `{"event": "error"}`.
