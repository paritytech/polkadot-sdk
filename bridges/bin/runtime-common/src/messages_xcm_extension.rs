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
//! [`XcmExecutor`] -> [`MessageSender`] -> <outbound message queue>
//!                                             |
//!                                          <relayer>
//!                                             |
//! [`XcmRouter`] <- [`MessageDispatch`] <- <inbound message queue>

use bp_messages::{
	source_chain::MessagesBridge,
	target_chain::{DispatchMessage, MessageDispatch},
	LaneId,
};
use bp_runtime::{messages::MessageDispatchResult, AccountIdOf, Chain};
use codec::{Decode, Encode};
use frame_support::{dispatch::Weight, traits::Get, CloneNoBound, EqNoBound, PartialEqNoBound};
use scale_info::TypeInfo;
use xcm_builder::{DispatchBlob, DispatchBlobError, HaulBlob, HaulBlobError};

/// Plain "XCM" payload, which we transfer through bridge
pub type XcmAsPlainPayload = sp_std::prelude::Vec<u8>;

/// Message dispatch result type for single message
#[derive(CloneNoBound, EqNoBound, PartialEqNoBound, Encode, Decode, Debug, TypeInfo)]
pub enum XcmBlobMessageDispatchResult {
	InvalidPayload,
	Dispatched,
	NotDispatched(#[codec(skip)] &'static str),
}

/// [`XcmBlobMessageDispatch`] is responsible for dispatching received messages
pub struct XcmBlobMessageDispatch<
	SourceBridgeHubChain,
	TargetBridgeHubChain,
	DispatchBlob,
	DispatchBlobWeigher,
> {
	_marker: sp_std::marker::PhantomData<(
		SourceBridgeHubChain,
		TargetBridgeHubChain,
		DispatchBlob,
		DispatchBlobWeigher,
	)>,
}

impl<
		SourceBridgeHubChain: Chain,
		TargetBridgeHubChain: Chain,
		BlobDispatcher: DispatchBlob,
		DispatchBlobWeigher: Get<Weight>,
	> MessageDispatch<AccountIdOf<SourceBridgeHubChain>>
	for XcmBlobMessageDispatch<
		SourceBridgeHubChain,
		TargetBridgeHubChain,
		BlobDispatcher,
		DispatchBlobWeigher,
	>
{
	type DispatchPayload = XcmAsPlainPayload;
	type DispatchLevelResult = XcmBlobMessageDispatchResult;

	fn dispatch_weight(_message: &mut DispatchMessage<Self::DispatchPayload>) -> Weight {
		DispatchBlobWeigher::get()
	}

	fn dispatch(
		_relayer_account: &AccountIdOf<SourceBridgeHubChain>,
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
					// TODO:check-parameter - setup uspent_weight? https://github.com/paritytech/polkadot/issues/6629
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
				let e = match e {
					DispatchBlobError::Unbridgable => "DispatchBlobError::Unbridgable",
					DispatchBlobError::InvalidEncoding => "DispatchBlobError::InvalidEncoding",
					DispatchBlobError::UnsupportedLocationVersion =>
						"DispatchBlobError::UnsupportedLocationVersion",
					DispatchBlobError::UnsupportedXcmVersion =>
						"DispatchBlobError::UnsupportedXcmVersion",
					DispatchBlobError::RoutingError => "DispatchBlobError::RoutingError",
					DispatchBlobError::NonUniversalDestination =>
						"DispatchBlobError::NonUniversalDestination",
					DispatchBlobError::WrongGlobal => "DispatchBlobError::WrongGlobal",
				};
				log::error!(
					target: crate::LOG_TARGET_BRIDGE_DISPATCH,
					"[XcmBlobMessageDispatch] DispatchBlob::dispatch_blob failed, error: {:?} - message_nonce: {:?}",
					e, message.key.nonce
				);
				XcmBlobMessageDispatchResult::NotDispatched(e)
			},
		};
		MessageDispatchResult {
			// TODO:check-parameter - setup uspent_weight?  https://github.com/paritytech/polkadot/issues/6629
			unspent_weight: Weight::zero(),
			dispatch_level_result,
		}
	}
}

/// [`XcmBlobHauler`] is responsible for sending messages to the bridge "point-to-point link" from
/// one side, where on the other it can be dispatched by [`XcmBlobMessageDispatch`].
pub trait XcmBlobHauler {
	/// Runtime message sender adapter.
	type MessageSender: MessagesBridge<Self::MessageSenderOrigin, XcmAsPlainPayload>;

	/// Runtime message sender origin, which is used by [`MessageSender`].
	type MessageSenderOrigin;
	/// Our location within the Consensus Universe.
	fn message_sender_origin() -> Self::MessageSenderOrigin;

	/// Return message lane (as "point-to-point link") used to deliver XCM messages.
	fn xcm_lane() -> LaneId;
}

/// XCM bridge adapter which connects [`XcmBlobHauler`] with [`MessageSender`] and makes sure that
/// XCM blob is sent to the [`pallet_bridge_messages`] queue to be relayed.
pub struct XcmBlobHaulerAdapter<XcmBlobHauler>(sp_std::marker::PhantomData<XcmBlobHauler>);
impl<HaulerOrigin, H: XcmBlobHauler<MessageSenderOrigin = HaulerOrigin>> HaulBlob
	for XcmBlobHaulerAdapter<H>
{
	fn haul_blob(blob: sp_std::prelude::Vec<u8>) -> Result<(), HaulBlobError> {
		let lane = H::xcm_lane();
		let result = H::MessageSender::send_message(H::message_sender_origin(), lane, blob);
		let result = result
			.map(|artifacts| (lane, artifacts.nonce).using_encoded(sp_io::hashing::blake2_256));
		match &result {
			Ok(result) => log::info!(
				target: crate::LOG_TARGET_BRIDGE_DISPATCH,
				"haul_blob result - ok: {:?} on lane: {:?}",
				result,
				lane
			),
			Err(error) => log::error!(
				target: crate::LOG_TARGET_BRIDGE_DISPATCH,
				"haul_blob result - error: {:?} on lane: {:?}",
				error,
				lane
			),
		};
		result.map(|_| ()).map_err(|_| HaulBlobError::Transport("MessageSenderError"))
	}
}
