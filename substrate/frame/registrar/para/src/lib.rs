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

//! # User Interface Pallet For Parachain Registrations
//!
//! This pallet exposes the extrinsics that can be used to manage parachain registrations. It
//! communicates over XCM with the `pallet-registrar-relay`
//!
//! ## Registration flow
//!
//! Registering a parachain takes two transactions on two different chains, because the validation
//! code is far too large to travel over XCM:
//!
//! 1. [`Pallet::reserve`] allocates a para id here and holds [`Config::ParaDeposit`].
//! 2. [`Pallet::register`] holds the deposit for the head data and the *declared* code length, and
//!    asks the relay chain to accept the registration. Only the code hash and length cross the
//!    bridge.
//! 3. The user uploads the actual validation code via the relay's `apply_authorized_code`, which
//!    checks it against the hash and length it was told about, onboards the para, and reports back.
//! 4. The relay chain's report arrives here as [`Pallet::receive`], which either finalises the
//!    registration or releases the registration deposit.
//!
//! Deposits only ever live on this chain. The relay chain takes nothing.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::traits::{
	fungible::{Inspect, Mutate, MutateHold},
	tokens::Precision,
	Get,
};
use registrar_primitives::{
	FailureReason, MessageToPara, MessageToParaV1, MessageToRelay, MessageToRelayV1, ParaId,
	RegistrationOutcome,
};
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::traits::Saturating;

pub use pallet::*;
pub use weights::WeightInfo;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

/// Balance of the pallet's currency.
pub type BalanceOf<T> =
	<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

/// Used to send an XCM `Transact` to the registrar pallet on the remote relay chain.
///
/// Deliberately free of XCM types: what a message means is this pallet's business, how it travels
/// is the runtime's. The runtime supplies an implementation that encodes the relay chain's call
/// index and hands the program to the router.
pub trait SendToRelay {
	/// The account id used to identify a registration's manager on both chains.
	type AccountId;

	/// Send `message` to the relay chain.
	///
	/// `Err(())` means the message could not be handed to the transport at all. Callers are
	/// expected to fail the whole extrinsic, so nothing is left half-done.
	#[allow(clippy::result_unit_err)]
	fn send(message: MessageToRelay<Self::AccountId>) -> Result<(), ()>;
}

#[cfg(feature = "std")]
impl SendToRelay for () {
	type AccountId = sp_runtime::AccountId32;

	fn send(_message: MessageToRelay<Self::AccountId>) -> Result<(), ()> {
		Ok(())
	}
}

/// Where a para id sits in the registration flow.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum RegistrationState<Balance, BlockNumber> {
	/// The para id is held by its manager, but nothing is registered on the relay chain yet.
	Reserved,
	/// The relay chain has been asked to register this para and has not reported back.
	///
	/// `deposit` is held on top of the para id reservation and covers the head data and the
	/// declared code length.
	Pending {
		/// The registration deposit, released if the registration does not go through.
		deposit: Balance,
		/// The block from which the manager may give up and reclaim `deposit` themselves.
		///
		/// This is a backstop for a report that never arrives; the relay chain expires pending
		/// registrations on its own, sooner than this.
		cancellable_at: BlockNumber,
	},
	/// The relay chain has onboarded this para.
	Registered {
		/// The registration deposit, held for as long as the para is registered.
		deposit: Balance,
	},
}

/// Everything this chain knows about one para id.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, TypeInfo, MaxEncodedLen, Debug,
)]
pub struct ParaInfo<AccountId, Balance, BlockNumber> {
	/// The account that reserved the para id and controls it.
	pub manager: AccountId,
	/// The deposit held for the para id itself, released only on deregistration.
	pub reservation_deposit: Balance,
	/// Where this para id sits in the registration flow.
	pub state: RegistrationState<Balance, BlockNumber>,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The currency that registration deposits are taken in.
		type Currency: Inspect<Self::AccountId>
			+ Mutate<Self::AccountId>
			+ MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>;

		/// The overarching hold reason.
		type RuntimeHoldReason: From<HoldReason>;

		/// Sends messages to the relay chain.
		type SendToRelay: SendToRelay<AccountId = Self::AccountId>;

		/// An origin that is sure to be the relay chain's registrar pallet.
		type RelayOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The deposit held for holding onto a para id.
		#[pallet::constant]
		type ParaDeposit: Get<BalanceOf<Self>>;

		/// The deposit held per byte of head data and validation code.
		#[pallet::constant]
		type DataDepositPerByte: Get<BalanceOf<Self>>;

		/// The lowest para id this pallet will hand out.
		///
		/// Mirrors the relay chain's `LOWEST_PUBLIC_ID`. Ids below it are reserved for system
		/// parachains and are not obtainable here.
		#[pallet::constant]
		type FirstPublicParaId: Get<ParaId>;

		/// The smallest validation code the relay chain will accept.
		///
		/// A local mirror of the relay chain's `MIN_CODE_SIZE`, used to fail early. The relay
		/// chain checks the real thing against its own live configuration.
		#[pallet::constant]
		type MinCodeSize: Get<u32>;

