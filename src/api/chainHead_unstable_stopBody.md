# chainHead_unstable_stopBody

**Parameters**:

- `subscriptionId`: An opaque string that was returned by `chainHead_unstable_body`.

**Return value**: *null*

Stops a body fetch started with `chainHead_unstable_body`. If the body fetch was still in progress, this interrupts it. If the body fetch was already finished, this call has no effect.

JSON-RPC client implementations must be aware that, due to the asynchronous nature of JSON-RPC client <-> server communication, they might still receive a notification about this body fetch, for example because this notification was already in the process of being sent back by the JSON-RPC server.
