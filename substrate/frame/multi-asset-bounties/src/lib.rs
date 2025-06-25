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

//! > Made with *Substrate*, for *Polkadot*.
//!
//! [![github]](https://github.com/paritytech/substrate/frame/multi-asset-bounties) -
//! [![polkadot]](https://polkadot.com)
//!
//! [polkadot]: https://img.shields.io/badge/polkadot-E6007A?style=for-the-badge&logo=polkadot&logoColor=white
//! [github]: https://img.shields.io/badge/github-8da0cb?style=for-the-badge&labelColor=555555&logo=github
//!
//!
//! # Multi Asset Bounties Pallet ( `pallet-multi-asset-bounties` )
//!
//! ## Bounty
//!
//! > NOTE: This pallet is tightly coupled with pallet-treasury.
//!
//! A bounty is a reward for completing a specified body of work or achieving a defined set of
//! objectives.  The work must be completed for a predefined amount to be paid out. A curator is
//! assigned when the bounty is funded, and is responsible for awarding the bounty once the
//! objectives are met. To support parallel execution and better governance, a bounty can be split
//! into multiple child bounties. Each child bounty represents a smaller task derived from the
//! parent bounty. The parent bounty curator may assign a separate curator to each child bounty at
//! creation time. The curator may be unassigned, resulting in a new curator election. A bounty can
//! be canceled either before a curator is assigned, while active, or during a pending payout, which
//! results in slashing the curator’s deposit if one was assigned.
//!
//! > NOTE: A parent bounty cannot be closed if it has any active child bounties associated with it.
//!
//! ### Terminology
//!
//! TODO: Add terminology. See example in https://github.com/paritytech/polkadot-sdk/blob/252f3953247c7e9b9776c63cdeee35b4d51e9b24/substrate/frame/treasury/src/lib.rs#L40
//!
//! ### Example
//!
//! TODO: Add examples. See example in https://github.com/paritytech/polkadot-sdk/blob/252f3953247c7e9b9776c63cdeee35b4d51e9b24/substrate/frame/treasury/src/lib.rs#L49C1-L49C16
//!
//! ## Pallet API
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes,
//! including its configuration trait, dispatchables, storage items, events and errors.

#![cfg_attr(not(feature = "std"), no_std)]

mod mock;
mod tests;
pub mod weights;
pub use pallet::*;
pub use weights::WeightInfo;

extern crate alloc;
use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use frame_support::{
	dispatch::{DispatchResult, DispatchResultWithPostInfo},
	dispatch_context::with_context,
	pallet_prelude::*,
	traits::{
		tokens::{ConversionFromAssetBalance, Pay, PaymentStatus},
		EnsureOrigin, Get, OnUnbalanced, ReservableCurrency,
	},
};
use frame_system::pallet_prelude::{
	ensure_signed, BlockNumberFor as SystemBlockNumberFor, OriginFor,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{
		AccountIdConversion, BadOrigin, BlockNumberProvider, Saturating, StaticLookup, TryConvert,
		Zero,
	},
	Permill, RuntimeDebug,
};

type BalanceOf<T, I = ()> = pallet_treasury::BalanceOf<T, I>;
type BeneficiaryLookupOf<T, I = ()> = pallet_treasury::BeneficiaryLookupOf<T, I>;
/// An index of a bounty. Just a `u32`.
pub type BountyIndex = u32;
type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;
type PaymentIdOf<T, I = ()> = <<T as pallet_treasury::Config<I>>::Paymaster as Pay>::Id;
/// Convenience alias for `Bounty`.
pub type BountyOf<T, I> = Bounty<
	<T as frame_system::Config>::AccountId,
	BalanceOf<T, I>,
	BlockNumberFor<T, I>,
	<T as pallet_treasury::Config<I>>::AssetKind,
	PaymentIdOf<T, I>,
	<T as pallet_treasury::Config<I>>::Beneficiary,
>;
type BlockNumberFor<T, I = ()> =
	<<T as pallet_treasury::Config<I>>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;

/// A bounty funded.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct Bounty<AccountId, Balance, BlockNumber, AssetKind, PaymentId, Beneficiary> {
	/// The kind of asset this bounty is rewarded in.
	pub asset_kind: AssetKind,
	/// The (total) amount that should be paid if the bounty is rewarded, including beneficiary
	/// payout and curator fee.
	///
	/// The asset class determined by [`asset_kind`].
	pub value: Balance,
	/// The fee that the parent curator receives upon successful payout.
	///
	/// The asset class determined by [`asset_kind`].
	pub fee: Balance,
	/// The deposit of curator.
	///
	/// The asset class determined by the [`pallet_treasury::Config::Currency`].
	pub curator_deposit: Balance,
	/// The status of this bounty.
	pub status: BountyStatus<AccountId, BlockNumber, PaymentId, Beneficiary>,
}

