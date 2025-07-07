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
	<T as pallet_treasury::Config<I>>::AssetKind,
	PaymentIdOf<T, I>,
	<T as pallet_treasury::Config<I>>::Beneficiary,
>;
type ChildBountyOf<T, I> = ChildBounty<
	<T as frame_system::Config>::AccountId,
	BalanceOf<T, I>,
	PaymentIdOf<T, I>,
	<T as pallet_treasury::Config<I>>::Beneficiary,
>;

/// A bounty funded.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct Bounty<AccountId, Balance, AssetKind, PaymentId, Beneficiary> {
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
	pub status: BountyStatus<AccountId, PaymentId, Beneficiary>,
}

/// A child-bounty funded.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ChildBounty<AccountId, Balance, PaymentId, Beneficiary> {
	/// The parent bounty index of this child-bounty.
	pub parent_bounty: BountyIndex,
	/// The (total) amount that should be paid if the child-bounty is rewarded, including
	/// beneficiary payout and child curator fee (of ).
	///
	/// The asset class determined by the parent bounty [`asset_kind`].
	pub value: Balance,
	/// The fee that the child curator receives upon successful payout.
	///
	/// The asset class determined by the parent bounty [`asset_kind`].
	pub fee: Balance,
	/// The deposit of curator.
	///
	/// The asset class determined by the [`pallet_treasury::Config::Currency`].
	pub curator_deposit: Balance,
	/// The status of this bounty.
	pub status: BountyStatus<AccountId, PaymentId, Beneficiary>,
}

