// Copyright (C) Parity Technologies (UK) Ltd.
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

use codec::{Decode, DecodeLimit, Encode};
use cumulus_primitives_core::{
	relay_chain::BlockNumber as RelayBlockNumber, ChannelStatus, GetChannelInfo, MessageSendError,
	ParaId, XcmpMessageFormat, XcmpMessageHandler, XcmpMessageSource,
};
use frame_support::{
	traits::{EnsureOrigin, Get},
	weights::{constants::WEIGHT_REF_TIME_PER_MILLIS, Weight},
};
use polkadot_runtime_common::xcm_sender::PriceForParachainDelivery;
use rand_chacha::{
	rand_core::{RngCore, SeedableRng},
	ChaChaRng,
};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_std::{convert::TryFrom, prelude::*};
use xcm::{latest::prelude::*, VersionedXcm, WrapVersion, MAX_XCM_DECODE_DEPTH};
use xcm_executor::traits::ConvertOrigin;

pub use pallet::*;

/// Index used to identify overweight XCMs.
pub type OverweightIndex = u64;

const LOG_TARGET: &str = "xcmp_queue";
const DEFAULT_POV_SIZE: u64 = 64 * 1024; // 64 KB

// Maximum amount of messages to process per block. This is a temporary measure until we properly
// account for proof size weights.
const MAX_MESSAGES_PER_BLOCK: u8 = 10;
// Maximum amount of messages that can exist in the overweight queue at any given time.
const MAX_OVERWEIGHT_MESSAGES: u32 = 1000;

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

		/// Something to execute an XCM message. We need this to service the XCMoXCMP queue.
		type XcmExecutor: ExecuteXcm<Self::RuntimeCall>;

		/// Information on the avaialble XCMP channels.
		type ChannelInfo: GetChannelInfo;

		/// Means of converting an `Xcm` into a `VersionedXcm`.
		type VersionWrapper: WrapVersion;

		/// The origin that is allowed to execute overweight messages.
		type ExecuteOverweightOrigin: EnsureOrigin<Self::RuntimeOrigin>;

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

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_idle(_now: BlockNumberFor<T>, max_weight: Weight) -> Weight {
			// on_idle processes additional messages with any remaining block weight.
			Self::service_xcmp_queue(max_weight)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Services a single overweight XCM.
		///
		/// - `origin`: Must pass `ExecuteOverweightOrigin`.
		/// - `index`: The index of the overweight XCM to service
		/// - `weight_limit`: The amount of weight that XCM execution may take.
		///
		/// Errors:
		/// - `BadOverweightIndex`: XCM under `index` is not found in the `Overweight` storage map.
		/// - `BadXcm`: XCM under `index` cannot be properly decoded into a valid XCM format.
		/// - `WeightOverLimit`: XCM execution may use greater `weight_limit`.
		///
		/// Events:
		/// - `OverweightServiced`: On success.
		#[pallet::call_index(0)]
		#[pallet::weight((weight_limit.saturating_add(Weight::from_parts(1_000_000, 0)), DispatchClass::Operational))]
		pub fn service_overweight(
			origin: OriginFor<T>,
			index: OverweightIndex,
			weight_limit: Weight,
		) -> DispatchResultWithPostInfo {
			T::ExecuteOverweightOrigin::ensure_origin(origin)?;

			let (sender, sent_at, data) =
				Overweight::<T>::get(index).ok_or(Error::<T>::BadOverweightIndex)?;
			let xcm = VersionedXcm::<T::RuntimeCall>::decode_all_with_depth_limit(
				MAX_XCM_DECODE_DEPTH,
				&mut data.as_slice(),
			)
			.map_err(|_| Error::<T>::BadXcm)?;
			let used = Self::handle_xcm_message(sender, sent_at, xcm, weight_limit)
				.map_err(|_| Error::<T>::WeightOverLimit)?;
			Overweight::<T>::remove(index);
			Self::deposit_event(Event::OverweightServiced { index, used });
			Ok(Some(used.saturating_add(Weight::from_parts(1_000_000, 0))).into())
		}

		/// Suspends all XCM executions for the XCMP queue, regardless of the sender's origin.
		///
		/// - `origin`: Must pass `ControllerOrigin`.
		#[pallet::call_index(1)]
		#[pallet::weight((T::DbWeight::get().writes(1), DispatchClass::Operational,))]
		pub fn suspend_xcm_execution(origin: OriginFor<T>) -> DispatchResult {
			T::ControllerOrigin::ensure_origin(origin)?;

			QueueSuspended::<T>::put(true);

			Ok(())
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

			QueueSuspended::<T>::put(false);

			Ok(())
		}

		/// Overwrites the number of pages of messages which must be in the queue for the other side
		/// to be told to suspend their sending.
		///
		/// - `origin`: Must pass `Root`.
		/// - `new`: Desired value for `QueueConfigData.suspend_value`
		#[pallet::call_index(3)]
		#[pallet::weight((T::WeightInfo::set_config_with_u32(), DispatchClass::Operational,))]
		pub fn update_suspend_threshold(origin: OriginFor<T>, new: u32) -> DispatchResult {
			ensure_root(origin)?;
			QueueConfig::<T>::mutate(|data| data.suspend_threshold = new);

			Ok(())
		}

		/// Overwrites the number of pages of messages which must be in the queue after which we
		/// drop any further messages from the channel.
		///
		/// - `origin`: Must pass `Root`.
		/// - `new`: Desired value for `QueueConfigData.drop_threshold`
		#[pallet::call_index(4)]
		#[pallet::weight((T::WeightInfo::set_config_with_u32(),DispatchClass::Operational,))]
		pub fn update_drop_threshold(origin: OriginFor<T>, new: u32) -> DispatchResult {
			ensure_root(origin)?;
			QueueConfig::<T>::mutate(|data| data.drop_threshold = new);

			Ok(())
		}

		/// Overwrites the number of pages of messages which the queue must be reduced to before it
		/// signals that message sending may recommence after it has been suspended.
		///
		/// - `origin`: Must pass `Root`.
		/// - `new`: Desired value for `QueueConfigData.resume_threshold`
		#[pallet::call_index(5)]
		#[pallet::weight((T::WeightInfo::set_config_with_u32(), DispatchClass::Operational,))]
		pub fn update_resume_threshold(origin: OriginFor<T>, new: u32) -> DispatchResult {
			ensure_root(origin)?;
			QueueConfig::<T>::mutate(|data| data.resume_threshold = new);

			Ok(())
		}

		/// Overwrites the amount of remaining weight under which we stop processing messages.
		///
		/// - `origin`: Must pass `Root`.
		/// - `new`: Desired value for `QueueConfigData.threshold_weight`
		#[pallet::call_index(6)]
		#[pallet::weight((T::WeightInfo::set_config_with_weight(), DispatchClass::Operational,))]
		pub fn update_threshold_weight(origin: OriginFor<T>, new: Weight) -> DispatchResult {
			ensure_root(origin)?;
			QueueConfig::<T>::mutate(|data| data.threshold_weight = new);

			Ok(())
		}

		/// Overwrites the speed to which the available weight approaches the maximum weight.
		/// A lower number results in a faster progression. A value of 1 makes the entire weight
		/// available initially.
		///
		/// - `origin`: Must pass `Root`.
		/// - `new`: Desired value for `QueueConfigData.weight_restrict_decay`.
		#[pallet::call_index(7)]
		#[pallet::weight((T::WeightInfo::set_config_with_weight(), DispatchClass::Operational,))]
		pub fn update_weight_restrict_decay(origin: OriginFor<T>, new: Weight) -> DispatchResult {
			ensure_root(origin)?;
			QueueConfig::<T>::mutate(|data| data.weight_restrict_decay = new);

			Ok(())
		}

		/// Overwrite the maximum amount of weight any individual message may consume.
		/// Messages above this weight go into the overweight queue and may only be serviced
		/// explicitly.
		///
		/// - `origin`: Must pass `Root`.
		/// - `new`: Desired value for `QueueConfigData.xcmp_max_individual_weight`.
		#[pallet::call_index(8)]
		#[pallet::weight((T::WeightInfo::set_config_with_weight(), DispatchClass::Operational,))]
		pub fn update_xcmp_max_individual_weight(
			origin: OriginFor<T>,
			new: Weight,
		) -> DispatchResult {
			ensure_root(origin)?;
			QueueConfig::<T>::mutate(|data| data.xcmp_max_individual_weight = new);

			Ok(())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Some XCM was executed ok.
		Success { message_hash: XcmHash, message_id: XcmHash, weight: Weight },
		/// Some XCM failed.
		Fail { message_hash: XcmHash, message_id: XcmHash, error: XcmError, weight: Weight },
		/// Bad XCM version used.
		BadVersion { message_hash: XcmHash },
		/// Bad XCM format used.
		BadFormat { message_hash: XcmHash },
		/// An HRMP message was sent to a sibling parachain.
		XcmpMessageSent { message_hash: XcmHash },
		/// An XCM exceeded the individual message weight budget.
		OverweightEnqueued {
			sender: ParaId,
			sent_at: RelayBlockNumber,
			index: OverweightIndex,
			required: Weight,
		},
		/// An XCM from the overweight queue was executed with the given actual weight used.
		OverweightServiced { index: OverweightIndex, used: Weight },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Failed to send XCM message.
		FailedToSend,
		/// Bad XCM origin.
		BadXcmOrigin,
		/// Bad XCM data.
		BadXcm,
		/// Bad overweight index.
		BadOverweightIndex,
		/// Provided weight is possibly not enough to execute the message.
		WeightOverLimit,
	}

	/// Status of the inbound XCMP channels.
	#[pallet::storage]
	pub(super) type InboundXcmpStatus<T: Config> =
		StorageValue<_, Vec<InboundChannelDetails>, ValueQuery>;

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

	/// The messages that exceeded max individual message weight budget.
	///
	/// These message stay in this storage map until they are manually dispatched via
	/// `service_overweight`.
	#[pallet::storage]
	pub(super) type Overweight<T: Config> =
		CountedStorageMap<_, Twox64Concat, OverweightIndex, (ParaId, RelayBlockNumber, Vec<u8>)>;

	/// The number of overweight messages ever recorded in `Overweight`. Also doubles as the next
	/// available free overweight index.
	#[pallet::storage]
	pub(super) type OverweightCount<T: Config> = StorageValue<_, OverweightIndex, ValueQuery>;

	/// Whether or not the XCMP queue is suspended from executing incoming XCMs or not.
	#[pallet::storage]
	pub(super) type QueueSuspended<T: Config> = StorageValue<_, bool, ValueQuery>;
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum InboundState {
	Ok,
	Suspended,
}

#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum OutboundState {
	Ok,
	Suspended,
}

