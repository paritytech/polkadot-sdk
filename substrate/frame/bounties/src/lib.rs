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

// TODO: update docs
//! # Bounties Module ( pallet-bounties )
//!
//! ## Bounty
//!
//! > NOTE: This pallet is tightly coupled with pallet-treasury.
//!
//! A Bounty Spending is a reward for a specified body of work - or specified set of objectives -
//! that needs to be executed for a predefined Treasury amount to be paid out. A curator is assigned
//! after the bounty is approved and funded by Council, to be delegated with the responsibility of
//! assigning a payout address once the specified set of objectives is completed.
//!
//! After the Council has activated a bounty, it delegates the work that requires expertise to a
//! curator in exchange of a deposit. Once the curator accepts the bounty, they get to close the
//! active bounty. Closing the active bounty enacts a delayed payout to the payout address, the
//! curator fee and the return of the curator deposit. The delay allows for intervention through
//! regular democracy. The Council gets to unassign the curator, resulting in a new curator
//! election. The Council also gets to cancel the bounty if deemed necessary before assigning a
//! curator or once the bounty is active or payout is pending, resulting in the slash of the
//! curator's deposit.
//!
//! This pallet may opt into using a [`ChildBountyManager`] that enables bounties to be split into
//! sub-bounties, as children of an established bounty (called the parent in the context of it's
//! children).
//!
//! > NOTE: The parent bounty cannot be closed if it has a non-zero number of it has active child
//! > bounties associated with it.
//!
//! ### Terminology
//!
//! Bounty:
//!
//! - **Bounty spending proposal:** A proposal to reward a predefined body of work upon completion
//!   by the Treasury.
//! - **Proposer:** An account proposing a bounty spending.
//! - **Curator:** An account managing the bounty and assigning a payout address receiving the
//!   reward for the completion of work.
//! - **Deposit:** The amount held on deposit for placing a bounty proposal plus the amount held on
//!   deposit per byte within the bounty description.
//! - **Curator deposit:** The payment from a candidate willing to curate an approved bounty. The
//!   deposit is returned when/if the bounty is completed.
//! - **Bounty value:** The total amount that should be paid to the Payout Address if the bounty is
//!   rewarded.
//! - **Payout address:** The account to which the total or part of the bounty is assigned to.
//! - **Payout Delay:** The delay period for which a bounty beneficiary needs to wait before
//!   claiming.
//! - **Curator fee:** The reserved upfront payment for a curator for work related to the bounty.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! Bounty protocol:
//!
//! - `propose_bounty` - Propose a specific treasury amount to be earmarked for a predefined set of
//!   tasks and stake the required deposit.
//! - `approve_bounty` - Accept a specific treasury amount to be earmarked for a predefined body of
//!   work.
//! - `propose_curator` - Assign an account to a bounty as candidate curator.
//! - `approve_bounty_with_curator` - Accept a specific treasury amount for a predefined body of
//!   work with assigned candidate curator account.
//! - `accept_curator` - Accept a bounty assignment from the Council, setting a curator deposit.
//! - `extend_bounty_expiry` - Extend the expiry block number of the bounty and stay active.
//! - `award_bounty` - Close and pay out the specified amount for the completed work.
//! - `claim_bounty` - Claim a specific bounty amount from the Payout Address.
//! - `unassign_curator` - Unassign an accepted curator from a specific earmark.
//! - `close_bounty` - Cancel the earmark for a specific treasury amount and close the bounty.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod migrations;
#[cfg(test)]
mod tests;
pub mod weights;

extern crate alloc;

use alloc::vec::Vec;

use frame_support::traits::{
	tokens::{ConversionFromAssetBalance, Pay, PaymentStatus},
	Get, OnUnbalanced, ReservableCurrency,
};

use sp_runtime::{
	traits::{AccountIdConversion, BadOrigin, BlockNumberProvider, Saturating, StaticLookup, Zero},
	DispatchResult, Permill, RuntimeDebug,
};

use frame_support::{dispatch::DispatchResultWithPostInfo, traits::EnsureOrigin};

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::{
	ensure_signed, BlockNumberFor as SystemBlockNumberFor, OriginFor,
};
use scale_info::TypeInfo;
pub use weights::WeightInfo;

pub use pallet::*;

type DepositBalanceOf<T, I = ()> = pallet_treasury::BalanceOf<T, I>;
type BountyBalanceOf<T, I = ()> = pallet_treasury::AssetBalanceOf<T, I>;
type BeneficiaryLookupOf<T, I = ()> = pallet_treasury::BeneficiaryLookupOf<T, I>;
type PaymentIdOf<T, I = ()> = <<T as pallet_treasury::Config<I>>::Paymaster as Pay>::Id;

/// An index of a bounty. Just a `u32`.
pub type BountyIndex = u32;

type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;
type BountyOf<T, I> = Bounty<
	<T as frame_system::Config>::AccountId,
	DepositBalanceOf<T, I>,
	BountyBalanceOf<T, I>,
	BlockNumberFor<T, I>,
	<T as pallet_treasury::Config<I>>::AssetKind,
	PaymentIdOf<T, I>,
	<T as pallet_treasury::Config<I>>::Beneficiary,
>;
type BountyStatusOf<T, I> = BountyStatus<
	<T as frame_system::Config>::AccountId,
	BlockNumberFor<T, I>,
	PaymentIdOf<T, I>,
	<T as pallet_treasury::Config<I>>::Beneficiary,
>;
type BlockNumberFor<T, I = ()> =
	<<T as pallet_treasury::Config<I>>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;

/// A bounty proposal.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct Bounty<
	AccountId,
	NativeBalance,
	BountyBalance,
	BlockNumber,
	AssetKind,
	PaymentId,
	Beneficiary,
> where
	Beneficiary: Clone,
	PaymentId: Clone,
{
	/// The account proposing it.
	proposer: AccountId,
	// TODO: new filed, migration required.
	/// The kind of asset this bounty is rewarded in.
	pub asset_kind: AssetKind,
	/// The (total) amount of the `asset_kind` that should be paid if the bounty is rewarded.
	value: BountyBalance,
	/// The curator fee in the `asset_kind`. Included in value.
	fee: BountyBalance,
	/// The deposit of curator.
	///
	/// The asset class determined by the [`pallet_treasury::Config::Currency`].
	curator_deposit: NativeBalance,
	/// The amount held on deposit (reserved) for making this proposal.
	///
	/// The asset class determined by the [`pallet_treasury::Config::Currency`].
	bond: NativeBalance,
	/// The status of this bounty.
	status: BountyStatus<AccountId, BlockNumber, PaymentId, Beneficiary>,
}

