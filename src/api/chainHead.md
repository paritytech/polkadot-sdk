# chainHead_v1_body

**Parameters**:
    - `followSubscriptionId`: An opaque string that was returned by `chainHead_v1_follow`.
    - `hash`: String containing an hexadecimal-encoded hash of the header of the block whose body to fetch.
    - `networkConfig` (optional): Object containing the configuration of the networking part of the function. See above for details. Ignored if the JSON-RPC server doesn't need to perform a network request. Sensible defaults are used if not provided.
**Return value**: An opaque string that identifies the body fetch in progress.

The JSON-RPC server must start obtaining the body (in other words the list of transactions) of the given block.

This function will later generate notifications looking like this:

```json
{
    "jsonrpc": "2.0",
    "method": "chainHead_v1_bodyEvent",
    "params": {
        "subscriptionId": "...",
        "result": ...
    }
}
```

If everything is successful, `result` will be:

```json
{
    "event": "done",
    "value": [...]
}
```

Where `value` is an array of strings containing the hex-encoded SCALE-encoded extrinsics found in this block.

Alternatively, `result` can also be:

```json
{
    "event": "failed"
}
```

Which indicates that the body has failed to be retrieved from the network.

Alternatively, if the `followSubscriptionId` is dead, then `result` can also be:

```json
{
    "event": "disjoint"
}
```

After an `"event": "done"`, `"event": "failed"`, or `"event": "disjoint"` is received, no more notification will be generated.

**Note**: Other events might be added in the future, such as reports on the progress of the fetch.

## Possible errors

- If the networking part of the behaviour fails, then a `{"event": "failed"}` notification is generated (as explained above).
- A JSON-RPC error is generated if the `followSubscriptionId` is invalid.
- If the `followSubscriptionId` is dead, then a `{"event": "disjoint"}` notification is generated (as explained above).
- A JSON-RPC error is generated if the block hash passed as parameter doesn't correspond to any block that has been reported by `chainHead_v1_follow`.
- A JSON-RPC error is generated if the `followSubscriptionId` is valid but the block hash passed as parameter has already been unpinned.

# chainHead_v1_call

**Parameters**:
    - `followSubscriptionId`: An opaque string that was returned by `chainHead_v1_follow`.
    - `hash`: String containing the hexadecimal-encoded hash of the header of the block to make the call against.
    - `function`: Name of the runtime entry point to call as a string.
    - `callParameters`: Array containing a list of hexadecimal-encoded SCALE-encoded parameters to pass to the runtime function.
    - `networkConfig` (optional): Object containing the configuration of the networking part of the function. See above for details. Ignored if the JSON-RPC server doesn't need to perform a network request. Sensible defaults are used if not provided.
**Return value**: An opaque string that identifies the call in progress.

**TODO**: in order to perform the runtime call, the implementation of this function will simply concatenate all the parameters (without any separator), so does it make sense for the JSON-RPC function to require to split them into an array?

This function will later generate a notification looking like this:

```json
{
    "jsonrpc": "2.0",
    "method": "chainHead_v1_callEvent",
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
    "output": "0x0000000..."
}
```

Where `output` is the hex-encoded output of the runtime function call.

Alternatively, `result` can also be:

```json
{
    "event": "failed",
    "error": "..."
}
```

Where `error` is a human-readable error message indicating why the call has failed. This string isn't meant to be shown to end users, but is for developers to understand the problem.

Alternatively, if the `followSubscriptionId` is dead, then `result` can also be:

```json
{
    "event": "disjoint"
}
```

Only one notification will ever be generated.

**Note**: Other events might be added in the future, such as reports on the progress of the call.

**Note**: This can be used as a replacement for `state_getMetadata`, `system_accountNextIndex`, and `payment_queryInfo`.

## Possible errors

**TODO**: more precise

