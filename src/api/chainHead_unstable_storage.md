# chainHead_unstable_storage

**Parameters**:

- `followSubscription`: An opaque string that was returned by `chainHead_unstable_follow`.
- `hash`: String containing an hexadecimal-encoded hash of the header of the block whose storage to fetch.
- `items`: Array of objects. The structure of these objects is found below.
- `childTrie`: `null` for main storage look-ups, or a string containing the hexadecimal-encoded key of the child trie of the "default" namespace.

Each element in `items` must be an object containing the following fields:

- `key`: String containing the hexadecimal-encoded key to fetch in the storage.
- `type`: String equal to one of: `value`, `hash`, `closestDescendantMerkleValue`, `descendantsValues`, `descendantsHashes`.

**Return value**: A JSON object.

The JSON object returned by this function has one the following formats:

### Started

```
{
    "result": "started",
    "operationId": ...
    "discardedItems": ...
}
```

This return value indicates that the request has successfully started.

Where:

- `operationId` is a string containing an opaque value representing the operation.
- `discardedItems` is an integer indicating the number of items at the back of the array of the `items` parameters that couldn't be processed.

If the list the JSON-RPC server is overloaded, it might refuse to accept new storage requests. In that situation, the JSON-RPC server will discard some or all the `items` passed as parameter. The number of items discarded is indicated in `discardedItems`. When that happens, the JSON-RPC client should try again after an on-going [`chainHead_unstable_storage`], [`chainHead_unstable_body`], or [`chainHead_unstable_call`] operation finishes.

The JSON-RPC server must accept at least 16 concurrent operations for any given [`chainHead_unstable_follow`] subscription. In other words, as long as the JSON-RPC client makes sure that no more than 16 operations are in progress at any given item, it is guaranteed that all of its operations will be accepted by the JSON-RPC server.
For this purpose, each item requested through [`chainHead_unstable_storage`] counts as one operation, and each call to [`chainHead_unstable_body`] and [`chainHead_unstable_call`] counts as one operation.

### LimitReached

```
{
    "result": "limitReached"
}
```

This return value indicates that all the items would be discarded, or that the provided `followSubscription` is invalid or stale.

## Overview

For each item in `items`, the JSON-RPC server must start obtaining the value of the entry with the given `key` from the storage, either from the main trie or from `childTrie`. If `type` is `descendantsValues` or `descendantsHashes`, then it must also obtain the values of all the descendants of the entry.

For the purpose of storage requests, the trie root hash of the child tries of the storage can be found in the main trie at keys starting the bytes of the ASCII string `:child_storage:`. This behaviour is consistent with all the other storage-request-alike mechanisms of Polkadot and Substrate-based chains, such as host functions or libp2p network requests.

The progress of the operation is indicated through `operationStorageItems`, `operationWaitingForContinue`, `operationStorageDone`, `operationInaccessible`, or `operationError` notifications generated on the corresponding `chainHead_unstable_follow` subscription.

The operation continues even if the target block is unpinned with `chainHead_unstable_unpin`.

This function should be seen as a complement to `chainHead_unstable_follow`, allowing the JSON-RPC client to retrieve more information about a block that has been reported. Use `archive_unstable_storage` if instead you want to retrieve the storage of an arbitrary block.

`{"event": "operationStorageItems"}` notifications will be generated. Each notification contains a list of items. The list of items, concatenated together, forms the result.

If the `type` of an item is `value`, and `key` is associated with a storage value in the trie, then the result will include an item that contains this storage value. If `key` is not associated with a storage value in the trie, then the result will not include such item.

If the `type` of an item is `hash`, the behaviour is similar to a `type` equal to `value`, except that the cryptographic hash of the value is included in the result rather than the value itself. The hashing algorithm used is the one of the chain, which is typically blake2b. This can lead to significantly less bandwidth usage and can be used in order to compare the value of an item with a known hash and querying the full value only if it differs.

If the `type` of an item is `descendantsValues` or `descendantsHashes`, then the result will contain zero or more items whose key starts with the `key` of this item.

If the `type` of an item is `closestDescendantMerkleValue`, then the so-called trie Merkle value of the `key` can be found in the result. If `key` doesn't exist in the trie, then the Merkle value of the closest descendant of `key` (including branch nodes) is provided. If `key` doesn't have any descendant in the trie, then the result will not contain any relevant item.

If `items` contains multiple identical or overlapping queries, the JSON-RPC server can choose whether to merge or not the items in the result. For example, if the request contains two items with the same key, one with `hash` and one with `value`, the JSON-RPC server can choose whether to generate two `item` objects, one with the value and one with the hash, or only a single `item` object with both `hash` and `value` set. The JSON-RPC server is encouraged to notify as soon as possible of the information at its disposal, without waiting for missing information.

It is allowed (but discouraged) for the JSON-RPC server to provide the same information multiple times in the result, for example providing the `value` field of the same `key` twice. Forcing the JSON-RPC server to de-duplicate items in the result might lead to unnecessary overhead.

If a `{"event": "operationWaitingForContinue"}` notification is generated, the subscription will not generate any more notification unless the JSON-RPC client calls the `chainHead_unstable_continue` JSON-RPC function. The JSON-RPC server is encouraged to generate this event after having sent a certain number of bytes to the JSON-RPC client in order to avoid head-of-line-blocking issues.

## Possible errors

- A JSON-RPC error is generated if `type` isn't one of the allowed values (similarly to a missing parameter or an invalid parameter type).
- If the networking part of the behaviour fails, then a `{"event": "operationInaccessible"}` notification is generated (as explained above).
- If the `followSubscription` is invalid or stale, then `"result": "limitReached"` is returned (as explained above).
- A JSON-RPC error is generated if the block hash passed as parameter doesn't correspond to any block that has been reported by `chainHead_unstable_follow`.
- A JSON-RPC error is generated if the `followSubscription` is valid but the block hash passed as parameter has already been unpinned.
