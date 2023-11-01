// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Westend-to-Millau parachains sync entrypoint.

use crate::cli::bridge::{CliBridgeBase, ParachainToRelayHeadersCliBridge};
use relay_millau_client::Millau;
use relay_westend_client::Westend;
use substrate_relay_helper::parachains::{
	DirectSubmitParachainHeadsCallBuilder, SubstrateParachainsPipeline,
};

/// Westend-to-Millau parachains sync description.
#[derive(Clone, Debug)]
pub struct WestendParachainsToMillau;

impl SubstrateParachainsPipeline for WestendParachainsToMillau {
	type SourceParachain = relay_asset_hub_westend_client::AssetHubWestend;
	type SourceRelayChain = Westend;
	type TargetChain = Millau;

	type SubmitParachainHeadsCallBuilder = WestendParachainsToMillauSubmitParachainHeadsCallBuilder;
}

/// `submit_parachain_heads` call builder for Rialto-to-Millau parachains sync pipeline.
pub type WestendParachainsToMillauSubmitParachainHeadsCallBuilder =
	DirectSubmitParachainHeadsCallBuilder<
		WestendParachainsToMillau,
		millau_runtime::Runtime,
		millau_runtime::WithWestendParachainsInstance,
	>;

/// `WestendParachain` to `Millau` bridge definition.
pub struct AssetHubWestendToMillauCliBridge {}

impl ParachainToRelayHeadersCliBridge for AssetHubWestendToMillauCliBridge {
	type SourceRelay = Westend;
	type ParachainFinality = WestendParachainsToMillau;
	type RelayFinality =
		crate::bridges::westend_millau::westend_headers_to_millau::WestendFinalityToMillau;
}

impl CliBridgeBase for AssetHubWestendToMillauCliBridge {
	type Source = relay_asset_hub_westend_client::AssetHubWestend;
	type Target = Millau;
}

/// TODO: Note: I know this does not belong here, but I don't want to add it to the
/// `chain-asset-hub-westend` or `chain-westend`, because we wont use it for production and I don't
/// want to bring this to the bridges subtree now. Anyway, we plan to retire millau/rialto, so this
/// hack will disappear with that.
pub mod relay_asset_hub_westend_client {
	use bp_runtime::{ChainId, UnderlyingChainProvider};
	use relay_substrate_client::Chain;
	use std::time::Duration;

	/// `AssetHubWestend` parachain definition
	#[derive(Debug, Clone, Copy)]
	pub struct AssetHubWestend;

	impl UnderlyingChainProvider for AssetHubWestend {
		type Chain = millau_runtime::bp_bridged_chain::AssetHubWestend;
	}

	// Westmint seems to use the same configuration as all Polkadot-like chains, so we'll use
	// Westend primitives here.
	impl Chain for AssetHubWestend {
		const ID: ChainId = bp_runtime::ASSET_HUB_WESTEND_CHAIN_ID;
		const NAME: &'static str = "Westmint";
		const BEST_FINALIZED_HEADER_ID_METHOD: &'static str =
			millau_runtime::bp_bridged_chain::BEST_FINALIZED_ASSETHUBWESTEND_HEADER_METHOD;
		const AVERAGE_BLOCK_INTERVAL: Duration = Duration::from_secs(6);

		type SignedBlock = bp_polkadot_core::SignedBlock;
		type Call = ();
	}
}
