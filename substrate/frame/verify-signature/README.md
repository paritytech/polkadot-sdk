# Verify Signature Module
A module that provides a `TransactionExtension` that validates a signature against a payload and
authorizes the origin.

## Overview

This module serves two purposes:
- `VerifySignature`: A `TransactionExtension` that checks the provided signature against a payload
  constructed through hashing the inherited implication with `blake2b_256`. If the signature is
  valid, then the extension authorizes the origin as signed. The extension can be disabled, or
  passthrough, allowing users to use other extensions to authorize different origins other than the
  traditionally signed origin.
- Benchmarking: The extension is bound within a pallet to leverage the benchmarking functionality in
  FRAME. The `Signature` and `Signer` types are specified in the pallet configuration and a
  benchmark helper trait is used to create a signature which is then validated in the benchmark.

[`Config`]: ./trait.Config.html

License: Apache-2.0
