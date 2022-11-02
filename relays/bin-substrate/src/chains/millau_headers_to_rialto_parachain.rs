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

//! Millau-to-RialtoParachain headers sync entrypoint.

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

//! Millau-to-RialtoParachain headers sync entrypoint.

use crate::cli::bridge::{CliBridgeBase, MessagesCliBridge, RelayToRelayHeadersCliBridge};
use substrate_relay_helper::finality::{
	engine::Grandpa as GrandpaFinalityEngine, DirectSubmitGrandpaFinalityProofCallBuilder,
	SubstrateFinalitySyncPipeline,
};

/// Description of Millau -> Rialto finalized headers bridge.
#[derive(Clone, Debug)]
pub struct MillauFinalityToRialtoParachain;

impl SubstrateFinalitySyncPipeline for MillauFinalityToRialtoParachain {
	type SourceChain = relay_millau_client::Millau;
	type TargetChain = relay_rialto_parachain_client::RialtoParachain;

	type FinalityEngine = GrandpaFinalityEngine<Self::SourceChain>;
	type SubmitFinalityProofCallBuilder = DirectSubmitGrandpaFinalityProofCallBuilder<
		Self,
		rialto_parachain_runtime::Runtime,
		rialto_parachain_runtime::MillauGrandpaInstance,
	>;
}

//// `Millau` to `RialtoParachain`  bridge definition.
pub struct MillauToRialtoParachainCliBridge {}

impl CliBridgeBase for MillauToRialtoParachainCliBridge {
	type Source = relay_millau_client::Millau;
	type Target = relay_rialto_parachain_client::RialtoParachain;
}

impl RelayToRelayHeadersCliBridge for MillauToRialtoParachainCliBridge {
	type Finality = MillauFinalityToRialtoParachain;
}

impl MessagesCliBridge for MillauToRialtoParachainCliBridge {
	const ESTIMATE_MESSAGE_FEE_METHOD: &'static str =
		bp_rialto_parachain::TO_RIALTO_PARACHAIN_ESTIMATE_MESSAGE_FEE_METHOD;
	type MessagesLane =
		crate::chains::millau_messages_to_rialto_parachain::MillauMessagesToRialtoParachain;
}
