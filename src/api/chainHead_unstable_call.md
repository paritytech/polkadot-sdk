# chainHead_unstable_call

**Parameters**:

- `followSubscription`: An opaque string that was returned by `chainHead_unstable_follow`. The `withRuntime` parameter of the call must have been equal to `true`.
- `hash`: String containing the hexadecimal-encoded hash of the header of the block to make the call against.
- `function`: Name of the runtime entry point to call as a string.
- `callParameters`: Hexadecimal-encoded SCALE-encoded value to pass as input to the runtime function.
- `networkConfig` (optional): Object containing the configuration of the networking part of the function. See [here](./api.md) for details. Ignored if the JSON-RPC server doesn't need to perform a network request. Sensible defaults are used if not provided.

**Return value**: String containing an opaque value representing the operation.

The JSON-RPC server must invoke the entry point of the runtime of the given block using the storage of the given block.

**Note**: The runtime is still allowed to call host functions with side effects, however these side effects must be discarded. For example, a runtime function call can try to modify the storage of the chain, but this modification must not be actually applied. The only motivation for performing a call is to obtain the return value.

The operation will continue even if the given block is unpinned while it is in progress.

This function should be seen as a complement to `chainHead_unstable_follow`, allowing the JSON-RPC client to retrieve more information about a block that has been reported. Use `archive_unstable_call` if instead you want to call the runtime of an arbitrary block.

**Note**: This can be used as a replacement for the legacy `state_getMetadata`, `system_accountNextIndex`, and `payment_queryInfo` functions.

## Notifications format

This function will later generate a notification in the following format:

```json
{
    "jsonrpc": "2.0",
    "method": "chainHead_unstable_callEvent",
    "params": {
        "subscription": "...",
        "result": ...
    }
}
```

Where `subscription` is the value returned by this function, and `result` can be one of:

### done

```json
{
    "event": "done",
    "output": "0x0000000..."
}
```

The `done` event indicates that everything was successful.

`output` is the hexadecimal-encoded output of the runtime function call.

No more event will be generated with this `subscription`.

### inaccessible

```json
{
    "event": "inaccessible"
}
```

The `inaccessible` event is produced if the JSON-RPC server was incapable of obtaining the storage items necessary for the call.

Contrary to the `error` event, repeating the same call in the future might succeed.

No more event will be generated with this `subscription`.

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

No more event will be generated with this `subscription`.

### disjoint

```json
{
    "event": "disjoint"
}
```

The `disjoint` event indicates that the provided `followSubscription` is invalid or stale.

No more event will be generated with this `subscription`.

## Possible errors

- If the networking part of the behaviour fails, then an `{"event": "inaccessible"}` notification is generated (as explained above).
- If the `followSubscription` is invalid or stale, then a `{"event": "disjoint"}` notification is generated (as explained above).
- A JSON-RPC error is generated if the `followSubscription` corresponds to a follow where `withRuntime` was `̀false`.
- A JSON-RPC error is generated if the block hash passed as parameter doesn't correspond to any block that has been reported by `chainHead_unstable_follow`.
- A JSON-RPC error is generated if the `followSubscription` is valid but the block hash passed as parameter has already been unpinned.
- If the method to call doesn't exist in the Wasm runtime of the chain, then an `{"event": "error"}` notification is generated.
- If the runtime call fails (e.g. because it triggers a panic in the runtime, running out of memory, etc., or if the runtime call takes too much time), then an `{"event": "error"}` notification is generated.

## About `callParameters`

Runtime entry points typically accept multiple input parameters, but this JSON-RPC function accepts as parameter a single hexadecimal-encoded value. The reason is that all the parameters are concatenated together before performing the call anyway. There is no reason to force JSON-RPC clients to provide a proper division between the various parameters if they are all concatenated in the implementation anyway.
