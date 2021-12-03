# Introduction

Functions with the `archive` prefix allow obtaining the state of the chain at any point in the past.

These functions are meant to be used to inspect the history of a chain, and not recent information.

These functions are typically expensive for a JSON-RPC server, because they likely have to perform some disk access. Consequently, JSON-RPC servers are encouraged to put a global limit on the number of concurrent calls to `archive`-prefixed functions.
