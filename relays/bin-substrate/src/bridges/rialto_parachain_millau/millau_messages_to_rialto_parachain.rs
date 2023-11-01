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

//! Millau-to-RialtoParachain messages sync entrypoint.

use relay_millau_client::Millau;
use relay_rialto_parachain_client::RialtoParachain;
use substrate_relay_helper::{
	messages_lane::{DirectReceiveMessagesDeliveryProofCallBuilder, SubstrateMessageLane},
	UtilityPalletBatchCallBuilder,
};

substrate_relay_helper::generate_receive_message_proof_call_builder!(
	MillauMessagesToRialtoParachain,
	MillauMessagesToRialtoParachainReceiveMessagesProofCallBuilder,
	relay_rialto_parachain_client::RuntimeCall::BridgeMillauMessages,
	relay_rialto_parachain_client::BridgeMessagesCall::receive_messages_proof
);

/// Description of Millau -> RialtoParachain messages bridge.
#[derive(Clone, Debug)]
pub struct MillauMessagesToRialtoParachain;

impl SubstrateMessageLane for MillauMessagesToRialtoParachain {
	type SourceChain = Millau;
	type TargetChain = RialtoParachain;

	type ReceiveMessagesProofCallBuilder =
		MillauMessagesToRialtoParachainReceiveMessagesProofCallBuilder;
	type ReceiveMessagesDeliveryProofCallBuilder = DirectReceiveMessagesDeliveryProofCallBuilder<
		Self,
		millau_runtime::Runtime,
		millau_runtime::WithRialtoParachainMessagesInstance,
	>;

	type SourceBatchCallBuilder = UtilityPalletBatchCallBuilder<Millau>;
	type TargetBatchCallBuilder = ();
}
