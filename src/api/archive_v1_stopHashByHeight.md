# archive_v1_stopHashByHeight

**Parameters**:
    - `subscriptionId`: An opaque string that was returned by `archive_v1_hashByHeight`.
**Return value**: *null*

Stops a query started with `archive_v1_hashByHeight`. If the query was still in progress, this interrupts it. If the query was already finished, this call has no effect.

JSON-RPC client implementations must be aware that, due to the asynchronous nature of JSON-RPC client <-> server communication, they might still receive a notification about this call, for example because this notification was already in the process of being sent back by the JSON-RPC server.

## Possible errors

A JSON-RPC error is generated if the `subscriptionId` doesn't correspond to any active subscription.
