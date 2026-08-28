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

//! # Parachain registrar pallet
//!
//! The user-facing half of parachain registration. It hands out para ids, takes the manager's
//! deposits as [`Consideration`] tickets, and coordinates the registration itself asynchronously
//! with the chain that owns the parachain registry.
//!
//! Both directions of that coordination are abstract: requests go out through [`SendToRelay`],
//! verdicts come back in through [`Pallet::receive`], gated by [`Config::RelayOrigin`]. Nothing
//! here depends on XCM or on the other chain's extrinsics.
//!
//! ## Registration flow
//!
//! Registration takes two transactions on two chains. Sending a multi-megabyte validation code
//! through the messaging layer would be wasteful, so this chain only commits to its hash and
//! length and the blob is uploaded to the relay chain directly:
//!
//! 1. [`Pallet::reserve`] allocates a para id here and takes [`Config::ReservationConsideration`].
//! 2. [`Pallet::register`] takes [`Config::RegistrationConsideration`] for the head data and the
//!    *declared* code length, then asks the relay chain to accept the registration. Only the code
//!    hash and length are sent.
//! 3. The manager uploads the validation code to the relay chain, which accepts it only if it
//!    matches the hash and length committed to in step 2.
//! 4. The verdict arrives back as [`Pallet::receive`], which either finalises the registration or
//!    releases the registration deposit.
//!
//! ## Giving up
//!
//! Nothing on the relay chain times a registration out, so a request whose code never turns up
//! waits until the manager ends it with [`Pallet::cancel_registration`]. That asks the relay chain
//! to drop the authorization and only releases the deposit once it confirms, which is what
//! keeps a cancellation from freeing the deposit on a para that did register after all.
//!
//! ## Deregistering
//!
//! [`Pallet::deregister`] drops a merely reserved id on the spot, deposit returned. A registered
//! para is deregistered on the relay chain too: only its confirmation releases the deposits, a
//! refusal puts the entry back to registered. A verdict that never arrives is chased up with
//! [`Pallet::cancel_deregistration`], the counterpart of [`Pallet::cancel_registration`].
//!
//! Deposits only ever live on this chain; the relay chain takes nothing.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{
	defensive, ensure,
	traits::{
		fungible::{Inspect, Mutate},
		tokens::{Fortitude, Precision, Preservation},
		Consideration, EnsureOrigin, Footprint, Get,
	},
};
use hrmp_primitives::{OnParaRegistered, ParaManager};
use registrar_primitives::{
	FailureReason, MessageToPara, MessageToParaV1, MessageToRelay, MessageToRelayV1, Outcome,
	ParaId,
};
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::{
	traits::{BlockNumberProvider, Saturating, Zero},
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

/// The balance type this pallet burns in, taken from the configured fungible.
pub type BalanceOf<T> =
	<<T as Config>::Fungible as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

/// Block number used for registration deadlines.
///
/// On a parachain, configure [`Config::BlockNumberProvider`] to
/// `cumulus_pallet_parachain_system::RelaychainDataProvider`, so deadlines are expressed in
/// relay-chain blocks and keep their meaning through a stall in this chain's own block production.
pub type ProvidedBlockNumberOf<T> =
	<<T as Config>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;

/// Used to send an XCM `Transact` to the registrar pallet on the remote relay chain.
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

/// Whether a para id still holds a coretime assignment (a lease or a region) on this chain.
///
/// A para that can still be scheduled must not be deregistered out from under itself, so
/// [`Pallet::deregister`] refuses while this says yes. Runtimes without coretime knowledge use
/// `()`, which never blocks.
pub trait AssignmentChecker {
	/// Whether this implementation can ever report an assignment.
	///
	/// Declared rather than inferred so that a runtime which *should* know about coretime can be
	/// stopped at startup from shipping one that does not — see [`Config::RequireAssignmentLock`].
	const NEVER_ASSIGNS: bool = false;

	/// Whether `para_id` still holds a lease or a region.
	fn has_assignment(para_id: ParaId) -> bool;
}

/// An [`AssignmentChecker`] that never reports an assignment.
///
/// For runtimes that genuinely have no coretime to consult. **Not** for the chain that hosts it:
/// there, a para holding a core is what locks it against its manager, and this would leave a live
/// parachain's manager able to deregister it.
///
/// A named type rather than an impl on `()`, so choosing it is a decision somebody wrote down and
/// a reviewer can grep for, instead of the thing you get by leaving a config line alone.
pub struct NoAssignments;

impl AssignmentChecker for NoAssignments {
	const NEVER_ASSIGNS: bool = true;

	fn has_assignment(_para_id: ParaId) -> bool {
		false
	}
}

/// Where a para id sits in the registration flow.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum RegistrationState<Ticket, BlockNumber> {
	/// The para id is held by its manager, but nothing is registered on the relay chain yet.
	Reserved,
	/// The relay chain has been asked to register this para and has not reported back.
	Pending {
		/// The registration's [`Consideration`] ticket, returned if the registration fails.
		ticket: Ticket,
		/// The block from which the manager may give up on this registration.
		///
		/// Expressed in [`Config::BlockNumberProvider`] blocks. Long enough that a verdict already
		/// on its way arrives first, so a cancellation is only ever sent for a registration that
		/// really has gone quiet. Pushed out again by every [`Pallet::cancel_registration`], so a
		/// cancellation that gets lost can be retried but not spammed.
		cancellable_at: BlockNumber,
	},
	/// The relay chain has onboarded this para.
	Registered {
		/// The registration's [`Consideration`] ticket, kept while the para is registered.
		ticket: Ticket,
	},
	/// The relay chain has been asked to deregister this para and has not reported back.
	Deregistering {
		/// The registration's [`Consideration`] ticket. Released, together with the reservation,
		/// only when the relay chain confirms the deregistration.
		ticket: Ticket,
		/// The block from which the manager may chase up this deregistration.
		cancellable_at: BlockNumber,
	},
}