/// Struct containing detailed information about the inbound channel.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Encode, Decode, TypeInfo)]
pub struct InboundChannelDetails {
	/// The `ParaId` of the parachain that this channel is connected with.
	sender: ParaId,
	/// The state of the channel.
	state: InboundState,
	/// The ordered metadata of each inbound message.
	///
	/// Contains info about the relay block number that the message was sent at, and the format
	/// of the incoming message.
	message_metadata: Vec<(RelayBlockNumber, XcmpMessageFormat)>,
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
	/// The number of pages of messages which must be in the queue for the other side to be told to
	/// suspend their sending.
	suspend_threshold: u32,
	/// The number of pages of messages which must be in the queue after which we drop any further
	/// messages from the channel.
	drop_threshold: u32,
	/// The number of pages of messages which the queue must be reduced to before it signals that
	/// message sending may recommence after it has been suspended.
	resume_threshold: u32,
	/// The amount of remaining weight under which we stop processing messages.
	threshold_weight: Weight,
	/// The speed to which the available weight approaches the maximum weight. A lower number
	/// results in a faster progression. A value of 1 makes the entire weight available initially.
	weight_restrict_decay: Weight,
	/// The maximum amount of weight any individual message may consume. Messages above this weight
	/// go into the overweight queue and may only be serviced explicitly.
	xcmp_max_individual_weight: Weight,
}

