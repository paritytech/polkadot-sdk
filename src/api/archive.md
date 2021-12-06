# Introduction

Functions with the `archive` prefix allow obtaining the state of the chain at any point in the present or in the past.

These functions are meant to be used to inspect the history of a chain. They can be used to access recent information as well, but JSON-RPC clients should keep in mind that the `chainHead` functions could be more appropriate.

These functions are typically expensive for a JSON-RPC server, because they likely have to perform either disk accesses or network requests. Consequently, JSON-RPC servers are encouraged to put a global limit on the number of concurrent calls to `archive`-prefixed functions.
