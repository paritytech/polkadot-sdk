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

//! RialtoParachain-to-Millau messages sync entrypoint.

use relay_millau_client::Millau;
use relay_rialto_parachain_client::RialtoParachain;
use substrate_relay_helper::{
	messages_lane::{DirectReceiveMessagesProofCallBuilder, SubstrateMessageLane},
	UtilityPalletBatchCallBuilder,
};

substrate_relay_helper::generate_receive_message_delivery_proof_call_builder!(
	RialtoParachainMessagesToMillau,
	RialtoParachainMessagesToMillauReceiveMessagesDeliveryProofCallBuilder,
	relay_rialto_parachain_client::RuntimeCall::BridgeMillauMessages,
	relay_rialto_parachain_client::BridgeMessagesCall::receive_messages_delivery_proof
);

/// Description of RialtoParachain -> Millau messages bridge.
#[derive(Clone, Debug)]
pub struct RialtoParachainMessagesToMillau;

impl SubstrateMessageLane for RialtoParachainMessagesToMillau {
	type SourceChain = RialtoParachain;
	type TargetChain = Millau;

	type ReceiveMessagesProofCallBuilder = DirectReceiveMessagesProofCallBuilder<
		Self,
		millau_runtime::Runtime,
		millau_runtime::WithRialtoParachainMessagesInstance,
	>;
	type ReceiveMessagesDeliveryProofCallBuilder =
		RialtoParachainMessagesToMillauReceiveMessagesDeliveryProofCallBuilder;

	type SourceBatchCallBuilder = ();
	type TargetBatchCallBuilder = UtilityPalletBatchCallBuilder<Millau>;
}
