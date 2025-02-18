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

// TODO: update docs.
//! # Child Bounties Pallet ( `pallet-child-bounties` )
//!
//! ## Child Bounty
//!
//! > NOTE: This pallet is tightly coupled with `pallet-treasury` and `pallet-bounties`.
//!
//! With child bounties, a large bounty proposal can be divided into smaller chunks,
//! for parallel execution, and for efficient governance and tracking of spent funds.
//! A child bounty is a smaller piece of work, extracted from a parent bounty.
//! A curator is assigned after the child bounty is created by the parent bounty curator,
//! to be delegated with the responsibility of assigning a payout address once the specified
//! set of tasks is completed.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! Child Bounty protocol:
//! - `add_child_bounty` - Add a child bounty for a parent bounty to for dividing the work in
//!   smaller tasks.
//! - `propose_curator` - Assign an account to a child bounty as candidate curator.
//! - `accept_curator` - Accept a child bounty assignment from the parent bounty curator, setting a
//!   curator deposit.
//! - `award_child_bounty` - Close and pay out the specified amount for the completed work.
//! - `claim_child_bounty` - Claim a specific child bounty amount from the payout address.
//! - `unassign_curator` - Unassign an accepted curator from a specific child bounty.
//! - `close_child_bounty` - Cancel the child bounty for a specific treasury amount and close the
//!   bounty.

// Most of the business logic in this pallet has been
// originally contributed by "https://github.com/shamb0",
// as part of the PR - https://github.com/paritytech/substrate/pull/7965.
// The code has been moved here and then refactored in order to
// extract child bounties as a separate pallet.

#![cfg_attr(not(feature = "std"), no_std)]

mod benchmarking;
pub mod migration;
mod tests;
pub mod weights;
pub use pallet::*;
pub use weights::WeightInfo;

extern crate alloc;
use alloc::vec::Vec;
use frame_support::pallet_prelude::*;
use frame_support::traits::{
	tokens::{ConversionFromAssetBalance, Pay},
	Currency,
	ExistenceRequirement::AllowDeath,
	Get, OnUnbalanced, ReservableCurrency,
};
use frame_system::pallet_prelude::{
	ensure_signed, BlockNumberFor as SystemBlockNumberFor, OriginFor,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{AccountIdConversion, BadOrigin, BlockNumberProvider, Saturating, StaticLookup, Zero},
	DispatchResult, RuntimeDebug,
};

type BeneficiaryLookupOf<T, I = ()> = pallet_treasury::BeneficiaryLookupOf<T, I>;
type DepositBalanceOf<T, I = ()> = pallet_treasury::BalanceOf<T, I>;
type BountyBalanceOf<T, I = ()> = pallet_treasury::AssetBalanceOf<T, I>;
type BountiesError<T, I> = pallet_bounties::Error<T, I>;
type BountyIndex = pallet_bounties::BountyIndex;
type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;
type BlockNumberFor<T, I = ()> =
	<<T as pallet_treasury::Config<I>>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;
type PaymentIdOf<T, I = ()> = <<T as pallet_treasury::Config<I>>::Paymaster as Pay>::Id;
type ChildBountyOf<T, I> = ChildBounty<
	<T as frame_system::Config>::AccountId,
	DepositBalanceOf<T, I>,
	BountyBalanceOf<T, I>,
	BlockNumberFor<T, I>,
	<T as pallet_treasury::Config<I>>::AssetKind,
	PaymentIdOf<T, I>,
	<T as pallet_treasury::Config<I>>::Beneficiary,
>;
/// The status of a child-bounty.
pub type ChildBountyStatus<AccountId, BlockNumber, PaymentId, Beneficiary> =
	pallet_bounties::BountyStatus<AccountId, BlockNumber, PaymentId, Beneficiary>;

/// The log target for this pallet.
const LOG_TARGET: &str = "runtime::child-bounties";

/// A child bounty proposal.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ChildBounty<
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
	/// The parent of this child-bounty.
	parent_bounty: BountyIndex,
	// TODO: new filed, migration required.
	/// The kind of asset this child-bounty is rewarded in.
	pub asset_kind: AssetKind,
	/// The (total) amount that should be paid if this child-bounty is rewarded.
	value: BountyBalance,
	/// The child bounty curator fee in the `asset_kind`. Included in value.
	fee: BountyBalance,
	// TODO: use consideration
	/// The deposit of child-bounty curator.
	///
	/// The asset class determined by the [`pallet_treasury::Config::Currency`].
	curator_deposit: NativeBalance,
	/// The status of this child-bounty.
	status: ChildBountyStatus<AccountId, BlockNumber, PaymentId, Beneficiary>,
}