/// A child-bounty funded.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ChildBounty<AccountId, Balance, BlockNumber, PaymentId, Beneficiary> {
	/// The parent bounty index of this child-bounty.
	pub parent_bounty: BountyIndex,
	/// The (total) amount that should be paid if the child-bounty is rewarded, including
	/// beneficiary payout and child curator fee (of ).
	///
	/// The asset class determined by [`asset_kind`].
	pub value: Balance,
	/// The fee that the parent curator receives upon successful payout.
	///
	/// The asset class determined by [`asset_kind`].
	pub fee: Balance,
	/// The deposit of curator.
	///
	/// The asset class determined by the [`pallet_treasury::Config::Currency`].
	pub curator_deposit: Balance,
	/// The status of this bounty.
	pub status: BountyStatus<AccountId, BlockNumber, PaymentId, Beneficiary>,
}

/// The status of a bounty proposal.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum BountyStatus<AccountId, BlockNumber, PaymentId, Beneficiary> {
	/// The bounty funding has been attempted is waiting to confirm the funds allocation.
	///
	/// Call `check_status` to confirm whether the funding payment succeeded. If successful, the
	/// bounty transitions to `Funded`. Otherwise, use `retry_payment` to reinitialize the funding
	/// transfer.
	FundingAttempted {
		/// The curator of this bounty.
		payment_status: PaymentState<PaymentId>,
	},
	/// The bounty is approved and waiting to confirm the funds allocation.
	Approved {
		/// The status of the bounty amount transfer from the source (e.g. Treasury) to
		/// the bounty account.
		///
		/// Once `check_payment_status` confirms, the bounty will transition to either
		/// [`BountyStatus::Funded`] or [`BountyStatus::ApprovedWithCurator`].
		payment_status: PaymentState<PaymentId>,
	},
	/// The bounty is funded and waiting for curator assignment.
	Funded,
	/// A curator has been proposed. Waiting for acceptance from the curator.
	CuratorProposed {
		/// The assigned curator of this bounty.
		curator: AccountId,
	},
	/// The bounty is active and waiting to be awarded.
	Active {
		/// The curator of this bounty.
		curator: AccountId,
		/// The curator's stash account/location used as a fee destination.
		curator_stash: Beneficiary,
		/// An update from the curator is due by this block, else they are considered inactive.
		update_due: BlockNumber,
	},
	/// The bounty is awarded and waiting to released after a delay.
	PendingPayout {
		/// The curator of this bounty.
		curator: AccountId,
		/// The curator's stash account/location used as a fee destination.
		curator_stash: Beneficiary,
		/// The beneficiary of the bounty.
		beneficiary: Beneficiary,
		/// When the bounty can be claimed.
		unlock_at: BlockNumber,
	},
	/// The bounty is approved with a curator and waiting to confirm the funds allocation.
	ApprovedWithCurator {
		/// The assigned curator of this bounty.
		curator: AccountId,
		/// The status of the bounty amount transfer from the source (e.g. Treasury) to
		/// the bounty account.
		///
		/// Once `check_payment_status` confirms the payment succeeded, the bounty will transition
		/// to [`BountyStatus::CuratorProposed`].
		payment_status: PaymentState<PaymentId>,
	},
	/// The bounty payout has been attempted.
	///
	/// The transfers to both the curator stash and the beneficiary have been initiated.
	/// You can call `process_payment` to retry one or both payments, and `check_payment_status`
	/// to advance each payment’s state. Once `check_payment_status` confirms both payments
	/// succeeded, the bounty is finalized and removed from storage.
	PayoutAttempted {
		/// The curator of this bounty.
		curator: AccountId,
		/// The curator stash account/location with the payout status.
		curator_stash: (Beneficiary, PaymentState<PaymentId>),
		/// The beneficiary stash account/location with the payout status.
		beneficiary: (Beneficiary, PaymentState<PaymentId>),
	},
	/// The bounty is closed, and the funds are being refunded to the original source (e.g.,
	/// Treasury). Once `check_payment_status` confirms the payment succeeded, the bounty is
	/// finalized and removed from storage.
	RefundAttempted {
		/// The curator of this bounty.
		curator: Option<AccountId>,
		/// The refund status.
		///
		/// Once the refund is successful, the bounty is removed from the storage.
		payment_status: PaymentState<PaymentId>,
	},
}

