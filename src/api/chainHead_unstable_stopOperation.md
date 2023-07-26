# chainHead_unstable_stopOperation

**Parameters**:

- `followSubscription`: An opaque string that was returned by `chainHead_unstable_follow`.
- `operationId`: An opaque string that was returned by `chainHead_unstable_body`, `chainHead_unstable_call`, or `chainHead_unstable_storage`.

**Return value**: *null*

Stops an operation started with `chainHead_unstable_body`, `chainHead_unstable_call`, or `chainHead_unstable_storage`. If the operation was still in progress, this interrupts it. If the operation was already finished, this call has no effect.

Has no effect if the `followSubscription` is invalid or stale.

JSON-RPC client implementations must be aware that, due to the asynchronous nature of JSON-RPC client <-> server communication, they might still receive notifications about the operation, for example because a notification was already in the process of being sent back by the JSON-RPC server.
