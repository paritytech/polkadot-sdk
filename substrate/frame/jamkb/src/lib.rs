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

//! # JAMKB Management Pallet (prototype)
//!
//! Manages the Asset Hub representation of JAMKB, the state-footprint token of JAM, as
//! described in `designs/jamkb-management/jamkb-management.md`.
//!
//! This prototype covers the permanent release, holder redemption and voluntary return flows,
//! the governance corrections and the asynchronous operation machinery they rely on. Leases and
//! lease recovery are not implemented; the enums reserve their encodings.
//!
//! ## Pallet API
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes,
//! including its configuration trait, dispatchables, storage items, events and errors.
//!
//! - Governance: `initialize`, `attest`, `pause`, `resume`, `grant`, `approve_release`,
//!   `force_mint`, `dispose_excess`, `retry_move`, `force_resolve` and `force_burn`.
//! - Signed: `execute_release`, `redeem`, `cancel_allocation`, `settle` and `claim`.
//! - `MaxUserOperations` bounds the unsettled `Delivery` and `Redemption` operations below
//!   `MaxUnsettledOperations`; `UserOperations` counts them.
//! - `Unmoved` keeps the transfers of a failed `Move` for `retry_move`.
//! - [`transport::OnIncomingTransfers`] hands over buckets bounded by its `MaxTransfers`, which the
//!   pallet sets to `MaxTransfersPerBucket`.
//!
//! ## Overview
//!
//! JAMKB on Asset Hub represents the Parachain Service's regular balance on JAM. The pallet holds
//! the four privileged roles of the asset through its custody account and keeps undistributed
//! units there. Units sent to JAM are burned when the operation sending them is recorded; units
//! that come back are minted. A mint into an account that cannot receive it is booked as
//! `Unclaimed` for that account and minted later by `claim`.
//!
//! The transport to JAM is the responsibility of `pallet-parachain-system`. This crate does not
//! depend on it; the [`transport`] traits are the boundary. The pallet queues each operation's
//! messages through [`transport::JamMessageSender`], and parachain-system reports each
//! operation's outcome, once and in send order, through [`transport::OnJamOutcome`], and hands
//! over the Parachain Service's `incoming_transfers` buckets through
//! [`transport::OnIncomingTransfers`]. The [`messages`] module carries a minimal mirror of the
//! Parachain Service message and log entry shapes.
//!
//! ## Flows
//!
//! - Governance release: `approve_release` records the allocation; `execute_release` burns the
//!   amount from custody and sends a `Delivery` operation; its settlement moves the allocation to
//!   `Delivered` on Confirmed, or mints the units back to custody and returns the allocation to
//!   `Approved` on Failed.
//! - Holder redemption: `redeem` burns the amount from the caller and sends a `Redemption`
//!   operation; on Failed its settlement mints the units back to the caller.
//! - Voluntary return: each incoming transfer is minted to the beneficiary its memo names, or
//!   booked as `Excess` for its source when the memo names none. Transfers that landed on the
//!   Parachain Service's supervisor balance are first moved to its regular balance by a `Move`
//!   operation and booked once it is confirmed. A failed `Move` keeps its transfers in `Unmoved`
//!   until `retry_move` sends them again. `dispose_excess` mints an excess to custody or refunds it
//!   to its source by a `Refund` operation.
//! - `grant` moves undistributed units from custody to an Asset Hub account; `force_mint` corrects
//!   the supply after an unrecorded return on JAM, `force_burn` after an unrecorded outflow.
//!
//! Once every operation has settled, the Parachain Service's regular balance equals the Asset Hub
//! supply plus the `Unclaimed` and `Excess` books (design §2).
//!
//! ## Resolution and settlement
//!
//! An operation is `Requested` from the moment its messages are queued. Its outcome marks it
//! `Failed` when the reported entries hold a `TransferFailed` for its message, and `Confirmed`
//! otherwise. Every operation has a target, and the operations of one target settle in the order
//! they were sent. In each `on_poll`, the resolved operations are settled within
//! `SettlementWeight`: a Confirmed operation applies its effect, a Failed one is undone. A
//! settlement that fails is rolled back and blocks its target's later operations until the
//! permissionless `settle` call retries it; `settle` also takes any resolved operation at the
//! head of its target. Governance resolves an operation the transport never reported with
//! `force_resolve`.

#![deny(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{vec, vec::Vec};
use codec::{Decode, MaxEncodedLen};
use frame_support::{
	defensive,
	pallet_prelude::*,
	storage::with_storage_layer,
	traits::{
		fungibles::{Create, Inspect, Mutate},
		tokens::{DepositConsequence, Fortitude, Precision, Preservation, Provenance},
		EnsureOrigin,
	},
	weights::WeightMeter,
	PalletId,
};
use frame_system::pallet_prelude::*;
use messages::{AccumulateLog, MessageId, UpwardMessage};
use sp_runtime::{
	traits::{AccountIdConversion, BlockNumberProvider, CheckedSub, One, Zero},
	ArithmeticError,
};
use transport::{
	ChargeDeliveryFee, IncomingTransfer, IncomingTransfers, JamMessageSender, MessageSendError,
	OnIncomingTransfers, OnJamOutcome, OperationId, SenderId,
};

pub use pallet::*;
pub use types::*;
pub use weights::WeightInfo;

pub mod messages;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod transport;
mod types;
pub mod weights;

/// Maps an account in pallet-revive, so that contracts can address it. `()` maps nothing.
///
/// The runtime implements it with pallet-revive's `AddressMapper::map_no_deposit_unchecked`,
/// guarded by `AddressMapper::is_mapped`. It must be idempotent: the `OnNewAccount` hook (revive's
/// `AutoMapper`) may already have mapped custody when `initialize` gave it a provider reference.
/// Custody has no native balance, so the depositing `AddressMapper::map` must not be used.
pub trait MapAccount<AccountId> {
	/// Maps `who`. Succeeds when `who` is already mapped.
	fn map(who: &AccountId) -> DispatchResult;
}

