# Introduction

Functions with the `chainHead` prefix allow tracking the head of the chain (in other words, the latest new and finalized blocks) and their storage.

The most important function in this category is `chainHead_unstable_follow`. It is the first function that is the user is expected to call, before (almost) any other. `chainHead_unstable_follow` returns the current list of blocks that are near the head of the chain, and generates notifications about new blocks. The `chainHead_unstable_body`, `chainHead_unstable_call`, `chainHead_unstable_header` and `chainHead_unstable_storage` functions can be used to obtain more details about the blocks that have been reported by `chainHead_unstable_follow`.

These functions are the functions most of the JSON-RPC clients will most commonly use. A JSON-RPC server implementation is encouraged to prioritize serving these functions over other functions, and to put pinned blocks in a quickly-accessible cache.

## Usage

_This section contains a small beginner guide destined for JSON-RPC client users._

This beginner guide shows how to use the `chainHead` functions in order to know the value of a certain storage item.

1. Call `chainHead_unstable_follow` with `withRuntime: true` to obtain a `followSubscription`. This `followSubscription` will need to be passed when calling most of the other `chainHead`-prefixed functions. If at any point in the future the JSON-RPC server sends back a `{"event": "stop"}` notification, jump back to step 1.

2. When the JSON-RPC server sends back a `{"event": "initialized"}` notification with `subscription` equal to your `followSubscription`, store the value of `finalizedBlockHashes` found in that notification. If `finalizedBlockHashes` contains multiple values, the user is encouraged to use the last one. This is the hash of the last block that has been finalized, for the purpose of this guide it will be named the `currentFinalizedBlock`. For all other values in `finalizedBlockHashes`, the user is encouraged to call `chainHead_unstable_unpin` with the `followSubscription`.

3. Make sure that the `finalizedBlockRuntime` field of the event contains a field `type` containining `valid`, and that the `spec` -> `apis` object contains a key `0xd2bc9897eed08f15` whose value is `3`. This verifies that the runtime of the chain supports the `Metadata_metadata` function that we will call below (`0xd2bc9897eed08f15` is the 64bits blake2 hash of the ASCII string `Metadata`). If it is not the case, enter panic mode as the client software is incompatible with the current state of the blockchain.

4. Call `chainHead_unstable_call` with `hash` equal to the `currentFinalizedBlock` you've just retrieved, `function` equal to `Metadata_metadata`, and an empty `callParameters`.

5. If the JSON-RPC server sends back a `{"event": "operationInaccessible"}` notification, jump back to step 4 to try again. If the JSON-RPC server sends back a `{"event": "operationError"}` notification, enter panic mode. If the JSON-RPC server instead sends back a `{"event": "operationCallDone"}` notification, save the return value.

6. The return value you've just saved is called the metadata, prefixed with its SCALE-compact-encoded length. You must decode and parse this metadata. How to do this is out of scope of this small guide. The metadata contains information about the layout of the storage of the chain. Inspect it to determine how to find the storage item you're looking for.

7. In order to obtain a value in the storage, call `chainHead_unstable_storage` with `hash` equal to the `currentFinalizedBlock`, `key` the desired key, and `type` equal to `value`. If the JSON-RPC server instead sends back a `{"event": "operationInaccessible"}` notification, the value you're looking for is unfortunately inaccessible and you can either try again or give up. If the JSON-RPC server instead sends back a `{"event": "operationStorageItems"}` notification, you can find the desired value inside.

8. You are strongly encouraged to maintain [a `Set`](https://developer.mozilla.org/fr/docs/Web/JavaScript/Reference/Global_Objects/Set) of the blocks where the runtime changes. Whenever a `{"event": "newBlock"}` notification is received with `subscription` equal to your `followSubcriptionId`, and `newRuntime` is non-null, store the provided `blockHash` in this set.

9. Whenever a `{"event": "finalized"}` notification is received with `subscription` equal to your `followSubcriptionId`, call `chainHead_unstable_unpin` with `currentFinalizedBlock` hash, the `prunedBlockHashes`, and with each value in `finalizedBlockHashes` of the `finalized` event except for the last one. The last value in `finalizedBlockHashes` becomes your new `currentFinalizedBlock`. If one or more entries of `finalizedBlockHashes` is found in your `Set` (see step 7), remove them from the set and jump to step 3 as the metadata has likely been modified. Otherwise, jump to step 7.

Note that these steps are a bit complicated. Any serious user of the JSON-RPC interface is expected to implement high-level wrappers around the various JSON-RPC functions.

For example, if multiple storage values are desired, only step 7 should be repeated once per storage item. All other steps are application-wide.
