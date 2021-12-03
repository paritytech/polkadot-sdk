# chainHead_v1_genesisHash

**Parameters**: *none*
**Return value**: String containing the hex-encoded hash of the genesis block of the chain.

This function is a simple getter. The JSON-RPC server is expected to keep in its memory the hash of the genesis block.

The value returned by this function must never change.

# chainHead_v1_header

**Parameters**:
    - `followSubscriptionId`: An opaque string that was returned by `chainHead_v1_follow`.
    - `hash`: String containing the hexadecimal-encoded hash of the header to retrieve.
**Return value**:
    - If the `followSubscriptionId` is still alive (the vast majority of the time), the hexadecimal-encoded SCALE-encoded header of the block.
    - If the `followSubscriptionId` is dead, *null*.

Retrieves the header of a pinned block.

This function should be seen as a complement to `chainHead_v1_follow`, allowing the JSON-RPC client to retrieve more information about a block that has been reported. Use `archive_header_v1` if instead you want to retrieve the header of an arbitrary block.

As explained in the documentation of `chainHead_v1_follow`, the JSON-RPC server reserves the right to kill an existing subscription and unpin all its blocks at any moment in case it is overloaded or incapable of following the chain. If that happens, `chainHead_v1_header` will return `null`.

## Possible errors

- A JSON-RPC error is generated if the `followSubscriptionId` is invalid.
- A JSON-RPC error is generated if the block hash passed as parameter doesn't correspond to any block that has been reported by `chainHead_v1_follow`.
- A JSON-RPC error is generated if the `followSubscriptionId` is valid but the block hash passed as parameter has already been unpinned.

# chainHead_v1_stopBody

**Parameters**:
    - `subscriptionId`: An opaque string that was returned by `chainHead_v1_body`.
**Return value**: *null*

Stops a body fetch started with `chainHead_v1_body`. If the body fetch was still in progress, this interrupts it. If the body fetch was already finished, this call has no effect.

JSON-RPC client implementations must be aware that, due to the asynchronous nature of JSON-RPC client <-> server communication, they might still receive a notification about this body fetch, for example because this notification was already in the process of being sent back by the JSON-RPC server.

## Possible errors

A JSON-RPC error is generated if the `subscriptionId` doesn't correspond to any active subscription.

# chainHead_v1_stopCall

**Parameters**:
    - `subscriptionId`: An opaque string that was returned by `chainHead_v1_call`.
**Return value**: *null*

Stops a call started with `chainHead_v1_call`. If the call was still in progress, this interrupts it. If the call was already finished, this call has no effect.

JSON-RPC client implementations must be aware that, due to the asynchronous nature of JSON-RPC client <-> server communication, they might still receive a notification about this call, for example because this notification was already in the process of being sent back by the JSON-RPC server.

# chainHead_v1_stopStorage

**Parameters**:
    - `followSubscriptionId`: An opaque string that was returned by `chainHead_v1_storage`.
**Return value**: *null*

Stops a storage fetch started with `chainHead_v1_storage`. If the storage fetch was still in progress, this interrupts it. If the storage fetch was already finished, this call has no effect.

JSON-RPC client implementations must be aware that, due to the asynchronous nature of JSON-RPC client <-> server communication, they might still receive a notification about this storage fetch, for example because this notification was already in the process of being sent back by the JSON-RPC server.

# chainHead_v1_storage

**Parameters**:
    - `followSubscriptionId`: An opaque string that was returned by `chainHead_v1_follow`.
    - `hash`: String containing an hexadecimal-encoded hash of the header of the block whose storage to fetch.
    - `key`: String containing the hexadecimal-encoded key to fetch in the storage.
    - `childKey`: `null` for main storage look-ups, or a string containing the hexadecimal-encoded key of the trie key of the trie that `key` refers to. **TODO**: I don't know enough about child tries to design this properly
    - `type`: String that must be equal to one of: `value`, `hash`, or `size`.
    - `networkConfig` (optional): Object containing the configuration of the networking part of the function. See above for details. Ignored if the JSON-RPC server doesn't need to perform a network request. Sensible defaults are used if not provided.
**Return value**: An opaque string that identifies the storage fetch in progress.

The JSON-RPC server must start obtaining the value of the entry with the given `key` (and possibly `childKey`) from the storage.

For optimization purposes, the JSON-RPC server is allowed to wait a little bit (e.g. up to 100ms) before starting to try fulfill the storage request, in order to batch multiple storage requests together.

This function will later generate notifications looking like this:

```json
{
    "jsonrpc": "2.0",
    "method": "chainHead_v1_storageEvent",
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
    "value": "0x0000000..."
}
```

Where `value` is:
- If `type` was `value`, either `null` if the storage doesn't contain a value at the given key, or a string containing the hex-encoded value of the storage entry.
- If `type` was `hash`, either `null` if the storage doesn't contain a value at the given key, or a string containing the hex-encoded hash of the value of the storage item. The hashing algorithm is the same as the one used by the trie of the chain.
- If `type` was `size`, either `null` if the storage doesn't contain a value at the given key, or a string containing the number of bytes of the storage entry. Note that a string is used rather than a number in order to prevent JavaScript clients from accidentally rounding the value.

Alternatively, if  `result` can also be:

```json
{
    "event": "failed"
}
```

Which indicates that the storage value has failed to be retrieved from the network.

Alternatively, if the `followSubscriptionId` is dead, then `result` can also be:

```json
{
    "event": "disjoint"
}
```

After an `"event": "done"`, `"event": "failed"`, or `"event": "disjoint"` is received, no more notification will be generated.

**Note**: Other events might be added in the future, such as reports on the progress of the fetch.

## Possible errors

- A JSON-RPC error is generated if `type` isn't one of the allowed values (similarly to a missing parameter or an invalid parameter type).
- If the networking part of the behaviour fails, then a `{"event": "failed"}` notification is generated (as explained above).
- A JSON-RPC error is generated if the `followSubscriptionId` is invalid.
- If the `followSubscriptionId` is dead, then a `{"event": "disjoint"}` notification is generated (as explained above).
- A JSON-RPC error is generated if the block hash passed as parameter doesn't correspond to any block that has been reported by `chainHead_v1_follow`.
- A JSON-RPC error is generated if the `followSubscriptionId` is valid but the block hash passed as parameter has already been unpinned.

# chainHead_v1_unfollow

**Parameters**:
    - `followSubscriptionId`: An opaque string that was returned by `chainHead_v1_follow`.
**Return value**: *null*

Stops a subscription started with `chainHead_v1_follow`.

JSON-RPC client implementations must be aware that, due to the asynchronous nature of JSON-RPC client <-> server communication, they might still receive chain updates notifications, for example because these notifications were already in the process of being sent back by the JSON-RPC server.

# chainHead_v1_unpin

**Parameters**:
    - `followSubscriptionId`: An opaque string that was returned by `chainHead_v1_follow`.
    - `hash`: String containing the hexadecimal-encoded hash of the header of the block to unpin.
**Return value**: *null*

See explanations in the documentation of `chainHead_v1_follow`.

## Possible errors

- A JSON-RPC error is generated if the `followSubscriptionId` doesn't correspond to any active subscription.
- A JSON-RPC error is generated if the block hash passed as parameter doesn't correspond to any block that has been reported by `chainHead_v1_follow`.
- A JSON-RPC error is generated if the `followSubscriptionId` is valid but the block hash passed as parameter has already been unpinned.
- No error is generated if the `followSubscriptionId` is dead. The call is simply ignored.

# chainHead_unstable_wasmQuery

**TODO**: allow passing a Wasm blob that is executed by a remote
