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

//! # Tokens in Substrate
//!
//! This reference doc serves as a high-level overview of the token-related logic in Substrate and
//! how to properly apply it to your use case.
//!
//! On completion of reading this doc, you should have a good understanding of:
//! - The distinction between token traits and trait implementations in Substrate, and why this
//!   distinction is helpful
//! - Token-related traits avaliable in Substrate
//! - Token-related trait implementations in Substrate
//! - How to choose the right trait or trait implementation for your use case
//! - Where to go next
//!
//! ## Traits and Trait Implementations
//!
//! Broadly speaking, token logic in Substrate can be divided into two categories: traits and
//! trait implementations.
//!
//! **Traits** define common interfaces that types of tokens should implement. For example, the
//! [`frame_support::traits::fungible::Inspect`] trait specifies an interface for *inspecting*
//! token state such as the total issuance of the token, the balance of individual accounts, etc.
//!
//! **Trait implementations** are concrete implementations of these traits. For example, one of the
//! many traits [`pallet_balances`] implements is [`frame_support::traits::fungible::Inspect`].
//!
//! The distinction between traits and trait implementations is helpful because it allows pallets
//! and other logic to be generic over their dependencies, avoiding cumbersome pallet tight
//! coupling.
//!
//! To illustrate this with an example let's consider [`pallet_preimage`]. This pallet takes a
//! deposit in exchange for storing some preimage for use later. A naive implementation of the
//! pallet may use [`pallet_balances`] as a dependency, and directly call the methods exposed by
//! [`pallet_balances`] to reserve and unreserve deposits. This approach works well, until someone
//! has a use case requiring that an asset from a different pallet such as [`pallet_assets`] is
//! used for the deposit. Rather than tightly couple [`pallet_preimage`] to [`pallet_balances`] AND
//! [`pallet_assets`], along with every other token-handling pallet a user could possibly specify,
//! [`pallet_preimage`] does not specify a concrete pallet as a dependency but instead accepts any
//! dependency which implements the [`frame_support::traits::tokens::currency::ReservableCurrency`]
//! trait. This allows [`pallet_preimage`] to support any arbitrary pallet implementing this trait,
//! without needing any knowledge of what those pallets may be or requiring changes to support new
//! pallets which may be written in the future.
//!
//! ## Fungible Token Traits in Substrate
//!
//! The [`frame_support::traits::fungible`] crate contains the latest set of Substrate
//! fungible token traits, and is recommended to use for all new logic requiring a fungible tokens.
//! See the crate documentation for more info about these fungible traits.
//!
//! [`frame_support::traits::fungibles`] provides very similar functionality to
//! [`frame_support::traits::fungible`], except it supports managing multiple tokens.
//!
//! You may notice the trait [`frame_support::traits::Currency`] with similar functionality is also
//! used in the codebase, however this trait is deprecated and existing logic is in the process of
//! being migrated to [`frame_support::traits::fungible`] ([tracking issue](https://github.com/paritytech/polkadot-sdk/issues/226)).
//!
//! ## Fungible Token Trait Implementations in Substrate
//!
//! [`pallet_balances`] implements [`frame_support::traits::fungible`], and is the most commonly
//! used fungible implementation in Substrate. Most of the time, it's used for managing the native
//! token of the blockchain network.
//!
//! [`pallet_assets`] implements [`frame_support::traits::fungibles`], and is another popular
//! fungible token implementation. It supports the creation and management of multiple assets in a
//! single crate, making it a good choice when a network requires more assets in
//! addition to its native token.
//!
//! ## Non-Fungible Tokens in Substrate
//!
//! [`pallet_uniques`] is recommended to use for all NFT use cases in Substrate.
//! See the crate documentation for more info about this pallet.
//!
//! [`pallet_nfts`] is deprecated and should not be used.
//!
//! # What Next?
//!
//! - If you are interested in implementing a single fungible token, continue reading the
//!   [`frame_support::traits::fungible`] and [`pallet_balances`] docs.
//! - If you are interested in implementing a set of fungible tokens, continue reading the
//!   [`frame_support::traits::fungibles`] trait and [`pallet_assets`] docs.
//! - If you are interested in implementing an NFT, continue reading the [`pallet_uniques`] docs.
