// Copyright 2023 Parity Technologies (UK) Ltd.
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

//! Module provides utilities for easier XCM handling, e.g:
//! `XcmExecutor` -> `MessageSender` -> `OutboundMessageQueue`
//!                                             |
//!                                          `Relayer`
//!                                             |
//! `XcmRouter` <- `MessageDispatch` <- `InboundMessageQueue`

use bp_messages::{
	source_chain::MessagesBridge,
	target_chain::{DispatchMessage, MessageDispatch},
	LaneId,
};
use bp_runtime::messages::MessageDispatchResult;
use codec::{Decode, Encode};
use frame_support::{dispatch::Weight, CloneNoBound, EqNoBound, PartialEqNoBound};
use pallet_bridge_messages::WeightInfoExt as MessagesPalletWeights;
use scale_info::TypeInfo;
use sp_runtime::SaturatedConversion;
use xcm_builder::{DispatchBlob, DispatchBlobError, HaulBlob, HaulBlobError};

/// Plain "XCM" payload, which we transfer through bridge
pub type XcmAsPlainPayload = sp_std::prelude::Vec<u8>;

/// Message dispatch result type for single message
#[derive(CloneNoBound, EqNoBound, PartialEqNoBound, Encode, Decode, Debug, TypeInfo)]
pub enum XcmBlobMessageDispatchResult {
	InvalidPayload,
	Dispatched,
	NotDispatched(#[codec(skip)] Option<DispatchBlobError>),
}

/// [`XcmBlobMessageDispatch`] is responsible for dispatching received messages
pub struct XcmBlobMessageDispatch<DispatchBlob, Weights> {
	_marker: sp_std::marker::PhantomData<(DispatchBlob, Weights)>,
}

impl<BlobDispatcher: DispatchBlob, Weights: MessagesPalletWeights> MessageDispatch
	for XcmBlobMessageDispatch<BlobDispatcher, Weights>
{
	type DispatchPayload = XcmAsPlainPayload;
	type DispatchLevelResult = XcmBlobMessageDispatchResult;

	fn dispatch_weight(message: &mut DispatchMessage<Self::DispatchPayload>) -> Weight {
		match message.data.payload {
			Ok(ref payload) => {
				let payload_size = payload.encoded_size().saturated_into();
				Weights::message_dispatch_weight(payload_size)
			},
			Err(_) => Weight::zero(),
		}
	}

	fn dispatch(
		message: DispatchMessage<Self::DispatchPayload>,
	) -> MessageDispatchResult<Self::DispatchLevelResult> {
		let payload = match message.data.payload {
			Ok(payload) => payload,
			Err(e) => {
				log::error!(
					target: crate::LOG_TARGET_BRIDGE_DISPATCH,
					"[XcmBlobMessageDispatch] payload error: {:?} - message_nonce: {:?}",
					e,
					message.key.nonce
				);
				return MessageDispatchResult {
					unspent_weight: Weight::zero(),
					dispatch_level_result: XcmBlobMessageDispatchResult::InvalidPayload,
				}
			},
		};
		let dispatch_level_result = match BlobDispatcher::dispatch_blob(payload) {
			Ok(_) => {
				log::debug!(
					target: crate::LOG_TARGET_BRIDGE_DISPATCH,
					"[XcmBlobMessageDispatch] DispatchBlob::dispatch_blob was ok - message_nonce: {:?}",
					message.key.nonce
				);
				XcmBlobMessageDispatchResult::Dispatched
			},
			Err(e) => {
				log::error!(
					target: crate::LOG_TARGET_BRIDGE_DISPATCH,
					"[XcmBlobMessageDispatch] DispatchBlob::dispatch_blob failed, error: {:?} - message_nonce: {:?}",
					e, message.key.nonce
				);
				XcmBlobMessageDispatchResult::NotDispatched(Some(e))
			},
		};
		MessageDispatchResult { unspent_weight: Weight::zero(), dispatch_level_result }
	}
}

/// [`XcmBlobHauler`] is responsible for sending messages to the bridge "point-to-point link" from
/// one side, where on the other it can be dispatched by [`XcmBlobMessageDispatch`].
pub trait XcmBlobHauler {
	/// Runtime message sender adapter.
	type MessageSender: MessagesBridge<XcmAsPlainPayload>;

	/// Return message lane (as "point-to-point link") used to deliver XCM messages.
	fn xcm_lane() -> LaneId;
}

/// XCM bridge adapter which connects [`XcmBlobHauler`] with [`XcmBlobHauler::MessageSender`] and
/// makes sure that XCM blob is sent to the [`pallet_bridge_messages`] queue to be relayed.
pub struct XcmBlobHaulerAdapter<XcmBlobHauler>(sp_std::marker::PhantomData<XcmBlobHauler>);
impl<H: XcmBlobHauler> HaulBlob for XcmBlobHaulerAdapter<H> {
	fn haul_blob(blob: sp_std::prelude::Vec<u8>) -> Result<(), HaulBlobError> {
		let lane = H::xcm_lane();
		H::MessageSender::send_message(lane, blob)
			.map(|artifacts| (lane, artifacts.nonce).using_encoded(sp_io::hashing::blake2_256))
			.map(|result| {
				log::info!(
					target: crate::LOG_TARGET_BRIDGE_DISPATCH,
					"haul_blob result - ok: {:?} on lane: {:?}",
					result,
					lane
				)
			})
			.map_err(|error| {
				log::error!(
					target: crate::LOG_TARGET_BRIDGE_DISPATCH,
					"haul_blob result - error: {:?} on lane: {:?}",
					error,
					lane
				);
				HaulBlobError::Transport("MessageSenderError")
			})
	}
}