impl<
		AccountId: PartialEq + Clone + Ord,
		NativeBalance,
		BountyBalance,
		BlockNumber: Clone,
		AssetKind,
		PaymentId: Clone,
		Beneficiary: Clone,
	> Bounty<AccountId, NativeBalance, BountyBalance, BlockNumber, AssetKind, PaymentId, Beneficiary>
{
	/// Getter for bounty status, to be used for child bounties.
	pub fn get_status(&self) -> BountyStatus<AccountId, BlockNumber, PaymentId, Beneficiary> {
		self.status.clone()
	}
}

// TODO: breaking changes to the stored type, migration required.

/// The status of a bounty proposal.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum BountyStatus<AccountId, BlockNumber, PaymentId, Beneficiary>
where
	Beneficiary: Clone,
	PaymentId: Clone,
{
	/// The bounty is proposed and waiting for approval.
	Proposed,
	/// The bounty is approved and waiting to confirm the funds allocation.
	Approved {
		/// The status of the bounty amount transfer from the source (e.g. Treasury) to
		/// the bounty account.
		///
		/// Once the payment is confirmed, the bounty will transition to either
		/// [`BountyStatus::Funded`]
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
		/// The curator's stash account used as a fee destination.
		curator_stash: Beneficiary,
		/// An update from the curator is due by this block, else they are considered inactive.
		update_due: BlockNumber,
	},
	/// The bounty is awarded and waiting to released after a delay.
	PendingPayout {
		/// The curator of this bounty.
		curator: AccountId,
		/// The curator's stash account used as a fee destination.
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
		/// Once the payment is confirmed, the bounty will transition to
		/// [`BountyStatus::CuratorProposed`], depending on the value
		payment_status: PaymentState<PaymentId>,
	},
	/// The bounty payout has been attempted.
	///
	/// In case of a failed payout, the payout can be retried. Once the payout is successful, the
	/// bounty is completed and removed from the storage.
	PayoutAttempted {
		/// The curator of this bounty.
		curator: AccountId,
		/// The curator's stash account with the payout status.
		curator_stash: (Beneficiary, PaymentState<PaymentId>),
		/// The beneficiary's stash account with the payout status.
		beneficiary: (Beneficiary, PaymentState<PaymentId>),
	},
	/// The bounty is closed, and the funds are being refunded to the original source (e.g.,
	/// Treasury).
	RefundAttempted {
		/// The refund status.
		///
		/// Once the refund is successful, the bounty is removed from the storage.
		payment_status: PaymentState<PaymentId>,
	},
}

/// The state of the payment claim.
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

/// The child bounty manager.
pub trait ChildBountyManager<Balance> {
	/// Get the active child bounties for a parent bounty.
	fn child_bounties_count(bounty_id: BountyIndex) -> BountyIndex;

	/// Take total curator fees of children-bounty curators.
	fn children_curator_fees(bounty_id: BountyIndex) -> Balance;

	/// Hook called when a parent bounty is removed.
	fn bounty_removed(bounty_id: BountyIndex);
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(4);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config + pallet_treasury::Config<I> {
		// TODO: since we break the API with this iteration it may be reasonable to migrate to
		// `Considerations` and remove old config parameters relater to deposits. This is optional
		// for current PR.

		/// The amount held on deposit for placing a bounty proposal.
		#[pallet::constant]
		type BountyDepositBase: Get<DepositBalanceOf<Self, I>>;

		/// The delay period for which a bounty beneficiary need to wait before claim the payout.
		#[pallet::constant]
		type BountyDepositPayoutDelay: Get<BlockNumberFor<Self, I>>;

		/// Bounty duration in blocks.
		#[pallet::constant]
		type BountyUpdatePeriod: Get<BlockNumberFor<Self, I>>;

		/// The curator deposit is calculated as a percentage of the curator fee.
		///
		/// This deposit has optional upper and lower bounds with `CuratorDepositMax` and
		/// `CuratorDepositMin`.
		#[pallet::constant]
		type CuratorDepositMultiplier: Get<Permill>;

		/// Maximum amount of funds that should be placed in a deposit for making a proposal.
		#[pallet::constant]
		type CuratorDepositMax: Get<Option<DepositBalanceOf<Self, I>>>;

		/// Minimum amount of funds that should be placed in a deposit for making a proposal.
		#[pallet::constant]
		type CuratorDepositMin: Get<Option<DepositBalanceOf<Self, I>>>;

		/// Minimum value for a bounty.
		#[pallet::constant]
		type BountyValueMinimum: Get<DepositBalanceOf<Self, I>>;

		/// The amount held on deposit per byte within the tip report reason or bounty description.
		#[pallet::constant]
		type DataDepositPerByte: Get<DepositBalanceOf<Self, I>>;

		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: benchmarking::ArgumentsFactory<Self::AssetKind>;

		/// The overarching event type.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Maximum acceptable reason length.
		///
		/// Benchmarks depend on this value, be sure to update weights file when changing this value
		#[pallet::constant]
		type MaximumReasonLength: Get<u32>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// The child bounty manager.
		type ChildBountyManager: ChildBountyManager<BountyBalanceOf<Self, I>>;

		/// Handler for the unbalanced decrease when slashing for a rejected bounty.
		type OnSlash: OnUnbalanced<pallet_treasury::NegativeImbalanceOf<Self, I>>;
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// Proposer's balance is too low.
		InsufficientProposersBalance,
		/// No proposal or bounty at that index.
		InvalidIndex,
		/// The reason given is just too big.
		ReasonTooBig,
		/// The bounty status is unexpected.
		UnexpectedStatus,
		/// Require bounty curator.
		RequireCurator,
		/// Invalid bounty value.
		InvalidValue,
		/// Invalid bounty fee.
		InvalidFee,
		/// A bounty payout is pending.
		/// To cancel the bounty, you must unassign and slash the curator.
		PendingPayout,
		/// The bounties cannot be claimed/closed because it's still in the countdown period.
		Premature,
		/// The bounty cannot be closed because it has active child bounties.
		HasActiveChildBounty,
		/// Too many approvals are already queued.
		TooManyQueued,
		/// There was issue with funding the bounty
		FundingError,
		/// Bounty funding has not concluded yet
		FundingInconclusive,
		/// There was issue paying out the bounty
		PayoutError,
		/// No progress in payouts was made
		PayoutInconclusive,
		/// There was issue with refunding the bounty
		RefundError,
		/// No progress was made processing a refund
		RefundInconclusive,
	}