#[frame_support::pallet]
pub mod pallet {

	use super::*;

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

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
		type ChildBountyValueMinimum: Get<DepositBalanceOf<Self, I>>;

		/// The overarching event type.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: benchmarking::ArgumentsFactory<Self::AssetKind>;
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

	// TODO: add new parameters for events
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// A child-bounty is added.
		Added { index: BountyIndex, child_index: BountyIndex },
		/// A child-bounty is awarded to a beneficiary.
		Awarded {
			index: BountyIndex,
			child_index: BountyIndex,
			beneficiary: <T as pallet_treasury::Config<I>>::Beneficiary,
		},
		/// A child-bounty is claimed by beneficiary.
		Claimed {
			index: BountyIndex,
			child_index: BountyIndex,
			asset_kind: T::AssetKind,
			asset_payout: BountyBalanceOf<T, I>,
			beneficiary: <T as pallet_treasury::Config<I>>::Beneficiary,
		},
		/// A child-bounty is cancelled.
		Canceled { index: BountyIndex, child_index: BountyIndex },
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

	/// The mapping of the child bounty ids from storage version `V0` to the new `V1` version.
	///
	/// The `V0` ids based on total child bounty count [`ChildBountyCount`]`. The `V1` version ids
	/// based on the child bounty count per parent bounty [`ParentTotalChildBounties`].
	/// The item intended solely for client convenience and not used in the pallet's core logic.
	#[pallet::storage]
	pub type V0ToV1ChildBountyIds<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BountyIndex, (BountyIndex, BountyIndex)>;

	/// The cumulative child-bounty curator fee for each parent bounty.
	#[pallet::storage]
	pub type ChildrenCuratorFees<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BountyIndex, BountyBalanceOf<T, I>, ValueQuery>;

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		// TODO: use pallet_treasury::Config::Paymaster for all transfers (except deposits).

		// TODO: update doc. `value` in the asset of the parent bounty.

		/// Add a new child-bounty.
		///
		/// The dispatch origin for this call must be the curator of parent
		/// bounty and the parent bounty must be in "active" state.
		///
		/// Child-bounty gets added successfully & fund gets transferred from
		/// parent bounty to child-bounty account, if parent bounty has enough
		/// funds, else the call fails.
		///
		/// Upper bound to maximum number of active  child bounties that can be
		/// added are managed via runtime trait config
		/// [`Config::MaxActiveChildBountyCount`].
		///
		/// If the call is success, the status of child-bounty is updated to
		/// "Added".
		///
		/// - `parent_bounty_id`: Index of parent bounty for which child-bounty is being added.
		/// - `value`: Value for executing the proposal.
		/// - `description`: Text description for the child-bounty.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::add_child_bounty(description.len() as u32))]
		pub fn add_child_bounty(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			// TODO: `value` should pallet_treasury::AssetBalanceOf<T, I>,
			// or better make pallet_treasury::AssetBalanceOf<T, I> and
			// pallet_treasury::BalanceOf<T, I> same types for simplicity.
			#[pallet::compact] value: BountyBalanceOf<T, I>,
			description: Vec<u8>,
		) -> DispatchResult {
			let signer = ensure_signed(origin)?;

			// Verify the arguments.
			let bounded_description =
				description.try_into().map_err(|_| BountiesError::<T, I>::ReasonTooBig)?;
			let asset_kind = Self::bounty_asset_kind(parent_bounty_id)?;
			let native_amount =
				<T as pallet_treasury::Config<I>>::BalanceConverter::from_asset_balance(
					value,
					asset_kind.clone(),
				)
				.map_err(|_| pallet_treasury::Error::<T, I>::FailedToConvertBalance)?;

			ensure!(
				native_amount >= T::ChildBountyValueMinimum::get(),
				BountiesError::<T, I>::InvalidValue
			);
			ensure!(
				ParentChildBounties::<T, I>::get(parent_bounty_id)
					<= T::MaxActiveChildBountyCount::get() as u32,
				Error::<T, I>::TooManyChildBounties,
			);

			let (curator, _) = Self::ensure_bounty_active(parent_bounty_id)?;
			ensure!(signer == curator, BountiesError::<T, I>::RequireCurator);

			// Read parent bounty account info.
			let _parent_bounty_account =
				pallet_bounties::Pallet::<T, I>::bounty_account_id(parent_bounty_id);

			// Ensure parent bounty has enough balance after adding child-bounty.
			// TODO: fix payment
			// let bounty_balance = T::Currency::free_balance(&parent_bounty_account);
			// let new_bounty_balance = bounty_balance
			// 	.checked_sub(&value)
			// 	.ok_or(Error::<T>::InsufficientBountyBalance)?;
			// T::Currency::ensure_can_withdraw(
			// 	&parent_bounty_account,
			// 	value,
			// 	WithdrawReasons::TRANSFER,
			// 	new_bounty_balance,
			// )?;

			// Get child-bounty ID.
			let child_bounty_id = ParentTotalChildBounties::<T, I>::get(parent_bounty_id);
			let _child_bounty_account =
				Self::child_bounty_account_id(parent_bounty_id, child_bounty_id);

			// Transfer funds from parent bounty to child-bounty.
			// T::Currency::transfer(&parent_bounty_account, &child_bounty_account, value,
			// KeepAlive)?;

			// Increment the active child-bounty count.
			ParentChildBounties::<T, I>::mutate(parent_bounty_id, |count| count.saturating_inc());
			ParentTotalChildBounties::<T, I>::insert(
				parent_bounty_id,
				child_bounty_id.saturating_add(1),
			);

			// Create child-bounty instance.
			Self::create_child_bounty(
				parent_bounty_id,
				child_bounty_id,
				asset_kind,
				value,
				bounded_description,
			);
			Ok(())
		}