		/// The largest validation code the relay chain will accept.
		///
		/// A local mirror of the relay chain's `max_code_size`. See [`Config::MinCodeSize`].
		#[pallet::constant]
		type MaxCodeSize: Get<u32>;

		/// The largest head data the relay chain will accept.
		///
		/// A local mirror of the relay chain's `max_head_data_size`. See [`Config::MinCodeSize`].
		#[pallet::constant]
		type MaxHeadDataSize: Get<u32>;

		/// How long a manager must wait before abandoning a pending registration themselves.
		///
		/// Must be comfortably longer than the relay chain's own expiry, so that in the normal
		/// course of events the relay chain's report arrives first and this never comes into play.
		#[pallet::constant]
		type PendingDeadline: Get<BlockNumberFor<Self>>;

		/// Weight information for the extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// A reason for this pallet placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Held for keeping a para id reserved.
		#[codec(index = 0)]
		ParaIdReservation,
		/// Held for the head data and validation code of a registration.
		#[codec(index = 1)]
		Registration,
	}

	/// The next para id that [`Pallet::reserve`] will hand out.
	#[pallet::storage]
	pub type NextFreeParaId<T: Config> = StorageValue<_, ParaId, ValueQuery>;

	/// Every para id reserved through this pallet, and what is happening with it.
	#[pallet::storage]
	pub type Paras<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ParaId,
		ParaInfo<T::AccountId, BalanceOf<T>, BlockNumberFor<T>>,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A para id was reserved.
		Reserved { para_id: ParaId, who: T::AccountId },
		/// A registration was requested and the relay chain has been asked to accept it.
		RegisterRequested { para_id: ParaId, manager: T::AccountId },
		/// The relay chain confirmed a registration.
		Registered { para_id: ParaId, manager: T::AccountId },
		/// The relay chain rejected a registration. The registration deposit was released.
		RegistrationFailed { para_id: ParaId, manager: T::AccountId, reason: FailureReason },
		/// A manager abandoned a pending registration. The registration deposit was released.
		RegistrationCancelled { para_id: ParaId, manager: T::AccountId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The para id has not been reserved.
		NotReserved,
		/// The caller does not manage this para id.
		NotOwner,
		/// The para id is already registered, or a registration is already in flight for it.
		AlreadyRegistered,
		/// There is no registration in flight for this para id.
		NotPending,
		/// The manager may not abandon this registration yet.
		CannotCancelYet,
		/// The head data is larger than the relay chain will accept.
		HeadDataTooLarge,
		/// The validation code is larger than the relay chain will accept.
		CodeTooLarge,
		/// The validation code is smaller than the relay chain will accept.
		CodeTooSmall,
		/// The message could not be handed to the transport.
		SendFailed,
		/// There are no more para ids to hand out.
		NoFreeParaId,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Reserve the next free para id for the caller.
		///
		/// Holds [`Config::ParaDeposit`]. The caller becomes the manager of the new id and is the
		/// only account that may [`Pallet::register`] against it.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::reserve())]
		pub fn reserve(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let para_id = NextFreeParaId::<T>::get().max(T::FirstPublicParaId::get());
			let next = para_id.checked_add(1).ok_or(Error::<T>::NoFreeParaId)?;
			ensure!(!Paras::<T>::contains_key(para_id), Error::<T>::AlreadyRegistered);

			let deposit = T::ParaDeposit::get();
			T::Currency::hold(&HoldReason::ParaIdReservation.into(), &who, deposit)?;

			Paras::<T>::insert(
				para_id,
				ParaInfo {
					manager: who.clone(),
					reservation_deposit: deposit,
					state: RegistrationState::Reserved,
				},
			);
			NextFreeParaId::<T>::put(next);

			Self::deposit_event(Event::Reserved { para_id, who });
			Ok(())
		}

		/// Ask the relay chain to register head data and validation code for a reserved para id.
		///
		/// The validation code itself stays here: only `code_hash` and `code_len` are sent. The
		/// caller uploads the blob to the relay chain separately, which accepts it only if it
		/// hashes to `code_hash` and is exactly `code_len` bytes long.
		///
		/// ## Deposits
		///
		/// Holds [`Config::DataDepositPerByte`] for every byte of head data and for every byte of
		/// the *declared* code length, on top of the para id reservation. It is released if the
		/// relay chain rejects the registration or if the caller later abandons it.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::register(genesis_head.len() as u32))]
		pub fn register(
			origin: OriginFor<T>,
			para_id: ParaId,
			genesis_head: Vec<u8>,
			code_len: u32,
			code_hash: H256,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let mut info = Paras::<T>::get(para_id).ok_or(Error::<T>::NotReserved)?;
			ensure!(info.manager == who, Error::<T>::NotOwner);
			ensure!(
				matches!(info.state, RegistrationState::Reserved),
				Error::<T>::AlreadyRegistered
			);

			let head_len =
				u32::try_from(genesis_head.len()).map_err(|_| Error::<T>::HeadDataTooLarge)?;
			ensure!(head_len <= T::MaxHeadDataSize::get(), Error::<T>::HeadDataTooLarge);
			ensure!(code_len >= T::MinCodeSize::get(), Error::<T>::CodeTooSmall);
			ensure!(code_len <= T::MaxCodeSize::get(), Error::<T>::CodeTooLarge);

			let deposit = Self::registration_deposit(head_len, code_len);
			T::Currency::hold(&HoldReason::Registration.into(), &who, deposit)?;

			let cancellable_at =
				frame_system::Pallet::<T>::block_number().saturating_add(T::PendingDeadline::get());
			info.state = RegistrationState::Pending { deposit, cancellable_at };
			Paras::<T>::insert(para_id, info);

			// A transport failure returns `Err` and unwinds everything above, including the hold.
			T::SendToRelay::send(MessageToRelay::V1(MessageToRelayV1::Register {
				para_id,
				manager: who.clone(),
				genesis_head,
				code_hash,
				code_len,
			}))
			.map_err(|()| Error::<T>::SendFailed)?;

			Self::deposit_event(Event::RegisterRequested { para_id, manager: who });
			Ok(())
		}

		/// Abandon a registration the relay chain never reported on, and reclaim its deposit.
		///
		/// A backstop for a report that got lost. The relay chain expires pending registrations
		/// well before [`Config::PendingDeadline`], so under normal operation the report arrives
		/// first and this is never needed.
		///
		/// The para id itself stays reserved, so the manager can simply try again.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::cancel_registration())]
		pub fn cancel_registration(origin: OriginFor<T>, para_id: ParaId) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let info = Paras::<T>::get(para_id).ok_or(Error::<T>::NotReserved)?;
			ensure!(info.manager == who, Error::<T>::NotOwner);
			let RegistrationState::Pending { cancellable_at, .. } = info.state else {
				return Err(Error::<T>::NotPending.into());
			};
			ensure!(
				frame_system::Pallet::<T>::block_number() >= cancellable_at,
				Error::<T>::CannotCancelYet
			);

			Self::release_registration_deposit(para_id)?;
			Self::deposit_event(Event::RegistrationCancelled { para_id, manager: who });
			Ok(())
		}

		/// Accept a report from the relay chain's registrar pallet.
		///
		/// Not callable by users: the origin must be the relay chain.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::receive())]
		pub fn receive(origin: OriginFor<T>, message: MessageToPara) -> DispatchResult {
			T::RelayOrigin::ensure_origin_or_root(origin)?;

			match message {
				MessageToPara::V1(MessageToParaV1::RegistrationResult { para_id, outcome }) => {
					Self::on_registration_result(para_id, outcome)
				},
			}
		}
	}
}