/// The state of payments associated with each bounty and its `BountyStatus`.
///
/// When a payment is initiated using `Paymaster::pay`, the payment enters in a pending state,
/// thus supporting asynchronous payments. Calling `check_payment_status` updates the payment state
/// and advances the bounty lifecycle. The `process_payment` can be called to retry a payment in
/// `Failed` or `Pending` state.
#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
pub enum PaymentState<Id> {
	/// Pending claim.
	Pending,
	/// Payment attempted with a payment identifier.
	Attempted { id: Id },
	/// Payment failed.
	Failed,
	/// Payment succeeded.
	Succeeded,
}
impl<Id: Clone> PaymentState<Id> {
	/// Used to check if payment can be retried.
	pub fn is_pending_or_failed(&self) -> bool {
		matches!(self, PaymentState::Pending | PaymentState::Failed)
	}

	/// If a payment has been initialized, returns its identifier, which is used to check its
	/// status.
	pub fn get_attempt_id(&self) -> Option<Id> {
		match self {
			PaymentState::Attempted { id } => Some(id.clone()),
			_ => None,
		}
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config + pallet_treasury::Config<I> {
		/// The curator deposit is calculated as a percentage of the curator fee.
		///
		/// This deposit has optional upper and lower bounds with `CuratorDepositMax` and
		/// `CuratorDepositMin`.
		#[pallet::constant]
		type CuratorDepositMultiplier: Get<Permill>;

		/// Maximum amount of funds that should be placed in a deposit for making a proposal.
		#[pallet::constant]
		type CuratorDepositMax: Get<Option<BalanceOf<Self, I>>>;

		/// Minimum amount of funds that should be placed in a deposit for making a proposal.
		#[pallet::constant]
		type CuratorDepositMin: Get<Option<BalanceOf<Self, I>>>;

		/// Minimum value for a bounty.
		#[pallet::constant]
		type BountyValueMinimum: Get<BalanceOf<Self, I>>;

		/// The amount held on deposit per byte within the tip report reason or bounty description.
		#[pallet::constant]
		type DataDepositPerByte: Get<BalanceOf<Self, I>>;

		/// The overarching event type.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Maximum acceptable reason length.
		///
		/// Benchmarks depend on this value, be sure to update weights file when changing this
		/// value.
		#[pallet::constant]
		type MaximumReasonLength: Get<u32>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// Handler for the unbalanced decrease when slashing for a rejected bounty.
		type OnSlash: OnUnbalanced<pallet_treasury::NegativeImbalanceOf<Self, I>>;

		/// Helper type for benchmarks.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: benchmarking::ArgumentsFactory<Self::AssetKind, Self::Beneficiary>;
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The reason given is just too big.
		ReasonTooBig,
		/// Invalid bounty value.
		InvalidValue,
		/// The balance of the asset kind is not convertible to the balance of the native asset for
		/// asserting the origin permissions.
		FailedToConvertBalance,
		/// The bounty status is unexpected.
		UnexpectedStatus,
		/// The spend origin is valid but the amount it is allowed to spend is lower than the
		/// requested amount.
		InsufficientPermission,
		/// There was issue with funding the bounty.
		FundingError,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		// New bounty created and funding initiated.
		BountyFunded {
			index: BountyIndex,
		},
		/// New bounty proposal.
		BountyProposed {
			index: BountyIndex,
		},
		/// A bounty proposal was rejected; funds were slashed.
		BountyRejected {
			index: BountyIndex,
			bond: BalanceOf<T, I>,
		},
		/// A bounty proposal is funded and became active.
		BountyBecameActive {
			index: BountyIndex,
		},
		/// A bounty is awarded to a beneficiary.
		BountyAwarded {
			index: BountyIndex,
			beneficiary: T::Beneficiary,
		},
		/// A bounty is claimed by beneficiary.
		BountyClaimed {
			index: BountyIndex,
			beneficiary: T::Beneficiary,
			curator_stash: T::Beneficiary,
		},
		/// Payout payments to the beneficiary and curator stash have concluded successfully.
		BountyPayoutProcessed {
			index: BountyIndex,
			asset_kind: T::AssetKind,
			/// The amount paid to the beneficiary.
			value: BalanceOf<T, I>,
			beneficiary: T::Beneficiary,
		},
		/// A bounty is cancelled.
		BountyCanceled {
			index: BountyIndex,
		},
		/// Refund payment has concluded successfully.
		BountyRefundProcessed {
			index: BountyIndex,
		},
		/// A bounty expiry is extended.
		BountyExtended {
			index: BountyIndex,
		},
		/// A bounty is approved.
		BountyApproved {
			index: BountyIndex,
		},
		/// A bounty curator is proposed.
		CuratorProposed {
			bounty_id: BountyIndex,
			curator: T::AccountId,
		},
		/// A bounty curator is unassigned.
		CuratorUnassigned {
			bounty_id: BountyIndex,
		},
		/// A bounty curator is accepted.
		CuratorAccepted {
			bounty_id: BountyIndex,
			curator: T::AccountId,
		},
		/// A payment failed and can be retried.
		PaymentFailed {
			index: BountyIndex,
			payment_id: PaymentIdOf<T, I>,
		},
		/// A payment happened and can be checked.
		Paid {
			index: BountyIndex,
			payment_id: PaymentIdOf<T, I>,
		},
	}

	/// Number of bounty proposals that have been made.
	#[pallet::storage]
	pub type BountyCount<T: Config<I>, I: 'static = ()> = StorageValue<_, BountyIndex, ValueQuery>;

	/// Bounties that have been made.
	#[pallet::storage]
	pub type Bounties<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BountyIndex, BountyOf<T, I>>;

	/// The description of each bounty.
	#[pallet::storage]
	pub type BountyDescriptions<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BountyIndex, BoundedVec<u8, T::MaximumReasonLength>>;

	/// Temporarily tracks spending limits within the current block to prevent overspending.
	#[derive(Default)]
	pub struct SpendContext<Balance> {
		pub spend_in_context: BTreeMap<Balance, Balance>,
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Fund a new bounty, iniitiating the payment from the treasury to the bounty account.
		///
		/// ## Dispatch Origin
		/// Must be [`Config::SpendOrigin`] with the `Success` value being at least
		/// the converted native amount of the bounty. The bounty value is validated
		/// against the maximum spendable amount of the [`Config::SpendOrigin`].
		///
		/// ## Details
		/// - The `SpendOrigin` must have sufficient permissions to approve the bounty.
		/// - In case of a funding failure, the bounty status must be updated with the
		/// `check_status` call before retrying with `retry_payment` call.
		///
		/// ### Parameters
		/// - `asset_kind`: An indicator of the specific asset class to be funded.
		/// - `value`: The total payment amount of this bounty, curator fee included.
		/// - `description`: The description of this bounty.
		///
		/// ## Events
		/// Emits [`Event::BountyFunded`] if successful.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::propose_bounty(description.len() as u32))]
		pub fn fund_bounty(
			origin: OriginFor<T>,
			asset_kind: Box<T::AssetKind>,
			#[pallet::compact] value: BalanceOf<T, I>,
			description: Vec<u8>,
		) -> DispatchResult {
			let max_amount = T::SpendOrigin::ensure_origin(origin)?;
			let bounded_description: BoundedVec<_, _> =
				description.try_into().map_err(|_| Error::<T, I>::ReasonTooBig)?;

			let index = BountyCount::<T, I>::get();
			let payment_status = Self::do_process_funding_payment(
				index.clone(),
				value.clone(),
				*asset_kind.clone(),
				None,
				Some(max_amount),
			)?;

			let bounty = BountyOf::<T, I> {
				asset_kind: *asset_kind,
				value,
				fee: 0u32.into(),
				curator_deposit: 0u32.into(),
				status: BountyStatus::FundingAttempted { payment_status: payment_status.clone() },
			};
			Bounties::<T, I>::insert(index, &bounty);
			BountyCount::<T, I>::put(index + 1);
			BountyDescriptions::<T, I>::insert(index, bounded_description);

			Self::deposit_event(Event::<T, I>::BountyFunded { index });

			Ok(())
		}

		// TODO: Propose and approve a new bounty and propose a curator simultaneously. This call is
		// a shortcut to calling `fund_bounty` and `propose_curator` separately. Combine
		// `pallet_bounties` `propose_bounty` and `approve_bounty_with_curator` calls.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::approve_bounty_with_curator())]
		pub fn fund_bounty_with_curator(
			origin: OriginFor<T>,
			asset_kind: Box<T::AssetKind>,
			#[pallet::compact] value: BalanceOf<T, I>,
			description: Vec<u8>,
			curator: AccountIdLookupOf<T>,
			#[pallet::compact] fee: BalanceOf<T, I>,
		) -> DispatchResult {
			Ok(())
		}

		// TODO: Same as `pallet_bounties` `propose_curator` call.
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::propose_curator())]
		pub fn propose_curator(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
		) -> DispatchResult {
			Ok(())
		}

