# archive_unstable_header

**Parameters**:

- `hash`: String containing the hexadecimal-encoded hash of the header to retreive.
- `networkConfig` (optional): Object containing the configuration of the networking part of the function. See [here](./api.md) for details. Ignored if the JSON-RPC server doesn't need to perform a network request. Sensible defaults are used if not provided.

**Return value**: String containing an opaque value representing the operation.

This function will later generate a notification in the following format:

```json
{
    "jsonrpc": "2.0",
    "method": "archive_unstable_headerEvent",
    "params":{
        "subscription": "...",
        "result": ...
    }
}
```

Where `result` can be:

```json
{
    "event": "done",
    "output": ...
}
```

Where `output` is a string containing the hexadecimal-encoded SCALE-codec encoding of the header of the block.

Alternatively, `result` can also be:

```json
{
    "event": "inaccessible"
}
```

Only one notification will ever be generated.

**Note**: Other events might be added in the future, such as reports on the progress of the call.

## Possible errors

If the block hash passed as parameter doesn't correspond to any known block, then a `{"event": "inaccessible"}` notification is generated (as explained above).

If the networking part of the behaviour fails, then a `{"event": "inaccessible"}` notification is generated (as explained above).

Due to the way blockchains work, it is never possible to be certain that a block doesn't exist. For this reason, networking-related errors and unknown block errors are reported in the same way.
