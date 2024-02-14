# chainHead_unstable_unpin

**Parameters**:

- `followSubscription`: An opaque string that was returned by `chainHead_unstable_follow`.
- `hashOrHashes`: String or array of unique strings containing the hexadecimal-encoded hash of the header of the block to unpin.

**Return value**: *null*

See explanations in the documentation of `chainHead_unstable_follow`.

On-going calls to `chainHead_unstable_body`, `chainHead_unstable_call` and `chainHead_unstable_storage` against this block will still finish normally.

Has no effect if the `followSubscription` is invalid or stale.

If this function returns an error, then no block has been unpinned. An JSON-RPC server implementation is expected to start unpinning the blocks only after it has made sure that all the blocks could be unpinned.

## Possible errors

- A JSON-RPC error with error code `-32801` is generated if the `followSubscription` is valid but at least one of the block hashes passed as parameter doesn't correspond to any block that has been reported by `chainHead_unstable_follow`, or at least one of the block hashes has been unpinned.
- A JSON-RPC error with error code `-32804` is generated if the `hashOrHashes` parameter is an array and at least one of the block hashes is duplicated.
- A JSON-RPC error with error code `-32602` is generated if one of the parameters doesn't correspond to the expected type (similarly to a missing parameter or an invalid parameter type).
- No error is generated if the `followSubscription` is invalid or stale. The call is simply ignored.
