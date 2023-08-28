// Copyright 2020-2021 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! A pallet which uses the XCMP transport layer to handle both incoming and outgoing XCM message
//! sending and dispatch, queuing, signalling and backpressure. To do so, it implements:
//! * `XcmpMessageHandler`
//! * `XcmpMessageSource`
//!
//! Also provides an implementation of `SendXcm` which can be placed in a router tuple for relaying
//! XCM over XCMP if the destination is `Parent/Parachain`. It requires an implementation of
//! `XcmExecutor` for dispatching incoming XCM messages.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod migration;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;
pub use weights::WeightInfo;

use bounded_collections::BoundedBTreeSet;
use codec::{Decode, DecodeLimit, Encode};
use cumulus_primitives_core::{
	relay_chain::BlockNumber as RelayBlockNumber, ChannelStatus, GetChannelInfo, MessageSendError,
	ParaId, XcmpMessageFormat, XcmpMessageHandler, XcmpMessageSource,
};
use frame_support::{
	defensive, defensive_assert,
	traits::{EnqueueMessage, EnsureOrigin, Get, QueuePausedQuery},
	weights::{constants::WEIGHT_REF_TIME_PER_MILLIS, Weight, WeightMeter},
	BoundedVec,
};
use pallet_message_queue::OnQueueChanged;
use polkadot_runtime_common::xcm_sender::PriceForParachainDelivery;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_std::prelude::*;
use xcm::{latest::prelude::*, VersionedXcm, WrapVersion, MAX_XCM_DECODE_DEPTH};
use xcm_executor::traits::ConvertOrigin;

pub use pallet::*;

/// Index used to identify overweight XCMs.
pub type OverweightIndex = u64;
pub type MaxXcmpMessageLenOf<T> =
	<<T as Config>::XcmpQueue as EnqueueMessage<ParaId>>::MaxMessageLen;

