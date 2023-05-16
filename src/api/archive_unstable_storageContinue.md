# archive_unstable_storageContinue

**Parameters**:

- `subscription`: An opaque string that was returned by `archive_unstable_storage`.

**Return value**: *null*

Resumes a storage fetch started with `archive_unstable_storage` after it has generated a `waiting-for-continue` event.

## Possible errors

- A JSON-RPC error is generated if the `subscription` is invalid or hasn't generated a `waiting-for-continue` event.
