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

//! Millau-to-Rialto headers sync entrypoint.

use crate::cli::bridge::{CliBridgeBase, MessagesCliBridge, RelayToRelayHeadersCliBridge};
use substrate_relay_helper::{
	finality::{DirectSubmitGrandpaFinalityProofCallBuilder, SubstrateFinalitySyncPipeline},
	finality_base::{engine::Grandpa as GrandpaFinalityEngine, SubstrateFinalityPipeline},
};

/// Description of Millau -> Rialto finalized headers bridge.
#[derive(Clone, Debug)]
pub struct MillauFinalityToRialto;

impl SubstrateFinalityPipeline for MillauFinalityToRialto {
	type SourceChain = relay_millau_client::Millau;
	type TargetChain = relay_rialto_client::Rialto;

	type FinalityEngine = GrandpaFinalityEngine<Self::SourceChain>;
}

impl SubstrateFinalitySyncPipeline for MillauFinalityToRialto {
	type SubmitFinalityProofCallBuilder = DirectSubmitGrandpaFinalityProofCallBuilder<
		Self,
		rialto_runtime::Runtime,
		rialto_runtime::MillauGrandpaInstance,
	>;
}

//// `Millau` to `Rialto` bridge definition.
pub struct MillauToRialtoCliBridge {}

impl CliBridgeBase for MillauToRialtoCliBridge {
	type Source = relay_millau_client::Millau;
	type Target = relay_rialto_client::Rialto;
}

impl RelayToRelayHeadersCliBridge for MillauToRialtoCliBridge {
	type Finality = MillauFinalityToRialto;
}

impl MessagesCliBridge for MillauToRialtoCliBridge {
	type MessagesLane =
		crate::bridges::rialto_millau::millau_messages_to_rialto::MillauMessagesToRialto;
}