/// Everything this chain knows about one para id.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, TypeInfo, MaxEncodedLen, Debug,
)]
pub struct ParaInfo<AccountId, ReservationTicket, RegistrationTicket, BlockNumber> {
	/// The account that reserved the para id and controls it.
	pub manager: AccountId,
	/// The [`Consideration`] ticket for the para id itself.
	pub reservation: ReservationTicket,
	/// Where this para id sits in the registration flow.
	pub state: RegistrationState<RegistrationTicket, BlockNumber>,
	/// Whether the manager is locked out of the calls that change the para.
	///
	/// Gates [`Pallet::deregister`], [`Pallet::schedule_code_upgrade`] and
	/// [`Pallet::set_current_head`]. Asymmetric on purpose, as on the relay chain: a manager may
	/// lock themselves out but may not let themselves back in.
	///
	/// A fresh registration starts unlocked, so a manager who has just made a mistake can undo
	/// it. What protects a para that is actually running is
	/// [`Config::AssignmentChecker`] — holding a core is a better test of "in use" than having
	/// once produced a block, which is what the relay chain's own lock keys off.
	pub locked: bool,
}

/// The [`ParaInfo`] type as configured.
pub type ParaInfoOf<T> = ParaInfo<
	<T as frame_system::Config>::AccountId,
	<T as Config>::ReservationConsideration,
	<T as Config>::RegistrationConsideration,
	ProvidedBlockNumberOf<T>,