- If the block hash passed as parameter doesn't correspond to any known block, then a `{"event":"failed","error":"..."}` notification is generated (as explained above).
- If the JSON-RPC server is incapable of executing the Wasm runtime of the given block, a JSON-RPC error should be returned.
- If the method to call doesn't exist in the Wasm runtime of the chain, **TODO**.
- If the runtime call fails (e.g. because it triggers a panic in the runtime, running out of memory, etc., or if the runtime call takes too much time), then **TODO**.
- If the networking part of the behaviour fails, then a `{"event":"failed","error":"..."}` notification is generated (as explained above).

# chainHead_v1_follow

**Parameters**:
    - `runtimeUpdates`: A boolean indicating whether the events should report changes to the runtime.
**Return value**: String containing an opaque value representing the subscription.

This function works as follows:

- When called, returns a subscription id.
- Later, generates an `initialized` notification containing the hash of the current finalized block, and if `runtimeUpdates` is `true` the runtime specification of the runtime of the current finalized block.
- Afterwards, generates one `newBlock` notification (see below) for each non-finalized block currently in the node's memory (including all forks), then a `bestBlockChanged` notification. The notifications must be sent in an ordered way such that the parent of each block either can be found in an earlier notification or is the current finalized block.
- When a new block arrives, generates a `newBlock` notification. If the new block is also the new best block of the node, also generates a `bestBlockChanged` notification.
- When the node finalizes a block, generates a `finalized` notification indicating which blocks have been finalized (both directly and indirectly) ordered by ascending height, and which blocks have been pruned (without any ordering). The latest notified best block must *not* be in the list of pruned blocks. If that would happen, a `bestBlockChanged` notification needs to be generated beforehand.
- If the node is overloaded and cannot avoid a gap in the notifications, or in case of a warp syncing, or if the maximum number of pinned blocks is reached (see below), generates a `stop` notification indicating that the subscription is now dead and must be re-created. No more notifications will be sent out on this subscription.

If `runtimeUpdates` is `true`, then blocks shouldn't (and can't) be reported to JSON-RPC clients before the JSON-RPC server has finished obtaining the runtime specification of the new block. This means that blocks might be reported more quickly when `runtimeUpdates` is `false`.
If `runtimeUpdates` is `false`, then the `initialized` event must be sent back quickly after the function returns. If `runtimeUpdates` is `true`, then the JSON-RPC server can take as much time as it wants to send back the `initialized` event.

Notifications format:

```json
{
    "jsonrpc": "2.0",
    "method": "chainHead_v1_followEvent",
    "params": {
        "subscriptionId": "...",
        "result": ...
    }
}
```

Where `result` can be one of:

```json
{
    "event": "initialized",
    "finalizedBlockHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
    "finalizedBlockRuntime": {
        "type": "valid",
        "spec": {
            "specName": ...,
            "implName": ...,
            "authoringVersion": ...,
            "specVersion": ...,
            "implVersion": ...,
            "transactionVersion": ...,
            "apis": [...],
        }
    }
}
```

The `initialized` event is always the first event to be sent back, and is only ever sent back once per subscription.
`finalizedBlockRuntime` is present if and only if `runtimeUpdates`, the parameter to this function, is `true`.

```json
{
    "event": "newBlock",
    "blockHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
    "parentBlockHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
    "newRuntime": ...
}
```

`newRuntime` must not be present if `runtimeUpdates`, the parameter to this function, is `false`. `newRuntime` must be `null` if the runtime hasn't changed compared to its parent, or . Its format is the same as the `finalizedBlockRuntime` field in the `initialized` event.

```json
{
    "event": "bestBlockChanged",
    "bestBlockHash": "0x0000000000000000000000000000000000000000000000000000000000000000"
}
```

```json
{
    "event": "finalized",
    "finalizedBlocksHashes": [
        "0x0000000000000000000000000000000000000000000000000000000000000000",
        "0x0000000000000000000000000000000000000000000000000000000000000000"
    ],
    "prunedBlocksHashes": [
        "0x0000000000000000000000000000000000000000000000000000000000000000",
        "0x0000000000000000000000000000000000000000000000000000000000000000"
    ]
}
```

