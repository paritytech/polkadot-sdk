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

use bp_messages::source_chain::MessagesBridge;
use bp_xcm_bridge_hub::XcmAsPlainPayload;
use bridge_runtime_common::messages_xcm_extension::{LocalXcmQueueManager, SenderAndLane};
use pallet_bridge_messages::{Config as BridgeMessagesConfig, Pallet as BridgeMessagesPallet};
use xcm::prelude::*;
use xcm_builder::{HaulBlob, HaulBlobError, HaulBlobExporter};
use xcm_executor::traits::ExportXcm;

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
		SenderAndLane,
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
		// Find supported lane_id.
		let sender_and_lane = Self::lane_for(
			universal_source.as_ref().ok_or(SendError::MissingArgument)?,
			(&network, destination.as_ref().ok_or(SendError::MissingArgument)?),
		)
		.ok_or(SendError::NotApplicable)?;

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

		let bridge_message = MessagesPallet::<T, I>::validate_message(sender_and_lane.lane, &blob)
			.map_err(|e| {
				log::debug!(
					target: LOG_TARGET,
					"XCM message {:?} cannot be exported because of bridge error {:?} on bridge {:?}",
					id,
					e,
					sender_and_lane.lane,
				);
				SendError::Transport("BridgeValidateError")
			})?;

		Ok(((sender_and_lane, bridge_message, id), price))
	}

	fn deliver((sender_and_lane, bridge_message, id): Self::Ticket) -> Result<XcmHash, SendError> {
		let lane_id = sender_and_lane.lane;
		let artifacts = MessagesPallet::<T, I>::send_message(bridge_message);

		log::info!(
			target: LOG_TARGET,
			"XCM message {:?} has been enqueued at bridge {:?} with nonce {}",
			id,
			lane_id,
			artifacts.nonce,
		);

		// notify XCM queue manager about updated lane state
		LocalXcmQueueManager::<T::LanesSupport>::on_bridge_message_enqueued(
			&sender_and_lane,
			artifacts.enqueued_messages,
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
pub struct DummyHaulBlob;

impl HaulBlob for DummyHaulBlob {
	fn haul_blob(_blob: XcmAsPlainPayload) -> Result<(), HaulBlobError> {
		Err(HaulBlobError::Transport("DummyHaulBlob"))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::*;
	use frame_support::assert_ok;
	use xcm_executor::traits::export_xcm;

	fn universal_source() -> InteriorLocation {
		[GlobalConsensus(RelayNetwork::get()), Parachain(SIBLING_ASSET_HUB_ID)].into()
	}

	fn universal_destination() -> InteriorLocation {
		BridgedDestination::get()
	}

	#[test]
	fn export_works() {
		run_test(|| {
			assert_ok!(export_xcm::<XcmOverBridge>(
				BridgedRelayNetwork::get(),
				0,
				universal_source(),
				universal_destination(),
				vec![Instruction::ClearOrigin].into(),
			));
		})
	}

	#[test]
	fn export_fails_if_argument_is_missing() {
		run_test(|| {
			assert_eq!(
				XcmOverBridge::validate(
					BridgedRelayNetwork::get(),
					0,
					&mut None,
					&mut Some(universal_destination()),
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
			let expected_lane_id = TEST_LANE_ID;

			assert_eq!(
				XcmOverBridge::validate(
					BridgedRelayNetwork::get(),
					0,
					&mut Some(universal_source()),
					&mut Some(universal_destination()),
					&mut Some(Vec::new().into()),
				)
				.unwrap()
				.0
				 .0
				.lane,
				expected_lane_id,
			);
		})
	}
}
