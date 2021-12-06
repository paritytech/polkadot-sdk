# chainHead_unstable_body

**Parameters**:

- `followSubscriptionId`: An opaque string that was returned by `chainHead_unstable_follow`.
- `hash`: String containing an hexadecimal-encoded hash of the header of the block whose body to fetch.
- `networkConfig` (optional): Object containing the configuration of the networking part of the function. See [here](./introduction.md) for details. Ignored if the JSON-RPC server doesn't need to perform a network request. Sensible defaults are used if not provided.

**Return value**: An opaque string that identifies the body fetch in progress.

The JSON-RPC server must start obtaining the body (in other words the list of transactions) of the given block.

This function should be seen as a complement to `chainHead_unstable_follow`, allowing the JSON-RPC client to retrieve more information about a block that has been reported. Use `archive_unstable_body` if instead you want to retrieve the body of an arbitrary block.

This function will later generate notifications in the following format:

```json
{
    "jsonrpc": "2.0",
    "method": "chainHead_unstable_bodyEvent",
    "params": {
        "subscriptionId": "...",
        "result": ...
    }
}
```

If everything is successful, `result` will be:

```json
{
    "event": "done",
    "value": [...]
}
```

Where `value` is an array of strings containing the hexadecimal-encoded SCALE-encoded extrinsics found in this block.

Alternatively, `result` can also be:

```json
{
    "event": "inaccessible"
}
```

Which indicates that the body has failed to be retrieved from the network.

Alternatively, if the `followSubscriptionId` is dead, then `result` can also be:

```json
{
    "event": "disjoint"
}
```

After an `"event": "done"`, `"event": "inaccessible"`, or `"event": "disjoint"` is received, no more notification will be generated.

**Note**: Other events might be added in the future, such as reports on the progress of the fetch.

## Possible errors

- If the networking part of the behaviour fails, then a `{"event": "inaccessible"}` notification is generated (as explained above).
- A JSON-RPC error is generated if the `followSubscriptionId` is invalid.
- If the `followSubscriptionId` is dead, then a `{"event": "disjoint"}` notification is generated (as explained above).
- A JSON-RPC error is generated if the block hash passed as parameter doesn't correspond to any block that has been reported by `chainHead_unstable_follow`.
- A JSON-RPC error is generated if the `followSubscriptionId` is valid but the block hash passed as parameter has already been unpinned.
