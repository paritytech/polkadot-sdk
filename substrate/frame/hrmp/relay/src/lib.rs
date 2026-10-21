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
//! Holds the relay chain's HRMP channel deposits on another chain, through
//! `pallet-hrmp-para` there. HRMP itself stays on the relay chain: this pallet is the `hrmp`
//! pallet's [`ChannelDepositProvider`].
//!
//! - A deposit change is sent as [`MessageToParaV1::SetDeposits`] and kept pending until the answer
//!   comes back through [`Call::receive`], which hands the request back to
//!   [`Config::OnDepositSet`]. One request per channel side may be pending at a time.
//! - A release is sent as [`MessageToParaV1::Release`], is never answered and is never refused. One
//!   that cannot be sent waits in [`Outbox`], and no later request overtakes it. The outbox is only
//!   sent in `on_initialize`, so that a request or a release costs the same whatever waits.
//! - [`Call::force_answer`] is governance's way out when an answer never arrives.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame_support::{pallet_prelude::*, weights::Weight};
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

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

/// The index of [`Call::receive`]. The chain that holds the deposits encodes the call with it.
pub const RECEIVE_CALL_INDEX: u8 = 0;

const LOG_TARGET: &str = "runtime::hrmp-relay";

/// Used to send an XCM `Transact` to `pallet-hrmp-para` on the chain that holds the deposits.
pub trait SendToPara {
	/// Send `message` to the parachain.
	///
	/// `Err(())` means the message could not be handed to the transport.
	#[allow(clippy::result_unit_err)]
	fn send(message: MessageToPara) -> Result<(), ()>;
}

#[cfg(feature = "std")]
impl SendToPara for () {
	fn send(_message: MessageToPara) -> Result<(), ()> {
		Ok(())
	}
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

		/// How many releases may wait in [`Outbox`] for the transport.
		#[pallet::constant]
		type MaxOutbox: Get<u32>;

		/// Weight information for the extrinsics in this pallet.
		type WeightInfo: WeightInfo;
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

	/// Releases the transport did not take yet, oldest first.
	#[pallet::storage]
	pub type Outbox<T: Config> =
		StorageValue<_, BoundedVec<MessageToPara, T::MaxOutbox>, ValueQuery>;

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
		/// No request is pending with this id.
		UnknownRequest,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			let (sent, left) = Self::flush_outbox();
			T::WeightInfo::flush_outbox(sent.saturating_add(left))
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(_: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Take a message from the parachain that holds the deposits.
		// Must equal `RECEIVE_CALL_INDEX`.
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
		///
		/// If the parachain did set the deposits, they stay held there until released by its
		/// own governance.
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
		let (max_capacity, max_message_size, max_total_size) = (0, 0, 0);
		[
			DepositAction::InitOpenChannel { max_capacity, max_message_size, max_total_size },
			DepositAction::InitAndAcceptOpenChannel {
				max_capacity,
				max_message_size,
				max_total_size,
			},
			DepositAction::AcceptOpenChannel,
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
			WireDepositError::CannotHold => DepositError::CannotHold,
			WireDepositError::Other => DepositError::Other,
		});
		T::OnDepositSet::on_deposit_set(request, result);
		Ok(())
	}

	/// Sends what waits in [`Outbox`], oldest first, until the transport refuses one. Returns how
	/// many were sent and how many are left.
	fn flush_outbox() -> (u32, u32) {
		let mut outbox = Outbox::<T>::get();
		if outbox.is_empty() {
			return (0, 0);
		}
		let mut sent = 0u32;
		for message in outbox.iter() {
			if T::SendToPara::send(message.clone()).is_err() {
				break;
			}
			// Only releases wait in the outbox.
			if let MessageToPara::V1(MessageToParaV1::Release { channel, .. }) = message {
				Self::deposit_event(Event::ReleaseSent { channel: *channel });
			}
			sent += 1;
		}
		let left = outbox.len() as u32 - sent;
		if sent > 0 {
			outbox.drain(..sent as usize);
			Outbox::<T>::put(outbox);
		}
		(sent, left)
	}

	/// Whether nothing waits in [`Outbox`].
	fn outbox_is_empty() -> bool {
		Outbox::<T>::decode_len().unwrap_or_default() == 0
	}

	/// Sends a release, or keeps it in [`Outbox`] after anything already waiting there.
	fn release(channel: ChannelId, sender: bool, recipient: bool) {
		let message = MessageToPara::V1(MessageToParaV1::Release { channel, sender, recipient });
		if Self::outbox_is_empty() && T::SendToPara::send(message.clone()).is_ok() {
			Self::deposit_event(Event::ReleaseSent { channel });
			return;
		}
		if Outbox::<T>::try_append(message).is_err() {
			log::error!(target: LOG_TARGET, "outbox full, dropped the release of {channel:?}");
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
		// the side is released, whatever the requester's records say.
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