const LOG_TARGET: &str = "xcmp_queue";
const DEFAULT_POV_SIZE: u64 = 64 * 1024; // 64 KB

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::storage_version(migration::STORAGE_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Information on the available XCMP channels.
		type ChannelInfo: GetChannelInfo;

		/// Means of converting an `Xcm` into a `VersionedXcm`.
		type VersionWrapper: WrapVersion;

		/// Enqueue an inbound horizontal message for later processing.
		///
		/// This defines the maximal message length via [`crate::MaxXcmpMessageLenOf`]. The pallet
		/// assumes that this hook will eventually process all the pushed messages. No further
		/// explicit nudging is required.
		type XcmpQueue: EnqueueMessage<ParaId>;

		/// The maximum number of inbound XCMP channels that can be suspended simultaneously.
		///
		/// Any further channel suspensions will fail and messages may get dropped without further
		/// notice. Choosing a high value (1000) is okay; the trade-off that is described in
		/// [`InboundXcmpSuspended`] still applies at that scale.
		#[pallet::constant]
		type MaxInboundSuspended: Get<u32>;

		/// The origin that is allowed to resume or suspend the XCMP queue.
		type ControllerOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The conversion function used to attempt to convert an XCM `MultiLocation` origin to a
		/// superuser origin.
		type ControllerOriginConverter: ConvertOrigin<Self::RuntimeOrigin>;

		/// The price for delivering an XCM to a sibling parachain destination.
		type PriceForSiblingDelivery: PriceForParachainDelivery;

		/// The weight information of this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Suspends all XCM executions for the XCMP queue, regardless of the sender's origin.
		///
		/// - `origin`: Must pass `ControllerOrigin`.
		#[pallet::call_index(1)]
		#[pallet::weight((T::DbWeight::get().writes(1), DispatchClass::Operational,))]
		pub fn suspend_xcm_execution(origin: OriginFor<T>) -> DispatchResult {
			T::ControllerOrigin::ensure_origin(origin)?;

			QueueSuspended::<T>::try_mutate(|suspended| {
				if *suspended {
					Err(Error::<T>::AlreadySuspended.into())
				} else {
					*suspended = true;
					Ok(())
				}
			})
		}

		/// Resumes all XCM executions for the XCMP queue.
		///
		/// Note that this function doesn't change the status of the in/out bound channels.
		///
		/// - `origin`: Must pass `ControllerOrigin`.
		#[pallet::call_index(2)]
		#[pallet::weight((T::DbWeight::get().writes(1), DispatchClass::Operational,))]
		pub fn resume_xcm_execution(origin: OriginFor<T>) -> DispatchResult {
			T::ControllerOrigin::ensure_origin(origin)?;

			QueueSuspended::<T>::try_mutate(|suspended| {
				if !*suspended {
					Err(Error::<T>::AlreadyResumed.into())
				} else {
					*suspended = false;
					Ok(())
				}
			})
		}

		/// Overwrites the number of messages which must be in the queue for the other side to be
		/// told to suspend their sending.
		///
		/// - `origin`: Must pass `Root`.
		/// - `new`: Desired value for `QueueConfigData.suspend_value`
		#[pallet::call_index(3)]
		#[pallet::weight((T::WeightInfo::set_config_with_u32(), DispatchClass::Operational,))]
		pub fn update_suspend_threshold(origin: OriginFor<T>, new: u32) -> DispatchResult {
			ensure_root(origin)?;

			QueueConfig::<T>::try_mutate(|data| {
				data.suspend_threshold = new;
				data.validate::<T>()
			})
		}

		/// Overwrites the number of messages which must be in the queue after which we drop any
		/// further messages from the channel.
		///
		/// - `origin`: Must pass `Root`.
		/// - `new`: Desired value for `QueueConfigData.drop_threshold`
		#[pallet::call_index(4)]
		#[pallet::weight((T::WeightInfo::set_config_with_u32(),DispatchClass::Operational,))]
		pub fn update_drop_threshold(origin: OriginFor<T>, new: u32) -> DispatchResult {
			ensure_root(origin)?;

			QueueConfig::<T>::try_mutate(|data| {
				data.drop_threshold = new;
				data.validate::<T>()
			})
		}

		/// Overwrites the number of messages which the queue must be reduced to before it signals
		/// that message sending may recommence after it has been suspended.
		///
		/// - `origin`: Must pass `Root`.
		/// - `new`: Desired value for `QueueConfigData.resume_threshold`
		#[pallet::call_index(5)]
		#[pallet::weight((T::WeightInfo::set_config_with_u32(), DispatchClass::Operational,))]
		pub fn update_resume_threshold(origin: OriginFor<T>, new: u32) -> DispatchResult {
			ensure_root(origin)?;

			QueueConfig::<T>::try_mutate(|data| {
				data.resume_threshold = new;
				data.validate::<T>()
			})
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An HRMP message was sent to a sibling parachain.
		XcmpMessageSent { message_hash: XcmHash },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Setting the queue config failed since one of its values was invalid.
		BadQueueConfig,
		/// The execution is already suspended.
		AlreadySuspended,
		/// The execution is already resumed.
		AlreadyResumed,
	}

	/// The suspended inbound XCMP channels. All others are not suspended.
	///
	/// This is a `StorageValue` instead of a `StorageMap` since we expect multiple reads per block
	/// to different keys with a one byte payload. The access to `BoundedBTreeSet` will be cached
	/// within the block and therefore only included once in the proof size.
	///
	/// NOTE: The PoV benchmarking cannot know this and will over-estimate, but the actual proof
	/// will be smaller.
	#[pallet::storage]
	pub type InboundXcmpSuspended<T: Config> =
		StorageValue<_, BoundedBTreeSet<ParaId, T::MaxInboundSuspended>, ValueQuery>;

	/// Inbound aggregate XCMP messages. It can only be one per ParaId/block.
	#[pallet::storage]
	pub(super) type InboundXcmpMessages<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ParaId,
		Twox64Concat,
		RelayBlockNumber,
		Vec<u8>,
		ValueQuery,
	>;

	/// The non-empty XCMP channels in order of becoming non-empty, and the index of the first
	/// and last outbound message. If the two indices are equal, then it indicates an empty
	/// queue and there must be a non-`Ok` `OutboundStatus`. We assume queues grow no greater
	/// than 65535 items. Queue indices for normal messages begin at one; zero is reserved in
	/// case of the need to send a high-priority signal message this block.
	/// The bool is true if there is a signal message waiting to be sent.
	#[pallet::storage]
	pub(super) type OutboundXcmpStatus<T: Config> =
		StorageValue<_, Vec<OutboundChannelDetails>, ValueQuery>;

	// The new way of doing it:
	/// The messages outbound in a given XCMP channel.
	#[pallet::storage]
	pub(super) type OutboundXcmpMessages<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, ParaId, Twox64Concat, u16, Vec<u8>, ValueQuery>;

	/// Any signal messages waiting to be sent.
	#[pallet::storage]
	pub(super) type SignalMessages<T: Config> =
		StorageMap<_, Blake2_128Concat, ParaId, Vec<u8>, ValueQuery>;

	/// The configuration which controls the dynamics of the outbound queue.
	#[pallet::storage]
	pub(super) type QueueConfig<T: Config> = StorageValue<_, QueueConfigData, ValueQuery>;

	/// Whether or not the XCMP queue is suspended from executing incoming XCMs or not.
	#[pallet::storage]
	pub(super) type QueueSuspended<T: Config> = StorageValue<_, bool, ValueQuery>;
}

