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
use relay_westend_client::{Westend, Westmint};
use substrate_relay_helper::parachains::{
	DirectSubmitParachainHeadsCallBuilder, SubstrateParachainsPipeline,
};

/// Westend-to-Millau parachains sync description.
#[derive(Clone, Debug)]
pub struct WestendParachainsToMillau;

impl SubstrateParachainsPipeline for WestendParachainsToMillau {
	type SourceParachain = Westmint;
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

//// `WestendParachain` to `Millau` bridge definition.
pub struct WestmintToMillauCliBridge {}

impl ParachainToRelayHeadersCliBridge for WestmintToMillauCliBridge {
	type SourceRelay = Westend;
	type ParachainFinality = WestendParachainsToMillau;
	type RelayFinality =
		crate::bridges::westend_millau::westend_headers_to_millau::WestendFinalityToMillau;
}

impl CliBridgeBase for WestmintToMillauCliBridge {
	type Source = Westmint;
	type Target = Millau;
}
