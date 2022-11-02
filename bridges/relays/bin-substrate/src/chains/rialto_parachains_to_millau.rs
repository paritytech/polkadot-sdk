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

//! Rialto-to-Millau parachains sync entrypoint.

use crate::cli::bridge::{CliBridgeBase, MessagesCliBridge, ParachainToRelayHeadersCliBridge};
use parachains_relay::ParachainsPipeline;
use relay_millau_client::Millau;
use relay_rialto_client::Rialto;
use relay_rialto_parachain_client::RialtoParachain;
use substrate_relay_helper::parachains::{
	DirectSubmitParachainHeadsCallBuilder, SubstrateParachainsPipeline,
};

/// Rialto-to-Millau parachains sync description.
#[derive(Clone, Debug)]
pub struct RialtoParachainsToMillau;

impl ParachainsPipeline for RialtoParachainsToMillau {
	type SourceChain = Rialto;
	type TargetChain = Millau;
}

impl SubstrateParachainsPipeline for RialtoParachainsToMillau {
	type SourceParachain = RialtoParachain;
	type SourceRelayChain = Rialto;
	type TargetChain = Millau;

	type SubmitParachainHeadsCallBuilder = RialtoParachainsToMillauSubmitParachainHeadsCallBuilder;

	const SOURCE_PARACHAIN_PARA_ID: u32 = bp_rialto_parachain::RIALTO_PARACHAIN_ID;
}

/// `submit_parachain_heads` call builder for Rialto-to-Millau parachains sync pipeline.
pub type RialtoParachainsToMillauSubmitParachainHeadsCallBuilder =
	DirectSubmitParachainHeadsCallBuilder<
		RialtoParachainsToMillau,
		millau_runtime::Runtime,
		millau_runtime::WithRialtoParachainsInstance,
	>;

//// `RialtoParachain` to `Millau` bridge definition.
pub struct RialtoParachainToMillauCliBridge {}

impl CliBridgeBase for RialtoParachainToMillauCliBridge {
	type Source = RialtoParachain;
	type Target = Millau;
}

impl ParachainToRelayHeadersCliBridge for RialtoParachainToMillauCliBridge {
	type SourceRelay = Rialto;
	type ParachainFinality = RialtoParachainsToMillau;
	type RelayFinality = crate::chains::rialto_headers_to_millau::RialtoFinalityToMillau;
}

impl MessagesCliBridge for RialtoParachainToMillauCliBridge {
	const ESTIMATE_MESSAGE_FEE_METHOD: &'static str =
		bp_millau::TO_MILLAU_ESTIMATE_MESSAGE_FEE_METHOD;
	type MessagesLane =
		crate::chains::rialto_parachain_messages_to_millau::RialtoParachainMessagesToMillau;
}
