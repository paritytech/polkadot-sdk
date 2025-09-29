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

The pallet includes **event-aware weight accounting** for `finalize_block()` operations through the `OnFinalizeBlockParts`
trait. The weight model uses differential benchmarking to precisely account for the computational cost of processing
events during Ethereum block construction:

```text
Total Weight = fixed_part +
               Σ(per_tx_part(payload_i)) +
               Σ(per_event_part(data_len_j))
```

**High-Level Weight API (`OnFinalizeBlockParts` trait):**
The pallet exposes these weight calculation methods for runtime use:
- **Fixed cost**: `on_finalize_block_fixed()` - Base overhead regardless of transaction/event count
- **Per-transaction cost**: `on_finalize_block_per_tx(payload_size)` - Applied incrementally during each `eth_call()`
- **Per-event cost**: `on_finalize_block_per_event(data_len)` - Applied dynamically during each `deposit_event()`

**Underlying Benchmark Functions (`WeightInfo` trait):**
These low-level benchmarks measure raw computational costs and are used to derive the high-level weights:
- **Per-transaction overhead**: `on_finalize_per_transaction(n)` - Measures cost scaling with `n` transaction count
- **Per-transaction data**: `on_finalize_per_transaction_data(d)` - Measures cost scaling with `d` bytes of transaction payload
- **Per-event overhead**: `on_finalize_per_event(e)` - Measures cost scaling with `e` event count
- **Per-event data**: `on_finalize_per_event_data(d)` - Measures cost scaling with `d` bytes of event data

**Weight Derivation Methodology:**
The high-level API methods use differential calculation to isolate marginal costs from benchmarks:
- Per-transaction base: `on_finalize_per_transaction(1) - on_finalize_per_transaction(0)`
- Per-transaction byte: `on_finalize_per_transaction_data(1) - on_finalize_per_transaction_data(0)`
- Per-event base: `on_finalize_per_event(1) - on_finalize_per_event(0)`
- Per-byte of event data: `on_finalize_per_event_data(data_len) - on_finalize_per_event_data(0)`

This comprehensive weight model ensures that:
- Transactions emitting many events are properly weighted based on event count and data size
- Resource exhaustion attacks via oversized event data are prevented through proactive weight enforcement
- Accurate block packing calculations include all processing costs (bloom filters, RLP encoding, log conversion)
- Gas limit enforcement occurs early in `eth_call()` to prevent block overruns

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
