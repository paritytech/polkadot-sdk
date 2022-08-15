# archive_unstable_hashByHeight

**Parameters**:

- `height`: String containing an hexadecimal-encoded integer.
- `networkConfig` (optional): Object containing the configuration of the networking part of the function. See [here](./api.md) for details. Ignored if the JSON-RPC server doesn't need to perform a network request. Sensible defaults are used if not provided.

**Return value**: String containing an opaque value representing the operation.

The JSON-RPC client must find the blocks (zero, one, or more) whose height is the one passed as parameter. If the `height` is inferior or equal to the finalized block height, then only finalized blocks must be fetched and returned.

This function will later generate a notification in the following format:

```json
{
    "jsonrpc": "2.0",
    "method": "archive_unstable_hashByHeightEvent",
    "params": {
        "subscription": "...",
        "result": ...
    }
}
```

Where `result` can be:

```json
{
    "event": "done",
    "output": [...]
}
```

Where `output` is an array of hexadecimal-encoded hashes corresponding to the blocks of this height that are known to the node. If the `height` is inferior or equal to the finalized block height, the array must contain either zero or one entry.

Only one notification will ever be generated.

**Note**: Other events might be added in the future, such as reports on the progress of the query.

**Important implementation note**: While it is possible for a light client to ask its peers which block hash corresponds to a certain height, it is at the moment impossible to obtain a proof of this. If a light client implements this JSON-RPC function, it must only look in its internal memory and **not** ask the network. Consequently, calling this function with a height more than a few blocks away from the finalized block height will always return zero blocks. Despite being currently useless, the `networkConfig` parameter is kept for the future.
