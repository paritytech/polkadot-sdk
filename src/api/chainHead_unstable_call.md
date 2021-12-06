# chainHead_unstable_call

**Parameters**:

- `followSubscriptionId`: An opaque string that was returned by `chainHead_unstable_follow`. The `runtimeUpdates` parameter of the call must have been equal to `true`.
- `hash`: String containing the hexadecimal-encoded hash of the header of the block to make the call against.
- `function`: Name of the runtime entry point to call as a string.
- `callParameters`: Array containing a list of hexadecimal-encoded SCALE-encoded parameters to pass to the runtime function.
- `networkConfig` (optional): Object containing the configuration of the networking part of the function. See [here](./introduction.md) for details. Ignored if the JSON-RPC server doesn't need to perform a network request. Sensible defaults are used if not provided.

**Return value**: String containing an opaque value representing the operation.

This function should be seen as a complement to `chainHead_unstable_follow`, allowing the JSON-RPC client to retrieve more information about a block that has been reported. Use `archive_unstable_call` if instead you want to call the runtime of an arbitrary block.

**TODO**: in order to perform the runtime call, the implementation of this function will simply concatenate all the parameters (without any separator), so does it make sense for the JSON-RPC function to require to split them into an array?

**Note**: This can be used as a replacement for the legacy `state_getMetadata`, `system_accountNextIndex`, and `payment_queryInfo`.

## Notifications format

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

Where `subscriptionId` is the value returned by this function, and `result` can be one of:

### done

```json
{
    "event": "done",
    "output": "0x0000000..."
}
```

The `done` event indicates that everything was successful.

`output` is the hexadecimal-encoded output of the runtime function call.

No more event will be generated with this `subscriptionId`.

### inaccessible

```json
{
    "event": "inaccessible",
    "error": "..."
}
```

The `inaccessible` event is produced if the JSON-RPC server was incapable of obtaining the storage items necessary for the call.

`error` is a human-readable error message indicating why the call has failed. This string isn't meant to be shown to end users, but is for developers to understand the problem.

Contrary to the `error` event, repeating the same call in the future might succeed.

No more event will be generated with this `subscriptionId`.

### error

```json
{
    "event": "error",
    "error": "..."
}
```

The `error` event indicates a problem during the call to the runtime, such the function missing or a runtime panic.

Contrary to the `inaccessible` event, repeating the same call in the future will not succeed.

`error` is a human-readable error message indicating why the call has failed. This string isn't meant to be shown to end users, but is for developers to understand the problem.

No more event will be generated with this `subscriptionId`.

### disjoint

```json
{
    "event": "disjoint"
}
```

The `disjoint` event indicates that the provided `followSubscriptionId` is dead.

No more event will be generated with this `subscriptionId`.

## Possible errors

- If the networking part of the behaviour fails, then an `{"event": "inaccessible"}` notification is generated (as explained above).
- A JSON-RPC error is generated if the `followSubscriptionId` is invalid.
- A JSON-RPC error is generated if the `followSubscriptionId` corresponds to a follow where `runtimeUpdates` was `̀false`.
- If the `followSubscriptionId` is dead, then a `{"event": "disjoint"}` notification is generated (as explained above).
- A JSON-RPC error is generated if the block hash passed as parameter doesn't correspond to any block that has been reported by `chainHead_unstable_follow`.
- A JSON-RPC error is generated if the `followSubscriptionId` is valid but the block hash passed as parameter has already been unpinned.
- If the method to call doesn't exist in the Wasm runtime of the chain, then an `{"event": "error"}` notification is generated.
- If the runtime call fails (e.g. because it triggers a panic in the runtime, running out of memory, etc., or if the runtime call takes too much time), then an `{"event": "error"}` notification is generated.
