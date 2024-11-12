// Copyright Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! # Supporting other parachain tokens
//!
//! This example shows how to configure a parachain to be able to handle other parachain tokens.
//!
//! The most important item to configure is the `AssetTransactor`.
//! This is the item that implements [`xcm_executor::traits::TransactAsset`].
//! It's used for handling withdrawing, depositing and transferring assets.
//!
//! We need to reference other parachain tokens coming in XCMs.
//! The assets coming in have an [`xcm::latest::AssetId`] which is just a wrapper around a
//! [`xcm::latest::Location`].
//! We could map these locations to integers and reference the assets this way internally.
//! However, a simpler way is just using the locations themselves as ids.
//!
//! Here's a configuration of the assets pallet with xcm locations as ids.
#![doc = docify::embed!("src/cookbook/other_parachain_tokens/parachain/mod.rs", foreign_assets)]
//!
//! Given that, we can configure the following `AssetTransactor`.
//! It is the combination of 2 [`xcm_executor::traits::TransactAsset`] implementations:
//! - One for managing the native token
//! - Another for these "foreign assets", the other parachain tokens
//! It has one for managing the native token and another for these "foreign assets", other parachain
//! tokens.
#![doc = docify::embed!("src/cookbook/other_parachain_tokens/parachain/xcm_config.rs", asset_transactor)]

/// The parachain runtime for this example.
pub mod parachain;

/// The relay chain runtime for this example.
pub mod relay_chain;

/// The network for this example.
pub mod network;

/// Tests for this example
#[cfg(test)]
pub mod tests;
