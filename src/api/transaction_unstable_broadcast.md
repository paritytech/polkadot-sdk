# transaction_unstable_broadcast

**Parameters**:

- `transaction`: String containing the hexadecimal-encoded SCALE-encoded transaction to try to include in a block.

**Return value**: String representing the operation, or `null` if the maximum number of broadcasted transactions has been reached.

The string returned by this function is opaque and its meaning can't be interpreted by the JSON-RPC client.

Once this function has been called, the JSON-RPC server will try to propagate this transaction over the peer-to-peer network until `transaction_unstable_stop` is called.

The JSON-RPC server must allow at least 4 transactions being broadcasted at the same time per JSON-RPC client.
Any attempt to broadcast more than 4 transactions simultaneously might result in `null` being returned.

The JSON-RPC server might check whether the transaction is valid before broadcasting it. If it does so and if the transaction is invalid, the server should silently do nothing and the JSON-RPC client is not informed of the problem. Invalid transactions should still count towards the limit to the number of simultaneously broadcasted transactions.
