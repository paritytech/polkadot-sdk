# archive_unstable_stopStorage

**Parameters**:

- `subscription`: An opaque string that was returned by `archive_unstable_storage`.

**Return value**: *null*

Stops a storage fetch started with `archive_unstable_storage`. If the storage fetch was still in progress, this interrupts it. If the storage fetch was already finished, this call has no effect.

JSON-RPC client implementations must be aware that, due to the asynchronous nature of JSON-RPC client <-> server communication, they might still receive a notification about this storage fetch, for example because this notification was already in the process of being sent back by the JSON-RPC server.

## Possible errors

A JSON-RPC error is generated if the `subscription` doesn't correspond to any active subscription.
