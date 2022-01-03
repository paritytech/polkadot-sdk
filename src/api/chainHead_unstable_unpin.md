# chainHead_unstable_unpin

**Parameters**:

- `followSubscriptionId`: An opaque string that was returned by `chainHead_unstable_follow`.
- `hash`: String containing the hexadecimal-encoded hash of the header of the block to unpin.

**Return value**: *null*

See explanations in the documentation of `chainHead_unstable_follow`.

On-going calls to `chainHead_unstable_body`, `chainHead_unstable_call` and `chainHead_unstable_storage` against this block will still finish normally.

Has no effect if the `followSubscriptionId` is invalid or stale.

## Possible errors

- A JSON-RPC error is generated if the `followSubscriptionId` is valid but the block hash passed as parameter doesn't correspond to any block that has been reported by `chainHead_unstable_follow`.
- A JSON-RPC error is generated if the `followSubscriptionId` is valid but the block hash passed as parameter has already been unpinned.
- No error is generated if the `followSubscriptionId` is invalid or stale. The call is simply ignored.
