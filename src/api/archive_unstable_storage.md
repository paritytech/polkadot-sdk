# archive_unstable_storage

**Parameters**:

- `hash`: String containing an hexadecimal-encoded hash of the header of the block whose storage to fetch.
- `key`: String containing the hexadecimal-encoded key to fetch in the storage.
- `childKey`: `null` for main storage look-ups, or a string containing the hexadecimal-encoded key of the trie key of the trie that `key` refers to. **TODO**: I don't know enough about child tries to design this properly
- `type`: String that must be equal to one of: `value`, `hash`, or `size`.
- `networkConfig` (optional): Object containing the configuration of the networking part of the function. See [here](./api.md) for details. Ignored if the JSON-RPC server doesn't need to perform a network request. Sensible defaults are used if not provided.

**Return value**: String containing an opaque value representing the operation.

This function works the same way as `chainHead_unstable_storage`, except that it is not connected to a chain head follow, and no `disjoint` event can be generated.

Note that `chainHead_unstable_storage` and `archive_unstable_storage` should be treated as two completely separate functions. It is forbidden to call `archive_unstable_stopStorage` with a storage fetch started with `chainHead_unstable_storage`, and vice versa. Some JSON-RPC servers might support only one of these functions.
