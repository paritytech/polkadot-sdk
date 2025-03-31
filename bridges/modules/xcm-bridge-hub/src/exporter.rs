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

//! The code that allows to use the pallet (`pallet-xcm-bridge-hub`) as XCM message
//! exporter at the sending bridge hub. Internally, it just enqueues outbound blob
//! in the messages pallet queue.
//!
//! This code is executed at the source bridge hub.

use crate::{Config, Pallet, LOG_TARGET};

use crate::{BridgeOf, Bridges};

use bp_messages::{
	source_chain::{MessagesBridge, OnMessagesDelivered},
	MessageNonce,
};
use bp_xcm_bridge_hub::{BridgeId, BridgeState, LocalXcmChannelManager, XcmAsPlainPayload};
use frame_support::{ensure, traits::Get};
use pallet_bridge_messages::{
	Config as BridgeMessagesConfig, Error, Pallet as BridgeMessagesPallet,
};
use xcm::prelude::*;
use xcm_builder::{HaulBlob, HaulBlobError, HaulBlobExporter};
use xcm_executor::traits::ExportXcm;

/// Maximal number of messages in the outbound bridge queue. Once we reach this limit, we
/// suspend a bridge.
const OUTBOUND_LANE_CONGESTED_THRESHOLD: MessageNonce = 8_192;

/// After we have suspended the bridge, we wait until number of messages in the outbound bridge
/// queue drops to this count, before sending resuming the bridge.
const OUTBOUND_LANE_UNCONGESTED_THRESHOLD: MessageNonce = 1_024;

/// An easy way to access `HaulBlobExporter`.
pub type PalletAsHaulBlobExporter<T, I> = HaulBlobExporter<
	DummyHaulBlob,
	<T as Config<I>>::BridgedNetwork,
	<T as Config<I>>::DestinationVersion,
	<T as Config<I>>::MessageExportPrice,
>;
/// An easy way to access associated messages pallet.
type MessagesPallet<T, I> = BridgeMessagesPallet<T, <T as Config<I>>::BridgeMessagesPalletInstance>;