```json
{
    "event": "stop"
}
```

**Note**: This list of notifications makes it very easy for a JSON-RPC client to follow just the best block updates (listening to just `bestBlockChanged` events) or follow just the finalized block updates (listening to just `initialized` and `finalized` events). It is however not possible to easily figure out whether the runtime has been modified when these updates happen. This is not problematic, as anyone using the JSON-RPC interface naively propably doesn't need to account for runtime changes anyway.

The current finalized block reported in the `initialized` event, and each subsequent block reported with a `newBlock` event, is automatically considered by the JSON-RPC server as *pinned*. A block is guaranteed to not leave the node's memory for as long as it is pinned, making it possible to call functions such as `chainHead_v1_header` on it. Blocks must be unpinned by the JSON-RPC client by calling `chainHead_v1_unpin`.

A block is pinned only in the context of a specific subscription. If multiple `chainHead_v1_follow` subscriptions exist, then each `(subscription, block)` tuple must be unpinned individually. Blocks stay pinned even if they have been pruned from the blockchain, and must always be unpinned by the JSON-RPC client.

The JSON-RPC server is strongly encouraged to enforce a limit to the maximum number of pinned blocks. If this limit is reached, it should then stop the subscription by emitting a `stop` event. This specification does not mention any specific limit, but it should be large enough for clients to be able to pin all existing non-finalized blocks and a few finalized blocks.

**Note**: A JSON-RPC client should call `chainHead_v1_unpin` only after it is sure to no longer be interested in a certain block. This typically happens after the block has been finalized or pruned. There is no requirement to call `chainHead_v1_unpin` as quickly as possible.

If a JSON-RPC client maintains mutiple `chainHead_v1_follow` subscriptions at the same time, it has no guarantee that the blocks reported by the various subscriptions are the same. While the finalized blocks reported should eventually be the same, it is possible that in the short term some subscriptions lag behind others.

**Note**: For example, imagine there exists two active `chainHead_v1_follow` subscriptions named A and B. Block N is announced on the peer-to-peer network and is announced to A. But then a sibling of block N gets finalized, leading to block N being pruned. Block N might never be announced to B.

#### About the runtime

The various fields of `spec` are:

- `specVersion`: Opaque version number. The JSON-RPC client can assume that the call to `Metadata_metadata` will always produce the same output as long as the `specVersion` is the same.
- `transactionVersion`: Opaque version number. Necessary when building the bytes of an extrinsic. Extrinsics that have been generated with a different `transactionVersion` are incompatible.
- `apis`: Object containing a list of "entry point APIs" supported by the runtime. Each key is the 8-bytes blake2 hash of the name of the API, and each value is a version number. Before making a runtime call (using `chainHead_v1_call`), you should make sure that this list contains the entry point API corresponding to the call and with a known version number.

**TODO**: detail the other fields

Example value:

```json
{
    "specName": "westend",
    "implName": "parity-westend",
    "authoringVersion": 2,
    "specVersion": 9122,
    "implVersion": 0,
    "transactionVersion": 7,
    "apis": {
        "0xdf6acb689907609b": 3
        "0x37e397fc7c91f5e4": 1,
        "0x40fe3ad401f8959a": 5,
        "0xd2bc9897eed08f15": 3,
        "0xf78b278be53f454c": 2,
        "0xaf2c0297a23e6d3d": 1,
        "0x49eaaf1b548a0cb0": 1,
        "0x91d5df18b0d2cf58": 1,
        "0xed99c5acb25eedf5": 3,
        "0xcbca25e39f142387": 2,
        "0x687ad44ad37f03c2": 1,
        "0xab3c0572291feb8b": 1,
        "0xbc9d89904f5b923f": 1,
        "0x37c8bb1350a9a2a8": 1
    }
}
```

**Note**: The format of `apis` is not the same as in the current JSON-RPC API.

