# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The semantic versioning guarantees cover the interface to the substrate runtime which
includes this pallet as a dependency. This module will also add storage migrations whenever
changes require it. Stability with regard to offchain tooling is explicitly excluded from
this guarantee: For example adding a new field to an in-storage data structure will require
changes to frontends to properly display it. However, those changes will still be regarded
as a minor version bump.

The interface provided to smart contracts will adhere to semver with one exception: Even
major version bumps will be backwards compatible with regard to already deployed contracts.
In other words: Upgrading this pallet will not break pre-existing contracts.

## [Unreleased]

### Added

- Add new `instantiate` RPC that allows clients to dry-run contract instantiation.

- Make storage and fields of `Schedule` private to the crate.
[1](https://github.com/paritytech/substrate/pull/8359)

- Add new version of `seal_random` which exposes additional information.
[1](https://github.com/paritytech/substrate/pull/8329)

- Add `seal_rent_params` contract callable function.
[1](https://github.com/paritytech/substrate/pull/8231)

## [v3.0.0] 2021-02-25

This version constitutes the first release that brings any stability guarantees (see above).

### Added

- Emit an event when a contract terminates (self-destructs).
[1](https://github.com/paritytech/substrate/pull/8014)

- Charge rent for code stored on the chain in addition to the already existing
rent that is payed for data storage.
[1](https://github.com/paritytech/substrate/pull/7935)

- Allow the runtime to configure per storage item costs in addition
to the already existing per byte costs.
[1](https://github.com/paritytech/substrate/pull/7819)

- Contracts are now deleted lazily so that the user who removes a contract
does not need to pay for the deletion of the contract storage.
[1](https://github.com/paritytech/substrate/pull/7740)

- Allow runtime authors to define chain extensions in order to provide custom
functionality to contracts.
[1](https://github.com/paritytech/substrate/pull/7548)
[2](https://github.com/paritytech/substrate/pull/8003)

- Proper weights which are fully automated by benchmarking.
[1](https://github.com/paritytech/substrate/pull/6715)
[2](https://github.com/paritytech/substrate/pull/7017)
[3](https://github.com/paritytech/substrate/pull/7361)

### Changes

- Collect the rent for one block during instantiation.
[1](https://github.com/paritytech/substrate/pull/7847)

- Instantiation takes a `salt` argument to allow for easier instantion of the
same code by the same sender.
[1](https://github.com/paritytech/substrate/pull/7482)

- Improve the information returned by the `contracts_call` RPC.
[1](https://github.com/paritytech/substrate/pull/7468)

- Simplify the node configuration necessary to add this module.
[1](https://github.com/paritytech/substrate/pull/7409)

### Fixed

- Consider the code size of a contract in the weight that is charged for
loading a contract from storage.
[1](https://github.com/paritytech/substrate/pull/8086)

- Fix possible overflow in storage size calculation
[1](https://github.com/paritytech/substrate/pull/7885)

- Cap the surcharge reward that can be claimed.
[1](https://github.com/paritytech/substrate/pull/7870)

- Fix a possible DoS vector where contracts could allocate too large buffers.
[1](https://github.com/paritytech/substrate/pull/7818)