impl<T: Config<I>, I: 'static> ExportXcm for Pallet<T, I>
where
	T: BridgeMessagesConfig<T::BridgeMessagesPalletInstance, OutboundPayload = XcmAsPlainPayload>,
{
	type Ticket = (
		BridgeId,
		BridgeOf<T, I>,
		<MessagesPallet<T, I> as MessagesBridge<T::OutboundPayload, T::LaneId>>::SendMessageArgs,
		XcmHash,
	);

	fn validate(
		network: NetworkId,
		channel: u32,
		universal_source: &mut Option<InteriorLocation>,
		destination: &mut Option<InteriorLocation>,
		message: &mut Option<Xcm<()>>,
	) -> Result<(Self::Ticket, Assets), SendError> {
		log::trace!(
			target: LOG_TARGET,
			"Validate for network: {network:?}, channel: {channel:?}, universal_source: {universal_source:?}, destination: {destination:?}"
		);

		// `HaulBlobExporter` may consume the `universal_source` and `destination` arguments, so
		// let's save them before
		let bridge_origin_universal_location =
			universal_source.clone().ok_or(SendError::MissingArgument)?;
		// Note: watch out this is `ExportMessage::destination`, which is relative to the `network`,
		// which means it does not contain `GlobalConsensus`, We need to find `BridgeId` with
		// `Self::bridge_locations` which requires **universal** location for destination.
		let bridge_destination_universal_location = {
			let dest = destination.clone().ok_or(SendError::MissingArgument)?;
			match dest.global_consensus() {
				Ok(dest_network) => {
					log::trace!(
						target: LOG_TARGET,
						"Destination: {dest:?} is already universal, checking dest_network: {dest_network:?} and network: {network:?} if matches: {:?}",
						dest_network == network
					);
					ensure!(dest_network == network, SendError::NotApplicable);
					// ok, `dest` looks like a universal location, so let's use it
					dest
				},
				Err(_) => {
					// `dest` is not a universal location, so we need to prepend it with
					// `GlobalConsensus`.
					dest.pushed_front_with(GlobalConsensus(network)).map_err(|error_data| {
						log::error!(
							target: LOG_TARGET,
							"Destination: {:?} is not a universal and prepending with {:?} failed!",
							error_data.0,
							error_data.1,
						);
						SendError::NotApplicable
					})?
				},
			}
		};

		// prepare the origin relative location
		let bridge_origin_relative_location =
			bridge_origin_universal_location.relative_to(&T::UniversalLocation::get());

		// then we are able to compute the `BridgeId` and find `LaneId` used to send messages
		let locations = Self::bridge_locations(
			bridge_origin_relative_location,
			bridge_destination_universal_location.into(),
		)
		.map_err(|e| {
			log::error!(
				target: LOG_TARGET,
				"Validate `bridge_locations` with error: {e:?}",
			);
			SendError::NotApplicable
		})?;
		let bridge = Self::bridge(locations.bridge_id()).ok_or_else(|| {
			log::error!(
				target: LOG_TARGET,
				"No opened bridge for requested bridge_origin_relative_location: {:?} and bridge_destination_universal_location: {:?}",
				locations.bridge_origin_relative_location(),
				locations.bridge_destination_universal_location(),
			);
			SendError::NotApplicable
		})?;

		// check if we are able to route the message. We use existing `HaulBlobExporter` for that.
		// It will make all required changes and will encode message properly, so that the
		// `DispatchBlob` at the bridged bridge hub will be able to decode it
		let ((blob, id), price) = PalletAsHaulBlobExporter::<T, I>::validate(
			network,
			channel,
			universal_source,
			destination,
			message,
		)?;

		let bridge_message = MessagesPallet::<T, I>::validate_message(bridge.lane_id, &blob)
			.map_err(|e| {
				match e {
					Error::LanesManager(ref ei) =>
						log::error!(target: LOG_TARGET, "LanesManager: {ei:?}"),
					Error::MessageRejectedByPallet(ref ei) =>
						log::error!(target: LOG_TARGET, "MessageRejectedByPallet: {ei:?}"),
					Error::ReceptionConfirmation(ref ei) =>
						log::error!(target: LOG_TARGET, "ReceptionConfirmation: {ei:?}"),
					_ => (),
				};

				log::error!(
					target: LOG_TARGET,
					"XCM message {:?} cannot be exported because of bridge error: {:?} on bridge {:?} and laneId: {:?}",
					id,
					e,
					locations,
					bridge.lane_id,
				);
				SendError::Transport("BridgeValidateError")
			})?;

		Ok(((*locations.bridge_id(), bridge, bridge_message, id), price))
	}

	fn deliver(
		(bridge_id, bridge, bridge_message, id): Self::Ticket,
	) -> Result<XcmHash, SendError> {
		let artifacts = MessagesPallet::<T, I>::send_message(bridge_message);

		log::info!(
			target: LOG_TARGET,
			"XCM message {:?} has been enqueued at bridge {:?} and lane_id: {:?} with nonce {}",
			id,
			bridge_id,
			bridge.lane_id,
			artifacts.nonce,
		);

		// maybe we need switch to congested state
		Self::on_bridge_message_enqueued(bridge_id, bridge, artifacts.enqueued_messages);

		Ok(id)
	}
}

