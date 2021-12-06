# extrinsic_unstable_unwatch

**Parameters**:

- `subscription`: Opaque string equal to the value returned by `extrinsic_unstable_submitAndWatch`

**Return value**: *null*

**Note**: This function does not remove the extrinsic from the pool. In other words, the node will still try to include the extrinsic in the chain. Having a function that removes the extrinsic from the pool would be almost useless, as the node might have already gossiped it to the rest of the network.

## Possible errors

A JSON-RPC error is generated if the `subscriptionId` doesn't correspond to any active subscription.