If the node fails to compile the Wasm runtime blob of a block, `finalizedBlockRuntime` or `newRuntime` can be of the format `{"type": "invalid", "error": "..."}` where `error` is a human-readable string indicating why the node considers it as invalid. This string isn't meant to be shown to end users, but is for developers to understand the problem.

**Note**: The typical situation where a node could consider the runtime as invalid is a light client after a warp syncing. The light client knows that it's its fault for considering the runtime as invalid, but it has no better way to handle this situation than to return an error through the JSON-RPC interface for the error to get shown to the user.

# chainHead_v1_genesisHash

**Parameters**: *none*
**Return value**: String containing the hex-encoded hash of the genesis block of the chain.

This function is a simple getter. The JSON-RPC server is expected to keep in its memory the hash of the genesis block.

The value returned by this function must never change.

# chainHead_v1_header

**Parameters**:
    - `followSubscriptionId`: An opaque string that was returned by `chainHead_v1_follow`.
    - `hash`: String containing the hexadecimal-encoded hash of the header to retrieve.
**Return value**:
    - If the `followSubscriptionId` is still alive (the vast majority of the time), the hexadecimal-encoded SCALE-encoded header of the block.
    - If the `followSubscriptionId` is dead, *null*.

Retrieves the header of a pinned block.

This function should be seen as a complement to `chainHead_v1_follow`, allowing the JSON-RPC client to retrieve more information about a block that has been reported. Use `archive_header_v1` if instead you want to retrieve the header of an arbitrary block.

As explained in the documentation of `chainHead_v1_follow`, the JSON-RPC server reserves the right to kill an existing subscription and unpin all its blocks at any moment in case it is overloaded or incapable of following the chain. If that happens, `chainHead_v1_header` will return `null`.

## Possible errors

- A JSON-RPC error is generated if the `followSubscriptionId` is invalid.
- A JSON-RPC error is generated if the block hash passed as parameter doesn't correspond to any block that has been reported by `chainHead_v1_follow`.
- A JSON-RPC error is generated if the `followSubscriptionId` is valid but the block hash passed as parameter has already been unpinned.

# chainHead_v1_stopBody

**Parameters**:
    - `subscriptionId`: An opaque string that was returned by `chainHead_v1_body`.
**Return value**: *null*

Stops a body fetch started with `chainHead_v1_body`. If the body fetch was still in progress, this interrupts it. If the body fetch was already finished, this call has no effect.

JSON-RPC client implementations must be aware that, due to the asynchronous nature of JSON-RPC client <-> server communication, they might still receive a notification about this body fetch, for example because this notification was already in the process of being sent back by the JSON-RPC server.

## Possible errors

A JSON-RPC error is generated if the `subscriptionId` doesn't correspond to any active subscription.

# chainHead_v1_stopCall

**Parameters**:
    - `subscriptionId`: An opaque string that was returned by `chainHead_v1_call`.
**Return value**: *null*

Stops a call started with `chainHead_v1_call`. If the call was still in progress, this interrupts it. If the call was already finished, this call has no effect.

JSON-RPC client implementations must be aware that, due to the asynchronous nature of JSON-RPC client <-> server communication, they might still receive a notification about this call, for example because this notification was already in the process of being sent back by the JSON-RPC server.

# chainHead_v1_stopStorage

**Parameters**:
    - `followSubscriptionId`: An opaque string that was returned by `chainHead_v1_storage`.
**Return value**: *null*

Stops a storage fetch started with `chainHead_v1_storage`. If the storage fetch was still in progress, this interrupts it. If the storage fetch was already finished, this call has no effect.

JSON-RPC client implementations must be aware that, due to the asynchronous nature of JSON-RPC client <-> server communication, they might still receive a notification about this storage fetch, for example because this notification was already in the process of being sent back by the JSON-RPC server.

# chainHead_v1_storage

