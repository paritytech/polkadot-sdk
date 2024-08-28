// Copyright (C) Parity Technologies (UK) Ltd.
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
	source_chain::OnMessagesDelivered,
	target_chain::{DispatchMessage, MessageDispatch},
	LaneId, MessageNonce,
};
use bp_runtime::messages::MessageDispatchResult;
pub use bp_xcm_bridge_hub::XcmAsPlainPayload;
use bp_xcm_bridge_hub_router::XcmChannelStatusProvider;
use codec::{Decode, Encode};
use frame_support::{traits::Get, weights::Weight, CloneNoBound, EqNoBound, PartialEqNoBound};
use pallet_bridge_messages::{
	Config as MessagesConfig, OutboundLanesCongestedSignals, WeightInfoExt as MessagesPalletWeights,
};
use scale_info::TypeInfo;
use sp_runtime::SaturatedConversion;
use sp_std::{fmt::Debug, marker::PhantomData};
use xcm::prelude::*;
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

/// [`XcmBlobMessageDispatch`] is responsible for dispatching received messages
///
/// It needs to be used at the target bridge hub.
pub struct XcmBlobMessageDispatch<DispatchBlob, Weights, Channel> {
	_marker: sp_std::marker::PhantomData<(DispatchBlob, Weights, Channel)>,
}

