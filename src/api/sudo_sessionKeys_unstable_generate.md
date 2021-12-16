# sudo_sessionKeys_unstable_generate

**Parameters**:

- `seed` (optional): TODO: what is that? I mean, it's the param for the function, but it doesn't explain

**Return value**: 

- If the runtime supports the function call (see below), an object of the form `{"result":"..."}` where `result` contains a string containing the hexadecimal-encoded output of the runtime function call.
- Otherwise, an object of the form `{"error":"..."}` where `error` is a human-readable error message indicating the problem. This string isn't meant to be shown to end users, but is for developers to understand the problem.

The JSON-RPC server must check that the runtime supports the `SessionKeys` API (64bits blake2 hash: `0xab3c0572291feb8b`) at version 1, and call the `SessionKeys_generate_session_keys` runtime function.
The runtime call is done against the current best block of the chain.

If there is no `SessionKeys` API being supported, or if it is not at version 1, the JSON-RPC server is allowed to call an alternative version of this function if it is sensible to do so. For example, if the `SessionKeys` API is updated to version 2 without a substantial change in the logic of `SessionKeys_generate_session_keys`, then the JSON-RPC server is allowed to call it as well. This specification should be updated if that happens.

Contrary to most other JSON-RPC functions that perform runtime function calls where side-effects are forbidden, this runtime must be allowed to call host functions that access the keys of the node (e.g. `ext_crypto_sr25519_generate_version_1`, `ext_crypto_ed25519_public_keys_version_1`, etc.).

**Note**: This can be used as a replacement for the legacy `author_rotateKeys` function.

## Possible errors

- `{"error":"..."}` is returned if the runtime doesn't support the given API.
- `{"error":"..."}` is returned if a problem happens during the call, such as a Wasm trap.
- `{"error":"..."}` is returned if the runtime attempts to modify the storage of the block.
