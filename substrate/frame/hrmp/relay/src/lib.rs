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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame_support::{
	pallet_prelude::*,
	weights::{Weight, WeightMeter},
};
use hrmp_primitives::{
	ChannelId, DepositError as WireDepositError, DepositSide, MessageToPara, MessageToParaV1,
	MessageToRelay, MessageToRelayV1,
};
use polkadot_primitives::{HrmpChannelId, Id as ParaId};
use polkadot_runtime_parachains::{
	ensure_parachain,
	hrmp::deposits::{
		ChannelDepositProvider, DepositAction, DepositChangeRequest, DepositChangeResponse,
		DepositError, OnDepositSet,
	},
	Origin as ParachainOrigin,
};

pub use pallet::*;
pub use weights::WeightInfo;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
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

#[cfg(feature = "std")]
impl SendToPara for () {
	fn send(_message: MessageToPara) -> Result<(), ()> {
		Ok(())
	}
}

/// Where the releases waiting in [`Outbox`] sit, oldest first: from `head` up to, not including,
/// `tail`.
#[derive(Encode, Decode, Clone, Copy, Default, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct OutboxWindow {
	/// The index of the oldest waiting release.
	pub head: u64,
	/// The index the next release takes.
	pub tail: u64,
}

impl OutboxWindow {
	/// Whether no release waits.
	pub fn is_empty(&self) -> bool {
		self.head == self.tail
	}

	/// How many releases wait.
	pub fn len(&self) -> u64 {
		self.tail.wrapping_sub(self.head)
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

		/// The para that holds the deposits. Only it may answer requests.
		#[pallet::constant]
		type HolderParaId: Get<ParaId>;

		/// Sends messages to the parachain that holds the deposits.
		type SendToPara: SendToPara;

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

	/// Releases the transport did not take yet, by their index in [`OutboxBounds`].
	///
	/// Not bounded: it only grows while the transport refuses messages, and then no request can
	/// take a new deposit. Each release stands for a deposit the parachain still holds.
	#[pallet::storage]
	pub type Outbox<T: Config> = StorageMap<_, Twox64Concat, u64, MessageToPara, OptionQuery>;

	/// Where the releases waiting in [`Outbox`] sit.
	#[pallet::storage]
	pub type OutboxBounds<T: Config> = StorageValue<_, OutboxWindow, ValueQuery>;

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
	}

	#[pallet::error]
	pub enum Error<T> {
		/// A request for one side of the channel is still pending.
		DepositPending,
		/// Earlier releases still wait for the transport.
		TransportBusy,
		/// The request could not be handed to the transport.
		SendFailed,
		/// No request is pending with this id.
		UnknownRequest,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_poll(_: BlockNumberFor<T>, meter: &mut WeightMeter) {
			Self::flush_outbox(meter);
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
					if Self::answer(id, result).is_err() {
						Self::deposit_event(Event::UnknownAnswer { id });
					}
				},
			}
			Ok(())
		}

		/// Answer the pending request `id` on the parachain's behalf, for an answer that never
		/// arrived.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::receive().saturating_add(Pallet::<T>::max_answer_weight()))]
		pub fn force_answer(
			origin: OriginFor<T>,
			id: u64,
			result: Result<(), WireDepositError>,
		) -> DispatchResult {
			ensure_root(origin)?;
			Self::answer(id, result)
		}
	}
}