	// TODO: add new parameters for events

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// New bounty proposal.
		BountyProposed { index: BountyIndex },
		/// A bounty proposal was rejected; funds were slashed.
		BountyRejected { index: BountyIndex, bond: DepositBalanceOf<T, I> },
		/// A bounty proposal is funded and became active.
		BountyBecameActive { index: BountyIndex },
		/// A bounty is awarded to a beneficiary.
		BountyAwarded { index: BountyIndex, beneficiary: T::Beneficiary },
		/// A bounty is claimed by beneficiary.
		BountyClaimed {
			index: BountyIndex,
			asset_kind: T::AssetKind,
			asset_payout: BountyBalanceOf<T, I>,
			beneficiary: T::Beneficiary,
		},
		/// A bounty is cancelled.
		BountyCanceled { index: BountyIndex },
		/// A bounty expiry is extended.
		BountyExtended { index: BountyIndex },
		/// A bounty is approved.
		BountyApproved { index: BountyIndex },
		/// A bounty curator is proposed.
		CuratorProposed { bounty_id: BountyIndex, curator: T::AccountId },
		/// A bounty curator is unassigned.
		CuratorUnassigned { bounty_id: BountyIndex },
		/// A bounty curator is accepted.
		CuratorAccepted { bounty_id: BountyIndex, curator: T::AccountId },
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

	// Tiago: how to remove if it is used in migrations/v4.rs?
	// TODO: most probably wont be needed, review and remove if not needed.
	/// Bounty indices that have been approved but not yet funded.
	#[pallet::storage]
	#[allow(deprecated)]
	pub type BountyApprovals<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BoundedVec<BountyIndex, T::MaxApprovals>, ValueQuery>;

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		// TODO: use pallet_treasury::Config::Paymaster for all transfers (except deposits).

		// TODO: update doc.
		/// Propose a new bounty.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// Payment: `TipReportDepositBase` will be reserved from the origin account, as well as
		/// `DataDepositPerByte` for each byte in `reason`. It will be unreserved upon approval,
		/// or slashed when rejected.
		///
		/// - `curator`: The curator account whom will manage this bounty.
		/// - `fee`: The curator fee.
		/// - `value`: The total payment amount of this bounty, curator fee included.
		/// - `description`: The description of this bounty.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::propose_bounty(description.len() as u32))]
		pub fn propose_bounty(
			origin: OriginFor<T>,
			asset_kind: Box<T::AssetKind>,
			// TODO: `value` should pallet_treasury::BountyBalanceOf<T, I>,
			// or better make pallet_treasury::BountyBalanceOf<T, I> and
			// pallet_treasury::BalanceOf<T, I> same types for simplicity.
			#[pallet::compact] value: BountyBalanceOf<T, I>,
			description: Vec<u8>,
		) -> DispatchResult {
			let proposer = ensure_signed(origin)?;
			Self::create_bounty(proposer, description, *asset_kind, value)?;
			Ok(())
		}

		/// Approve a bounty proposal. At a later time, the bounty will be funded and become active
		/// and the original deposit will be returned.
		///
		/// May only be called from `T::SpendOrigin`.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::approve_bounty())]
		pub fn approve_bounty(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
		) -> DispatchResult {
			let max_amount = T::SpendOrigin::ensure_origin(origin)?;
			Bounties::<T, I>::try_mutate_exists(bounty_id, |maybe_bounty| -> DispatchResult {
				let bounty = maybe_bounty.as_mut().ok_or(Error::<T, I>::InvalidIndex)?;
				let native_amount =
					<T as pallet_treasury::Config<I>>::BalanceConverter::from_asset_balance(
						bounty.value,
						bounty.asset_kind.clone(),
					)
					.map_err(|_| pallet_treasury::Error::<T, I>::FailedToConvertBalance)?;
				ensure!(
					native_amount <= max_amount,
					pallet_treasury::Error::<T, I>::InsufficientPermission
				);
				ensure!(bounty.status == BountyStatus::Proposed, Error::<T, I>::UnexpectedStatus);

				let bounty_account = Self::bounty_account_id(bounty_id);
				let treasury_account = Self::account_id();
				let payment_id = T::Paymaster::pay(
					&treasury_account,
					&bounty_account,
					bounty.asset_kind.clone(),
					bounty.value,
				)
				.map_err(|_| Error::<T, I>::FundingError)?;

				bounty.status = BountyStatus::Approved {
					payment_status: PaymentState::Attempted { id: payment_id },
				};

				Ok(())
			})?;

			Self::deposit_event(Event::<T, I>::BountyApproved { index: bounty_id });
			Ok(())
		}

		/// Propose a curator to a funded bounty.
		///
		/// May only be called from `T::SpendOrigin`.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::propose_curator())]
		pub fn propose_curator(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
			curator: AccountIdLookupOf<T>,
			#[pallet::compact] fee: BountyBalanceOf<T, I>,
		) -> DispatchResult {
			let max_amount = T::SpendOrigin::ensure_origin(origin)?;

			let curator = T::Lookup::lookup(curator)?;
			Bounties::<T, I>::try_mutate_exists(bounty_id, |maybe_bounty| -> DispatchResult {
				let bounty = maybe_bounty.as_mut().ok_or(Error::<T, I>::InvalidIndex)?;
				let native_amount =
					<T as pallet_treasury::Config<I>>::BalanceConverter::from_asset_balance(
						bounty.value,
						bounty.asset_kind.clone(),
					)
					.map_err(|_| pallet_treasury::Error::<T, I>::FailedToConvertBalance)?;
				ensure!(
					native_amount <= max_amount,
					pallet_treasury::Error::<T, I>::InsufficientPermission
				);
				// Tiago: maybe we should come up with a different error
				if bounty.status != BountyStatus::Funded {
					return Err(Error::<T, I>::UnexpectedStatus.into());
				}

				ensure!(fee < bounty.value, Error::<T, I>::InvalidFee);

				bounty.status = BountyStatus::CuratorProposed { curator: curator.clone() };
				bounty.fee = fee;

				Self::deposit_event(Event::<T, I>::CuratorProposed { bounty_id, curator });

				Ok(())
			})?;
			Ok(())
		}