impl Default for QueueConfigData {
	fn default() -> Self {
		Self {
			suspend_threshold: 2,
			drop_threshold: 5,
			resume_threshold: 1,
			threshold_weight: Weight::from_parts(100_000, 0),
			weight_restrict_decay: Weight::from_parts(2, 0),
			xcmp_max_individual_weight: Weight::from_parts(
				20u64 * WEIGHT_REF_TIME_PER_MILLIS,
				DEFAULT_POV_SIZE,
			),
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

	fn create_shuffle(len: usize) -> Vec<usize> {
		// Create a shuffled order for use to iterate through.
		// Not a great random seed, but good enough for our purposes.
		let seed = frame_system::Pallet::<T>::parent_hash();
		let seed =
			<[u8; 32]>::decode(&mut sp_runtime::traits::TrailingZeroInput::new(seed.as_ref()))
				.expect("input is padded with zeroes; qed");
		let mut rng = ChaChaRng::from_seed(seed);
		let mut shuffled = (0..len).collect::<Vec<_>>();
		for i in 0..len {
			let j = (rng.next_u32() as usize) % len;
			shuffled.as_mut_slice().swap(i, j);
		}
		shuffled
	}

	fn handle_blob_message(
		_sender: ParaId,
		_sent_at: RelayBlockNumber,
		_blob: Vec<u8>,
		_weight_limit: Weight,
	) -> Result<Weight, bool> {
		debug_assert!(false, "Blob messages not handled.");
		Err(false)
	}

	fn handle_xcm_message(
		sender: ParaId,
		_sent_at: RelayBlockNumber,
		xcm: VersionedXcm<T::RuntimeCall>,
		max_weight: Weight,
	) -> Result<Weight, XcmError> {
		let message_hash = xcm.using_encoded(sp_io::hashing::blake2_256);
		log::debug!("Processing XCMP-XCM: {:?}", &message_hash);
		let (result, event) = match Xcm::<T::RuntimeCall>::try_from(xcm) {
			Ok(xcm) => {
				let location = (Parent, Parachain(sender.into()));
				let mut message_id = message_hash;

				match T::XcmExecutor::prepare_and_execute(
					location,
					xcm,
					&mut message_id,
					max_weight,
					Weight::zero(),
				) {
					Outcome::Error(error) => (
						Err(error),
						Event::Fail { message_hash, message_id, error, weight: Weight::zero() },
					),
					Outcome::Complete(weight) =>
						(Ok(weight), Event::Success { message_hash, message_id, weight }),
					// As far as the caller is concerned, this was dispatched without error, so
					// we just report the weight used.
					Outcome::Incomplete(weight, error) =>
						(Ok(weight), Event::Fail { message_hash, message_id, error, weight }),
				}
			},
			Err(()) => (Err(XcmError::UnhandledXcmVersion), Event::BadVersion { message_hash }),
		};
		Self::deposit_event(event);
		result
	}

	fn process_xcmp_message(
		sender: ParaId,
		(sent_at, format): (RelayBlockNumber, XcmpMessageFormat),
		messages_processed: &mut u8,
		max_weight: Weight,
		max_individual_weight: Weight,
	) -> (Weight, bool) {
		let data = <InboundXcmpMessages<T>>::get(sender, sent_at);
		let mut last_remaining_fragments;
		let mut remaining_fragments = &data[..];
		let mut weight_used = Weight::zero();
		match format {
			XcmpMessageFormat::ConcatenatedVersionedXcm => {
				while !remaining_fragments.is_empty() &&
					*messages_processed < MAX_MESSAGES_PER_BLOCK
				{
					last_remaining_fragments = remaining_fragments;
					if let Ok(xcm) = VersionedXcm::<T::RuntimeCall>::decode_with_depth_limit(
						MAX_XCM_DECODE_DEPTH,
						&mut remaining_fragments,
					) {
						let weight = max_weight - weight_used;
						*messages_processed += 1;
						match Self::handle_xcm_message(sender, sent_at, xcm, weight) {
							Ok(used) => weight_used = weight_used.saturating_add(used),
							Err(XcmError::WeightLimitReached(required))
								if required.any_gt(max_individual_weight) =>
							{
								let is_under_limit =
									Overweight::<T>::count() < MAX_OVERWEIGHT_MESSAGES;
								weight_used.saturating_accrue(T::DbWeight::get().reads(1));
								if is_under_limit {
									// overweight - add to overweight queue and continue with
									// message execution consuming the message.
									let msg_len = last_remaining_fragments
										.len()
										.saturating_sub(remaining_fragments.len());
									let overweight_xcm =
										last_remaining_fragments[..msg_len].to_vec();
									let index =
										Self::stash_overweight(sender, sent_at, overweight_xcm);
									let e = Event::OverweightEnqueued {
										sender,
										sent_at,
										index,
										required,
									};
									Self::deposit_event(e);
								}
							},
							Err(XcmError::WeightLimitReached(required))
								if required.all_lte(max_weight) =>
							{
								// That message didn't get processed this time because of being
								// too heavy. We leave it around for next time and bail.
								remaining_fragments = last_remaining_fragments;
								break
							},
							Err(error) => {
								log::error!(
									"Failed to process XCMP-XCM message, caused by {:?}",
									error
								);
								// Message looks invalid; don't attempt to retry
							},
						}
					} else {
						debug_assert!(false, "Invalid incoming XCMP message data");
						remaining_fragments = &b""[..];
					}
				}
			},
			XcmpMessageFormat::ConcatenatedEncodedBlob => {
				while !remaining_fragments.is_empty() {
					last_remaining_fragments = remaining_fragments;

					if let Ok(blob) = <Vec<u8>>::decode(&mut remaining_fragments) {
						let weight = max_weight - weight_used;
						*messages_processed += 1;
						match Self::handle_blob_message(sender, sent_at, blob, weight) {
							Ok(used) => weight_used = weight_used.saturating_add(used),
							Err(true) => {
								// That message didn't get processed this time because of being
								// too heavy. We leave it around for next time and bail.
								remaining_fragments = last_remaining_fragments;
								break
							},
							Err(false) => {
								// Message invalid; don't attempt to retry
							},
						}
					} else {
						debug_assert!(false, "Invalid incoming blob message data");
						remaining_fragments = &b""[..];
					}
				}
			},
			XcmpMessageFormat::Signals => {
				debug_assert!(false, "All signals are handled immediately; qed");
				remaining_fragments = &b""[..];
			},
		}
		let is_empty = remaining_fragments.is_empty();
		if is_empty {
			<InboundXcmpMessages<T>>::remove(sender, sent_at);
		} else {
			<InboundXcmpMessages<T>>::insert(sender, sent_at, remaining_fragments);
		}
		(weight_used, is_empty)
	}

	/// Puts a given XCM into the list of overweight messages, allowing it to be executed later.
	fn stash_overweight(
		sender: ParaId,
		sent_at: RelayBlockNumber,
		xcm: Vec<u8>,
	) -> OverweightIndex {
		let index = OverweightCount::<T>::mutate(|count| {
			let index = *count;
			*count += 1;
			index
		});

		Overweight::<T>::insert(index, (sender, sent_at, xcm));
		index
	}

	/// Service the incoming XCMP message queue attempting to execute up to `max_weight` execution
	/// weight of messages.
	///
	/// Channels are first shuffled and then processed in this random one page at a time, order over
	/// and over until either `max_weight` is exhausted or no channel has messages that can be
	/// processed any more.
	///
	/// There are two obvious "modes" that we could apportion `max_weight`: one would be to attempt
	/// to spend it all on the first channel's first page, then use the leftover (if any) for the
	/// second channel's first page and so on until finally we cycle back and the process messages
	/// on the first channel's second page &c. The other mode would be to apportion only `1/N` of
	/// `max_weight` for the first page (where `N` could be, perhaps, the number of channels to
	/// service, using the remainder plus the next `1/N` for the next channel's page &c.
	///
	/// Both modes have good qualities, the first ensures that a channel with a large message (over
	/// `1/N` does not get indefinitely blocked if other channels have continuous, light traffic.
	/// The second is fairer, and ensures that channels with continuous light messages don't suffer
	/// high latency.
	///
	/// The following code is a hybrid solution; we have a concept of `weight_available` which
	/// incrementally approaches `max_weight` as more channels are attempted to be processed. We use
	/// the parameter `weight_restrict_decay` to control the speed with which `weight_available`
	/// approaches `max_weight`, with `0` being strictly equivalent to the first aforementioned
	/// mode, and `N` approximating the second. A reasonable parameter may be `1`, which makes
	/// half of the `max_weight` available for the first page, then a quarter plus the remainder
	/// for the second &c. though empirical and or practical factors may give rise to adjusting it
	/// further.
	fn service_xcmp_queue(max_weight: Weight) -> Weight {
		let suspended = QueueSuspended::<T>::get();
		let mut messages_processed = 0;

		let mut status = <InboundXcmpStatus<T>>::get(); // <- sorted.
		if status.is_empty() {
			return Weight::zero()
		}

		let QueueConfigData {
			resume_threshold,
			threshold_weight,
			weight_restrict_decay,
			xcmp_max_individual_weight,
			..
		} = <QueueConfig<T>>::get();

		let mut shuffled = Self::create_shuffle(status.len());
		let mut weight_used = Weight::zero();
		let mut weight_available = Weight::zero();

		// We don't want the possibility of a chain sending a series of really heavy messages and
		// tying up the block's execution time from other chains. Therefore we execute any remaining
		// messages in a random order.
		// Order within a single channel will always be preserved, however this does mean that
		// relative order between channels may not. The result is that chains which tend to send
		// fewer, lighter messages will generally have a lower latency than chains which tend to
		// send more, heavier messages.

		let mut shuffle_index = 0;
		while shuffle_index < shuffled.len() &&
			max_weight.saturating_sub(weight_used).all_gte(threshold_weight) &&
			messages_processed < MAX_MESSAGES_PER_BLOCK
		{
			let index = shuffled[shuffle_index];
			let sender = status[index].sender;
			let sender_origin = T::ControllerOriginConverter::convert_origin(
				(Parent, Parachain(sender.into())),
				OriginKind::Superuser,
			);
			let is_controller = sender_origin
				.map_or(false, |origin| T::ControllerOrigin::try_origin(origin).is_ok());

			if suspended && !is_controller {
				shuffle_index += 1;
				continue
			}

			if weight_available != max_weight {
				// Get incrementally closer to freeing up max_weight for message execution over the
				// first round. For the second round we unlock all weight. If we come close enough
				// on the first round to unlocking everything, then we do so.
				if shuffle_index < status.len() {
					weight_available +=
						(max_weight - weight_available) / (weight_restrict_decay.ref_time() + 1);
					if (weight_available + threshold_weight).any_gt(max_weight) {
						weight_available = max_weight;
					}
				} else {
					weight_available = max_weight;
				}
			}

			let weight_processed = if status[index].message_metadata.is_empty() {
				debug_assert!(false, "channel exists in status; there must be messages; qed");
				Weight::zero()
			} else {
				// Process up to one block's worth for now.
				let weight_remaining = weight_available.saturating_sub(weight_used);
				let (weight_processed, is_empty) = Self::process_xcmp_message(
					sender,
					status[index].message_metadata[0],
					&mut messages_processed,
					weight_remaining,
					xcmp_max_individual_weight,
				);
				if is_empty {
					status[index].message_metadata.remove(0);
				}
				weight_processed
			};
			weight_used += weight_processed;

			if status[index].message_metadata.len() as u32 <= resume_threshold &&
				status[index].state == InboundState::Suspended
			{
				// Resume
				let r = Self::send_signal(sender, ChannelSignal::Resume);
				debug_assert!(r.is_ok(), "WARNING: Failed sending resume into suspended channel");
				status[index].state = InboundState::Ok;
			}

			// If there are more and we're making progress, we process them after we've given the
			// other channels a look in. If we've still not unlocked all weight, then we set them
			// up for processing a second time anyway.
			if !status[index].message_metadata.is_empty() &&
				(weight_processed.any_gt(Weight::zero()) || weight_available != max_weight)
			{
				if shuffle_index + 1 == shuffled.len() {
					// Only this queue left. Just run around this loop once more.
					continue
				}
				shuffled.push(index);
			}
			shuffle_index += 1;
		}

		// Only retain the senders that have non-empty queues.
		status.retain(|item| !item.message_metadata.is_empty());

		<InboundXcmpStatus<T>>::put(status);
		weight_used
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
				debug_assert!(
					suspended,
					"WARNING: Attempt to resume channel that was not suspended."
				);
				if s[index].first_index == s[index].last_index {
					s.remove(index);
				} else {
					s[index].state = OutboundState::Ok;
				}
			} else {
				debug_assert!(false, "WARNING: Attempt to resume channel that was not suspended.");
			}
		});
	}
}

