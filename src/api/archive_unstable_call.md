# archive_unstable_call

**Parameters**:

- `hash`: String containing the hexadecimal-encoded hash of the header of the block to make the call against.
- `function`: Name of the runtime entry point to call as a string.
- `callParameters`: Hexadecimal-encoded SCALE-encoded value to pass as input to the runtime function.

**Return value**:

- If no block with that `hash` exists, `null`.
- If the call was successful, `{ "success": true, "value": ... }` where the `value` is a string containing the hexadecimal-encoded SCALE-encoded value returned by the runtime.
- If the call wasn't successful, `{ "success": false, "error": ... }` where the `error` is a human-readable message indicating the problem.

The JSON-RPC server must invoke the entry point of the runtime of the given block using the storage of the given block.

**Note**: The runtime is still allowed to call host functions with side effects, however these side effects must be discarded. For example, a runtime function call can try to modify the storage of the chain, but this modification must not be actually applied. The only motivation for performing a call is to obtain the return value.

In situations where the provided runtime function doesn't exist, or the runtime crashes, or similar, an error is returned. The `error` isn't meant to be shown to end users, but is for developers to understand the problem.

If the block was previously returned by `archive_unstable_hashByHeight` at a height inferior or equal to the current finalized block height (as indicated by `archive_unstable_finalizedHeight`), then calling this method multiple times is guaranteed to always return non-null and always the same result.

If the block was previously returned by `archive_unstable_hashByHeight` at a height strictly superior to the current finalized block height (as indicated by `archive_unstable_finalizedHeight`), then the block might "disappear" and calling this function might return `null` at any point.
