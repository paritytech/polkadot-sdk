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
//! Holds and release the deposits of parachain for the relay chain HRMP channel.
//!
//! [`Pallet::receive`] is called with [`MessageToPara`] from the relay chain and follows the
//! protocol defined in hrmp-primitives.
//! [`Call::force_set_deposit`] is called by governance to fix potential inconsistent state.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use core::cmp::Ordering;
use frame_support::{
	pallet_prelude::*,
	storage::{transactional::with_transaction_opaque_err, with_storage_layer, TransactionOutcome},
	traits::{
		fungible::{InspectHold, MutateHold},
		tokens::Precision,
	},
};
use hrmp_primitives::{
	Balance, ChannelId, DepositError, DepositKey, DepositSide, MessageToPara, MessageToParaV1,
	MessageToRelay, MessageToRelayV1, ParaId, ReceiveMigratedDeposits,
};
use sp_runtime::{traits::Convert, DispatchError, DispatchResult, TokenError};

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

/// Used to send an XCM `Transact` to `pallet-hrmp-relay` on the relay chain.
pub trait SendToRelay {
	/// Send `message` to the relay chain.
	#[allow(clippy::result_unit_err)]
	fn send(message: MessageToRelay) -> Result<(), ()>;
}

#[cfg(feature = "std")]
impl SendToRelay for () {
	fn send(_message: MessageToRelay) -> Result<(), ()> {
		Ok(())
	}
}

/// Prepares the runtime for this pallet's benchmarks.
#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper {
	/// Makes [`Config::SendToRelay`] take messages.
	fn ensure_send_to_relay();
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkHelper for () {
	fn ensure_send_to_relay() {}
}

/// The hold reason type of [`Config::Currency`].
pub type ReasonOf<T> =
	<<T as Config>::Currency as InspectHold<<T as frame_system::Config>::AccountId>>::Reason;

