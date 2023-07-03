# chainHead_unstable_storage

**Parameters**:

- `followSubscription`: An opaque string that was returned by `chainHead_unstable_follow`.
- `hash`: String containing an hexadecimal-encoded hash of the header of the block whose storage to fetch.
- `items`: Array of objects. The structure of these objects is found below.
- `childTrie`: `null` for main storage look-ups, or a string containing the hexadecimal-encoded key of the child trie of the "default" namespace.
- `networkConfig` (optional): Object containing the configuration of the networking part of the function. See [here](./api.md) for details. Ignored if the JSON-RPC server doesn't need to perform a network request. Sensible defaults are used if not provided.

Each element in `items` must be an object containing the following fields:

- `key`: String containing the hexadecimal-encoded key to fetch in the storage.
- `type`: String equal to one of: `value`, `hash`, `closest-descendant-merkle-value`, `descendants-values`, `descendants-hashes`.

**Return value**: String containing an opaque value representing the operation.

For each item in `items`, the JSON-RPC server must start obtaining the value of the entry with the given `key` from the storage, either from the main trie or from `childTrie`. If `type` is `descendants-values` or `descendants-hashes`, then it must also obtain the values of all the descendants of the entry.

The operation will continue even if the given block is unpinned while it is in progress.

This function should be seen as a complement to `chainHead_unstable_follow`, allowing the JSON-RPC client to retrieve more information about a block that has been reported. Use `archive_unstable_storage` if instead you want to retrieve the storage of an arbitrary block.

`{"event": "items"}` notifications will be generated. Each notification contains a list of items. The list of items, concatenated together, forms the result.

If the `type` of an item is `value`, and `key` is associated with a storage value in the trie, then the result will include an item that contains this storage value. If `key` is not associated with a storage value in the trie, then the result will not include such item.

If the `type` of an item is `hash`, the behaviour is similar to a `type` equal to `value`, except that the cryptographic hash of the value is included in the result rather than the value itself. The hashing algorithm used is the one of the chain, which is typically blake2b. This can lead to significantly less bandwidth usage and can be used in order to compare the value of an item with a known hash and querying the full value only if it differs.

If the `type` of an item is `descendants-values` or `descendants-hashes`, then the result will contain zero or more items whose key starts with the `key` of this item.

If the `type` of an item is `closest-descendant-merkle-value`, then the so-called trie Merkle value of the `key` can be found in the result. If `key` doesn't exist in the trie, then the Merkle value of the closest descendant of `key` (including branch nodes) is provided. If `key` doesn't have any descendant in the trie, then the result will not contain any relevant item.

If `items` contains multiple identical or overlapping queries, the JSON-RPC server can choose whether to merge or not the items in the result. For example, if the request contains two items with the same key, one with `hash` and one with `value`, the JSON-RPC server can choose whether to generate two `item` objects, one with the value and one with the hash, or only a single `item` object with both `hash` and `value` set. The JSON-RPC server is encouraged to notify as soon as possible of the information at its disposal, without waiting for missing information.

It is allowed (but discouraged) for the JSON-RPC server to provide the same information multiple times in the result, for example providing the `value` field of the same `key` twice. Forcing the JSON-RPC server to de-duplicate items in the result might lead to unnecessary overhead.

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

### items

```json
{
    "event": "items",
    "items": [
        {
            "key": "0x0000000...",
            "value": "0x0000000...",
            "hash": "0x0000000...",
            "closest-descendant-merkle-value": "0x000000..."
        },
        ...
    ]
}
```

Yields one or more items that were found in the storage.

The `key` field is a string containing the hexadecimal-encoded key of the item. This `key` is guaranteed to start with one of the `key`s provided as parameter.
If the `type` parameter was `"value"`, `"hash"`, `"closest-descendant-merkle-value"`, then it is also guaranteed to be equal to one of the `key`s provided as parameter.

In the situation where the `type` parameter was `"closest-descendant-merkle-value"`, the fact that `key` is equal to a `key` that was provided as parameter is necessary in order to avoid ambiguities when multiple `items` of type `"closest-descendant-merkle-value"` were requested.

The `value` field is set if this item corresponds to one of the requested items whose `type` was `"value"` or `"descendants-values"`. The `value` field is a string containing the hexadecimal-encoded value of the storage entry.

The `hash` field is set if this item corresponds to one of the requested items whose `type` was `"hash"` or `"descendants-hashes"`. The `hash` field is a string containing the hexadecimal-encoded hash of the storage entry.

The `closest-descendant-merkle-value` field is set if this item corresponds to one of the requested items whose `type` was `"closest-descendant-merkle-value"`. The trie node whose Merkle value is indicated in `closest-descendant-merkle-value` is not indicated, as determining the key of this node might incur an overhead for the JSON-RPC server.

### waiting-for-continue

```json
{
    "event": "waiting-for-continue"
}
```

The `waiting-for-continue` event is generated after at least one `"item"` event has been generated, and indicates that the JSON-RPC client must call `chainHead_unstable_storageContinue` before more events are generated.

This event only ever happens if the `type` parameter was `descendants-values` or `descendants-hashes`.

While the JSON-RPC server is waiting for a call to `chainHead_unstable_storageContinue`, it can generate an `inaccessible` event in order to indicate that it can no longer proceed with the request. If that is the case, the JSON-RPC client can simply try again.

### done

```json
{
    "event": "done"
}
```

The `done` event indicates that everything went well and all result has been provided through `items` events in the past.

If no `items` event was yielded, then the storage doesn't contain a value at the given key.

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

The `error` event indicates a problem during the storage access, such as failing to parse the block header to obtain the state root hash.

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
