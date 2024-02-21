# sudo_network_unstable_unwatch

**Parameters**:

- `subscription`: An opaque string that was returned by `sudo_network_unstable_watch`.

**Return value**: *null*

JSON-RPC client implementations must be aware that, due to the asynchronous nature of JSON-RPC client <-> server communication, they might still receive notifications concerning this subscription, for example because these notifications were already in the process of being sent back by the JSON-RPC server.

## Possible errors

A JSON-RPC error is generated if the `subscription` doesn't correspond to any active subscription.