impl<T: Config<I>, I: 'static> OnMessagesDelivered<T::LaneId> for Pallet<T, I> {
	fn on_messages_delivered(lane_id: T::LaneId, enqueued_messages: MessageNonce) {
		Self::on_bridge_messages_delivered(lane_id, enqueued_messages);
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Called when new message is pushed onto outbound bridge queue.
	fn on_bridge_message_enqueued(
		bridge_id: BridgeId,
		bridge: BridgeOf<T, I>,
		enqueued_messages: MessageNonce,
	) {
		// if the bridge queue is not congested, we don't want to do anything
		let is_congested = enqueued_messages > OUTBOUND_LANE_CONGESTED_THRESHOLD;
		if !is_congested {
			return
		}

		// TODO: https://github.com/paritytech/parity-bridges-common/issues/2006 we either need fishermens
		// to watch this rule violation (suspended, but keep sending new messages), or we need a
		// hard limit for that like other XCM queues have

		// check if the lane is already suspended. If it is, do nothing. We still accept new
		// messages to the suspended bridge, hoping that it'll be actually resumed soon
		if bridge.state == BridgeState::Suspended {
			return
		}

		// else - suspend the bridge
		let bridge_origin_relative_location = match bridge.bridge_origin_relative_location.try_as()
		{
			Ok(bridge_origin_relative_location) => bridge_origin_relative_location,
			Err(_) => {
				log::debug!(
					target: LOG_TARGET,
					"Failed to convert the bridge {:?} origin location {:?}",
					bridge_id,
					bridge.bridge_origin_relative_location,
				);

				return
			},
		};
		let suspend_result =
			T::LocalXcmChannelManager::suspend_bridge(bridge_origin_relative_location, bridge_id);
		match suspend_result {
			Ok(_) => {
				log::debug!(
					target: LOG_TARGET,
					"Suspended the bridge {:?}, originated by the {:?}",
					bridge_id,
					bridge.bridge_origin_relative_location,
				);
			},
			Err(e) => {
				log::debug!(
					target: LOG_TARGET,
					"Failed to suspended the bridge {:?}, originated by the {:?}: {:?}",
					bridge_id,
					bridge.bridge_origin_relative_location,
					e,
				);

				return
			},
		}

		// and remember that we have suspended the bridge
		Bridges::<T, I>::mutate_extant(bridge_id, |bridge| {
			bridge.state = BridgeState::Suspended;
		});
	}

	/// Must be called whenever we receive a message delivery confirmation.
	fn on_bridge_messages_delivered(lane_id: T::LaneId, enqueued_messages: MessageNonce) {
		// if the bridge queue is still congested, we don't want to do anything
		let is_congested = enqueued_messages > OUTBOUND_LANE_UNCONGESTED_THRESHOLD;
		if is_congested {
			return
		}

		// if we have not suspended the bridge before (or it is closed), we don't want to do
		// anything
		let (bridge_id, bridge) = match Self::bridge_by_lane_id(&lane_id) {
			Some(bridge) if bridge.1.state == BridgeState::Suspended => bridge,
			_ => {
				// if there is no bridge or it has been closed, then we don't need to send resume
				// signal to the local origin - it has closed bridge itself, so it should have
				// alrady pruned everything else
				return
			},
		};

		// else - resume the bridge
		let bridge_origin_relative_location = (*bridge.bridge_origin_relative_location).try_into();
		let bridge_origin_relative_location = match bridge_origin_relative_location {
			Ok(bridge_origin_relative_location) => bridge_origin_relative_location,
			Err(e) => {
				log::debug!(
					target: LOG_TARGET,
					"Failed to convert the bridge {:?} location for lane_id: {:?}, error {:?}",
					bridge_id,
					lane_id,
					e,
				);

				return
			},
		};

		let resume_result =
			T::LocalXcmChannelManager::resume_bridge(&bridge_origin_relative_location, bridge_id);
		match resume_result {
			Ok(_) => {
				log::debug!(
					target: LOG_TARGET,
					"Resumed the bridge {:?} and lane_id: {:?}, originated by the {:?}",
					bridge_id,
					lane_id,
					bridge_origin_relative_location,
				);
			},
			Err(e) => {
				log::debug!(
					target: LOG_TARGET,
					"Failed to resume the bridge {:?} and lane_id: {:?}, originated by the {:?}: {:?}",
					bridge_id,
					lane_id,
					bridge_origin_relative_location,
					e,
				);

				return
			},
		}

		// and forget that we have previously suspended the bridge
		Bridges::<T, I>::mutate_extant(bridge_id, |bridge| {
			bridge.state = BridgeState::Opened;
		});
	}
}

/// Dummy implementation of the `HaulBlob` trait that is never called.
///
/// We are using `HaulBlobExporter`, which requires `HaulBlob` implementation. It assumes that
/// there's a single channel between two bridge hubs - `HaulBlob` only accepts the blob and nothing
/// else. But bridge messages pallet may have a dedicated channel (lane) for every pair of bridged
/// chains. So we are using our own `ExportXcm` implementation, but to utilize `HaulBlobExporter` we
/// still need this `DummyHaulBlob`.
pub struct DummyHaulBlob;

