# chainHead_unstable_continue

**Parameters**:

- `followSubscription`: An opaque string that was returned by `chainHead_v1_follow`.
- `operationId`: An opaque string that was returned by `chainHead_v1_storage`.

**Return value**: *null*

Resumes a storage fetch started with `chainHead_v1_storage` after it has generated an `operationWaitingForContinue` event.

Has no effect if the `operationId` is invalid or refers to an operation that has emitted a `{"event": "operationInaccessible"}` event, or if the `followSubscription` is invalid or stale.

## Possible errors

- A JSON-RPC error with error code `-32803` is generated if the `followSubscription` and `operationId` are valid but haven't generated a `operationWaitingForContinue` event.
- A JSON-RPC error with error code `-32602` is generated if one of the parameters doesn't correspond to the expected type (similarly to a missing parameter or an invalid parameter type).