		/// Unassign curator from a bounty.
		///
		/// This function can only be called by the `RejectOrigin` a signed origin.
		///
		/// If this function is called by the `RejectOrigin`, we assume that the curator is
		/// malicious or inactive. As a result, we will slash the curator when possible.
		///
		/// If the origin is the curator, we take this as a sign they are unable to do their job and
		/// they willingly give up. We could slash them, but for now we allow them to recover their
		/// deposit and exit without issue. (We may want to change this if it is abused.)
		///
		/// Finally, the origin can be anyone if and only if the curator is "inactive". This allows
		/// anyone in the community to call out that a curator is not doing their due diligence, and
		/// we should pick a new curator. In this case the curator should also be slashed.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::unassign_curator())]
		pub fn unassign_curator(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
		) -> DispatchResult {
			let maybe_sender = ensure_signed(origin.clone())
				.map(Some)
				.or_else(|_| T::RejectOrigin::ensure_origin(origin).map(|_| None))?;

			Bounties::<T, I>::try_mutate_exists(bounty_id, |maybe_bounty| -> DispatchResult {
				let bounty = maybe_bounty.as_mut().ok_or(Error::<T, I>::InvalidIndex)?;

				let slash_curator =
					|curator: &T::AccountId, curator_deposit: &mut DepositBalanceOf<T, I>| {
						let imbalance = T::Currency::slash_reserved(curator, *curator_deposit).0;
						T::OnSlash::on_unbalanced(imbalance);
						*curator_deposit = Zero::zero();
					};

				match bounty.status {
					BountyStatus::Proposed |
					BountyStatus::Approved { .. } |
					BountyStatus::Funded |
					BountyStatus::PayoutAttempted { .. } |
					BountyStatus::RefundAttempted { .. } => {
						// No curator to unassign at this point.
						return Err(Error::<T, I>::UnexpectedStatus.into());
					},
					BountyStatus::ApprovedWithCurator { ref curator, ref payment_status } => {
						// Bounty not yet funded, but bounty was approved with curator.
						// `RejectOrigin` or curator himself can unassign from this bounty.
						ensure!(maybe_sender.map_or(true, |sender| sender == *curator), BadOrigin);
						// This state can only be while the bounty is not yet funded so we return
						// bounty to the `Approved` state without curator
						bounty.status =
							BountyStatus::Approved { payment_status: payment_status.clone() };
						return Ok(());
					},
					BountyStatus::CuratorProposed { ref curator } => {
						// A curator has been proposed, but not accepted yet.
						// Either `RejectOrigin` or the proposed curator can unassign the curator.
						ensure!(maybe_sender.map_or(true, |sender| sender == *curator), BadOrigin);
					},
					BountyStatus::Active { ref curator, ref update_due, .. } => {
						// The bounty is active.
						match maybe_sender {
							// If the `RejectOrigin` is calling this function, slash the curator.
							None => {
								slash_curator(curator, &mut bounty.curator_deposit);
								// Continue to change bounty status below...
							},
							Some(sender) => {
								// If the sender is not the curator, and the curator is inactive,
								// slash the curator.
								if sender != *curator {
									let block_number = Self::treasury_block_number();
									if *update_due < block_number {
										slash_curator(curator, &mut bounty.curator_deposit);
									// Continue to change bounty status below...
									} else {
										// Curator has more time to give an update.
										return Err(Error::<T, I>::Premature.into());
									}
								} else {
									// Else this is the curator, willingly giving up their role.
									// Give back their deposit.
									let err_amount =
										T::Currency::unreserve(curator, bounty.curator_deposit);
									debug_assert!(err_amount.is_zero());
									bounty.curator_deposit = Zero::zero();
									// Continue to change bounty status below...
								}
							},
						}
					},
					BountyStatus::PendingPayout { ref curator, .. } => {
						// The bounty is pending payout, so only council can unassign a curator.
						// By doing so, they are claiming the curator is acting maliciously, so
						// we slash the curator.
						ensure!(maybe_sender.is_none(), BadOrigin);
						slash_curator(curator, &mut bounty.curator_deposit);
						// Continue to change bounty status below...
					},
				};

				bounty.status = BountyStatus::Funded;
				Ok(())
			})?;

			Self::deposit_event(Event::<T, I>::CuratorUnassigned { bounty_id });
			Ok(())
		}

		// TODO: update doc, describe parameters.
		/// Accept the curator role for a bounty.
		/// A deposit will be reserved from curator and refund upon successful payout.
		///
		/// May only be called from the curator.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::accept_curator())]
		pub fn accept_curator(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
			// TODO: docs: curator stash account to receive the curator fee.
			stash: BeneficiaryLookupOf<T, I>,
		) -> DispatchResult {
			let signer = ensure_signed(origin)?;
			let stash = T::BeneficiaryLookup::lookup(stash)?;

			Bounties::<T, I>::try_mutate_exists(bounty_id, |maybe_bounty| -> DispatchResult {
				let bounty = maybe_bounty.as_mut().ok_or(Error::<T, I>::InvalidIndex)?;

				match bounty.status {
					BountyStatus::CuratorProposed { ref curator } => {
						ensure!(signer == *curator, Error::<T, I>::RequireCurator);

						let deposit = Self::calculate_curator_deposit(
							&bounty.fee,
							bounty.asset_kind.clone(),
						)?;
						T::Currency::reserve(curator, deposit)?;
						bounty.curator_deposit = deposit;
						
						let update_due =
							Self::treasury_block_number() + T::BountyUpdatePeriod::get();
						bounty.status = BountyStatus::Active {
							curator: curator.clone(),
							curator_stash: stash,
							update_due,
						};

						Self::deposit_event(Event::<T, I>::CuratorAccepted {
							bounty_id,
							curator: signer,
						});
						Ok(())
					},
					_ => Err(Error::<T, I>::UnexpectedStatus.into()),
				}
			})?;
			Ok(())
		}

		/// Award bounty to a beneficiary account. The beneficiary will be able to claim the funds
		/// after a delay.
		///
		/// The dispatch origin for this call must be the curator of this bounty.
		///
		/// - `bounty_id`: Bounty ID to award.
		/// - `beneficiary`: The beneficiary account whom will receive the payout.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::award_bounty())]
		pub fn award_bounty(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
			beneficiary: BeneficiaryLookupOf<T, I>,
		) -> DispatchResult {
			let signer = ensure_signed(origin)?;
			let beneficiary = T::BeneficiaryLookup::lookup(beneficiary)?;

			Bounties::<T, I>::try_mutate_exists(bounty_id, |maybe_bounty| -> DispatchResult {
				let bounty = maybe_bounty.as_mut().ok_or(Error::<T, I>::InvalidIndex)?;

				// Ensure no active child bounties before processing the call.
				ensure!(
					T::ChildBountyManager::child_bounties_count(bounty_id) == 0,
					Error::<T, I>::HasActiveChildBounty
				);

				match &bounty.status {
					BountyStatus::Active { curator, curator_stash, .. } => {
						ensure!(signer == *curator, Error::<T, I>::RequireCurator);
						bounty.status = BountyStatus::PendingPayout {
							curator: signer,
							beneficiary: beneficiary.clone(),
							unlock_at: Self::treasury_block_number() +
								T::BountyDepositPayoutDelay::get(),
							curator_stash: curator_stash.clone(),
						};
					},
					_ => return Err(Error::<T, I>::UnexpectedStatus.into()),
				}

				Ok(())
			})?;

			Self::deposit_event(Event::<T, I>::BountyAwarded { index: bounty_id, beneficiary });
			Ok(())
		}