#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum OutboundState {
	Ok,
	Suspended,
}

/// Struct containing detailed information about the outbound channel.
#[derive(Clone, Eq, PartialEq, Encode, Decode, TypeInfo)]
pub struct OutboundChannelDetails {
	/// The `ParaId` of the parachain that this channel is connected with.
	recipient: ParaId,
	/// The state of the channel.
	state: OutboundState,
	/// Whether or not any signals exist in this channel.
	signals_exist: bool,
	/// The index of the first outbound message.
	first_index: u16,
	/// The index of the last outbound message.
	last_index: u16,
}

impl OutboundChannelDetails {
	pub fn new(recipient: ParaId) -> OutboundChannelDetails {
		OutboundChannelDetails {
			recipient,
			state: OutboundState::Ok,
			signals_exist: false,
			first_index: 0,
			last_index: 0,
		}
	}

	pub fn with_signals(mut self) -> OutboundChannelDetails {
		self.signals_exist = true;
		self
	}

	pub fn with_suspended_state(mut self) -> OutboundChannelDetails {
		self.state = OutboundState::Suspended;
		self
	}
}

#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct QueueConfigData {
	/// The number of messages which must be in the queue for the other side to be told to suspend
	/// their sending.
	suspend_threshold: u32,
	/// The number of messages which must be in the queue after which we drop any further messages
	/// from the channel. This should normally not happen since the `suspend_threshold` can be used
	/// to suspend the channel.
	drop_threshold: u32,
	/// The number of messages which the queue must be reduced to before it signals that
	/// message sending may recommence after it has been suspended.
	resume_threshold: u32,
	/// UNUSED - The amount of remaining weight under which we stop processing messages.
	#[deprecated(note = "Will be removed")]
	threshold_weight: Weight,
	/// UNUSED - The speed to which the available weight approaches the maximum weight. A lower
	/// number results in a faster progression. A value of 1 makes the entire weight available
	/// initially.
	#[deprecated(note = "Will be removed")]
	weight_restrict_decay: Weight,
	/// UNUSED - The maximum amount of weight any individual message may consume. Messages above
	/// this weight go into the overweight queue and may only be serviced explicitly.
	#[deprecated(note = "Will be removed")]
	xcmp_max_individual_weight: Weight,
}

impl Default for QueueConfigData {
	fn default() -> Self {
		#![allow(deprecated)]
		Self {
			suspend_threshold: 2048,
			drop_threshold: 3096,
			resume_threshold: 1024,
			threshold_weight: Weight::from_parts(100_000, 0),
			weight_restrict_decay: Weight::from_parts(2, 0),
			xcmp_max_individual_weight: Weight::from_parts(
				20u64 * WEIGHT_REF_TIME_PER_MILLIS,
				DEFAULT_POV_SIZE,
			),
		}
	}
}

impl QueueConfigData {
	/// Validate all assumptions about `Self`.
	///
	/// Should be called prior to accepting this as new config.
	pub fn validate<T: crate::Config>(&self) -> sp_runtime::DispatchResult {
		if self.resume_threshold < self.suspend_threshold &&
			self.suspend_threshold <= self.drop_threshold &&
			self.resume_threshold > 0
		{
			Ok(())
		} else {
			Err(Error::<T>::BadQueueConfig.into())
		}
	}
}

#[derive(PartialEq, Eq, Copy, Clone, Encode, Decode, TypeInfo)]
pub enum ChannelSignal {
	Suspend,
	Resume,
}

