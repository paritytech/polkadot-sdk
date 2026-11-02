// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! XCM over Speculative Messaging, on one channel per peer, `(domain 0, num 0)`, whose `Data`
//! payload is the SCALE-encoded `VersionedXcm`. Inbound, [`EnqueueToXcmQueue`] queues it in the
//! channel's own message queue; outbound, [`SpecMsgRouter`] sends to a sibling with no HRMP
//! channel.

use crate::{
	BlockStreamsRoot, Config, Error, HrmpClosing, OnSpecMsgData, OutChannels, OutboundLeafHashes,
	OutboundMessages, Pallet, QueueFootprintQuery, SendsThisBlock,
};
use alloc::vec::Vec;
use codec::{DecodeAll, DecodeLimit, Encode};
use core::marker::PhantomData;
use cumulus_primitives_core::{ChannelStatus, GetChannelInfo};
use cumulus_primitives_spec_messaging::{
	ChannelMessage, InChannelId, MessagePosition, OutChannelId, StreamId,
};
use frame_support::{
	dispatch::DispatchClass,
	pallet_prelude::Weight,
	traits::{EnqueueMessage, Get},
	BoundedSlice,
};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use polkadot_runtime_common::xcm_sender::PriceForMessageDelivery;
use sp_runtime::traits::Convert;
use xcm::{latest::prelude::*, VersionedLocation, VersionedXcm, WrapVersion, MAX_XCM_DECODE_DEPTH};
use xcm_builder::InspectMessageQueues;

/// The channel `domain` XCM uses.
pub const XCM_CHANNEL_DOMAIN: u8 = 0;

/// The channel `num` XCM uses.
pub const XCM_CHANNEL_NUM: u16 = 0;

/// The outbound XCM channel to `peer`.
pub fn xcm_channel(peer: ParaId) -> OutChannelId {
	OutChannelId { recipient: peer, domain: XCM_CHANNEL_DOMAIN, num: XCM_CHANNEL_NUM }
}

/// Whether `stream` is an XCM channel's data stream.
fn is_xcm_channel(stream: &StreamId) -> bool {
	matches!(stream, StreamId::Channel { domain: XCM_CHANNEL_DOMAIN, num: XCM_CHANNEL_NUM, .. })
}

/// [`OnSpecMsgData`] that forwards the XCM channel's payloads, in order, to the channel's message
/// queue:
///
/// ```ignore
/// type DataHandler = EnqueueToXcmQueue<
/// 	TransformOrigin<MessageQueue, AggregateMessageOrigin, InChannelId, ChannelToSpecMsg>,
/// 	EnqueueWeight,
/// >;
/// ```
///
/// `EnqueueWeight` maps a payload length to the weight of enqueueing it. Other channels are never
/// executed as XCM, since XCM runs with the sibling origin; they are dropped. The pallet's
/// `integrity_test` keeps `MaxMsgLen` within the queue's `MaxMessageLen`.
pub struct EnqueueToXcmQueue<Queue, EnqueueWeight>(PhantomData<(Queue, EnqueueWeight)>);

impl<Queue: EnqueueMessage<InChannelId>, EnqueueWeight: Convert<u32, Weight>> OnSpecMsgData
	for EnqueueToXcmQueue<Queue, EnqueueWeight>
{
	fn on_data(source: ParaId, stream: StreamId, _position: MessagePosition, data: Vec<u8>) {
		let StreamId::Channel { domain, num, .. } = stream else { return };
		if !is_xcm_channel(&stream) {
			return;
		}
		let Ok(message) = BoundedSlice::try_from(&data[..]) else {
			frame_support::defensive!(
				"spec-msg XCM payload exceeds the queue's `MaxMessageLen`; dropped \
				 (`MaxMessageLen` must be at least `MaxMsgLen`)"
			);
			return;
		};
		Queue::enqueue_message(message, InChannelId { sender: source, domain, num });
	}

	fn on_data_weight(len: u32) -> Weight {
		EnqueueWeight::convert(len)
	}

	fn max_len() -> u32 {
		Queue::MaxMessageLen::get()
	}
}

/// [`QueueFootprintQuery`] over a message queue that reports footprints per [`InChannelId`], such
/// as `TransformOrigin<MessageQueue, AggregateMessageOrigin, InChannelId, ChannelToSpecMsg>`. A
/// channel whose queue has `MaxQueueFootprint` ready pages stops being consumed.
pub struct QueueFootprintOf<Queue>(PhantomData<Queue>);

impl<Queue: frame_support::traits::QueueFootprintQuery<InChannelId>>
	QueueFootprintQuery<InChannelId> for QueueFootprintOf<Queue>
{
	fn ready_pages(id: &InChannelId) -> u32 {
		Queue::footprint(*id).ready_pages
	}
}

