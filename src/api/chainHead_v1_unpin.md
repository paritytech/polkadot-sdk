# chainHead_v1_unpin

**Parameters**:
    - `followSubscriptionId`: An opaque string that was returned by `chainHead_v1_follow`.
    - `hash`: String containing the hexadecimal-encoded hash of the header of the block to unpin.
**Return value**: *null*

See explanations in the documentation of `chainHead_v1_follow`.

## Possible errors

- A JSON-RPC error is generated if the `followSubscriptionId` doesn't correspond to any active subscription.
- A JSON-RPC error is generated if the block hash passed as parameter doesn't correspond to any block that has been reported by `chainHead_v1_follow`.
- A JSON-RPC error is generated if the `followSubscriptionId` is valid but the block hash passed as parameter has already been unpinned.
- No error is generated if the `followSubscriptionId` is dead. The call is simply ignored.
