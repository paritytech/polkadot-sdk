# chainHead_unstable_stopCall

**Parameters**:

- `subscriptionId`: An opaque string that was returned by `chainHead_unstable_call`.

**Return value**: *null*

Stops a call started with `chainHead_unstable_call`. If the call was still in progress, this interrupts it. If the call was already finished, this call has no effect.

JSON-RPC client implementations must be aware that, due to the asynchronous nature of JSON-RPC client <-> server communication, they might still receive a notification about this call, for example because this notification was already in the process of being sent back by the JSON-RPC server.

## Possible errors

A JSON-RPC error is generated if the `subscriptionId` doesn't correspond to any active subscription.
