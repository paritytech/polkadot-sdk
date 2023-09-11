# archive_unstable_storage

**Parameters**:

- `hash`: String containing an hexadecimal-encoded hash of the header of the block whose storage to fetch.
- `items`: Array of objects. The structure of these objects is found below.
- `childTrie`: `null` for main storage look-ups, or a string containing the hexadecimal-encoded key of the child trie of the "default" namespace.

Each element in `items` must be an object containing the following fields:

- `key`: String containing the hexadecimal-encoded key to fetch in the storage.
- `type`: String equal to one of: `value`, `hash`, `closestDescendantMerkleValue`, `descendantsValues`, `descendantsHashes`.

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

### started

```
{
    "result": "started",
    "discardedItems": ...
}
```

This return value indicates that the request has successfully started.

Where `discardedItems` is an integer indicating the number of items at the back of the array of the `items` parameters that couldn't be processed.

If the list the JSON-RPC server is overloaded, it might refuse to accept new storage requests. In that situation, the JSON-RPC server will discard some or all the `items` passed as parameter. The number of items discarded is indicated in `discardedItems`. When that happens, the JSON-RPC client should try again after an on-going `archive_unstable_storage` operation finishes.

### items

```json
{
    "event": "items",
    "items": [
        {
            "key": "0x0000000...",
            "value": "0x0000000...",
            "hash": "0x0000000...",
            "closestDescendantMerkleValue": "0x000000..."
        },
        ...
    ]
}
```

Yields one or more items that were found in the storage.

The `key` field is a string containing the hexadecimal-encoded key of the item. This `key` is guaranteed to start with one of the `key`s provided as parameter to `archive_unstable_storage`.
If the `type` parameter was `"value"`, `"hash"`, `"closestDescendantMerkleValue"`, then it is also guaranteed to be equal to one of the `key`s provided as parameter to `archive_unstable_storage`.

In the situation where the `type` parameter was `"closestDescendantMerkleValue"`, the fact that `key` is equal to a `key` that was provided as parameter is necessary in order to avoid ambiguities when multiple `items` of type `"closestDescendantMerkleValue"` were requested.

The `value` field is set if this item corresponds to one of the requested items whose `type` was `"value"` or `"descendantsValues"`. The `value` field is a string containing the hexadecimal-encoded value of the storage entry.

The `hash` field is set if this item corresponds to one of the requested items whose `type` was `"hash"` or `"descendantsHashes"`. The `hash` field is a string containing the hexadecimal-encoded hash of the storage entry.

The `closestDescendantMerkleValue` field is set if this item corresponds to one of the requested items whose `type` was `"closestDescendantMerkleValue"`. The trie node whose Merkle value is indicated in `closestDescendantMerkleValue` is not indicated, as determining the key of this node might incur an overhead for the JSON-RPC server. The Merkle value is equal to either the node value or the hash of the node value, as defined in the [Polkadot specification](https://spec.polkadot.network/chap-state#defn-merkle-value).

### waitingForContinue

```json
{
    "event": "waitingForContinue",
}
```

The `waitingForContinue` event is generated after at least one `"items"` event has been generated, and indicates that the JSON-RPC client must call `archive_unstable_storageContinue` before more events are generated.

This event only ever happens if the `type` of one of the `items` provided as a parameter to `archive_unstable_storage` was `descendantsValues` or `descendantsHashes`.

### done

```json
{
    "event": "done",
    "operationId": ...
}
```

The `done` event indicates that an operation started with `archive_unstable_storage` went well and all result has been provided through `items` events in the past.

If no `items` event was yielded, then the storage doesn't contain a value at the given key.

No more event will be generated with this `subscription`.

### error

```json
{
    "event": "error",
    "error": "..."
}
```

The `error` event indicates a problem during the operation. This includes failing to parse the block header.

Repeating the same call in the future will not succeed.

`error` is a human-readable error message indicating why the call has failed. This string isn't meant to be shown to end users, but is for developers to understand the problem.

No more event will be generated with this `subscription`.

### stop

```json
{
    "event": "stop"
}
```

The `stop` event can be generated after a `waitingForContinue` event in order to indicate that the JSON-RPC server can't continue. The JSON-RPC client should simply try again.

No more event will be generated with this `subscription`.

**Note**: This event is generated in very niche situations, such as a node doing a clean shutdown of all its active subscriptions before shutting down.

## Overview

For each item in `items`, the JSON-RPC server must start obtaining the value of the entry with the given `key` from the storage, either from the main trie or from `childTrie`. If `type` is `descendantsValues` or `descendantsHashes`, then it must also obtain the values of all the descendants of the entry.

For the purpose of storage requests, the trie root hash of the child tries of the storage can be found in the main trie at keys starting the bytes of the ASCII string `:child_storage:`. This behaviour is consistent with all the other storage-request-alike mechanisms of Polkadot and Substrate-based chains, such as host functions or libp2p network requests.

This function should be used when the target block is older than the blocks reported by `chainHead_unstable_follow`.
Use `chainHead_unstable_storage` if instead you want to retrieve the storage of a block obtained by the `chainHead_unstable_follow`.

`{"event": "storageItems"}` notifications will be generated. Each notification contains a list of items. The list of items, concatenated together, forms the result.

If the `type` of an item is `value`, and `key` is associated with a storage value in the trie, then the result will include an item that contains this storage value. If `key` is not associated with a storage value in the trie, then the result will not include such item.

If the `type` of an item is `hash`, the behavior is similar to a `type` equal to `value`, except that the cryptographic hash of the value is included in the result rather than the value itself. The hashing algorithm used is the one of the chain, which is typically blake2b. This can lead to significantly less bandwidth usage and can be used in order to compare the value of an item with a known hash and querying the full value only if it differs.

If the `type` of an item is `descendantsValues` or `descendantsHashes`, then the result will contain zero or more items whose key starts with the `key` of this item.

If the `type` of an item is `closestDescendantMerkleValue`, then the so-called trie Merkle value of the `key` can be found in the result. If `key` doesn't exist in the trie, then the Merkle value of the closest descendant of `key` (including branch nodes) is provided. If `key` doesn't have any descendant in the trie, then the result will not contain any relevant item.

If `items` contains multiple identical or overlapping queries, the JSON-RPC server can choose whether to merge or not the items in the result. For example, if the request contains two items with the same key, one with `hash` and one with `value`, the JSON-RPC server can choose whether to generate two `item` objects, one with the value and one with the hash, or only a single `item` object with both `hash` and `value` set. The JSON-RPC server is encouraged to notify as soon as possible of the information at its disposal, without waiting for missing information.

It is allowed (but discouraged) for the JSON-RPC server to provide the same information multiple times in the result, for example providing the `value` field of the same `key` twice. Forcing the JSON-RPC server to de-duplicate items in the result might lead to unnecessary overhead.

If a `{"event": "waitingForContinue"}` notification is generated, the subscription will not generate any more notification unless the JSON-RPC client calls the `archive_unstable_storageContinue` JSON-RPC function. The JSON-RPC server is encouraged to generate this event after having sent a certain number of bytes to the JSON-RPC client in order to avoid head-of-line-blocking issues.

## Possible errors

- A JSON-RPC error is generated if `type` isn't one of the allowed values (similarly to a missing parameter or an invalid parameter type).