impl<T: Config> XcmpMessageHandler for Pallet<T> {
	fn handle_xcmp_messages<'a, I: Iterator<Item = (ParaId, RelayBlockNumber, &'a [u8])>>(
		iter: I,
		max_weight: Weight,
	) -> Weight {
		let mut status = <InboundXcmpStatus<T>>::get();

		let QueueConfigData { suspend_threshold, drop_threshold, .. } = <QueueConfig<T>>::get();

		for (sender, sent_at, data) in iter {
			// Figure out the message format.
			let mut data_ref = data;
			let format = match XcmpMessageFormat::decode_with_depth_limit(
				MAX_XCM_DECODE_DEPTH,
				&mut data_ref,
			) {
				Ok(f) => f,
				Err(_) => {
					debug_assert!(false, "Unknown XCMP message format. Silently dropping message");
					continue
				},
			};
			if format == XcmpMessageFormat::Signals {
				while !data_ref.is_empty() {
					use ChannelSignal::*;
					match ChannelSignal::decode(&mut data_ref) {
						Ok(Suspend) => Self::suspend_channel(sender),
						Ok(Resume) => Self::resume_channel(sender),
						Err(_) => break,
					}
				}
			} else {
				// Record the fact we received it.
				match status.binary_search_by_key(&sender, |item| item.sender) {
					Ok(i) => {
						let count = status[i].message_metadata.len();
						if count as u32 >= suspend_threshold && status[i].state == InboundState::Ok
						{
							status[i].state = InboundState::Suspended;
							let r = Self::send_signal(sender, ChannelSignal::Suspend);
							if r.is_err() {
								log::warn!(
									"Attempt to suspend channel failed. Messages may be dropped."
								);
							}
						}
						if (count as u32) < drop_threshold {
							status[i].message_metadata.push((sent_at, format));
						} else {
							debug_assert!(
								false,
								"XCMP channel queue full. Silently dropping message"
							);
						}
					},
					Err(_) => status.push(InboundChannelDetails {
						sender,
						state: InboundState::Ok,
						message_metadata: vec![(sent_at, format)],
					}),
				}
				// Queue the payload for later execution.
				<InboundXcmpMessages<T>>::insert(sender, sent_at, data_ref);
			}

			// Optimization note; it would make sense to execute messages immediately if
			// `status.is_empty()` here.
		}
		status.sort();
		<InboundXcmpStatus<T>>::put(status);

		Self::service_xcmp_queue(max_weight)
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
				//   the message was sent. We should parse it and split into smaller mesasges but
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
