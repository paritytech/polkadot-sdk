# Introduction

Functions with the `archive` prefix allow obtaining the state of the chain at any point in the present or in the past.

These functions are meant to be used to inspect the history of a chain. They can be used to access recent information as well, but JSON-RPC clients should keep in mind that the `chainHead` functions could be more appropriate.

These functions are typically expensive for a JSON-RPC server, because they likely have to perform either disk accesses or network requests. Consequently, JSON-RPC servers are encouraged to put a global limit on the number of concurrent calls to `archive`-prefixed functions.

# Usage

The JSON-RPC server exposes a finalized block height, which can be retrieved by calling `archive_unstable_finalizedHeight`.

Call `archive_unstable_hashByHeight` in order to obtain the hash of a block by its height.

If the height passed to `archive_unstable_hashByHeight` is inferior or equal to the value returned by `archive_unstable_finalizedHeight`, then it is always guaranteed that there is exactly one block with this hash.
The JSON-RPC client can then call `archive_unstable_header`, `archive_unstable_body`, `archive_unstable_storage`, and `archive_unstable_call` in order to obtain details about the block with this hash. It is always guaranteed to return a value.

If the height passed to `archive_unstable_hashByHeight` is strictly superior to the value returned by `archive_unstable_finalizedHeight`, then `archive_unstable_hashByHeight` might return zero, one, or more blocks. Furthermore, the list of blocks being returned can change at any point. It is also possible to call `archive_unstable_header`, `archive_unstable_body`, `archive_unstable_storage`, and `archive_unstable_call` on these blocks, but these functions might return `null` even if their hash was previously returned by `archive_unstable_hashByHeight`.