**Parameters**:
    - `followSubscriptionId`: An opaque string that was returned by `chainHead_v1_follow`.
    - `hash`: String containing an hexadecimal-encoded hash of the header of the block whose storage to fetch.
    - `key`: String containing the hexadecimal-encoded key to fetch in the storage.
    - `childKey`: `null` for main storage look-ups, or a string containing the hexadecimal-encoded key of the trie key of the trie that `key` refers to. **TODO**: I don't know enough about child tries to design this properly
    - `type`: String that must be equal to one of: `value`, `hash`, or `size`.
    - `networkConfig` (optional): Object containing the configuration of the networking part of the function. See above for details. Ignored if the JSON-RPC server doesn't need to perform a network request. Sensible defaults are used if not provided.
**Return value**: An opaque string that identifies the storage fetch in progress.

The JSON-RPC server must start obtaining the value of the entry with the given `key` (and possibly `childKey`) from the storage.

For optimization purposes, the JSON-RPC server is allowed to wait a little bit (e.g. up to 100ms) before starting to try fulfill the storage request, in order to batch multiple storage requests together.

This function will later generate notifications looking like this:

```json
{
    "jsonrpc": "2.0",
    "method": "chainHead_v1_storageEvent",
    "params": {
        "subscriptionId": "...",
        "result": ...
    }
}
```

If everything is successful, `result` will be:

```json
{
    "event": "done",
    "value": "0x0000000..."
}
```

Where `value` is:
- If `type` was `value`, either `null` if the storage doesn't contain a value at the given key, or a string containing the hex-encoded value of the storage entry.
- If `type` was `hash`, either `null` if the storage doesn't contain a value at the given key, or a string containing the hex-encoded hash of the value of the storage item. The hashing algorithm is the same as the one used by the trie of the chain.
- If `type` was `size`, either `null` if the storage doesn't contain a value at the given key, or a string containing the number of bytes of the storage entry. Note that a string is used rather than a number in order to prevent JavaScript clients from accidentally rounding the value.

Alternatively, if  `result` can also be:

```json
{
    "event": "failed"
}
```

Which indicates that the storage value has failed to be retrieved from the network.

Alternatively, if the `followSubscriptionId` is dead, then `result` can also be:

```json
{
    "event": "disjoint"
}
```

After an `"event": "done"`, `"event": "failed"`, or `"event": "disjoint"` is received, no more notification will be generated.

**Note**: Other events might be added in the future, such as reports on the progress of the fetch.

## Possible errors

- A JSON-RPC error is generated if `type` isn't one of the allowed values (similarly to a missing parameter or an invalid parameter type).
- If the networking part of the behaviour fails, then a `{"event": "failed"}` notification is generated (as explained above).
- A JSON-RPC error is generated if the `followSubscriptionId` is invalid.
- If the `followSubscriptionId` is dead, then a `{"event": "disjoint"}` notification is generated (as explained above).
- A JSON-RPC error is generated if the block hash passed as parameter doesn't correspond to any block that has been reported by `chainHead_v1_follow`.
- A JSON-RPC error is generated if the `followSubscriptionId` is valid but the block hash passed as parameter has already been unpinned.

# chainHead_v1_unfollow

**Parameters**:
    - `followSubscriptionId`: An opaque string that was returned by `chainHead_v1_follow`.
**Return value**: *null*

Stops a subscription started with `chainHead_v1_follow`.

JSON-RPC client implementations must be aware that, due to the asynchronous nature of JSON-RPC client <-> server communication, they might still receive chain updates notifications, for example because these notifications were already in the process of being sent back by the JSON-RPC server.

# chainHead_v1_unpin

**Parameters**:
    - `followSubscriptionId`: An opaque string that was returned by `chainHead_v1_follow`.
    - `hash`: String containing the hexadecimal-encoded hash of the header of the block to unpin.
**Return value**: *null*

See explanations in the documentation of `chainHead_v1_follow`.

## Possible errors

- A JSON-RPC error is generated if the `followSubscriptionId` doesn't correspond to any active subscription.
- A JSON-RPC error is generated if the block hash passed as parameter doesn't correspond to any block that has been reported by `chainHead_v1_follow`.
- A JSON-RPC error is generated if the `followSubscriptionId` is valid but the block hash passed as parameter has already been unpinned.
- No error is generated if the `followSubscriptionId` is dead. The call is simply ignored.