impl<AccountId> MapAccount<AccountId> for () {
	fn map(_: &AccountId) -> DispatchResult {
		Ok(())
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The asset identifier type of the assets pallet holding JAMKB.
		type AssetId: Member + Parameter + MaxEncodedLen + Clone;

		/// The balance type of JAMKB.
		type Balance: frame_support::traits::tokens::Balance;

		/// The assets pallet holding JAMKB.
		type Assets: Inspect<Self::AccountId, AssetId = Self::AssetId, Balance = Self::Balance>
			+ Mutate<Self::AccountId>
			+ Create<Self::AccountId>;

		/// The AH→JAM transport that queues the pallet's operations.
		type MessageSender: JamMessageSender<Self::Balance>;

		/// Charges the delivery fee of a signed call's messages.
		type DeliveryFee: ChargeDeliveryFee<Self::AccountId, Self::Balance>;

		/// The current JAM timeslot: the timeslot of the block's anchor, provided by the
		/// transport.
		type TimeslotProvider: BlockNumberProvider<BlockNumber = Timeslot>;

		/// The pallet's sender id at the transport.
		#[pallet::constant]
		type SenderId: Get<SenderId>;

		/// The identifier of the JAMKB asset.
		#[pallet::constant]
		type JamkbAssetId: Get<Self::AssetId>;

		/// The JAMKB units on JAM at genesis. `attest` mints the part on the Parachain Service into
		/// custody.
		#[pallet::constant]
		type Cap: Get<Self::Balance>;

		/// The least amount `approve_release`, `redeem` and a `dispose_excess` refund accept. At
		/// least one unit.
		#[pallet::constant]
		type MinTransfer: Get<Self::Balance>;

		/// The gas every deferred `TransferOut` carries. A destination whose `min_memo_gas` is
		/// above it refuses the transfer.
		#[pallet::constant]
		type MemoGas: Get<u64>;

		/// The weight `on_poll` spends per block settling operations. Covers at least one
		/// settlement of a `Move` of `MaxTransfersPerBucket` transfers.
		#[pallet::constant]
		type SettlementWeight: Get<Weight>;

		/// The pallet's identifier, from which the custody account is derived.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// The service id of the Parachain Service. Rejected as a target.
		#[pallet::constant]
		type ParachainService: Get<ServiceId>;

		/// The most unsettled operations. A call that would exceed it is rejected. Must not be
		/// lowered below the number of unsettled operations by a runtime upgrade.
		#[pallet::constant]
		type MaxUnsettledOperations: Get<u32>;

		/// The most unsettled `Delivery` and `Redemption` operations, the ones signed calls
		/// record. A call that would exceed it is rejected. Below `MaxUnsettledOperations`, so
		/// that cheap redemptions cannot take the room of a `Move` or a `Refund`.
		#[pallet::constant]
		type MaxUserOperations: Get<u32>;

		/// The most `incoming_transfers` buckets taken per block.
		#[pallet::constant]
		type MaxBucketsPerBlock: Get<u32>;

		/// The most transfers in one `incoming_transfers` bucket. At least the Parachain
		/// Service's bucket size; the transport never hands over a larger bucket.
		#[pallet::constant]
		type MaxTransfersPerBucket: Get<u32>;

		/// Maps the custody account in pallet-revive, so contracts can address it.
		type CustodyMapping: MapAccount<Self::AccountId>;

		/// The origin allowed to act as governance.
		type GovernanceOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Weight information for the calls.
		type WeightInfo: WeightInfo;
	}

	/// The JAM block the attestation refers to. Unset until attested.
	#[pallet::storage]
	pub type Attestation<T: Config> = StorageValue<_, Anchor, OptionQuery>;

	/// The pause flag.
	#[pallet::storage]
	pub type Paused<T: Config> = StorageValue<_, bool, ValueQuery>;

	/// The last allocation id assigned.
	#[pallet::storage]
	pub type LastAllocationId<T: Config> = StorageValue<_, AllocationId, ValueQuery>;

	/// The allocations.
	#[pallet::storage]
	pub type Allocations<T: Config> =
		StorageMap<_, Blake2_128Concat, AllocationId, AllocationOf<T>, OptionQuery>;

	/// The unsettled operations, keyed by the id parachain-system assigned.
	#[pallet::storage]
	pub type Operations<T: Config> =
		CountedStorageMap<_, Blake2_128Concat, OperationId, OperationOf<T>, OptionQuery>;

	/// The number of unsettled `Delivery` and `Redemption` operations.
	#[pallet::storage]
	pub type UserOperations<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// The unsettled operation of an allocation, if any.
	#[pallet::storage]
	pub type PendingOperationOf<T: Config> =
		StorageMap<_, Blake2_128Concat, AllocationId, OperationId, OptionQuery>;

	/// The unsettled operations of each target, in send order.
	#[pallet::storage]
	pub type TargetOperations<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ServiceId,
		BoundedVec<OperationId, T::MaxUnsettledOperations>,
		ValueQuery,
	>;

	/// The targets whose earliest unsettled operation failed to settle, with that operation. The
	/// target's later operations wait until `settle` retries it.
	#[pallet::storage]
	pub type Blocked<T: Config> = StorageMap<_, Twox64Concat, ServiceId, OperationId, OptionQuery>;

	/// Operations resolved and not yet tried by `on_poll`, in the order sent, with their
	/// targets. An operation of a blocked target is not listed.
	#[pallet::storage]
	pub type Resolved<T: Config> = StorageValue<
		_,
		BoundedVec<(OperationId, ServiceId), T::MaxUnsettledOperations>,
		ValueQuery,
	>;

	/// Units whose mint to the beneficiary failed, awaiting `claim`.
	#[pallet::storage]
	pub type Unclaimed<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, T::Balance, ValueQuery>;

	/// Returned units with no beneficiary, per source service, until `dispose_excess`.
	#[pallet::storage]
	pub type Excess<T: Config> = StorageMap<_, Twox64Concat, ServiceId, T::Balance, ValueQuery>;

	/// The transfers of each failed `Move`, by the id of that operation, until `retry_move`
	/// sends them again. Their units stay on the Parachain Service's supervisor balance.
	#[pallet::storage]
	pub type Unmoved<T: Config> = StorageMap<
		_,
		Twox64Concat,
		OperationId,
		BoundedVec<IncomingTransfer<T::Balance>, T::MaxTransfersPerBucket>,
		OptionQuery,
	>;