impl<T: Config> Pallet<T> {
	/// The deposit held for a registration, on top of the para id reservation.
	pub fn registration_deposit(head_len: u32, code_len: u32) -> BalanceOf<T> {
		let per_byte = T::DataDepositPerByte::get();
		per_byte
			.saturating_mul(head_len.into())
			.saturating_add(per_byte.saturating_mul(code_len.into()))
	}

	/// Apply the relay chain's verdict on a registration.
	///
	/// A report about a para id we are not expecting one for is dropped rather than treated as an
	/// error: erroring here would unwind the whole XCM `Transact` for a message we can do nothing
	/// about anyway.
	fn on_registration_result(
		para_id: ParaId,
		outcome: RegistrationOutcome,
	) -> sp_runtime::DispatchResult {
		let Some(mut info) = Paras::<T>::get(para_id) else {
			log::warn!(
				target: "runtime::registrar-para",
				"registration result for unknown para {para_id}, dropping",
			);
			return Ok(());
		};
		let RegistrationState::Pending { deposit, .. } = info.state else {
			log::warn!(
				target: "runtime::registrar-para",
				"registration result for para {para_id} which is not pending, dropping",
			);
			return Ok(());
		};

		match outcome {
			RegistrationOutcome::Registered => {
				info.state = RegistrationState::Registered { deposit };
				let manager = info.manager.clone();
				Paras::<T>::insert(para_id, info);
				Self::deposit_event(Event::Registered { para_id, manager });
			},
			RegistrationOutcome::Failed(reason) => {
				let manager = info.manager.clone();
				Self::release_registration_deposit(para_id)?;
				Self::deposit_event(Event::RegistrationFailed { para_id, manager, reason });
			},
		}

		Ok(())
	}

	/// Release the registration deposit for a pending para id and put it back to `Reserved`.
	fn release_registration_deposit(para_id: ParaId) -> sp_runtime::DispatchResult {
		Paras::<T>::try_mutate(para_id, |maybe_info| -> sp_runtime::DispatchResult {
			let info = maybe_info.as_mut().ok_or(Error::<T>::NotReserved)?;
			let RegistrationState::Pending { deposit, .. } = info.state else {
				return Err(Error::<T>::NotPending.into());
			};

			T::Currency::release(
				&HoldReason::Registration.into(),
				&info.manager,
				deposit,
				Precision::BestEffort,
			)?;
			info.state = RegistrationState::Reserved;
			Ok(())
		})
	}
}
