# archive_unstable_storageContinue

**Parameters**:

- `subscription`: An opaque string that was returned by `archive_unstable_storage`.

**Return value**: *null*

Resumes a storage fetch started with `archive_unstable_storage` after it has generated a `waiting-for-continue` event.

Has no effect if the `subscription` is invalid or refers to a subscription that has emitted a `{"event": "stop"}` event.

## Possible errors

- A JSON-RPC error is generated if the `subscription` is valid but hasn't generated a `waiting-for-continue` event.