impl HaulBlob for DummyHaulBlob {
	fn haul_blob(_blob: XcmAsPlainPayload) -> Result<(), HaulBlobError> {
		Err(HaulBlobError::Transport("DummyHaulBlob"))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{mock::*, Bridges, LaneToBridge, LanesManagerOf};

	use bp_runtime::RangeInclusiveExt;
	use bp_xcm_bridge_hub::{Bridge, BridgeLocations, BridgeState};
	use frame_support::{assert_ok, traits::EnsureOrigin};
	use pallet_bridge_messages::InboundLaneStorage;
	use xcm_builder::{NetworkExportTable, UnpaidRemoteExporter};
	use xcm_executor::traits::{export_xcm, ConvertLocation};

	fn universal_source() -> InteriorLocation {
		SiblingUniversalLocation::get()
	}

	fn bridged_relative_destination() -> InteriorLocation {
		BridgedRelativeDestination::get()
	}

	fn bridged_universal_destination() -> InteriorLocation {
		BridgedUniversalDestination::get()
	}

	fn open_lane(origin: RuntimeOrigin) -> (BridgeLocations, TestLaneIdType) {
		// open expected outbound lane
		let with = bridged_asset_hub_universal_location();
		let locations =
			XcmOverBridge::bridge_locations_from_origin(origin, Box::new(with.into())).unwrap();
		let lane_id = locations.calculate_lane_id(xcm::latest::VERSION).unwrap();

		if !Bridges::<TestRuntime, ()>::contains_key(locations.bridge_id()) {
			// insert bridge
			Bridges::<TestRuntime, ()>::insert(
				locations.bridge_id(),
				Bridge {
					bridge_origin_relative_location: Box::new(SiblingLocation::get().into()),
					bridge_origin_universal_location: Box::new(
						locations.bridge_origin_universal_location().clone().into(),
					),
					bridge_destination_universal_location: Box::new(
						locations.bridge_destination_universal_location().clone().into(),
					),
					state: BridgeState::Opened,
					bridge_owner_account: LocationToAccountId::convert_location(
						locations.bridge_origin_relative_location(),
					)
					.expect("valid accountId"),
					deposit: 0,
					lane_id,
				},
			);
			LaneToBridge::<TestRuntime, ()>::insert(lane_id, locations.bridge_id());

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

		(*locations, lane_id)
	}

	fn open_lane_and_send_regular_message() -> (BridgeId, TestLaneIdType) {
		let (locations, lane_id) = open_lane(OpenBridgeOrigin::sibling_parachain_origin());

		// now let's try to enqueue message using our `ExportXcm` implementation
		export_xcm::<XcmOverBridge>(
			BridgedRelayNetwork::get(),
			0,
			locations.bridge_origin_universal_location().clone(),
			locations.bridge_destination_universal_location().clone(),
			vec![Instruction::ClearOrigin].into(),
		)
		.unwrap();

		(*locations.bridge_id(), lane_id)
	}

	#[test]
	fn exporter_works() {
		run_test(|| {
			let (_, lane_id) = open_lane_and_send_regular_message();

			// double check that the message has been pushed to the expected lane
			// (it should already been checked during `send_message` call)
			assert!(!LanesManagerOf::<TestRuntime, ()>::new()
				.active_outbound_lane(lane_id)
				.unwrap()
				.queued_messages()
				.is_empty());
		});
	}

	#[test]
	fn exporter_does_not_suspend_the_bridge_if_outbound_bridge_queue_is_not_congested() {
		run_test(|| {
			let (bridge_id, _) = open_lane_and_send_regular_message();
			assert!(!TestLocalXcmChannelManager::is_bridge_suspended(&bridge_id));
			assert_eq!(XcmOverBridge::bridge(&bridge_id).unwrap().state, BridgeState::Opened);
		});
	}

	#[test]
	fn exporter_does_not_suspend_the_bridge_if_it_is_already_suspended() {
		run_test(|| {
			let (bridge_id, _) = open_lane_and_send_regular_message();
			Bridges::<TestRuntime, ()>::mutate_extant(bridge_id, |bridge| {
				bridge.state = BridgeState::Suspended;
			});
			for _ in 1..OUTBOUND_LANE_CONGESTED_THRESHOLD {
				open_lane_and_send_regular_message();
			}

			open_lane_and_send_regular_message();
			assert!(!TestLocalXcmChannelManager::is_bridge_suspended(&bridge_id));
		});
	}

	#[test]
	fn exporter_suspends_the_bridge_if_outbound_bridge_queue_is_congested() {
		run_test(|| {
			let (bridge_id, _) = open_lane_and_send_regular_message();
			for _ in 1..OUTBOUND_LANE_CONGESTED_THRESHOLD {
				open_lane_and_send_regular_message();
			}

			assert!(!TestLocalXcmChannelManager::is_bridge_suspended(&bridge_id));
			assert_eq!(XcmOverBridge::bridge(&bridge_id).unwrap().state, BridgeState::Opened);

			open_lane_and_send_regular_message();
			assert!(TestLocalXcmChannelManager::is_bridge_suspended(&bridge_id));
			assert_eq!(XcmOverBridge::bridge(&bridge_id).unwrap().state, BridgeState::Suspended);
		});
	}

	#[test]
	fn bridge_is_not_resumed_if_outbound_bridge_queue_is_still_congested() {
		run_test(|| {
			let (bridge_id, lane_id) = open_lane_and_send_regular_message();
			Bridges::<TestRuntime, ()>::mutate_extant(bridge_id, |bridge| {
				bridge.state = BridgeState::Suspended;
			});
			XcmOverBridge::on_bridge_messages_delivered(
				lane_id,
				OUTBOUND_LANE_UNCONGESTED_THRESHOLD + 1,
			);

			assert!(!TestLocalXcmChannelManager::is_bridge_resumed(&bridge_id));
			assert_eq!(XcmOverBridge::bridge(&bridge_id).unwrap().state, BridgeState::Suspended);
		});
	}

	#[test]
	fn bridge_is_not_resumed_if_it_was_not_suspended_before() {
		run_test(|| {
			let (bridge_id, lane_id) = open_lane_and_send_regular_message();
			XcmOverBridge::on_bridge_messages_delivered(
				lane_id,
				OUTBOUND_LANE_UNCONGESTED_THRESHOLD,
			);

			assert!(!TestLocalXcmChannelManager::is_bridge_resumed(&bridge_id));
			assert_eq!(XcmOverBridge::bridge(&bridge_id).unwrap().state, BridgeState::Opened);
		});
	}

	#[test]
	fn bridge_is_resumed_when_enough_messages_are_delivered() {
		run_test(|| {
			let (bridge_id, lane_id) = open_lane_and_send_regular_message();
			Bridges::<TestRuntime, ()>::mutate_extant(bridge_id, |bridge| {
				bridge.state = BridgeState::Suspended;
			});
			XcmOverBridge::on_bridge_messages_delivered(
				lane_id,
				OUTBOUND_LANE_UNCONGESTED_THRESHOLD,
			);

			assert!(TestLocalXcmChannelManager::is_bridge_resumed(&bridge_id));
			assert_eq!(XcmOverBridge::bridge(&bridge_id).unwrap().state, BridgeState::Opened);
		});
	}

	#[test]
	fn export_fails_if_argument_is_missing() {
		run_test(|| {
			assert_eq!(
				XcmOverBridge::validate(
					BridgedRelayNetwork::get(),
					0,
					&mut None,
					&mut Some(bridged_relative_destination()),
					&mut Some(Vec::new().into()),
				),
				Err(SendError::MissingArgument),
			);

			assert_eq!(
				XcmOverBridge::validate(
					BridgedRelayNetwork::get(),
					0,
					&mut Some(universal_source()),
					&mut None,
					&mut Some(Vec::new().into()),
				),
				Err(SendError::MissingArgument),
			);
		})
	}

	#[test]
	fn exporter_computes_correct_lane_id() {
		run_test(|| {
			assert_ne!(bridged_universal_destination(), bridged_relative_destination());

			let locations = BridgeLocations::bridge_locations(
				UniversalLocation::get(),
				SiblingLocation::get(),
				bridged_universal_destination(),
				BridgedRelayNetwork::get(),
			)
			.unwrap();
			let expected_bridge_id = locations.bridge_id();
			let expected_lane_id = locations.calculate_lane_id(xcm::latest::VERSION).unwrap();

			if LanesManagerOf::<TestRuntime, ()>::new()
				.create_outbound_lane(expected_lane_id)
				.is_ok()
			{
				Bridges::<TestRuntime, ()>::insert(
					expected_bridge_id,
					Bridge {
						bridge_origin_relative_location: Box::new(
							locations.bridge_origin_relative_location().clone().into(),
						),
						bridge_origin_universal_location: Box::new(
							locations.bridge_origin_universal_location().clone().into(),
						),
						bridge_destination_universal_location: Box::new(
							locations.bridge_destination_universal_location().clone().into(),
						),
						state: BridgeState::Opened,
						bridge_owner_account: [0u8; 32].into(),
						deposit: 0,
						lane_id: expected_lane_id,
					},
				);
			}

			let ticket = XcmOverBridge::validate(
				BridgedRelayNetwork::get(),
				0,
				&mut Some(universal_source()),
				// Note:  The `ExportMessage` expects relative `InteriorLocation` in the
				// `BridgedRelayNetwork`.
				&mut Some(bridged_relative_destination()),
				&mut Some(Vec::new().into()),
			)
			.unwrap()
			.0;
			assert_eq!(&ticket.0, expected_bridge_id);
			assert_eq!(ticket.1.lane_id, expected_lane_id);
		});
	}

	#[test]
	fn exporter_is_compatible_with_pallet_xcm_bridge_hub_router() {
		run_test(|| {
			// valid routable destination
			let dest = Location::new(2, BridgedUniversalDestination::get());

			// open bridge
			let origin = OpenBridgeOrigin::sibling_parachain_origin();
			let origin_as_location =
				OpenBridgeOriginOf::<TestRuntime, ()>::try_origin(origin.clone()).unwrap();
			let (_, expected_lane_id) = open_lane(origin);

			// check before - no messages
			assert_eq!(
				pallet_bridge_messages::Pallet::<TestRuntime, ()>::outbound_lane_data(
					expected_lane_id
				)
				.unwrap()
				.queued_messages()
				.saturating_len(),
				0
			);

			// send `ExportMessage(message)` by `UnpaidRemoteExporter`.
			ExecuteXcmOverSendXcm::set_origin_for_execute(origin_as_location);
			assert_ok!(send_xcm::<
				UnpaidRemoteExporter<
					NetworkExportTable<BridgeTable>,
					ExecuteXcmOverSendXcm,
					UniversalLocation,
				>,
			>(dest.clone(), Xcm::<()>::default()));

			// we need to set `UniversalLocation` for `sibling_parachain_origin` for
			// `XcmOverBridgeWrappedWithExportMessageRouterInstance`.
			ExportMessageOriginUniversalLocation::set(Some(SiblingUniversalLocation::get()));
			// send `ExportMessage(message)` by `pallet_xcm_bridge_hub_router`.
			ExecuteXcmOverSendXcm::set_origin_for_execute(SiblingLocation::get());
			assert_ok!(send_xcm::<XcmOverBridgeWrappedWithExportMessageRouter>(
				dest.clone(),
				Xcm::<()>::default()
			));

			// check after - a message ready to be relayed
			assert_eq!(
				pallet_bridge_messages::Pallet::<TestRuntime, ()>::outbound_lane_data(
					expected_lane_id
				)
				.unwrap()
				.queued_messages()
				.saturating_len(),
				2
			);
		})
	}

	#[test]
	fn validate_works() {
		run_test(|| {
			let xcm: Xcm<()> = vec![ClearOrigin].into();

			// check that router does not consume when `NotApplicable`
			let mut xcm_wrapper = Some(xcm.clone());
			let mut universal_source_wrapper = Some(universal_source());

			// wrong `NetworkId`
			let mut dest_wrapper = Some(bridged_relative_destination());
			assert_eq!(
				XcmOverBridge::validate(
					NetworkId::ByGenesis([0; 32]),
					0,
					&mut universal_source_wrapper,
					&mut dest_wrapper,
					&mut xcm_wrapper,
				),
				Err(SendError::NotApplicable),
			);
			// dest and xcm is NOT consumed and untouched
			assert_eq!(&Some(xcm.clone()), &xcm_wrapper);
			assert_eq!(&Some(universal_source()), &universal_source_wrapper);
			assert_eq!(&Some(bridged_relative_destination()), &dest_wrapper);

			// dest starts with wrong `NetworkId`
			let mut invalid_dest_wrapper = Some(
				[GlobalConsensus(NetworkId::ByGenesis([0; 32])), Parachain(BRIDGED_ASSET_HUB_ID)]
					.into(),
			);
			assert_eq!(
				XcmOverBridge::validate(
					BridgedRelayNetwork::get(),
					0,
					&mut Some(universal_source()),
					&mut invalid_dest_wrapper,
					&mut xcm_wrapper,
				),
				Err(SendError::NotApplicable),
			);
			// dest and xcm is NOT consumed and untouched
			assert_eq!(&Some(xcm.clone()), &xcm_wrapper);
			assert_eq!(&Some(universal_source()), &universal_source_wrapper);
			assert_eq!(
				&Some(
					[
						GlobalConsensus(NetworkId::ByGenesis([0; 32]),),
						Parachain(BRIDGED_ASSET_HUB_ID)
					]
					.into()
				),
				&invalid_dest_wrapper
			);

			// no opened lane for dest
			let mut dest_without_lane_wrapper =
				Some([GlobalConsensus(BridgedRelayNetwork::get()), Parachain(5679)].into());
			assert_eq!(
				XcmOverBridge::validate(
					BridgedRelayNetwork::get(),
					0,
					&mut Some(universal_source()),
					&mut dest_without_lane_wrapper,
					&mut xcm_wrapper,
				),
				Err(SendError::NotApplicable),
			);
			// dest and xcm is NOT consumed and untouched
			assert_eq!(&Some(xcm.clone()), &xcm_wrapper);
			assert_eq!(&Some(universal_source()), &universal_source_wrapper);
			assert_eq!(
				&Some([GlobalConsensus(BridgedRelayNetwork::get(),), Parachain(5679)].into()),
				&dest_without_lane_wrapper
			);

			// ok
			let _ = open_lane(OpenBridgeOrigin::sibling_parachain_origin());
			let mut dest_wrapper = Some(bridged_relative_destination());
			assert_ok!(XcmOverBridge::validate(
				BridgedRelayNetwork::get(),
				0,
				&mut Some(universal_source()),
				&mut dest_wrapper,
				&mut xcm_wrapper,
			));
			// dest and xcm IS consumed
			assert_eq!(None, xcm_wrapper);
			assert_eq!(&Some(universal_source()), &universal_source_wrapper);
			assert_eq!(None, dest_wrapper);
		});
	}

	#[test]
	fn congestion_with_pallet_xcm_bridge_hub_router_works() {
		run_test(|| {
			// valid routable destination
			let dest = Location::new(2, BridgedUniversalDestination::get());

			fn router_bridge_state() -> pallet_xcm_bridge_hub_router::BridgeState {
				pallet_xcm_bridge_hub_router::Bridge::<
					TestRuntime,
					XcmOverBridgeWrappedWithExportMessageRouterInstance,
				>::get()
			}

			// open two bridges
			let origin = OpenBridgeOrigin::sibling_parachain_origin();
			let origin_as_location =
				OpenBridgeOriginOf::<TestRuntime, ()>::try_origin(origin.clone()).unwrap();
			let (bridge_1, expected_lane_id_1) = open_lane(origin);

			// we need to set `UniversalLocation` for `sibling_parachain_origin` for
			// `XcmOverBridgeWrappedWithExportMessageRouterInstance`.
			ExportMessageOriginUniversalLocation::set(Some(SiblingUniversalLocation::get()));

			// check before
			// bridges are opened
			assert_eq!(
				XcmOverBridge::bridge(bridge_1.bridge_id()).unwrap().state,
				BridgeState::Opened
			);

			// the router is uncongested
			assert!(!router_bridge_state().is_congested);
			assert!(!TestLocalXcmChannelManager::is_bridge_suspended(bridge_1.bridge_id()));
			assert!(!TestLocalXcmChannelManager::is_bridge_resumed(bridge_1.bridge_id()));

			// make bridges congested with sending too much messages
			for _ in 1..(OUTBOUND_LANE_CONGESTED_THRESHOLD + 2) {
				// send `ExportMessage(message)` by `pallet_xcm_bridge_hub_router`.
				ExecuteXcmOverSendXcm::set_origin_for_execute(origin_as_location.clone());
				assert_ok!(send_xcm::<XcmOverBridgeWrappedWithExportMessageRouter>(
					dest.clone(),
					Xcm::<()>::default()
				));
			}

			// checks after
			// bridges are suspended
			assert_eq!(
				XcmOverBridge::bridge(bridge_1.bridge_id()).unwrap().state,
				BridgeState::Suspended,
			);
			// the router is congested
			assert!(router_bridge_state().is_congested);
			assert!(TestLocalXcmChannelManager::is_bridge_suspended(bridge_1.bridge_id()));
			assert!(!TestLocalXcmChannelManager::is_bridge_resumed(bridge_1.bridge_id()));

			// make bridges uncongested to trigger resume signal
			XcmOverBridge::on_bridge_messages_delivered(
				expected_lane_id_1,
				OUTBOUND_LANE_UNCONGESTED_THRESHOLD,
			);

			// bridge is again opened
			assert_eq!(
				XcmOverBridge::bridge(bridge_1.bridge_id()).unwrap().state,
				BridgeState::Opened
			);
			// the router is uncongested
			assert!(!router_bridge_state().is_congested);
			assert!(TestLocalXcmChannelManager::is_bridge_resumed(bridge_1.bridge_id()));
		})
	}
}
