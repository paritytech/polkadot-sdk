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
		tokens::{ConversionFromAssetBalance, PayWithSource, PaymentStatus},
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
type PaymentIdOf<T, I = ()> = <<T as crate::Config<I>>::Paymaster as PayWithSource>::Id;
/// Convenience alias for `Bounty`.
pub type BountyOf<T, I> = Bounty<
	<T as frame_system::Config>::AccountId,
	BalanceOf<T, I>,
	BlockNumberFor<T, I>,
	<T as pallet_treasury::Config<I>>::AssetKind,
	PaymentIdOf<T, I>,
	<T as pallet_treasury::Config<I>>::Beneficiary,
>;
type ChildBountyOf<T, I> = ChildBounty<
	<T as frame_system::Config>::AccountId,
	BalanceOf<T, I>,
	BlockNumberFor<T, I>,
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
	/// The asset class determined by the parent bounty [`asset_kind`].
	pub value: Balance,
	/// The fee that the parent curator receives upon successful payout.
	///
	/// The asset class determined by the parent bounty [`asset_kind`].
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
	/// The child-/bounty funding has been attempted is waiting to confirm the funds allocation.
	///
	/// Call `check_status` to confirm whether the funding payment succeeded. If successful, the
	/// child-/bounty transitions to `Funded`. Otherwise, use `retry_payment` to reinitialize the
	/// funding transfer.
	FundingAttempted {
		/// The proposed curator of this child-/bounty.
		curator: AccountId,
		/// The status of the child-/bounty amount transfer from the source (e.g. Treasury) to
		/// the child-/bounty account/location.
		///
		/// Once `check_status` confirms, the child-/bounty will transition to
		/// [`BountyStatus::Funded`].
		payment_status: PaymentState<PaymentId>,
	},
	/// The child-/bounty is funded and waiting for curator assignment.
	Funded {
		/// The proposed curator of this child-/bounty.
		curator: AccountId,
	},
	/// A new child-/bounty curator is proposed.
	CuratorProposed {
		/// The proposed curator of this child-/bounty.
		curator: AccountId,
	},
	/// The child-/bounty previously assigned curator has been unassigned.
	///
	/// It remains funded and is waiting for a curator proposal.
	CuratorUnassigned,
	/// The child-/bounty is active and waiting to be awarded.
	///
	/// Parent bounties can have child bounties.
	Active {
		/// The curator of this child-/bounty.
		curator: AccountId,
		/// The curator stash account/location used as a fee destination.
		curator_stash: Beneficiary,
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
		/// the bounty account/location.
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

		/// Converts an `AssetKind` into the treasury funding source.
		///
		/// Used when the treasury funds a bounty.
		type TreasurySource: TryConvert<
			Self::AssetKind,
			<<Self as pallet::Config<I>>::Paymaster as PayWithSource>::Source,
		>;

		/// Type used to derive the account/location of a bounty.
		///
		/// The account/location is derived from asset kind/class `AssetKind` and
		/// parent bounty `BountyIndex`.
		type BountySource: TryConvert<
			(BountyIndex, Self::AssetKind),
			<<Self as pallet::Config<I>>::Paymaster as PayWithSource>::Source,
		>;

		/// Type used to derive the account/location of a child-bounty.
		///
		/// The account/location is derived from asset kind/class `AssetKind`,
		/// parent bounty and child-bounty `BountyIndex`.
		type ChildBountySource: TryConvert<
			(BountyIndex, BountyIndex, Self::AssetKind),
			<<Self as pallet::Config<I>>::Paymaster as PayWithSource>::Source,
		>;

		/// Type for processing payments of [`Self::AssetKind`] from [`Self::Source`] in favor of
		/// [`Self::Beneficiary`].
		type Paymaster: PayWithSource<
			Balance = BalanceOf<Self, I>,
			Source = Self::Beneficiary,
			Beneficiary = Self::Beneficiary,
			AssetKind = Self::AssetKind,
		>;

		/// Helper type for benchmarks.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: benchmarking::ArgumentsFactory<Self::AssetKind, Self::Beneficiary>;
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// No child-/bounty at that index.
		InvalidIndex,
		/// The reason given is just too big.
		ReasonTooBig,
		/// Invalid child-/bounty value.
		InvalidValue,
		/// Invalid child-/bounty fee.
		InvalidFee,
		/// The balance of the asset kind is not convertible to the balance of the native asset for
		/// asserting the origin permissions.
		FailedToConvertBalance,
		/// The child-/bounty status is unexpected.
		UnexpectedStatus,
		/// Require child-/bounty curator.
		RequireCurator,
		/// The spend origin is valid but the amount it is allowed to spend is lower than the
		/// requested amount.
		InsufficientPermission,
		/// There was issue with funding the child-/bounty.
		FundingError,
		/// Child-/bounty funding has not concluded yet.
		FundingInconclusive,
		/// The child-/bounty account could not be derived from the index and asset kind.
		FailedToConvertSource,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		// New bounty created and funding initiated.
		BountyFunded {
			index: BountyIndex,
		},
		// New bounty created and funding initiated.
		ChildBountyFunded {
			index: BountyIndex,
			child_index: BountyIndex,
		},
		/// Funding payment has concluded successfully.
		BountyFundingProcessed {
			index: BountyIndex,
			child_index: Option<BountyIndex>,
		},
		/// Curator acccepts role and child-/bounty becomes active.
		BountyBecameActive {
			index: BountyIndex,
			child_index: Option<BountyIndex>,
			curator: T::AccountId,
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
			index: BountyIndex,
			child_index: Option<BountyIndex>,
			curator: T::AccountId,
		},
		/// A bounty curator is unassigned.
		CuratorUnassigned {
			index: BountyIndex,
			child_index: Option<BountyIndex>,
		},
		/// A child-/bounty curator is accepted.
		CuratorAccepted {
			index: BountyIndex,
			child_index: Option<BountyIndex>,
			curator: T::AccountId,
		},
		/// A payment failed and can be retried.
		PaymentFailed {
			index: BountyIndex,
			child_index: Option<BountyIndex>,
			payment_id: PaymentIdOf<T, I>,
		},
		/// A payment happened and can be checked.
		Paid {
			index: BountyIndex,
			child_index: Option<BountyIndex>,
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

	/// Child bounties that have been added.
	#[pallet::storage]
	pub type ChildBounties<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Twox64Concat,
		BountyIndex,
		Twox64Concat,
		BountyIndex,
		ChildBountyOf<T, I>,
	>;

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
		/// Fund a new bounty, iniitiating the payment from the treasury to the bounty
		/// account/location.
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
			curator: AccountIdLookupOf<T>,
			#[pallet::compact] fee: BalanceOf<T, I>,
			description: Vec<u8>,
		) -> DispatchResult {
			let max_amount = T::SpendOrigin::ensure_origin(origin)?;
			let curator = T::Lookup::lookup(curator)?;
			let bounded_description: BoundedVec<_, _> =
				description.try_into().map_err(|_| Error::<T, I>::ReasonTooBig)?;
			ensure!(fee < value, Error::<T, I>::InvalidFee);

			let native_amount = T::BalanceConverter::from_asset_balance(value, *asset_kind.clone())
				.map_err(|_| Error::<T, I>::FailedToConvertBalance)?;
			ensure!(native_amount >= T::BountyValueMinimum::get(), Error::<T, I>::InvalidValue);
			ensure!(native_amount <= max_amount, Error::<T, I>::InsufficientPermission);

			with_context::<SpendContext<BalanceOf<T, I>>, _>(|v| {
				let context = v.or_default();
				let funding = context.spend_in_context.entry(max_amount).or_default();

				if funding.checked_add(&native_amount).map(|s| s > max_amount).unwrap_or(true) {
					Err(Error::<T, I>::InsufficientPermission)
				} else {
					*funding = funding.saturating_add(native_amount);
					Ok(())
				}
			})
			.unwrap_or(Ok(()))?;

			let index = BountyCount::<T, I>::get();
			let payment_status = Self::do_process_funding_payment(
				index.clone(),
				None,
				value.clone(),
				*asset_kind.clone(),
				None,
			)?;

			let bounty = BountyOf::<T, I> {
				asset_kind: *asset_kind,
				value,
				fee,
				curator_deposit: 0u32.into(),
				status: BountyStatus::FundingAttempted {
					curator,
					payment_status: payment_status.clone(),
				},
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
		pub fn fund_child_bounty(
			origin: OriginFor<T>,
			asset_kind: Box<T::AssetKind>,
			#[pallet::compact] value: BalanceOf<T, I>,
			description: Vec<u8>,
			curator: AccountIdLookupOf<T>,
			#[pallet::compact] fee: BalanceOf<T, I>,
		) -> DispatchResult {
			Ok(())
		}

		/// Propose a new curator for a child-/bounty after the previous was unassigned.
		///
		/// ## Dispatch Origin
		/// Must be called from `T::SpendOrigin`.
		///
		/// ## Details
		/// - The child-/bounty must be in the `CuratorUnassigned` state.
		/// - The `SpendOrigin` must have sufficient permissions to propose the curator.
		/// - The curator fee must be less than the total bounty value.
		///
		/// ### Parameters
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child-bounty.
		/// - `curator`: Account to be proposed as the curator.
		/// - `fee`: Curator fee.
		///
		/// ## Events
		/// Emits [`Event::CuratorProposed`] if successful.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::propose_curator())]
		pub fn propose_curator(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			child_bounty_id: Option<BountyIndex>,
			curator: AccountIdLookupOf<T>,
			#[pallet::compact] fee: BalanceOf<T, I>,
		) -> DispatchResult {
			let max_amount = T::SpendOrigin::ensure_origin(origin)?;
			let curator = T::Lookup::lookup(curator)?;

			let (asset_kind, value, _, _, status) =
				Self::get_bounty_details(parent_bounty_id, child_bounty_id)?;
			ensure!(status == BountyStatus::CuratorUnassigned, Error::<T, I>::UnexpectedStatus);
			ensure!(fee < value, Error::<T, I>::InvalidFee);

			let native_amount =
				<T as pallet_treasury::Config<I>>::BalanceConverter::from_asset_balance(
					value, asset_kind,
				)
				.map_err(|_| Error::<T, I>::FailedToConvertBalance)?;
			ensure!(native_amount <= max_amount, Error::<T, I>::InsufficientPermission);

			let new_status = BountyStatus::Funded { curator: curator.clone() };
			Self::update_bounty_details(
				parent_bounty_id,
				child_bounty_id,
				new_status,
				Some(fee),
				None,
			)?;

			Self::deposit_event(Event::<T, I>::CuratorProposed {
				index: parent_bounty_id,
				child_index: child_bounty_id,
				curator,
			});

			Ok(())
		}

		/// Accept the curator role for a child-/bounty.
		///
		/// ## Dispatch Origin
		/// Must be signed by the proposed curator.
		///
		/// ## Details
		/// - The bounty must be in the `Funded` state.
		/// - The curator must accept the role by calling this function.
		/// - A deposit will be reserved from the curator and refunded upon successful payout.
		///
		/// ### Parameters
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child-bounty.
		/// - `stash`: Curator stash account/location that will receive the fee.
		///
		/// ## Events
		/// Emits [`Event::CuratorAccepted`] and [`Event::BountyBecameActive`] if successful.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::accept_curator())]
		pub fn accept_curator(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			child_bounty_id: Option<BountyIndex>,
			stash: BeneficiaryLookupOf<T, I>,
		) -> DispatchResult {
			let signer = ensure_signed(origin)?;
			let stash = T::BeneficiaryLookup::lookup(stash)?;

			let (asset_kind, value, fee, _, status) =
				Self::get_bounty_details(parent_bounty_id, child_bounty_id)?;

			match status {
				BountyStatus::Funded { ref curator } => {
					ensure!(signer == *curator, Error::<T, I>::RequireCurator);

					let deposit = Self::calculate_curator_deposit(&fee, asset_kind.clone())?;
					T::Currency::reserve(curator, deposit)?;

					let new_status =
						BountyStatus::Active { curator: curator.clone(), curator_stash: stash };
					Self::update_bounty_details(
						parent_bounty_id,
						child_bounty_id,
						new_status,
						None,
						Some(deposit),
					)?;

					Self::deposit_event(Event::<T, I>::BountyBecameActive {
						index: parent_bounty_id,
						child_index: child_bounty_id,
						curator: signer,
					});

					Ok(())
				},
				_ => Err(Error::<T, I>::UnexpectedStatus.into()),
			}
		}

		/// Unassign curator from a child-/bounty.
		///
		/// ## Dispatch Origin
		/// This function can only be called by the `RejectOrigin` or the child-/bounty curator.
		///
		/// ## Details
		/// - If this function is called by the `RejectOrigin`, we assume that the curator is
		///   malicious or inactive. As a result, we will slash the curator when possible.
		/// - If the origin is the curator, we take this as a sign they are unable to do their job
		///   and they willingly give up. We could slash them, but for now we allow them to recover
		///   their deposit and exit without issue. (We may want to change this if it is abused).
		/// - If successful, the child-/bounty status is updated to `CuratorUnassigned`. To
		///   reactivate the bounty, a new curator must be proposed and must accept the role.
		///
		/// ### Parameters
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child-bounty.
		///
		/// ## Events
		/// Emits [`Event::CuratorUnassigned`] if successful.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::unassign_curator())]
		pub fn unassign_curator(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			child_bounty_id: Option<BountyIndex>,
		) -> DispatchResult {
			let maybe_sender = ensure_signed(origin.clone())
				.map(Some)
				.or_else(|_| T::RejectOrigin::ensure_origin(origin).map(|_| None))?;

			let (_, _, _, mut curator_deposit, status) =
				Self::get_bounty_details(parent_bounty_id, child_bounty_id)?;

			let slash_curator = |curator: &T::AccountId, curator_deposit: &mut BalanceOf<T, I>| {
				let imbalance = T::Currency::slash_reserved(curator, *curator_deposit).0;
				T::OnSlash::on_unbalanced(imbalance);
				*curator_deposit = Zero::zero();
			};

			match status {
				BountyStatus::FundingAttempted { .. } => {
					// Funding payment initialized and curator proposed, but not possible to
					// unassign yet.
					return Err(Error::<T, I>::UnexpectedStatus.into());
				},
				BountyStatus::Funded { ref curator } |
				BountyStatus::CuratorProposed { ref curator } => {
					// Child-/Bounty curator has been proposed, but not accepted yet.
					// `RejectOrigin` or curator himself can unassign from this bounty.
					ensure!(maybe_sender.map_or(true, |sender| sender == *curator), BadOrigin);
				},
				BountyStatus::Active { ref curator, .. } => {
					// The child-/bounty is active.
					match maybe_sender {
						// If the `RejectOrigin` is calling this function, slash the curator.
						None => {
							slash_curator(curator, &mut curator_deposit);
							// Continue to change bounty status below...
						},
						Some(sender) => {
							// This is the curator, willingly giving up their role. Give back their
							// deposit.
							ensure!(sender == *curator, BadOrigin);
							let err_amount = T::Currency::unreserve(curator, curator_deposit);
							debug_assert!(err_amount.is_zero());
							curator_deposit = Zero::zero();
							// Continue to change bounty status below...
						},
					}
				},
				BountyStatus::PendingPayout { ref curator, .. } => {
					// The bounty is pending payout, so only `RejectOrigin` can unassign a curator.
					// By doing so, they are claiming the curator is acting maliciously, so
					// we slash the curator.
					ensure!(maybe_sender.is_none(), BadOrigin);
					slash_curator(curator, &mut curator_deposit);
					// Continue to change bounty status below...
				},
				_ => return Err(Error::<T, I>::UnexpectedStatus.into()),
			};

			let new_status = BountyStatus::CuratorUnassigned;
			Self::update_bounty_details(
				parent_bounty_id,
				child_bounty_id,
				new_status,
				None,
				Some(curator_deposit),
			)?;

			Self::deposit_event(Event::<T, I>::CuratorUnassigned {
				index: parent_bounty_id,
				child_index: child_bounty_id,
			});

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

		/// Retry the funding, refund or payout payments.
		///
		/// ## Dispatch Origin
		/// Must be signed.
		///
		/// ## Details
		/// - If the bounty status is `FundingAttempted`, it retries the funding payment from
		///   funding source the child-/bounty account/location.
		///
		/// ### Parameters
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child-bounty.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(9)]
		// TODO: change weight
		#[pallet::weight(<T as Config<I>>::WeightInfo::approve_bounty_with_curator())]
		pub fn retry_payment(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			child_bounty_id: Option<BountyIndex>,
		) -> DispatchResultWithPostInfo {
			use BountyStatus::*;

			ensure_signed(origin)?;
			let (asset_kind, value, _, _, status) =
				Self::get_bounty_details(parent_bounty_id, child_bounty_id)?;

			let (new_status, weight) = match status {
				FundingAttempted { ref payment_status, ref curator } => {
					let new_payment_status = Self::do_process_funding_payment(
						parent_bounty_id,
						child_bounty_id,
						value,
						asset_kind,
						Some(payment_status.clone()),
					)?;
					// TODO: change weight
					(
						FundingAttempted {
							payment_status: new_payment_status,
							curator: curator.clone(),
						},
						<T as Config<I>>::WeightInfo::approve_bounty_with_curator(),
					)
				},
				_ => return Err(Error::<T, I>::UnexpectedStatus.into()),
			};

			Self::update_bounty_details(parent_bounty_id, child_bounty_id, new_status, None, None)?;

			Ok(Some(weight).into())
		}

		/// Check and update the payment status of a child-/bounty.
		///
		/// ## Dispatch Origin
		/// Must be signed.
		///
		/// ## Details
		/// - If the child-/bounty status is `FundingAttempted`, it checks if the funding payment
		///   has succeeded. If successful, the bounty becomes `Funded`.
		///
		/// ### Parameters
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child-bounty.
		///
		/// ## Events
		/// - Emits `BountyBecameActive` when the bounty transitions to `Active`.
		/// - Emits `BountyPayoutProcessed` when the payout payments complete successfully.
		/// - Emits `BountyRefundProcessed` when the refund payment completes successfully.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(10)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::approve_bounty_with_curator())]
		pub fn check_status(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			child_bounty_id: Option<BountyIndex>,
		) -> DispatchResultWithPostInfo {
			use BountyStatus::*;

			ensure_signed(origin)?;
			let status = Self::get_bounty_status(parent_bounty_id, child_bounty_id)?;

			let (new_status, weight) = match status {
				FundingAttempted { ref payment_status, curator } => {
					let new_payment_status = Self::do_check_funding_payment_status(
						parent_bounty_id,
						child_bounty_id,
						payment_status.clone(),
					)?;
					// TODO: change weight
					match new_payment_status {
						PaymentState::Succeeded => (
							BountyStatus::<
								T::AccountId,
								BlockNumberFor<T, I>,
								PaymentIdOf<T, I>,
								T::Beneficiary,
							>::Funded {
								curator,
							},
							<T as Config<I>>::WeightInfo::approve_bounty_with_curator(),
						),
						_ => (
							BountyStatus::FundingAttempted {
								payment_status: new_payment_status,
								curator,
							},
							<T as Config<I>>::WeightInfo::approve_bounty_with_curator(),
						),
					}
				},
				_ => return Err(Error::<T, I>::UnexpectedStatus.into()),
			};

			Self::update_bounty_details(parent_bounty_id, child_bounty_id, new_status, None, None)?;

			Ok(Some(weight).into())
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

	/// The account/location of the treasury pot.
	pub fn treasury_account(asset_kind: T::AssetKind) -> Result<T::Beneficiary, DispatchError> {
		T::TreasurySource::try_convert(asset_kind)
			.map_err(|_| Error::<T, I>::FailedToConvertSource.into())
	}

	/// The account/location of a parent bounty.
	pub fn bounty_account(
		bounty_id: BountyIndex,
		asset_kind: T::AssetKind,
	) -> Result<T::Beneficiary, DispatchError> {
		T::BountySource::try_convert((bounty_id, asset_kind))
			.map_err(|_| Error::<T, I>::FailedToConvertSource.into())
	}

	/// The account/location of a child-bounty.
	pub fn child_bounty_account(
		parent_bounty_id: BountyIndex,
		child_bounty_id: BountyIndex,
		asset_kind: T::AssetKind,
	) -> Result<T::Beneficiary, DispatchError> {
		T::ChildBountySource::try_convert((parent_bounty_id, child_bounty_id, asset_kind))
			.map_err(|_| Error::<T, I>::FailedToConvertSource.into())
	}
	/// Returns the status of a child-/bounty.
	pub fn get_bounty_status(
		parent_bounty_id: BountyIndex,
		child_bounty_id: Option<BountyIndex>,
	) -> Result<
		BountyStatus<T::AccountId, BlockNumberFor<T, I>, PaymentIdOf<T, I>, T::Beneficiary>,
		DispatchError,
	> {
		match child_bounty_id {
			None => Bounties::<T, I>::get(parent_bounty_id)
				.map(|bounty| bounty.status)
				.ok_or(Error::<T, I>::InvalidIndex.into()),
			Some(child_id) => ChildBounties::<T, I>::get(parent_bounty_id, child_id)
				.map(|bounty| bounty.status)
				.ok_or(Error::<T, I>::InvalidIndex.into()),
		}
	}

	/// Returns the asset class, value, and status of a child-/bounty.
	///
	/// The asset class is always derived from the parent bounty.
	pub fn get_bounty_details(
		parent_bounty_id: BountyIndex,
		child_bounty_id: Option<BountyIndex>,
	) -> Result<
		(
			T::AssetKind,
			BalanceOf<T, I>,
			BalanceOf<T, I>,
			BalanceOf<T, I>,
			BountyStatus<T::AccountId, BlockNumberFor<T, I>, PaymentIdOf<T, I>, T::Beneficiary>,
		),
		DispatchError,
	> {
		let parent_bounty =
			Bounties::<T, I>::get(parent_bounty_id).ok_or(Error::<T, I>::InvalidIndex)?;

		match child_bounty_id {
			None => Ok((
				parent_bounty.asset_kind,
				parent_bounty.value,
				parent_bounty.fee,
				parent_bounty.curator_deposit,
				parent_bounty.status,
			)),
			Some(child_bounty_id) => {
				let child_bounty = ChildBounties::<T, I>::get(parent_bounty_id, child_bounty_id)
					.ok_or(Error::<T, I>::InvalidIndex)?;
				Ok((
					parent_bounty.asset_kind,
					child_bounty.value,
					child_bounty.fee,
					child_bounty.curator_deposit,
					child_bounty.status,
				))
			},
		}
	}

	/// Updates the status and optionally the fee and curator deposit of a child-/bounty.
	pub fn update_bounty_details(
		parent_bounty_id: BountyIndex,
		child_bounty_id: Option<BountyIndex>,
		new_status: BountyStatus<
			T::AccountId,
			BlockNumberFor<T, I>,
			PaymentIdOf<T, I>,
			T::Beneficiary,
		>,
		maybe_fee: Option<BalanceOf<T, I>>,
		maybe_curator_deposit: Option<BalanceOf<T, I>>,
	) -> Result<(), DispatchError> {
		match child_bounty_id {
			None => {
				let mut bounty =
					Bounties::<T, I>::get(parent_bounty_id).ok_or(Error::<T, I>::InvalidIndex)?;
				bounty.status = new_status;
				if let Some(curator_deposit) = maybe_curator_deposit {
					bounty.curator_deposit = curator_deposit;
				}
				if let Some(fee) = maybe_fee {
					bounty.fee = fee;
				}
				Bounties::<T, I>::insert(parent_bounty_id, bounty);
				Ok(())
			},
			Some(child_bounty_id) => {
				let mut bounty = ChildBounties::<T, I>::get(parent_bounty_id, child_bounty_id)
					.ok_or(Error::<T, I>::InvalidIndex)?;
				bounty.status = new_status;
				if let Some(curator_deposit) = maybe_curator_deposit {
					bounty.curator_deposit = curator_deposit;
				}
				if let Some(fee) = maybe_fee {
					bounty.fee = fee;
				}
				ChildBounties::<T, I>::insert(parent_bounty_id, child_bounty_id, bounty);
				Ok(())
			},
		}
	}

	/// Initializes payment from the funding source to the child-/bounty account/location.
	fn do_process_funding_payment(
		parent_bounty_id: BountyIndex,
		child_bounty_id: Option<BountyIndex>,
		value: BalanceOf<T, I>,
		asset_kind: T::AssetKind,
		maybe_payment_status: Option<PaymentState<PaymentIdOf<T, I>>>,
	) -> Result<PaymentState<PaymentIdOf<T, I>>, DispatchError> {
		if let Some(payment_status) = maybe_payment_status {
			ensure!(payment_status.is_pending_or_failed(), Error::<T, I>::UnexpectedStatus);
		}

		let (source, beneficiary) = match child_bounty_id {
			None => (
				Self::treasury_account(asset_kind.clone())?,
				Self::bounty_account(parent_bounty_id, asset_kind.clone())?,
			),
			Some(child_bounty_id) => (
				Self::bounty_account(parent_bounty_id, asset_kind.clone())?,
				Self::child_bounty_account(parent_bounty_id, child_bounty_id, asset_kind.clone())?,
			),
		};

		let id = <T as Config<I>>::Paymaster::pay(&source, &beneficiary, asset_kind, value)
			.map_err(|_| Error::<T, I>::FundingError)?;

		Self::deposit_event(Event::<T, I>::Paid {
			index: parent_bounty_id,
			child_index: child_bounty_id,
			payment_id: id,
		});

		Ok(PaymentState::Attempted { id })
	}

	/// Queries the status of the payment from the funding source to the child-/bounty
	/// account/location
	fn do_check_funding_payment_status(
		parent_bounty_id: BountyIndex,
		child_bounty_id: Option<BountyIndex>,
		payment_status: PaymentState<PaymentIdOf<T, I>>,
	) -> Result<PaymentState<PaymentIdOf<T, I>>, DispatchError> {
		let payment_id = payment_status.get_attempt_id().ok_or(Error::<T, I>::UnexpectedStatus)?;

		match <T as Config<I>>::Paymaster::check_payment(payment_id) {
			PaymentStatus::Success => {
				Self::deposit_event(Event::<T, I>::BountyFundingProcessed {
					index: parent_bounty_id,
					child_index: child_bounty_id,
				});
				Ok(PaymentState::Succeeded)
			},
			PaymentStatus::InProgress => return Err(Error::<T, I>::FundingInconclusive.into()),
			PaymentStatus::Unknown | PaymentStatus::Failure => {
				Self::deposit_event(Event::<T, I>::PaymentFailed {
					index: parent_bounty_id,
					child_index: child_bounty_id,
					payment_id,
				});
				return Ok(PaymentState::Failed)
			},
		}
	}
}

/// TryConvert implementation to get the account/location of the treasury pot.
pub struct TreasurySource<T, I = ()>(PhantomData<(T, I)>);
impl<T, I> TryConvert<T::AssetKind, T::Beneficiary> for TreasurySource<T, I>
where
	T: crate::Config<I>,
{
	fn try_convert(_asset_kind: T::AssetKind) -> Result<T::Beneficiary, T::AssetKind> {
		let account = T::PalletId::get().into_account_truncating();
		Ok(account)
	}
}

/// TryConvert implementation to get the account/location of a parent bounty.
pub struct BountySource<T, I = ()>(PhantomData<(T, I)>);
impl<T, I> TryConvert<(BountyIndex, T::AssetKind), T::Beneficiary> for BountySource<T, I>
where
	T: crate::Config<I>,
{
	fn try_convert(
		(parent_bounty_id, _asset_kind): (BountyIndex, T::AssetKind),
	) -> Result<T::Beneficiary, (BountyIndex, T::AssetKind)> {
		let account = T::PalletId::get().into_sub_account_truncating(("bt", parent_bounty_id));
		Ok(account)
	}
}

/// TryConvert implementation to get the account/location of a child-bounty.
pub struct ChildBountySource<T, I = ()>(PhantomData<(T, I)>);
impl<T, I> TryConvert<(BountyIndex, BountyIndex, T::AssetKind), T::Beneficiary>
	for ChildBountySource<T, I>
where
	T: crate::Config<I>,
{
	fn try_convert(
		(parent_bounty_id, child_bounty_id, _asset_kind): (BountyIndex, BountyIndex, T::AssetKind),
	) -> Result<T::Beneficiary, (BountyIndex, BountyIndex, T::AssetKind)> {
		// The prefix is changed to have different AccountId when the index of
		// parent and child is same.
		let account = T::PalletId::get().into_sub_account_truncating((
			"cb",
			parent_bounty_id,
			child_bounty_id,
		));
		Ok(account)
	}
}
