# Revive Pallet

This is an **experimental** module that provides functionality for the runtime to deploy and execute PolkaVM
smart-contracts. It is a heavily modified `pallet_contracts` fork.

## Overview

This module extends accounts based on the [`frame_support::traits::fungible`] traits to have smart-contract
functionality. It can be used with other modules that implement accounts based on [`frame_support::traits::fungible`].
These "smart-contract accounts" have the ability to instantiate smart-contracts and make calls to other contract and
non-contract accounts.

The smart-contract code is stored once, and later retrievable via its `code_hash`. This means that multiple
smart-contracts can be instantiated from the same `code`, without replicating the code each time.

When a smart-contract is called, its associated code is retrieved via the code hash and gets executed. This call can
alter the storage entries of the smart-contract account, instantiate new smart-contracts, or call other smart-contracts.

Finally, when an account is reaped, its associated code and storage of the smart-contract account will also be deleted.

### Weight

Senders must specify a [`Weight`](https://paritytech.github.io/substrate/master/sp_weights/struct.Weight.html) limit
with every call, as all instructions invoked by the smart-contract require weight. Unused weight is refunded after the
call, regardless of the execution outcome.

If the weight limit is reached, then all calls and state changes (including balance transfers) are only reverted at the
current call's contract level. For example, if contract A calls B and B runs out of weight mid-call, then all of B's
calls are reverted. Assuming correct error handling by contract A, A's other calls and state changes still persist.

One `ref_time` `Weight` is defined as one picosecond of execution time on the runtime's reference machine.

#### Event-Aware Weight Accounting

The pallet includes **event-aware weight accounting** for `finalize_block()` operations. The weight model accounts for
the computational cost of processing events during Ethereum block construction:

```
Total Weight = fixed_cost + (transaction_count × per_tx_cost) + (event_count × per_event_cost)
```

This comprehensive weight model accounts for both the number of events and their data size, ensuring that:
- Transactions emitting many events are properly weighted based on event count
- Large event payloads are accounted for based on their data size in bytes
- Resource exhaustion attacks via oversized event data are prevented
- Accurate block packing calculations include all processing costs

The event processing costs include bloom filter computation, RLP encoding, and log conversion operations that occur
during `on_finalize()` when building Ethereum block headers. The data size component specifically accounts for the
computational overhead of processing variable-length event data during these operations.

### Revert Behaviour

Contract call failures are not cascading. When failures occur in a sub-call, they do not "bubble up", and the call will
only revert at the specific contract level. For example, if contract A calls contract B, and B fails, A can decide how
to handle that failure, either proceeding or reverting A's changes.

## Interface

### Dispatchable functions

Those are documented in the [reference
documentation](https://paritytech.github.io/polkadot-sdk/master/pallet_revive/pallet/dispatchables/index.html).

## Usage

This module executes PolkaVM smart contracts. These can potentially be written in any language that compiles to
RISC-V. For now, the only officially supported languages are Solidity (via [`revive`](https://github.com/xermicus/revive))
and Rust (check the `fixtures` directory for Rust examples).

## Host function tracing

For contract authors, it can be a helpful debugging tool to see which host functions are called, with which arguments,
and what the result was.

In order to see these messages on the node console, the log level for the `runtime::revive::strace` target needs to
be raised to the `trace` level.

Example:

```bash
cargo run --release -- --dev -lerror,runtime::revive::strace=trace,runtime::revive=debug
```

## Unstable Interfaces

Driven by the desire to have an iterative approach in developing new contract interfaces this pallet contains the
concept of an unstable interface. Akin to the rust nightly compiler it allows us to add new interfaces but mark them as
unstable so that contract languages can experiment with them and give feedback before we stabilize those.

In order to access interfaces which don't have a stable `#[stable]` in [`runtime.rs`](src/vm/runtime.rs)
one need to set `pallet_revive::Config::UnsafeUnstableInterface` to `ConstU32<true>`.
**It should be obvious that any production runtime should never be compiled with this feature: In addition to be
subject to change or removal those interfaces might not have proper weights associated with them and are therefore
considered unsafe**.

New interfaces are generally added as unstable and might go through several iterations before they are promoted to a
stable interface.

License: Apache-2.0
