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

use bp_xcm_bridge_hub::XcmAsPlainPayload;

use bp_messages::{source_chain::MessagesBridge, LaneId};
use frame_support::traits::Get;
use pallet_bridge_messages::{
	Config as BridgeMessagesConfig, Error, Pallet as BridgeMessagesPallet,
};
use xcm::prelude::*;
use xcm_builder::{HaulBlob, HaulBlobError, HaulBlobExporter};
use xcm_executor::traits::ExportXcm;

/// An easy way to access `HaulBlobExporter`.
type PalletAsHaulBlobExporter<T, I> = HaulBlobExporter<
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
		LaneId,
		<MessagesPallet<T, I> as MessagesBridge<T::OutboundPayload>>::SendMessageArgs,
		XcmHash,
	);

	fn validate(
		network: NetworkId,
		channel: u32,
		universal_source: &mut Option<InteriorLocation>,
		destination: &mut Option<InteriorLocation>,
		message: &mut Option<Xcm<()>>,
	) -> Result<(Self::Ticket, Assets), SendError> {
		// `HaulBlobExporter` may consume the `universal_source` and `destination` arguments, so
		// let's save them before
		let bridge_origin_universal_location =
			universal_source.clone().take().ok_or(SendError::MissingArgument)?;
		let bridge_destination_interior_location =
			destination.clone().take().ok_or(SendError::MissingArgument)?;

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

		// ok - now we know that the message may be routed by the pallet, let's prepare the
		// destination universal location
		let mut bridge_destination_universal_location: InteriorLocation =
			GlobalConsensus(network).into();
		bridge_destination_universal_location
			.append_with(bridge_destination_interior_location)
			.map_err(|_| SendError::Unroutable)?;

		// .. and the origin relative location
		let bridge_origin_relative_location =
			bridge_origin_universal_location.relative_to(&T::UniversalLocation::get());

		// then we are able to compute the lane id used to send messages
		let bridge_locations = Self::bridge_locations(
			Box::new(bridge_origin_relative_location),
			Box::new(bridge_destination_universal_location.into()),
		)
		.map_err(|_| SendError::Unroutable)?;

		let bridge_message =
			MessagesPallet::<T, I>::validate_message(bridge_locations.lane_id, &blob).map_err(
				|e| {
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
						"XCM message {:?} cannot be exported because of bridge error: {:?} on bridge {:?}",
						id,
						e,
						bridge_locations,
					);
					SendError::Transport("BridgeValidateError")
				},
			)?;

		Ok(((bridge_locations.lane_id, bridge_message, id), price))
	}

	fn deliver((lane_id, bridge_message, id): Self::Ticket) -> Result<XcmHash, SendError> {
		let artifacts = MessagesPallet::<T, I>::send_message(bridge_message);

		log::info!(
			target: LOG_TARGET,
			"XCM message {:?} has been enqueued at bridge {:?} with nonce {}",
			id,
			lane_id,
			artifacts.nonce,
		);

		Ok(id)
	}
}

/// Dummy implementation of the `HaulBlob` trait that is never called.
///
/// We are using `HaulBlobExporter`, which requires `HaulBlob` implementation. It assumes that
/// there's a single channel between two bridge hubs - `HaulBlob` only accepts the blob and nothing
/// else. But bridge messages pallet may have a dedicated channel (lane) for every pair of bridged
/// chains. So we are using our own `ExportXcm` implementation, but to utilize `HaulBlobExporter` we
/// still need this `DummyHaulBlob`.
struct DummyHaulBlob;

impl HaulBlob for DummyHaulBlob {
	fn haul_blob(_blob: XcmAsPlainPayload) -> Result<(), HaulBlobError> {
		Err(HaulBlobError::Transport("DummyHaulBlob"))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{mock::*, LanesManagerOf};
	use xcm_executor::traits::export_xcm;

	#[test]
	fn proper_lane_is_used_by_export_xcm() {
		run_test(|| {
			// open expected outbound lane
			let origin = OpenBridgeOrigin::sibling_parachain_origin();
			let with = bridged_asset_hub_location();
			let locations =
				XcmOverBridge::bridge_locations_from_origin(origin, Box::new(with.into())).unwrap();

			let lanes_manager = LanesManagerOf::<TestRuntime, ()>::new();
			lanes_manager.create_outbound_lane(locations.lane_id).unwrap();
			assert!(lanes_manager
				.active_outbound_lane(locations.lane_id)
				.unwrap()
				.queued_messages()
				.is_empty());

			// now let's try to enqueue message using our `ExportXcm` implementation
			export_xcm::<XcmOverBridge>(
				BridgedRelayNetwork::get(),
				0,
				locations.bridge_origin_universal_location,
				locations.bridge_destination_universal_location.split_first().0,
				vec![Instruction::ClearOrigin].into(),
			)
			.unwrap();

			// double check that the message has been pushed to the expected lane
			// (it should already been checked during `send_message` call)
			assert!(!lanes_manager
				.active_outbound_lane(locations.lane_id)
				.unwrap()
				.queued_messages()
				.is_empty());
		});
	}
}
