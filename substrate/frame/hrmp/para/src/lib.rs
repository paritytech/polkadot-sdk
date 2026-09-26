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

//! # HRMP deposit pallet (parachain side)
//!
//! Holds HRMP channel deposits for the relay chain. The relay chain owns every channel and
//! decides every deposit; this pallet holds and releases what it is told, against a
//! [`DepositKey`], from the paying para's sovereign account on this chain.
//!
//! - A [`MessageToParaV1::Hold`] holds the amount and answers the relay chain with
//!   [`MessageToRelayV1::HoldResult`].
//! - A [`MessageToParaV1::Release`] releases what it names of what is held for the key, or all of
//!   it. Releasing a key with nothing held does nothing.
//! - [`Call::poke_channel_deposits`] and [`Call::establish_system_channel`] are the relay chain's
//!   signed HRMP calls, asked of it from here.
//! - [`Call::force_release`] and [`Call::force_answer`] are root's way out when a release or an
//!   answer never arrived.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
	ensure,
	traits::{
		fungible::{Inspect, InspectHold, Mutate, MutateHold},
		tokens::{Fortitude, Precision, Preservation},
		EnsureOrigin,
	},
};
use hrmp_primitives::{
	Balance, ChannelId, DepositKey, MessageToPara, MessageToParaV1, MessageToRelay,
	MessageToRelayV1, ParaId, ReceiveMigratedDeposits,
};
use sp_runtime::{
	traits::{Convert, Zero},
	DispatchResult,
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

/// Used to send an XCM `Transact` to the HRMP pallet on the relay chain.
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
	use frame_support::pallet_prelude::{DispatchResult, *};
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The overarching hold reason.
		type RuntimeHoldReason: From<HoldReason>;

		/// The currency deposits are held in.
		type Currency: MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason, Balance = Balance>
			+ Mutate<Self::AccountId>;

		/// The relay chain.
		type RelayOrigin: EnsureOrigin<Self::RuntimeOrigin>;

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

	/// What is held for each deposit.
	#[pallet::storage]
	pub type Deposits<T: Config> = StorageMap<_, Blake2_128Concat, DepositKey, Balance>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A deposit was held.
		DepositHeld { key: DepositKey, amount: Balance },
		/// A deposit could not be held.
		DepositRefused { key: DepositKey, amount: Balance },
		/// A deposit was released.
		DepositReleased { key: DepositKey, amount: Balance },
		/// Root released a deposit.
		DepositForceReleased { key: DepositKey, amount: Balance },
		/// A deposit migrated from the relay chain was held. `missing` is what the paying para
		/// could not cover.
		DepositMigrated { key: DepositKey, held: Balance, missing: Balance },
		/// The relay chain was asked to reprice a channel's deposits.
		PokeRequested { channel: ChannelId },
		/// The relay chain was asked to open a channel between two system chains.
		SystemChannelRequested { channel: ChannelId },
		/// Root answered a hold on this chain's behalf.
		AnswerForced { key: DepositKey, held: bool },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The answer could not be handed to the transport.
		SendFailed,
		/// Nothing is held for this deposit.
		NoSuchDeposit,
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
		#[pallet::call_index(0)]
		#[pallet::weight(match message {
			MessageToPara::V1(MessageToParaV1::Hold { .. }) => T::WeightInfo::receive_hold(),
			MessageToPara::V1(MessageToParaV1::Release { .. }) =>
				T::WeightInfo::receive_release(),
		})]
		pub fn receive(origin: OriginFor<T>, message: MessageToPara) -> DispatchResult {
			T::RelayOrigin::ensure_origin_or_root(origin)?;

			match message {
				MessageToPara::V1(MessageToParaV1::Hold { key, amount }) => {
					let held = Self::do_hold(key, amount).is_ok();
					if held {
						Self::deposit_event(Event::DepositHeld { key, amount });
					} else {
						Self::deposit_event(Event::DepositRefused { key, amount });
					}
					Self::send(MessageToRelayV1::HoldResult { key, held })?;
				},
				MessageToPara::V1(MessageToParaV1::Release { key, amount }) => {
					if let Some(amount) = Self::do_release(key, amount) {
						Self::deposit_event(Event::DepositReleased { key, amount });
					}
				},
			}
			Ok(())
		}

		/// Release everything held for `key`.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::force_release())]
		pub fn force_release(origin: OriginFor<T>, key: DepositKey) -> DispatchResult {
			frame_system::ensure_root(origin)?;
			let amount = Self::do_release(key, None).ok_or(Error::<T>::NoSuchDeposit)?;
			Self::deposit_event(Event::DepositForceReleased { key, amount });
			Ok(())
		}

		/// Ask the relay chain to bring a channel's deposits in line with its configuration.
		///
		/// Any signed origin may call this, as on the relay chain.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::poke_channel_deposits())]
		pub fn poke_channel_deposits(
			origin: OriginFor<T>,
			sender: ParaId,
			recipient: ParaId,
		) -> DispatchResult {
			ensure_signed(origin)?;
			let channel = ChannelId { sender, recipient };
			Self::send(MessageToRelayV1::PokeChannelDeposits { channel })?;
			Self::deposit_event(Event::PokeRequested { channel });
			Ok(())
		}

		/// Ask the relay chain to open a channel between two system chains.
		///
		/// Any signed origin may call this, as on the relay chain.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::establish_system_channel())]
		pub fn establish_system_channel(
			origin: OriginFor<T>,
			sender: ParaId,
			recipient: ParaId,
		) -> DispatchResult {
			ensure_signed(origin)?;
			let channel = ChannelId { sender, recipient };
			Self::send(MessageToRelayV1::EstablishSystemChannel { channel })?;
			Self::deposit_event(Event::SystemChannelRequested { channel });
			Ok(())
		}

		/// Answer a hold on this chain's behalf, for one whose answer never arrived.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::force_answer())]
		pub fn force_answer(origin: OriginFor<T>, key: DepositKey, held: bool) -> DispatchResult {
			frame_system::ensure_root(origin)?;
			Self::send(MessageToRelayV1::HoldResult { key, held })?;
			Self::deposit_event(Event::AnswerForced { key, held });
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	fn payer(key: &DepositKey) -> T::AccountId {
		T::SovereignAccountOf::convert(key.para())
	}

	fn send(message: MessageToRelayV1) -> DispatchResult {
		T::SendToRelay::send(MessageToRelay::V1(message))
			.map_err(|()| Error::<T>::SendFailed.into())
	}

	/// Hold `amount` for `key`, adding to anything already held for it.
	fn do_hold(key: DepositKey, amount: Balance) -> DispatchResult {
		T::Currency::hold(&HoldReason::ChannelDeposit.into(), &Self::payer(&key), amount)?;
		Deposits::<T>::mutate(key, |held| {
			*held = Some(held.unwrap_or_default().saturating_add(amount))
		});
		Ok(())
	}

	/// Release `amount` of what is held for `key`, or all of it, returning how much that was.
	fn do_release(key: DepositKey, amount: Option<Balance>) -> Option<Balance> {
		let held = Deposits::<T>::get(key)?;
		let amount = amount.map_or(held, |amount| amount.min(held));
		if amount == held {
			Deposits::<T>::remove(key);
		} else {
			Deposits::<T>::insert(key, held - amount);
		}
		let released = T::Currency::release(
			&HoldReason::ChannelDeposit.into(),
			&Self::payer(&key),
			amount,
			Precision::BestEffort,
		)
		.unwrap_or_default();
		Some(released)
	}

	/// Check that each para has exactly its recorded deposits on hold.
	#[cfg(any(feature = "try-runtime", feature = "std", test))]
	pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		let mut per_para = alloc::collections::btree_map::BTreeMap::<ParaId, Balance>::new();
		for (key, amount) in Deposits::<T>::iter() {
			ensure!(!amount.is_zero(), "hrmp-para: a deposit with nothing held is recorded");
			let total = per_para.entry(key.para()).or_default();
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

extern crate alloc;

/// Holds what the paying para can cover, up to `amount`. A shortfall is recorded, never refused.
impl<T: Config> ReceiveMigratedDeposits for Pallet<T> {
	fn receive_deposit(key: DepositKey, amount: Balance) -> DispatchResult {
		let held = if Self::do_hold(key, amount).is_ok() {
			amount
		} else {
			let who = Self::payer(&key);
			let available =
				T::Currency::reducible_balance(&who, Preservation::Protect, Fortitude::Force);
			let held = amount.min(available);
			if !held.is_zero() {
				Self::do_hold(key, held)?;
			}
			held
		};
		Self::deposit_event(Event::DepositMigrated {
			key,
			held,
			missing: amount.saturating_sub(held),
		});
		Ok(())
	}
}