		// TODO: Unassign curator from a bounty. Same as `pallet_bounties` `unassign_curator` call
		// without handling Proposed, Approved and ApprovedWithCurator status.
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::unassign_curator())]
		pub fn unassign_curator(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
			curator: AccountIdLookupOf<T>,
			#[pallet::compact] fee: BalanceOf<T, I>,
		) -> DispatchResult {
			Ok(())
		}

		// TODO: Accept the curator role for a bounty. Same as `pallet_bounties` `accept_curator`
		// call.
		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::accept_curator())]
		pub fn accept_curator(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
			stash: BeneficiaryLookupOf<T, I>,
		) -> DispatchResult {
			Ok(())
		}

		// TODO: Award bounty to a beneficiary account/location. Same as `pallet_bounties`
		// `award_bounty` call.
		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::award_bounty())]
		pub fn award_bounty(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
			beneficiary: BeneficiaryLookupOf<T, I>,
		) -> DispatchResult {
			Ok(())
		}

		// TODO: Claim the payout from an awarded bounty. Same as `pallet_bounties` `claim_bounty`
		// call.
		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::claim_bounty())]
		pub fn claim_bounty(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
		) -> DispatchResult {
			Ok(())
		}

		// TODO: Cancel an active bounty. Same as `pallet_bounties` `close_bounty` call without
		// handling Proposed, Approved and ApprovedWithCurator status.
		#[pallet::call_index(7)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::close_bounty_proposed()
			.max(<T as Config<I>>::WeightInfo::close_bounty_active()))]
		pub fn close_bounty(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
		) -> DispatchResultWithPostInfo {
			Ok(Some(<T as Config<I>>::WeightInfo::close_bounty_proposed()).into())
		}

		// TODO: Extend the expiry time of an active bounty. Same as `pallet_bounties`
		// `extend_bounty_expiry` call.
		#[pallet::call_index(8)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::extend_bounty_expiry())]
		pub fn extend_bounty_expiry(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
			_remark: Vec<u8>,
		) -> DispatchResult {
			Ok(())
		}

		// TODO: Retry a payment for funding, refund or payout of a bounty. Same as `pallet_bounties` `process_payment` call in https://github.com/paritytech/polkadot-sdk/blob/252f3953247c7e9b9776c63cdeee35b4d51e9b24/substrate/frame/bounties/src/lib.rs#L1212.
		#[pallet::call_index(9)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::approve_bounty_with_curator())]
		pub fn process_payment(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
		) -> DispatchResultWithPostInfo {
			Ok(Some(<T as Config<I>>::WeightInfo::approve_bounty_with_curator()).into())
		}

		// TODO: Check and update the payment status of a bounty. Same as `pallet_bounties` `check_payment_status` call in https://github.com/paritytech/polkadot-sdk/blob/252f3953247c7e9b9776c63cdeee35b4d51e9b24/substrate/frame/bounties/src/lib.rs#L1323C10-L1323C30.
		#[pallet::call_index(10)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::approve_bounty_with_curator())]
		pub fn check_payment_status(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
		) -> DispatchResultWithPostInfo {
			Ok(Some(<T as Config<I>>::WeightInfo::approve_bounty_with_curator()).into())
		}
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<SystemBlockNumberFor<T>> for Pallet<T, I> {
		#[cfg(feature = "try-runtime")]
		fn try_state(_n: SystemBlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()
		}
	}
}

