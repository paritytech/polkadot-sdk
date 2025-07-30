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

//! The code that allows to use the pallet (`pallet-xcm-bridge`) as XCM message
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
use bp_xcm_bridge::{BridgeId, BridgeState, LocalXcmChannelManager, XcmAsPlainPayload};
use frame_support::{ensure, traits::Get};
use pallet_bridge_messages::{
	Config as BridgeMessagesConfig, Error, Pallet as BridgeMessagesPallet,
};
use polkadot_runtime_common::xcm_sender::PriceForMessageDelivery;
use xcm::prelude::*;
use xcm_builder::{HaulBlob, HaulBlobError, HaulBlobExporter};
use xcm_executor::traits::ExportXcm;

/// An easy way to access `HaulBlobExporter`.
///
/// Note: Set no price for `HaulBlobExporter`, because `ExportXcm for Pallet` handles the fees.
pub type PalletAsHaulBlobExporter<T, I> = HaulBlobExporter<
	DummyHaulBlob,
	<T as Config<I>>::BridgedNetwork,
	<T as Config<I>>::DestinationVersion,
	(),
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
				"No opened bridge for requested bridge_origin_relative_location: {:?} (bridge_origin_universal_location: {:?}) and bridge_destination_universal_location: {:?}",
				locations.bridge_origin_relative_location(),
				locations.bridge_origin_universal_location(),
				locations.bridge_destination_universal_location(),
			);
			SendError::NotApplicable
		})?;

		// Get the potential price for a message over the bridge.
		let price_for_delivery = message
			.as_ref()
			.map(|msg| T::MessageExportPrice::price_for_delivery(*locations.bridge_id(), msg));

		// check if we are able to route the message. We use the existing ` HaulBlobExporter ` for
		// that. It will make all required changes and will encode a message properly, so that the
		// `DispatchBlob` at the bridged xcm-bridge will be able to decode it.
		let ((blob, id), mut price) = PalletAsHaulBlobExporter::<T, I>::validate(
			network,
			channel,
			universal_source,
			destination,
			message,
		)?;

		// Add `price_for_delivery` to the `price`.
		if let Some(delivery_prices) = price_for_delivery {
			for dp in delivery_prices.into_inner() {
				price.push(dp);
			}
		}

		// Here, we know that the message is relevant to this pallet instance, so let's check for
		// congestion defense.
		if bridge.state == BridgeState::HardSuspended {
			log::error!(
				target: LOG_TARGET,
				"Bridge for requested bridge_origin_relative_location: {:?} (bridge_origin_universal_location: {:?}) and bridge_destination_universal_location: {:?} \
				is suspended and does not accept more messages!",
				locations.bridge_origin_relative_location(),
				locations.bridge_origin_universal_location(),
				locations.bridge_destination_universal_location(),
			);
			return Err(SendError::Transport("Exporter is suspended!"));
		}

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
		let is_congested =
			enqueued_messages > T::CongestionLimits::get().outbound_lane_congested_threshold;
		if !is_congested {
			return
		}

		// check if the lane is already suspended or not.
		match bridge.state {
			BridgeState::SoftSuspended => {
				if enqueued_messages > T::CongestionLimits::get().outbound_lane_stop_threshold {
					// If its suspended and reached `outbound_lane_stop_threshold`, we stop
					// accepting new messages (a.k.a. start dropping).
					Bridges::<T, I>::mutate_extant(bridge_id, |bridge| {
						bridge.state = BridgeState::HardSuspended;
					});
					return
				} else {
					// We still can accept new messages to the suspended bridge, hoping that it'll
					// be actually resumed soon
					return
				}
			},
			BridgeState::HardSuspended => {
				// We cannot accept new messages and start dropping messages, until the outbound
				// lane goes below the drop limit.
				return
			},
			_ => {
				// otherwise, continue handling the suspension
			},
		}

		// else - suspend the bridge
		let result_bridge_origin_relative_location =
			(*bridge.bridge_origin_relative_location).clone().try_into();
		let bridge_origin_relative_location = match &result_bridge_origin_relative_location {
			Ok(bridge_origin_relative_location) => bridge_origin_relative_location,
			Err(_) => {
				log::error!(
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
				log::error!(
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
			bridge.state = BridgeState::SoftSuspended;
		});
	}

	/// Must be called whenever we receive a message delivery confirmation.
	fn on_bridge_messages_delivered(lane_id: T::LaneId, enqueued_messages: MessageNonce) {
		// if the bridge queue is still congested, we don't want to do anything
		let is_congested =
			enqueued_messages > T::CongestionLimits::get().outbound_lane_uncongested_threshold;
		if is_congested {
			// and if it is below the `stop_threshold`
			if enqueued_messages < T::CongestionLimits::get().outbound_lane_stop_threshold {
				if let Some((bridge_id, bridge)) = Self::bridge_by_lane_id(&lane_id) {
					if let BridgeState::HardSuspended = bridge.state {
						// we allow exporting again
						Bridges::<T, I>::mutate_extant(bridge_id, |b| {
							b.state = BridgeState::SoftSuspended;
						});
					}
				}
			}
			return
		}

		// if we have not suspended the bridge before (or it is closed), we don't want to do
		// anything
		let (bridge_id, bridge) = match Self::bridge_by_lane_id(&lane_id) {
			Some(bridge)
				if bridge.1.state == BridgeState::SoftSuspended ||
					bridge.1.state == BridgeState::HardSuspended =>
				bridge,
			_ => {
				// if there is no bridge, or it has been closed, then we don't need to send resume
				// signal to the local origin - it has closed bridge itself, so it should have
				// already pruned everything else
				return
			},
		};

		// else - resume the bridge
		let bridge_origin_relative_location = (*bridge.bridge_origin_relative_location).try_into();
		let bridge_origin_relative_location = match bridge_origin_relative_location {
			Ok(bridge_origin_relative_location) => bridge_origin_relative_location,
			Err(e) => {
				log::error!(
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
				log::error!(
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
	use crate::{mock::*, Bridges, LanesManagerOf};

	use bp_runtime::RangeInclusiveExt;
	use bp_xcm_bridge::{Bridge, BridgeLocations, BridgeState, Receiver};
	use bp_xcm_bridge_router::MINIMAL_DELIVERY_FEE_FACTOR;
	use frame_support::{
		assert_err, assert_ok,
		traits::{Contains, EnsureOrigin},
	};
	use pallet_xcm_bridge_router::ResolveBridgeId;
	use xcm_builder::{NetworkExportTable, UnpaidRemoteExporter};
	use xcm_executor::traits::export_xcm;

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
			XcmOverBridge::bridge_locations_from_origin(origin.clone(), Box::new(with.into()))
				.unwrap();
		let lane_id = locations.calculate_lane_id(xcm::latest::VERSION).unwrap();

		if !Bridges::<TestRuntime, ()>::contains_key(locations.bridge_id()) {
			// fund origin (if needed)
			if !<TestRuntime as Config<()>>::AllowWithoutBridgeDeposit::contains(
				locations.bridge_origin_relative_location(),
			) {
				fund_origin_sovereign_account(
					&locations,
					BridgeDeposit::get() + ExistentialDeposit::get(),
				);
			}

			// open bridge
			assert_ok!(XcmOverBridge::do_open_bridge(locations.clone(), lane_id, true, None));
		}
		assert_ok!(XcmOverBridge::do_try_state());

		(*locations, lane_id)
	}

	fn open_lane_and_send_regular_message(
		source_origin: RuntimeOrigin,
	) -> (BridgeId, TestLaneIdType) {
		let (locations, lane_id) = open_lane(source_origin);

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
			let (_, lane_id) =
				open_lane_and_send_regular_message(OpenBridgeOrigin::sibling_parachain_origin());

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
			let (bridge_id, _) =
				open_lane_and_send_regular_message(OpenBridgeOrigin::sibling_parachain_origin());
			assert!(!TestLocalXcmChannelManager::is_bridge_suspened(&bridge_id));
			assert_eq!(XcmOverBridge::bridge(&bridge_id).unwrap().state, BridgeState::Opened);
		});
	}

	#[test]
	fn exporter_does_not_suspend_the_bridge_if_it_is_already_suspended() {
		run_test(|| {
			let (bridge_id, _) =
				open_lane_and_send_regular_message(OpenBridgeOrigin::sibling_parachain_origin());
			Bridges::<TestRuntime, ()>::mutate_extant(bridge_id, |bridge| {
				bridge.state = BridgeState::SoftSuspended;
			});
			for _ in 1..TestCongestionLimits::get().outbound_lane_congested_threshold {
				open_lane_and_send_regular_message(OpenBridgeOrigin::sibling_parachain_origin());
			}

			open_lane_and_send_regular_message(OpenBridgeOrigin::sibling_parachain_origin());
			assert!(!TestLocalXcmChannelManager::is_bridge_suspened(&bridge_id));
		});
	}

	#[test]
	fn exporter_suspends_the_bridge_if_outbound_bridge_queue_is_congested() {
		run_test(|| {
			let (bridge_id, _) =
				open_lane_and_send_regular_message(OpenBridgeOrigin::sibling_parachain_origin());
			for _ in 1..TestCongestionLimits::get().outbound_lane_congested_threshold {
				open_lane_and_send_regular_message(OpenBridgeOrigin::sibling_parachain_origin());
			}

			assert!(!TestLocalXcmChannelManager::is_bridge_suspened(&bridge_id));
			assert_eq!(XcmOverBridge::bridge(&bridge_id).unwrap().state, BridgeState::Opened);

			open_lane_and_send_regular_message(OpenBridgeOrigin::sibling_parachain_origin());
			assert!(TestLocalXcmChannelManager::is_bridge_suspened(&bridge_id));
			assert_eq!(
				XcmOverBridge::bridge(&bridge_id).unwrap().state,
				BridgeState::SoftSuspended
			);

			// send more messages to reach `outbound_lane_stop_threshold`
			for _ in TestCongestionLimits::get().outbound_lane_congested_threshold..
				TestCongestionLimits::get().outbound_lane_stop_threshold
			{
				open_lane_and_send_regular_message(OpenBridgeOrigin::sibling_parachain_origin());
			}
			assert_eq!(
				XcmOverBridge::bridge(&bridge_id).unwrap().state,
				BridgeState::HardSuspended
			);
		});
	}

	#[test]
	fn bridge_is_not_resumed_if_outbound_bridge_queue_is_still_congested() {
		run_test(|| {
			let (bridge_id, lane_id) =
				open_lane_and_send_regular_message(OpenBridgeOrigin::sibling_parachain_origin());
			Bridges::<TestRuntime, ()>::mutate_extant(bridge_id, |bridge| {
				bridge.state = BridgeState::SoftSuspended;
			});
			XcmOverBridge::on_bridge_messages_delivered(
				lane_id,
				TestCongestionLimits::get().outbound_lane_uncongested_threshold + 1,
			);

			assert!(!TestLocalXcmChannelManager::is_bridge_resumed(&bridge_id));
			assert_eq!(
				XcmOverBridge::bridge(&bridge_id).unwrap().state,
				BridgeState::SoftSuspended
			);
		});
	}

	#[test]
	fn bridge_is_not_resumed_if_it_was_not_suspended_before() {
		run_test(|| {
			let (bridge_id, lane_id) =
				open_lane_and_send_regular_message(OpenBridgeOrigin::sibling_parachain_origin());
			XcmOverBridge::on_bridge_messages_delivered(
				lane_id,
				TestCongestionLimits::get().outbound_lane_uncongested_threshold,
			);

			assert!(!TestLocalXcmChannelManager::is_bridge_resumed(&bridge_id));
			assert_eq!(XcmOverBridge::bridge(&bridge_id).unwrap().state, BridgeState::Opened);
		});
	}

	#[test]
	fn exporter_respects_stop_threshold() {
		run_test(|| {
			let (bridge_id, lane_id) =
				open_lane_and_send_regular_message(OpenBridgeOrigin::sibling_parachain_origin());
			let xcm: Xcm<()> = vec![ClearOrigin].into();

			// Opened - exporter works
			assert_eq!(XcmOverBridge::bridge(&bridge_id).unwrap().state, BridgeState::Opened);
			assert_ok!(XcmOverBridge::validate(
				BridgedRelayNetwork::get(),
				0,
				&mut Some(universal_source()),
				&mut Some(bridged_relative_destination()),
				&mut Some(xcm.clone()),
			),);

			// SoftSuspended - exporter still works
			XcmOverBridge::on_bridge_message_enqueued(
				bridge_id,
				XcmOverBridge::bridge(&bridge_id).unwrap(),
				TestCongestionLimits::get().outbound_lane_congested_threshold + 1,
			);
			assert_eq!(
				XcmOverBridge::bridge(&bridge_id).unwrap().state,
				BridgeState::SoftSuspended
			);
			assert_ok!(XcmOverBridge::validate(
				BridgedRelayNetwork::get(),
				0,
				&mut Some(universal_source()),
				&mut Some(bridged_relative_destination()),
				&mut Some(xcm.clone()),
			),);

			// HardSuspended - exporter stops working
			XcmOverBridge::on_bridge_message_enqueued(
				bridge_id,
				XcmOverBridge::bridge(&bridge_id).unwrap(),
				TestCongestionLimits::get().outbound_lane_stop_threshold + 1,
			);
			assert_eq!(
				XcmOverBridge::bridge(&bridge_id).unwrap().state,
				BridgeState::HardSuspended
			);
			assert_err!(
				XcmOverBridge::validate(
					BridgedRelayNetwork::get(),
					0,
					&mut Some(universal_source()),
					&mut Some(bridged_relative_destination()),
					&mut Some(xcm.clone()),
				),
				SendError::Transport("Exporter is suspended!"),
			);

			// Back to SoftSuspended - exporter again works
			XcmOverBridge::on_bridge_messages_delivered(
				lane_id,
				TestCongestionLimits::get().outbound_lane_stop_threshold - 1,
			);
			assert_eq!(
				XcmOverBridge::bridge(&bridge_id).unwrap().state,
				BridgeState::SoftSuspended
			);
			assert_ok!(XcmOverBridge::validate(
				BridgedRelayNetwork::get(),
				0,
				&mut Some(universal_source()),
				&mut Some(bridged_relative_destination()),
				&mut Some(xcm.clone()),
			),);

			// Back to Opened - exporter works
			XcmOverBridge::on_bridge_messages_delivered(
				lane_id,
				TestCongestionLimits::get().outbound_lane_uncongested_threshold - 1,
			);
			assert_eq!(XcmOverBridge::bridge(&bridge_id).unwrap().state, BridgeState::Opened);
			assert_ok!(XcmOverBridge::validate(
				BridgedRelayNetwork::get(),
				0,
				&mut Some(universal_source()),
				&mut Some(bridged_relative_destination()),
				&mut Some(xcm.clone()),
			),);
		});
	}

	#[test]
	fn bridge_is_resumed_when_enough_messages_are_delivered() {
		run_test(|| {
			let (bridge_id, lane_id) =
				open_lane_and_send_regular_message(OpenBridgeOrigin::sibling_parachain_origin());
			Bridges::<TestRuntime, ()>::mutate_extant(bridge_id, |bridge| {
				bridge.state = BridgeState::SoftSuspended;
			});
			XcmOverBridge::on_bridge_messages_delivered(
				lane_id,
				TestCongestionLimits::get().outbound_lane_uncongested_threshold,
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
						deposit: None,
						lane_id: expected_lane_id,
						maybe_notify: None,
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
	fn pallet_as_exporter_is_compatible_with_pallet_xcm_bridge_hub_router_for_export_message() {
		run_test(|| {
			// valid routable destination
			let dest = Location::new(2, BridgedUniversalDestination::get());

			// open bridge
			let origin = OpenBridgeOrigin::sibling_parachain_origin();
			let origin_as_location =
				OpenBridgeOriginOf::<TestRuntime, ()>::try_origin(origin.clone()).unwrap();
			let (bridge, expected_lane_id) = open_lane(origin);

			// we need to set `UniversalLocation` for `sibling_parachain_origin` for
			// `XcmOverBridgeWrappedWithExportMessageRouterInstance`.
			ExportMessageOriginUniversalLocation::set(Some(SiblingUniversalLocation::get()));

			// check compatible bridge_id
			assert_eq!(
				bridge.bridge_id(),
				&<TestRuntime as pallet_xcm_bridge_router::Config<
					XcmOverBridgeWrappedWithExportMessageRouterInstance,
				>>::BridgeIdResolver::resolve_for_dest(&dest)
				.unwrap()
			);

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
			ExecuteXcmOverSendXcm::set_origin_for_execute(origin_as_location.clone());
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
			ExecuteXcmOverSendXcm::set_origin_for_execute(origin_as_location);
			assert_ok!(send_xcm::<XcmOverBridgeWrappedWithExportMessageRouter>(
				dest,
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
	fn pallet_as_exporter_is_compatible_with_pallet_xcm_bridge_hub_router_for_export_xcm() {
		run_test(|| {
			// valid routable destination
			let dest = Location::new(2, BridgedUniversalDestination::get());

			// open bridge as a root on the local chain, which should be converted as
			// `Location::here()`
			let (bridge, expected_lane_id) = open_lane(RuntimeOrigin::root());

			// check compatible bridge_id
			assert_eq!(
				bridge.bridge_id(),
				&<TestRuntime as pallet_xcm_bridge_router::Config<
					XcmOverBridgeByExportXcmRouterInstance,
				>>::BridgeIdResolver::resolve_for_dest(&dest)
				.unwrap()
			);

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

			// trigger `ExportXcm` by `pallet_xcm_bridge_hub_router`.
			assert_ok!(send_xcm::<XcmOverBridgeByExportXcmRouter>(dest, Xcm::<()>::default()));

			// check after - a message ready to be relayed
			assert_eq!(
				pallet_bridge_messages::Pallet::<TestRuntime, ()>::outbound_lane_data(
					expected_lane_id
				)
				.unwrap()
				.queued_messages()
				.saturating_len(),
				1
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
			let (locations, _) = open_lane(OpenBridgeOrigin::sibling_parachain_origin());
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

			// send more messages to reach `outbound_lane_congested_threshold`
			for _ in 0..=TestCongestionLimits::get().outbound_lane_congested_threshold {
				open_lane_and_send_regular_message(OpenBridgeOrigin::sibling_parachain_origin());
			}
			// bridge is suspended but exporter accepts more messages
			assert_eq!(
				XcmOverBridge::bridge(locations.bridge_id()).unwrap().state,
				BridgeState::SoftSuspended
			);

			// export still can accept more messages
			assert_ok!(XcmOverBridge::validate(
				BridgedRelayNetwork::get(),
				0,
				&mut Some(universal_source()),
				&mut Some(bridged_relative_destination()),
				&mut Some(xcm.clone()),
			));

			// send more messages to reach `outbound_lane_stop_threshold`
			for _ in TestCongestionLimits::get().outbound_lane_congested_threshold..
				TestCongestionLimits::get().outbound_lane_stop_threshold
			{
				open_lane_and_send_regular_message(OpenBridgeOrigin::sibling_parachain_origin());
			}

			// bridge is suspended but exporter CANNOT accept more messages
			assert_eq!(
				XcmOverBridge::bridge(locations.bridge_id()).unwrap().state,
				BridgeState::HardSuspended
			);

			// export still can accept more messages
			assert_err!(
				XcmOverBridge::validate(
					BridgedRelayNetwork::get(),
					0,
					&mut Some(universal_source()),
					&mut Some(bridged_relative_destination()),
					&mut Some(xcm.clone()),
				),
				SendError::Transport("Exporter is suspended!"),
			);
		});
	}

	#[test]
	fn congestion_with_pallet_xcm_bridge_hub_router_works() {
		run_test(|| {
			// valid routable destination
			let dest = Location::new(2, BridgedUniversalDestination::get());

			fn router_bridge_state<T: pallet_xcm_bridge_router::Config<I>, I: 'static>(
				dest: &Location,
			) -> pallet_xcm_bridge_router::BridgeState {
				let bridge_id =
					<T::BridgeIdResolver as ResolveBridgeId>::resolve_for_dest(dest).unwrap();
				pallet_xcm_bridge_router::Bridges::<T, I>::get(&bridge_id)
			}

			// open two bridges
			let origin = OpenBridgeOrigin::sibling_parachain_origin();
			let origin_as_location =
				OpenBridgeOriginOf::<TestRuntime, ()>::try_origin(origin.clone()).unwrap();
			let (bridge_1, expected_lane_id_1) = open_lane(origin);
			let (bridge_2, expected_lane_id_2) = open_lane(RuntimeOrigin::root());
			assert_ne!(expected_lane_id_1, expected_lane_id_2);
			assert_ne!(bridge_1.bridge_id(), bridge_2.bridge_id());

			// we need to set `UniversalLocation` for `sibling_parachain_origin` for
			// `XcmOverBridgeWrappedWithExportMessageRouterInstance`.
			ExportMessageOriginUniversalLocation::set(Some(SiblingUniversalLocation::get()));

			// we need to update `maybe_notify` for `bridge_1` with `pallet_index` of
			// `XcmOverBridgeWrappedWithExportMessageRouter`,
			Bridges::<TestRuntime, ()>::mutate_extant(bridge_1.bridge_id(), |bridge| {
				bridge.maybe_notify = Some(Receiver::new(57, 0));
			});

			// check before
			// bridges are opened
			assert_eq!(
				XcmOverBridge::bridge(bridge_1.bridge_id()).unwrap().state,
				BridgeState::Opened
			);
			assert_eq!(
				XcmOverBridge::bridge(bridge_2.bridge_id()).unwrap().state,
				BridgeState::Opened
			);
			// both routers are uncongested
			assert!(
				!router_bridge_state::<
					TestRuntime,
					XcmOverBridgeWrappedWithExportMessageRouterInstance,
				>(&dest)
				.is_congested
			);
			assert!(
				!router_bridge_state::<TestRuntime, XcmOverBridgeByExportXcmRouterInstance>(&dest)
					.is_congested
			);
			assert!(!TestLocalXcmChannelManager::is_bridge_suspened(bridge_1.bridge_id()));
			assert!(!TestLocalXcmChannelManager::is_bridge_suspened(bridge_2.bridge_id()));
			assert!(!TestLocalXcmChannelManager::is_bridge_resumed(bridge_1.bridge_id()));
			assert!(!TestLocalXcmChannelManager::is_bridge_resumed(bridge_2.bridge_id()));

			// make bridges congested with sending too much messages
			for _ in 1..(TestCongestionLimits::get().outbound_lane_congested_threshold + 2) {
				// send `ExportMessage(message)` by `pallet_xcm_bridge_hub_router`.
				ExecuteXcmOverSendXcm::set_origin_for_execute(origin_as_location.clone());
				assert_ok!(send_xcm::<XcmOverBridgeWrappedWithExportMessageRouter>(
					dest.clone(),
					Xcm::<()>::default()
				));

				// call direct `ExportXcm` by `pallet_xcm_bridge_hub_router`.
				assert_ok!(send_xcm::<XcmOverBridgeByExportXcmRouter>(
					dest.clone(),
					Xcm::<()>::default()
				));
			}

			// checks after
			// bridges are suspended
			assert_eq!(
				XcmOverBridge::bridge(bridge_1.bridge_id()).unwrap().state,
				BridgeState::SoftSuspended
			);
			assert_eq!(
				XcmOverBridge::bridge(bridge_2.bridge_id()).unwrap().state,
				BridgeState::SoftSuspended
			);
			// both routers are congested
			assert!(
				router_bridge_state::<
					TestRuntime,
					XcmOverBridgeWrappedWithExportMessageRouterInstance,
				>(&dest)
				.is_congested
			);
			assert!(
				router_bridge_state::<TestRuntime, XcmOverBridgeByExportXcmRouterInstance>(&dest)
					.is_congested
			);
			assert!(TestLocalXcmChannelManager::is_bridge_suspened(bridge_1.bridge_id()));
			assert!(TestLocalXcmChannelManager::is_bridge_suspened(bridge_2.bridge_id()));
			assert!(!TestLocalXcmChannelManager::is_bridge_resumed(bridge_1.bridge_id()));
			assert!(!TestLocalXcmChannelManager::is_bridge_resumed(bridge_2.bridge_id()));

			// make bridges uncongested to trigger resume signal
			XcmOverBridge::on_bridge_messages_delivered(
				expected_lane_id_1,
				TestCongestionLimits::get().outbound_lane_uncongested_threshold,
			);
			XcmOverBridge::on_bridge_messages_delivered(
				expected_lane_id_2,
				TestCongestionLimits::get().outbound_lane_uncongested_threshold,
			);

			// bridges are again opened
			assert_eq!(
				XcmOverBridge::bridge(bridge_1.bridge_id()).unwrap().state,
				BridgeState::Opened
			);
			assert_eq!(
				XcmOverBridge::bridge(bridge_2.bridge_id()).unwrap().state,
				BridgeState::Opened
			);
			// both routers are uncongested
			assert_eq!(
				router_bridge_state::<
					TestRuntime,
					XcmOverBridgeWrappedWithExportMessageRouterInstance,
				>(&dest),
				pallet_xcm_bridge_router::BridgeState {
					delivery_fee_factor: MINIMAL_DELIVERY_FEE_FACTOR,
					is_congested: false
				}
			);
			assert_eq!(
				router_bridge_state::<TestRuntime, XcmOverBridgeByExportXcmRouterInstance>(&dest),
				pallet_xcm_bridge_router::BridgeState {
					delivery_fee_factor: MINIMAL_DELIVERY_FEE_FACTOR,
					is_congested: false
				}
			);
			assert!(TestLocalXcmChannelManager::is_bridge_resumed(bridge_1.bridge_id()));
			assert!(TestLocalXcmChannelManager::is_bridge_resumed(bridge_2.bridge_id()));
		})
	}
}
