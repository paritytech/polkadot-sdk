// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
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

//! # Foreign Assets and Asset Conversion Example
//!
//! This example shows how to configure a parachain (namely the Asset Para) to be able to have other
//! parachains register their native token as foreign assets, and how the other chains actually
//! register their token. Additionally, the example shows how we can create asset conversion pools
//! to trade foreign assets to the Asset Para's native token, and how this setup can be used to pay
//! XCM-execution fees in a foreign asset.
//!
//! The first step is using the [`pallet_assets::Config`] to create an `ForeignAssets` pallet
//! instance that allows sibling parachains to create their native token on our Asset Para.
#![doc = docify::embed!(
    "src/cookbook/foreign_assets_and_asset_conversion/asset_para/assets.rs",
    foreign_assets
)]
//! As the second step, we will configure the `AssetConvertion` pallet, which will create, and
//! manage liquidity pools for asset swaps. To achieve this, we must add another `pallet_assets`
//! instance, which will be used to manage a liquidity pool's token.
#![doc = docify::embed!(
    "src/cookbook/foreign_assets_and_asset_conversion/asset_para/assets.rs",
    asset_conversion
)]
//! Subsequently, we will configure XCM to allow for foreign assets to be automatically swapped into
//! the native asset to pay for XCM execution fees. After the groundwork of the previous steps, this
//! is very simple and can be done with the trader.
#![doc = docify::embed!(
    "src/cookbook/foreign_assets_and_asset_conversion/asset_para/xcm_config.rs",
    traders
)]
//! Once we have set up the facilities to create and swap foreign tokens, we need to ensure that
//! we can also send the tokens back and forth, which is done via the `IsTrustedTeleporter` config
//! that we define as follows.
//!
//! For the Simple Para:
#![doc = docify::embed!(
    "src/cookbook/foreign_assets_and_asset_conversion/simple_para/xcm_config.rs",
    teleport_config
)]
//! For the Asset Para:
#![doc = docify::embed!(
    "src/cookbook/foreign_assets_and_asset_conversion/asset_para/xcm_config.rs",
    teleport_config
)]
//!
//! Finally, in the test we show how the flow to create and use a foreign asset would look like, and
//! what events would be emitted by the chain.
#![doc = docify::embed!(
    "src/cookbook/foreign_assets_and_asset_conversion/tests.rs",
    registering_foreign_assets_work
)]
//! For the rest of the code, be sure to check the contents of this module.

/// The asset parachain runtime that will accept foreign tokens (the main stage of this example).
pub mod asset_para;

/// The parachain runtime that wants to register its token on the asset para as a foreign token.
pub mod simple_para;

/// The relay chain runtime for this example.
pub mod relay_chain;

/// The network for this example.
pub mod network;

/// Tests for this example.
#[cfg(test)]
pub mod tests;
