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

use parachains_relay::ParachainsPipeline;
use relay_millau_client::Millau;
use relay_westend_client::{Westend, Westmint};
use substrate_relay_helper::parachains::{
	DirectSubmitParachainHeadsCallBuilder, SubstrateParachainsPipeline,
};

/// Westend-to-Millau parachains sync description.
#[derive(Clone, Debug)]
pub struct WestendParachainsToMillau;

impl ParachainsPipeline for WestendParachainsToMillau {
	type SourceChain = Westend;
	type TargetChain = Millau;
}

impl SubstrateParachainsPipeline for WestendParachainsToMillau {
	type SourceParachain = Westmint;
	type SourceRelayChain = Westend;
	type TargetChain = Millau;

	type SubmitParachainHeadsCallBuilder = WestendParachainsToMillauSubmitParachainHeadsCallBuilder;
	type TransactionSignScheme = Millau;

	const SOURCE_PARACHAIN_PARA_ID: u32 = bp_westend::WESTMINT_PARACHAIN_ID;
}

/// `submit_parachain_heads` call builder for Rialto-to-Millau parachains sync pipeline.
pub type WestendParachainsToMillauSubmitParachainHeadsCallBuilder =
	DirectSubmitParachainHeadsCallBuilder<
		WestendParachainsToMillau,
		millau_runtime::Runtime,
		millau_runtime::WithWestendParachainsInstance,
	>;
