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

//! Helpers for implementing various message-related runtime API mthods.

use crate::messages::{target::FromBridgedChainMessagePayload, MessageBridge};

use bp_messages::{LaneId, MessageDetails, MessageKey, MessageNonce};
use codec::Decode;
use frame_support::weights::Weight;
use sp_std::vec::Vec;

/// Implementation of the `To*OutboundLaneApi::message_details`.
pub fn outbound_message_details<Runtime, MessagesPalletInstance, BridgeConfig, XcmWeigher>(
	lane: LaneId,
	begin: MessageNonce,
	end: MessageNonce,
) -> Vec<MessageDetails<Runtime::OutboundMessageFee>>
where
	Runtime: pallet_bridge_messages::Config<MessagesPalletInstance>,
	MessagesPalletInstance: 'static,
	BridgeConfig: MessageBridge,
	XcmWeigher: xcm_executor::traits::WeightBounds<()>,
{
	(begin..=end)
		.filter_map(|nonce| {
			let message_data =
				pallet_bridge_messages::Pallet::<Runtime, MessagesPalletInstance>::outbound_message_data(lane, nonce)?;
			Some(MessageDetails {
				nonce,
				// this shall match the similar code in the `FromBridgedChainMessageDispatch` - if we have failed
				// to decode or estimate dispatch weight, we'll just return 0 to disable actual execution
				dispatch_weight: compute_message_weight::<XcmWeigher>(
					MessageKey { lane_id: lane, nonce },
					&message_data.payload,
				).unwrap_or(0),
				size: message_data.payload.len() as _,
				delivery_and_dispatch_fee: message_data.fee,
				dispatch_fee_payment: bp_runtime::messages::DispatchFeePayment::AtTargetChain,
			})
		})
		.collect()
}

// at the source chain we don't know the type of target chain `Call` => `()` is used (it is
// similarly currently used in Polkadot codebase)
fn compute_message_weight<XcmWeigher: xcm_executor::traits::WeightBounds<()>>(
	message_key: MessageKey,
	encoded_payload: &[u8],
) -> Result<Weight, ()> {
	let mut payload = FromBridgedChainMessagePayload::<()>::decode(&mut &encoded_payload[..])
		.map_err(|e| {
			log::debug!(
				target: "runtime::bridge-dispatch",
				"Failed to decode outbound XCM message {:?}: {:?}",
				message_key,
				e,
			);
		})?;
	let weight = XcmWeigher::weight(&mut payload.xcm.1);
	let weight = weight.map_err(|e| {
		log::debug!(
			target: "runtime::bridge-dispatch",
			"Failed to compute dispatch weight of outbound XCM message {:?}: {:?}",
			message_key,
			e,
		);
	})?;
	Ok(weight)
}