impl<T: Config> Pallet<T> {
	/// Place a message `fragment` on the outgoing XCMP queue for `recipient`.
	///
	/// Format is the type of aggregate message that the `fragment` may be safely encoded and
	/// appended onto. Whether earlier unused space is used for the fragment at the risk of sending
	/// it out of order is determined with `qos`. NOTE: For any two messages to be guaranteed to be
	/// dispatched in order, then both must be sent with `ServiceQuality::Ordered`.
	///
	/// ## Background
	///
	/// For our purposes, one HRMP "message" is actually an aggregated block of XCM "messages".
	///
	/// For the sake of clarity, we distinguish between them as message AGGREGATEs versus
	/// message FRAGMENTs.
	///
	/// So each AGGREGATE is comprised of one or more concatenated SCALE-encoded `Vec<u8>`
	/// FRAGMENTs. Though each fragment is already probably a SCALE-encoded Xcm, we can't be
	/// certain, so we SCALE encode each `Vec<u8>` fragment in order to ensure we have the
	/// length prefixed and can thus decode each fragment from the aggregate stream. With this,
	/// we can concatenate them into a single aggregate blob without needing to be concerned
	/// about encoding fragment boundaries.
	fn send_fragment<Fragment: Encode>(
		recipient: ParaId,
		format: XcmpMessageFormat,
		fragment: Fragment,
	) -> Result<u32, MessageSendError> {
		let data = fragment.encode();

		// Optimization note: `max_message_size` could potentially be stored in
		// `OutboundXcmpMessages` once known; that way it's only accessed when a new page is needed.

		let max_message_size =
			T::ChannelInfo::get_channel_max(recipient).ok_or(MessageSendError::NoChannel)?;
		if data.len() > max_message_size {
			return Err(MessageSendError::TooBig)
		}

		let mut s = <OutboundXcmpStatus<T>>::get();
		let details = if let Some(details) = s.iter_mut().find(|item| item.recipient == recipient) {
			details
		} else {
			s.push(OutboundChannelDetails::new(recipient));
			s.last_mut().expect("can't be empty; a new element was just pushed; qed")
		};
		let have_active = details.last_index > details.first_index;
		let appended = have_active &&
			<OutboundXcmpMessages<T>>::mutate(recipient, details.last_index - 1, |s| {
				if XcmpMessageFormat::decode_with_depth_limit(MAX_XCM_DECODE_DEPTH, &mut &s[..]) !=
					Ok(format)
				{
					return false
				}
				if s.len() + data.len() > max_message_size {
					return false
				}
				s.extend_from_slice(&data[..]);
				true
			});
		if appended {
			Ok((details.last_index - details.first_index - 1) as u32)
		} else {
			// Need to add a new page.
			let page_index = details.last_index;
			details.last_index += 1;
			let mut new_page = format.encode();
			new_page.extend_from_slice(&data[..]);
			<OutboundXcmpMessages<T>>::insert(recipient, page_index, new_page);
			let r = (details.last_index - details.first_index - 1) as u32;
			<OutboundXcmpStatus<T>>::put(s);
			Ok(r)
		}
	}

	/// Sends a signal to the `dest` chain over XCMP. This is guaranteed to be dispatched on this
	/// block.
	fn send_signal(dest: ParaId, signal: ChannelSignal) -> Result<(), ()> {
		let mut s = <OutboundXcmpStatus<T>>::get();
		if let Some(details) = s.iter_mut().find(|item| item.recipient == dest) {
			details.signals_exist = true;
		} else {
			s.push(OutboundChannelDetails::new(dest).with_signals());
		}
		<SignalMessages<T>>::mutate(dest, |page| {
			if page.is_empty() {
				*page = (XcmpMessageFormat::Signals, signal).encode();
			} else {
				signal.using_encoded(|s| page.extend_from_slice(s));
			}
		});
		<OutboundXcmpStatus<T>>::put(s);

		Ok(())
	}

	pub fn send_blob_message(recipient: ParaId, blob: Vec<u8>) -> Result<u32, MessageSendError> {
		Self::send_fragment(recipient, XcmpMessageFormat::ConcatenatedEncodedBlob, blob)
	}

