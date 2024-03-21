# chainHead_v1_header

**Parameters**:

- `followSubscription`: An opaque string that was returned by `chainHead_v1_follow`.
- `hash`: String containing the hexadecimal-encoded hash of the header to retrieve.

**Return value**:

- If the `followSubscription` is still alive (the vast majority of the time), the hexadecimal-encoded SCALE-encoded header of the block.
- If the `followSubscription` is invalid or stale, *null*.

Retrieves the header of a pinned block.

This function should be seen as a complement to `chainHead_v1_follow`, allowing the JSON-RPC client to retrieve more information about a block that has been reported. Use `archive_unstable_header` if instead you want to retrieve the header of an arbitrary block.

**Note**: As explained in the documentation of `chainHead_v1_follow`, the JSON-RPC server reserves the right to kill an existing subscription and unpin all its blocks at any moment in case it is overloaded or incapable of following the chain. If that happens, `chainHead_v1_header` will return `null`.

## Possible errors

- A JSON-RPC error with error code `-32801` is generated if the block hash passed as parameter doesn't correspond to any block that has been reported by `chainHead_v1_follow`, or the block hash has been unpinned.
- A JSON-RPC error with error code `-32602` is generated if one of the parameters doesn't correspond to the expected type (similarly to a missing parameter or an invalid parameter type).
- A JSON-RPC error with error code `-32603` is generated if the JSON-RPC server cannot interpret the block (hardware issues, corrupted database, disk failure etc).