		// TODO: should be able to retry claim if the payment from prev claim attempt failed.
		/// Claim the payout from an awarded bounty after payout delay.
		///
		/// The dispatch origin for this call must be the beneficiary of this bounty.
		///
		/// - `bounty_id`: Bounty ID to claim.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::claim_bounty())]
		pub fn claim_bounty(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
		) -> DispatchResult {
			let _ = ensure_signed(origin)?; // anyone can trigger claim

			Bounties::<T, I>::try_mutate_exists(bounty_id, |maybe_bounty| -> DispatchResult {
				let bounty = maybe_bounty.as_mut().ok_or(Error::<T, I>::InvalidIndex)?;

				if let BountyStatus::PendingPayout {
					curator, beneficiary, unlock_at, curator_stash
				} = &bounty.status
				{
					ensure!(Self::treasury_block_number() >= *unlock_at, Error::<T, I>::Premature);

					let (final_fee, payout) =
						Self::calculate_curator_fee_and_payout(bounty_id, bounty.fee, bounty.value);
					let bounty_account = Self::bounty_account_id(bounty_id);

					let curator_payment_id = T::Paymaster::pay(
						&bounty_account,
						&curator_stash,
						bounty.asset_kind.clone(),
						final_fee,
					)
					.map_err(|_| Error::<T, I>::PayoutError)?;
					let beneficiary_payment_id = T::Paymaster::pay(
						&bounty_account,
						&beneficiary,
						bounty.asset_kind.clone(),
						payout,
					)
					.map_err(|_| Error::<T, I>::PayoutError)?;

					bounty.status = BountyStatus::PayoutAttempted {
						curator: curator.clone(),
						curator_stash: (
							curator_stash.clone(),
							PaymentState::Attempted { id: curator_payment_id },
						),
						beneficiary: (
							beneficiary.clone(),
							PaymentState::Attempted { id: beneficiary_payment_id },
						),
					};

					Ok(())
				} else {
					Err(Error::<T, I>::UnexpectedStatus.into())
				}
			})?;
			Ok(())
		}