	pub fn send_xcm_message(
		recipient: ParaId,
		xcm: VersionedXcm<()>,
	) -> Result<u32, MessageSendError> {
		Self::send_fragment(recipient, XcmpMessageFormat::ConcatenatedVersionedXcm, xcm)
	}

	fn suspend_channel(target: ParaId) {
		<OutboundXcmpStatus<T>>::mutate(|s| {
			if let Some(details) = s.iter_mut().find(|item| item.recipient == target) {
				let ok = details.state == OutboundState::Ok;
				debug_assert!(ok, "WARNING: Attempt to suspend channel that was not Ok.");
				details.state = OutboundState::Suspended;
			} else {
				s.push(OutboundChannelDetails::new(target).with_suspended_state());
			}
		});
	}

	fn resume_channel(target: ParaId) {
		<OutboundXcmpStatus<T>>::mutate(|s| {
			if let Some(index) = s.iter().position(|item| item.recipient == target) {
				let suspended = s[index].state == OutboundState::Suspended;
				defensive_assert!(
					suspended,
					"WARNING: Attempt to resume channel that was not suspended."
				);
				if s[index].first_index == s[index].last_index {
					s.remove(index);
				} else {
					s[index].state = OutboundState::Ok;
				}
			} else {
				defensive!("WARNING: Attempt to resume channel that was not suspended.");
			}
		});
	}

	fn enqueue_xcmp_messages(
		sender: ParaId,
		mut xcms: Vec<BoundedVec<u8, MaxXcmpMessageLenOf<T>>>,
		meter: &mut WeightMeter,
	) {
		if meter
			.try_consume(T::WeightInfo::enqueue_xcmp_messages(xcms.len() as u32))
			.is_err()
		{
			defensive!("Out of weight: cannot enqueue XCMP messages; dropping msgs: ", xcms.len());
			return
		}
		let QueueConfigData { drop_threshold, .. } = <QueueConfig<T>>::get();
		let fp = T::XcmpQueue::footprint(sender);

		let new_count = xcms.len().saturating_add(fp.count as usize);
		let to_enqueue = (drop_threshold as usize).saturating_sub(new_count) as usize;
		if to_enqueue < xcms.len() {
			// This should not happen since the channel should have been suspended in
			// [`on_queue_changed`].
			log::error!("XCMP queue for sibling {:?} is full; dropping messages.", sender,);
			xcms.truncate(to_enqueue);
		}

		T::XcmpQueue::enqueue_messages(xcms.iter().map(|xcm| xcm.as_bounded_slice()), sender);
	}

	/// Split concatenated encoded `VersionedXcm`s into individual items.
	///
	/// We directly encode them again since that is needed later on.
	fn split_concatenated_xcms(
		data: &mut &[u8],
		meter: &mut WeightMeter,
	) -> Result<Vec<BoundedVec<u8, MaxXcmpMessageLenOf<T>>>, ()> {
		let mut encoded_xcms = Vec::new();
		while !data.is_empty() {
			if meter.try_consume(T::WeightInfo::split_concatenated_xcm()).is_err() {
				defensive!("Could not decode all; dropping");
				return Err(())
			}

			let xcm =
				VersionedXcm::<T::RuntimeCall>::decode_with_depth_limit(MAX_XCM_DECODE_DEPTH, data)
					.map_err(|_| ())?;
			let bounded = xcm.encode().try_into().map_err(|_| ())?;
			encoded_xcms.push(bounded);
		}
		Ok(encoded_xcms)
	}
}

impl<T: Config> OnQueueChanged<ParaId> for Pallet<T> {
	// Suspends/Resumes the queue when certain thresholds are reached.
	fn on_queue_changed(para: ParaId, count: u64, _size: u64) {
		let QueueConfigData { resume_threshold, suspend_threshold, .. } = <QueueConfig<T>>::get();

		let mut suspended_channels = <InboundXcmpSuspended<T>>::get();
		let suspended = suspended_channels.contains(&para);

		if suspended && count <= resume_threshold as u64 {
			if let Err(err) = Self::send_signal(para, ChannelSignal::Resume) {
				log::error!("Cannot resume channel from sibling {:?}: {:?}", para, err);
			} else {
				suspended_channels.remove(&para);
				<InboundXcmpSuspended<T>>::put(suspended_channels);
			}
		} else if !suspended && count >= suspend_threshold as u64 {
			log::warn!("XCMP queue for sibling {:?} is full; suspending channel.", para);

			if let Err(err) = Self::send_signal(para, ChannelSignal::Suspend) {
				// This is an edge-case, but we will not regard the channel as `Suspended` without
				// confirmation. It will just re-try to suspend in the next block.
				log::error!("Cannot suspend channel from sibling {:?}: {:?}; further messages may be dropped.", para, err);
			} else if let Err(err) = suspended_channels.try_insert(para) {
				log::error!("Too many channels suspended; cannot suspend sibling {:?}: {:?}; further messages may be dropped.", para, err);
			} else {
				<InboundXcmpSuspended<T>>::put(suspended_channels);
			}
		}
	}
}