impl<T: Config> Pallet<T> {
	fn to_wire(channel_id: &HrmpChannelId) -> ChannelId {
		ChannelId { sender: channel_id.sender.into(), recipient: channel_id.recipient.into() }
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
			DepositAction::PokeChannelDeposits,
		]
		.iter()
		.map(T::OnDepositSet::weight_of_on_deposit_set)
		.fold(Weight::zero(), |max, w| max.max(w))
	}

	/// Hands the answer to request `id` back to [`Config::OnDepositSet`].
	fn answer(id: u64, result: Result<(), WireDepositError>) -> DispatchResult {
		let request = PendingRequests::<T>::take(id).ok_or(Error::<T>::UnknownRequest)?;
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
		Ok(())
	}

	/// Sends what waits in [`Outbox`], oldest first, until `meter` has no room for the next one or
	/// the transport refuses it.
	fn flush_outbox(meter: &mut WeightMeter) {
		if meter.try_consume(T::WeightInfo::flush_outbox(0)).is_err() {
			return;
		}
		let mut window = OutboxBounds::<T>::get();
		let head = window.head;
		for n in 1.. {
			if window.is_empty() {
				break;
			}
			// What the `n`th message adds to the flush.
			let weight =
				T::WeightInfo::flush_outbox(n).saturating_sub(T::WeightInfo::flush_outbox(n - 1));
			if meter.try_consume(weight).is_err() {
				break;
			}
			let Some(message) = Outbox::<T>::get(window.head) else {
				frame_support::defensive!("hrmp-relay: no release in the outbox window");
				window.head = window.head.wrapping_add(1);
				continue;
			};
			if T::SendToPara::send(message.clone()).is_err() {
				break;
			}
			// Only releases wait in the outbox.
			if let MessageToPara::V1(MessageToParaV1::Release { channel, .. }) = message {
				Self::deposit_event(Event::ReleaseSent { channel });
			}
			Outbox::<T>::remove(window.head);
			window.head = window.head.wrapping_add(1);
		}
		if window.head != head {
			OutboxBounds::<T>::put(window);
		}
	}

	/// Whether nothing waits in [`Outbox`].
	fn outbox_is_empty() -> bool {
		OutboxBounds::<T>::get().is_empty()
	}

	/// Sends a release, or keeps it in [`Outbox`] after anything already waiting there.
	fn release(channel: ChannelId, sender: bool, recipient: bool) {
		let message = MessageToPara::V1(MessageToParaV1::Release { channel, sender, recipient });
		let mut window = OutboxBounds::<T>::get();
		if window.is_empty() && T::SendToPara::send(message.clone()).is_ok() {
			Self::deposit_event(Event::ReleaseSent { channel });
			return;
		}
		log::warn!(target: LOG_TARGET, "kept the release of {channel:?} in the outbox");
		Outbox::<T>::insert(window.tail, message);
		window.tail = window.tail.wrapping_add(1);
		OutboxBounds::<T>::put(window);
	}

	/// Checks that the pending requests and the sides they lock agree, and that the outbox holds
	/// exactly the releases of its window.
	#[cfg(any(feature = "try-runtime", test))]
	pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		let window = OutboxBounds::<T>::get();
		ensure!(
			Outbox::<T>::iter_keys().count() as u64 == window.len(),
			"hrmp-relay: the outbox holds releases outside its window"
		);
		let mut index = window.head;
		while index != window.tail {
			ensure!(
				Outbox::<T>::contains_key(index),
				"hrmp-relay: a release is missing from the outbox window"
			);
			index = index.wrapping_add(1);
		}
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
		ensure!(Self::outbox_is_empty(), Error::<T>::TransportBusy);

		let id = NextRequestId::<T>::get();
		let message = MessageToPara::V1(MessageToParaV1::SetDeposits {
			id,
			channel,
			sender: request.sender.map(|c| c.target),
			recipient: request.recipient.map(|c| c.target),
		});
		T::SendToPara::send(message).map_err(|()| Error::<T>::SendFailed)?;

		NextRequestId::<T>::put(id.wrapping_add(1));
		for side in Self::sides(&request) {
			PendingSides::<T>::insert((channel, side), id);
		}
		PendingRequests::<T>::insert(id, request);
		Self::deposit_event(Event::DepositRequested { id, channel });
		Ok(DepositChangeResponse::Pending)
	}

	fn weight_of_try_set_deposit() -> Weight {
		T::WeightInfo::try_set_deposit()
	}
}
