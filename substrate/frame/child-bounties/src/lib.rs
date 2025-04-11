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

//! # Child Bounties Pallet ( `pallet-child-bounties` )
//!
//! ## Child Bounty
//!
//! > NOTE: This pallet is tightly coupled with `pallet-treasury` and `pallet-bounties`.
//!
//! With child bounties, a large bounty proposal can be divided into smaller chunks,
//! for parallel execution, and for efficient governance and tracking of spent funds.
//! A child-bounty is a smaller piece of work, extracted from a parent bounty.
//! A curator is assigned after the child-bounty is created by the parent bounty curator,
//! to be delegated with the responsibility of assigning a payout address once the specified
//! set of tasks is completed.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! Child Bounty protocol:
//! - `add_child_bounty` - Add a child-bounty for a parent bounty to for dividing the work in
//!   smaller tasks.
//! - `propose_curator` - Assign an account to a child-bounty as candidate curator.
//! - `accept_curator` - Accept a child-bounty assignment from the parent bounty curator, setting a
//!   curator deposit.
//! - `award_child_bounty` - Close and pay out the specified amount for the completed work.
//! - `claim_child_bounty` - Claim a specific child-bounty amount from the payout address.
//! - `unassign_curator` - Unassign an accepted curator from a specific child-bounty.
//! - `close_child_bounty` - Cancel the child-bounty for a specific treasury amount and close the
//!   bounty.
//! - `process_payment` - Retry a failed payment for a specific child-bounty funding, curator and
//!   beneficiary payout or refund.
//! - `check_payment_status` - Check and update the current state of a specific child-bounty
//!   funding, payout or refund.

// Most of the business logic in this pallet has been
// originally contributed by "https://github.com/shamb0",
// as part of the PR - https://github.com/paritytech/substrate/pull/7965.
// The code has been moved here and then refactored in order to
// extract child bounties as a separate pallet.

#![cfg_attr(not(feature = "std"), no_std)]

mod benchmarking;
pub mod migrations;
mod mock;
mod tests;
pub mod weights;
pub use pallet::*;
pub use weights::WeightInfo;

extern crate alloc;
use alloc::vec::Vec;
use frame_support::{
	pallet_prelude::*,
	traits::{
		tokens::{ConversionFromAssetBalance, Pay, PaymentStatus},
		Currency,
		ExistenceRequirement::AllowDeath,
		Get, OnUnbalanced, ReservableCurrency,
	},
};
use frame_system::pallet_prelude::{
	ensure_signed, BlockNumberFor as SystemBlockNumberFor, OriginFor,
};
use pallet_bounties::PaymentState;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{AccountIdConversion, BadOrigin, BlockNumberProvider, Saturating, StaticLookup, Zero},
	DispatchResult, RuntimeDebug,
};

pub(crate) const LOG_TARGET: &'static str = "runtime::child_bounties";
// syntactic sugar for logging.
#[macro_export]
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: crate::LOG_TARGET,
			concat!("[{:?}] ✍️ ", $patter), <frame_system::Pallet<T>>::block_number() $(, $values)*
		)
	};
}

type BeneficiaryLookupOf<T, I = ()> = pallet_treasury::BeneficiaryLookupOf<T, I>;
type BalanceOf<T, I = ()> = pallet_treasury::BalanceOf<T, I>;
type BountiesError<T, I = ()> = pallet_bounties::Error<T, I>;
type BountyIndex = pallet_bounties::BountyIndex;
type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;
type BlockNumberFor<T, I = ()> =
	<<T as pallet_treasury::Config<I>>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;
type PaymentIdOf<T, I = ()> = <<T as pallet_bounties::Config<I>>::Paymaster as Pay>::Id;
type ChildBountyOf<T, I> = ChildBounty<
	<T as frame_system::Config>::AccountId,
	BalanceOf<T, I>,
	BlockNumberFor<T, I>,
	<T as pallet_treasury::Config<I>>::AssetKind,
	PaymentIdOf<T, I>,
	<T as pallet_treasury::Config<I>>::Beneficiary,
>;
/// The status of a child-bounty.
pub type ChildBountyStatus<AccountId, BlockNumber, PaymentId, Beneficiary> =
	pallet_bounties::BountyStatus<AccountId, BlockNumber, PaymentId, Beneficiary>;

/// A child-bounty proposal.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ChildBounty<AccountId, Balance, BlockNumber, AssetKind, PaymentId, Beneficiary> {
	/// The parent of this child-bounty.
	parent_bounty: BountyIndex,
	// TODO: new filed, migration required.
	/// The kind of asset this child-bounty is rewarded in.
	pub asset_kind: AssetKind,
	/// The (total) amount that should be paid if this child-bounty is rewarded.
	value: Balance,
	/// The child-bounty curator fee in the `asset_kind`. Included in value.
	fee: Balance,
	/// The deposit of child-bounty curator.
	///
	/// The asset class determined by the [`pallet_treasury::Config::Currency`].
	curator_deposit: Balance,
	/// The status of this child-bounty.
	status: ChildBountyStatus<AccountId, BlockNumber, PaymentId, Beneficiary>,
}

#[frame_support::pallet]
pub mod pallet {

