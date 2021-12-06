# chainHead_unstable_call

**Parameters**:

- `followSubscriptionId`: An opaque string that was returned by `chainHead_unstable_follow`.
- `hash`: String containing the hexadecimal-encoded hash of the header of the block to make the call against.
- `function`: Name of the runtime entry point to call as a string.
- `callParameters`: Array containing a list of hexadecimal-encoded SCALE-encoded parameters to pass to the runtime function.
- `networkConfig` (optional): Object containing the configuration of the networking part of the function. See [here](./introduction.md) for details. Ignored if the JSON-RPC server doesn't need to perform a network request. Sensible defaults are used if not provided.

**Return value**: An opaque string that identifies the call in progress.

**TODO**: in order to perform the runtime call, the implementation of this function will simply concatenate all the parameters (without any separator), so does it make sense for the JSON-RPC function to require to split them into an array?

This function will later generate a notification in the following format:

```json
{
    "jsonrpc": "2.0",
    "method": "chainHead_unstable_callEvent",
    "params": {
        "subscriptionId": "...",
        "result": ...
    }
}
```

Where `result` can be:

```json
{
    "event": "done",
    "output": "0x0000000..."
}
```

Where `output` is the hexadecimal-encoded output of the runtime function call.

Alternatively, `result` can also be:

```json
{
    "event": "failed",
    "error": "..."
}
```

Where `error` is a human-readable error message indicating why the call has failed. This string isn't meant to be shown to end users, but is for developers to understand the problem.

Alternatively, if the `followSubscriptionId` is dead, then `result` can also be:

```json
{
    "event": "disjoint"
}
```

Only one notification will ever be generated.

**Note**: Other events might be added in the future, such as reports on the progress of the call.

**Note**: This can be used as a replacement for `state_getMetadata`, `system_accountNextIndex`, and `payment_queryInfo`.

## Possible errors

**TODO**: more precise

- If the block hash passed as parameter doesn't correspond to any known block, then a `{"event":"failed","error":"..."}` notification is generated (as explained above).
- If the JSON-RPC server is incapable of executing the Wasm runtime of the given block, a JSON-RPC error should be returned.
- If the method to call doesn't exist in the Wasm runtime of the chain, **TODO**.
- If the runtime call fails (e.g. because it triggers a panic in the runtime, running out of memory, etc., or if the runtime call takes too much time), then **TODO**.
- If the networking part of the behaviour fails, then a `{"event":"failed","error":"..."}` notification is generated (as explained above).
