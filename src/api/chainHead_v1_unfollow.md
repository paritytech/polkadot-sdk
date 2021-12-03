# chainHead_v1_unfollow

**Parameters**:
    - `followSubscriptionId`: An opaque string that was returned by `chainHead_v1_follow`.
**Return value**: *null*

Stops a subscription started with `chainHead_v1_follow`.

JSON-RPC client implementations must be aware that, due to the asynchronous nature of JSON-RPC client <-> server communication, they might still receive chain updates notifications, for example because these notifications were already in the process of being sent back by the JSON-RPC server.
