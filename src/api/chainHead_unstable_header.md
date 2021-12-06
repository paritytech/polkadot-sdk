# chainHead_unstable_header

**Parameters**:

- `followSubscriptionId`: An opaque string that was returned by `chainHead_unstable_follow`.
- `hash`: String containing the hexadecimal-encoded hash of the header to retrieve.

**Return value**:

- If the `followSubscriptionId` is still alive (the vast majority of the time), the hexadecimal-encoded SCALE-encoded header of the block.
- If the `followSubscriptionId` is dead, *null*.

Retrieves the header of a pinned block.

This function should be seen as a complement to `chainHead_unstable_follow`, allowing the JSON-RPC client to retrieve more information about a block that has been reported. Use `archive_header_v1` if instead you want to retrieve the header of an arbitrary block.

**Note**: As explained in the documentation of `chainHead_unstable_follow`, the JSON-RPC server reserves the right to kill an existing subscription and unpin all its blocks at any moment in case it is overloaded or incapable of following the chain. If that happens, `chainHead_unstable_header` will return `null`.

## Possible errors

- A JSON-RPC error is generated if the `followSubscriptionId` is invalid.
- A JSON-RPC error is generated if the block hash passed as parameter doesn't correspond to any block that has been reported by `chainHead_unstable_follow`.
- A JSON-RPC error is generated if the `followSubscriptionId` is valid but the block hash passed as parameter has already been unpinned.
