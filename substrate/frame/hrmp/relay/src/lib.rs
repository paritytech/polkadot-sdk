// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! # Relay-chain HRMP deposit pallet
//!
//! Forward the HRMP channel deposit operations between the relay chain HRMP pallet and the
//! parachain handling deposits.
//! Implements [`ChannelDepositProvider`] and [`OnDepositSet`] for the relay chain HRMP pallet.
//! Forward the deposit operations with [`SendToPara`] following the protocol defined in
//! hrmp-primitives and get results back through [`Call::receive`].
//!
//! [`Call::force_answer`] is called by governance to fix potential inconsistent state.
//!
//! The requests are sent without payment, to avoid payment on the relay chain, but it must be
//! limited to avoid filling up the DMP/UMP capacities. So a parachain may only make
//! [`Config::MaxRequestsPerWindow`] of them per [`Config::RequestWindow`], unless it is a system
//! parachain. A window that is over is removed in `on_idle`, which looks at one per block.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame_support::{
	pallet_prelude::*,
	weights::{Weight, WeightMeter},
};
use frame_system::pallet_prelude::BlockNumberFor;
use hrmp_primitives::{
	ChannelId, DepositError as WireDepositError, DepositSide, MessageToPara, MessageToParaV1,
	MessageToRelay, MessageToRelayV1,
};
use polkadot_parachain_primitives::primitives::IsSystem;
use polkadot_primitives::{HrmpChannelId, Id as ParaId};
use polkadot_runtime_parachains::{
	ensure_parachain,
	hrmp::deposits::{
		ChannelDepositProvider, DepositAction, DepositChangeRequest, DepositChangeResponse,
		DepositError, OnDepositSet,
	},
	Origin as ParachainOrigin,
};
use sp_runtime::traits::Saturating;

pub use pallet::*;
pub use weights::WeightInfo;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

/// The index of [`Call::receive`] that receives [`MessageToPara`].
pub const RECEIVE_CALL_INDEX: u8 = 0;

const LOG_TARGET: &str = "runtime::hrmp-relay";

/// Used to send an XCM `Transact` to `pallet-hrmp-para` on the chain that holds the deposits.
pub trait SendToPara {
	/// Send `message` to the parachain.
	#[allow(clippy::result_unit_err)]
	fn send(message: MessageToPara) -> Result<(), ()>;
}

/// A [`MessageToParaV1::Release`] the transport refused, waiting in [`FailedReleases`] to be
/// retried.
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct ReleaseMessageToPara {
	/// The channel whose deposits are given back.
	pub channel: ChannelId,
	/// Whether the sender's deposit is given back.
	pub sender: bool,
	/// Whether the recipient's deposit is given back.
	pub recipient: bool,
}

impl From<ReleaseMessageToPara> for MessageToPara {
	fn from(release: ReleaseMessageToPara) -> Self {
		let ReleaseMessageToPara { channel, sender, recipient } = release;
		MessageToPara::V1(MessageToParaV1::Release { channel, sender, recipient })
	}
}

#[cfg(feature = "std")]
impl SendToPara for () {
	fn send(_message: MessageToPara) -> Result<(), ()> {
		Ok(())
	}
}

