# extrinsic_unstable_submitAndWatch

**Parameters**:

- `extrinsic`: String containing the hexadecimal-encoded SCALE-encoded extrinsic to try to include in a block.

**Return value**: String representing the subscription.

The string returned by this function is opaque and its meaning can't be interpreted by the JSON-RPC client. It is only meant to be potentially passed to `extrinsic_unstable_unwatch`.

## Notifications format

This function will later generate a notification in the following format:

```json
{
    "jsonrpc": "2.0",
    "method": "extrinsic_unstable_watchEvent",
    "params": {
        "subscriptionId": "...",
        "result": ...
    }
}
```

Where `result` can be one of:

### validated

```json
{
    "event": "validated"
}
```

The `validated` event indicates that this extrinsic has been checked and is considered as valid by the runtime.

This extrinsic might still become invalid in the future, for example because a conflicting extrinsic is included in the chain in-between.

Multiple `validated` events can be generated during the lifetime of an extrinsic. If multiple `validated` events happen in a row, the JSON-RPC server is allowed to skip all but the last one.

**Note**: In theory, this event could include a field indicating the block against which this extrinsic was validated. It has been decided to not include this field for pragmatic reasons: implementing it might be complicated, and it is not very useful for a JSON-RPC client to know this information.

### broadcasted

```json
{
    "event": "broadcasted",
    "numPeers": ...
}
```

The `broadcasted` event indicates the number of other peers this extrinsic has been broadcasted to.

`numPeers` is the total number of individual peers this extrinsic has been broadcasted to.

The JSON-RPC server doesn't (and can't) offer any guarantee that these peers have received the extrinsic or have saved it in their own transactions pool. In other words, no matter how large the value in `numPeers` is, no guarantee is offered that shutting down the local node will lead to the extrinsic being included.

**Note**: In principle, a value of `numPeers` equal to 0 guarantees that shutting down the local node will lead to the extrinsic _not_ being included, assuming that no other node has submitted the same extrinsic. However, because JSON-RPC servers are allowed to delay or skip events, the JSON-RPC client can never be sure that `numPeers` was still equal to 0 when shutting down the node.

If multiple `broadcasted` events happen in a row, the JSON-RPC server is allowed to skip all but the last.

### bestChainBlockIncluded

```json
{
    "event": "bestChainBlockIncluded",
    "block": {
        "hash": "...",
        "index": "..."
    }
}
```

Or

```json
{
    "event": "bestChainBlockIncluded",
    "block": null
}
```

The ̀`bestChainBlockIncluded` event indicates which block of the best chain the extrinsic is included in.

`null` can be sent back in case the block is no longer in any block of the best chain. This is the state an extrinsic starts in.

`hash` is a string containing the hex-encoded hash of the header of the block. `index` is a string containing an integer indicating the 0-based index of this extrinsic within the body of this block.

If multiple `bestChainBlockIncluded` events happen in a row, the JSON-RPC server is allowed to skip all but the last.

### finalized

```json
{
    "event": "finalized",
    "block": {
        "hash": "...",
        "index": "..."
    }
}
```

The `finalized` event indicates that this extrinsic is present in a block of the chain that is finalized.

`hash` is a string containing the hex-encoded hash of the header of the block. `index` is a string containing an integer indicating the 0-based index of this extrinsic within the body of this block.

No more event will be generated about this extrinsic.

### error

```json
{
    "event": "error",
    "error": "..."
}
```

The `error` event indicates that an internal error within the client has happened.

Examples include: the runtime crashes, the runtime is missing the function to validate a extrinsic, the format of the value returned by the runtime is invalid, etc.

This typically indicates a bug in the runtime of the chain or an incompatibility between the client and the runtime of the chain, and there is nothing the end user can do to fix the problem.

The extrinsic that has been submitted will not be included in the chain by the local node, but it could be included by sending it via a different client implementation.

`error` is a human-readable error message indicating what happened. This string isn't meant to be shown to end users, but is for developers to understand the problem.

No more event will be generated about this extrinsic.

### invalid

```json
{
    "event": "invalid",
    "error": "..."
}
```

The `invalid` event indicates that the runtime has marked the extrinsic as invalid.

This can happen for a variety of reasons specific to the chain, such as a bad signature, bad nonce, not enough balance for fees, etc.

`error` is a human-readable error message indicating why the extrinsic is invalid. This string isn't meant to be shown to end users, but is for developers to understand the problem.

No more event will be generated about this extrinsic.

### dropped

```json
{
    "event": "dropped",
    "broadcasted": true,
    "error": "..."
}
```

The `dropped` event indicates that the client wasn't capable of keeping track of this extrinsic.

If the `broadcasted` field is `true`, then this extrinsic has been sent to other peers and might still be included in the chain in the future. No guarantee is offered that the extrinsic will be included in the chain even if `broadcasted` is ̀`true`. However, if `broadcasted` is `false`, then it is guaranteed that this extrinsic will not be included, unless it has been sent in parallel on a different node.

This can happen for example if the JSON-RPC server's extrinsics pool is full, if the JSON-RPC server's resources have reached their limit, if the block the extrinsic is included in takes too long to be finalized, or the syncing requires a gap in the chain that prevents the JSON-RPC server from knowing whether the extrinsic has been included and/or finalized.

`error` is a human-readable error message indicating why the extrinsic is invalid. This string isn't meant to be shown to end users, but is for developers to understand the problem.

No more event will be generated about this extrinsic.

## Extrinsic state

While an extrinsic is being watched, it has the following properties:

- `isValidated`: `yes` or `not-yet`. An extrinsic is initially `not-yet` validated. A `validated` event indicates that the extrinsic has now been validated. After a certain number of blocks, an extrinsic will automatically become `not-yet` validated again and needs to be validated again. No event is generated to indicate that an extrinsic is no longer validated, however a `validated` event will be generated again when an extrinsic is validated again.

- `bestChainBlockIncluded`: an optional block hash and index. An extrinsic is initially included in no block. It can automatically become included in a block of the best chain. A `bestChainBlockIncluded` event reports updates to this property.

- `numBroadcastedPeers`: _integer_. An extrinsic is initially broadcasted to 0 other peers. After an extrinsic is in the `isValidated: yes` and `bestChainBlockIncluded: none` states, the number of broadcaster peers can increase. This number never decreases and is never reset to 0, even if an extrinsic becomes `isValidated: not-yet`. The `broadcasted` event is used to report about updates to this value.

Note that these three properties are orthogonal to each other, except for the fact that `numBroadcastedPeers` can only increase when `isValidated: yes` and `bestChainBlockIncluded: none`. In particular, an extrinsic can be included in a block before being validated or broadcasted.

The `finalized`, `error`, `invalid`, and `dropped` event indicate that the extrinsic is no longer being watched. The state of the extrinsic is entirely discarded.

JSON-RPC servers are allowed to skip sending events as long as it properly keeps the JSON-RPC client up to date with the state of the extrinsic. In other words, multiple `validated`, `broadcasted`, or `bestChainBlockIncluded` events in a row might be merged into one.

**Note**: In order to implement this properly, JSON-RPC servers should maintain a buffer of three notifications (one for each property), and overwrite any unsent notification with a more recent status update.

## Possible errors

A JSON-RPC error is generated if the `extrinsic` parameter has an invalid format, but no error is produced if the bytes of the `extrinsic`, once decoded, are invalid. Instead, an `invalid` notification will be generated.
