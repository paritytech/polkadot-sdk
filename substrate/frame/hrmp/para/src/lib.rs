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

//! # Parachain HRMP deposit pallet
//!
//! Holds the relay chain's HRMP channel deposits. The relay chain owns every channel and decides
//! every deposit; this pallet holds and releases what it is told, against a channel side, from
//! the paying para's sovereign account on this chain.
//!
//! - [`MessageToParaV1::SetDeposits`] moves the named sides to what they should hold, both or
//!   neither, and is answered with [`MessageToRelayV1::DepositsSet`].
//! - [`MessageToParaV1::Release`] releases everything held for the named sides. It never fails and
//!   is not answered.
//! - [`Call::force_set_deposit`] is governance's way out when a side holds other than what the
//!   relay chain records, e.g. after a lost release or a wrong forced answer on the relay chain.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame_support::{
	pallet_prelude::*,
	storage::{transactional::with_transaction_opaque_err, TransactionOutcome},
	traits::{fungible::MutateHold, tokens::Precision},
};
use hrmp_primitives::{
	Balance, ChannelId, DepositError, DepositSide, MessageToPara, MessageToParaV1, MessageToRelay,
	MessageToRelayV1, ParaId,
};
use sp_runtime::{traits::Convert, DispatchResult};

pub use pallet::*;
pub use weights::WeightInfo;

pub mod weights;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

/// The index of [`Call::receive`]. The relay chain encodes the call with it.
pub const RECEIVE_CALL_INDEX: u8 = 0;

/// Used to send an XCM `Transact` to `pallet-hrmp-relay` on the relay chain.
pub trait SendToRelay {
	/// Send `message` to the relay chain.
	///
	/// `Err(())` means the message could not be handed to the transport.
	#[allow(clippy::result_unit_err)]
	fn send(message: MessageToRelay) -> Result<(), ()>;
}

#[cfg(feature = "std")]
impl SendToRelay for () {
	fn send(_message: MessageToRelay) -> Result<(), ()> {
		Ok(())
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The currency the deposits are held in.
		type Currency: MutateHold<Self::AccountId, Reason: From<HoldReason>, Balance = Balance>;

		/// Sends messages to the relay chain.
		type SendToRelay: SendToRelay;

		/// A para's sovereign account on this chain, which pays its deposits.
		type SovereignAccountOf: Convert<ParaId, Self::AccountId>;

		/// Weight information for the extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// A reason for this pallet placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// An HRMP channel deposit.
		#[codec(index = 0)]
		ChannelDeposit,
	}

	/// What is held for each side of each channel. A side holding nothing has no entry.
	#[pallet::storage]
	pub type Deposits<T: Config> =
		StorageMap<_, Twox64Concat, (ChannelId, DepositSide), Balance, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A side now holds `amount`.
		DepositSet { channel: ChannelId, side: DepositSide, amount: Balance },
		/// The deposits of request `id` could not be set. Nothing changed.
		DepositRefused { id: u64, channel: ChannelId },
		/// Everything held for a side, `amount`, was released.
		DepositReleased { channel: ChannelId, side: DepositSide, amount: Balance },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The answer could not be handed to the transport.
		SendFailed,
		/// The side already holds this amount.
		Unchanged,
		/// The side's deposit could not be held, e.g. its funds are short.
		CannotHold,
		/// The side's deposit could not be released.
		CannotRelease,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		#[cfg(feature = "try-runtime")]
		fn try_state(_: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Take a message from the relay chain.
		// Must equal `RECEIVE_CALL_INDEX`.
		#[pallet::call_index(0)]
		#[pallet::weight(match message {
			MessageToPara::V1(MessageToParaV1::SetDeposits { .. }) =>
				T::WeightInfo::receive_set_deposits(),
			MessageToPara::V1(MessageToParaV1::Release { .. }) => T::WeightInfo::receive_release(),
		})]
		pub fn receive(origin: OriginFor<T>, message: MessageToPara) -> DispatchResult {
			// The relay chain's `Superuser` arrives as Root.
			ensure_root(origin)?;
			match message {
				MessageToPara::V1(MessageToParaV1::SetDeposits {
					id,
					channel,
					sender,
					recipient,
				}) => {
					let result = with_transaction_opaque_err(|| {
						let result =
							[(DepositSide::Sender, sender), (DepositSide::Recipient, recipient)]
								.into_iter()
								.try_for_each(|(side, target)| match target {
									Some(target) => Self::set_side(channel, side, target),
									None => Ok(()),
								});
						if result.is_ok() {
							TransactionOutcome::Commit(result)
						} else {
							TransactionOutcome::Rollback(result)
						}
					})
					// Too deep in nested transactions.
					.unwrap_or(Err(DepositError::Other));
					if result.is_err() {
						Self::deposit_event(Event::DepositRefused { id, channel });
					}
					Self::send(MessageToRelayV1::DepositsSet { id, result })
				},
				MessageToPara::V1(MessageToParaV1::Release { channel, sender, recipient }) => {
					for (side, release) in
						[(DepositSide::Sender, sender), (DepositSide::Recipient, recipient)]
					{
						if release {
							Self::release_side(channel, side);
						}
					}
					Ok(())
				},
			}
		}

		/// Hold or release so that `side` of `channel` holds `amount`, for a side that holds other
		/// than what the relay chain records.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::receive_set_deposits())]
		pub fn force_set_deposit(
			origin: OriginFor<T>,
			channel: ChannelId,
			side: DepositSide,
			amount: Balance,
		) -> DispatchResult {
			ensure_root(origin)?;
			let held = Deposits::<T>::get((channel, side)).unwrap_or_default();
			ensure!(amount != held, Error::<T>::Unchanged);
			Self::set_side(channel, side, amount).map_err(|e| match e {
				DepositError::CannotHold => Error::<T>::CannotHold.into(),
				DepositError::Other => Error::<T>::CannotRelease.into(),
			})
		}
	}
}

