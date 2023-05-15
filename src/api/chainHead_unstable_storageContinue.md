# chainHead_unstable_storageContinue

**Parameters**:

- `subscription`: An opaque string that was returned by `chainHead_unstable_storage`.

**Return value**: *null*

Resumes a storage fetch started with `chainHead_unstable_storage`. If the storage fetch was still in progress, this interrupts it. If the storage fetch was already finished, this call has no effect.

## Possible errors

- A JSON-RPC error is generated if the `subscription` is invalid or hasn't generated a `waiting-for-continue` event.
