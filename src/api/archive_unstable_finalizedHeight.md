# archive_unstable_finalizedHeight

**Parameters**: *none*

**Return value**: String containing the hexadecimal-encoded height of the current finalized block of the chain.

The value returned by this function must only ever increase over time. In other words, if calling this function returns `N`, then calling it again later must return a value superior or equal to `N`.

When implemented on a load balancer, keep in mind that the other functions of the `archive` namespace must always return the same value when the block's height is inferior or equal to the finalized block height indicated by this function. One possible implementation strategy is for this function to call `archive_unstable_finalizedHeight` on every node being load balanced and return the smallest value.

This function is expected to be called rarely by the JSON-RPC client. This function exists in order to give an indication of which blocks are accessible, and not for JSON-RPC clients to follow the finalized block. The `archive` namespace isn't meant to follow the head of the chain, and `chainHead` should be used instead in that situation.