/// Prepares the runtime for this pallet's benchmarks.
#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper {
	/// Makes [`Config::SendToPara`] take messages.
	fn ensure_send_to_para();
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkHelper for () {
	fn ensure_send_to_para() {}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config:
		frame_system::Config<
		RuntimeOrigin: Into<Result<ParachainOrigin, <Self as frame_system::Config>::RuntimeOrigin>>,
	>
	{
		/// Receives the outcome of each pending request: the `hrmp` pallet.
		type OnDepositSet: OnDepositSet;

		/// The para that holds the deposits.
		///
		/// Only it may answer requests.
		#[pallet::constant]
		type HolderParaId: Get<ParaId>;

		/// Sends messages to the parachain that holds the deposits.
		type SendToPara: SendToPara;

		/// How many releases may wait in [`FailedReleases`] to be retried.
		#[pallet::constant]
		type MaxFailedReleases: Get<u32>;

		/// How many requests a non-system parachain may make in one [`Config::RequestWindow`].
		///
		/// Releases are not counted: they are never refused. Neither are the requests of a
		/// system parachain.
		#[pallet::constant]
		type MaxRequestsPerWindow: Get<u32>;

		/// The length, in blocks, of the window over which a non-system parachain's requests are
		/// counted.
		///
		/// A parachain's first window starts at its first request. Once a window is over, the
		/// next one starts at the parachain's next request.
		#[pallet::constant]
		type RequestWindow: Get<BlockNumberFor<Self>>;

		/// Weight information for the extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// Prepares the runtime for the benchmarks.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: BenchmarkHelper;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// The id of the next [`MessageToParaV1::SetDeposits`].
	#[pallet::storage]
	pub type NextRequestId<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// The requests waiting for their answer, handed back unchanged once it comes.
	#[pallet::storage]
	pub type PendingRequests<T: Config> =
		StorageMap<_, Twox64Concat, u64, DepositChangeRequest, OptionQuery>;

	/// The pending request of each channel side, at most one.
	#[pallet::storage]
	pub type PendingSides<T: Config> =
		StorageMap<_, Twox64Concat, (ChannelId, DepositSide), u64, OptionQuery>;

	/// For each non-system parachain, the block at which its current request window started and
	/// the number of requests it has made since then. See [`Config::MaxRequestsPerWindow`].
	///
	/// A window that is over is removed in `on_idle`, see [`PruneCursor`].
	#[pallet::storage]
	pub type RequestWindows<T: Config> =
		StorageMap<_, Twox64Concat, ParaId, (BlockNumberFor<T>, u32), OptionQuery>;

	/// The parachain whose entry of [`RequestWindows`] `on_idle` looked at last: it looks at the
	/// one after it next, in the order the map iterates. `None` to look at the first one.
	#[pallet::storage]
	pub type PruneCursor<T: Config> = StorageValue<_, ParaId, OptionQuery>;

	/// Releases that could not be sent yet, oldest first.
	///
	/// A release cannot fail: [`ChannelDepositProvider`] requires it to be applied. So when
	/// [`Config::SendToPara`] refuses a release, it waits here instead of being dropped, and is
	/// retried in `on_poll`. Any later release joins the queue behind it, to keep releases in
	/// order.
	///
	/// Normally nothing waits here. It only fills when the transport to the parachain is broken.
	/// While it is not empty, other deposit requests are refused with [`Error::TransportBusy`] so
	/// that they do not overtake a waiting release. Once it is full, further releases are dropped
	/// with [`Event::ReleaseDropped`]; governance then has to fix the deposits on the parachain
	/// with `force_set_deposit` of `pallet-hrmp-para`.
	#[pallet::storage]
	pub type FailedReleases<T: Config> =
		StorageValue<_, BoundedVec<ReleaseMessageToPara, T::MaxFailedReleases>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The parachain was asked to set the deposits of a channel.
		DepositRequested { id: u64, channel: ChannelId },
		/// The parachain answered a request.
		DepositAnswered { id: u64, result: Result<(), WireDepositError> },
		/// An answer came for no pending request. It was ignored.
		UnknownAnswer { id: u64 },
		/// The parachain was asked to release deposits of a channel.
		ReleaseSent { channel: ChannelId },
		/// A release could not be sent nor kept, and was dropped.
		ReleaseDropped { channel: ChannelId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// A request for one side of the channel is still pending.
		DepositPending,
		/// Earlier releases still wait for the transport.
		TransportBusy,
		/// The request could not be handed to the transport.
		SendFailed,
		/// The parachain made all the requests it may make in its current window.
		TooManyRequests,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_poll(_: BlockNumberFor<T>, meter: &mut WeightMeter) {
			Self::retry_failed_releases(meter);
		}

		fn on_idle(_: BlockNumberFor<T>, remaining: Weight) -> Weight {
			let weight = T::WeightInfo::prune_request_window();
			if !remaining.all_gte(weight) {
				return Weight::zero();
			}
			Self::prune_request_window();
			weight
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(_: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Take a message from the parachain that holds the HRMP deposits.
		///
		/// The origin must be the parachain holding HRMP deposits.
		// The index must equal `RECEIVE_CALL_INDEX`.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::receive().saturating_add(Pallet::<T>::max_answer_weight()))]
		pub fn receive(origin: OriginFor<T>, message: MessageToRelay) -> DispatchResult {
			let para = ensure_parachain(origin)?;
			ensure!(para == T::HolderParaId::get(), DispatchError::BadOrigin);

			match message {
				MessageToRelay::V1(MessageToRelayV1::DepositsSet { id, result }) => {
					Self::answer(id, result)
				},
			}

			Ok(())
		}

		/// Answer the pending request `id` on the parachain's behalf, for an answer that never
		/// arrived.
		///
		/// An answer for no pending request is ignored, with [`Event::UnknownAnswer`].
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::receive().saturating_add(Pallet::<T>::max_answer_weight()))]
		pub fn force_answer(
			origin: OriginFor<T>,
			id: u64,
			result: Result<(), WireDepositError>,
		) -> DispatchResult {
			ensure_root(origin)?;

			Self::answer(id, result);

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	fn to_wire(channel_id: &HrmpChannelId) -> ChannelId {
		ChannelId { sender: channel_id.sender.into(), recipient: channel_id.recipient.into() }
	}

	/// The window of `para` once one more request is counted in it. A new window starts now if
	/// `para` has none or its last one is over.
	///
	/// Writes nothing. Fails with [`Error::TooManyRequests`] if `para` already made
	/// [`Config::MaxRequestsPerWindow`] requests in its window.
	fn count_request(para: ParaId) -> Result<(BlockNumberFor<T>, u32), Error<T>> {
		let (start, used) = match RequestWindows::<T>::get(para) {
			Some((start, used)) if !Self::is_window_over(start) => (start, used),
			_ => (frame_system::Pallet::<T>::block_number(), 0),
		};
		ensure!(used < T::MaxRequestsPerWindow::get(), Error::<T>::TooManyRequests);
		Ok((start, used.saturating_add(1)))
	}

	/// Whether a window that started at `start` is over.
	fn is_window_over(start: BlockNumberFor<T>) -> bool {
		let now = frame_system::Pallet::<T>::block_number();
		now.saturating_sub(start) >= T::RequestWindow::get()
	}

	/// Looks at one entry of [`RequestWindows`], the one after [`PruneCursor`], and removes it if
	/// its window is over. Starts over from the first entry once past the last.
	pub(crate) fn prune_request_window() {
		let mut windows = match PruneCursor::<T>::get() {
			Some(para) => RequestWindows::<T>::iter_from_key(para),
			None => RequestWindows::<T>::iter(),
		};
		let Some((para, (start, _))) = windows.next() else {
			PruneCursor::<T>::kill();
			return;
		};
		if Self::is_window_over(start) {
			RequestWindows::<T>::remove(para);
		}
		PruneCursor::<T>::put(para);
	}

	/// The parachain `request` is counted for, see [`Config::MaxRequestsPerWindow`]: the side of
	/// the channel that makes it, if it comes from one of them and is not a system parachain.
	fn counted_for(request: &DepositChangeRequest) -> Option<ParaId> {
		let side = match request.action {
			DepositAction::InitOpenChannel { .. } => Some(request.channel_id.sender),
			DepositAction::AcceptOpenChannel { .. } => Some(request.channel_id.recipient),
			DepositAction::PokeChannelDeposits { by } => by,
			// Opened by governance, not by a side.
			DepositAction::InitAndAcceptOpenChannel { .. } => None,
			// Never refused, so never counted.
			DepositAction::Release => None,
		};
		side.filter(|para| !para.is_system())
	}

	/// The sides `request` changes.
	fn sides(request: &DepositChangeRequest) -> impl Iterator<Item = DepositSide> {
		[
			request.sender.map(|_| DepositSide::Sender),
			request.recipient.map(|_| DepositSide::Recipient),
		]
		.into_iter()
		.flatten()
	}

	/// The most [`Config::OnDepositSet`] may spend on an answer.
	fn max_answer_weight() -> Weight {
		[
			DepositAction::InitOpenChannel {
				max_capacity: 1,
				max_message_size: 1,
				max_total_size: 1,
			},
			DepositAction::InitAndAcceptOpenChannel {
				max_capacity: 1,
				max_message_size: 1,
				max_total_size: 1,
			},
			DepositAction::AcceptOpenChannel { max_capacity: 1, max_message_size: 1 },
			DepositAction::PokeChannelDeposits { by: None },
		]
		.iter()
		.map(T::OnDepositSet::weight_of_on_deposit_set)
		.fold(Weight::zero(), |max, w| max.max(w))
	}

	/// Hands the answer to request `id` back to [`Config::OnDepositSet`].
	fn answer(id: u64, result: Result<(), WireDepositError>) {
		let Some(request) = PendingRequests::<T>::take(id) else {
			Self::deposit_event(Event::UnknownAnswer { id });
			return;
		};
		let channel = Self::to_wire(&request.channel_id);

		for side in Self::sides(&request) {
			PendingSides::<T>::remove((channel, side));
		}
		Self::deposit_event(Event::DepositAnswered { id, result });
		let result = result.map_err(|e| match e {
			WireDepositError::InsufficientFunds => DepositError::InsufficientFunds,
			WireDepositError::Other => DepositError::Other,
		});
		T::OnDepositSet::on_deposit_set(request, result);
	}

	/// Retries the releases in [`FailedReleases`], oldest first, until `meter` has no room for the
	/// next one or the transport refuses it.
	fn retry_failed_releases(meter: &mut WeightMeter) {
		if meter.try_consume(T::WeightInfo::retry_failed_releases(0)).is_err() {
			return;
		}

		let mut releases = FailedReleases::<T>::get();
		let mut sent = 0u32;
		for release in releases.iter() {
			// What one more message adds to the retry.
			let weight = T::WeightInfo::retry_failed_releases(sent + 1)
				.saturating_sub(T::WeightInfo::retry_failed_releases(sent));
			if meter.try_consume(weight).is_err() {
				break;
			}
			if T::SendToPara::send(release.clone().into()).is_err() {
				break;
			}
			Self::deposit_event(Event::ReleaseSent { channel: release.channel });
			sent += 1;
		}

		if sent > 0 {
			releases.drain(..sent as usize);
			FailedReleases::<T>::put(releases);
		}
	}

	/// Whether no release waits in [`FailedReleases`].
	fn failed_releases_is_empty() -> bool {
		FailedReleases::<T>::decode_len().unwrap_or_default() == 0
	}

	/// Sends a release, or keeps it in [`FailedReleases`] behind those already waiting there.
	fn release(channel: ChannelId, sender: bool, recipient: bool) {
		let release = ReleaseMessageToPara { channel, sender, recipient };
		if Self::failed_releases_is_empty() && T::SendToPara::send(release.clone().into()).is_ok() {
			Self::deposit_event(Event::ReleaseSent { channel });
			return;
		}
		if FailedReleases::<T>::try_append(release).is_err() {
			log::error!(
				target: LOG_TARGET,
				"no room for failed releases, dropped the release of {channel:?}"
			);
			Self::deposit_event(Event::ReleaseDropped { channel });
		}
	}

	/// Checks that the pending requests and the sides they lock agree.
	#[cfg(any(feature = "try-runtime", test))]
	pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		for ((channel, side), id) in PendingSides::<T>::iter() {
			let request = PendingRequests::<T>::get(id)
				.ok_or("hrmp-relay: a side is locked by no pending request")?;
			ensure!(
				Self::to_wire(&request.channel_id) == channel &&
					Self::sides(&request).any(|s| s == side),
				"hrmp-relay: a side is locked by a request for another side"
			);
		}

		for (id, request) in PendingRequests::<T>::iter() {
			let channel = Self::to_wire(&request.channel_id);
			for side in Self::sides(&request) {
				ensure!(
					PendingSides::<T>::get((channel, side)) == Some(id),
					"hrmp-relay: a pending request does not lock its side"
				);
			}
		}

		Ok(())
	}
}

impl<T: Config> ChannelDepositProvider for Pallet<T> {
	type OnDepositSet = T::OnDepositSet;

	fn try_set_deposit(
		request: DepositChangeRequest,
	) -> Result<DepositChangeResponse, DispatchError> {
		let channel = Self::to_wire(&request.channel_id);

		// A release only gives back: it is sent in order and never refused. Everything held for
		// the side is released.
		if request.action == DepositAction::Release {
			Self::release(channel, request.sender.is_some(), request.recipient.is_some());
			return Ok(DepositChangeResponse::Applied);
		}

		ensure!(
			Self::sides(&request).all(|side| !PendingSides::<T>::contains_key((channel, side))),
			Error::<T>::DepositPending
		);
		// A request must not overtake a release made before it.
		ensure!(Self::failed_releases_is_empty(), Error::<T>::TransportBusy);
		// Recorded once the request is sent.
		let window = match Self::counted_for(&request) {
			Some(para) => Some((para, Self::count_request(para)?)),
			None => None,
		};

		let id = NextRequestId::<T>::get();
		let message = MessageToPara::V1(MessageToParaV1::SetDeposits {
			id,
			channel,
			sender: request.sender.map(|c| c.target),
			recipient: request.recipient.map(|c| c.target),
		});
		T::SendToPara::send(message).map_err(|()| Error::<T>::SendFailed)?;

		NextRequestId::<T>::put(id.wrapping_add(1));
		if let Some((para, window)) = window {
			RequestWindows::<T>::insert(para, window);
		}
		for side in Self::sides(&request) {
			PendingSides::<T>::insert((channel, side), id);
		}
		PendingRequests::<T>::insert(id, request);
		Self::deposit_event(Event::DepositRequested { id, channel });
		Ok(DepositChangeResponse::Pending)
	}

	fn weight_of_try_set_deposit() -> Weight {
		T::WeightInfo::try_set_deposit_both_sides().max(T::WeightInfo::try_set_deposit_release())
	}
}
