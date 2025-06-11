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
//!
//! This code is executed at the target bridge hub.

use crate::{Config, Pallet, LOG_TARGET};

use bp_messages::target_chain::{DispatchMessage, MessageDispatch};
use bp_runtime::messages::MessageDispatchResult;
use bp_xcm_bridge_hub::{LocalXcmChannelManager, XcmAsPlainPayload};
use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::{weights::Weight, CloneNoBound, EqNoBound, PartialEqNoBound};
use pallet_bridge_messages::{Config as BridgeMessagesConfig, WeightInfoExt};
use scale_info::TypeInfo;
use sp_runtime::SaturatedConversion;
use xcm::prelude::*;
use xcm_builder::{DispatchBlob, DispatchBlobError};

/// Message dispatch result type for single message.
#[derive(
	CloneNoBound,
	EqNoBound,
	PartialEqNoBound,
	Encode,
	Decode,
	DecodeWithMemTracking,
	Debug,
	TypeInfo,
)]
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
	T: BridgeMessagesConfig<T::BridgeMessagesPalletInstance, InboundPayload = XcmAsPlainPayload>,
{
	type DispatchPayload = XcmAsPlainPayload;
	type DispatchLevelResult = XcmBlobMessageDispatchResult;
	type LaneId = T::LaneId;

	fn is_active(lane: Self::LaneId) -> bool {
		Pallet::<T, I>::bridge_by_lane_id(&lane)
			.and_then(|(_, bridge)| (*bridge.bridge_origin_relative_location).try_into().ok())
			.map(|recipient: Location| !T::LocalXcmChannelManager::is_congested(&recipient))
			.unwrap_or(false)
	}

	fn dispatch_weight(
		message: &mut DispatchMessage<Self::DispatchPayload, Self::LaneId>,
	) -> Weight {
		match message.data.payload {
			Ok(ref payload) => {
				let payload_size = payload.encoded_size().saturated_into();
				MessagesPalletWeights::<T, I>::message_dispatch_weight(payload_size)
			},
			Err(_) => Weight::zero(),
		}
	}

	fn dispatch(
		message: DispatchMessage<Self::DispatchPayload, Self::LaneId>,
	) -> MessageDispatchResult<Self::DispatchLevelResult> {
		let payload = match message.data.payload {
			Ok(payload) => payload,
			Err(e) => {
				log::error!(
					target: LOG_TARGET,
					"dispatch - payload error: {e:?} for lane_id: {:?} and message_nonce: {:?}",
					message.key.lane_id,
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
					"dispatch - `DispatchBlob::dispatch_blob` was ok for lane_id: {:?} and message_nonce: {:?}",
					message.key.lane_id,
					message.key.nonce
				);
				XcmBlobMessageDispatchResult::Dispatched
			},
			Err(e) => {
				log::error!(
					target: LOG_TARGET,
					"dispatch - `DispatchBlob::dispatch_blob` failed with error: {e:?} for lane_id: {:?} and message_nonce: {:?}",
					message.key.lane_id,
					message.key.nonce
				);
				XcmBlobMessageDispatchResult::NotDispatched(Some(e))
			},
		};
		MessageDispatchResult { unspent_weight: Weight::zero(), dispatch_level_result }
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{mock::*, Bridges, LaneToBridge, LanesManagerOf};

	use bp_messages::{target_chain::DispatchMessageData, LaneIdType, MessageKey};
	use bp_xcm_bridge_hub::{Bridge, BridgeLocations, BridgeState};
	use frame_support::assert_ok;
	use pallet_bridge_messages::InboundLaneStorage;
	use xcm_executor::traits::ConvertLocation;

	fn bridge() -> (Box<BridgeLocations>, TestLaneIdType) {
		let origin = OpenBridgeOrigin::sibling_parachain_origin();
		let with = bridged_asset_hub_universal_location();
		let locations =
			XcmOverBridge::bridge_locations_from_origin(origin, Box::new(with.into())).unwrap();
		let lane_id = locations.calculate_lane_id(xcm::latest::VERSION).unwrap();
		(locations, lane_id)
	}

	fn run_test_with_opened_bridge(test: impl FnOnce()) {
		run_test(|| {
			let (bridge, lane_id) = bridge();

			if !Bridges::<TestRuntime, ()>::contains_key(bridge.bridge_id()) {
				// insert bridge
				Bridges::<TestRuntime, ()>::insert(
					bridge.bridge_id(),
					Bridge {
						bridge_origin_relative_location: Box::new(
							bridge.bridge_origin_relative_location().clone().into(),
						),
						bridge_origin_universal_location: Box::new(
							bridge.bridge_origin_universal_location().clone().into(),
						),
						bridge_destination_universal_location: Box::new(
							bridge.bridge_destination_universal_location().clone().into(),
						),
						state: BridgeState::Opened,
						bridge_owner_account: LocationToAccountId::convert_location(
							bridge.bridge_origin_relative_location(),
						)
						.expect("valid accountId"),
						deposit: 0,
						lane_id,
					},
				);
				LaneToBridge::<TestRuntime, ()>::insert(lane_id, bridge.bridge_id());

				// create lanes
				let lanes_manager = LanesManagerOf::<TestRuntime, ()>::new();
				if lanes_manager.create_inbound_lane(lane_id).is_ok() {
					assert_eq!(
						0,
						lanes_manager
							.active_inbound_lane(lane_id)
							.unwrap()
							.storage()
							.data()
							.last_confirmed_nonce
					);
				}
				if lanes_manager.create_outbound_lane(lane_id).is_ok() {
					assert!(lanes_manager
						.active_outbound_lane(lane_id)
						.unwrap()
						.queued_messages()
						.is_empty());
				}
			}
			assert_ok!(XcmOverBridge::do_try_state());

			test();
		});
	}

	fn invalid_message() -> DispatchMessage<Vec<u8>, TestLaneIdType> {
		DispatchMessage {
			key: MessageKey { lane_id: TestLaneIdType::try_new(1, 2).unwrap(), nonce: 1 },
			data: DispatchMessageData { payload: Err(codec::Error::from("test")) },
		}
	}

	fn valid_message() -> DispatchMessage<Vec<u8>, TestLaneIdType> {
		DispatchMessage {
			key: MessageKey { lane_id: TestLaneIdType::try_new(1, 2).unwrap(), nonce: 1 },
			data: DispatchMessageData { payload: Ok(vec![42]) },
		}
	}

	#[test]
	fn dispatcher_is_inactive_when_channel_with_target_chain_is_congested() {
		run_test_with_opened_bridge(|| {
			TestLocalXcmChannelManager::make_congested();
			assert!(!XcmOverBridge::is_active(bridge().1));
		});
	}

	#[test]
	fn dispatcher_is_active_when_channel_with_target_chain_is_not_congested() {
		run_test_with_opened_bridge(|| {
			assert!(XcmOverBridge::is_active(bridge().1));
		});
	}

	#[test]
	fn dispatch_weight_is_zero_if_we_have_failed_to_decode_message() {
		run_test(|| {
			assert_eq!(XcmOverBridge::dispatch_weight(&mut invalid_message()), Weight::zero());
		});
	}

	#[test]
	fn dispatch_weight_is_non_zero_if_we_have_decoded_message() {
		run_test(|| {
			assert_ne!(XcmOverBridge::dispatch_weight(&mut valid_message()), Weight::zero());
		});
	}

	#[test]
	fn message_is_not_dispatched_when_we_have_failed_to_decode_message() {
		run_test(|| {
			assert_eq!(
				XcmOverBridge::dispatch(invalid_message()),
				MessageDispatchResult {
					unspent_weight: Weight::zero(),
					dispatch_level_result: XcmBlobMessageDispatchResult::InvalidPayload,
				},
			);
			assert!(!TestBlobDispatcher::is_dispatched());
		});
	}

	#[test]
	fn message_is_dispatched_when_we_have_decoded_message() {
		run_test(|| {
			assert_eq!(
				XcmOverBridge::dispatch(valid_message()),
				MessageDispatchResult {
					unspent_weight: Weight::zero(),
					dispatch_level_result: XcmBlobMessageDispatchResult::Dispatched,
				},
			);
			assert!(TestBlobDispatcher::is_dispatched());
		});
	}
}
