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

//! The code that allows to use the pallet (`pallet-xcm-bridge-hub`) as inbound
//! bridge messages dispatcher. Internally, it just forwards inbound blob to the
//! XCM-level blob dispatcher, which pushes message to some other queue (e.g.
//! to HRMP queue with the sibling target chain).

use crate::{Config, Pallet, XcmAsPlainPayload, LOG_TARGET};

use bp_messages::target_chain::{DispatchMessage, MessageDispatch};
use bp_runtime::messages::MessageDispatchResult;
use codec::{Decode, Encode};
use frame_support::{weights::Weight, CloneNoBound, EqNoBound, PartialEqNoBound};
use pallet_bridge_messages::{Config as BridgeMessagesConfig, WeightInfoExt};
use scale_info::TypeInfo;
use sp_runtime::SaturatedConversion;
use xcm_builder::{DispatchBlob, DispatchBlobError};

/// Message dispatch result type for single message.
#[derive(CloneNoBound, EqNoBound, PartialEqNoBound, Encode, Decode, Debug, TypeInfo)]
pub enum XcmBlobMessageDispatchResult {
	/// We've been unable to decode message payload.
	InvalidPayload,
	/// Message has been dispatched.
	Dispatched,
	/// Message has **NOT** been dispatched because of given error.
	NotDispatched(#[codec(skip)] Option<DispatchBlobError>),
}

/// An easy way to access associated messages pallet weights.
type MessagesPalletWeights<T, I> =
	<T as BridgeMessagesConfig<<T as Config<I>>::BridgeMessagesPalletInstance>>::WeightInfo;

impl<T: Config<I>, I: 'static> MessageDispatch for Pallet<T, I>
where
	T: BridgeMessagesConfig<InboundPayload = XcmAsPlainPayload>,
{
	type DispatchPayload = XcmAsPlainPayload;
	type DispatchLevelResult = XcmBlobMessageDispatchResult;

	fn is_active() -> bool {
		true
	}

	fn dispatch_weight(message: &mut DispatchMessage<Self::DispatchPayload>) -> Weight {
		match message.data.payload {
			Ok(ref payload) => {
				let payload_size = payload.encoded_size().saturated_into();
				MessagesPalletWeights::<T, I>::message_dispatch_weight(payload_size)
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
					target: LOG_TARGET,
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
		let dispatch_level_result = match T::BlobDispatcher::dispatch_blob(payload) {
			Ok(_) => {
				log::debug!(
					target: LOG_TARGET,
					"[XcmBlobMessageDispatch] DispatchBlob::dispatch_blob was ok - message_nonce: {:?}",
					message.key.nonce
				);
				XcmBlobMessageDispatchResult::Dispatched
			},
			Err(e) => {
				log::error!(
					target: LOG_TARGET,
					"[XcmBlobMessageDispatch] DispatchBlob::dispatch_blob failed, error: {:?} - message_nonce: {:?}",
					e, message.key.nonce
				);
				XcmBlobMessageDispatchResult::NotDispatched(Some(e))
			},
		};
		MessageDispatchResult { unspent_weight: Weight::zero(), dispatch_level_result }
	}
}