/// XCM sender to sibling parachains over Speculative Messaging. Place it **before** `XcmpQueue`,
/// which accepts any sibling.
///
/// - HRMP wins while a channel exists (`Full` included), unless the sibling is [`HrmpClosing`].
/// - Otherwise the XCM goes over the spec-msg XCM channel if it is open, else falls through.
/// - An open channel without capacity is a hard [`SendError::Transport`]: falling through would
///   lose the XCM in `XcmpQueue`.
/// - Each delivery charges [`Pallet::send_weight`] to the block.
///
/// `ChannelInfo` is usually `ParachainSystem`, `VersionWrapper` usually `PolkadotXcm` (same
/// destination `Location` as HRMP, so negotiated versions carry over).
pub struct SpecMsgRouter<T, ChannelInfo, VersionWrapper, Price>(
	PhantomData<(T, ChannelInfo, VersionWrapper, Price)>,
);

impl<T, ChannelInfo, VersionWrapper, Price> SendXcm
	for SpecMsgRouter<T, ChannelInfo, VersionWrapper, Price>
where
	T: Config,
	ChannelInfo: GetChannelInfo,
	VersionWrapper: WrapVersion,
	Price: PriceForMessageDelivery<Id = ParaId>,
{
	type Ticket = (OutChannelId, Vec<u8>);

	fn validate(
		dest: &mut Option<Location>,
		msg: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		let d = dest.take().ok_or(SendError::MissingArgument)?;
		let id = match d.unpack() {
			(1, [Parachain(id)]) => ParaId::from(*id),
			_ => {
				*dest = Some(d);
				return Err(SendError::NotApplicable);
			},
		};
		let channel = xcm_channel(id);
		let hrmp_open = !HrmpClosing::<T>::contains_key(id) &&
			!matches!(ChannelInfo::get_channel_status(id), ChannelStatus::Closed);
		let spec_msg_open = OutChannels::<T>::get(channel)
			.is_some_and(|state| state.active_session(T::SupportedVersions::get()).is_some());
		if hrmp_open || !spec_msg_open {
			*dest = Some(d);
			return Err(SendError::NotApplicable);
		}

		let xcm = msg.take().ok_or(SendError::MissingArgument)?;
		let price = Price::price_for_delivery(id, &xcm);
		let versioned = VersionWrapper::wrap_version(&d, xcm)
			.map_err(|()| SendError::DestinationUnsupported)?;
		versioned.check_is_decodable().map_err(|()| SendError::ExceedsMaxMessageSize)?;
		let encoded = versioned.encode();
		Pallet::<T>::can_send(channel, encoded.len()).map_err(|error| {
			if error == Error::<T>::MessageTooBig {
				SendError::ExceedsMaxMessageSize
			} else {
				SendError::Transport("spec-msg XCM channel has no capacity")
			}
		})?;
		Ok(((channel, encoded), price))
	}

	fn deliver((channel, encoded): Self::Ticket) -> Result<XcmHash, SendError> {
		let hash = sp_io::hashing::blake2_256(&encoded);
		let weight = Pallet::<T>::send_weight(encoded.len() as u32);
		Pallet::<T>::send(channel, encoded)
			.map_err(|_| SendError::Transport("spec-msg XCM send failed"))?;
		// Not a call, so nothing else charges the send and the commit it causes.
		frame_system::Pallet::<T>::register_extra_weight_unchecked(weight, DispatchClass::Normal);
		Ok(hash)
	}
}

/// The dry-run APIs' view of this block's XCM sends: the XCM channels' `Data` leaves, decoded back
/// to the `VersionedXcm`s the router sent, keyed by destination. Mirrors `XcmpQueue`.
impl<T: Config, ChannelInfo, VersionWrapper, Price> InspectMessageQueues
	for SpecMsgRouter<T, ChannelInfo, VersionWrapper, Price>
{
	fn clear_messages() {
		// These hold only this block's sends, so this isolates a dry run's output.
		let _ = OutboundMessages::<T>::clear(u32::MAX, None);
		let _ = OutboundLeafHashes::<T>::clear(u32::MAX, None);
		SendsThisBlock::<T>::kill();
		// The state is the last block's, whose `on_finalize` may have committed a root: a send
		// would then fail with `RootCommitted`, though it would not in a real block.
		BlockStreamsRoot::<T>::kill();
	}

	fn get_messages() -> Vec<(VersionedLocation, Vec<VersionedXcm<()>>)> {
		OutboundMessages::<T>::iter()
			.filter(|(stream, _)| is_xcm_channel(stream))
			.filter_map(|(stream, leaves)| {
				let StreamId::Channel { recipient, .. } = stream else { return None };
				let messages: Vec<_> = leaves
					.iter()
					.filter_map(|leaf| match ChannelMessage::decode_all(&mut &leaf[..]).ok()? {
						ChannelMessage::Data(data) => {
							VersionedXcm::<()>::decode_all_with_depth_limit(
								MAX_XCM_DECODE_DEPTH,
								&mut &data[..],
							)
							.ok()
						},
						_ => None,
					})
					.collect();
				(!messages.is_empty()).then(|| {
					(
						VersionedLocation::from(Location::new(1, Parachain(recipient.into()))),
						messages,
					)
				})
			})
			.collect()
	}
}
