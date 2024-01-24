# transaction_unstable_stop

**Parameters**:

- `operationId`: Opaque string equal to the value returned by `transaction_unstable_broadcast`

**Return value**: *null*

The node will no longer try to broadcast the transaction over the peer-to-peer network.

## Possible errors

A JSON-RPC error is generated if the `operationId` doesn't correspond to any active `transaction_unstable_broadcast` operation.
