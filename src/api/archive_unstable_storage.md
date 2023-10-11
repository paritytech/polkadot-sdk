# archive_unstable_storage

**Parameters**:

- `hash`: String containing a hexadecimal-encoded hash of the header of the block whose storage to fetch.
- `items`: Array of objects. The structure of these objects is found below.
- `childTrie`: `null` for main storage look-ups, or a string containing the hexadecimal-encoded key of the child trie of the "default" namespace.

Each element in `items` must be an object containing the following fields:

- `key`: String containing the hexadecimal-encoded key to fetch in the storage.
- `type`: String equal to one of: `value`, `hash`, `closestDescendantMerkleValue`, `descendantsValues`, `descendantsHashes`.
- `paginationStartKey`: This parameter is optional and should be a string containing the hexadecimal-encoded key from which the storage iteration should resume. This parameter is only valid in the context of `descendantsValues` and `descendantsHashes`.

**Return value**: A JSON object.

The JSON object returned by this function has the following format:

```
{
    "result": [
        {
            ...
        },
        ...
    ],
    "discardedItems": ...
}
```

Where:

- `result` contains a vector of JSON objects (possibly empty) that were found in the storage.
- `discardedItems` is an integer indicating the number of items at the back of the array of the `items` parameters that couldn't be processed.

The JSON objects in the `"result"` field can have one of the following formats based on their type:

### Value

```
{
    "key": "0x0000000...",
    "value": "0x0000000...",
}
```

The JSON object corresponds to one of the requested items whose `type` was `"value"` or `"descendantsValues"`.

If the `key` is not associated with a storage value in the trie, then no response is generated.

Returned when the `type` of the query was `"value"`:

- `key` is guaranteed to be equal to one of the `key`s provided.
- `value` is a string containing the hexadecimal-encoded value of the storage entry.

Returned when the `type` of the query was `"descendantsValues"`:

- `key` is guaranteed to start with one of the `key`s provided.
- `value` is a string containing the hexadecimal-encoded value of the storage entry.

### Hash

```
{
    "key": "0x0000000...",
    "hash": "0x0000000...",
}
```

The JSON object corresponds to one of the requested items whose `type` was `"hash"` or `"descendantsHashes"`.

If the `key` is not associated with a storage value in the trie, then no response is generated.

Returned when the `type` of the query was `"hash"`:

- `key` is guaranteed to be equal to one of the `key`s provided.
- `hash` is a string containing the hexadecimal-encoded cryptographic hash of the storage entry.

Returned when the `type` of the query was `"descendantsHashes"`:

- `key` is guaranteed to start with one of the `key`s provided.
- `hash` is a string containing the hexadecimal-encoded cryptographic hash of the storage entry.


### ClosestDescendantMerkleValue

```
{
    "key": "0x0000000...",
    "closestDescendantMerkleValue": "0x000000..."
}
```

The JSON object corresponds to one of the requested items whose `type` was `"closestDescendantMerkleValue"`.

If the `key` doesn't exist in the trie, then the Merkle value of the closest descendant of `key` (including branch nodes) is provided. If `key` doesn't have any descendant in the trie, then no response is generated.

- `key` is guaranteed to be equal to one of the `key`s provided.
- `closestDescendantMerkleValue` is the closest trie Merkle value of the `key`.

The trie node whose Merkle value is indicated in `closestDescendantMerkleValue` is not indicated, as determining the key of this node might incur an overhead for the JSON-RPC server. The Merkle value is equal to either the node value or the hash of the node value, as defined in the [Polkadot specification](https://spec.polkadot.network/chap-state#defn-merkle-value).

**Note**:

For each item in `items`, the JSON-RPC server must start obtaining the value of the entry with the given `key` from the storage, either from the main trie or from `childTrie`. If `type` is `descendantsValues` or `descendantsHashes`, then it must also obtain the values of all the descendants of the entry.

For the purpose of storage requests, the trie root hash of the child tries of the storage can be found in the main trie at keys starting the bytes of the ASCII string `:child_storage:`. This behaviour is consistent with all the other storage-request-alike mechanisms of Polkadot and Substrate-based chains, such as host functions or libp2p network requests.

If the height of the block hash provided is less than or equal to the current finalized block height (which can be obtained via archive_unstable_finalizedHeight), then calling this method with the same parameters will always return the same response.
If the height of the block hash provided is greater than the current finalized block height, then the block might be pruned at any time and calling this method may return null.

This function should be used when the target block is older than the blocks reported by `chainHead_unstable_follow`.
Use `chainHead_unstable_storage` if instead you want to retrieve the storage of a block obtained by the `chainHead_unstable_follow`.

If `items` contains multiple identical or overlapping queries, the JSON-RPC server can choose whether to merge or not the items in the result. For example, if the request contains two items with the same key, one with `hash` and one with `value`, the JSON-RPC server can choose whether to generate two `item` objects, one with the value and one with the hash, or only a single `item` object with both `hash` and `value` set. The JSON-RPC server is encouraged to notify as soon as possible of the information at its disposal, without waiting for missing information.

It is allowed (but discouraged) for the JSON-RPC server to provide the same information multiple times in the result, for example providing the `value` field of the same `key` twice. Forcing the JSON-RPC server to de-duplicate items in the result might lead to unnecessary overhead.

## Possible errors

- A JSON-RPC error is generated if `type` isn't one of the allowed values (similarly to a missing parameter or an invalid parameter type).
