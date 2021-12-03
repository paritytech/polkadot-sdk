# extrinsic_v1_submitAndWatch

**Parameters**:

- `extrinsic`: String containing the hexadecimal-encoded SCALE-encoded extrinsic to try to include in a block.

**Return value**: String representing the subscription.

The string returned by this function is opaque and its meaning can't be interpreted by the JSON-RPC client. It is only meant to be potentially passed to `extrinsic_v1_unwatch`.

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