>;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::{DispatchResult, *};
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The cost of reserving a para id. The footprint is a single zero-sized item, so a flat
		/// price fits.
		type ReservationConsideration: Consideration<Self::AccountId, Footprint>;

		/// The cost of a registration, on top of the reservation. The footprint is one item sized
		/// as head data plus *declared* code length, so a per-byte price fits.
		type RegistrationConsideration: Consideration<Self::AccountId, Footprint>;

		/// Sends messages to the relay chain.
		type SendToRelay: SendToRelay<AccountId = Self::AccountId>;

		/// Knows whether a para id still holds a coretime assignment on this chain.
		type AssignmentChecker: AssignmentChecker;

		/// Whether this runtime is one that must be able to tell when a para holds a core.
		///
		/// True on the chain that hosts coretime. Set it there and the startup check refuses a
		/// [`NoAssignments`] checker, which would otherwise silently leave every live parachain's
		/// manager able to deregister it — a one-line config slip with the worst payoff in this
		/// pallet. False elsewhere, where there is genuinely nothing to consult.
		#[pallet::constant]
		type RequireAssignmentLock: Get<bool>;

		/// An origin that is sure to be the relay chain's registrar pallet.
		type RelayOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// An origin a parachain uses to act as itself, resolved to its para id.
		type ParachainOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = ParaId>;

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

		/// How long a manager waits for the relay chain before giving up on a registration.
		///
		/// Measured in [`Config::BlockNumberProvider`] blocks. Should comfortably cover a round
		/// trip, so that a verdict that is merely slow lands before anybody tries to cancel.
		#[pallet::constant]
		type PendingDeadline: Get<ProvidedBlockNumberOf<Self>>;

		/// Source of block numbers for registration deadlines.
		///
		/// On a parachain this should be
		/// `cumulus_pallet_parachain_system::RelaychainDataProvider`, so
		/// [`Config::PendingDeadline`] is in relay-chain blocks.
		type BlockNumberProvider: BlockNumberProvider;

		/// The token an upgrade cooldown is bought out in.
		///
		/// Only used by [`Pallet::remove_upgrade_cooldown`], which burns rather than holds: the
		/// point is to make skipping a cooldown cost something, not to return it later.
		type Fungible: Mutate<Self::AccountId>;

		/// What it costs to drop a para's upgrade cooldown.
		///
		/// The relay chain prices this on how much of the cooldown is left. This chain cannot see
		/// that without another round trip, so the price is flat and governance-tunable. Cruder,
		/// but the cooldown exists to deter rapid upgrades, not to raise revenue.
		#[pallet::constant]
		type UpgradeCooldownCost: Get<BalanceOf<Self>>;

		/// Told when a para finishes registering.
		///
		/// Normally `pallet-hrmp-para`, which opens a channel with the new para so this chain has
		/// a route to every para it is the control plane for. `()` for a runtime that does not
		/// manage HRMP.
		type OnRegistered: OnParaRegistered;

		/// Weight information for the extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Hold reasons for runtimes that pay the considerations out of held funds.
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

	/// The id the next message to the relay chain will carry.
	///
	/// One per message sent, echoed back in the relay chain's response, so a request, its
	/// response and the events on both chains can be tied together.
	#[pallet::storage]
	pub type NextMessageId<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// Every para id reserved through this pallet, and what is happening with it.
	#[pallet::storage]
	pub type Paras<T: Config> = StorageMap<_, Blake2_128Concat, ParaId, ParaInfoOf<T>>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A para id was reserved.
		Reserved { para_id: ParaId, who: T::AccountId },
		/// A registration was requested and the relay chain has been asked to accept it.
		RegisterRequested { para_id: ParaId, message_id: u64, manager: T::AccountId },
		/// The relay chain confirmed a registration.
		Registered { para_id: ParaId, message_id: u64, manager: T::AccountId },
		/// The relay chain rejected a registration. The registration consideration was returned.
		RegistrationFailed {
			para_id: ParaId,
			message_id: u64,
			manager: T::AccountId,
			reason: FailureReason,
		},
		/// A manager gave up on a pending registration, and the relay chain has been asked to
		/// drop the authorization. The consideration stays taken until it answers.
		CancelRequested { para_id: ParaId, message_id: u64, manager: T::AccountId },
		/// The relay chain confirmed a cancellation. The registration consideration was returned.
		RegistrationCancelled { para_id: ParaId, message_id: u64, manager: T::AccountId },
		/// A deregistration was requested and the relay chain has been asked to apply it. Both
		/// considerations stay taken until it answers.
		DeregisterRequested { para_id: ParaId, message_id: u64, manager: T::AccountId },
		/// The para id is gone and both considerations were returned.
		Deregistered { para_id: ParaId, manager: T::AccountId },
		/// The relay chain refused a deregistration. The para is registered again and both
		/// considerations stay taken.
		DeregistrationFailed {
			para_id: ParaId,
			message_id: u64,
			manager: T::AccountId,
			reason: FailureReason,
		},
		/// A manager chased up a deregistration the relay chain never answered.
		CancelDeregistrationRequested { para_id: ParaId, message_id: u64, manager: T::AccountId },
		/// The relay chain called a deregistration off: the para is still registered there, and
		/// is registered again here. Both considerations stay taken.
		DeregistrationCancelled { para_id: ParaId, message_id: u64, manager: T::AccountId },
		/// The para was locked against its manager.
		Locked { para_id: ParaId },
		/// The para was unlocked, returning control to its manager.
		Unlocked { para_id: ParaId },
		/// A validation code upgrade was requested and the relay chain has been asked to
		/// authorize it. No deposit changes hands.
		CodeUpgradeRequested { para_id: ParaId, message_id: u64, code_hash: H256 },
		/// New head data was sent to the relay chain.
		HeadUpdateRequested { para_id: ParaId, message_id: u64 },
		/// Governance moved the para id counter.
		NextFreeParaIdSet { para_id: ParaId },
		/// Governance dropped this chain's record of a para and released its deposits.
		ParaForceRemoved { para_id: ParaId, manager: T::AccountId },
		/// Somebody paid to drop a para's upgrade cooldown, and the relay chain has been asked
		/// to apply it. The cost is burned here and is not returned if the relay chain finds
		/// nothing to drop.
		UpgradeCooldownRemovalRequested { para_id: ParaId, message_id: u64, who: T::AccountId },
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
		/// A registration or deregistration is already in flight for this para id.
		RequestInFlight,
		/// There is no deregistration in flight for this para id.
		NotDeregistering,
		/// The para id still holds a lease or a region.
		StillAssigned,
		/// The para is locked against its manager.
		ParaLocked,
		/// The para id is reserved but not registered on the relay chain.
		NotRegistered,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		#[cfg(feature = "try-runtime")]
		fn try_state(_: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()
		}

		fn integrity_test() {
			// Otherwise no validation code could ever pass `register`.
			assert!(
				T::MinCodeSize::get() <= T::MaxCodeSize::get(),
				"MinCodeSize ({}) must not exceed MaxCodeSize ({})",
				T::MinCodeSize::get(),
				T::MaxCodeSize::get(),
			);

			// A chain that hosts coretime and cannot see it has no lock at all: holding a core is
			// what shuts a manager out of deregistering, upgrading and setting head data, and
			// there is no other signal here to fall back on.
			assert!(
				!(T::RequireAssignmentLock::get() && T::AssignmentChecker::NEVER_ASSIGNS),
				"AssignmentChecker never reports an assignment, but this runtime declares that it \
				 manages coretime. A live parachain's manager would be able to deregister it.",
			);

			// A zero deadline would let a manager chase a verdict in the same block they asked
			// for it, which is the whole thing the deadline exists to prevent.
			assert!(
				!T::PendingDeadline::get().is_zero(),
				"PendingDeadline must not be zero",
			);
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Reserve the next free para id for the caller.
		///
		/// Takes [`Config::ReservationConsideration`]. The caller becomes the manager of the new
		/// id and is the only account that may [`Pallet::register`] against it.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::reserve())]
		pub fn reserve(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let para_id = Self::next_free_para_id()?;
			let next = para_id.checked_add(1).ok_or(Error::<T>::NoFreeParaId)?;

			let reservation = T::ReservationConsideration::new(&who, Footprint::from_parts(1, 0))?;

			Paras::<T>::insert(
				para_id,
				ParaInfo {
					manager: who.clone(),
					reservation,
					state: RegistrationState::Reserved,
					locked: false,
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
		/// ## Costs
		///
		/// Takes [`Config::RegistrationConsideration`] for the head data and the *declared* code
		/// length, on top of the para id reservation. It is returned if the relay chain rejects
		/// the registration or if the caller later abandons it.
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

			let head_len = genesis_head.len() as u32;
			ensure!(head_len <= T::MaxHeadDataSize::get(), Error::<T>::HeadDataTooLarge);
			ensure!(code_len >= T::MinCodeSize::get(), Error::<T>::CodeTooSmall);
			ensure!(code_len <= T::MaxCodeSize::get(), Error::<T>::CodeTooLarge);

			let ticket = T::RegistrationConsideration::new(
				&who,
				Self::registration_footprint(head_len),
			)?;

			let cancellable_at = T::BlockNumberProvider::current_block_number()
				.saturating_add(T::PendingDeadline::get());
			info.state = RegistrationState::Pending { ticket, cancellable_at };
			Paras::<T>::insert(para_id, info);

			// A transport failure returns `Err` and unwinds everything above, ticket included.
			let message_id = Self::next_message_id();
			T::SendToRelay::send(MessageToRelay::V1(MessageToRelayV1::Register {
				para_id,
				message_id,
				manager: who.clone(),
				genesis_head,
				code_hash,
				code_len,
			}))
			.map_err(|()| Error::<T>::SendFailed)?;

			Self::deposit_event(Event::RegisterRequested { para_id, message_id, manager: who });
			Ok(())
		}

		/// Give up on a registration the relay chain never reported on.
		///
		/// Callable from [`Config::PendingDeadline`] blocks after the request. Nothing on the
		/// relay chain abandons a registration on its own, so this is what ends one whose code
		/// never turned up, and the manager pays for the round trip rather than every relay-chain
		/// block paying for a sweep.
		///
		/// The deposit is not released here: the relay chain is asked to drop the authorization
		/// first, and [`Pallet::receive`] releases the deposit when it confirms. Waiting for that
		/// answer is the point. A registration that did go through, with a verdict that got lost on
		/// the way here, must not have its deposit refunded, and only the relay chain knows
		/// which of the two happened.
		///
		/// The para id itself stays reserved either way, so the manager can simply try again.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::cancel_registration())]
		pub fn cancel_registration(origin: OriginFor<T>, para_id: ParaId) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let mut info = Paras::<T>::get(para_id).ok_or(Error::<T>::NotReserved)?;
			ensure!(info.manager == who, Error::<T>::NotOwner);
			let RegistrationState::Pending { ticket, cancellable_at } = info.state else {
				return Err(Error::<T>::NotPending.into());
			};
			let now = T::BlockNumberProvider::current_block_number();
			ensure!(now >= cancellable_at, Error::<T>::CannotCancelYet);

			// Another deadline's grace before the manager may ask again, so a request that goes
			// missing can be retried without the relay chain being asked once per block.
			info.state = RegistrationState::Pending {
				ticket,
				cancellable_at: now.saturating_add(T::PendingDeadline::get()),
			};
			Paras::<T>::insert(para_id, info);

			// A transport failure returns `Err` and unwinds the new deadline with it.
			let message_id = Self::next_message_id();
			T::SendToRelay::send(MessageToRelay::V1(MessageToRelayV1::CancelRegistration {
				para_id,
				message_id,
			}))
			.map_err(|()| Error::<T>::SendFailed)?;

			Self::deposit_event(Event::CancelRequested { para_id, message_id, manager: who });
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
				MessageToPara::V1(MessageToParaV1::RegisterResponse {
					para_id,
					message_id,
					outcome,
				}) => Self::on_register_response(para_id, message_id, outcome),
				MessageToPara::V1(MessageToParaV1::CancelResponse {
					para_id,
					message_id,
					outcome,
				}) => Self::on_cancel_response(para_id, message_id, outcome),
				MessageToPara::V1(MessageToParaV1::DeregisterResponse {
					para_id,
					message_id,
					outcome,
				}) => Self::on_deregister_response(para_id, message_id, outcome),
				MessageToPara::V1(MessageToParaV1::CancelDeregistrationResponse {
					para_id,
					message_id,
					outcome,
				}) => Self::on_cancel_deregistration_response(para_id, message_id, outcome),
			}
		}

		/// Deregister a para id and, eventually, get the deposits back.
		///
		/// Callable by the para's manager, the para itself, or root, the same set the relay
		/// chain's own registrar accepts. Deposits always go back to the manager who paid them.
		///
		/// A merely reserved id is dropped here and now, with its deposit returned. A registered
		/// para must leave the relay chain's registry too, so the entry moves to
		/// [`RegistrationState::Deregistering`] and the relay chain's answer decides:
		/// confirmation releases both deposits and frees the id, refusal puts the para back to
		/// registered with the deposits still held.
		#[pallet::call_index(4)]
		#[pallet::weight(
			T::WeightInfo::deregister_reserved().max(T::WeightInfo::deregister_registered())
		)]
		pub fn deregister(origin: OriginFor<T>, para_id: ParaId) -> DispatchResult {
			let info = Paras::<T>::get(para_id).ok_or(Error::<T>::NotReserved)?;
			Self::ensure_root_para_or_manager(origin, para_id, &info.manager, Self::is_locked(para_id, &info))?;
			let ParaInfo { manager, reservation, state, locked } = info;

			match state {
				RegistrationState::Reserved => {
					reservation.drop(&manager)?;
					Paras::<T>::remove(para_id);
					Self::deposit_event(Event::Deregistered { para_id, manager });
				},
				RegistrationState::Registered { ticket } => {
					ensure!(
						!T::AssignmentChecker::has_assignment(para_id),
						Error::<T>::StillAssigned
					);

					let cancellable_at = T::BlockNumberProvider::current_block_number()
						.saturating_add(T::PendingDeadline::get());
					Paras::<T>::insert(
						para_id,
						ParaInfo {
							manager: manager.clone(),
							reservation,
							state: RegistrationState::Deregistering { ticket, cancellable_at },
							locked,
						},
					);

					// A transport failure returns `Err` and unwinds everything above.
					let message_id = Self::next_message_id();
					T::SendToRelay::send(MessageToRelay::V1(MessageToRelayV1::Deregister {
						para_id,
						message_id,
					}))
					.map_err(|()| Error::<T>::SendFailed)?;

					Self::deposit_event(Event::DeregisterRequested {
						para_id,
						message_id,
						manager,
					});
				},
				RegistrationState::Pending { .. } | RegistrationState::Deregistering { .. } => {
					return Err(Error::<T>::RequestInFlight.into())
				},
			}

			Ok(())
		}

		/// Chase up a deregistration the relay chain never reported on.
		///
		/// Callable by the same origins as [`Pallet::deregister`], from
		/// [`Config::PendingDeadline`] blocks after the request, with the same mechanics as
		/// [`Pallet::cancel_registration`]. Nothing is released here: only the relay chain knows
		/// whether the deregistration went through, and its answer settles the entry either way,
		/// back to registered or gone with the deposits returned.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::cancel_deregistration())]
		pub fn cancel_deregistration(origin: OriginFor<T>, para_id: ParaId) -> DispatchResult {
			let mut info = Paras::<T>::get(para_id).ok_or(Error::<T>::NotReserved)?;
			Self::ensure_root_para_or_manager(origin, para_id, &info.manager, Self::is_locked(para_id, &info))?;
			let manager = info.manager.clone();
			let RegistrationState::Deregistering { ticket, cancellable_at } = info.state else {
				return Err(Error::<T>::NotDeregistering.into());
			};
			let now = T::BlockNumberProvider::current_block_number();
			ensure!(now >= cancellable_at, Error::<T>::CannotCancelYet);

			// Another deadline's grace before the manager may ask again, so a request that goes
			// missing can be retried without the relay chain being asked once per block.
			info.state = RegistrationState::Deregistering {
				ticket,
				cancellable_at: now.saturating_add(T::PendingDeadline::get()),
			};
			Paras::<T>::insert(para_id, info);

			// A transport failure returns `Err` and unwinds the new deadline with it.
			let message_id = Self::next_message_id();
			T::SendToRelay::send(MessageToRelay::V1(MessageToRelayV1::CancelDeregistration {
				para_id,
				message_id,
			}))
			.map_err(|()| Error::<T>::SendFailed)?;

			Self::deposit_event(Event::CancelDeregistrationRequested {
				para_id,
				message_id,
				manager,
			});
			Ok(())
		}

		/// Lock the para against its manager.
		///
		/// Callable by the manager, the para itself, or root. Shuts the manager out of
		/// [`Pallet::deregister`], [`Pallet::schedule_code_upgrade`] and
		/// [`Pallet::set_current_head`] until somebody who is not the manager unlocks it.
		///
		/// Locking an already locked para is not an error, so a manager racing a lock does not
		/// get a spurious failure.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::add_lock())]
		pub fn add_lock(origin: OriginFor<T>, para_id: ParaId) -> DispatchResult {
			let mut info = Paras::<T>::get(para_id).ok_or(Error::<T>::NotReserved)?;
			// A manager may always lock, including one that is already locked out.
			Self::ensure_root_para_or_manager(origin, para_id, &info.manager, false)?;

			if !info.locked {
				info.locked = true;
				Paras::<T>::insert(para_id, info);
				Self::deposit_event(Event::Locked { para_id });
			}
			Ok(())
		}

		/// Unlock the para, returning control to its manager.
		///
		/// Callable by the para itself or root, and deliberately **not** by the manager: a lock
		/// exists to protect the para from whoever manages it, so letting the manager lift it
		/// would make it decorative.
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::remove_lock())]
		pub fn remove_lock(origin: OriginFor<T>, para_id: ParaId) -> DispatchResult {
			Self::ensure_root_or_para(origin, para_id)?;
			let mut info = Paras::<T>::get(para_id).ok_or(Error::<T>::NotReserved)?;

			if info.locked {
				info.locked = false;
				Paras::<T>::insert(para_id, info);
				Self::deposit_event(Event::Unlocked { para_id });
			}
			Ok(())
		}

		/// Ask the relay chain to accept new validation code for a registered para.
		///
		/// Only the hash and length are sent; the blob is uploaded to the relay chain separately,
		/// exactly as in [`Pallet::register`].
		///
        /// ## Costs
        ///
		/// None. A registration is priced at [`Config::MaxCodeSize`], so any code the relay chain
		/// would accept is already paid for and no top-up is possible or needed.
		///
		/// Unanswered: nothing here is staked on the outcome, so the relay chain reports a refusal
		/// as an event of its own rather than spending a round trip on it.
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::schedule_code_upgrade())]
		pub fn schedule_code_upgrade(
			origin: OriginFor<T>,
			para_id: ParaId,
			code_len: u32,
			code_hash: H256,
		) -> DispatchResult {
			let info = Paras::<T>::get(para_id).ok_or(Error::<T>::NotReserved)?;
			Self::ensure_root_para_or_manager(origin, para_id, &info.manager, Self::is_locked(para_id, &info))?;
			ensure!(
				matches!(info.state, RegistrationState::Registered { .. }),
				Error::<T>::NotRegistered
			);
			ensure!(code_len >= T::MinCodeSize::get(), Error::<T>::CodeTooSmall);
			ensure!(code_len <= T::MaxCodeSize::get(), Error::<T>::CodeTooLarge);

			// A transport failure returns `Err` and unwinds everything above.
			let message_id = Self::next_message_id();
			T::SendToRelay::send(MessageToRelay::V1(MessageToRelayV1::AuthorizeCodeUpgrade {
				para_id,
				message_id,
				code_hash,
				code_len,
			}))
			.map_err(|()| Error::<T>::SendFailed)?;

			Self::deposit_event(Event::CodeUpgradeRequested { para_id, message_id, code_hash });
			Ok(())
		}

		/// Ask the relay chain to set a registered para's head data.
		///
		/// Head data is small enough to travel inline, so unlike validation code this needs no
		/// separate upload. Unanswered, like [`Pallet::schedule_code_upgrade`].
		#[pallet::call_index(9)]
		#[pallet::weight(T::WeightInfo::set_current_head(head.len() as u32))]
		pub fn set_current_head(
			origin: OriginFor<T>,
			para_id: ParaId,
			head: Vec<u8>,
		) -> DispatchResult {
			let info = Paras::<T>::get(para_id).ok_or(Error::<T>::NotReserved)?;
			Self::ensure_root_para_or_manager(origin, para_id, &info.manager, Self::is_locked(para_id, &info))?;
			ensure!(
				matches!(info.state, RegistrationState::Registered { .. }),
				Error::<T>::NotRegistered
			);
			ensure!(
				head.len() as u32 <= T::MaxHeadDataSize::get(),
				Error::<T>::HeadDataTooLarge
			);

			// A transport failure returns `Err` and unwinds everything above.
			let message_id = Self::next_message_id();
			T::SendToRelay::send(MessageToRelay::V1(MessageToRelayV1::SetCurrentHead {
				para_id,
				message_id,
				head,
			}))
			.map_err(|()| Error::<T>::SendFailed)?;

			Self::deposit_event(Event::HeadUpdateRequested { para_id, message_id });
			Ok(())
		}

		/// Drop this chain's record of a para, releasing whatever deposits it holds.
		///
		/// Root only, and the counterpart of `pallet-hrmp-para`'s `force_remove_channel`. It
		/// exists because the two chains can diverge in ways no user-facing call can repair:
		///
		/// - governance acts on the relay chain's own registrar directly, which this chain never
		///   hears about;
		/// - a verdict is lost for good, so the entry stays in an in-flight state past every
		///   deadline;
		/// - a para arrives on the relay chain by some route this chain did not drive.
		///
		/// Without it those cases leave a manager's deposit held forever against a para this chain
		/// can no longer act on.
		///
		/// Deliberately blunt: it tells the relay chain nothing, so the two chains can be left
		/// disagreeing. That is why it is governance's tool and not a user's, and why an entry
		/// still waiting on a verdict is refused until [`Config::PendingDeadline`] has passed —
		/// a verdict that is merely slow must not be torn down from under the relay chain.
		#[pallet::call_index(11)]
		#[pallet::weight(T::WeightInfo::force_remove_para())]
		pub fn force_remove_para(origin: OriginFor<T>, para_id: ParaId) -> DispatchResult {
			frame_system::ensure_root(origin)?;
			let info = Paras::<T>::get(para_id).ok_or(Error::<T>::NotReserved)?;

			let now = T::BlockNumberProvider::current_block_number();
			let overdue = match info.state {
				RegistrationState::Pending { cancellable_at, .. } |
				RegistrationState::Deregistering { cancellable_at, .. } => now >= cancellable_at,
				// Nothing is in flight for these, so there is no verdict to be overdue.
				RegistrationState::Reserved | RegistrationState::Registered { .. } => true,
			};
			ensure!(overdue, Error::<T>::CannotCancelYet);

			let ParaInfo { manager, reservation, state, .. } = info;
			reservation.drop(&manager)?;
			match state {
				RegistrationState::Pending { ticket, .. } |
				RegistrationState::Registered { ticket } |
				RegistrationState::Deregistering { ticket, .. } => {
					ticket.drop(&manager)?;
				},
				RegistrationState::Reserved => {},
			}
			Paras::<T>::remove(para_id);

			Self::deposit_event(Event::ParaForceRemoved { para_id, manager });
			Ok(())
		}

		/// Pay to drop a para's upgrade cooldown, so it can upgrade again sooner.
		///
		/// Permissionless, as on the relay chain: anybody may pay to unblock anybody's para. The
		/// cost is burned from the caller, never held, so there is nothing to return.
		///
		/// Unanswered. If the cooldown has already expired by the time the request lands, the
		/// relay chain says so in an event and the caller is not made whole — the same deal the
		/// relay chain's own call gives today.
		#[pallet::call_index(12)]
		#[pallet::weight(T::WeightInfo::remove_upgrade_cooldown())]
		pub fn remove_upgrade_cooldown(
			origin: OriginFor<T>,
			para_id: ParaId,
		) -> DispatchResult {
			let who = frame_system::ensure_signed(origin)?;
			let info = Paras::<T>::get(para_id).ok_or(Error::<T>::NotReserved)?;
			ensure!(
				matches!(info.state, RegistrationState::Registered { .. }),
				Error::<T>::NotRegistered
			);

			T::Fungible::burn_from(
				&who,
				T::UpgradeCooldownCost::get(),
				Preservation::Preserve,
				Precision::Exact,
				Fortitude::Polite,
			)?;

			// A transport failure returns `Err` and unwinds the burn with it.
			let message_id = Self::next_message_id();
			T::SendToRelay::send(MessageToRelay::V1(MessageToRelayV1::RemoveUpgradeCooldown {
				para_id,
				message_id,
			}))
			.map_err(|()| Error::<T>::SendFailed)?;

			Self::deposit_event(Event::UpgradeCooldownRemovalRequested {
				para_id,
				message_id,
				who,
			});
			Ok(())
		}

		/// Move the para id counter.
		///
		/// Root only. [`Pallet::reserve`] steps over ids it already knows, so this is not needed
		/// to get past a clash; it is here for the case the counter has to be moved deliberately,
		/// such as after ids arrive from another chain.
		#[pallet::call_index(10)]
		#[pallet::weight(T::WeightInfo::force_set_next_free_para_id())]
		pub fn force_set_next_free_para_id(
			origin: OriginFor<T>,
			para_id: ParaId,
		) -> DispatchResult {
			frame_system::ensure_root(origin)?;

			NextFreeParaId::<T>::put(para_id);

			Self::deposit_event(Event::NextFreeParaIdSet { para_id });
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Check that this pallet only ever holds ids it is allowed to hand out.
	///
	/// Ids below [`Config::FirstPublicParaId`] belong to system chains, which are not registered
	/// through here and whose deposits are not this pallet's to hold. `reserve` cannot produce
	/// one, so an entry below the floor means something else put it there — a migration seeding
	/// the wrong range being the way it would actually happen.
	#[cfg(any(feature = "try-runtime", test))]
	pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		for (para_id, _) in Paras::<T>::iter() {
			frame_support::ensure!(
				para_id >= T::FirstPublicParaId::get(),
				"registrar-para: a para id below the public floor is registered here"
			);
		}
		Ok(())
	}

	/// The footprint a registration is charged for: the head data plus the largest validation
	/// code the relay chain will accept.
	///
	/// Priced at [`Config::MaxCodeSize`] rather than the code actually declared, which is what
	/// makes [`Pallet::schedule_code_upgrade`] free: any code the relay chain would accept has
	/// already been paid for, so no upgrade ever needs a top-up and the protocol needs no message
	/// for one. It also matches how the relay chain's own registrar has always priced a
	/// registration, so paras that predate this pallet are charged the same way as new ones.
	pub fn registration_footprint(head_len: u32) -> Footprint {
		Footprint::from_parts(1, head_len.saturating_add(T::MaxCodeSize::get()) as usize)
	}

	/// The next para id [`Pallet::reserve`] can hand out.
	///
	/// Steps over ids this pallet already knows rather than failing on one. The counter and the
	/// map can disagree — a para id may arrive from elsewhere, or a counter may be restored from
	/// another chain's — and failing would brick `reserve` for everybody, permanently, with no
	/// way to advance past the clash.
	fn next_free_para_id() -> Result<ParaId, Error<T>> {
		let mut para_id = NextFreeParaId::<T>::get().max(T::FirstPublicParaId::get());
		while Paras::<T>::contains_key(para_id) {
			para_id = para_id.checked_add(1).ok_or(Error::<T>::NoFreeParaId)?;
		}
		Ok(para_id)
	}

	/// Take the id for the next message to the relay chain.
	fn next_message_id() -> u64 {
		NextMessageId::<T>::mutate(|next| {
			let id = *next;
			*next = next.wrapping_add(1);
			id
		})
	}

	/// Whether the manager is shut out of the calls a lock gates.
	///
	/// Two things lock a para. The stored flag is deliberate — set by the manager, the para, root,
	/// or carried over by the migration, since every live para arrives locked. The assignment
	/// check is automatic: **a para holding a core is locked for as long as it holds one**, so its
	/// registrar manager cannot deregister it, change its code, or rewrite its head while it is in
	/// use.
	///
	/// That second half is what replaces the relay chain's `OnNewHead` lock. The relay chain locks
	/// at a para's first block because that is the only "in use" signal it has; this chain hosts
	/// coretime, so it can ask the better question directly through
	/// [`Config::AssignmentChecker`] — and it is a question, not an event, so there is no hook to
	/// miss and no ordering to get wrong.
	///
	/// A para that *loses* its core becomes manager-controllable again, unlike on the relay chain
	/// where the lock is permanent once set. Where that is not wanted, the coretime side can make
	/// it stick with [`Pallet::add_lock`], which is exactly what the stored flag is for.
	/// A merely reserved id is never locked by an assignment. It cannot hold a core — nothing is
	/// registered for it to schedule — so treating a stray assignment entry as a lock would strand
	/// the reservation deposit behind a governance call for no reason.
	fn is_locked(para_id: ParaId, info: &ParaInfoOf<T>) -> bool {
		if info.locked {
			return true;
		}
		!matches!(info.state, RegistrationState::Reserved) &&
			T::AssignmentChecker::has_assignment(para_id)
	}

	/// Ensure `origin` may manage `para_id`: the para itself, its `manager`, or root.
	///
	/// `locked` shuts the manager out but leaves the para and root alone, which is the same
	/// asymmetry the relay chain's registrar has always had: a lock is protection *from* the
	/// manager, so it cannot be something the manager can work around.
	fn ensure_root_para_or_manager(
		origin: frame_system::pallet_prelude::OriginFor<T>,
		para_id: ParaId,
		manager: &T::AccountId,
		locked: bool,
	) -> DispatchResult {
		// The para is tried first: a runtime may deliver a para origin that also reads as a
		// signed one, and the manager branch must not swallow it.
		if let Ok(id) = T::ParachainOrigin::ensure_origin(origin.clone()) {
			ensure!(id == para_id, Error::<T>::NotOwner);
			return Ok(());
		}
		if let Ok(who) = frame_system::ensure_signed(origin.clone()) {
			ensure!(&who == manager, Error::<T>::NotOwner);
			ensure!(!locked, Error::<T>::ParaLocked);
			return Ok(());
		}
		frame_system::ensure_root(origin)?;
		Ok(())
	}

	/// Ensure `origin` is the para itself or root. Unaffected by the lock.
	fn ensure_root_or_para(
		origin: frame_system::pallet_prelude::OriginFor<T>,
		para_id: ParaId,
	) -> DispatchResult {
		if let Ok(id) = T::ParachainOrigin::ensure_origin(origin.clone()) {
			ensure!(id == para_id, Error::<T>::NotOwner);
			return Ok(());
		}
		frame_system::ensure_root(origin)?;
		Ok(())
	}

	/// Apply the relay chain's verdict on a registration.
	///
	/// A response about a para id we are not expecting one for is dropped rather than treated as a
	/// dispatch error: erroring here would unwind the whole incoming message for something we can
	/// do nothing about anyway. Unexpected responses still trip a defensive failure so they are
	/// loud in logs (and panic under `debug_assertions`).
	fn on_register_response(para_id: ParaId, message_id: u64, outcome: Outcome) -> DispatchResult {
		let Some(mut info) = Paras::<T>::get(para_id) else {
			defensive!("register response for unknown para, dropping", para_id);
			return Ok(());
		};
		let RegistrationState::Pending { ticket, .. } = info.state else {
			defensive!("register response for para which is not pending, dropping", para_id);
			return Ok(());
		};

		let manager = info.manager.clone();
		match outcome {
			Ok(()) => {
				info.state = RegistrationState::Registered { ticket };
				Paras::<T>::insert(para_id, info);
				Self::deposit_event(Event::Registered { para_id, message_id, manager });
				T::OnRegistered::on_registered(para_id);
			},
			Err(reason) => {
				ticket.drop(&info.manager)?;
				info.state = RegistrationState::Reserved;
				Paras::<T>::insert(para_id, info);
				Self::deposit_event(Event::RegistrationFailed {
					para_id,
					message_id,
					manager,
					reason,
				});
			},
		}

		Ok(())
	}

	/// Apply the relay chain's answer to a cancellation.
	///
	/// `Ok(())` means the authorization is gone, so the deposit goes back. The one refusal is
	/// [`FailureReason::AlreadyRegistered`]: the code landed after all and the earlier verdict was
	/// simply lost, so the para is recorded as registered and the deposit stays held.
	///
	/// Unlike a register response, an answer for a para that is no longer pending is expected
	/// rather than defensive: a verdict already in flight when the cancellation was sent settles
	/// the registration first, and this then has nothing left to do.
	fn on_cancel_response(para_id: ParaId, message_id: u64, outcome: Outcome) -> DispatchResult {
		let Some(mut info) = Paras::<T>::get(para_id) else {
			defensive!("cancel response for unknown para, dropping", para_id);
			return Ok(());
		};
		let RegistrationState::Pending { ticket, .. } = info.state else {
			log::debug!(
				target: "runtime::registrar-para",
				"cancel response for para {para_id} which is no longer pending, dropping",
			);
			return Ok(());
		};

		let manager = info.manager.clone();
		match outcome {
			Ok(()) => {
				ticket.drop(&info.manager)?;
				info.state = RegistrationState::Reserved;
				Paras::<T>::insert(para_id, info);
				Self::deposit_event(Event::RegistrationCancelled { para_id, message_id, manager });
			},
			Err(FailureReason::AlreadyRegistered) => {
				info.state = RegistrationState::Registered { ticket };
				Paras::<T>::insert(para_id, info);
				Self::deposit_event(Event::Registered { para_id, message_id, manager });
				T::OnRegistered::on_registered(para_id);
			},
			// Nothing else is a cancellation the relay chain refuses, so leave the registration
			// pending: the manager can ask again once the deadline comes round.
			Err(reason) => {
				defensive!("unexpected cancel refusal, leaving pending", (para_id, &reason));
			},
		}

		Ok(())
	}

	/// Apply the relay chain's verdict on a deregistration.
	fn on_deregister_response(
		para_id: ParaId,
		message_id: u64,
		outcome: Outcome,
	) -> DispatchResult {
		let Some(info) = Paras::<T>::get(para_id) else {
			defensive!("deregister response for unknown para, dropping", para_id);
			return Ok(());
		};
		let ParaInfo { manager, reservation, state, locked } = info;
		let RegistrationState::Deregistering { ticket, .. } = state else {
			defensive!(
				"deregister response for para which is not deregistering, dropping",
				para_id
			);
			return Ok(());
		};

		match outcome {
			Ok(()) => {
				ticket.drop(&manager)?;
				reservation.drop(&manager)?;
				Paras::<T>::remove(para_id);
				Self::deposit_event(Event::Deregistered { para_id, manager });
			},
			Err(reason) => {
				Paras::<T>::insert(
					para_id,
					ParaInfo {
						manager: manager.clone(),
						reservation,
						state: RegistrationState::Registered { ticket },
						locked,
					},
				);
				Self::deposit_event(Event::DeregistrationFailed {
					para_id,
					message_id,
					manager,
					reason,
				});
			},
		}

		Ok(())
	}

	/// Apply the relay chain's answer to a deregistration chase-up.
	///
	/// `Ok(())` means the para is still registered on the relay chain, so the deregistration
	/// never happened and the entry goes back to registered. The one refusal is
	/// [`FailureReason::NotRegistered`]: the deregistration did go through and its report was
	/// lost, so the deposits are released after all.
	fn on_cancel_deregistration_response(
		para_id: ParaId,
		message_id: u64,
		outcome: Outcome,
	) -> DispatchResult {
		let Some(info) = Paras::<T>::get(para_id) else {
			log::debug!(
				target: "runtime::registrar-para",
				"cancel deregistration answer for unknown para {para_id}, dropping",
			);
			return Ok(());
		};
		let ParaInfo { manager, reservation, state, locked } = info;
		let RegistrationState::Deregistering { ticket, .. } = state else {
			log::debug!(
				target: "runtime::registrar-para",
				"cancel deregistration answer for para {para_id} which is not deregistering, \
				dropping",
			);
			return Ok(());
		};

		match outcome {
			Ok(()) => {
				Paras::<T>::insert(
					para_id,
					ParaInfo {
						manager: manager.clone(),
						reservation,
						state: RegistrationState::Registered { ticket },
						locked,
					},
				);
				Self::deposit_event(Event::DeregistrationCancelled {
					para_id,
					message_id,
					manager,
				});
			},
			Err(FailureReason::NotRegistered) => {
				ticket.drop(&manager)?;
				reservation.drop(&manager)?;
				Paras::<T>::remove(para_id);
				Self::deposit_event(Event::Deregistered { para_id, manager });
			},
			// Nothing else is an answer the relay chain gives to a chase-up, so leave the entry
			// deregistering: the manager can ask again once the deadline comes round.
			Err(reason) => {
				defensive!(
					"unexpected cancel deregistration refusal, leaving deregistering",
					(para_id, &reason)
				);
			},
		}

		Ok(())
	}
}

/// Who manages a para here, for pallets that need to know without depending on this one's
/// wire types.
///
/// `pallet-hrmp-para` uses it to let a para's manager act for it as a signed account, alongside
/// the para speaking for itself.
impl<T: Config> ParaManager for Pallet<T> {
	type AccountId = T::AccountId;

	fn manager_of(para_id: ParaId) -> Option<T::AccountId> {
		Paras::<T>::get(para_id).map(|info| info.manager)
	}
}
