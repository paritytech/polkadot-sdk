// This file is part of polkadot-sdk.
//
// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! # FRAME Tokens
//!
//! This reference doc serves as a high-level overview of the token-related logic in FRAME, and
//! how to properly apply it to your use case.
//!
//! On completion of reading this doc, you should have a good understanding of:
//! - The distinction between token traits and trait implementations in FRAME, and why this
//!   distinction is helpful
//! - Token-related traits avaliable in FRAME
//! - Token-related trait implementations in FRAME
//! - How to choose the right trait or trait implementation for your use case
//! - Where to go next
//!
//! ## Getting Started
//!
//! The most ubiquitous way to add a token to a FRAME runtime is [`pallet_balances`]. Read
//! more about pallets [here](crate::polkadot_sdk::frame_runtime#pallets).
//!
//! You may then write custom pallets that interact with [`pallet_balances`]. The fastest way to
//! get started with that is by
//! [tightly coupling](crate::reference_docs::frame_pallet_coupling#tight-coupling-pallets) your
//! custom pallet to [`pallet_balances`].
//!
//! However, to keep pallets flexible and modular, it is often prefered to
//! [loosely couple](crate::reference_docs::frame_pallet_coupling#loosely--coupling-pallets).
//!
//! To achieve loose coupling,
//! we separate token logic into traits and trait implementations.
//!
//! ## Traits and Trait Implementations
//!
//! Broadly speaking, token logic in FRAME can be divided into two categories: traits and
//! trait implementations.
//!
//! **Traits** define common interfaces that types of tokens should implement. For example, the
//! [`fungible::Inspect`](`frame_support::traits::fungible::Inspect`) trait specifies an interface
//! for *inspecting* token state such as the total issuance of the token, the balance of individual
//! accounts, etc.
//!
//! **Trait implementations** are concrete implementations of these traits. For example, one of the
//! many traits [`pallet_balances`] implements is
//! [`fungible::Inspect`](`frame_support::traits::fungible::Inspect`)*. It provides the concrete way
//! of inspecting the total issuance, balance of accounts, etc. There can be many implementations of
//! the same traits.
//!
//! The distinction between traits and trait implementations is helpful because it allows pallets
//! and other logic to be generic over their dependencies, avoiding tight coupling.
//!
//! To illustrate this with an example let's consider [`pallet_preimage`]. This pallet takes a
//! deposit in exchange for storing a preimage for later use. A naive implementation of the
//! pallet may use [`pallet_balances`] in a tightly coupled manner, directly calling methods
//! on the pallet to reserve and unreserve deposits. This approach works well,
//! until someone has a use case requiring that an asset from a different pallet such as
//! [`pallet_assets`] is used for the deposit. Rather than tightly couple [`pallet_preimage`] to
//! [`pallet_balances`], [`pallet_assets`], and every other token-handling pallet a user
//! could possibly specify, [`pallet_preimage`] does not specify a concrete pallet as a dependency
//! but instead accepts any dependency which implements the
//! [`currency::ReservableCurrency`](`frame_support::traits::tokens::currency::ReservableCurrency`)
//! trait, namely via its [`Config::Currency`](`pallet_preimage::pallet::Config::Currency`)
//! associated type. This allows [`pallet_preimage`] to support any arbitrary pallet implementing
//! this trait, without needing any knowledge of what those pallets may be or requiring changes to
//! support new pallets which may be written in the future.
//!
//! Read more about coupling, and the benefits of loose coupling
//! [here](crate::reference_docs::frame_pallet_coupling).
//!
//! ##### *Rust Advanced Tip
//!
//! The knowledge that [`pallet_balances`] implements
//! [`fungible::Inspect`](`frame_support::traits::fungible::Inspect`) is not some arcane knowledge
//! that you have to know by heart or memorize. One can simply look at the list of the implementors
//! of any trait in the Rust Doc to find all implementors (e.g.
//! <https://paritytech.github.io/polkadot-sdk/master/frame/traits/tokens/fungible/trait.Mutate.html#implementors>),
//! or use the `rust-analyzer` `Implementations` action.
//!
//! ## Fungible Token Traits in FRAME
//!
//! The [`fungible`](`frame_support::traits::fungible`) crate contains the latest set of FRAME
//! fungible token traits, and is recommended to use for all new logic requiring a fungible token.
//! See the crate documentation for more info about these fungible traits.
//!
//! [`fungibles`](`frame_support::traits::fungibles`) provides very similar functionality to
//! [`fungible`](`frame_support::traits::fungible`), except it supports managing multiple tokens.
//!
//! You may notice the trait [`Currency`](`frame_support::traits::Currency`) with similar
//! functionality is also used in the codebase, however this trait is deprecated and existing logic
//! is in the process of being migrated to [`fungible`](`frame_support::traits::fungible`) ([tracking issue](https://github.com/paritytech/polkadot-sdk/issues/226)).
//!
//! ## Fungible Token Trait Implementations in FRAME
//!
//! [`pallet_balances`] implements [`fungible`](`frame_support::traits::fungible`), and is the most
//! commonly used fungible implementation in FRAME. Most of the time, it's used for managing the
//! native token of the blockchain network it's used in.
//!
//! [`pallet_assets`] implements [`fungibles`](`frame_support::traits::fungibles`), and is another
//! popular fungible token implementation. It supports the creation and management of multiple
//! assets in a single crate, making it a good choice when a network requires more assets in
//! addition to its native token.
//!
//! ## Non-Fungible Tokens in FRAME
//!
//! [`pallet_nfts`] is recommended to use for all NFT use cases in FRAME.
//! See the crate documentation for more info about this pallet.
//!
//! [`pallet_uniques`] is deprecated and should not be used.
//!
//!
//! # What Next?
//!
//! - If you are interested in implementing a single fungible token, continue reading the
//!   [`fungible`](`frame_support::traits::fungible`) and [`pallet_balances`] docs.
//! - If you are interested in implementing a set of fungible tokens, continue reading the
//!   [`fungibles`](`frame_support::traits::fungibles`) trait and [`pallet_assets`] docs.
//! - If you are interested in implementing an NFT, continue reading the [`pallet_nfts`] docs.
