# Introduction

Functions with the `archive` prefix allow obtaining the state of the chain at any point in the past.

These functions are meant to be used to inspect the history of a chain, and not recent information.

These functions are typically expensive for a JSON-RPC server, because they likely have to perform some disk access. Consequently, JSON-RPC servers are encouraged to put a global limit on the number of concurrent calls to `archive`-prefixed functions.

# archive_v1_body

**TODO**

# archive_v1_genesisHash

**Parameters**: *none*
**Return value**: String containing the hex-encoded hash of the genesis block of the chain.

This function is a simple getter. The JSON-RPC server is expected to keep in its memory the hash of the genesis block.

The value returned by this function must never change.

# archive_v1_hashByHeight

**Parameters**:
    - `height`: String containing an hexadecimal-encoded integer.
    - `networkConfig` (optional): Object containing the configuration of the networking part of the function. See above for details. Ignored if the JSON-RPC server doesn't need to perform a network request. Sensible defaults are used if not provided.
**Return value**: An opaque string that identifies the query in progress.

The JSON-RPC client must find the blocks (zero, one, or more) whose height is the one passed as parameter. If the `height` is inferior or equal to the finalized block height, then only finalized blocks must be fetched and returned.

This function will later generate a notification looking like this:

```json
{
    "jsonrpc": "2.0",
    "method": "archive_v1_hashByHeightEvent",
    "params": {
        "subscriptionId": "...",
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

# archive_v1_header

**Parameters**:
    - `hash`: String containing the hexadecimal-encoded hash of the header to retreive.
    - `networkConfig` (optional): Object containing the configuration of the networking part of the function. See above for details. Ignored if the JSON-RPC server doesn't need to perform a network request. Sensible defaults are used if not provided.
**Return value**: An opaque string that identifies the query in progress.

This function will later generate a notification looking like this:

```json
{
    "jsonrpc": "2.0",
    "method": "archive_v1_headerEvent",
    "params":{
        "subscriptionId": "...",
        "result": ...
    }
}
```

Where `result` can be:

```json
{
    "event": "done",
    "output": ...
}
```

Where `output` is a string containing the hexadecimal-encoded SCALE-codec encoding of the header of the block.

Alternatively, `result` can also be:

```json
{
    "event": "failed"
}
```

Only one notification will ever be generated.

**Note**: Other events might be added in the future, such as reports on the progress of the call.

## Possible errors

- If the block hash passed as parameter doesn't correspond to any known block, then a `{"event": "failed"}` notification is generated (as explained above).
- If the networking part of the behaviour fails, then a `{"event": "failed"}` notification is generated (as explained above).

Due to the way blockchains work, it is never possible to be certain that a block doesn't exist. For this reason, networking-related errors and unknown block errors are reported in the same way.

# archive_v1_stopBody

**TODO**

# archive_v1_stopHashByHeight

**Parameters**:
    - `subscriptionId`: An opaque string that was returned by `archive_v1_hashByHeight`.
**Return value**: *null*

Stops a query started with `archive_v1_hashByHeight`. If the query was still in progress, this interrupts it. If the query was already finished, this call has no effect.

JSON-RPC client implementations must be aware that, due to the asynchronous nature of JSON-RPC client <-> server communication, they might still receive a notification about this call, for example because this notification was already in the process of being sent back by the JSON-RPC server.

## Possible errors

A JSON-RPC error is generated if the `subscriptionId` doesn't correspond to any active subscription.

# archive_v1_stopHeader

**Parameters**:
    - `subscriptionId`: An opaque string that was returned by `archive_v1_header`.
**Return value**: *null*

Stops a query started with `archive_v1_header`. If the query was still in progress, this interrupts it. If the query was already finished, this call has no effect.

JSON-RPC client implementations must be aware that, due to the asynchronous nature of JSON-RPC client <-> server communication, they might still receive a notification about this call, for example because this notification was already in the process of being sent back by the JSON-RPC server.

## Possible errors

A JSON-RPC error is generated if the `subscriptionId` doesn't correspond to any active subscription.

# archive_v1_stopStorage

**Parameters**:
    - `subscriptionId`: An opaque string that was returned by `archive_v1_storage`.
**Return value**: *null*

Stops a storage fetch started with `archive_v1_storage`. If the storage fetch was still in progress, this interrupts it. If the storage fetch was already finished, this call has no effect.

JSON-RPC client implementations must be aware that, due to the asynchronous nature of JSON-RPC client <-> server communication, they might still receive a notification about this storage fetch, for example because this notification was already in the process of being sent back by the JSON-RPC server.

## Possible errors

A JSON-RPC error is generated if the `subscriptionId` doesn't correspond to any active subscription.

# archive_v1_storage

**Parameters**:
    - `hash`: String containing an hexadecimal-encoded hash of the header of the block whose storage to fetch.
    - `key`: String containing the hexadecimal-encoded key to fetch in the storage.
    - `childKey`: `null` for main storage look-ups, or a string containing the hexadecimal-encoded key of the trie key of the trie that `key` refers to. **TODO**: I don't know enough about child tries to design this properly
    - `type`: String that must be equal to one of: `value`, `hash`, or `size`.
    - `networkConfig` (optional): Object containing the configuration of the networking part of the function. See above for details. Ignored if the JSON-RPC server doesn't need to perform a network request. Sensible defaults are used if not provided.
**Return value**: An opaque string that identifies the storage fetch in progress.

This function works the same way as `chainHead_v1_storage`, except that it is not connected to a chain head follow, and no `disjoint` event can be generated.

Note that `chainHead_v1_storage` and `archive_v1_storage` should be treated as two completely separate functions. It is forbidden to call `archive_v1_stopStorage` with a storage fetch started with `chainHead_v1_storage`, and vice versa. Some JSON-RPC servers might support only one of these functions.