	/// Events.
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The genesis attestation is recorded.
		Attested {
			/// The JAM block the balances were checked against.
			anchor: Anchor,
			/// The units held by services other than the Parachain Service at `anchor`.
			released: T::Balance,
		},
		/// A permanent release was approved.
		ReleaseApproved {
			/// The allocation.
			id: AllocationId,
			/// The credited service.
			target: ServiceId,
			/// The amount to deliver.
			amount: T::Balance,
		},
		/// An allocation was cancelled.
		AllocationCancelled {
			/// The allocation.
			id: AllocationId,
		},
		/// An operation was recorded and its messages queued.
		OperationRequested {
			/// The operation.
			id: OperationId,
			/// The allocation; `None` for a redemption, a refund or a move.
			allocation: Option<AllocationId>,
		},
		/// The operation resolved to success.
		OperationConfirmed {
			/// The operation.
			id: OperationId,
			/// Its success entry, for an operation that has one.
			entry: Option<SuccessEntry<T::Balance>>,
		},
		/// The operation resolved to failure.
		OperationFailed {
			/// The operation.
			id: OperationId,
			/// The Parachain Service failure.
			error: FailureReason,
		},
		/// The settlement of the operation failed; `settle` retries it, and the target's later
		/// operations wait.
		SettlementFailed {
			/// The operation.
			id: OperationId,
			/// The error the settlement failed with.
			error: DispatchError,
		},
		/// An incoming transfer was booked as `class`, with the raw memo it carried. A transfer
		/// to the supervisor balance is booked when its `Move` is confirmed.
		ReturnBooked {
			/// The sending service.
			source: ServiceId,
			/// The amount booked.
			amount: T::Balance,
			/// Where it was booked.
			class: ReturnClass<T::AccountId>,
			/// The memo of the return.
			memo: Memo,
		},
		/// A mint to the beneficiary failed; the amount awaits `claim`.
		UnclaimedBooked {
			/// The account that could not receive the mint.
			beneficiary: T::AccountId,
			/// The amount booked.
			amount: T::Balance,
		},
		/// A beneficiary's unclaimed balance was claimed.
		Claimed {
			/// The beneficiary minted to.
			beneficiary: T::AccountId,
			/// The amount minted.
			amount: T::Balance,
		},
		/// Units moved from custody to an Asset Hub account.
		BudgetGranted {
			/// The receiving account.
			beneficiary: T::AccountId,
			/// The amount moved.
			amount: T::Balance,
		},
		/// An excess was disposed: minted to custody or refunded to its source.
		ExcessDisposed {
			/// The source service of the excess.
			source: ServiceId,
			/// The amount disposed.
			amount: T::Balance,
		},
		/// Governance minted units to correct the supply after an unrecorded return.
		ForceMinted {
			/// The JAM block whose balances show the difference.
			anchor: Anchor,
			/// The account minted to, or booked as `Unclaimed` for.
			beneficiary: T::AccountId,
			/// The amount minted.
			amount: T::Balance,
		},
		/// A `Move` failed; its transfers are kept in `Unmoved` for `retry_move`.
		MoveFailed {
			/// The failed operation.
			id: OperationId,
			/// The sum of its transfers.
			amount: T::Balance,
		},
		/// Governance resolved an operation the transport never reported.
		ForceResolved {
			/// The operation.
			id: OperationId,
		},
		/// Governance burned units from custody to correct the supply after an unrecorded
		/// outflow.
		ForceBurned {
			/// The JAM block whose balances show the difference.
			anchor: Anchor,
			/// The amount burned.
			amount: T::Balance,
		},
		/// The pallet is paused.
		Paused,
		/// The pallet is resumed.
		Resumed,
	}

	/// Errors.
	#[pallet::error]
	pub enum Error<T> {
		/// The pallet is not attested yet.
		NotAttested,
		/// The pallet is paused.
		Paused,
		/// The amount is below `MinTransfer`.
		BelowMinTransfer,
		/// The allocation is not in the state the call needs.
		WrongState,
		/// The allocation is not valid yet.
		NotYetValid,
		/// The allocation has expired.
		Expired,
		/// The target is the Parachain Service.
		TargetIsParachainService,
		/// The allocation has an unsettled operation.
		UnsettledOperation,
		/// The operation is not resolved yet.
		NotResolved,
		/// `MaxUnsettledOperations` operations are unsettled.
		TooManyOperations,
		/// `MaxUserOperations` `Delivery` and `Redemption` operations are unsettled.
		TooManyUserOperations,
		/// The beneficiary has no JAMKB account and cannot have one created.
		CannotCreate,
		/// The transport refused the operation: it names something already in flight.
		TransportConflict,
		/// The transport refused the operation: it can never fit one block.
		TransportTooBig,
		/// The transport refused the operation: there is no room in its queue now.
		TransportQueueFull,
		/// No allocation with this id.
		UnknownAllocation,
		/// No unsettled operation with this id.
		UnknownOperation,
		/// The source's excess does not cover the amount.
		InsufficientExcess,
		/// The debited account cannot cover the amount.
		InsufficientBalance,
		/// The beneficiary has nothing to claim.
		NothingToClaim,
		/// The operation is not the earliest unsettled one of its target.
		OutOfOrder,
		/// The asset does not exist yet.
		NotInitialized,
		/// The attestation is already recorded.
		AlreadyAttested,
		/// No failed `Move` with this id.
		UnknownMove,
	}

	impl<T> From<MessageSendError> for Error<T> {
		fn from(error: MessageSendError) -> Self {
			match error {
				MessageSendError::Conflict => Error::TransportConflict,
				MessageSendError::TooBig => Error::TransportTooBig,
				MessageSendError::QueueFull => Error::TransportQueueFull,
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Creates the JAMKB asset and maps the custody account in pallet-revive.
		///
		/// Origin: governance. The asset is non-sufficient with a minimum balance of one unit;
		/// the custody account takes a provider reference so that it can hold the asset.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::initialize())]
		pub fn initialize(origin: OriginFor<T>) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			let asset = T::JamkbAssetId::get();
			let custody = Self::custody_account();
			T::Assets::create(asset, custody.clone(), false, One::one())?;
			frame_system::Pallet::<T>::inc_providers(&custody);
			T::CustodyMapping::map(&custody)?;
			Ok(())
		}

		/// Records the genesis attestation against JAM state and enables operations.
		///
		/// Origin: governance. `released` is the JAMKB held by services other than the
		/// Parachain Service at `anchor`; `Cap − released` is minted into custody. Rejected when
		/// `released` is above `Cap`. `released` must include the units in the `incoming_transfers`
		/// buckets queued at `anchor`, because those are minted when they are delivered.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::attest())]
		pub fn attest(
			origin: OriginFor<T>,
			anchor: Anchor,
			released: T::Balance,
		) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			ensure!(T::Assets::asset_exists(T::JamkbAssetId::get()), Error::<T>::NotInitialized);
			ensure!(Attestation::<T>::get().is_none(), Error::<T>::AlreadyAttested);
			let on_parachain_service =
				T::Cap::get().checked_sub(&released).ok_or(ArithmeticError::Underflow)?;
			if !on_parachain_service.is_zero() {
				T::Assets::mint_into(
					T::JamkbAssetId::get(),
					&Self::custody_account(),
					on_parachain_service,
				)?;
			}
			Attestation::<T>::put(anchor);
			Self::deposit_event(Event::Attested { anchor, released });
			Ok(())
		}

		/// Sets the pause flag. While set, a call that creates an allocation, records an
		/// operation or distributes units from custody is rejected, and no `incoming_transfers`
		/// bucket is taken. Operations already sent still settle.
		///
		/// Origin: governance.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::pause())]
		pub fn pause(origin: OriginFor<T>) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			Self::ensure_attested()?;
			Paused::<T>::put(true);
			Self::deposit_event(Event::Paused);
			Ok(())
		}

		/// Clears the pause flag.
		///
		/// Origin: governance.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::resume())]
		pub fn resume(origin: OriginFor<T>) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			Self::ensure_attested()?;
			Paused::<T>::put(false);
			Self::deposit_event(Event::Resumed);
			Ok(())
		}

		/// Moves `amount` of undistributed units from custody to `beneficiary`.
		///
		/// Origin: governance. A budget can return by an ordinary transfer to custody.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::grant())]
		pub fn grant(
			origin: OriginFor<T>,
			beneficiary: T::AccountId,
			amount: T::Balance,
		) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			Self::ensure_active()?;
			T::Assets::transfer(
				T::JamkbAssetId::get(),
				&Self::custody_account(),
				&beneficiary,
				amount,
				Preservation::Expendable,
			)?;
			Self::deposit_event(Event::BudgetGranted { beneficiary, amount });
			Ok(())
		}

		/// Approves a permanent release of `amount` to `target`.
		///
		/// Origin: governance, which becomes the approver. `memo` goes to `target` unchanged
		/// with the delivery. `valid_from` `None` means the current timeslot; the allocation
		/// expires `valid_for` timeslots after `valid_from`. Rejected when `target` is the
		/// Parachain Service or `amount` is below `MinTransfer`.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::approve_release())]
		pub fn approve_release(
			origin: OriginFor<T>,
			target: ServiceId,
			amount: T::Balance,
			memo: Memo,
			valid_from: Option<Timeslot>,
			valid_for: Timeslot,
		) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			Self::ensure_active()?;
			ensure!(target != T::ParachainService::get(), Error::<T>::TargetIsParachainService);
			ensure!(amount >= T::MinTransfer::get(), Error::<T>::BelowMinTransfer);
			let valid_from = valid_from.unwrap_or_else(Self::now);
			let expires_at = valid_from.saturating_add(valid_for);
			let id = LastAllocationId::<T>::mutate(|last| {
				*last = last.saturating_add(1);
				*last
			});
			Allocations::<T>::insert(
				id,
				Allocation {
					id,
					mode: AllocationMode::Permanent { memo },
					target,
					amount,
					state: AllocationState::Approved,
					delivered_at: None,
					target_reclaim_from: None,
					approver: None,
					valid_from,
					expires_at,
				},
			);
			Self::deposit_event(Event::ReleaseApproved { id, target, amount });
			Ok(())
		}

		/// Executes an approved release: burns the amount from custody, sends a `Delivery`
		/// operation and moves the allocation to `Delivering`.
		///
		/// Origin: any signed account, which pays the delivery fee. Legal while the allocation is
		/// `Approved` and within its validity window. Rejected with `InsufficientBalance` when
		/// custody cannot cover the amount.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::execute_release())]
		pub fn execute_release(origin: OriginFor<T>, id: AllocationId) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::do_execute_release(&who, id).map(|_| ())
		}

		/// Moves `amount` of the caller's own units to `target`: burns the amount from the caller
		/// and sends a `Redemption` operation.
		///
		/// Origin: any signed account, which pays the delivery fee. `memo` goes to `target`
		/// unchanged. Rejected when `target` is the Parachain Service or `amount` is below
		/// `MinTransfer`, and with `InsufficientBalance` when the caller cannot cover `amount`.
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::redeem())]
		pub fn redeem(
			origin: OriginFor<T>,
			amount: T::Balance,
			target: ServiceId,
			memo: Memo,
		) -> DispatchResult {
			let holder = ensure_signed(origin)?;
			Self::do_redeem(holder, amount, target, memo).map(|_| ())
		}

		/// Cancels an allocation in `Approved`: it moves to `Closed`.
		///
		/// Origin: the allocation's approver, or any signed account from `expires_at` on.
		/// Governance is the approver of a permanent release.
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::cancel_allocation())]
		pub fn cancel_allocation(origin: OriginFor<T>, id: AllocationId) -> DispatchResult {
			let signer = ensure_signed(origin.clone()).ok();
			let governance = T::GovernanceOrigin::try_origin(origin).is_ok();
			ensure!(governance || signer.is_some(), DispatchError::BadOrigin);
			Self::do_cancel_allocation(signer.as_ref(), governance, id)
		}

		/// Settles a resolved operation: retries a failed settlement, or settles one ahead of
		/// `on_poll`.
		///
		/// Origin: any signed account. Rejected with `OutOfOrder` unless the operation is the
		/// earliest unsettled one of its target, and with `NotResolved` while it is `Requested`.
		/// When the operation blocked its target, the target's later resolved operations are
		/// listed for `on_poll` again.
		#[pallet::call_index(9)]
		#[pallet::weight(
			T::WeightInfo::settle_move(T::MaxTransfersPerBucket::get())
				.max(T::WeightInfo::settle())
				.saturating_add(
					T::DbWeight::get().reads_writes(T::MaxUnsettledOperations::get().into(), 2)
				)
		)]
		pub fn settle(origin: OriginFor<T>, op_id: OperationId) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;
			Self::ensure_attested()?;
			let operation = Operations::<T>::get(op_id).ok_or(Error::<T>::UnknownOperation)?;
			let target = operation.target;
			ensure!(
				TargetOperations::<T>::get(target).first() == Some(&op_id),
				Error::<T>::OutOfOrder
			);
			ensure!(operation.state != OperationState::Requested, Error::<T>::NotResolved);
			Self::apply_settlement(&operation)?;
			Self::remove_settled(op_id, target);
			Resolved::<T>::mutate(|list| list.retain(|(id, _)| *id != op_id));
			let mut actual = Self::settlement_weight(&operation);
			if Blocked::<T>::get(target) == Some(op_id) {
				Blocked::<T>::remove(target);
				// The target's resolved operations waited unlisted; they are next in line.
				let queue = TargetOperations::<T>::get(target);
				// The queue and each of its operations are read; `Blocked` and `Resolved` are
				// written.
				actual = actual
					.saturating_add(T::DbWeight::get().reads_writes(queue.len() as u64 + 1, 2));
				for id in queue {
					let resolved = Operations::<T>::get(id)
						.is_some_and(|op| op.state != OperationState::Requested);
					if resolved &&
						Resolved::<T>::mutate(|list| list.try_push((id, target))).is_err()
					{
						defensive!("resolved operations exceed their bound");
					}
				}
			}
			Ok(Some(actual).into())
		}

		/// Corrects the Asset Hub supply after an unrecorded return on JAM (design §2): mints
		/// `amount` to `beneficiary`, or books it as `Unclaimed` when `beneficiary` cannot receive
		/// it.
		///
		/// Origin: governance. `anchor` is the JAM block whose balances show the difference.
		#[pallet::call_index(10)]
		#[pallet::weight(T::WeightInfo::force_mint())]
		pub fn force_mint(
			origin: OriginFor<T>,
			anchor: Anchor,
			beneficiary: T::AccountId,
			amount: T::Balance,
		) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			Self::ensure_attested()?;
			Self::mint_or_book(&beneficiary, amount)?;
			Self::deposit_event(Event::ForceMinted { anchor, beneficiary, amount });
			Ok(())
		}

		/// Mints `beneficiary`'s unclaimed balance to it.
		///
		/// Origin: any signed account. Rejected with `CannotCreate` when `beneficiary` has no
		/// JAMKB account and no native balance to create one; the unclaimed balance stays booked.
		#[pallet::call_index(11)]
		#[pallet::weight(T::WeightInfo::claim())]
		pub fn claim(origin: OriginFor<T>, beneficiary: T::AccountId) -> DispatchResult {
			ensure_signed(origin)?;
			Self::do_claim(beneficiary).map(|_| ())
		}

		/// Disposes `amount` of the excess booked for `source`.
		///
		/// Origin: governance. Lowers `Excess[source]` by `amount`. With `refund` it records a
		/// `Refund` operation sending the units back to `source`'s regular balance with `memo`
		/// unchanged. Without `refund` it mints the units to custody and `memo` is ignored.
		#[pallet::call_index(12)]
		#[pallet::weight(T::WeightInfo::dispose_excess())]
		pub fn dispose_excess(
			origin: OriginFor<T>,
			source: ServiceId,
			amount: T::Balance,
			refund: bool,
			memo: Memo,
		) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			if refund {
				Self::ensure_active()?;
				ensure!(amount >= T::MinTransfer::get(), Error::<T>::BelowMinTransfer);
			} else {
				Self::ensure_attested()?;
			}
			Excess::<T>::try_mutate_exists(source, |entry| {
				let left = entry
					.unwrap_or_default()
					.checked_sub(&amount)
					.ok_or(Error::<T>::InsufficientExcess)?;
				*entry = (!left.is_zero()).then_some(left);
				Ok::<_, Error<T>>(())
			})?;
			if refund {
				Self::send_operation(
					OperationPayload::Refund { source, amount, memo },
					source,
					None,
					None,
					|id| Self::transfer_out(source, amount, id, Some(memo)),
				)?;
			} else if !amount.is_zero() {
				T::Assets::mint_into(T::JamkbAssetId::get(), &Self::custody_account(), amount)?;
			}
			Self::deposit_event(Event::ExcessDisposed { source, amount });
			Ok(())
		}

		/// Sends the transfers of the failed `Move` `id` again, as a new `Move` operation. They
		/// are booked when it is confirmed.
		///
		/// Origin: governance. No fee is charged. Rejected with `UnknownMove` unless `id` is a
		/// `Move` whose failure was settled and not retried yet.
		#[pallet::call_index(13)]
		#[pallet::weight(T::WeightInfo::retry_move())]
		pub fn retry_move(origin: OriginFor<T>, id: OperationId) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			Self::ensure_active()?;
			let transfers = Unmoved::<T>::take(id).ok_or(Error::<T>::UnknownMove)?;
			Self::send_move(transfers)?;
			Ok(())
		}

		/// Resolves an operation the transport never reported: `Failed` with `failure`, or
		/// `Confirmed` when `failure` is `None`, accumulated at the current timeslot. It then
		/// settles as a reported operation does, and a later report for it is ignored.
		///
		/// Origin: governance. Rejected with `OutOfOrder` unless the operation is `Requested`
		/// and the earliest `Requested` one of its target. The transport still counts the
		/// operation as in flight, so a later send naming the same target may be refused with
		/// `TransportConflict`.
		#[pallet::call_index(14)]
		#[pallet::weight(T::WeightInfo::force_resolve())]
		pub fn force_resolve(
			origin: OriginFor<T>,
			id: OperationId,
			failure: Option<FailureReason>,
		) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			Self::ensure_attested()?;
			let operation = Operations::<T>::get(id).ok_or(Error::<T>::UnknownOperation)?;
			let earliest_requested =
				TargetOperations::<T>::get(operation.target).into_iter().find(|op| {
					Operations::<T>::get(op).is_some_and(|o| o.state == OperationState::Requested)
				});
			ensure!(earliest_requested == Some(id), Error::<T>::OutOfOrder);
			Self::resolve(id, Self::now(), |_| failure);
			Self::deposit_event(Event::ForceResolved { id });
			Ok(())
		}

		/// Burns `amount` from custody, to correct the supply after an unrecorded outflow on
		/// JAM (design §2).
		///
		/// Origin: governance. `anchor` is the JAM block whose balances show the difference.
		/// Rejected with the assets pallet's error when custody cannot cover `amount`.
		#[pallet::call_index(15)]
		#[pallet::weight(T::WeightInfo::force_burn())]
		pub fn force_burn(
			origin: OriginFor<T>,
			anchor: Anchor,
			amount: T::Balance,
		) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			Self::ensure_attested()?;
			T::Assets::burn_from(
				T::JamkbAssetId::get(),
				&Self::custody_account(),
				amount,
				Preservation::Expendable,
				Precision::Exact,
				Fortitude::Polite,
			)?;
			Self::deposit_event(Event::ForceBurned { anchor, amount });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// The custody account: undistributed DAO units.
		pub fn custody_account() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}

		/// The allocation with `id`, if any.
		pub fn allocation(id: AllocationId) -> Option<AllocationOf<T>> {
			Allocations::<T>::get(id)
		}

		/// The unsettled operation with `id`, if any.
		pub fn operation(id: OperationId) -> Option<OperationOf<T>> {
			Operations::<T>::get(id)
		}

		/// The current JAM timeslot.
		pub fn now() -> Timeslot {
			T::TimeslotProvider::current_block_number()
		}

		/// Shared by `execute_release` and the precompile. Returns the new operation's id.
		/// Must run inside a storage layer.
		pub fn do_execute_release(
			who: &T::AccountId,
			id: AllocationId,
		) -> Result<OperationId, DispatchError> {
			Self::ensure_active()?;
			let mut allocation = Allocations::<T>::get(id).ok_or(Error::<T>::UnknownAllocation)?;
			ensure!(allocation.state == AllocationState::Approved, Error::<T>::WrongState);
			ensure!(PendingOperationOf::<T>::get(id).is_none(), Error::<T>::UnsettledOperation);
			let now = Self::now();
			ensure!(now >= allocation.valid_from, Error::<T>::NotYetValid);
			ensure!(now < allocation.expires_at, Error::<T>::Expired);
			let AllocationMode::Permanent { memo } = allocation.mode;
			let (target, amount) = (allocation.target, allocation.amount);
			Self::burn(&Self::custody_account(), amount)?;
			let op_id = Self::send_operation(
				OperationPayload::Delivery { allocation: id, amount },
				target,
				Some(id),
				Some(who),
				|message_id| Self::transfer_out(target, amount, message_id, Some(memo)),
			)?;
			allocation.state = AllocationState::Delivering;
			Allocations::<T>::insert(id, allocation);
			PendingOperationOf::<T>::insert(id, op_id);
			Ok(op_id)
		}

		/// Shared by `redeem` and the precompile. Returns the new operation's id.
		/// Must run inside a storage layer.
		pub fn do_redeem(
			holder: T::AccountId,
			amount: T::Balance,
			target: ServiceId,
			memo: Memo,
		) -> Result<OperationId, DispatchError> {
			Self::ensure_active()?;
			ensure!(target != T::ParachainService::get(), Error::<T>::TargetIsParachainService);
			ensure!(amount >= T::MinTransfer::get(), Error::<T>::BelowMinTransfer);
			Self::burn(&holder, amount)?;
			let payer = holder.clone();
			Self::send_operation(
				OperationPayload::Redemption { holder, target, amount, memo },
				target,
				None,
				Some(&payer),
				|message_id| Self::transfer_out(target, amount, message_id, Some(memo)),
			)
		}

		/// Shared by `cancel_allocation` and the precompile. `signer` is the signing account,
		/// if any; `governance` whether the origin is governance.
		/// Must run inside a storage layer.
		pub fn do_cancel_allocation(
			signer: Option<&T::AccountId>,
			governance: bool,
			id: AllocationId,
		) -> DispatchResult {
			Self::ensure_attested()?;
			let allocation = Allocations::<T>::get(id).ok_or(Error::<T>::UnknownAllocation)?;
			ensure!(allocation.state == AllocationState::Approved, Error::<T>::WrongState);
			let is_approver = match &allocation.approver {
				None => governance,
				Some(approver) => signer == Some(approver),
			};
			ensure!(
				is_approver || (signer.is_some() && Self::now() >= allocation.expires_at),
				DispatchError::BadOrigin
			);
			Allocations::<T>::mutate(id, |a| {
				if let Some(a) = a {
					a.state = AllocationState::Closed;
				}
			});
			Self::deposit_event(Event::AllocationCancelled { id });
			Ok(())
		}

		/// Shared by `claim` and the precompile. Returns the amount minted.
		/// Must run inside a storage layer.
		pub fn do_claim(beneficiary: T::AccountId) -> Result<T::Balance, DispatchError> {
			Self::ensure_attested()?;
			let amount = Unclaimed::<T>::get(&beneficiary);
			ensure!(!amount.is_zero(), Error::<T>::NothingToClaim);
			let asset = T::JamkbAssetId::get();
			match T::Assets::can_deposit(asset.clone(), &beneficiary, amount, Provenance::Minted) {
				DepositConsequence::CannotCreate => Err(Error::<T>::CannotCreate.into()),
				other => other.into_result(),
			}?;
			T::Assets::mint_into(asset, &beneficiary, amount)?;
			Unclaimed::<T>::remove(&beneficiary);
			Self::deposit_event(Event::Claimed { beneficiary, amount });
			Ok(amount)
		}

		fn ensure_attested() -> DispatchResult {
			ensure!(Attestation::<T>::exists(), Error::<T>::NotAttested);
			Ok(())
		}

		fn ensure_active() -> DispatchResult {
			Self::ensure_attested()?;
			ensure!(!Paused::<T>::get(), Error::<T>::Paused);
			Ok(())
		}

		/// Burns `amount` from `who`, down to its whole balance. Rejected with
		/// `InsufficientBalance` when `who` cannot cover it.
		fn burn(who: &T::AccountId, amount: T::Balance) -> DispatchResult {
			let asset = T::JamkbAssetId::get();
			let available = T::Assets::reducible_balance(
				asset.clone(),
				who,
				Preservation::Expendable,
				Fortitude::Polite,
			);
			ensure!(available >= amount, Error::<T>::InsufficientBalance);
			T::Assets::burn_from(
				asset,
				who,
				amount,
				Preservation::Expendable,
				Precision::Exact,
				Fortitude::Polite,
			)?;
			Ok(())
		}

		/// Mints `amount` to `who`. When `who` cannot receive it (no JAMKB account and none can
		/// be created, below the minimum balance, or blocked), books the amount as `Unclaimed`
		/// for it instead. Fails when the mint cannot happen for anyone: the supply would
		/// overflow, or the asset is gone. Returns whether it was minted.
		fn mint_or_book(who: &T::AccountId, amount: T::Balance) -> Result<bool, DispatchError> {
			if amount.is_zero() {
				return Ok(true);
			}
			let asset = T::JamkbAssetId::get();
			match T::Assets::can_deposit(asset.clone(), who, amount, Provenance::Minted) {
				DepositConsequence::Success => {},
				DepositConsequence::CannotCreate |
				DepositConsequence::BelowMinimum |
				DepositConsequence::Blocked => {
					Unclaimed::<T>::try_mutate(who, |b| {
						*b = b.checked_add(&amount).ok_or(ArithmeticError::Overflow)?;
						Ok::<_, DispatchError>(())
					})?;
					Self::deposit_event(Event::UnclaimedBooked {
						beneficiary: who.clone(),
						amount,
					});
					return Ok(false);
				},
				consequence => consequence.into_result()?,
			}
			T::Assets::mint_into(asset, who, amount)?;
			Ok(true)
		}

		/// A `TransferOut` from the Parachain Service's regular balance to `dest`'s regular
		/// balance; deferred with `MemoGas` when it carries a memo.
		fn transfer_out(
			dest: ServiceId,
			amount: T::Balance,
			id: MessageId,
			memo: Option<Memo>,
		) -> UpwardMessage<T::Balance> {
			UpwardMessage::TransferOut {
				source: None,
				dest,
				amount,
				id,
				source_supervisor_balance: false,
				dest_supervisor_balance: false,
				deferred: memo.map(|memo| (memo, T::MemoGas::get())),
			}
		}

		/// Queues the message `build` makes as a new operation on `target`, charges `payer` its
		/// delivery fee, and records it as `Requested`.
		///
		/// The callers run in a storage layer: dispatchables are transactional,
		/// `on_incoming_transfers` wraps its call, and the precompile runs in pallet-revive's
		/// transaction. So a failure after `send` rolls the queued operation back.
		fn send_operation(
			payload: OperationPayloadOf<T>,
			target: ServiceId,
			allocation: Option<AllocationId>,
			payer: Option<&T::AccountId>,
			build: impl FnOnce(MessageId) -> UpwardMessage<T::Balance>,
		) -> Result<OperationId, DispatchError> {
			let user = payload.is_user();
			ensure!(
				!user || UserOperations::<T>::get() < T::MaxUserOperations::get(),
				Error::<T>::TooManyUserOperations
			);
			ensure!(
				Operations::<T>::count() < T::MaxUnsettledOperations::get(),
				Error::<T>::TooManyOperations
			);
			let message_id = T::MessageSender::next_message_id();
			let messages = vec![build(message_id)];
			if let Some(payer) = payer {
				T::DeliveryFee::charge(payer, &messages)?;
			}
			let id =
				T::MessageSender::send(T::SenderId::get(), messages).map_err(Error::<T>::from)?;
			TargetOperations::<T>::try_mutate(target, |queue| queue.try_push(id))
				.map_err(|_| Error::<T>::TooManyOperations)?;
			Operations::<T>::insert(
				id,
				Operation {
					id,
					payload,
					state: OperationState::Requested,
					accumulated_at: None,
					target,
					message_id,
				},
			);
			if user {
				UserOperations::<T>::mutate(|n| *n = n.saturating_add(1));
			}
			Self::deposit_event(Event::OperationRequested { id, allocation });
			Ok(id)
		}

		/// The Asset Hub beneficiary a voluntary return's memo names: the account in its first 32
		/// bytes, the rest zero. `None` for a missing (all zero) or malformed memo.
		fn beneficiary(memo: &Memo) -> Option<T::AccountId> {
			if memo.iter().all(|b| *b == 0) || memo[32..].iter().any(|b| *b != 0) {
				return None;
			}
			let mut input = &memo[..32];
			let who = T::AccountId::decode(&mut input).ok()?;
			input.iter().all(|b| *b == 0).then_some(who)
		}

		/// Books one incoming transfer: mints it to the beneficiary its memo names, or books it as
		/// `Excess` for its source.
		fn book(transfer: &IncomingTransfer<T::Balance>) -> DispatchResult {
			let IncomingTransfer { source, amount, memo, .. } = transfer;
			let class = match Self::beneficiary(memo) {
				Some(who) => match Self::mint_or_book(&who, *amount)? {
					true => ReturnClass::Credited(who),
					false => ReturnClass::Unclaimed(who),
				},
				None => {
					Self::add_excess(*source, *amount)?;
					ReturnClass::Excess
				},
			};
			Self::deposit_event(Event::ReturnBooked {
				source: *source,
				amount: *amount,
				class,
				memo: *memo,
			});
			Ok(())
		}

		/// Adds `amount` to `Excess[source]`.
		fn add_excess(source: ServiceId, amount: T::Balance) -> DispatchResult {
			Excess::<T>::try_mutate(source, |b| {
				*b = b.checked_add(&amount).ok_or(ArithmeticError::Overflow)?;
				Ok(())
			})
		}

		/// The sum of `transfers`. Fails rather than saturate, so that a `Move` never carries
		/// the wrong amount.
		fn sum_of(transfers: &[IncomingTransfer<T::Balance>]) -> Result<T::Balance, DispatchError> {
			transfers.iter().try_fold(T::Balance::zero(), |acc, t| {
				acc.checked_add(&t.amount).ok_or_else(|| ArithmeticError::Overflow.into())
			})
		}

		/// Books a bucket: the transfers to the regular balance at once, those to the supervisor
		/// balance by one `Move` of their sum.
		fn book_bucket(
			transfers: IncomingTransfers<T::Balance, T::MaxTransfersPerBucket>,
		) -> DispatchResult {
			for transfer in transfers.iter().filter(|t| !t.to_supervisor_balance) {
				Self::book(transfer)?;
			}
			let mut to_move = transfers;
			to_move.retain(|t| t.to_supervisor_balance);
			if to_move.is_empty() {
				return Ok(());
			}
			Self::send_move(to_move)?;
			Ok(())
		}

		/// Sends one `Move` of `transfers` from the Parachain Service's supervisor balance to
		/// its regular balance. No fee is charged. Returns the new operation's id.
		fn send_move(
			transfers: BoundedVec<IncomingTransfer<T::Balance>, T::MaxTransfersPerBucket>,
		) -> Result<OperationId, DispatchError> {
			let sum = Self::sum_of(&transfers)?;
			let parachain_service = T::ParachainService::get();
			Self::send_operation(
				OperationPayload::Move { transfers },
				parachain_service,
				None,
				None,
				|id| UpwardMessage::TransferOut {
					source: None,
					dest: parachain_service,
					amount: sum,
					id,
					source_supervisor_balance: true,
					dest_supervisor_balance: false,
					deferred: None,
				},
			)
		}

		/// Applies the outcome of a resolved operation.
		fn apply_settlement(operation: &OperationOf<T>) -> DispatchResult {
			match operation.state {
				OperationState::Confirmed(_) => Self::confirm(operation),
				OperationState::Failed(_) => Self::undo(operation),
				OperationState::Requested => Err(Error::<T>::NotResolved.into()),
			}
		}

		fn confirm(operation: &OperationOf<T>) -> DispatchResult {
			match &operation.payload {
				OperationPayload::Delivery { allocation, .. } => {
					Allocations::<T>::mutate(allocation, |a| {
						if let Some(a) = a {
							a.state = AllocationState::Delivered;
							a.delivered_at = operation.accumulated_at;
						}
					});
					PendingOperationOf::<T>::remove(allocation);
				},
				OperationPayload::Redemption { .. } | OperationPayload::Refund { .. } => {},
				OperationPayload::Move { transfers } => {
					for transfer in transfers {
						Self::book(transfer)?;
					}
				},
			}
			Ok(())
		}

		fn undo(operation: &OperationOf<T>) -> DispatchResult {
			match &operation.payload {
				OperationPayload::Delivery { allocation, amount } => {
					Self::mint_or_book(&Self::custody_account(), *amount)?;
					Allocations::<T>::mutate(allocation, |a| {
						if let Some(a) = a {
							a.state = AllocationState::Approved;
						}
					});
					PendingOperationOf::<T>::remove(allocation);
				},
				OperationPayload::Redemption { holder, amount, .. } => {
					Self::mint_or_book(holder, *amount)?;
				},
				OperationPayload::Move { transfers } => {
					let amount = Self::sum_of(transfers)?;
					Unmoved::<T>::insert(operation.id, transfers);
					Self::deposit_event(Event::MoveFailed { id: operation.id, amount });
				},
				OperationPayload::Refund { source, amount, .. } => {
					Self::add_excess(*source, *amount)?;
				},
			}
			Ok(())
		}

		/// Resolves the `Requested` operation `id` as accumulated at `accumulated_at`: `Failed`
		/// with the reason `failure` finds, `Confirmed` when it finds none. Emits the outcome
		/// event and lists the operation for `on_poll` unless its target is blocked. Does nothing
		/// for an unknown or already resolved operation. Returns whether it resolved it.
		fn resolve(
			id: OperationId,
			accumulated_at: Timeslot,
			failure: impl FnOnce(&OperationOf<T>) -> Option<FailureReason>,
		) -> bool {
			let resolved = Operations::<T>::mutate(id, |op| {
				let op = op.as_mut()?;
				if op.state != OperationState::Requested {
					return None;
				}
				let error = failure(op);
				op.accumulated_at = Some(accumulated_at);
				match error {
					Some(error) => {
						op.state = OperationState::Failed(error);
						Self::deposit_event(Event::OperationFailed { id, error });
					},
					None => {
						op.state = OperationState::Confirmed(None);
						Self::deposit_event(Event::OperationConfirmed { id, entry: None });
					},
				}
				Some(op.target)
			});
			let Some(target) = resolved else { return false };
			// A blocked target's operations wait unlisted until `settle` unblocks it.
			if !Blocked::<T>::contains_key(target) &&
				Resolved::<T>::mutate(|list| list.try_push((id, target))).is_err()
			{
				// Only if `MaxUnsettledOperations` was lowered below the unsettled operations.
				defensive!("resolved operations exceed their bound");
			}
			true
		}

		/// Removes a settled operation: from the operations and from the front of its target's
		/// queue.
		fn remove_settled(id: OperationId, target: ServiceId) {
			if Operations::<T>::take(id).is_some_and(|op| op.payload.is_user()) {
				UserOperations::<T>::mutate(|n| *n = n.saturating_sub(1));
			}
			TargetOperations::<T>::mutate_exists(target, |queue| {
				let Some(q) = queue else { return };
				if q.first() == Some(&id) {
					q.remove(0);
				} else {
					defensive!("a settled operation is not the earliest of its target");
					q.retain(|op| *op != id);
				}
				if q.is_empty() {
					*queue = None;
				}
			});
		}

		/// The weight of settling `operation`: a `Move` by the number of its transfers.
		fn settlement_weight(operation: &OperationOf<T>) -> Weight {
			match &operation.payload {
				OperationPayload::Move { transfers } => {
					T::WeightInfo::settle_move(transfers.len() as u32)
				},
				_ => T::WeightInfo::settle(),
			}
		}

		/// Settles `operation` in its own storage layer. On failure the layer is rolled back,
		/// the target is blocked and the failure is reported. Returns whether it settled.
		fn try_settle(operation: &OperationOf<T>) -> bool {
			let (id, target) = (operation.id, operation.target);
			match with_storage_layer(|| Self::apply_settlement(operation)) {
				Ok(()) => {
					Self::remove_settled(id, target);
					true
				},
				Err(error) => {
					Blocked::<T>::insert(target, id);
					Self::deposit_event(Event::SettlementFailed { id, error });
					false
				},
			}
		}

		/// Settles the resolved operations in send order, as long as `meter` has room. A
		/// settlement that fails blocks its target: its later operations leave the list and wait
		/// for `settle`.
		fn settle_resolved(meter: &mut WeightMeter) {
			if meter.try_consume(T::DbWeight::get().reads(1)).is_err() {
				return;
			}
			let resolved = Resolved::<T>::get();
			if resolved.is_empty() {
				return;
			}
			if meter.try_consume(T::DbWeight::get().writes(1)).is_err() {
				return;
			}
			let mut blocked: Vec<ServiceId> = Vec::new();
			let mut left = Vec::new();
			for (id, target) in resolved {
				if blocked.contains(&target) {
					continue;
				}
				if !left.is_empty() {
					left.push((id, target));
					continue;
				}
				if meter.try_consume(T::DbWeight::get().reads(1)).is_err() {
					left.push((id, target));
					continue;
				}
				let Some(operation) = Operations::<T>::get(id) else {
					defensive!("a listed operation is missing");
					continue;
				};
				if meter.try_consume(Self::settlement_weight(&operation)).is_err() {
					left.push((id, target));
					continue;
				}
				if !Self::try_settle(&operation) {
					blocked.push(target);
				}
			}
			// What is left is part of a bounded list, so it fits the bound.
			Resolved::<T>::put(BoundedVec::truncate_from(left));
		}

		/// Checks the invariants the pallet maintains. Fails when they do not hold.
		#[cfg(any(feature = "try-runtime", test))]
		pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
			// A delivery operation and its delivering allocation point at each other.
			for (allocation, op_id) in PendingOperationOf::<T>::iter() {
				let op = Operations::<T>::get(op_id).ok_or("pending operation is missing")?;
				let belongs = matches!(
					op.payload,
					OperationPayload::Delivery { allocation: a, .. } if a == allocation
				);
				frame_support::ensure!(
					belongs,
					"pending operation does not belong to the allocation"
				);
			}
			for (id, a) in Allocations::<T>::iter() {
				if a.state == AllocationState::Delivering {
					frame_support::ensure!(
						PendingOperationOf::<T>::contains_key(id),
						"delivering allocation has no pending operation"
					);
				}
			}
			for (id, op) in Operations::<T>::iter() {
				if let OperationPayload::Delivery { allocation, .. } = op.payload {
					frame_support::ensure!(
						PendingOperationOf::<T>::get(allocation) == Some(id),
						"delivery operation is not its allocation's pending operation"
					);
					frame_support::ensure!(
						Allocations::<T>::get(allocation).map(|a| a.state) ==
							Some(AllocationState::Delivering),
						"delivery operation's allocation is not delivering"
					);
				}
			}

			// Each operation is queued once, on its own target. Per target, the resolved
			// operations come first; they are listed in queue order, unless the target is blocked
			// by its earliest operation, when none of them is listed.
			frame_support::ensure!(
				Operations::<T>::count() <= T::MaxUnsettledOperations::get(),
				"too many unsettled operations"
			);
			frame_support::ensure!(
				UserOperations::<T>::get() as usize ==
					Operations::<T>::iter_values().filter(|op| op.payload.is_user()).count(),
				"the user operation counter does not match the user operations"
			);
			let listed = Resolved::<T>::get().into_inner();
			let mut queued = 0u32;
			for (target, queue) in TargetOperations::<T>::iter() {
				frame_support::ensure!(!queue.is_empty(), "an empty queue is stored");
				queued += queue.len() as u32;
				let mut resolved = Vec::new();
				let mut requested_seen = false;
				for id in queue.iter() {
					let op = Operations::<T>::get(id).ok_or("queued operation is missing")?;
					frame_support::ensure!(
						op.target == target,
						"operation queued on another target"
					);
					let requested = op.state == OperationState::Requested;
					frame_support::ensure!(
						requested == op.accumulated_at.is_none(),
						"operation state and accumulation timeslot disagree"
					);
					frame_support::ensure!(
						!requested_seen || requested,
						"a resolved operation follows a requested one"
					);
					requested_seen |= requested;
					if !requested {
						resolved.push((*id, target));
					}
				}
				let of_target: Vec<_> =
					listed.iter().filter(|(_, t)| *t == target).cloned().collect();
				match Blocked::<T>::get(target) {
					Some(head) => {
						frame_support::ensure!(
							queue.first() == Some(&head) &&
								resolved.first().map(|r| r.0) == Some(head),
							"a target is blocked by an operation that is not its earliest resolved"
						);
						frame_support::ensure!(of_target.is_empty(), "a blocked target is listed");
					},
					None => frame_support::ensure!(
						of_target == resolved,
						"the listed operations of a target are not its resolved ones in order"
					),
				}
			}
			frame_support::ensure!(
				queued == Operations::<T>::count(),
				"the queues do not hold every operation once"
			);
			frame_support::ensure!(
				listed.iter().all(|(id, _)| Operations::<T>::contains_key(id)),
				"listed operation is missing"
			);
			frame_support::ensure!(
				Blocked::<T>::iter_keys().all(|t| TargetOperations::<T>::contains_key(t)),
				"a target without operations is blocked"
			);
			frame_support::ensure!(
				Unmoved::<T>::iter_values().all(|transfers| !transfers.is_empty()),
				"a failed move without transfers is kept"
			);
			Ok(())
		}
	}

	impl<T: Config> OnJamOutcome for Pallet<T> {
		fn on_outcome(
			sender: SenderId,
			id: OperationId,
			accumulated_at: Timeslot,
			entries: &[AccumulateLog],
			meter: &mut WeightMeter,
		) {
			meter.consume(T::WeightInfo::on_outcome());
			if sender != T::SenderId::get() {
				return;
			}
			Self::resolve(id, accumulated_at, |op| {
				// Only an entry naming this operation's message counts (§5.1).
				entries.iter().find_map(|entry| match entry {
					AccumulateLog::TransferFailed { id, error } if *id == op.message_id => {
						Some(FailureReason::TransferFailed(*error))
					},
					_ => None,
				})
			});
		}
	}

	impl<T: Config> OnIncomingTransfers<T::Balance> for Pallet<T> {
		type MaxTransfers = T::MaxTransfersPerBucket;

		fn capacity() -> u32 {
			if Attestation::<T>::exists() && !Paused::<T>::get() {
				T::MaxBucketsPerBlock::get()
			} else {
				0
			}
		}

		fn on_incoming_transfers(
			transfers: IncomingTransfers<T::Balance, T::MaxTransfersPerBucket>,
			meter: &mut WeightMeter,
		) -> Result<(), ()> {
			let weight = T::WeightInfo::on_incoming_transfers(transfers.len() as u32);
			if Self::capacity() == 0 || meter.try_consume(weight).is_err() {
				return Err(());
			}
			with_storage_layer(|| Self::book_bucket(transfers)).map_err(|_| ())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_poll(_n: BlockNumberFor<T>, meter: &mut WeightMeter) {
			let budget = T::SettlementWeight::get().min(meter.remaining());
			let mut own = WeightMeter::with_limit(budget);
			Self::settle_resolved(&mut own);
			meter.consume(own.consumed());
		}

		fn integrity_test() {
			assert!(!T::MinTransfer::get().is_zero(), "`MinTransfer` must be at least one unit");
			assert!(T::MaxUnsettledOperations::get() > 0, "`MaxUnsettledOperations` is zero");
			assert!(
				T::MaxUserOperations::get() < T::MaxUnsettledOperations::get(),
				"`MaxUserOperations` must be below `MaxUnsettledOperations`"
			);
			assert!(T::MaxTransfersPerBucket::get() > 0, "`MaxTransfersPerBucket` is zero");
			// The list is read and written, then the operation is read and settled.
			let first = T::DbWeight::get().reads_writes(2, 1).saturating_add(
				T::WeightInfo::settle_move(T::MaxTransfersPerBucket::get())
					.max(T::WeightInfo::settle()),
			);
			assert!(
				T::SettlementWeight::get().all_gte(first),
				"`SettlementWeight` must cover at least one settlement"
			);
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()
		}
	}
}
