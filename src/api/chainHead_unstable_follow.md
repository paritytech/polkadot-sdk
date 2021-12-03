# chainHead_unstable_follow

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
    "method": "chainHead_unstable_followEvent",
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

The current finalized block reported in the `initialized` event, and each subsequent block reported with a `newBlock` event, is automatically considered by the JSON-RPC server as *pinned*. A block is guaranteed to not leave the node's memory for as long as it is pinned, making it possible to call functions such as `chainHead_unstable_header` on it. Blocks must be unpinned by the JSON-RPC client by calling `chainHead_unstable_unpin`.

A block is pinned only in the context of a specific subscription. If multiple `chainHead_unstable_follow` subscriptions exist, then each `(subscription, block)` tuple must be unpinned individually. Blocks stay pinned even if they have been pruned from the blockchain, and must always be unpinned by the JSON-RPC client.

The JSON-RPC server is strongly encouraged to enforce a limit to the maximum number of pinned blocks. If this limit is reached, it should then stop the subscription by emitting a `stop` event. This specification does not mention any specific limit, but it should be large enough for clients to be able to pin all existing non-finalized blocks and a few finalized blocks.

**Note**: A JSON-RPC client should call `chainHead_unstable_unpin` only after it is sure to no longer be interested in a certain block. This typically happens after the block has been finalized or pruned. There is no requirement to call `chainHead_unstable_unpin` as quickly as possible.

If a JSON-RPC client maintains mutiple `chainHead_unstable_follow` subscriptions at the same time, it has no guarantee that the blocks reported by the various subscriptions are the same. While the finalized blocks reported should eventually be the same, it is possible that in the short term some subscriptions lag behind others.

**Note**: For example, imagine there exists two active `chainHead_unstable_follow` subscriptions named A and B. Block N is announced on the peer-to-peer network and is announced to A. But then a sibling of block N gets finalized, leading to block N being pruned. Block N might never be announced to B.

#### About the runtime

The various fields of `spec` are:

- `specVersion`: Opaque version number. The JSON-RPC client can assume that the call to `Metadata_metadata` will always produce the same output as long as the `specVersion` is the same.
- `transactionVersion`: Opaque version number. Necessary when building the bytes of an extrinsic. Extrinsics that have been generated with a different `transactionVersion` are incompatible.
- `apis`: Object containing a list of "entry point APIs" supported by the runtime. Each key is the 8-bytes blake2 hash of the name of the API, and each value is a version number. Before making a runtime call (using `chainHead_unstable_call`), you should make sure that this list contains the entry point API corresponding to the call and with a known version number.

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
