# Assets Freezer Pallet

A pallet capable of freezing fungibles from `pallet-assets`.

> Made with *Substrate*, for *Polkadot*.

[![github]](https://github.com/paritytech/polkadot-sdk/tree/master/substrate/frame/examples/basic)
[![polkadot]](https://polkadot.network)

[polkadot]: https://img.shields.io/badge/polkadot-E6007A?style=for-the-badge&logo=polkadot&logoColor=white
[github]: https://img.shields.io/badge/github-8da0cb?style=for-the-badge&labelColor=555555&logo=github

## Pallet API

See the [`pallet`] module for more information about the interfaces this pallet exposes,
including its configuration trait, dispatchables, storage items, events and errors.

## Overview

This pallet provides the following functionality:

- Pallet hooks that implement custom logic to let `pallet-assets` know whether an balance is
  frozen for an account on a given asset (see: [`pallet_assets::types::FrozenBalance`][docs:frozen_balance]).
- An implementation of fungibles [inspect][docs:inspect_freeze] and
  [mutation][docs:mutate_freeze] APIs.
- Support for force freezing and thawing assets, given a Freezer ID
  (see [`Config::RuntimeFreezeReason`][src:lib]).

[docs:frozen_balance]: https://docs.rs/pallet-assets/latest/pallet_assets/trait.FrozenBalance.html
[docs:inspect_freeze]: https://docs.rs/frame-support/latest/frame_support/traits/tokens/fungibles/index.html#reexport.InspectFreeze
[docs:mutate_freeze]: https://docs.rs/frame-support/latest/frame_support/traits/tokens/fungibles/index.html#reexport.MutateFreeze
[src:lib]: ./src/lib.rs