# chainHead_unstable_wasmQuery

**TODO**: allow passing a Wasm blob that is executed by a remote

# chainSpec_v1_chainName

**Parameters**: *none*
**Return value**: String containing the human-readable name of the chain.

The value returned by this function must never change.

# chainSpec_v1_genesisHash

**Parameters**: *none*
**Return value**: String containing the hex-encoded hash of the genesis block of the chain.

This function is a simple getter. The JSON-RPC server is expected to keep in its memory the hash of the genesis block.

The value returned by this function must never change.

# chainSpec_v1_properties

**Parameters**: *none*
**Return value**: *any*.

Returns the JSON payload found in the chain specification under the key `properties`. No guarantee is offered about the content of this object.

The value returned by this function must never change.

**TODO**: is that bad? stronger guarantees?

### extrinsic_v1_submitAndWatch

**Parameters**:
    - `extrinsic`: A hexadecimal-encoded SCALE-encoded extrinsic to try to include in a block.
**Return value**: An opaque string representing the subscription.

This function is similar to the current `author_submitAndWatchExtrinsic`. Note that `author_submitExtrinsic` is gone because it seems not useful.

Notifications format:

```json
{
    "jsonrpc": "2.0",
    "method": "extrinsic_watchEvent_v1",
    "params": {
        "subscriptionId": "...",
        "result": ...
    }
}
```

Where `result` is:

**TODO**: roughly the same as author_submitAndWatchExtrinsic, but needs to be written down

The node can drop a transaction (i.e. send back a `dropped` event extrinsic and discard the extrinsic altogether) if the transaction is invalid, if the JSON-RPC server's transactions pool is full, if the JSON-RPC server's resources have reached their limit, or the syncing requires a gap in the chain that prevents the JSON-RPC server from knowing whether the transaction has been included and/or finalized.

The JSON-RPC client should unconditionally call `extrinsic_v1_unwatch`, even if **TODO**.

A JSON-RPC error is generated if the `extrinsic` parameter has an invalid format, but no error is produced if the bytes of the `extrinsic`, once decoded, are invalid. Instead, a `dropped` notification will be generated.

### extrinsic_v1_unwatch

**Parameters**:
    - `subscription`: An opaque string equal to the value returned by `extrinsic_v1_submitAndWatch`
**Return value**: *null*

**Note**: This function does not remove the extrinsic from the pool. In other words, the node will still try to include the extrinsic in the chain. Having a function that removes the extrinsic from the pool would be almost useless, as the node might have already gossiped it to the rest of the network.

## Possible errors

A JSON-RPC error is generated if the `subscriptionId` doesn't correspond to any active subscription.

### rpc_methods

**Parameters**: *none*
**Return value**: an array of strings indicating the names of all the JSON-RPC functions supported by the JSON-RPC server.

### sudo_v1_addReservedPeer

**TODO**: same as `system_addReservedPeer` but properly defined

**TODO**: is this function actually necessary?

### sudo_v1_pendingExtrinsics

**Parameters**: *none*
**Return value**: an array of hexadecimal-encoded SCALE-encoded extrinsics that are in the transactions pool of the node

**TODO**: is this function actually necessary?

### sudo_v1_rotateKeys

**TODO**: same as `author_rotateKeys`

### sudo_v1_removeReservedPeer

**TODO**: same as `system_removeReservedPeer` but properly defined

**TODO**: is this function actually necessary?

### sudo_v1_version

**Parameters**: *none*
**Return value**: String containing a human-readable name and version of the implementation of the JSON-RPC server.

The return value shouldn't be parsed by a program. It is purely meant to be shown to a human.

**Note**: Example return values: "polkadot 0.9.12-ec34cf7e0-x86_64-linux-gnu", "smoldot-light 0.5.4"