impl<T: Config> QueuePausedQuery<ParaId> for Pallet<T> {
	fn is_paused(para: &ParaId) -> bool {
		if !QueueSuspended::<T>::get() {
			return false
		}

		let sender_origin = T::ControllerOriginConverter::convert_origin(
			(Parent, Parachain((*para).into())),
			OriginKind::Superuser,
		);
		let is_controller =
			sender_origin.map_or(false, |origin| T::ControllerOrigin::try_origin(origin).is_ok());

		!is_controller
	}
}

impl<T: Config> XcmpMessageHandler for Pallet<T> {
	fn handle_xcmp_messages<'a, I: Iterator<Item = (ParaId, RelayBlockNumber, &'a [u8])>>(
		iter: I,
		max_weight: Weight,
	) -> Weight {
		let mut meter = WeightMeter::from_limit(max_weight);

		for (sender, _sent_at, mut data) in iter {
			let format = match XcmpMessageFormat::decode(&mut data) {
				Ok(f) => f,
				Err(_) => {
					defensive!("Unknown XCMP message format. Message silently dropped.");
					continue
				},
			};

			match format {
				XcmpMessageFormat::Signals =>
					while !data.is_empty() {
						if meter
							.try_consume(
								T::WeightInfo::suspend_channel()
									.max(T::WeightInfo::resume_channel()),
							)
							.is_err()
						{
							defensive!("Not enough weight to process signals - dropping.");
							break
						}

						match ChannelSignal::decode(&mut data) {
							Ok(ChannelSignal::Suspend) => Self::suspend_channel(sender),
							Ok(ChannelSignal::Resume) => Self::resume_channel(sender),
							Err(_) => {
								defensive!("Undecodable channel signal. Message silently dropped.");
								break
							},
						}
					},
				XcmpMessageFormat::ConcatenatedVersionedXcm => {
					match Self::split_concatenated_xcms(&mut data, &mut meter) {
						Ok(xcms) => {
							Self::enqueue_xcmp_messages(sender, xcms, &mut meter);
						},
						Err(()) => {
							defensive!(
								"Could not parse incoming XCMP messages. Used weight: ",
								meter.consumed_ratio()
							);
							continue
						},
					}
					if !data.is_empty() {
						defensive!("All XCM data must be consumed.");
					}
				},
				XcmpMessageFormat::ConcatenatedEncodedBlob => {
					defensive!("Blob messages not handled");
					continue
				},
			}
		}

		meter.consumed()
	}
}