		/// Cancel a proposed or active bounty. All the funds will be sent to treasury and
		/// the curator deposit will be unreserved if possible.
		///
		/// Only `T::RejectOrigin` is able to cancel a bounty.
		///
		/// - `bounty_id`: Bounty ID to cancel.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(7)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::close_bounty_proposed()
			.max(<T as Config<I>>::WeightInfo::close_bounty_active()))]
		pub fn close_bounty(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
		) -> DispatchResultWithPostInfo {
			T::RejectOrigin::ensure_origin(origin)?;

			Bounties::<T, I>::try_mutate_exists(
				bounty_id,
				|maybe_bounty| -> DispatchResultWithPostInfo {
					let bounty = maybe_bounty.as_mut().ok_or(Error::<T, I>::InvalidIndex)?;

					// Ensure no active child bounties before processing the call.
					ensure!(
						T::ChildBountyManager::child_bounties_count(bounty_id) == 0,
						Error::<T, I>::HasActiveChildBounty
					);

					match &bounty.status {
						BountyStatus::Proposed => {
							// The reject origin would like to cancel a proposed bounty.
							BountyDescriptions::<T, I>::remove(bounty_id);
							let value = bounty.bond;
							let imbalance = T::Currency::slash_reserved(&bounty.proposer, value).0;
							T::OnSlash::on_unbalanced(imbalance);
							*maybe_bounty = None;

							Self::deposit_event(Event::<T, I>::BountyRejected {
								index: bounty_id,
								bond: value,
							});
							// Return early, nothing else to do.
							return Ok(
								Some(<T as Config<I>>::WeightInfo::close_bounty_proposed()).into()
							)
						},
						BountyStatus::Approved { .. } |
						BountyStatus::ApprovedWithCurator { .. } => {
							// For weight reasons, we don't allow a council to cancel in this phase.
							// We ask for them to wait until it is funded before they can cancel.
							return Err(Error::<T, I>::UnexpectedStatus.into())
						},
						BountyStatus::Funded | BountyStatus::CuratorProposed { .. } => {
							// Nothing extra to do besides the removal of the bounty below.
						},
						BountyStatus::Active { curator, .. } => {
							// Cancelled by council, refund deposit of the working curator.
							let err_amount =
								T::Currency::unreserve(curator, bounty.curator_deposit);
							debug_assert!(err_amount.is_zero());
							// Then execute removal of the bounty below.
						},
						BountyStatus::PendingPayout { .. } |
						BountyStatus::PayoutAttempted { .. } => {
							// Bounty is already pending payout. If council wants to cancel
							// this bounty, it should mean the curator was acting maliciously.
							// So the council should first unassign the curator, slashing their
							// deposit.
							return Err(Error::<T, I>::PendingPayout.into())
						},
						BountyStatus::RefundAttempted { .. } => {
							// Bounty refund is already attempted. Flow should be
							// finished with calling `check_payment_status`
							// or retrying payment with `process_payment`
							// if it failed
							return Err(Error::<T, I>::PendingPayout.into())
						},
					}

					let treasury_account = Self::account_id();
					let bounty_account = Self::bounty_account_id(bounty_id);
					let payment_id = T::Paymaster::pay(
						&bounty_account,
						&treasury_account,
						bounty.asset_kind.clone(),
						bounty.value,
					)
					.map_err(|_| Error::<T, I>::RefundError)?;
					bounty.status = BountyStatus::RefundAttempted {
						payment_status: PaymentState::Attempted { id: payment_id },
					};

					Ok(Some(<T as Config<I>>::WeightInfo::close_bounty_proposed()).into())
				},
			)
		}

		/// Extend the expiry time of an active bounty.
		///
		/// The dispatch origin for this call must be the curator of this bounty.
		///
		/// - `bounty_id`: Bounty ID to extend.
		/// - `remark`: additional information.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(8)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::extend_bounty_expiry())]
		pub fn extend_bounty_expiry(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
			_remark: Vec<u8>,
		) -> DispatchResult {
			let signer = ensure_signed(origin)?;

			Bounties::<T, I>::try_mutate_exists(bounty_id, |maybe_bounty| -> DispatchResult {
				let bounty = maybe_bounty.as_mut().ok_or(Error::<T, I>::InvalidIndex)?;

				match bounty.status {
					BountyStatus::Active { ref curator, ref mut update_due, .. } => {
						ensure!(*curator == signer, Error::<T, I>::RequireCurator);
						*update_due = (Self::treasury_block_number() +
							T::BountyUpdatePeriod::get())
						.max(*update_due);
					},
					_ => return Err(Error::<T, I>::UnexpectedStatus.into()),
				}

				Ok(())
			})?;

			Self::deposit_event(Event::<T, I>::BountyExtended { index: bounty_id });
			Ok(())
		}

		/// Approve bountry and propose a curator simultaneously.
		/// This call is a shortcut to calling `approve_bounty` and `propose_curator` separately.
		///
		/// May only be called from `T::SpendOrigin`.
		///
		/// - `bounty_id`: Bounty ID to approve.
		/// - `curator`: The curator account whom will manage this bounty.
		/// - `fee`: The curator fee.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(9)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::approve_bounty_with_curator())]
		pub fn approve_bounty_with_curator(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
			curator: AccountIdLookupOf<T>,
			#[pallet::compact] fee: BountyBalanceOf<T, I>,
		) -> DispatchResult {
			let max_amount = T::SpendOrigin::ensure_origin(origin)?;
			let curator = T::Lookup::lookup(curator)?;
			Bounties::<T, I>::try_mutate_exists(bounty_id, |maybe_bounty| -> DispatchResult {
				// approve bounty
				let bounty = maybe_bounty.as_mut().ok_or(Error::<T, I>::InvalidIndex)?;
				let native_amount =
					<T as pallet_treasury::Config<I>>::BalanceConverter::from_asset_balance(
						bounty.value,
						bounty.asset_kind.clone(),
					)
					.map_err(|_| pallet_treasury::Error::<T, I>::FailedToConvertBalance)?;
				ensure!(
					native_amount <= max_amount,
					pallet_treasury::Error::<T, I>::InsufficientPermission
				);
				ensure!(bounty.status == BountyStatus::Proposed, Error::<T, I>::UnexpectedStatus);
				ensure!(fee < bounty.value, Error::<T, I>::InvalidFee);

				let bounty_account = Self::bounty_account_id(bounty_id);
				let treasury_account = Self::account_id();
				let payment_id = T::Paymaster::pay(
					&treasury_account,
					&bounty_account,
					bounty.asset_kind.clone(),
					bounty.value,
				)
				.map_err(|_| Error::<T, I>::FundingError)?;
				
				bounty.status = BountyStatus::ApprovedWithCurator {
					curator: curator.clone(),
					payment_status: PaymentState::Attempted { id: payment_id },
				};
				bounty.fee = fee;

				Ok(())
			})?;

			Self::deposit_event(Event::<T, I>::BountyApproved { index: bounty_id });
			Self::deposit_event(Event::<T, I>::CuratorProposed { bounty_id, curator });

			Ok(())
		}

		// TODO: retries payment for funding the bounty or closing the bounty. updates the bounty
		// status or removed it (when payment successful and bounty is closed).
		#[pallet::call_index(10)]
		// TODO: change weight
		#[pallet::weight(<T as Config<I>>::WeightInfo::approve_bounty_with_curator())]
		pub fn process_payment(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
		) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;

			Bounties::<T, I>::try_mutate_exists(
				bounty_id,
				|maybe_bounty| -> DispatchResultWithPostInfo {
					let bounty = maybe_bounty.as_mut().ok_or(Error::<T, I>::InvalidIndex)?;
					match bounty.status {
						BountyStatus::Approved { ref mut payment_status } => {
							let treasury_account = Self::account_id();
							let bounty_account = Self::bounty_account_id(bounty_id);
							Self::process_payment_status(
								&treasury_account,
								&bounty_account,
								&bounty.asset_kind,
								bounty.value,
								payment_status,
							)
						},
						BountyStatus::ApprovedWithCurator { ref mut payment_status, .. } => {
							let treasury_account = Self::account_id();
							let bounty_account = Self::bounty_account_id(bounty_id);
							Self::process_payment_status(
								&treasury_account,
								&bounty_account,
								&bounty.asset_kind,
								bounty.value,
								payment_status,
							)
						},
						BountyStatus::PayoutAttempted {
							ref mut curator_stash,
							ref mut beneficiary,
							..
						} => {
							let (final_fee, payout) = Self::calculate_curator_fee_and_payout(
								bounty_id,
								bounty.fee,
								bounty.value,
							);
							let bounty_account = Self::bounty_account_id(bounty_id);
							let statuses = [
								Self::process_payment_status(
									&bounty_account,
									&curator_stash.0,
									&bounty.asset_kind,
									final_fee,
									&mut curator_stash.1,
								),
								Self::process_payment_status(
									&bounty_account,
									&beneficiary.0,
									&bounty.asset_kind,
									payout,
									&mut beneficiary.1,
								),
							];

							// only if both payments failed we fail as well
							// if at least one payment succeeded we shouldn't revert
							// to advance the state machine
							let succeeded = statuses.iter().filter(|i| i.is_ok()).count();
							if succeeded > 0 {
								Ok(Pays::Yes.into())
							} else {
								// No payments succeeded, return error
								Err(Error::<T, I>::PayoutError.into())
							}
						},
						BountyStatus::RefundAttempted { ref mut payment_status } => {
							let treasury_account = Self::account_id();
							let bounty_account = Self::bounty_account_id(bounty_id);
							Self::process_payment_status(
								&bounty_account,
								&treasury_account,
								&bounty.asset_kind,
								bounty.value,
								payment_status,
							)
						},
						_ => Err(Error::<T, I>::UnexpectedStatus.into()),
					}
				},
			)
		}

		// TODO: check payment statuses for all possible bounty statuses and updates it. Similar to
		// `pallet_treasury` `check_status` call.
		#[pallet::call_index(11)]
		// TODO: change weight
		#[pallet::weight(<T as Config<I>>::WeightInfo::approve_bounty_with_curator())]
		pub fn check_payment_status(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
		) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;
			Bounties::<T, I>::try_mutate_exists(
				bounty_id,
				|maybe_bounty| -> DispatchResultWithPostInfo {
					let bounty = maybe_bounty.as_mut().ok_or(Error::<T, I>::InvalidIndex)?;
					let mut new_bounty_status = None;

					let result = match bounty.status {
						BountyStatus::Approved { ref mut payment_status } =>
						{
							let result = Self::check_payment_status_for_funding(
								&mut new_bounty_status,
								payment_status,
								None,
							);

							// Unreserve the deposit when payment succeeds
							if let PaymentState::Succeeded = payment_status {
								let err_amount = T::Currency::unreserve(&bounty.proposer, bounty.bond);
								debug_assert!(err_amount.is_zero());  // Ensure nothing remains reserved
								Self::deposit_event(Event::<T, I>::BountyBecameActive { index: bounty_id });
							}
						
							result
						},
						BountyStatus::ApprovedWithCurator {
							ref mut payment_status,
							ref curator,
						} => {
							let result = Self::check_payment_status_for_funding(
								&mut new_bounty_status,
								payment_status,
								Some(curator),
							);

							// Unreserve the deposit when payment succeeds
							if let PaymentState::Succeeded = payment_status {
								let err_amount = T::Currency::unreserve(&bounty.proposer, bounty.bond);
								debug_assert!(err_amount.is_zero());  // Ensure nothing remains reserved
								Self::deposit_event(Event::<T, I>::BountyBecameActive { index: bounty_id });
							}
						
							result
						},
						BountyStatus::PayoutAttempted {
							ref curator,
							ref mut curator_stash,
							ref mut beneficiary,
						} => {
							let (mut payments_progressed, mut payments_succeeded) = (0, 0);
							// advance both curator, and beneficiary payments
							let results = [
								Self::check_payment_status_for_payout(
									&mut payments_progressed,
									&mut payments_succeeded,
									curator_stash,
								),
								Self::check_payment_status_for_payout(
									&mut payments_progressed,
									&mut payments_succeeded,
									beneficiary,
								),
							];

							// best scenario, both payments have succeeded,
							// emit events and advance state machine to the end
							if payments_succeeded >= results.len() as i32 {
								// all payments succeeded, cleanup the bounty
								let (_final_fee, payout) = Self::calculate_curator_fee_and_payout(
									bounty_id,
									bounty.fee,
									bounty.value,
								);
								
								// Tiago: Should I remove the bounty since it was being removed in claim_bounty
								Bounties::<T, I>::remove(bounty_id);
								BountyDescriptions::<T, I>::remove(bounty_id);
								T::ChildBountyManager::bounty_removed(bounty_id);
								// Tiago: Unreserve here?
								// Unreserve the curator deposit when payment succeeds
								let err_amount = T::Currency::unreserve(&curator, bounty.curator_deposit);
								debug_assert!(err_amount.is_zero());  // Ensure nothing remains reserved
								Self::deposit_event(Event::<T, I>::BountyClaimed {
									index: bounty_id,
									asset_kind: bounty.asset_kind.clone(),
									asset_payout: payout,
									beneficiary: beneficiary.0.clone(),
								});
								*maybe_bounty = None;

								return Ok(Pays::No.into());
							} else if payments_progressed > 0 {
								// some payments have progressed in the state machine
								// return ok so these changes are saved to the state
								Ok(Pays::Yes.into())
							} else {
								// try returning payment status errors if there are any
								for result in results {
									result?;
								}

								// no progress was made in the state machine if we're here,
								return Err(Error::<T, I>::PayoutInconclusive.into())
							}
						},
						BountyStatus::RefundAttempted { ref mut payment_status } => {
							match payment_status {
								PaymentState::Attempted { id } => {
									match T::Paymaster::check_payment(*id) {
										PaymentStatus::Success => {

											// Tiago: Do you need this?
											// let res = T::Currency::transfer(
											// 	&bounty_account,
											// 	&Self::account_id(),
											// 	balance,
											// 	AllowDeath,
											// ); // should not fail
											// debug_assert!(res.is_ok());

											// refund succeeded, cleanup the bounty
											BountyDescriptions::<T, I>::remove(bounty_id);
											T::ChildBountyManager::bounty_removed(bounty_id);
											*maybe_bounty = None;
											Self::deposit_event(Event::<T, I>::BountyCanceled {
												index: bounty_id,
											});
											return Ok(Pays::No.into());
										},
										PaymentStatus::InProgress => {
											// nothing new to report
											return Err(Error::<T, I>::RefundInconclusive.into())
										},
										PaymentStatus::Unknown | PaymentStatus::Failure => {
											// assume payment has failed, allow user to retry
											*payment_status = PaymentState::Failed;
											return Ok(Pays::Yes.into());
										},
									}
								},
								PaymentState::Pending | PaymentState::Failed => {
									// nothing to do here, user should try calling
									// `process_payment` again to try payment again
									Err(Error::<T, I>::UnexpectedStatus.into())
								},
								PaymentState::Succeeded => {
									// should never be reached, if refund succeeded
									// it should have been removed from the bounties
									// already
									Err(Error::<T, I>::UnexpectedStatus.into())
								},
							}
						},
						_ => return Err(Error::<T, I>::UnexpectedStatus.into()),
					};

					// set bounty status only now to satisfy ownership rules
					if let Some(new_status) = new_bounty_status {
						bounty.status = new_status;
					}

					result
				},
			)
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
		fee: &BountyBalanceOf<T, I>,
		kind: T::AssetKind,
	) -> Result<DepositBalanceOf<T, I>, pallet_treasury::Error<T, I>> {
		let fee =
			<T as pallet_treasury::Config<I>>::BalanceConverter::from_asset_balance(*fee, kind)
				.map_err(|_| pallet_treasury::Error::<T, I>::FailedToConvertBalance)?;

		let mut deposit = T::CuratorDepositMultiplier::get() * fee;

		if let Some(max_deposit) = T::CuratorDepositMax::get() {
			deposit = deposit.min(max_deposit)
		}

		if let Some(min_deposit) = T::CuratorDepositMin::get() {
			deposit = deposit.max(min_deposit)
		}

		Ok(deposit)
	}

	/// The account ID of the treasury pot.
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

	fn create_bounty(
		proposer: T::AccountId,
		description: Vec<u8>,
		asset_kind: T::AssetKind,
		value: BountyBalanceOf<T, I>,
	) -> DispatchResult {
		let bounded_description: BoundedVec<_, _> =
			description.try_into().map_err(|_| Error::<T, I>::ReasonTooBig)?;
		let native_amount =
			<T as pallet_treasury::Config<I>>::BalanceConverter::from_asset_balance(
				value,
				asset_kind.clone(),
			)
			.map_err(|_| pallet_treasury::Error::<T, I>::FailedToConvertBalance)?;

		ensure!(native_amount >= T::BountyValueMinimum::get(), Error::<T, I>::InvalidValue);

		let index = BountyCount::<T, I>::get();

		// reserve deposit for new bounty
		let bond = T::BountyDepositBase::get() +
			T::DataDepositPerByte::get() * (bounded_description.len() as u32).into();
		T::Currency::reserve(&proposer, bond)
			.map_err(|_| Error::<T, I>::InsufficientProposersBalance)?;

		BountyCount::<T, I>::put(index + 1);

		let bounty = BountyOf::<T, I> {
			proposer,
			asset_kind,
			value,
			fee: 0u32.into(),
			curator_deposit: 0u32.into(),
			bond,
			status: BountyStatus::Proposed,
		};

		Bounties::<T, I>::insert(index, &bounty);
		BountyDescriptions::<T, I>::insert(index, bounded_description);

		Self::deposit_event(Event::<T, I>::BountyProposed { index });

		Ok(())
	}

	fn process_payment_status(
		source: &T::Beneficiary,
		beneficiary: &T::Beneficiary,
		asset_kind: &T::AssetKind,
		value: BountyBalanceOf<T, I>,
		payment_status: &mut PaymentState<PaymentIdOf<T, I>>,
	) -> DispatchResultWithPostInfo {
		match payment_status {
			PaymentState::Failed | PaymentState::Pending => {
				// attempt payment again from failed/pending status
				let id = T::Paymaster::pay(source, beneficiary, asset_kind.clone(), value)
					.map_err(|_| Error::<T, I>::FundingError)?;
				*payment_status = PaymentState::Attempted { id };
				return Ok(Pays::No.into());
			},
			PaymentState::Succeeded => {
				// should never happen for funding payments,
				// if payment succeeds in check_status it is advanced
				// to new bounty status skipping setting `Succeeded`
				// payment state
				return Err(Error::<T, I>::UnexpectedStatus.into());
			},
			PaymentState::Attempted { .. } => {
				// payment already in progress, its status needs to be checked
				return Err(Error::<T, I>::UnexpectedStatus.into());
			},
		}
	}

	fn check_payment_status_for_funding(
		bounty_status: &mut Option<BountyStatusOf<T, I>>,
		payment_status: &mut PaymentState<PaymentIdOf<T, I>>,
		approved_curator: Option<&T::AccountId>,
	) -> DispatchResultWithPostInfo {
		match payment_status {
			PaymentState::Pending => {
				// user should try processing payment again, not check its status
				return Err(Error::<T, I>::UnexpectedStatus.into());
			},
			PaymentState::Failed => {
				// payment is known to have failed, this function should revert
				// not to pass as valid transaction
				return Err(Error::<T, I>::UnexpectedStatus.into());
			},
			PaymentState::Succeeded => {
				// should never happen in funding payment, we should
				// have advanced to the next bounty state on success
				// from ::Attempted payment status
				return Err(Error::<T, I>::UnexpectedStatus.into());
			},
			PaymentState::Attempted { id } => {
				match T::Paymaster::check_payment(*id) {
					PaymentStatus::Success => {
						*payment_status = PaymentState::Succeeded;
						// success, advance bounty state machine to
						// either Funded or CuratorProposed depending
						// on whether approval was with curator
						match approved_curator {
							Some(curator) => {
								*bounty_status = Some(BountyStatus::CuratorProposed {
									curator: curator.clone(),
								});
							},
							None => {
								*bounty_status = Some(BountyStatus::Funded);
							},
						}
						return Ok(Pays::No.into());
					},
					PaymentStatus::InProgress => {
						// nothing new to report, revert so this is not spammed
						return Err(Error::<T, I>::FundingInconclusive.into());
					},
					PaymentStatus::Unknown | PaymentStatus::Failure => {
						// TODO: should we assume payment has failed on unknown? not
						// sure yet
						*payment_status = PaymentState::Failed;
						// user can retry from this tate
						return Ok(Pays::No.into());
					},
				}
			},
		}
	}

	/// advance state machine of payout, used for curator and beneficiary payments
	fn check_payment_status_for_payout(
		// counter for the state changes that we'd like to keep in the storage
		// say, first payment progresses and second fails, we want to save progress
		// of the first in the state machine even if second payment is not successful yet
		payments_progressed: &mut i32,
		// payments that have finished processing successfully
		payments_succeeded: &mut i32,
		beneficiary: &mut (T::Beneficiary, PaymentState<PaymentIdOf<T, I>>),
	) -> Result<(), Error<T, I>> {
		match beneficiary.1 {
			PaymentState::Pending => {
				// user should try processing payment again, not check its status
				return Err(Error::<T, I>::UnexpectedStatus.into());
			},
			PaymentState::Failed => {
				// payment is known to have failed, and should be retried
				// instead of checking its status
				return Err(Error::<T, I>::UnexpectedStatus.into());
			},
			// this could be retried multiple times, say, curator payment succeeded
			// and beneficiary payment failed, we should still allow beneficiary to
			// retry, while having curator succeeded
			PaymentState::Succeeded => {
				*payments_succeeded += 1;
			},
			PaymentState::Attempted { id } => {
				match T::Paymaster::check_payment(id) {
					PaymentStatus::Success => {
						// success, change payment state and increment successful payments counter
						beneficiary.1 = PaymentState::Succeeded;
						*payments_succeeded += 1;
						*payments_progressed += 1;
					},
					PaymentStatus::InProgress => {
						// nothing new to report, return function without error
						// so we could drive the next payment
					},
					PaymentStatus::Unknown | PaymentStatus::Failure => {
						*payments_progressed += 1;
						beneficiary.1 = PaymentState::Failed;
					},
				}
			},
		};

		Ok(())
	}

	fn calculate_curator_fee_and_payout(
		bounty_id: BountyIndex,
		fee: BountyBalanceOf<T, I>,
		value: BountyBalanceOf<T, I>,
	) -> (BountyBalanceOf<T, I>, BountyBalanceOf<T, I>) {
		let payout = value - fee;

		// Get total child bounties curator fees, and subtract it from the parent
		// curator fee (the fee in present referenced bounty, `self`).
		let children_fee = T::ChildBountyManager::children_curator_fees(bounty_id);
		debug_assert!(children_fee <= fee);
		let final_fee = fee.saturating_sub(children_fee);
		(final_fee, payout)
	}
}

// Default impl for when ChildBounties is not being used in the runtime.
impl<Balance: Zero> ChildBountyManager<Balance> for () {
	fn child_bounties_count(_bounty_id: BountyIndex) -> BountyIndex {
		Default::default()
	}

	fn children_curator_fees(_bounty_id: BountyIndex) -> Balance {
		Zero::zero()
	}

	fn bounty_removed(_bounty_id: BountyIndex) {}
}