/// What one side of a channel holds, and from whose account.
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct Deposit<AccountId> {
	/// What is held.
	pub amount: Balance,
	/// The account it is held from, and released to.
	///
	/// The depositor is always the sovereign account of the para and should be constant, but we
	/// record it and use the recorded value defensively, and to help re-anchoring.
	pub depositor: AccountId,
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
		///
		/// It must be constant for a given para.
		// Defensively, it is only consulted for a side that holds nothing: a side that holds
		// something keeps holding from, and is released to, the account recorded in [`Deposits`].
		type SovereignAccountOf: Convert<ParaId, Self::AccountId>;

		/// Weight information for the operations in this pallet.
		type WeightInfo: WeightInfo;

		/// Prepares the runtime for the benchmarks.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: BenchmarkHelper;
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

	/// What is held for each side of each channel, and from whose account.
	///
	/// A side holding nothing has no entry.
	#[pallet::storage]
	pub type Deposits<T: Config> =
		StorageMap<_, Twox64Concat, (ChannelId, DepositSide), Deposit<T::AccountId>, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A side now holds `amount`.
		DepositSet { channel: ChannelId, side: DepositSide, amount: Balance },
		/// The deposits of request `id` could not be set. Nothing changed.
		DepositRefused { id: u64, channel: ChannelId },
		/// A migrated deposit of `amount` was taken over: the side now holds `held`, which is
		/// `amount` unless the hold to move from, the migration's or this pallet's, was short.
		DepositMigrated { channel: ChannelId, side: DepositSide, amount: Balance, held: Balance },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The answer could not be handed to the transport.
		SendFailed,
		/// The side cannot cover its deposit.
		InsufficientFunds,
		/// The side's deposit could not be held or released for another reason.
		CannotChangeDeposit,
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
		/// Receive a message from the relay chain.
		///
		/// The origin must be root.
		// The index must equal `RECEIVE_CALL_INDEX`.
		#[pallet::call_index(0)]
		#[pallet::weight(match message {
			MessageToPara::V1(MessageToParaV1::SetDeposits { .. }) =>
				T::WeightInfo::receive_set_deposits(),
			MessageToPara::V1(MessageToParaV1::Release { .. }) => T::WeightInfo::receive_release(),
		})]
		pub fn receive(origin: OriginFor<T>, message: MessageToPara) -> DispatchResult {
			ensure_root(origin)?;

			match message {
				MessageToPara::V1(MessageToParaV1::SetDeposits {
					id,
					channel,
					sender,
					recipient,
				}) => {
					let result = with_transaction_opaque_err(|| {
						if let Some(target) = sender {
							if let Err(err) = Self::set_side(channel, DepositSide::Sender, target) {
								return TransactionOutcome::Rollback(Err(err));
							}
						}
						if let Some(target) = recipient {
							if let Err(err) =
								Self::set_side(channel, DepositSide::Recipient, target)
							{
								return TransactionOutcome::Rollback(Err(err));
							}
						}

						TransactionOutcome::Commit(Ok(()))
					})
					.unwrap_or(Err(DepositError::Other)); // Too deep in nested transactions.

					if result.is_err() {
						Self::deposit_event(Event::DepositRefused { id, channel });
					}
					Self::send(MessageToRelayV1::DepositsSet { id, result })
				},
				MessageToPara::V1(MessageToParaV1::Release { channel, sender, recipient }) => {
					if sender {
						Self::release_side(channel, DepositSide::Sender);
					}
					if recipient {
						Self::release_side(channel, DepositSide::Recipient);
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

			if Deposits::<T>::get((channel, side)).map_or(0, |d| d.amount) == amount {
				return Ok(());
			}
			Self::set_side(channel, side, amount).map_err(|e| match e {
				DepositError::InsufficientFunds => Error::<T>::InsufficientFunds.into(),
				DepositError::Other => Error::<T>::CannotChangeDeposit.into(),
			})
		}
	}
}

impl<T: Config> ReceiveMigratedDeposits<ReasonOf<T>> for Pallet<T> {
	fn receive_deposit(key: DepositKey, amount: Balance, source: &ReasonOf<T>) -> DispatchResult {
		let DepositKey { channel, side } = key;
		let local = HoldReason::ChannelDeposit.into();
		// As `set_side`: the recorded depositor, or the payer if nothing is held.
		let (current, who) = match Deposits::<T>::get((channel, side)) {
			Some(Deposit { amount, depositor }) => (amount, depositor),
			None => (0, Self::payer(&channel, side)),
		};
		// What the side is missing comes from the migration's hold, what it holds in excess
		// goes back to it; as much as the hold it comes from has.
		let (from, to, wanted) = if amount > current {
			(source, &local, amount - current)
		} else {
			(&local, source, current - amount)
		};

		let moved = wanted.min(T::Currency::balance_on_hold(from, &who));
		if moved != 0 {
			// Released from one hold and held again for the other, on the same account.
			with_storage_layer(|| -> DispatchResult {
				T::Currency::release(from, &who, moved, Precision::Exact)?;
				T::Currency::hold(to, &who, moved)
			})?;
		}
		let held = if amount > current {
			current.saturating_add(moved)
		} else {
			current.saturating_sub(moved)
		};

		if moved != wanted {
			frame_support::defensive!("hrmp-para: moved holds not as wanted", (wanted, moved));
		}
		if held != current {
			if held == 0 {
				Deposits::<T>::remove((channel, side));
			} else {
				Deposits::<T>::insert((channel, side), Deposit { amount: held, depositor: who });
			}
			Self::deposit_event(Event::DepositSet { channel, side, amount: held });
		}
		Self::deposit_event(Event::DepositMigrated { channel, side, amount, held });
		Ok(())
	}

	fn weight_of_receive_deposit() -> Weight {
		T::WeightInfo::receive_deposit()
	}
}

impl<T: Config> Pallet<T> {
	/// The account that pays for `side` of `channel` when it holds nothing yet.
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
		let (held, who) = match Deposits::<T>::get((channel, side)) {
			Some(Deposit { amount, depositor }) => (amount, depositor),
			None => (0, Self::payer(&channel, side)),
		};

		match target.cmp(&held) {
			Ordering::Greater => {
				T::Currency::hold(&reason, &who, target - held).map_err(|e| match e {
					DispatchError::Token(TokenError::FundsUnavailable) => {
						DepositError::InsufficientFunds
					},
					_ => DepositError::Other,
				})?;
			},
			Ordering::Less => {
				let to_release = held - target;
				let released =
					T::Currency::release(&reason, &who, to_release, Precision::BestEffort)
						.map_err(|_| DepositError::Other)?;
				if released != to_release {
					frame_support::defensive!(
						"hrmp-para: released less than recorded",
						(to_release, released)
					);
				}
			},
			Ordering::Equal => {},
		}

		if target == 0 {
			Deposits::<T>::remove((channel, side));
		} else {
			Deposits::<T>::insert((channel, side), Deposit { amount: target, depositor: who });
		}
		Self::deposit_event(Event::DepositSet { channel, side, amount: target });
		Ok(())
	}

	/// Releases everything held for `side`.
	///
	/// Nothing held is not an error.
	fn release_side(channel: ChannelId, side: DepositSide) {
		let Some(Deposit { amount: held, depositor: who }) = Deposits::<T>::take((channel, side))
		else {
			return;
		};
		let reason = HoldReason::ChannelDeposit.into();

		let released =
			T::Currency::release(&reason, &who, held, Precision::BestEffort).unwrap_or_default();
		if released != held {
			frame_support::defensive!("hrmp-para: released less than recorded", (held, released));
		}

		Self::deposit_event(Event::DepositSet { channel, side, amount: 0 });
	}

	/// Checks that each depositor has exactly its recorded deposits on hold.
	#[cfg(any(feature = "try-runtime", test))]
	pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		use alloc::collections::btree_map::BTreeMap;
		let mut per_depositor = BTreeMap::<T::AccountId, Balance>::new();

		for (_, Deposit { amount, depositor }) in Deposits::<T>::iter() {
			ensure!(amount != 0, "hrmp-para: a side holding nothing is recorded");
			let total = per_depositor.entry(depositor).or_default();
			*total = total.saturating_add(amount);
		}

		for (depositor, recorded) in per_depositor {
			let on_hold =
				T::Currency::balance_on_hold(&HoldReason::ChannelDeposit.into(), &depositor);
			ensure!(on_hold == recorded, "hrmp-para: a depositor's hold differs from its deposits");
		}

		Ok(())
	}
}