	use super::*;

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::config]
	pub trait Config<I: 'static = ()>:
		frame_system::Config + pallet_bounties::Config<I> + pallet_treasury::Config<I>
	{
		/// Maximum number of child bounties that can be added to a parent bounty.
		#[pallet::constant]
		type MaxActiveChildBountyCount: Get<u32>;

		/// Minimum value for a child-bounty.
		#[pallet::constant]
		type ChildBountyValueMinimum: Get<BalanceOf<Self, I>>;

		/// The overarching event type.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: benchmarking::ArgumentsFactory<Self::AssetKind, Self::Beneficiary>;
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The parent bounty is not in active state.
		ParentBountyNotActive,
		/// The bounty balance is not enough to add new child-bounty.
		InsufficientBountyBalance,
		/// Number of child bounties exceeds limit `MaxActiveChildBountyCount`.
		TooManyChildBounties,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// A child-bounty is added.
		Added { index: BountyIndex, child_index: BountyIndex },
		/// A child-bounty is funded and became active.
		BecameActive { index: BountyIndex, child_index: BountyIndex },
		/// A child-bounty is awarded to a beneficiary.
		Awarded {
			index: BountyIndex,
			child_index: BountyIndex,
			beneficiary: <T as pallet_treasury::Config<I>>::Beneficiary,
		},
		/// A child-bounty is claimed by beneficiary.
		Claimed {
			beneficiary: <T as pallet_treasury::Config<I>>::Beneficiary,
			curator_stash: <T as pallet_treasury::Config<I>>::Beneficiary,
		},
		/// Payout payments to the beneficiary and curator stash have been successfully concluded.
		PayoutProcessed {
			index: BountyIndex,
			child_index: BountyIndex,
			asset_kind: T::AssetKind,
			value: BalanceOf<T, I>,
			beneficiary: <T as pallet_treasury::Config<I>>::Beneficiary,
		},
		/// A child-bounty is cancelled.
		Canceled { index: BountyIndex, child_index: BountyIndex },
		/// Refund payment has concluded successfully.
		RefundProcessed { index: BountyIndex, child_index: BountyIndex },
		/// A payment failed and can be retried.
		PaymentFailed {
			index: BountyIndex,
			child_index: BountyIndex,
			payment_id: PaymentIdOf<T, I>,
		},
		/// A payment happened and can be checked.
		Paid { index: BountyIndex, child_index: BountyIndex, payment_id: PaymentIdOf<T, I> },
	}

	/// DEPRECATED: Replaced with `ParentTotalChildBounties` storage item keeping dedicated counts
	/// for each parent bounty. Number of total child bounties. Will be removed in May 2025.
	#[pallet::storage]
	pub type ChildBountyCount<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BountyIndex, ValueQuery>;

	/// Number of active child bounties per parent bounty.
	/// Map of parent bounty index to number of child bounties.
	#[pallet::storage]
	pub type ParentChildBounties<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BountyIndex, u32, ValueQuery>;

	/// Number of total child bounties per parent bounty, including completed bounties.
	#[pallet::storage]
	pub type ParentTotalChildBounties<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BountyIndex, u32, ValueQuery>;

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

	/// The description of each child-bounty. Indexed by `(parent_id, child_id)`.
	///
	/// This item replaces the `ChildBountyDescriptions` storage item from the V0 storage version.
	#[pallet::storage]
	pub type ChildBountyDescriptionsV1<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Twox64Concat,
		BountyIndex,
		Twox64Concat,
		BountyIndex,
		BoundedVec<u8, T::MaximumReasonLength>,
	>;

	/// The mapping of the child-bounty ids from storage version `V0` to the new `V1` version.
	///
	/// The `V0` ids based on total child-bounty count [`ChildBountyCount`]`. The `V1` version ids
	/// based on the child-bounty count per parent bounty [`ParentTotalChildBounties`].
	/// The item intended solely for client convenience and not used in the pallet's core logic.
	#[pallet::storage]
	pub type V0ToV1ChildBountyIds<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BountyIndex, (BountyIndex, BountyIndex)>;

	/// The cumulative child-bounty value for each parent bounty.
	#[pallet::storage]
	pub type ChildrenValue<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BountyIndex, BalanceOf<T, I>, ValueQuery>;

	/// The cumulative child-bounty curator fee for each parent bounty.
	#[pallet::storage]
	pub type ChildrenCuratorFees<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BountyIndex, BalanceOf<T, I>, ValueQuery>;

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Add a new child-bounty.
		///
		/// ## Dispatch Origin
		/// The dispatch origin for this call must be the curator of parent
		/// bounty and the parent bounty must be in "active" state.
		///
		/// ## Details
		/// - A child-bounty is successfully created and funded if the parent bounty has sufficient
		///   funds of `asset_kind`; otherwise, the call fails.
		/// - The maximum number of active child bounties is limited by the runtime configuration
		///   [`Config::MaxActiveChildBountyCount`].
		/// - If successful, the funding payment is initiated and the status of the child-bounty is
		///   updated to `Added`.
		///
		/// ### Parameters
		/// - `parent_bounty_id`: Index of parent bounty for which child-bounty is being added.
		/// - `value`: Amount allocated for executing the proposal.
		/// - `description`: Text description for the child-bounty.
		///
		/// ## Events
		/// Emits [`Event::Paid`] and [`Event::ChildBountyAdded`] if successful.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::add_child_bounty(description.len() as u32))]
		pub fn add_child_bounty(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			#[pallet::compact] value: BalanceOf<T, I>,
			description: Vec<u8>,
		) -> DispatchResult {
			let signer = ensure_signed(origin)?;

			// Verify the arguments.
			let bounded_description =
				description.try_into().map_err(|_| BountiesError::<T, I>::ReasonTooBig)?;
			let parent_bounty = Self::parent_bounty(parent_bounty_id)?;
			let native_amount =
				<T as pallet_treasury::Config<I>>::BalanceConverter::from_asset_balance(
					value,
					parent_bounty.asset_kind.clone(),
				)
				.map_err(|_| pallet_treasury::Error::<T, I>::FailedToConvertBalance)?;

			ensure!(
				native_amount >= T::ChildBountyValueMinimum::get(),
				BountiesError::<T, I>::InvalidValue
			);
			ensure!(
				ParentChildBounties::<T, I>::get(parent_bounty_id) <
					T::MaxActiveChildBountyCount::get() as u32,
				Error::<T, I>::TooManyChildBounties,
			);

			let (curator, _) = Self::ensure_bounty_active(parent_bounty_id)?;
			ensure!(signer == curator, BountiesError::<T, I>::RequireCurator);

			// Read parent bounty account info.
			let children_value = ChildrenValue::<T, I>::get(parent_bounty_id);
			let remaining_parent_value = parent_bounty.value.saturating_sub(children_value);
			ensure!(remaining_parent_value >= value, Error::<T, I>::InsufficientBountyBalance);

			// Get child-bounty ID.
			let child_bounty_id = ParentTotalChildBounties::<T, I>::get(parent_bounty_id);

			// Initiate payment
			let payment_status = Self::do_process_funding_payment(
				parent_bounty_id,
				child_bounty_id,
				parent_bounty.asset_kind.clone(),
				value,
				None,
			)?;

			// Add child-bounty value to the cumulative sum. To be
			// subtracted from the parent bounty payout when claiming
			// bounty.
			ChildrenValue::<T, I>::mutate(parent_bounty_id, |children_value| {
				*children_value = children_value.saturating_add(value)
			});

			// Increment the child-bounty count.
			ParentTotalChildBounties::<T, I>::insert(
				parent_bounty_id,
				child_bounty_id.saturating_add(1),
			);
			ParentChildBounties::<T, I>::mutate(parent_bounty_id, |count| count.saturating_inc());

			// Create child-bounty instance.
			Self::create_child_bounty(
				parent_bounty_id,
				child_bounty_id,
				parent_bounty.asset_kind,
				value,
				bounded_description,
				payment_status,
			);
			Ok(())
		}

		/// Propose curator for funded child-bounty.
		///
		/// ## Dispatch Origin
		/// The dispatch origin for this call must be the curator of the parent bounty.
		///
		/// ## Details
		/// - The parent bounty must be in the `Active` state for this call to proceed.
		/// - The child-bounty must be in the `Added` state, for processing the call.
		/// - If successful, the state of the child-bounty transitions to `CuratorProposed`.
		///
		/// ### Parameters
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child-bounty.
		/// - `curator`: Address of child-bounty curator.
		/// - `fee`: payment fee to child-bounty curator for execution.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::propose_curator())]
		pub fn propose_curator(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			#[pallet::compact] child_bounty_id: BountyIndex,
			curator: AccountIdLookupOf<T>,
			#[pallet::compact] fee: BalanceOf<T, I>,
		) -> DispatchResult {
			let signer = ensure_signed(origin)?;
			let child_bounty_curator = T::Lookup::lookup(curator)?;

			let (curator, _) = Self::ensure_bounty_active(parent_bounty_id)?;
			ensure!(signer == curator, BountiesError::<T, I>::RequireCurator);

			// Mutate the child-bounty instance.
			ChildBounties::<T, I>::try_mutate_exists(
				parent_bounty_id,
				child_bounty_id,
				|maybe_child_bounty| -> DispatchResult {
					let child_bounty =
						maybe_child_bounty.as_mut().ok_or(BountiesError::<T, I>::InvalidIndex)?;

					// Ensure child-bounty is in expected state.
					ensure!(
						child_bounty.status == ChildBountyStatus::Funded,
						BountiesError::<T, I>::UnexpectedStatus,
					);

					// Ensure child-bounty curator fee is less than child-bounty value.
					ensure!(fee < child_bounty.value, BountiesError::<T, I>::InvalidFee);

					// Add child-bounty curator fee to the cumulative sum. To be
					// subtracted from the parent bounty curator when claiming
					// bounty.
					ChildrenCuratorFees::<T, I>::mutate(parent_bounty_id, |value| {
						*value = value.saturating_add(fee)
					});

					// Update the child-bounty curator fee.
					child_bounty.fee = fee;

					// Update the child-bounty state.
					child_bounty.status =
						ChildBountyStatus::CuratorProposed { curator: child_bounty_curator };

					Ok(())
				},
			)
		}

		/// Accept the curator role for the child-bounty.
		///
		/// ## Dispatch Origin
		/// The dispatch origin for this call must be the curator of the child-bounty.
		///
		/// ## Details
		/// - A deposit will be reserved from the curator and refunded upon successful payout or
		///   cancellation.
		/// - The curator's fee is deducted from the parent bounty's curator fee.
		/// - The parent bounty must be in the `Active` state for this child-bounty call to proceed.
		/// - The child-bounty must be in the `CuratorProposed` state.
		/// - If successful, the child-bounty transitions to the `Active` state.
		///
		/// ### Parameters
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child-bounty.
		/// - `stash`: Account to receive the curator fee.
		///
		/// ## Events
		/// Emits [`Event::ChildBountyCuratorAccepted`] if successful.
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::accept_curator())]
		pub fn accept_curator(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			#[pallet::compact] child_bounty_id: BountyIndex,
			stash: BeneficiaryLookupOf<T, I>,
		) -> DispatchResult {
			let signer = ensure_signed(origin)?;
			let stash = T::BeneficiaryLookup::lookup(stash)?;

			let (parent_curator, update_due) = Self::ensure_bounty_active(parent_bounty_id)?;

			// Mutate child-bounty.
			ChildBounties::<T, I>::try_mutate_exists(
				parent_bounty_id,
				child_bounty_id,
				|maybe_child_bounty| -> DispatchResult {
					let child_bounty =
						maybe_child_bounty.as_mut().ok_or(BountiesError::<T, I>::InvalidIndex)?;

					// Ensure child-bounty is in expected state.
					if let ChildBountyStatus::CuratorProposed { ref curator } = child_bounty.status
					{
						ensure!(signer == *curator, BountiesError::<T, I>::RequireCurator);

						let parent_bounty = Self::parent_bounty(parent_bounty_id)?;
						// Reserve child-bounty curator deposit.
						let deposit = Self::calculate_curator_deposit(
							&parent_curator,
							curator,
							&child_bounty.fee,
							parent_bounty.asset_kind,
						)?;

						T::Currency::reserve(curator, deposit)?;
						child_bounty.curator_deposit = deposit;

						child_bounty.status = ChildBountyStatus::Active {
							curator: curator.clone(),
							curator_stash: stash,
							update_due,
						};
						Ok(())
					} else {
						Err(BountiesError::<T, I>::UnexpectedStatus.into())
					}
				},
			)
		}

		/// Unassign curator from a child-bounty.
		///
		/// ## Dispatch Origin
		/// The dispatch origin for this call can be one of the following:
		/// - `RejectOrigin`
		/// - The curator of the parent bounty
		/// - Any signed origin
		///
		/// ## Details
		/// - If the origin is neither `RejectOrigin` nor the child-bounty curator, the parent
		///   bounty must be in the `Active` state for this call to be executed.
		/// - The child-bounty curator and `RejectOrigin` can execute this call regardless of the
		///   parent bounty's state.
		/// - If the call is made by `RejectOrigin` or the parent bounty curator, the child-bounty
		///   curator is assumed to be malicious or inactive, and their deposit is slashed.
		/// - If the call is made by the child-bounty curator, they are considered to be voluntarily
		///   stepping down. Their deposit is unreserved, and they exit without penalty. (This
		///   behavior may be revised if abused.)
		/// - If the call is made by any signed account and the child-bounty curator is deemed
		///   inactive, the curator is removed, and their deposit is slashed. The inactivity status
		///   is estimated based on the parent bounty's expiry update due.
		/// - This mechanism allows anyone in the community to report an inactive child-bounty
		///   curator, prompting the selection of a new curator.
		/// - If successful, the child-bounty transitions back to the `Added` state.
		///
		/// ### Parameters
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child-bounty.
		///
		/// ## Events
		/// Emits [`Event::ChildBountyCuratorUnassigned`] if successful.
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::unassign_curator())]
		pub fn unassign_curator(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			#[pallet::compact] child_bounty_id: BountyIndex,
		) -> DispatchResult {
			let maybe_sender = ensure_signed(origin.clone())
				.map(Some)
				.or_else(|_| T::RejectOrigin::ensure_origin(origin).map(|_| None))?;

			ChildBounties::<T, I>::try_mutate_exists(
				parent_bounty_id,
				child_bounty_id,
				|maybe_child_bounty| -> DispatchResult {
					let child_bounty =
						maybe_child_bounty.as_mut().ok_or(BountiesError::<T, I>::InvalidIndex)?;

					let slash_curator =
						|curator: &T::AccountId, curator_deposit: &mut BalanceOf<T, I>| {
							let imbalance =
								T::Currency::slash_reserved(curator, *curator_deposit).0;
							T::OnSlash::on_unbalanced(imbalance);
							*curator_deposit = Zero::zero();
						};

					match child_bounty.status {
						ChildBountyStatus::Proposed |
						ChildBountyStatus::Approved { .. } |
						ChildBountyStatus::Funded |
						ChildBountyStatus::PayoutAttempted { .. } |
						ChildBountyStatus::RefundAttempted { .. } => {
							// No curator to unassign at this point.
							return Err(BountiesError::<T, I>::UnexpectedStatus.into());
						},
						ChildBountyStatus::ApprovedWithCurator {
							ref curator,
							ref payment_status,
						} => {
							// Bounty not yet funded, but bounty was approved with curator.
							// `RejectOrigin` or curator himself can unassign from this bounty.
							ensure!(
								maybe_sender.map_or(true, |sender| sender == *curator),
								BadOrigin
							);
							// This state can only be while the bounty is not yet funded so we
							// return bounty to the `Approved` state without curator
							child_bounty.status = ChildBountyStatus::Approved {
								payment_status: payment_status.clone(),
							};
							return Ok(());
						},
						ChildBountyStatus::CuratorProposed { ref curator } => {
							// A child-bounty curator has been proposed, but not accepted yet.
							// Either `RejectOrigin`, parent bounty curator or the proposed
							// child-bounty curator can unassign the child-bounty curator.
							ensure!(
								maybe_sender.map_or(true, |sender| {
									sender == *curator ||
										Self::ensure_bounty_active(parent_bounty_id)
											.map_or(false, |(parent_curator, _)| {
												sender == parent_curator
											})
								}),
								BadOrigin
							);
							// Continue to change bounty status below.
						},
						ChildBountyStatus::Active { ref curator, .. } => {
							// The child-bounty is active.
							match maybe_sender {
								// If the `RejectOrigin` is calling this function, slash the curator
								// deposit.
								None => {
									slash_curator(curator, &mut child_bounty.curator_deposit);
									// Continue to change child-bounty status below.
								},
								Some(sender) if sender == *curator => {
									// This is the child-bounty curator, willingly giving up their
									// role. Give back their deposit.
									T::Currency::unreserve(curator, child_bounty.curator_deposit);
									// Reset curator deposit.
									child_bounty.curator_deposit = Zero::zero();
									// Continue to change bounty status below.
								},
								Some(sender) => {
									let (parent_curator, update_due) =
										Self::ensure_bounty_active(parent_bounty_id)?;
									if sender == parent_curator ||
										update_due < Self::treasury_block_number()
									{
										// Slash the child-bounty curator if
										// + the call is made by the parent bounty curator.
										// + or the curator is inactive.
										slash_curator(curator, &mut child_bounty.curator_deposit);
									// Continue to change bounty status below.
									} else {
										// Curator has more time to give an update.
										return Err(BountiesError::<T, I>::Premature.into());
									}
								},
							}
						},
						ChildBountyStatus::PendingPayout { ref curator, .. } => {
							let (parent_curator, _) = Self::ensure_bounty_active(parent_bounty_id)?;
							ensure!(
								maybe_sender.map_or(true, |sender| parent_curator == sender),
								BadOrigin,
							);
							slash_curator(curator, &mut child_bounty.curator_deposit);
							// Continue to change child-bounty status below.
						},
					};
					// Move the child-bounty state to Funded.
					child_bounty.status = ChildBountyStatus::Funded;
					Ok(())
				},
			)
		}

		/// Award child-bounty to a beneficiary.
		///
		/// ## Dispatch Origin
		/// The dispatch origin for this call must be the parent curator or
		/// curator of this child-bounty.
		///
		/// ## Details
		/// - The beneficiary will be able to claim the funds after a delay.
		/// - The parent bounty must be in the `Active` state for this call to be executed.
		/// - The child-bounty must be in the `Active` state.
		/// - If successful, the child-bounty transitions to the `PendingPayout` state.
		///
		/// ### Parameters
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child-bounty.
		/// - `beneficiary`: Account to receive the bounty payout.
		///
		/// ## Events
		/// Emits [`Event::ChildBountyAwarded`] if successful.
		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::award_child_bounty())]
		pub fn award_child_bounty(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			#[pallet::compact] child_bounty_id: BountyIndex,
			beneficiary: BeneficiaryLookupOf<T, I>,
		) -> DispatchResult {
			let signer = ensure_signed(origin)?;
			let beneficiary = T::BeneficiaryLookup::lookup(beneficiary)?;

			// Ensure parent bounty exists, and is active.
			let (parent_curator, _) = Self::ensure_bounty_active(parent_bounty_id)?;

			ChildBounties::<T, I>::try_mutate_exists(
				parent_bounty_id,
				child_bounty_id,
				|maybe_child_bounty| -> DispatchResult {
					let child_bounty =
						maybe_child_bounty.as_mut().ok_or(BountiesError::<T, I>::InvalidIndex)?;

					// Ensure child-bounty is in active state.
					if let ChildBountyStatus::Active { ref curator, curator_stash, .. } =
						&child_bounty.status
					{
						ensure!(
							signer == *curator || signer == parent_curator,
							BountiesError::<T, I>::RequireCurator,
						);
						// Move the child-bounty state to pending payout.
						child_bounty.status = ChildBountyStatus::PendingPayout {
							curator: signer,
							beneficiary: beneficiary.clone(),
							unlock_at: Self::treasury_block_number() +
								T::BountyDepositPayoutDelay::get(),
							curator_stash: curator_stash.clone(),
						};
						Ok(())
					} else {
						Err(BountiesError::<T, I>::UnexpectedStatus.into())
					}
				},
			)?;

			// Trigger the event Awarded.
			Self::deposit_event(Event::<T, I>::Awarded {
				index: parent_bounty_id,
				child_index: child_bounty_id,
				beneficiary,
			});

			Ok(())
		}

		/// Claim the payout from an awarded child-bounty after payout delay.
		///
		/// ## Dispatch Origin
		/// The dispatch origin for this call may be any signed origin.
		///
		/// ## Details
		/// - Call works independent of the parent bounty's state; the parent bounty does not need
		///   to be `Active`.
		/// - The beneficiary is paid the agreed bounty amount.
		/// - The curator's fee is paid, and the curator's deposit is unreserved.
		/// - The child-bounty must be in the `PendingPayout` state.
		/// - If successful, the child-bounty transitions to the `PayoutAttempted` state.
		/// - `check_payment_status` must be called to advance bounty status.
		///
		/// ### Parameters
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child-bounty.
		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::claim_child_bounty())]
		pub fn claim_child_bounty(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			#[pallet::compact] child_bounty_id: BountyIndex,
		) -> DispatchResult {
			let _ = ensure_signed(origin)?;

			// Ensure child-bounty is in expected state.
			ChildBounties::<T, I>::try_mutate_exists(
				parent_bounty_id,
				child_bounty_id,
				|maybe_child_bounty| -> DispatchResult {
					let child_bounty =
						maybe_child_bounty.as_mut().ok_or(BountiesError::<T, I>::InvalidIndex)?;

					if let ChildBountyStatus::PendingPayout {
						ref curator,
						beneficiary,
						ref unlock_at,
						curator_stash,
					} = &child_bounty.status
					{
						// Ensure block number is elapsed for processing the
						// claim.
						ensure!(
							Self::treasury_block_number() >= *unlock_at,
							BountiesError::<T, I>::Premature,
						);

						// Initiate payout payments.
						let (curator_payment_status, beneficiary_payment_status) =
							Self::do_process_payout_payment(
								parent_bounty_id,
								child_bounty_id,
								&child_bounty,
								(curator_stash.clone(), None),
								(beneficiary.clone(), None),
							)?;

						child_bounty.status = ChildBountyStatus::PayoutAttempted {
							curator: curator.clone(),
							curator_stash: (curator_stash.clone(), curator_payment_status),
							beneficiary: (beneficiary.clone(), beneficiary_payment_status),
						};

						Ok(())
					} else {
						Err(BountiesError::<T, I>::UnexpectedStatus.into())
					}
				},
			)
		}

		/// Cancel a proposed or active child-bounty. Child-bounty account funds
		/// are transferred to parent bounty account. The child-bounty curator
		/// deposit may be unreserved if possible.
		///
		/// ## Dispatch Origin
		/// The dispatch origin for this call must be either parent curator or
		/// `T::RejectOrigin`.
		///
		/// ## Details
		/// - If the child-bounty is in the `Proposed` state, it is simply removed from storage.
		/// - If the child-bounty is in the `Funded` or `CuratorProposed` state, a refund payment is
		///   initiated to return funds to the parent bounty account.
		/// - If the child-bounty is in the `Active` state, the curator’s deposit is unreserved, and
		///   a refund payment is initiated.
		/// - If the child-bounty is in the `PendingPayout`, `PayoutAttempted`, or `RefundAttempted`
		///   state, the call fails.
		/// - If the origin is not `T::RejectOrigin`, the parent bounty must be in the `Active`
		///   state for this call to succeed.
		/// - If the origin is `T::RejectOrigin`, execution is forced regardless of the parent
		///   bounty state.
		/// - The child-bounty status transitions to `RefundAttempted`, and `check_payment_status`
		///   must be called to finalize the refund.
		///
		/// ### Parameters
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child-bounty.
		///
		/// ## Events
		/// If the child-bounty was in the `Proposed` state, emits [`Event::ChildBountyCanceled`]
		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::close_child_bounty_added()
			.max(<T as Config<I>>::WeightInfo::close_child_bounty_active()))]
		pub fn close_child_bounty(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			#[pallet::compact] child_bounty_id: BountyIndex,
		) -> DispatchResult {
			let maybe_sender = ensure_signed(origin.clone())
				.map(Some)
				.or_else(|_| T::RejectOrigin::ensure_origin(origin).map(|_| None))?;

			// Ensure parent bounty exist, get parent curator.
			let (parent_curator, _) = Self::ensure_bounty_active(parent_bounty_id)?;

			ensure!(maybe_sender.map_or(true, |sender| parent_curator == sender), BadOrigin);

			Self::impl_close_child_bounty(parent_bounty_id, child_bounty_id)?;
			Ok(())
		}

		/// Retry a payment for funding, payout or closing a child-bounty.
		///
		/// ## Dispatch Origin
		/// Must be signed.
		///
		/// ## Details
		/// - If the child-bounty is in the `Approved` state, it retries the funding payment.
		/// - If the child-bounty is in the `PayoutAttempted` state, it retries the curator and
		///   beneficiary payouts.
		/// - If the bounty is in the `RefundAttempted` state, it retries the refund payment to
		///   return funds to the parent bounty.
		/// - `check_payment_status` must be called to advance bounty status.
		///
		/// ### Parameters
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child-bounty.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(7)]
		// TODO: change weight
		#[pallet::weight(<T as Config<I>>::WeightInfo::accept_curator())]
		pub fn process_payment(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			#[pallet::compact] child_bounty_id: BountyIndex,
		) -> DispatchResultWithPostInfo {
			use pallet_bounties::BountyStatus::*;

			ensure_signed(origin)?;
			let mut child_bounty = ChildBounties::<T, I>::get(parent_bounty_id, child_bounty_id)
				.ok_or(BountiesError::<T, I>::InvalidIndex)?;
			let parent_bounty = Self::parent_bounty(parent_bounty_id)?;

			let (new_status, weight) = match child_bounty.status {
				Approved { ref payment_status } => {
					let new_payment_status = Self::do_process_funding_payment(
						parent_bounty_id,
						child_bounty_id,
						parent_bounty.asset_kind.clone(),
						child_bounty.value,
						Some(payment_status.clone()),
					)?;
					// TODO: change weight
					(
						Approved { payment_status: new_payment_status },
						<T as Config<I>>::WeightInfo::accept_curator(),
					)
				},
				ApprovedWithCurator { ref payment_status, ref curator } => {
					let new_payment_status = Self::do_process_funding_payment(
						parent_bounty_id,
						child_bounty_id,
						parent_bounty.asset_kind.clone(),
						child_bounty.value,
						Some(payment_status.clone()),
					)?;
					// TODO: change weight
					(
						ApprovedWithCurator {
							curator: curator.clone(),
							payment_status: new_payment_status,
						},
						<T as Config<I>>::WeightInfo::accept_curator(),
					)
				},
				RefundAttempted { ref payment_status, ref curator } => {
					let new_payment_status = Self::do_process_refund_payment(
						parent_bounty_id,
						child_bounty_id,
						parent_bounty.asset_kind.clone(),
						child_bounty.value,
						Some(payment_status.clone()),
					)?;
					// TODO: change weight
					(
						RefundAttempted {
							payment_status: new_payment_status,
							curator: curator.clone(),
						},
						<T as Config<I>>::WeightInfo::accept_curator(),
					)
				},
				PayoutAttempted { ref curator, ref curator_stash, ref beneficiary } => {
					let (new_curator_payment_status, new_beneficiary_payment_status) =
						Self::do_process_payout_payment(
							parent_bounty_id,
							child_bounty_id,
							&child_bounty,
							(curator_stash.0.clone(), Some(curator_stash.1.clone())),
							(beneficiary.0.clone(), Some(beneficiary.1.clone())),
						)?;
					// TODO: change weight
					(
						PayoutAttempted {
							curator: curator.clone(),
							curator_stash: (curator_stash.0.clone(), new_curator_payment_status),
							beneficiary: (beneficiary.0.clone(), new_beneficiary_payment_status),
						},
						<T as Config<I>>::WeightInfo::accept_curator(),
					)
				},
				_ => return Err(BountiesError::<T, I>::UnexpectedStatus.into()),
			};

			child_bounty.status = new_status;
			ChildBounties::<T, I>::insert(parent_bounty_id, child_bounty_id, child_bounty);

			Ok(Some(weight).into())
		}

		/// Check and update the payment status of a child-bounty.
		///
		/// ## Dispatch Origin
		/// Must be signed.
		///
		/// ## Details
		/// - If the child-bounty is in the `Approved` state, it checks if the funding payment has
		///   succeeded. If successful, the child-bounty status advanced to `Funded.
		/// - If the bounty is in the `PayoutAttempted` state, it checks the status of curator and
		///   beneficiary payouts. If both payments succeed, the bounty is removed, and the
		///   curator's deposit is unreserved. If any payment failed, the bounty status is updated.
		/// - If the bounty is in the `RefundAttempted` state, it checks if the refund was
		///   completed. If successful, the bounty is removed.
		///
		/// ### Parameters
		/// - `bounty_id`: The bounty index.
		///
		/// ## Events
		/// - Emits `BecameActive` when the bounty transitions to `Active`.
		/// - Emits `PayoutProcessed` when the payouts payments conclude successfully.
		/// - Emits `RefundProcessed` if the refund payment concludes successfully.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(8)]
		// TODO: change weight
		#[pallet::weight(<T as Config<I>>::WeightInfo::accept_curator())]
		pub fn check_payment_status(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			#[pallet::compact] child_bounty_id: BountyIndex,
		) -> DispatchResultWithPostInfo {
			use pallet_bounties::BountyStatus::*;

			ensure_signed(origin)?;
			let mut child_bounty = ChildBounties::<T, I>::get(parent_bounty_id, child_bounty_id)
				.ok_or(BountiesError::<T, I>::InvalidIndex)?;

			let (new_status, weight) = match child_bounty.status {
				Approved { ref payment_status } => {
					let new_payment_status = Self::do_check_funding_payment_status(
						parent_bounty_id,
						child_bounty_id,
						payment_status.clone(),
					)?;
					// TODO: change weight
					match new_payment_status {
						PaymentState::Succeeded => (
							ChildBountyStatus::Funded,
							<T as Config<I>>::WeightInfo::accept_curator(),
						),
						_ => (
							ChildBountyStatus::Approved { payment_status: new_payment_status },
							<T as Config<I>>::WeightInfo::accept_curator(),
						),
					}
				},
				RefundAttempted { ref payment_status, ref curator } => {
					let new_payment_status = Self::do_check_refund_payment_status(
						parent_bounty_id,
						child_bounty_id,
						&child_bounty,
						payment_status.clone(),
						curator.clone(),
					)?;
					// TODO: change weight
					match new_payment_status {
						PaymentState::Succeeded => return Ok(Pays::No.into()),
						_ => (
							ChildBountyStatus::RefundAttempted {
								payment_status: new_payment_status,
								curator: curator.clone(),
							},
							<T as Config<I>>::WeightInfo::accept_curator(),
						),
					}
				},
				PayoutAttempted { ref curator, ref curator_stash, ref beneficiary } => {
					let (new_curator_stash_payment_status, new_beneficiary_payment_status) =
						Self::do_check_payout_payment_status(
							parent_bounty_id,
							child_bounty_id,
							&child_bounty,
							curator.clone(),
							curator_stash.clone(),
							beneficiary.clone(),
						)?;
					// TODO: change weight
					match (
						new_curator_stash_payment_status.clone(),
						new_beneficiary_payment_status.clone(),
					) {
						(PaymentState::Succeeded, PaymentState::Succeeded) =>
							return Ok(Pays::No.into()),
						_ => (
							ChildBountyStatus::PayoutAttempted {
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
							<T as Config<I>>::WeightInfo::accept_curator(),
						),
					}
				},
				_ => return Err(BountiesError::<T, I>::UnexpectedStatus.into()),
			};

			child_bounty.status = new_status;
			ChildBounties::<T, I>::insert(parent_bounty_id, child_bounty_id, child_bounty);

			return Ok(Some(weight).into());
		}
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<SystemBlockNumberFor<T>> for Pallet<T, I> {
		fn integrity_test() {
			let parent_bounty_id: BountyIndex = 1;
			let child_bounty_id: BountyIndex = 2;
			let _: T::AccountId = T::PalletId::get()
				.try_into_sub_account(("cb", parent_bounty_id, child_bounty_id))
				.expect(
					"The `AccountId` type must be large enough to fit the child bounty account ID.",
				);
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Get the block number used in the treasury pallet.
	///
	/// It may be configured fto use the relay chain block number on a parachain.
	pub fn treasury_block_number() -> BlockNumberFor<T, I> {
		<T as pallet_treasury::Config<I>>::BlockNumberProvider::current_block_number()
	}

	// This function will calculate the deposit of a curator.
	fn calculate_curator_deposit(
		parent_curator: &T::AccountId,
		child_curator: &T::AccountId,
		bounty_fee: &BalanceOf<T, I>,
		asset_kind: T::AssetKind,
	) -> Result<BalanceOf<T, I>, pallet_bounties::Error<T, I>> {
		if parent_curator == child_curator {
			return Ok(Zero::zero());
		}

		// We just use the same logic from the parent bounties pallet.
		pallet_bounties::Pallet::<T, I>::calculate_curator_deposit(bounty_fee, asset_kind)
	}

	/// The account ID of a child-bounty account.
	pub fn child_bounty_account_id(
		parent_bounty_id: BountyIndex,
		child_bounty_id: BountyIndex,
	) -> T::Beneficiary {
		// This function is taken from the parent (bounties) pallet, but the
		// prefix is changed to have different AccountId when the index of
		// parent and child is same.
		T::PalletId::get().into_sub_account_truncating(("cb", parent_bounty_id, child_bounty_id))
	}

	fn create_child_bounty(
		parent_bounty_id: BountyIndex,
		child_bounty_id: BountyIndex,
		asset_kind: T::AssetKind,
		child_bounty_value: BalanceOf<T, I>,
		description: BoundedVec<u8, T::MaximumReasonLength>,
		payment_status: PaymentState<PaymentIdOf<T, I>>,
	) {
		let child_bounty = ChildBounty {
			parent_bounty: parent_bounty_id,
			asset_kind,
			value: child_bounty_value,
			fee: 0u32.into(),
			curator_deposit: 0u32.into(),
			status: ChildBountyStatus::Approved { payment_status },
		};
		ChildBounties::<T, I>::insert(parent_bounty_id, child_bounty_id, &child_bounty);
		ChildBountyDescriptionsV1::<T, I>::insert(parent_bounty_id, child_bounty_id, description);
		Self::deposit_event(Event::Added { index: parent_bounty_id, child_index: child_bounty_id });
	}

	fn ensure_bounty_active(
		bounty_id: BountyIndex,
	) -> Result<(T::AccountId, BlockNumberFor<T, I>), DispatchError> {
		let parent_bounty = pallet_bounties::Bounties::<T, I>::get(bounty_id)
			.ok_or(BountiesError::<T, I>::InvalidIndex)?;
		if let ChildBountyStatus::Active { curator, update_due, .. } = parent_bounty.status {
			Ok((curator, update_due))
		} else {
			Err(Error::<T, I>::ParentBountyNotActive.into())
		}
	}

	fn parent_bounty(
		parent_bounty_id: BountyIndex,
	) -> Result<pallet_bounties::BountyOf<T, I>, DispatchError> {
		let parent_bounty = pallet_bounties::Bounties::<T, I>::get(parent_bounty_id)
			.ok_or(BountiesError::<T, I>::InvalidIndex)?;
		Ok(parent_bounty)
	}

	fn impl_close_child_bounty(
		parent_bounty_id: BountyIndex,
		child_bounty_id: BountyIndex,
	) -> DispatchResult {
		ChildBounties::<T, I>::try_mutate_exists(
			parent_bounty_id,
			child_bounty_id,
			|maybe_child_bounty| -> DispatchResult {
				let child_bounty =
					maybe_child_bounty.as_mut().ok_or(BountiesError::<T, I>::InvalidIndex)?;

				let maybe_curator = match &child_bounty.status {
					ChildBountyStatus::Proposed { .. } |
					ChildBountyStatus::Approved { .. } |
					ChildBountyStatus::ApprovedWithCurator { .. } => {
						// For weight reasons, we don't allow a council to cancel in this phase.
						// We ask for them to wait until it is funded before they can cancel.
						return Err(BountiesError::<T, I>::UnexpectedStatus.into());
					},
					ChildBountyStatus::Funded | ChildBountyStatus::CuratorProposed { .. } => {
						// Nothing extra to do besides initiating refund payment.
						None
					},
					ChildBountyStatus::Active { curator, .. } => {
						// Nothing extra to do besides initiating refund payment.
						Some(curator)
					},
					ChildBountyStatus::PendingPayout { .. } |
					ChildBountyStatus::PayoutAttempted { .. } => {
						// Child-bounty is already in pending payout. If parent
						// curator or RejectOrigin wants to close this
						// child-bounty, it should mean the child-bounty curator
						// was acting maliciously. So first unassign the
						// child-bounty curator, slashing their deposit.
						return Err(BountiesError::<T, I>::PendingPayout.into());
					},
					ChildBountyStatus::RefundAttempted { .. } => {
						// Child Bounty refund is already attempted. Flow should be
						// finished with calling `check_payment_status`
						// or retrying payment with `process_payment`
						// if it failed
						return Err(BountiesError::<T, I>::UnexpectedStatus.into())
					},
				};

				// Transfer fund from child-bounty to parent bounty.
				let parent_bounty = Self::parent_bounty(parent_bounty_id)?;
				let payment_status = Self::do_process_refund_payment(
					parent_bounty_id,
					child_bounty_id,
					parent_bounty.asset_kind.clone(),
					child_bounty.value,
					None,
				)?;

				child_bounty.status = ChildBountyStatus::RefundAttempted {
					payment_status,
					curator: maybe_curator.cloned(),
				};

				Self::deposit_event(Event::<T, I>::Canceled {
					index: parent_bounty_id,
					child_index: child_bounty_id,
				});

				Ok(())
			},
		)
	}

	/// Cleanup a child-bounty from the storage.
	fn remove_child_bounty(parent_bounty_id: BountyIndex, child_bounty_id: BountyIndex) {
		ChildBounties::<T, I>::remove(parent_bounty_id, child_bounty_id);
		ChildBountyDescriptionsV1::<T, I>::remove(parent_bounty_id, child_bounty_id);
		ParentChildBounties::<T, I>::mutate(parent_bounty_id, |count| count.saturating_dec());
	}

	fn calculate_curator_fee_and_payout(
		fee: BalanceOf<T, I>,
		value: BalanceOf<T, I>,
	) -> (BalanceOf<T, I>, BalanceOf<T, I>) {
		let curator_fee = fee.min(value);
		let payout = value.saturating_sub(curator_fee);

		(curator_fee, payout)
	}

	fn do_process_funding_payment(
		parent_bounty_id: BountyIndex,
		child_bounty_id: BountyIndex,
		asset_kind: T::AssetKind,
		value: BalanceOf<T, I>,
		payment_status: Option<PaymentState<PaymentIdOf<T, I>>>,
	) -> Result<PaymentState<PaymentIdOf<T, I>>, DispatchError> {
		if let Some(payment_status) = payment_status {
			ensure!(payment_status.is_pending_or_failed(), BountiesError::<T, I>::UnexpectedStatus);
		}

		let parent_bounty_account = pallet_bounties::Pallet::<T, I>::bounty_account_id(
			parent_bounty_id,
			asset_kind.clone(),
		)?;
		let child_bounty_account = Self::child_bounty_account_id(parent_bounty_id, child_bounty_id);

		let id = <T as pallet_bounties::Config<I>>::Paymaster::pay(
			&parent_bounty_account,
			&child_bounty_account,
			asset_kind.clone(),
			value,
		)
		.map_err(|_| BountiesError::<T, I>::FundingError)?;

		Self::deposit_event(Event::<T, I>::Paid {
			index: parent_bounty_id,
			child_index: child_bounty_id,
			payment_id: id,
		});

		Ok(PaymentState::Attempted { id })
	}

	fn do_process_refund_payment(
		parent_bounty_id: BountyIndex,
		child_bounty_id: BountyIndex,
		asset_kind: T::AssetKind,
		value: BalanceOf<T, I>,
		payment_status: Option<PaymentState<PaymentIdOf<T, I>>>,
	) -> Result<PaymentState<PaymentIdOf<T, I>>, DispatchError> {
		if let Some(payment_status) = payment_status {
			ensure!(payment_status.is_pending_or_failed(), BountiesError::<T, I>::UnexpectedStatus);
		}

		let parent_bounty_account = pallet_bounties::Pallet::<T, I>::bounty_account_id(
			parent_bounty_id,
			asset_kind.clone(),
		)?;
		let child_bounty_account = Self::child_bounty_account_id(parent_bounty_id, child_bounty_id);
		let id = <T as pallet_bounties::Config<I>>::Paymaster::pay(
			&child_bounty_account,
			&parent_bounty_account,
			asset_kind,
			value,
		)
		.map_err(|_| BountiesError::<T, I>::RefundError)?;

		Self::deposit_event(Event::<T, I>::Paid {
			index: parent_bounty_id,
			child_index: child_bounty_id,
			payment_id: id,
		});

		Ok(PaymentState::Attempted { id })
	}

	fn do_process_payout_payment(
		parent_bounty_id: BountyIndex,
		child_bounty_id: BountyIndex,
		child_bounty: &ChildBountyOf<T, I>,
		curator_stash: (T::Beneficiary, Option<PaymentState<PaymentIdOf<T, I>>>),
		beneficiary: (T::Beneficiary, Option<PaymentState<PaymentIdOf<T, I>>>),
	) -> Result<(PaymentState<PaymentIdOf<T, I>>, PaymentState<PaymentIdOf<T, I>>), DispatchError>
	{
		let (mut curator_status, mut beneficiary_status) = (curator_stash.1, beneficiary.1);
		let (process_curator, process_beneficiary) = match (&curator_status, &beneficiary_status) {
			(None, None) => (true, true),
			(Some(curator), Some(beneficiary)) =>
				(curator.is_pending_or_failed(), beneficiary.is_pending_or_failed()),
			_ => unreachable!(),
		};
		ensure!(process_curator || process_beneficiary, BountiesError::<T, I>::UnexpectedStatus);

		let child_bounty_account = Self::child_bounty_account_id(parent_bounty_id, child_bounty_id);
		let (final_fee, payout) =
			Self::calculate_curator_fee_and_payout(child_bounty.fee, child_bounty.value);
		let parent_bounty = Self::parent_bounty(parent_bounty_id)?;

		// Retry curator payout if needed
		if process_curator {
			let id = <T as pallet_bounties::Config<I>>::Paymaster::pay(
				&child_bounty_account,
				&curator_stash.0,
				parent_bounty.asset_kind.clone(),
				final_fee,
			)
			.map_err(|_| BountiesError::<T, I>::PayoutError)?;
			curator_status = Some(PaymentState::Attempted { id });
			Self::deposit_event(Event::<T, I>::Paid {
				index: child_bounty_id,
				child_index: child_bounty_id,
				payment_id: id,
			});
		}

		// Retry beneficiary payout if needed
		if process_beneficiary {
			let id = <T as pallet_bounties::Config<I>>::Paymaster::pay(
				&child_bounty_account,
				&beneficiary.0,
				parent_bounty.asset_kind.clone(),
				payout,
			)
			.map_err(|_| BountiesError::<T, I>::PayoutError)?;
			beneficiary_status = Some(PaymentState::Attempted { id });
			Self::deposit_event(Event::<T, I>::Paid {
				index: child_bounty_id,
				child_index: child_bounty_id,
				payment_id: id,
			});
		}

		// Both will always be `Some` if we are here
		Ok((
			curator_status.unwrap_or(PaymentState::Pending),
			beneficiary_status.unwrap_or(PaymentState::Pending),
		))
	}

	fn do_check_funding_payment_status(
		parent_bounty_id: BountyIndex,
		child_bounty_id: BountyIndex,
		payment_status: PaymentState<PaymentIdOf<T, I>>,
	) -> Result<PaymentState<PaymentIdOf<T, I>>, DispatchError> {
		let payment_id =
			payment_status.get_attempt_id().ok_or(BountiesError::<T, I>::UnexpectedStatus)?;

		match <T as pallet_bounties::Config<I>>::Paymaster::check_payment(payment_id) {
			PaymentStatus::Success => {
				Self::deposit_event(Event::<T, I>::BecameActive {
					index: parent_bounty_id,
					child_index: child_bounty_id,
				});
				Ok(PaymentState::Succeeded)
			},
			PaymentStatus::InProgress =>
				return Err(BountiesError::<T, I>::FundingInconclusive.into()),
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

	fn do_check_refund_payment_status(
		parent_bounty_id: BountyIndex,
		child_bounty_id: BountyIndex,
		child_bounty: &ChildBountyOf<T, I>,
		payment_status: PaymentState<PaymentIdOf<T, I>>,
		curator: Option<T::AccountId>,
	) -> Result<PaymentState<PaymentIdOf<T, I>>, DispatchError> {
		let payment_id =
			payment_status.get_attempt_id().ok_or(BountiesError::<T, I>::UnexpectedStatus)?;

		match <T as pallet_bounties::Config<I>>::Paymaster::check_payment(payment_id) {
			PaymentStatus::Success => {
				if let Some(curator) = curator {
					// Cancelled by parent curator or RejectOrigin,
					// refund deposit of the working child-bounty curator.
					let err_amount = T::Currency::unreserve(&curator, child_bounty.curator_deposit);
					debug_assert!(err_amount.is_zero());
				}
				// Revert the curator fee back to parent bounty curator
				// & reduce the active child-bounty count.
				ChildrenValue::<T, I>::mutate(parent_bounty_id, |value| {
					*value = value.saturating_sub(child_bounty.value)
				});
				ChildrenCuratorFees::<T, I>::mutate(parent_bounty_id, |value| {
					*value = value.saturating_sub(child_bounty.fee)
				});
				// refund succeeded, cleanup the bounty
				Self::remove_child_bounty(parent_bounty_id, child_bounty_id);
				Self::deposit_event(Event::<T, I>::RefundProcessed {
					index: parent_bounty_id,
					child_index: child_bounty_id,
				});
				Ok(PaymentState::Succeeded)
			},
			PaymentStatus::InProgress =>
			// nothing new to report
				Err(BountiesError::<T, I>::RefundInconclusive.into()),
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

	fn do_check_payout_payment_status(
		parent_bounty_id: BountyIndex,
		child_bounty_id: BountyIndex,
		child_bounty: &ChildBountyOf<T, I>,
		curator: T::AccountId,
		curator_stash: (T::Beneficiary, PaymentState<PaymentIdOf<T, I>>),
		beneficiary: (T::Beneficiary, PaymentState<PaymentIdOf<T, I>>),
	) -> Result<(PaymentState<PaymentIdOf<T, I>>, PaymentState<PaymentIdOf<T, I>>), DispatchError>
	{
		// counters for payments that have changed state during this call and that have finished
		// processing successfully. For If one payment succeeds and another fails, both count as
		// "progressed" since they advanced the state machine.
		let (mut payments_progressed, mut payments_succeeded) = (0, 0);
		// check both curator stash, and beneficiary payments
		let (mut curator_stash_status, mut beneficiary_status) = (curator_stash.1, beneficiary.1);
		for payment_status in [&mut curator_stash_status, &mut beneficiary_status] {
			match payment_status {
				PaymentState::Attempted { id } =>
					match <T as pallet_bounties::Config<I>>::Paymaster::check_payment(*id) {
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
				_ => {
					// return function without error so we could drive the next payment
				},
			}
		}

		// best scenario, both payments have succeeded,
		// emit events and advance state machine to the end
		if payments_succeeded >= 2 {
			let (_final_fee, payout) =
				Self::calculate_curator_fee_and_payout(child_bounty.fee, child_bounty.value);

			// payout succeeded, cleanup the bounty
			Self::remove_child_bounty(parent_bounty_id, child_bounty_id);

			// Unreserve the curator deposit when payment succeeds. Should not
			// fail because the deposit is always reserved when curator
			// is assigned.
			let parent_bounty = Self::parent_bounty(parent_bounty_id)?;
			let _ = T::Currency::unreserve(&curator, child_bounty.curator_deposit);

			Self::deposit_event(Event::<T, I>::PayoutProcessed {
				index: parent_bounty_id,
				child_index: child_bounty_id,
				asset_kind: parent_bounty.asset_kind.clone(),
				value: payout,
				beneficiary: beneficiary.0.clone(),
			});

			return Ok((curator_stash_status, beneficiary_status));
		} else if payments_progressed > 0 {
			// some payments have progressed in the state machine
			// return ok so these changes are saved to the state
			return Ok((curator_stash_status, beneficiary_status));
		} else {
			// no progress was made in the state machine if we're here,
			return Err(BountiesError::<T, I>::PayoutInconclusive.into())
		}
	}
}

/// Implement ChildBountyManager to connect with the bounties pallet. This is
/// where we pass the active child bounties and child curator fees to the parent
/// bounty.
///
/// Function `children_curator_fees` not only returns the fee but also removes cumulative curator
/// fees during call.
impl<T: Config<I>, I: 'static> pallet_bounties::ChildBountyManager<BalanceOf<T, I>>
	for Pallet<T, I>
{
	/// Returns number of active child bounties for `bounty_id`
	fn child_bounties_count(
		bounty_id: pallet_bounties::BountyIndex,
	) -> pallet_bounties::BountyIndex {
		ParentChildBounties::<T, I>::get(bounty_id)
	}

	/// Returns cumulative child-bounties' value for `bounty_id` and removes the associated
	/// storage item. This function is assumed to be called when parent bounty is claimed.
	fn children_value(bounty_id: pallet_bounties::BountyIndex) -> BalanceOf<T, I> {
		// This is asked for when the parent bounty is being claimed. No use of
		// keeping it in state after that. Hence removing.
		let children_value_total = ChildrenValue::<T, I>::get(bounty_id);
		ChildrenValue::<T, I>::remove(bounty_id);
		children_value_total
	}

	/// Returns cumulative child-bounties' curator fees for `bounty_id` and removes the associated
	/// storage item. This function is assumed to be called when the parent bounty is claimed.
	fn children_curator_fees(bounty_id: pallet_bounties::BountyIndex) -> BalanceOf<T, I> {
		// This is asked for when the parent bounty is being claimed. No use of
		// keeping it in state after that. Hence removing.
		let children_fee_total = ChildrenCuratorFees::<T, I>::get(bounty_id);
		ChildrenCuratorFees::<T, I>::remove(bounty_id);
		children_fee_total
	}

	/// Clean up the storage on a parent bounty removal.
	fn bounty_removed(bounty_id: BountyIndex) {
		debug_assert!(ParentChildBounties::<T, I>::get(bounty_id).is_zero());
		debug_assert!(ChildrenValue::<T, I>::get(bounty_id).is_zero());
		debug_assert!(ChildrenCuratorFees::<T, I>::get(bounty_id).is_zero());
		debug_assert!(ChildBounties::<T, I>::iter_key_prefix(bounty_id).count().is_zero());
		debug_assert!(ChildBountyDescriptionsV1::<T, I>::iter_key_prefix(bounty_id)
			.count()
			.is_zero());
		ParentChildBounties::<T, I>::remove(bounty_id);
		ParentTotalChildBounties::<T, I>::remove(bounty_id);
	}
}