/// The status of a bounty proposal.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum BountyStatus<AccountId, PaymentId, Beneficiary> {
	/// The child-/bounty funding has been attempted is waiting to confirm the funds allocation.
	///
	/// Call `check_status` to confirm whether the funding payment succeeded. If successful, the
	/// child-/bounty transitions to `Funded`. Otherwise, use `retry_payment` to reinitiate the
	/// funding payment.
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
	/// The child-/bounty is funded and waiting for curator to accept role.
	Funded {
		/// The proposed curator of this child-/bounty.
		curator: AccountId,
	},
	/// The child-/bounty previously assigned curator has been unassigned.
	///
	/// It remains funded and is waiting for a curator proposal.
	CuratorUnassigned,
	/// The child-/bounty is active and waiting to be awarded.
	///
	/// During the `Active` state, the curator can call `fund_child_bounty` to create multiple
	/// child bounties.
	Active {
		/// The curator of this child-/bounty.
		curator: AccountId,
		/// The curator stash account/location used as a fee destination.
		curator_stash: Beneficiary,
	},
	/// The child-/bounty payout has been attempted.
	///
	/// The transfers to both the curator stash and the beneficiary have been initiated.
	/// Call `check_status` to confirm whether both payments succeeded. If successful, the
	/// child-/bounty is finalized and removed from storage. If either payment fails, you can retry
	/// one or both using `retry_payment`.
	PayoutAttempted {
		/// The curator of this bounty.
		curator: AccountId,
		/// The beneficiary stash account/location with its payout payment status.
		beneficiary: (Beneficiary, PaymentState<PaymentId>),
		/// The curator stash account/location with its payout payment status.
		curator_stash: (Beneficiary, PaymentState<PaymentId>),
	},
	/// The bounty is closed, and the funds are being refunded to the original source (e.g.,
	/// Treasury). Once `check_status` confirms the payment succeeded, child-/the bounty is
	/// finalized and removed from storage.
	RefundAttempted {
		/// The curator of this bounty.
		curator: Option<AccountId>,
		/// The refund payment status.
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

	/// If a payment has been initiated, returns its identifier, which is used to check its
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

		/// Minimum value for a child-bounty.
		#[pallet::constant]
		type ChildBountyValueMinimum: Get<BalanceOf<Self, I>>;

		/// Maximum number of child bounties that can be added to a parent bounty.
		#[pallet::constant]
		type MaxActiveChildBountyCount: Get<u32>;

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
		/// There was issue with refunding the child-/bounty.
		RefundError,
		// There was issue paying out the child-/bounty.
		PayoutError,
		/// Child-/bounty funding has not concluded yet.
		FundingInconclusive,
		/// Child-/bounty refund has not concluded yet.
		RefundInconclusive,
		/// Child-/bounty payout has not concluded yet.
		PayoutInconclusive,
		/// The child-/bounty account could not be derived from the index and asset kind.
		FailedToConvertSource,
		/// The parent bounty cannot be closed because it has active child bounties.
		HasActiveChildBounty,
		/// Number of child bounties exceeds limit `MaxActiveChildBountyCount`.
		TooManyChildBounties,
		/// The parent bounty value is not enough to add new child-bounty.
		InsufficientBountyValue,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		// New bounty created and funding initiated.
		BountyFunded {
			index: BountyIndex,
		},
		// New child-bounty created and funding initiated.
		ChildBountyFunded {
			index: BountyIndex,
			child_index: BountyIndex,
		},
		/// Curator acccepts role and child-/bounty becomes active.
		BountyBecameActive {
			index: BountyIndex,
			child_index: Option<BountyIndex>,
			curator: T::AccountId,
		},
		/// A child-/bounty is awarded to a beneficiary and fee paid to the curator.
		BountyAwarded {
			index: BountyIndex,
			child_index: Option<BountyIndex>,
			beneficiary: T::Beneficiary,
			curator_stash: T::Beneficiary,
		},
		/// Payout payments to the beneficiary and curator stash have concluded successfully.
		BountyPayoutProcessed {
			index: BountyIndex,
			child_index: Option<BountyIndex>,
			asset_kind: T::AssetKind,
			/// The amount paid to the beneficiary.
			value: BalanceOf<T, I>,
			beneficiary: T::Beneficiary,
		},
		/// Funding payment has concluded successfully.
		BountyFundingProcessed {
			index: BountyIndex,
			child_index: Option<BountyIndex>,
		},
		/// Refund payment has concluded successfully.
		BountyRefundProcessed {
			index: BountyIndex,
			child_index: Option<BountyIndex>,
		},
		/// A bounty is cancelled.
		BountyCanceled {
			index: BountyIndex,
			child_index: Option<BountyIndex>,
		},
		/// A child-/bounty curator is unassigned.
		CuratorUnassigned {
			index: BountyIndex,
			child_index: Option<BountyIndex>,
		},
		/// A child-/bounty curator is proposed.
		CuratorProposed {
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

	/// The description of each child-bounty. Indexed by `(parent_id, child_id)`.
	#[pallet::storage]
	pub type ChildBountyDescriptions<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Twox64Concat,
		BountyIndex,
		Twox64Concat,
		BountyIndex,
		BoundedVec<u8, T::MaximumReasonLength>,
	>;

	/// Number of active child bounties per parent bounty.
	/// Map of parent bounty index to number of child bounties.
	#[pallet::storage]
	pub type ChildBountiesPerParent<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BountyIndex, u32, ValueQuery>;

	/// Number of total child bounties per parent bounty, including completed bounties.
	#[pallet::storage]
	pub type TotalChildBountiesPerParent<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BountyIndex, u32, ValueQuery>;

	/// The cumulative child-bounty value for each parent bounty.
	#[pallet::storage]
	pub type ChildBountiesValuePerParent<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BountyIndex, BalanceOf<T, I>, ValueQuery>;

	/// The cumulative child-bounty curator fees for each parent bounty.
	#[pallet::storage]
	pub type ChildBountiesCuratorFeesPerParent<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BountyIndex, BalanceOf<T, I>, ValueQuery>;

	/// Temporarily tracks spending limits within the current block to prevent overspending.
	#[derive(Default)]
	pub struct SpendContext<Balance> {
		pub spend_in_context: BTreeMap<Balance, Balance>,
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Fund a new parent bounty, iniitiating the payment from the treasury to the bounty
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
		///   `check_status` call before retrying with `retry_payment` call.
		///
		/// ### Parameters
		/// - `asset_kind`: An indicator of the specific asset class to be funded.
		/// - `value`: The total payment amount of this parent bounty, curator fee included.
		/// - `curator`: Address of parent bounty curator.
		/// - `fee`: Payment fee to parent bounty curator for execution.
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
				*asset_kind.clone(),
				value.clone(),
				None,
			)?;

			let bounty = BountyOf::<T, I> {
				asset_kind: *asset_kind,
				value,
				fee,
				curator_deposit: 0u32.into(),
				status: BountyStatus::FundingAttempted { curator, payment_status },
			};
			Bounties::<T, I>::insert(index, &bounty);
			BountyCount::<T, I>::put(index + 1);
			BountyDescriptions::<T, I>::insert(index, bounded_description);

			Self::deposit_event(Event::<T, I>::BountyFunded { index });

			Ok(())
		}

		/// Fund a new child-bounty, initiating the payment from the parent bounty
		/// to the child-bounty account/location.
		///
		/// ## Dispatch Origin
		/// Must be signed by the parent curator.
		///
		/// ## Details
		/// - If `curator` is not provided, the child-bounty will default to using the parent
		///   curator. If `fee` is not provided, it will default to `0`. This setup allows the
		///   parent curator to immediately call `check_status` and `award_bounty` to payout the
		///   child-bounty.
		/// - In case of a funding failure, the child-/bounty status must be updated with the
		///   `check_status` call before retrying with `retry_payment` call.
		///
		/// ### Parameters
		/// - `parent_bounty_id`: Index of parent bounty for which child-bounty is being added.
		/// - `value`: The total payment amount of this child-bounty, curator fee included.
		/// - `curator`: Address of child-bounty curator.
		/// - `fee`: Payment fee to child-bounty curator for execution.
		/// - `description`: The description of this bounty.
		///
		/// ## Events
		/// Emits [`Event::BountyFunded`] if successful.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::approve_bounty_with_curator())]
		pub fn fund_child_bounty(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			#[pallet::compact] value: BalanceOf<T, I>,
			curator: Option<AccountIdLookupOf<T>>,
			fee: Option<BalanceOf<T, I>>,
			description: Vec<u8>,
		) -> DispatchResult {
			let signer = ensure_signed(origin)?;

			let bounded_description: BoundedVec<_, _> =
				description.try_into().map_err(|_| Error::<T, I>::ReasonTooBig)?;
			let (asset_kind, parent_value, _, _, _, parent_curator, _) =
				Self::get_bounty_details(parent_bounty_id, None)
					.map_err(|_| Error::<T, I>::InvalidIndex)?;
			let native_amount =
				<T as pallet_treasury::Config<I>>::BalanceConverter::from_asset_balance(
					value,
					asset_kind.clone(),
				)
				.map_err(|_| pallet_treasury::Error::<T, I>::FailedToConvertBalance)?;

			ensure!(
				native_amount >= T::ChildBountyValueMinimum::get(),
				Error::<T, I>::InvalidValue
			);
			ensure!(
				ChildBountiesPerParent::<T, I>::get(parent_bounty_id) <
					T::MaxActiveChildBountyCount::get() as u32,
				Error::<T, I>::TooManyChildBounties,
			);

			// Parent bounty must be `Active` with a curator assigned.
			let parent_curator = parent_curator.ok_or(Error::<T, I>::UnexpectedStatus)?;
			let final_curator = match curator {
				Some(curator) => T::Lookup::lookup(curator)?,
				None => parent_curator.clone(),
			};
			ensure!(signer == parent_curator, Error::<T, I>::RequireCurator);

			// Check value
			let child_bounties_value = ChildBountiesValuePerParent::<T, I>::get(parent_bounty_id);
			let remaining_parent_value = parent_value.saturating_sub(child_bounties_value);
			ensure!(remaining_parent_value >= value, Error::<T, I>::InsufficientBountyValue);

			// Ensure child-bounty fee is less than value.
			let final_fee = fee.unwrap_or_else(Zero::zero);
			ensure!(final_fee < value, Error::<T, I>::InvalidFee);

			// Get child-bounty ID.
			let child_bounty_id = TotalChildBountiesPerParent::<T, I>::get(parent_bounty_id);

			// Initiate funding payment
			let payment_status = Self::do_process_funding_payment(
				parent_bounty_id,
				Some(child_bounty_id),
				asset_kind,
				value,
				None,
			)?;

			let child_bounty = ChildBounty {
				parent_bounty: parent_bounty_id,
				value,
				fee: final_fee,
				curator_deposit: 0u32.into(),
				status: BountyStatus::FundingAttempted {
					curator: final_curator,
					payment_status: payment_status.clone(),
				},
			};

			ChildBounties::<T, I>::insert(parent_bounty_id, child_bounty_id, child_bounty);
			ChildBountyDescriptions::<T, I>::insert(
				parent_bounty_id,
				child_bounty_id,
				bounded_description,
			);

			// Add child-bounty fee to the cumulative fee sum. To be
			// subtracted from the parent bounty payout when awarding
			// bounty.
			ChildBountiesCuratorFeesPerParent::<T, I>::mutate(parent_bounty_id, |value| {
				*value = value.saturating_add(final_fee)
			});

			// Add child-bounty value to the cumulative value sum. To be
			// subtracted from the parent bounty payout when awarding
			// bounty.
			ChildBountiesValuePerParent::<T, I>::mutate(parent_bounty_id, |children_value| {
				*children_value = children_value.saturating_add(value)
			});

			// Increment the active child-bounty count.
			ChildBountiesPerParent::<T, I>::mutate(parent_bounty_id, |count| {
				count.saturating_inc()
			});
			TotalChildBountiesPerParent::<T, I>::insert(
				parent_bounty_id,
				child_bounty_id.saturating_add(1),
			);

			Self::deposit_event(Event::<T, I>::ChildBountyFunded {
				index: parent_bounty_id,
				child_index: child_bounty_id,
			});

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

			let (asset_kind, value, _, _, status, _, _) =
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
		/// Emits [`Event::BountyBecameActive`] if successful.
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

			let (asset_kind, value, fee, _, status, _, _) =
				Self::get_bounty_details(parent_bounty_id, child_bounty_id)?;

			let BountyStatus::Funded { ref curator } = status else {
				return Err(Error::<T, I>::UnexpectedStatus.into())
			};
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

			let (_, _, _, mut curator_deposit, status, _, _) =
				Self::get_bounty_details(parent_bounty_id, child_bounty_id)?;

			let slash_curator = |curator: &T::AccountId, curator_deposit: &mut BalanceOf<T, I>| {
				let imbalance = T::Currency::slash_reserved(curator, *curator_deposit).0;
				T::OnSlash::on_unbalanced(imbalance);
				*curator_deposit = Zero::zero();
			};

			match status {
				BountyStatus::Funded { ref curator } => {
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

		/// Awards the child-/bounty to a beneficiary account/location,
		/// initiating the payout payments to both the beneficiary and the curator.
		///
		/// ## Dispatch Origin
		/// Must be signed by the curator of the child-/bounty.
		///
		/// ## Details
		/// - The child-/bounty must be in the `Active` state.
		/// - if awarding a parent bounty it must not have active or funded child bounties.
		/// - Initiates two payout payments from the child-/bounty account: child-/bounty `value` to
		///   the beneficiary, and `fee` to the curator's stash account.
		/// - If successful the bounty status is updated to `PayoutAttempted`.
		/// - In case of a payout failure, the child-/bounty status must be updated with the
		/// `check_status` call before retrying with `retry_payment` call.
		///
		/// ### Parameters
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child-bounty.
		/// - `beneficiary`: Account/location to be awarded the child-/bounty.
		///
		/// ## Events
		/// Emits [`Event::BountyAwarded`] if successful.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::award_bounty())]
		pub fn award_bounty(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			child_bounty_id: Option<BountyIndex>,
			beneficiary: BeneficiaryLookupOf<T, I>,
		) -> DispatchResult {
			let signer = ensure_signed(origin)?;
			let beneficiary = T::BeneficiaryLookup::lookup(beneficiary)?;

			let (asset_kind, value, fee, _, status, _, _) =
				Self::get_bounty_details(parent_bounty_id, child_bounty_id)?;

			if child_bounty_id.is_none() {
				ensure!(
					ChildBountiesPerParent::<T, I>::get(parent_bounty_id) == 0,
					Error::<T, I>::HasActiveChildBounty
				);
			}

			match status {
				BountyStatus::Active { ref curator, curator_stash } => {
					ensure!(signer == *curator, Error::<T, I>::RequireCurator);

					let (beneficiary_payment_status, curator_payment_status) =
						Self::do_process_payout_payments(
							parent_bounty_id,
							child_bounty_id,
							asset_kind,
							value,
							fee,
							(beneficiary.clone(), None),
							(curator_stash.clone(), None),
						)?;

					let new_status = BountyStatus::PayoutAttempted {
						curator: curator.clone(),
						beneficiary: (beneficiary.clone(), beneficiary_payment_status.clone()),
						curator_stash: (curator_stash.clone(), curator_payment_status.clone()),
					};
					Self::update_bounty_details(
						parent_bounty_id,
						child_bounty_id,
						new_status,
						None,
						None,
					)?;

					Self::deposit_event(Event::<T, I>::BountyAwarded {
						index: parent_bounty_id,
						child_index: child_bounty_id,
						beneficiary,
						curator_stash,
					});
				},
				_ => return Err(Error::<T, I>::UnexpectedStatus.into()),
			}

			Ok(())
		}

		/// Cancel an active child-/bounty. A payment to send all the funds to the funding source is
		/// initialized.
		///
		/// ## Dispatch Origin
		/// A parent bounty can only be cancelled by the `T::RejectOrigin`. A child bounty can be
		/// cancelled either by the parent bounty curator or `T::RejectOrigin`.
		///
		/// ## Details
		/// - If the child-/bounty is in the `Funded` state, a refund payment is initiated.
		/// - If the child-/bounty is in the `Active` state, a refund payment is initiated and the
		///   child-/bounty status is updated with the curator account/location.
		/// - If the child-/bounty is already in the payout phase, it cannot be canceled.
		/// - In case of a refund failure, the child-/bounty status must be updated with the
		/// `check_status` call before retrying with `retry_payment` call.
		///
		/// ### Parameters
		/// - `bounty_id`: The index of the bounty to cancel.
		///
		/// ## Events
		/// - Emits `BountyCanceled` if the child-/bounty was already funded and is being refunded.
		/// - Emits `Paid` if the child-/bounty refund payment is initiated.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(7)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::close_bounty_proposed()
			.max(<T as Config<I>>::WeightInfo::close_bounty_active()))]
		pub fn close_bounty(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			child_bounty_id: Option<BountyIndex>,
		) -> DispatchResultWithPostInfo {
			let maybe_sender = ensure_signed(origin.clone())
				.map(Some)
				.or_else(|_| T::RejectOrigin::ensure_origin(origin).map(|_| None))?;

			let (asset_kind, value, _, _, status, parent_curator, _) =
				Self::get_bounty_details(parent_bounty_id, child_bounty_id)?;

			match child_bounty_id {
				None => {
					ensure!(maybe_sender.is_none(), BadOrigin);
					ensure!(
						ChildBountiesPerParent::<T, I>::get(parent_bounty_id) == 0,
						Error::<T, I>::HasActiveChildBounty
					);
				},
				Some(_) => {
					match maybe_sender {
						Some(sender) => {
							// If the parent bounty does not have a curator, then it cannot be
							// closed
							let parent_curator =
								parent_curator.ok_or(Error::<T, I>::UnexpectedStatus)?;
							ensure!(sender == parent_curator, BadOrigin);
						},
						None => {}, // `RejectOrigin` is calling this function
					}
				},
			}

			let maybe_curator = match status {
				BountyStatus::Funded { curator } | BountyStatus::Active { curator, .. } =>
					Some(curator),
				BountyStatus::CuratorUnassigned => None,
				_ => return Err(Error::<T, I>::UnexpectedStatus.into()),
			};

			let payment_status = Self::do_process_refund_payment(
				parent_bounty_id,
				child_bounty_id,
				asset_kind,
				value,
				None,
			)?;
			let new_status = BountyStatus::RefundAttempted {
				payment_status: payment_status.clone(),
				curator: maybe_curator.clone(),
			};
			Self::update_bounty_details(parent_bounty_id, child_bounty_id, new_status, None, None);

			Self::deposit_event(Event::<T, I>::BountyCanceled {
				index: parent_bounty_id,
				child_index: child_bounty_id,
			});

			// TODO: change weight
			Ok(Some(<T as Config<I>>::WeightInfo::close_bounty_proposed()).into())
		}

		/// Check and update the payment status of a child-/bounty.
		///
		/// ## Dispatch Origin
		/// Must be signed.
		///
		/// ## Details
		/// - If the child-/bounty status is `FundingAttempted`, it checks if the funding payment
		///   has succeeded. If successful, the bounty becomes `Funded`.
		/// - If the child-/bounty status is `RefundAttempted`, it checks if the refund payment has
		///   succeeded. If successful, the child-/bounty is removed from storage.
		/// - If the child-/bounty status is `PayoutAttempted`, it checks the 2 payout payments have
		///   succeeded. If both succeed, the child-/bounty is removed from storage.
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
		#[pallet::call_index(9)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::approve_bounty_with_curator())]
		pub fn check_status(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			child_bounty_id: Option<BountyIndex>,
		) -> DispatchResultWithPostInfo {
			use BountyStatus::*;

			ensure_signed(origin)?;
			let (asset_kind, value, fee, curator_deposit, status, parent_curator, parent_curator_stash) =
				Self::get_bounty_details(parent_bounty_id, child_bounty_id)?;

			let (new_status, weight) = match status {
				FundingAttempted { ref payment_status, curator } => {
					let new_payment_status = Self::do_check_funding_payment_status(
						parent_bounty_id,
						child_bounty_id,
						payment_status.clone(),
					)?;
					// TODO: change weight
					match new_payment_status {
						PaymentState::Succeeded => {
							if let Some(child_bounty_id) = child_bounty_id {
								if let Some(curator_stash) = parent_curator_stash {
									if let Some(parent_curator) = parent_curator {
										if curator == parent_curator {
											(
												BountyStatus::Active {
													curator,
													curator_stash,
												},
												<T as Config<I>>::WeightInfo::approve_bounty_with_curator(),
											)
										} else {
											(
												BountyStatus::Funded { curator },
												<T as Config<I>>::WeightInfo::approve_bounty_with_curator(),
											)
										}
									} else {
										(
											BountyStatus::Funded { curator },
											<T as Config<I>>::WeightInfo::approve_bounty_with_curator(),
										)
									}
								} else {
									(
										BountyStatus::Funded { curator },
										<T as Config<I>>::WeightInfo::approve_bounty_with_curator(),
									)
								}
							} else {
								(
									BountyStatus::Funded { curator },
									<T as Config<I>>::WeightInfo::approve_bounty_with_curator(),
								)
							}
						},
						_ => (
							BountyStatus::FundingAttempted {
								payment_status: new_payment_status,
								curator,
							},
							<T as Config<I>>::WeightInfo::approve_bounty_with_curator(),
						),
					}
				},
				RefundAttempted { ref payment_status, ref curator } => {
					let new_payment_status = Self::do_check_refund_payment_status(
						parent_bounty_id,
						child_bounty_id,
						asset_kind,
						value,
						curator_deposit,
						payment_status.clone(),
						curator.clone(),
					)?;
					// TODO: change weight
					match new_payment_status {
						PaymentState::Succeeded => {
							if let Some(curator) = curator {
								// Unreserve the curator deposit when payment succeeds
								let err_amount = T::Currency::unreserve(&curator, curator_deposit);
								debug_assert!(err_amount.is_zero()); // Ensure nothing remains reserved
							}
							// refund succeeded, cleanup the bounty
							Self::remove_bounty(parent_bounty_id, child_bounty_id);
							return Ok(Pays::No.into())
						},
						_ => (
							BountyStatus::RefundAttempted {
								payment_status: new_payment_status,
								curator: curator.clone(),
							},
							<T as Config<I>>::WeightInfo::approve_bounty_with_curator(),
						),
					}
				},
				PayoutAttempted { ref curator, ref curator_stash, ref beneficiary } => {
					let (new_curator_stash_payment_status, new_beneficiary_payment_status) =
						Self::do_check_payout_payment_status(
							parent_bounty_id,
							child_bounty_id,
							asset_kind,
							value,
							curator.clone(),
							fee,
							curator_deposit,
							curator_stash.clone(),
							beneficiary.clone(),
						)?;
					// TODO: change weight
					match (
						new_curator_stash_payment_status.clone(),
						new_beneficiary_payment_status.clone(),
					) {
						(PaymentState::Succeeded, PaymentState::Succeeded) => {
							// Unreserve the curator deposit when both payments succeed
							let err_amount = T::Currency::unreserve(&curator, curator_deposit);
							debug_assert!(err_amount.is_zero()); // Ensure nothing remains reserved
											// payout succeeded, cleanup the bounty
							Self::remove_bounty(parent_bounty_id, child_bounty_id);
							return Ok(Pays::No.into())
						},
						_ => (
							BountyStatus::PayoutAttempted {
								curator: curator.clone(),
								curator_stash: (
									curator_stash.0.clone(),
									new_curator_stash_payment_status.clone(),
								),
								beneficiary: (
									beneficiary.0.clone(),
									new_beneficiary_payment_status.clone(),
								),
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

		/// Retry the funding, refund or payout payments.
		///
		/// ## Dispatch Origin
		/// Must be signed.
		///
		/// ## Details
		/// - If the child-/bounty status is `FundingAttempted`, it retries the funding payment from
		///   funding source the child-/bounty account/location.
		/// - If the child-/bounty status is `RefundAttempted`, it retries the refund payment from
		///   the child-/bounty account/location to the funding source.
		/// - If the child-/bounty status is `PayoutAttempted`, it retries the payout payments from
		///   the child-/bounty account/location to the beneficiary and curator stash
		///   accounts/locations.
		///
		/// ### Parameters
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child-bounty.
		///
		/// ## Events
		/// - Emits `Paid` for each individual payment initiated.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(10)]
		// TODO: change weight
		#[pallet::weight(<T as Config<I>>::WeightInfo::approve_bounty_with_curator())]
		pub fn retry_payment(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			child_bounty_id: Option<BountyIndex>,
		) -> DispatchResultWithPostInfo {
			use BountyStatus::*;

			ensure_signed(origin)?;
			let (asset_kind, value, fee, _, status, _, _) =
				Self::get_bounty_details(parent_bounty_id, child_bounty_id)?;

			let (new_status, weight) = match status {
				FundingAttempted { ref payment_status, ref curator } => {
					let new_payment_status = Self::do_process_funding_payment(
						parent_bounty_id,
						child_bounty_id,
						asset_kind,
						value,
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
				RefundAttempted { ref payment_status, ref curator } => {
					let new_payment_status = Self::do_process_refund_payment(
						parent_bounty_id,
						child_bounty_id,
						asset_kind,
						value,
						Some(payment_status.clone()),
					)?;
					// TODO: change weight
					(
						RefundAttempted {
							payment_status: new_payment_status,
							curator: curator.clone(),
						},
						<T as Config<I>>::WeightInfo::approve_bounty_with_curator(),
					)
				},
				PayoutAttempted { ref curator, ref curator_stash, ref beneficiary } => {
					let (new_beneficiary_payment_status, new_curator_payment_status) =
						Self::do_process_payout_payments(
							parent_bounty_id,
							child_bounty_id,
							asset_kind,
							value,
							fee,
							(beneficiary.0.clone(), Some(beneficiary.1.clone())),
							(curator_stash.0.clone(), Some(curator_stash.1.clone())),
						)?;
					// TODO: change weight
					(
						PayoutAttempted {
							curator: curator.clone(),
							beneficiary: (beneficiary.0.clone(), new_beneficiary_payment_status),
							curator_stash: (curator_stash.0.clone(), new_curator_payment_status),
						},
						<T as Config<I>>::WeightInfo::approve_bounty_with_curator(),
					)
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
	) -> Result<BountyStatus<T::AccountId, PaymentIdOf<T, I>, T::Beneficiary>, DispatchError> {
		match child_bounty_id {
			None => Bounties::<T, I>::get(parent_bounty_id)
				.map(|bounty| bounty.status)
				.ok_or(Error::<T, I>::InvalidIndex.into()),
			Some(child_id) => ChildBounties::<T, I>::get(parent_bounty_id, child_id)
				.map(|bounty| bounty.status)
				.ok_or(Error::<T, I>::InvalidIndex.into()),
		}
	}

	/// Returns the asset class, value, curator fee, curator deposit, status, parent curator, and parent curator stash of a child-/bounty.
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
			BountyStatus<T::AccountId, PaymentIdOf<T, I>, T::Beneficiary>,
			Option<T::AccountId>,
			Option<T::Beneficiary>,
		),
		DispatchError,
	> {
		let parent_bounty =
			Bounties::<T, I>::get(parent_bounty_id).ok_or(Error::<T, I>::InvalidIndex)?;
		let (parent_curator, parent_curator_stash) = if let BountyStatus::Active { curator, curator_stash } = &parent_bounty.status {
			(Some(curator.clone()), Some(curator_stash.clone()))
		} else {
			(None, None)
		};
		match child_bounty_id {
			None => Ok((
				parent_bounty.asset_kind,
				parent_bounty.value,
				parent_bounty.fee,
				parent_bounty.curator_deposit,
				parent_bounty.status,
				parent_curator,
				parent_curator_stash
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
					parent_curator,
					parent_curator_stash
				))
			},
		}
	}

	/// Updates the status and optionally the fee and curator deposit of a child-/bounty.
	pub fn update_bounty_details(
		parent_bounty_id: BountyIndex,
		child_bounty_id: Option<BountyIndex>,
		new_status: BountyStatus<T::AccountId, PaymentIdOf<T, I>, T::Beneficiary>,
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

	/// Calculates amount the beneficiary and curator stash during child-/bounty payout.
	fn calculate_curator_fee_and_payout(
		parent_bounty_id: BountyIndex,
		fee: BalanceOf<T, I>,
		value: BalanceOf<T, I>,
	) -> (BalanceOf<T, I>, BalanceOf<T, I>) {
		// Get total child bounties curator fees, and subtract it from the parent
		// curator fee (the fee in present referenced bounty, `self`).
		let children_fee = ChildBountiesCuratorFeesPerParent::<T, I>::get(parent_bounty_id);
		debug_assert!(children_fee <= fee);
		let final_fee = fee.saturating_sub(children_fee);

		// Get total child bounties value, and subtract it from the parent
		// value (the value in present referenced bounty, `self`).
		let children_value = ChildBountiesValuePerParent::<T, I>::get(parent_bounty_id);
		debug_assert!(children_value <= value);
		let value_remaining = value.saturating_sub(children_value);
		let payout = value_remaining.saturating_sub(final_fee);

		(final_fee, payout)
	}

	/// Cleanup a child-/bounty from the storage.
	fn remove_bounty(parent_bounty_id: BountyIndex, child_bounty_id: Option<BountyIndex>) {
		match child_bounty_id {
			None => {
				Bounties::<T, I>::remove(parent_bounty_id);
				BountyDescriptions::<T, I>::remove(parent_bounty_id);
				ChildBountiesPerParent::<T, I>::remove(parent_bounty_id);
				TotalChildBountiesPerParent::<T, I>::remove(parent_bounty_id);
			},
			Some(child_bounty_id) => {
				ChildBounties::<T, I>::remove(parent_bounty_id, child_bounty_id);
				ChildBountyDescriptions::<T, I>::remove(parent_bounty_id, child_bounty_id);
				ChildBountiesPerParent::<T, I>::mutate(parent_bounty_id, |count| {
					count.saturating_dec()
				});
			},
		}
	}

	/// Initiates payment from the funding source to the child-/bounty account/location.
	fn do_process_funding_payment(
		parent_bounty_id: BountyIndex,
		child_bounty_id: Option<BountyIndex>,
		asset_kind: T::AssetKind,
		value: BalanceOf<T, I>,
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

	/// Initializes payment from the child-/bounty account/location to the funding source i.e.
	/// treasury pot.
	fn do_process_refund_payment(
		parent_bounty_id: BountyIndex,
		child_bounty_id: Option<BountyIndex>,
		asset_kind: T::AssetKind,
		value: BalanceOf<T, I>,
		payment_status: Option<PaymentState<PaymentIdOf<T, I>>>,
	) -> Result<PaymentState<PaymentIdOf<T, I>>, DispatchError> {
		if let Some(payment_status) = payment_status {
			ensure!(payment_status.is_pending_or_failed(), Error::<T, I>::UnexpectedStatus);
		}

		let (source, beneficiary) = match child_bounty_id {
			None => (
				Self::bounty_account(parent_bounty_id, asset_kind.clone())?,
				Self::treasury_account(asset_kind.clone())?,
			),
			Some(child_bounty_id) => (
				Self::child_bounty_account(parent_bounty_id, child_bounty_id, asset_kind.clone())?,
				Self::bounty_account(parent_bounty_id, asset_kind.clone())?,
			),
		};

		let id = <T as Config<I>>::Paymaster::pay(&source, &beneficiary, asset_kind, value)
			.map_err(|_| Error::<T, I>::RefundError)?;

		Self::deposit_event(Event::<T, I>::Paid {
			index: parent_bounty_id,
			child_index: child_bounty_id,
			payment_id: id,
		});

		Ok(PaymentState::Attempted { id })
	}

	/// Queries the status of the refund payment from the child-/bounty account/location to the
	/// funding source and returns a new payment status.
	fn do_check_refund_payment_status(
		parent_bounty_id: BountyIndex,
		child_bounty_id: Option<BountyIndex>,
		asset_kind: T::AssetKind,
		value: BalanceOf<T, I>,
		curator_deposit: BalanceOf<T, I>,
		payment_status: PaymentState<PaymentIdOf<T, I>>,
		curator: Option<T::AccountId>,
	) -> Result<PaymentState<PaymentIdOf<T, I>>, DispatchError> {
		let payment_id = payment_status.get_attempt_id().ok_or(Error::<T, I>::UnexpectedStatus)?;

		match <T as pallet::Config<I>>::Paymaster::check_payment(payment_id) {
			PaymentStatus::Success => {
				Self::deposit_event(Event::<T, I>::BountyRefundProcessed {
					index: parent_bounty_id,
					child_index: child_bounty_id,
				});
				Ok(PaymentState::Succeeded)
			},
			PaymentStatus::InProgress =>
			// nothing new to report
				Err(Error::<T, I>::RefundInconclusive.into()),
			PaymentStatus::Unknown | PaymentStatus::Failure => {
				// assume payment has failed, allow user to retry
				Self::deposit_event(Event::<T, I>::PaymentFailed {
					index: parent_bounty_id,
					child_index: child_bounty_id,
					payment_id,
				});
				Ok(PaymentState::Failed)
			},
		}
	}

	/// Initializes payments from the child-/bounty account/location to the curator stash and
	/// beneficiary.
	fn do_process_payout_payments(
		parent_bounty_id: BountyIndex,
		child_bounty_id: Option<BountyIndex>,
		asset_kind: T::AssetKind,
		value: BalanceOf<T, I>,
		fee: BalanceOf<T, I>,
		beneficiary: (T::Beneficiary, Option<PaymentState<PaymentIdOf<T, I>>>),
		curator_stash: (T::Beneficiary, Option<PaymentState<PaymentIdOf<T, I>>>),
	) -> Result<(PaymentState<PaymentIdOf<T, I>>, PaymentState<PaymentIdOf<T, I>>), DispatchError>
	{
		// Determine if payouts to the curator stash and/or beneficiary need to be (re)processed.
		let (beneficiary_account, mut beneficiary_status) = beneficiary;
		let (curator_stash_account, mut curator_stash_status) = curator_stash;
		let process_curator_stash = curator_stash_status
			.as_ref()
			.map_or(true, |status| status.is_pending_or_failed());
		let process_beneficiary =
			beneficiary_status.as_ref().map_or(true, |status| status.is_pending_or_failed());
		ensure!(process_curator_stash || process_beneficiary, Error::<T, I>::UnexpectedStatus);

		let (final_fee, payout) =
			Self::calculate_curator_fee_and_payout(parent_bounty_id, fee, value);

		let source = match child_bounty_id {
			None => Self::bounty_account(parent_bounty_id, asset_kind.clone())?,
			Some(child_bounty_id) =>
				Self::child_bounty_account(parent_bounty_id, child_bounty_id, asset_kind.clone())?,
		};

		if process_beneficiary {
			let id = <T as Config<I>>::Paymaster::pay(
				&source,
				&beneficiary_account,
				asset_kind.clone(),
				payout,
			)
			.map_err(|_| Error::<T, I>::PayoutError)?;
			beneficiary_status = Some(PaymentState::Attempted { id });

			Self::deposit_event(Event::<T, I>::Paid {
				index: parent_bounty_id,
				child_index: child_bounty_id,
				payment_id: id,
			});
		}

		if process_curator_stash {
			let id = <T as Config<I>>::Paymaster::pay(
				&source,
				&curator_stash_account,
				asset_kind.clone(),
				final_fee,
			)
			.map_err(|_| Error::<T, I>::PayoutError)?;
			curator_stash_status = Some(PaymentState::Attempted { id });

			Self::deposit_event(Event::<T, I>::Paid {
				index: parent_bounty_id,
				child_index: child_bounty_id,
				payment_id: id,
			});
		}

		// Both will always be `Some` if we are here
		Ok((
			beneficiary_status.unwrap_or(PaymentState::Pending),
			curator_stash_status.unwrap_or(PaymentState::Pending),
		))
	}

	/// Queries the status of the payment from the child-/bounty account/location to the beneficiary
	/// and curator stash
	///
	/// If both payments succeed, the child-/bounty is removed and the curator deposit is
	/// unreserved.
	fn do_check_payout_payment_status(
		parent_bounty_id: BountyIndex,
		child_bounty_id: Option<BountyIndex>,
		asset_kind: T::AssetKind,
		value: BalanceOf<T, I>,
		curator: T::AccountId,
		fee: BalanceOf<T, I>,
		curator_deposit: BalanceOf<T, I>,
		curator_stash: (T::Beneficiary, PaymentState<PaymentIdOf<T, I>>),
		beneficiary: (T::Beneficiary, PaymentState<PaymentIdOf<T, I>>),
	) -> Result<(PaymentState<PaymentIdOf<T, I>>, PaymentState<PaymentIdOf<T, I>>), DispatchError>
	{
		// counters for payments that have changed state during this call and that have finished
		// processing successfully. For If one payment succeeds and another fails, both count as
		// "progressed" since they advanced the state machine.
		let (mut payments_progressed, mut payments_succeeded) = (0, 0);
		// check both curator stash, and beneficiary payments
		let (mut beneficiary_status, mut curator_stash_status) = (beneficiary.1, curator_stash.1);
		for payment_status in [&mut beneficiary_status, &mut curator_stash_status] {
			match payment_status {
				PaymentState::Attempted { id } =>
					match <T as pallet::Config<I>>::Paymaster::check_payment(*id) {
						PaymentStatus::Success => {
							payments_succeeded += 1;
							payments_progressed += 1;
							*payment_status = PaymentState::Succeeded;
						},
						PaymentStatus::InProgress => {
							// nothing new to report, return function without
							// error so we could drive the next
							// payment
						},
						PaymentStatus::Unknown | PaymentStatus::Failure => {
							payments_progressed += 1;
							Self::deposit_event(Event::<T, I>::PaymentFailed {
								index: parent_bounty_id,
								child_index: child_bounty_id,
								payment_id: *id,
							});
							*payment_status = PaymentState::Failed;
						},
					},
				PaymentState::Succeeded => {
					payments_succeeded += 1;
				},
				_ => {}, // return function without error so we could drive the next payment
			}
		}

		// best scenario, both payments have succeeded,
		if payments_succeeded >= 2 {
			let (_final_fee, payout) =
				Self::calculate_curator_fee_and_payout(parent_bounty_id, fee, value);
			Self::deposit_event(Event::<T, I>::BountyPayoutProcessed {
				index: parent_bounty_id,
				child_index: child_bounty_id,
				asset_kind: asset_kind.clone(),
				value: payout,
				beneficiary: beneficiary.0,
			});
			return Ok((curator_stash_status, beneficiary_status));
		} else if payments_progressed > 0 {
			// some payments have progressed in the state machine
			// return ok so these changes are saved to the state
			return Ok((curator_stash_status, beneficiary_status));
		} else {
			// no progress was made in the state machine if we're here,
			return Err(Error::<T, I>::PayoutInconclusive.into())
		}
	}
}

/// Derives the treasury pot account/location from an `AssetKind`.
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

/// Derives a parent bounty account/location from its index and an `AssetKind`.
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

/// Derives a child-bounty account/location from its index, the parent bounty index and an
/// `AssetKind`.
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