		/// Propose curator for funded child-bounty.
		///
		/// The dispatch origin for this call must be curator of parent bounty.
		///
		/// Parent bounty must be in active state, for this child-bounty call to
		/// work.
		///
		/// Child-bounty must be in "Added" state, for processing the call. And
		/// state of child-bounty is moved to "CuratorProposed" on successful
		/// call completion.
		///
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child bounty.
		/// - `curator`: Address of child-bounty curator.
		/// - `fee`: payment fee to child-bounty curator for execution.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::propose_curator())]
		pub fn propose_curator(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			#[pallet::compact] child_bounty_id: BountyIndex,
			curator: AccountIdLookupOf<T>,
			#[pallet::compact] fee: BountyBalanceOf<T, I>,
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
		/// The dispatch origin for this call must be the curator of this
		/// child-bounty.
		///
		/// A deposit will be reserved from the curator and refund upon
		/// successful payout or cancellation.
		///
		/// Fee for curator is deducted from curator fee of parent bounty.
		///
		/// Parent bounty must be in active state, for this child-bounty call to
		/// work.
		///
		/// Child-bounty must be in "CuratorProposed" state, for processing the
		/// call. And state of child-bounty is moved to "Active" on successful
		/// call completion.
		///
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child bounty.
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::accept_curator())]
		pub fn accept_curator(
			origin: OriginFor<T>,
			#[pallet::compact] parent_bounty_id: BountyIndex,
			#[pallet::compact] child_bounty_id: BountyIndex,
			// TODO: docs: curator stash account to receive the curator fee.
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

						let asset_kind = Self::bounty_asset_kind(parent_bounty_id)?;
						// Reserve child-bounty curator deposit.
						let deposit = Self::calculate_curator_deposit(
							&parent_curator,
							curator,
							&child_bounty.fee,
							asset_kind,
						)?;

						// TODO: use consideration here
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
		/// The dispatch origin for this call can be either `RejectOrigin`, or
		/// the curator of the parent bounty, or any signed origin.
		///
		/// For the origin other than T::RejectOrigin and the child-bounty
		/// curator, parent bounty must be in active state, for this call to
		/// work. We allow child-bounty curator and T::RejectOrigin to execute
		/// this call irrespective of the parent bounty state.
		///
		/// If this function is called by the `RejectOrigin` or the
		/// parent bounty curator, we assume that the child-bounty curator is
		/// malicious or inactive. As a result, child-bounty curator deposit is
		/// slashed.
		///
		/// If the origin is the child-bounty curator, we take this as a sign
		/// that they are unable to do their job, and are willingly giving up.
		/// We could slash the deposit, but for now we allow them to unreserve
		/// their deposit and exit without issue. (We may want to change this if
		/// it is abused.)
		///
		/// Finally, the origin can be anyone iff the child-bounty curator is
		/// "inactive". Expiry update due of parent bounty is used to estimate
		/// inactive state of child-bounty curator.
		///
		/// This allows anyone in the community to call out that a child-bounty
		/// curator is not doing their due diligence, and we should pick a new
		/// one. In this case the child-bounty curator deposit is slashed.
		///
		/// State of child-bounty is moved to Added state on successful call
		/// completion.
		///
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child bounty.
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
						|curator: &T::AccountId, curator_deposit: &mut DepositBalanceOf<T, I>| {
							// TODO: use consideration
							let imbalance =
								T::Currency::slash_reserved(curator, *curator_deposit).0;
							T::OnSlash::on_unbalanced(imbalance);
							*curator_deposit = Zero::zero();
						};

					match child_bounty.status {
						ChildBountyStatus::Proposed
						| ChildBountyStatus::Approved { .. }
						| ChildBountyStatus::Funded
						| ChildBountyStatus::PayoutAttempted { .. }
						| ChildBountyStatus::RefundAttempted { .. } => {
							// No curator to unassign at this point.
							return Err(BountiesError::<T, I>::UnexpectedStatus.into());
						},
						ChildBountyStatus::ApprovedWithCurator { ref curator, ref payment_status } => {
							// Bounty not yet funded, but bounty was approved with curator.
							// `RejectOrigin` or curator himself can unassign from this bounty.
							ensure!(
								maybe_sender.map_or(true, |sender| sender == *curator),
								BadOrigin
							);
							// This state can only be while the bounty is not yet funded so we return
							// bounty to the `Approved` state without curator
							child_bounty.status =
							ChildBountyStatus::Approved { payment_status: payment_status.clone() };
							return Ok(());
						},
						ChildBountyStatus::CuratorProposed { ref curator } => {
							// A child-bounty curator has been proposed, but not accepted yet.
							// Either `RejectOrigin`, parent bounty curator or the proposed
							// child-bounty curator can unassign the child-bounty curator.
							ensure!(
								maybe_sender.map_or(true, |sender| {
									sender == *curator
										|| Self::ensure_bounty_active(parent_bounty_id)
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
									// TODO: use consideration here
									T::Currency::unreserve(curator, child_bounty.curator_deposit);
									// Reset curator deposit.
									child_bounty.curator_deposit = Zero::zero();
									// Continue to change bounty status below.
								},
								Some(sender) => {
									let (parent_curator, update_due) =
										Self::ensure_bounty_active(parent_bounty_id)?;
									if sender == parent_curator
										|| update_due < Self::treasury_block_number()
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
		/// The beneficiary will be able to claim the funds after a delay.
		///
		/// The dispatch origin for this call must be the parent curator or
		/// curator of this child-bounty.
		///
		/// Parent bounty must be in active state, for this child-bounty call to
		/// work.
		///
		/// Child-bounty must be in active state, for processing the call. And
		/// state of child-bounty is moved to "PendingPayout" on successful call
		/// completion.
		///
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child bounty.
		/// - `beneficiary`: Beneficiary account.
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
							unlock_at: Self::treasury_block_number()
								+ T::BountyDepositPayoutDelay::get(),
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

		// TODO: should be able to retry claim if the payment from prev claim attempt failed.
		/// Claim the payout from an awarded child-bounty after payout delay.
		///
		/// The dispatch origin for this call may be any signed origin.
		///
		/// Call works independent of parent bounty state, No need for parent
		/// bounty to be in active state.
		///
		/// The Beneficiary is paid out with agreed bounty value. Curator fee is
		/// paid & curator deposit is unreserved.
		///
		/// Child-bounty must be in "PendingPayout" state, for processing the
		/// call. And instance of child-bounty is removed from the state on
		/// successful call completion.
		///
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child bounty.
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
						beneficiary: _,
						ref unlock_at,
						curator_stash: _,
					} = &child_bounty.status
					{
						// Ensure block number is elapsed for processing the
						// claim.
						ensure!(
							Self::treasury_block_number() >= *unlock_at,
							BountiesError::<T, I>::Premature,
						);

						// Make curator fee payment.
						let _child_bounty_account =
							Self::child_bounty_account_id(parent_bounty_id, child_bounty_id);
						// TODO: use Paymaster trait here
						// let balance = T::Currency::free_balance(&child_bounty_account);
						// let curator_fee = child_bounty.fee.min(balance);
						// let payout = balance.saturating_sub(curator_fee);

						// Unreserve the curator deposit. Should not fail
						// because the deposit is always reserved when curator is
						// assigned.
						// TODO: use consideration here
						let _ = T::Currency::unreserve(curator, child_bounty.curator_deposit);

						// Make payout to child-bounty curator.
						// Should not fail because curator fee is always less than bounty value.
						// TODO: use Paymaster trait here
						// let fee_transfer_result = T::Currency::transfer(
						// 	&child_bounty_account,
						// 	curator,
						// 	curator_fee,
						// 	AllowDeath,
						// );
						// debug_assert!(fee_transfer_result.is_ok());

						// Make payout to beneficiary.
						// Should not fail.
						// TODO: use Paymaster trait here
						// let payout_transfer_result = T::Currency::transfer(
						// 	&child_bounty_account,
						// 	beneficiary,
						// 	payout,
						// 	AllowDeath,
						// );
						// debug_assert!(payout_transfer_result.is_ok());

						// Trigger the Claimed event.
						// Self::deposit_event(Event::<T>::Claimed {
						// 	index: parent_bounty_id,
						// 	child_index: child_bounty_id,
						// 	payout,
						// 	beneficiary: beneficiary.clone(),
						// });

						// Update the active child-bounty tracking count.
						ParentChildBounties::<T, I>::mutate(parent_bounty_id, |count| {
							count.saturating_dec()
						});

						// Remove the child-bounty description.
						ChildBountyDescriptionsV1::<T, I>::remove(
							parent_bounty_id,
							child_bounty_id,
						);

						// Remove the child-bounty instance from the state.
						*maybe_child_bounty = None;

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
		/// The dispatch origin for this call must be either parent curator or
		/// `T::RejectOrigin`.
		///
		/// If the state of child-bounty is `Active`, curator deposit is
		/// unreserved.
		///
		/// If the state of child-bounty is `PendingPayout`, call fails &
		/// returns `PendingPayout` error.
		///
		/// For the origin other than T::RejectOrigin, parent bounty must be in
		/// active state, for this child-bounty call to work. For origin
		/// T::RejectOrigin execution is forced.
		///
		/// Instance of child-bounty is removed from the state on successful
		/// call completion.
		///
		/// - `parent_bounty_id`: Index of parent bounty.
		/// - `child_bounty_id`: Index of child bounty.
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

	// TODO: similar with `pallet_bounties` add `process_payment` and `check_payment_status`.
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Get the block number used in the treasury pallet.
	///
	/// It may be configured to use the relay chain block number on a parachain.
	pub fn treasury_block_number() -> BlockNumberFor<T, I> {
		<T as pallet_treasury::Config<I>>::BlockNumberProvider::current_block_number()
	}

	// This function will calculate the deposit of a curator.
	fn calculate_curator_deposit(
		parent_curator: &T::AccountId,
		child_curator: &T::AccountId,
		bounty_fee: &BountyBalanceOf<T, I>,
		asset_kind: T::AssetKind,
	) -> Result<DepositBalanceOf<T, I>, pallet_treasury::Error<T, I>> {
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
	) -> T::AccountId {
		// This function is taken from the parent (bounties) pallet, but the
		// prefix is changed to have different AccountId when the index of
		// parent and child is same.
		T::PalletId::get().into_sub_account_truncating(("cb", parent_bounty_id, child_bounty_id))
	}

	fn create_child_bounty(
		parent_bounty_id: BountyIndex,
		child_bounty_id: BountyIndex,
		child_bounty_asset_kind: T::AssetKind,
		child_bounty_value: BountyBalanceOf<T, I>,
		description: BoundedVec<u8, T::MaximumReasonLength>,
	) {
		let child_bounty = ChildBounty {
			parent_bounty: parent_bounty_id,
			asset_kind: child_bounty_asset_kind,
			value: child_bounty_value,
			fee: 0u32.into(),
			curator_deposit: 0u32.into(),
			status: ChildBountyStatus::Proposed,
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
		if let ChildBountyStatus::Active { curator, update_due, .. } = parent_bounty.get_status() {
			Ok((curator, update_due))
		} else {
			Err(Error::<T, I>::ParentBountyNotActive.into())
		}
	}

	fn bounty_asset_kind(parent_bounty_id: BountyIndex) -> Result<T::AssetKind, DispatchError> {
		let parent_bounty = pallet_bounties::Bounties::<T, I>::get(parent_bounty_id)
			.ok_or(BountiesError::<T, I>::InvalidIndex)?;
		Ok(parent_bounty.asset_kind.clone())
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

				match &child_bounty.status {
					ChildBountyStatus::Proposed => {
						*maybe_child_bounty = None;

						Self::deposit_event(Event::<T, I>::Canceled {
							index: parent_bounty_id,
							child_index: child_bounty_id,
						});
						// Return early, nothing else to do.
						return Ok(());
					},
					ChildBountyStatus::Approved { .. } | ChildBountyStatus::ApprovedWithCurator { .. } => {
						// For weight reasons, we don't allow a council to cancel in this phase.
						// We ask for them to wait until it is funded before they can cancel.
						return Err(BountiesError::<T, I>::UnexpectedStatus.into());
					},
					ChildBountyStatus::Funded | ChildBountyStatus::CuratorProposed { .. } => {
						// Nothing extra to do besides the removal of the child-bounty below.
					},
					ChildBountyStatus::Active { curator, .. } => {
						// Cancelled by parent curator or RejectOrigin,
						// refund deposit of the working child-bounty curator.
						let _ = T::Currency::unreserve(curator, child_bounty.curator_deposit);
						// Then execute removal of the child-bounty below.
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
						return Err(BountiesError::<T, I>::PendingPayout.into())
					},
				}

				// Revert the curator fee back to parent bounty curator &
				// reduce the active child-bounty count.
				ChildrenCuratorFees::<T, I>::mutate(parent_bounty_id, |value| {
					*value = value.saturating_sub(child_bounty.fee)
				});
				ParentChildBounties::<T, I>::mutate(parent_bounty_id, |count| {
					*count = count.saturating_sub(1)
				});

				// Transfer fund from child-bounty to parent bounty.
				let _parent_bounty_account =
					pallet_bounties::Pallet::<T, I>::bounty_account_id(parent_bounty_id);
				let _child_bounty_account =
					Self::child_bounty_account_id(parent_bounty_id, child_bounty_id);
				// TODO: use paymaster transfer instead of currency
				// let balance = T::Currency::free_balance(&child_bounty_account);
				// let transfer_result = T::Currency::transfer(
				// 	&child_bounty_account,
				// 	&parent_bounty_account,
				// 	balance,
				// 	AllowDeath,
				// ); // Should not fail; child bounty account gets this balance during creation.
				// debug_assert!(transfer_result.is_ok());

				// Remove the child-bounty description.
				ChildBountyDescriptionsV1::<T, I>::remove(parent_bounty_id, child_bounty_id);

				*maybe_child_bounty = None;

				Self::deposit_event(Event::<T, I>::Canceled {
					index: parent_bounty_id,
					child_index: child_bounty_id,
				});
				Ok(())
			},
		)
	}
}

/// Implement ChildBountyManager to connect with the bounties pallet. This is
/// where we pass the active child bounties and child curator fees to the parent
/// bounty.
///
/// Function `children_curator_fees` not only returns the fee but also removes cumulative curator
/// fees during call.
impl<T: Config<I>, I: 'static> pallet_bounties::ChildBountyManager<BountyBalanceOf<T, I>>
	for Pallet<T, I>
{
	/// Returns number of active child bounties for `bounty_id`
	fn child_bounties_count(
		bounty_id: pallet_bounties::BountyIndex,
	) -> pallet_bounties::BountyIndex {
		ParentChildBounties::<T, I>::get(bounty_id)
	}

	/// Returns cumulative child bounty curator fees for `bounty_id` also removing the associated
	/// storage item. This function is assumed to be called when parent bounty is claimed.
	fn children_curator_fees(bounty_id: pallet_bounties::BountyIndex) -> BountyBalanceOf<T, I> {
		// This is asked for when the parent bounty is being claimed. No use of
		// keeping it in state after that. Hence removing.
		let children_fee_total = ChildrenCuratorFees::<T, I>::get(bounty_id);
		ChildrenCuratorFees::<T, I>::remove(bounty_id);
		children_fee_total
	}

	/// Clean up the storage on a parent bounty removal.
	fn bounty_removed(bounty_id: BountyIndex) {
		debug_assert!(ParentChildBounties::<T, I>::get(bounty_id).is_zero());
		debug_assert!(ChildrenCuratorFees::<T, I>::get(bounty_id).is_zero());
		debug_assert!(ChildBounties::<T, I>::iter_key_prefix(bounty_id).count().is_zero());
		debug_assert!(ChildBountyDescriptionsV1::<T, I>::iter_key_prefix(bounty_id)
			.count()
			.is_zero());
		ParentChildBounties::<T, I>::remove(bounty_id);
		ParentTotalChildBounties::<T, I>::remove(bounty_id);
	}
}