#[cfg(any(feature = "try-runtime", test))]
impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Ensure the correctness of the state of this pallet.
	///
	/// This should be valid before or after each state transition of this pallet.
	pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		Self::try_state_bounties_count()?;

		Ok(())
	}

	/// # Invariants
	///
	/// * `BountyCount` should be greater or equals to the length of the number of items in
	///   `Bounties`.
	/// * `BountyCount` should be greater or equals to the length of the number of items in
	///   `BountyDescriptions`.
	/// * Number of items in `Bounties` should be the same as `BountyDescriptions` length.
	fn try_state_bounties_count() -> Result<(), sp_runtime::TryRuntimeError> {
		let bounties_length = Bounties::<T, I>::iter().count() as u32;

		ensure!(
			<BountyCount<T, I>>::get() >= bounties_length,
			"`BountyCount` must be grater or equals the number of `Bounties` in storage"
		);

		let bounties_description_length = BountyDescriptions::<T, I>::iter().count() as u32;
		ensure!(
			<BountyCount<T, I>>::get() >= bounties_description_length,
			"`BountyCount` must be grater or equals the number of `BountiesDescriptions` in storage."
		);

		ensure!(
				bounties_length == bounties_description_length,
				"Number of `Bounties` in storage must be the same as the Number of `BountiesDescription` in storage."
		);
		Ok(())
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Get the block number used in the treasury pallet.
	///
	/// It may be configured to use the relay chain block number on a parachain.
	pub fn treasury_block_number() -> BlockNumberFor<T, I> {
		<T as pallet_treasury::Config<I>>::BlockNumberProvider::current_block_number()
	}

	/// Calculate the deposit required for a curator.
	pub fn calculate_curator_deposit(
		fee: &BalanceOf<T, I>,
		asset_kind: T::AssetKind,
	) -> Result<BalanceOf<T, I>, Error<T, I>> {
		let fee = <T as pallet_treasury::Config<I>>::BalanceConverter::from_asset_balance(
			*fee, asset_kind,
		)
		.map_err(|_| Error::<T, I>::FailedToConvertBalance)?;

		let mut deposit = T::CuratorDepositMultiplier::get() * fee;

		if let Some(max_deposit) = T::CuratorDepositMax::get() {
			deposit = deposit.min(max_deposit)
		}

		if let Some(min_deposit) = T::CuratorDepositMin::get() {
			deposit = deposit.max(min_deposit)
		}

		Ok(deposit)
	}

	/// The account/location ID of the treasury pot.
	///
	/// This actually does computation. If you need to keep using it, then make sure you cache the
	/// value and only call this once.
	pub fn account_id() -> T::Beneficiary {
		T::PalletId::get().into_account_truncating()
	}

	/// The account ID of a bounty account
	pub fn bounty_account_id(id: BountyIndex) -> T::Beneficiary {
		// only use two byte prefix to support 16 byte account id (used by test)
		// "modl" ++ "py/trsry" ++ "bt" is 14 bytes, and two bytes remaining for bounty index
		T::PalletId::get().into_sub_account_truncating(("bt", id))
	}

	/// Initializes payment from the treasury pot to the bounty account/location.
	fn do_process_funding_payment(
		bounty_id: BountyIndex,
		value: BalanceOf<T, I>,
		asset_kind: T::AssetKind,
		maybe_payment_status: Option<PaymentState<PaymentIdOf<T, I>>>,
		maybe_max_amount: Option<BalanceOf<T, I>>,
	) -> Result<PaymentState<PaymentIdOf<T, I>>, DispatchError> {
		if let Some(payment_status) = maybe_payment_status {
			ensure!(payment_status.is_pending_or_failed(), Error::<T, I>::UnexpectedStatus);
		}

		println!("max_amount: {:?}", maybe_max_amount);
		if let Some(max_amount) = maybe_max_amount {
			let native_amount = T::BalanceConverter::from_asset_balance(value, asset_kind.clone())
				.map_err(|_| Error::<T, I>::FailedToConvertBalance)?;
			println!("native_amount: {:?}", native_amount);
			ensure!(native_amount >= T::BountyValueMinimum::get(), Error::<T, I>::InvalidValue);
			ensure!(native_amount <= max_amount, Error::<T, I>::InsufficientPermission);

			with_context::<SpendContext<BalanceOf<T, I>>, _>(|v| {
				let context = v.or_default();
				let spend = context.spend_in_context.entry(max_amount).or_default();
				println!("spend: {:?}", spend);
				if spend.checked_add(&native_amount).map(|s| s > max_amount).unwrap_or(true) {
					Err(Error::<T, I>::InsufficientPermission)
				} else {
					*spend = spend.saturating_add(native_amount);
					Ok(())
				}
			})
			.unwrap_or(Ok(()))?;
		}

		let bounty_account = Self::bounty_account_id(bounty_id.clone());
		let id =
			<T as pallet_treasury::Config<I>>::Paymaster::pay(&bounty_account, asset_kind, value)
				.map_err(|_| Error::<T, I>::FundingError)?;

		Self::deposit_event(Event::<T, I>::Paid { index: bounty_id, payment_id: id });

		Ok(PaymentState::Attempted { id })
	}
}