impl<T: Config> XcmpMessageSource for Pallet<T> {
	fn take_outbound_messages(maximum_channels: usize) -> Vec<(ParaId, Vec<u8>)> {
		let mut statuses = <OutboundXcmpStatus<T>>::get();
		let old_statuses_len = statuses.len();
		let max_message_count = statuses.len().min(maximum_channels);
		let mut result = Vec::with_capacity(max_message_count);

		for status in statuses.iter_mut() {
			let OutboundChannelDetails {
				recipient: para_id,
				state: outbound_state,
				mut signals_exist,
				mut first_index,
				mut last_index,
			} = *status;

			if result.len() == max_message_count {
				// We check this condition in the beginning of the loop so that we don't include
				// a message where the limit is 0.
				break
			}
			if outbound_state == OutboundState::Suspended {
				continue
			}
			let (max_size_now, max_size_ever) = match T::ChannelInfo::get_channel_status(para_id) {
				ChannelStatus::Closed => {
					// This means that there is no such channel anymore. Nothing to be done but
					// swallow the messages and discard the status.
					for i in first_index..last_index {
						<OutboundXcmpMessages<T>>::remove(para_id, i);
					}
					if signals_exist {
						<SignalMessages<T>>::remove(para_id);
					}
					*status = OutboundChannelDetails::new(para_id);
					continue
				},
				ChannelStatus::Full => continue,
				ChannelStatus::Ready(n, e) => (n, e),
			};

			let page = if signals_exist {
				let page = <SignalMessages<T>>::get(para_id);
				if page.len() < max_size_now {
					<SignalMessages<T>>::remove(para_id);
					signals_exist = false;
					page
				} else {
					continue
				}
			} else if last_index > first_index {
				let page = <OutboundXcmpMessages<T>>::get(para_id, first_index);
				if page.len() < max_size_now {
					<OutboundXcmpMessages<T>>::remove(para_id, first_index);
					first_index += 1;
					page
				} else {
					continue
				}
			} else {
				continue
			};
			if first_index == last_index {
				first_index = 0;
				last_index = 0;
			}

			if page.len() > max_size_ever {
				// TODO: #274 This means that the channel's max message size has changed since
				//   the message was sent. We should parse it and split into smaller messages but
				//   since it's so unlikely then for now we just drop it.
				log::warn!("WARNING: oversize message in queue. silently dropping.");
			} else {
				result.push((para_id, page));
			}

			*status = OutboundChannelDetails {
				recipient: para_id,
				state: outbound_state,
				signals_exist,
				first_index,
				last_index,
			};
		}

		// Sort the outbound messages by ascending recipient para id to satisfy the acceptance
		// criteria requirement.
		result.sort_by_key(|m| m.0);

		// Prune hrmp channels that became empty. Additionally, because it may so happen that we
		// only gave attention to some channels in `non_empty_hrmp_channels` it's important to
		// change the order. Otherwise, the next `on_finalize` we will again give attention
		// only to those channels that happen to be in the beginning, until they are emptied.
		// This leads to "starvation" of the channels near to the end.
		//
		// To mitigate this we shift all processed elements towards the end of the vector using
		// `rotate_left`. To get intuition how it works see the examples in its rustdoc.
		statuses.retain(|x| {
			x.state == OutboundState::Suspended || x.signals_exist || x.first_index < x.last_index
		});

		// old_status_len must be >= status.len() since we never add anything to status.
		let pruned = old_statuses_len - statuses.len();
		// removing an item from status implies a message being sent, so the result messages must
		// be no less than the pruned channels.
		statuses.rotate_left(result.len() - pruned);

		<OutboundXcmpStatus<T>>::put(statuses);

		result
	}
}

/// Xcm sender for sending to a sibling parachain.
impl<T: Config> SendXcm for Pallet<T> {
	type Ticket = (ParaId, VersionedXcm<()>);

	fn validate(
		dest: &mut Option<MultiLocation>,
		msg: &mut Option<Xcm<()>>,
	) -> SendResult<(ParaId, VersionedXcm<()>)> {
		let d = dest.take().ok_or(SendError::MissingArgument)?;

		match &d {
			// An HRMP message for a sibling parachain.
			MultiLocation { parents: 1, interior: X1(Parachain(id)) } => {
				let xcm = msg.take().ok_or(SendError::MissingArgument)?;
				let id = ParaId::from(*id);
				let price = T::PriceForSiblingDelivery::price_for_parachain_delivery(id, &xcm);
				let versioned_xcm = T::VersionWrapper::wrap_version(&d, xcm)
					.map_err(|()| SendError::DestinationUnsupported)?;
				Ok(((id, versioned_xcm), price))
			},
			_ => {
				// Anything else is unhandled. This includes a message that is not meant for us.
				// We need to make sure that dest/msg is not consumed here.
				*dest = Some(d);
				Err(SendError::NotApplicable)
			},
		}
	}

	fn deliver((id, xcm): (ParaId, VersionedXcm<()>)) -> Result<XcmHash, SendError> {
		let hash = xcm.using_encoded(sp_io::hashing::blake2_256);

		match Self::send_fragment(id, XcmpMessageFormat::ConcatenatedVersionedXcm, xcm) {
			Ok(_) => {
				Self::deposit_event(Event::XcmpMessageSent { message_hash: hash });
				Ok(hash)
			},
			Err(e) => Err(SendError::Transport(<&'static str>::from(e))),
		}
	}
}
