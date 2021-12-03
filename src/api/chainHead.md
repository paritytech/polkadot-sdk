# Introduction

Functions with the `chainHead` prefix allow tracking the head of the chain (in other words, the latest new and finalized blocks) and their storage.

The most important function in this category is `chainHead_unstable_follow`. It is the first function that is the user is expected to call, before (almost) any other. `chainHead_unstable_follow` returns the current list of blocks that are near the head of the chain, and generates notifications about new blocks. The `chainHead_unstable_body`, `chainHead_unstable_call`, `chainHead_unstable_header` and `chainHead_unstable_storage` functions can be used to obtain more details about the blocks that have been reported by `chainHead_unstable_follow`.

These functions are the functions most of the JSON-RPC clients will most commonly use. A JSON-RPC server implementation is encouraged to prioritize serving these functions over other functions, and to put pinned blocks in a quickly-accessible cache.
