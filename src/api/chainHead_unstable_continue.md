# chainHead_unstable_continue

**Parameters**:

- `followSubscription`: An opaque string that was returned by `chainHead_unstable_follow`.
- `operationId`: An opaque string that was returned by `chainHead_unstable_storage`.

**Return value**: *null*

Resumes a storage fetch started with `chainHead_unstable_storage` after it has generated an `operationWaitingForContinue` event.

Has no effect if the `operationId` is invalid or refers to an operation that has emitted a `{"event": "operationInaccessible"}` event, or if the `followSubscription` is invalid or stale.

## Possible errors

- A JSON-RPC error is generated if the `followSubscription` and `operationId` are valid but haven't generated a `operationWaitingForContinue` event.
