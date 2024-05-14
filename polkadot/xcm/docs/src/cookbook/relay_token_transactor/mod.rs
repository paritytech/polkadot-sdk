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

//! # Relay Asset Transactor
//!
//! This example shows how to configure a parachain to only deal with the Relay Chain token.
//!
//! The first step is using the [`xcm_builder::FungibleAdapter`] to create an `AssetTransactor` that
//! can handle the relay chain token.
#![doc = docify::embed!("src/cookbook/relay_token_transactor/parachain/xcm_config.rs", asset_transactor)]
//!
//! The second step is to configure `IsReserve` to recognize the relay chain as a reserve for its
//! own asset.
//! With this, you'll be able to easily mint a derivative asset, backed one-to-one from the Relay
//! Chain, by using the xcm pallet's `transfer_assets` extrinsic.
//!
//! The `IsReserve` type takes a type that implements `ContainsPair<MultiAsset, MultiLocation>`.
//! In this case, we want a type that contains the pair `(relay_chain_native_token, relay_chain)`.
#![doc = docify::embed!("src/cookbook/relay_token_transactor/parachain/xcm_config.rs", is_reserve)]
//!
//! With this setup, we are able to do a reserve asset transfer to and from the parachain and relay
//! chain.
#![doc = docify::embed!("src/cookbook/relay_token_transactor/tests.rs", reserve_asset_transfers_work)]
//!
//! For the rest of the code, be sure to check the contents of this module.

/// The parachain runtime for this example
pub mod parachain;

/// The relay chain runtime for this example.
pub mod relay_chain;

/// The network for this example.
pub mod network;

/// Tests for this example.
#[cfg(test)]
pub mod tests;