impl<
		BlobDispatcher: DispatchBlob,
		Weights: MessagesPalletWeights,
		Channel: XcmChannelStatusProvider,
	> MessageDispatch for XcmBlobMessageDispatch<BlobDispatcher, Weights, Channel>
{
	type DispatchPayload = XcmAsPlainPayload;
	type DispatchLevelResult = XcmBlobMessageDispatchResult;

	fn is_active() -> bool {
		!Channel::is_congested()
	}

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

/// A pair of sending chain location and message lane, used by this chain to send messages
/// over the bridge.
#[cfg_attr(feature = "std", derive(Debug, Eq, PartialEq))]
pub struct SenderAndLane {
	/// Sending chain relative location.
	pub location: Location,
	/// Message lane, used by the sending chain.
	pub lane: LaneId,
}

impl SenderAndLane {
	/// Create new object using provided location and lane.
	pub fn new(location: Location, lane: LaneId) -> Self {
		SenderAndLane { location, lane }
	}
}

/// [`XcmBlobHauler`] is responsible for sending messages to the bridge "point-to-point link" from
/// one side, where on the other it can be dispatched by [`XcmBlobMessageDispatch`].
pub trait XcmBlobHauler {
	/// Runtime that has messages pallet deployed.
	type Runtime: MessagesConfig<Self::MessagesInstance>;
	/// Instance of the messages pallet that is used to send messages.
	type MessagesInstance: 'static;

	/// Actual XCM message sender (`HRMP` or `UMP`) to the source chain
	/// location (`Self::SenderAndLane::get().location`).
	type ToSourceChainSender: SendXcm;
	/// An XCM message that is sent to the sending chain when the bridge queue becomes congested.
	type CongestedMessage: Get<Option<Xcm<()>>>;
	/// An XCM message that is sent to the sending chain when the bridge queue becomes not
	/// congested.
	type UncongestedMessage: Get<Option<Xcm<()>>>;

	/// Returns `true` if we want to handle congestion.
	fn supports_congestion_detection() -> bool {
		Self::CongestedMessage::get().is_some() || Self::UncongestedMessage::get().is_some()
	}
}

/// XCM bridge adapter which connects [`XcmBlobHauler`] with [`pallet_bridge_messages`] and
/// makes sure that XCM blob is sent to the outbound lane to be relayed.
///
/// It needs to be used at the source bridge hub.
pub struct XcmBlobHaulerAdapter<XcmBlobHauler, Lanes>(
	sp_std::marker::PhantomData<(XcmBlobHauler, Lanes)>,
);

impl<
		H: XcmBlobHauler,
		Lanes: Get<sp_std::vec::Vec<(SenderAndLane, (NetworkId, InteriorLocation))>>,
	> OnMessagesDelivered for XcmBlobHaulerAdapter<H, Lanes>
{
	fn on_messages_delivered(lane: LaneId, enqueued_messages: MessageNonce) {
		if let Some(sender_and_lane) =
			Lanes::get().iter().find(|link| link.0.lane == lane).map(|link| &link.0)
		{
			// notify XCM queue manager about updated lane state
			LocalXcmQueueManager::<H>::on_bridge_messages_delivered(
				sender_and_lane,
				enqueued_messages,
			);
		}
	}
}

/// Manager of local XCM queues (and indirectly - underlying transport channels) that
/// controls the queue state.
///
/// It needs to be used at the source bridge hub.
pub struct LocalXcmQueueManager<H>(PhantomData<H>);

/// Maximal number of messages in the outbound bridge queue. Once we reach this limit, we
/// send a "congestion" XCM message to the sending chain.
const OUTBOUND_LANE_CONGESTED_THRESHOLD: MessageNonce = 8_192;

/// After we have sent "congestion" XCM message to the sending chain, we wait until number
/// of messages in the outbound bridge queue drops to this count, before sending `uncongestion`
/// XCM message.
const OUTBOUND_LANE_UNCONGESTED_THRESHOLD: MessageNonce = 1_024;

impl<H: XcmBlobHauler> LocalXcmQueueManager<H> {
	/// Must be called whenever we push a message to the bridge lane.
	pub fn on_bridge_message_enqueued(
		sender_and_lane: &SenderAndLane,
		enqueued_messages: MessageNonce,
	) {
		// skip if we dont want to handle congestion
		if !H::supports_congestion_detection() {
			return
		}

		// if we have already sent the congestion signal, we don't want to do anything
		if Self::is_congested_signal_sent(sender_and_lane.lane) {
			return
		}

		// if the bridge queue is not congested, we don't want to do anything
		let is_congested = enqueued_messages > OUTBOUND_LANE_CONGESTED_THRESHOLD;
		if !is_congested {
			return
		}

		log::info!(
			target: crate::LOG_TARGET_BRIDGE_DISPATCH,
			"Sending 'congested' XCM message to {:?} to avoid overloading lane {:?}: there are\
			{} messages queued at the bridge queue",
			sender_and_lane.location,
			sender_and_lane.lane,
			enqueued_messages,
		);

		if let Err(e) = Self::send_congested_signal(sender_and_lane) {
			log::info!(
				target: crate::LOG_TARGET_BRIDGE_DISPATCH,
				"Failed to send the 'congested' XCM message to {:?}: {:?}",
				sender_and_lane.location,
				e,
			);
		}
	}

	/// Must be called whenever we receive a message delivery confirmation.
	pub fn on_bridge_messages_delivered(
		sender_and_lane: &SenderAndLane,
		enqueued_messages: MessageNonce,
	) {
		// skip if we don't want to handle congestion
		if !H::supports_congestion_detection() {
			return
		}

		// if we have not sent the congestion signal before, we don't want to do anything
		if !Self::is_congested_signal_sent(sender_and_lane.lane) {
			return
		}

		// if the bridge queue is still congested, we don't want to do anything
		let is_congested = enqueued_messages > OUTBOUND_LANE_UNCONGESTED_THRESHOLD;
		if is_congested {
			return
		}

		log::info!(
			target: crate::LOG_TARGET_BRIDGE_DISPATCH,
			"Sending 'uncongested' XCM message to {:?}. Lane {:?}: there are\
			{} messages queued at the bridge queue",
			sender_and_lane.location,
			sender_and_lane.lane,
			enqueued_messages,
		);

		if let Err(e) = Self::send_uncongested_signal(sender_and_lane) {
			log::info!(
				target: crate::LOG_TARGET_BRIDGE_DISPATCH,
				"Failed to send the 'uncongested' XCM message to {:?}: {:?}",
				sender_and_lane.location,
				e,
			);
		}
	}

	/// Returns true if we have sent "congested" signal to the `sending_chain_location`.
	fn is_congested_signal_sent(lane: LaneId) -> bool {
		OutboundLanesCongestedSignals::<H::Runtime, H::MessagesInstance>::get(lane)
	}

	/// Send congested signal to the `sending_chain_location`.
	fn send_congested_signal(sender_and_lane: &SenderAndLane) -> Result<(), SendError> {
		if let Some(msg) = H::CongestedMessage::get() {
			send_xcm::<H::ToSourceChainSender>(sender_and_lane.location.clone(), msg)?;
			OutboundLanesCongestedSignals::<H::Runtime, H::MessagesInstance>::insert(
				sender_and_lane.lane,
				true,
			);
		}
		Ok(())
	}

	/// Send `uncongested` signal to the `sending_chain_location`.
	fn send_uncongested_signal(sender_and_lane: &SenderAndLane) -> Result<(), SendError> {
		if let Some(msg) = H::UncongestedMessage::get() {
			send_xcm::<H::ToSourceChainSender>(sender_and_lane.location.clone(), msg)?;
			OutboundLanesCongestedSignals::<H::Runtime, H::MessagesInstance>::remove(
				sender_and_lane.lane,
			);
		}
		Ok(())
	}
}

/// Adapter for the implementation of `GetVersion`, which attempts to find the minimal
/// configured XCM version between the destination `dest` and the bridge hub location provided as
/// `Get<Location>`.
pub struct XcmVersionOfDestAndRemoteBridge<Version, RemoteBridge>(
	sp_std::marker::PhantomData<(Version, RemoteBridge)>,
);
impl<Version: GetVersion, RemoteBridge: Get<Location>> GetVersion
	for XcmVersionOfDestAndRemoteBridge<Version, RemoteBridge>
{
	fn get_version_for(dest: &Location) -> Option<XcmVersion> {
		let dest_version = Version::get_version_for(dest);
		let bridge_hub_version = Version::get_version_for(&RemoteBridge::get());

		match (dest_version, bridge_hub_version) {
			(Some(dv), Some(bhv)) => Some(sp_std::cmp::min(dv, bhv)),
			(Some(dv), None) => Some(dv),
			(None, Some(bhv)) => Some(bhv),
			(None, None) => None,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::*;

	use bp_messages::OutboundLaneData;
	use frame_support::parameter_types;
	use pallet_bridge_messages::OutboundLanes;

	parameter_types! {
		pub TestSenderAndLane: SenderAndLane = SenderAndLane {
			location: Location::new(1, [Parachain(1000)]),
			lane: TEST_LANE_ID,
		};
		pub TestLanes: sp_std::vec::Vec<(SenderAndLane, (NetworkId, InteriorLocation))> = sp_std::vec![
			(TestSenderAndLane::get(), (NetworkId::ByGenesis([0; 32]), InteriorLocation::Here))
		];
		pub DummyXcmMessage: Xcm<()> = Xcm::new();
	}

	struct DummySendXcm;

	impl DummySendXcm {
		fn messages_sent() -> u32 {
			frame_support::storage::unhashed::get(b"DummySendXcm").unwrap_or(0)
		}
	}

	impl SendXcm for DummySendXcm {
		type Ticket = ();

		fn validate(
			_destination: &mut Option<Location>,
			_message: &mut Option<Xcm<()>>,
		) -> SendResult<Self::Ticket> {
			Ok(((), Default::default()))
		}

		fn deliver(_ticket: Self::Ticket) -> Result<XcmHash, SendError> {
			let messages_sent: u32 = Self::messages_sent();
			frame_support::storage::unhashed::put(b"DummySendXcm", &(messages_sent + 1));
			Ok(XcmHash::default())
		}
	}

	struct TestBlobHauler;

	impl XcmBlobHauler for TestBlobHauler {
		type Runtime = TestRuntime;
		type MessagesInstance = ();

		type ToSourceChainSender = DummySendXcm;
		type CongestedMessage = DummyXcmMessage;
		type UncongestedMessage = DummyXcmMessage;
	}

	type TestBlobHaulerAdapter = XcmBlobHaulerAdapter<TestBlobHauler, TestLanes>;

	fn fill_up_lane_to_congestion() -> MessageNonce {
		let latest_generated_nonce = OUTBOUND_LANE_CONGESTED_THRESHOLD;
		OutboundLanes::<TestRuntime, ()>::insert(
			TEST_LANE_ID,
			OutboundLaneData {
				oldest_unpruned_nonce: 0,
				latest_received_nonce: 0,
				latest_generated_nonce,
			},
		);
		latest_generated_nonce
	}

	#[test]
	fn congested_signal_is_not_sent_twice() {
		run_test(|| {
			let enqueued = fill_up_lane_to_congestion();

			// next sent message leads to congested signal
			LocalXcmQueueManager::<TestBlobHauler>::on_bridge_message_enqueued(
				&TestSenderAndLane::get(),
				enqueued + 1,
			);
			assert_eq!(DummySendXcm::messages_sent(), 1);

			// next sent message => we don't sent another congested signal
			LocalXcmQueueManager::<TestBlobHauler>::on_bridge_message_enqueued(
				&TestSenderAndLane::get(),
				enqueued,
			);
			assert_eq!(DummySendXcm::messages_sent(), 1);
		});
	}

	#[test]
	fn congested_signal_is_not_sent_when_outbound_lane_is_not_congested() {
		run_test(|| {
			LocalXcmQueueManager::<TestBlobHauler>::on_bridge_message_enqueued(
				&TestSenderAndLane::get(),
				1,
			);
			assert_eq!(DummySendXcm::messages_sent(), 0);
		});
	}

	#[test]
	fn congested_signal_is_sent_when_outbound_lane_is_congested() {
		run_test(|| {
			let enqueued = fill_up_lane_to_congestion();

			// next sent message leads to congested signal
			LocalXcmQueueManager::<TestBlobHauler>::on_bridge_message_enqueued(
				&TestSenderAndLane::get(),
				enqueued + 1,
			);
			assert_eq!(DummySendXcm::messages_sent(), 1);
			assert!(LocalXcmQueueManager::<TestBlobHauler>::is_congested_signal_sent(TEST_LANE_ID));
		});
	}

	#[test]
	fn uncongested_signal_is_not_sent_when_messages_are_delivered_at_other_lane() {
		run_test(|| {
			LocalXcmQueueManager::<TestBlobHauler>::send_congested_signal(&TestSenderAndLane::get()).unwrap();
			assert_eq!(DummySendXcm::messages_sent(), 1);

			// when we receive a delivery report for other lane, we don't send an uncongested signal
			TestBlobHaulerAdapter::on_messages_delivered(LaneId([42, 42, 42, 42]), 0);
			assert_eq!(DummySendXcm::messages_sent(), 1);
		});
	}

	#[test]
	fn uncongested_signal_is_not_sent_when_we_havent_send_congested_signal_before() {
		run_test(|| {
			TestBlobHaulerAdapter::on_messages_delivered(TEST_LANE_ID, 0);
			assert_eq!(DummySendXcm::messages_sent(), 0);
		});
	}

	#[test]
	fn uncongested_signal_is_not_sent_if_outbound_lane_is_still_congested() {
		run_test(|| {
			LocalXcmQueueManager::<TestBlobHauler>::send_congested_signal(&TestSenderAndLane::get()).unwrap();
			assert_eq!(DummySendXcm::messages_sent(), 1);

			TestBlobHaulerAdapter::on_messages_delivered(
				TEST_LANE_ID,
				OUTBOUND_LANE_UNCONGESTED_THRESHOLD + 1,
			);
			assert_eq!(DummySendXcm::messages_sent(), 1);
		});
	}

	#[test]
	fn uncongested_signal_is_sent_if_outbound_lane_is_uncongested() {
		run_test(|| {
			LocalXcmQueueManager::<TestBlobHauler>::send_congested_signal(&TestSenderAndLane::get()).unwrap();
			assert_eq!(DummySendXcm::messages_sent(), 1);

			TestBlobHaulerAdapter::on_messages_delivered(
				TEST_LANE_ID,
				OUTBOUND_LANE_UNCONGESTED_THRESHOLD,
			);
			assert_eq!(DummySendXcm::messages_sent(), 2);
		});
	}
}