impl<T: Config> Pallet<T> {
	fn payer(channel: &ChannelId, side: DepositSide) -> T::AccountId {
		T::SovereignAccountOf::convert(side.para(channel))
	}

	fn send(message: MessageToRelayV1) -> DispatchResult {
		T::SendToRelay::send(MessageToRelay::V1(message))
			.map_err(|()| Error::<T>::SendFailed.into())
	}

	/// Holds or releases the difference between what `side` holds and `target`.
	fn set_side(
		channel: ChannelId,
		side: DepositSide,
		target: Balance,
	) -> Result<(), DepositError> {
		let reason = HoldReason::ChannelDeposit.into();
		let who = Self::payer(&channel, side);
		let held = Deposits::<T>::get((channel, side)).unwrap_or_default();
		if target > held {
			T::Currency::hold(&reason, &who, target - held)
				.map_err(|_| DepositError::CannotHold)?;
		} else if target < held {
			T::Currency::release(&reason, &who, held - target, Precision::BestEffort)
				// Only fails on a broken account, e.g. free balance below ED.
				.map_err(|_| DepositError::Other)?;
		}
		if target == 0 {
			Deposits::<T>::remove((channel, side));
		} else {
			Deposits::<T>::insert((channel, side), target);
		}
		Self::deposit_event(Event::DepositSet { channel, side, amount: target });
		Ok(())
	}

	/// Releases everything held for `side`. Nothing held is not an error.
	fn release_side(channel: ChannelId, side: DepositSide) {
		let Some(held) = Deposits::<T>::take((channel, side)) else { return };
		let reason = HoldReason::ChannelDeposit.into();
		let who = Self::payer(&channel, side);
		let amount =
			T::Currency::release(&reason, &who, held, Precision::BestEffort).unwrap_or_default();
		Self::deposit_event(Event::DepositReleased { channel, side, amount });
	}

	/// Checks that each para has exactly its recorded deposits on hold.
	#[cfg(any(feature = "try-runtime", test))]
	pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		use alloc::collections::btree_map::BTreeMap;
		use frame_support::traits::fungible::InspectHold;
		let mut per_para = BTreeMap::<ParaId, Balance>::new();
		for ((channel, side), amount) in Deposits::<T>::iter() {
			ensure!(amount != 0, "hrmp-para: a side holding nothing is recorded");
			let total = per_para.entry(side.para(&channel)).or_default();
			*total = total.saturating_add(amount);
		}
		for (para, recorded) in per_para {
			let on_hold = T::Currency::balance_on_hold(
				&HoldReason::ChannelDeposit.into(),
				&T::SovereignAccountOf::convert(para),
			);
			ensure!(on_hold == recorded, "hrmp-para: a para's hold differs from its deposits");
		}
		Ok(())
	}
}
