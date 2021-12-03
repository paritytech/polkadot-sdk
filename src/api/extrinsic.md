# extrinsic_v1_submitAndWatch

**Parameters**:
    - `extrinsic`: A hexadecimal-encoded SCALE-encoded extrinsic to try to include in a block.
**Return value**: An opaque string representing the subscription.

This function is similar to the current `author_submitAndWatchExtrinsic`. Note that `author_submitExtrinsic` is gone because it seems not useful.

Notifications format:

```json
{
    "jsonrpc": "2.0",
    "method": "extrinsic_v1_watchEvent",
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

# extrinsic_v1_unwatch

**Parameters**:
    - `subscription`: An opaque string equal to the value returned by `extrinsic_v1_submitAndWatch`
**Return value**: *null*

**Note**: This function does not remove the extrinsic from the pool. In other words, the node will still try to include the extrinsic in the chain. Having a function that removes the extrinsic from the pool would be almost useless, as the node might have already gossiped it to the rest of the network.

## Possible errors

A JSON-RPC error is generated if the `subscriptionId` doesn't correspond to any active subscription.
