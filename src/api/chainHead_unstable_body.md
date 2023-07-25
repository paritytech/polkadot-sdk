# chainHead_unstable_body

**Parameters**:

- `followSubscription`: An opaque string that was returned by `chainHead_unstable_follow`.
- `hash`: String containing an hexadecimal-encoded hash of the header of the block whose body to fetch.

**Return value**: A JSON object.

The JSON object returned by this function has one the following formats:

### Started

```
{
    "result": "started",
    "operationId": ...
}
```

This return value indicates that the request has successfully started.

`operationId` is a string containing an opaque value representing the operation.

### LimitReached

```
{
    "result": "limitReached"
}
```

This return value indicates the request couldn't be started because the server is overloaded, or that the `followSubscription` is invalid or stale.

The JSON-RPC client should try again after an on-going [`chainHead_unstable_storage`], [`chainHead_unstable_body`], or [`chainHead_unstable_call`] operation finishes.

The JSON-RPC server must accept at least 16 concurrent operations for any given [`chainHead_unstable_follow`] subscription. In other words, as long as the JSON-RPC client makes sure that no more than 16 operations are in progress at any given item, it is guaranteed that all of its operations will be accepted by the JSON-RPC server.
For this purpose, each item requested through [`chainHead_unstable_storage`] counts as one operation, and each call to [`chainHead_unstable_body`] and [`chainHead_unstable_call`] counts as one operation.

## Overview

The JSON-RPC server must start obtaining the body (in other words the list of transactions) of the given block.

The progress of the operation is indicated through `operation-body-done`, `operation-inaccessible`, or `operation-error` notifications generated on the corresponding `chainHead_unstable_follow` subscription.

The operation continues even if the target block is unpinned with `chainHead_unstable_unpin`.

This function should be seen as a complement to `chainHead_unstable_follow`, allowing the JSON-RPC client to retrieve more information about a block that has been reported. Use `archive_unstable_body` if instead you want to retrieve the body of an arbitrary block.

## Possible errors

- If the networking part of the behaviour fails, then a `{"event": "operation-inaccessible"}` notification is generated (as explained above).
- If the `followSubscription` is invalid or stale, then `"result": "limitReached"` is returned (as explained above).
- A JSON-RPC error is generated if the block hash passed as parameter doesn't correspond to any block that has been reported by `chainHead_unstable_follow`.
- A JSON-RPC error is generated if the `followSubscription` is valid but the block hash passed as parameter has already been unpinned.
