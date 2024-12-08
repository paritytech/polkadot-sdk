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
//! [![github]](https://github.com/paritytech/substrate/frame/fast-unstake) -
//! [![polkadot]](https://polkadot.network)
//!
//! [polkadot]: https://img.shields.io/badge/polkadot-E6007A?style=for-the-badge&logo=polkadot&logoColor=white
//! [github]: https://img.shields.io/badge/github-8da0cb?style=for-the-badge&labelColor=555555&logo=github
//!
//! # Treasury Pallet
//!
//! The Treasury pallet provides a "pot" of funds that can be managed by stakeholders in the system
//! and a structure for making spending proposals from this pot.
//!
//! ## Overview
//!
//! The Treasury Pallet itself provides the pot to store funds, and a means for stakeholders to
//! propose and claim expenditures (aka spends). The chain will need to provide a method to approve
//! spends (e.g. public referendum) and a method for collecting funds (e.g. inflation, fees).
//!
//! By way of example, stakeholders could vote to fund the Treasury with a portion of the block
//! reward and use the funds to pay developers.
//!
//! ### Terminology
//!
//! - **Proposal:** A suggestion to allocate funds from the pot to a beneficiary.
//! - **Beneficiary:** An account who will receive the funds from a proposal iff the proposal is
//!   approved.
//! - **Pot:** Unspent funds accumulated by the treasury pallet.
//! - **Spend** An approved proposal for transferring a specific amount of funds to a designated
//!   beneficiary.
//!
//! ### Example
//!
//! 1. Multiple local spends approved by spend origins and received by a beneficiary.
#![doc = docify::embed!("src/tests.rs", spend_local_origin_works)]
//!
//! 2. Approve a spend of some asset kind and claim it.
#![doc = docify::embed!("src/tests.rs", spend_payout_works)]
//!
//! ## Pallet API
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes,
//! including its configuration trait, dispatchables, storage items, events and errors.
//!
//! ## Low Level / Implementation Details
//!
//! Spends can be initiated using either the `spend_local` or `spend` dispatchable. The
//! `spend_local` dispatchable enables the creation of spends using the native currency of the
//! chain, utilizing the funds stored in the pot. These spends are automatically paid out every
//! [`pallet::Config::SpendPeriod`]. On the other hand, the `spend` dispatchable allows spending of
//! any asset kind managed by the treasury, with payment facilitated by a designated
//! [`pallet::Config::Paymaster`]. To claim these spends, the `payout` dispatchable should be called
//! within some temporal bounds, starting from the moment they become valid and within one
//! [`pallet::Config::PayoutPeriod`].

#![cfg_attr(not(feature = "std"), no_std)]

mod benchmarking;
pub mod migration;
#[cfg(test)]
mod tests;
pub mod weights;
use core::marker::PhantomData;

#[cfg(feature = "runtime-benchmarks")]
pub use benchmarking::ArgumentsFactory;

extern crate alloc;

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

use alloc::{boxed::Box, collections::btree_map::BTreeMap};
use sp_runtime::{
	traits::{
		AccountIdConversion, BlockNumberProvider, CheckedAdd, One, Saturating, StaticLookup,
		UniqueSaturatedInto, Zero,
	},
	PerThing, Permill, RuntimeDebug,
};

use frame_support::{
	dispatch::{DispatchResult, DispatchResultWithPostInfo},
	ensure, print,
	traits::{
		tokens::Pay, Currency, ExistenceRequirement::KeepAlive, Get, Imbalance, OnUnbalanced,
		ReservableCurrency, WithdrawReasons,
	},
	weights::Weight,
	BoundedVec, PalletId,
};
use frame_system::pallet_prelude::BlockNumberFor as SystemBlockNumberFor;

pub use pallet::*;
pub use weights::WeightInfo;

pub type BalanceOf<T, I = ()> =
	<<T as Config<I>>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
pub type AssetBalanceOf<T, I> = <<T as Config<I>>::Paymaster as Pay>::Balance;
pub type PositiveImbalanceOf<T, I = ()> = <<T as Config<I>>::Currency as Currency<
	<T as frame_system::Config>::AccountId,
>>::PositiveImbalance;
pub type NegativeImbalanceOf<T, I = ()> = <<T as Config<I>>::Currency as Currency<
	<T as frame_system::Config>::AccountId,
>>::NegativeImbalance;
type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;
type BeneficiaryLookupOf<T, I> = <<T as Config<I>>::BeneficiaryLookup as StaticLookup>::Source;
pub type BlockNumberFor<T, I = ()> =
	<<T as Config<I>>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;

/// A trait to allow the Treasury Pallet to spend it's funds for other purposes.
/// There is an expectation that the implementer of this trait will correctly manage
/// the mutable variables passed to it:
/// * `budget_remaining`: How much available funds that can be spent by the treasury. As funds are
///   spent, you must correctly deduct from this value.
/// * `imbalance`: Any imbalances that you create should be subsumed in here to maximize efficiency
///   of updating the total issuance. (i.e. `deposit_creating`)
/// * `total_weight`: Track any weight that your `spend_fund` implementation uses by updating this
///   value.
/// * `missed_any`: If there were items that you want to spend on, but there were not enough funds,
///   mark this value as `true`. This will prevent the treasury from burning the excess funds.
#[impl_trait_for_tuples::impl_for_tuples(30)]
pub trait SpendFunds<T: Config<I>, I: 'static = ()> {
	fn spend_funds(
		budget_remaining: &mut BalanceOf<T, I>,
		imbalance: &mut PositiveImbalanceOf<T, I>,
		total_weight: &mut Weight,
		missed_any: &mut bool,
	);
}

/// An index of a proposal. Just a `u32`.
pub type ProposalIndex = u32;

/// A spending proposal.
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
pub struct Proposal<AccountId, Balance> {
	/// The account proposing it.
	proposer: AccountId,
	/// The (total) amount that should be paid if the proposal is accepted.
	value: Balance,
	/// The account to whom the payment should be made if the proposal is accepted.
	beneficiary: AccountId,
	/// The amount held on deposit (reserved) for making this proposal.
	bond: Balance,
}

/// The state of the payment claim.
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
pub enum PaymentState<Id> {
	/// Pending claim.
	Pending,
	/// Payment attempted with a payment identifier.
	Attempted { id: Id },
	/// Payment failed.
	Failed,
}

/// Info regarding an approved treasury spend.
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
pub struct SpendStatus<AssetKind, AssetBalance, Beneficiary, BlockNumber, PaymentId> {
	// The kind of asset to be spent.
	asset_kind: AssetKind,
	/// The asset amount of the spend.
	amount: AssetBalance,
	/// The beneficiary of the spend.
	beneficiary: Beneficiary,
	/// The block number from which the spend can be claimed.
	valid_from: BlockNumber,
	/// The block number by which the spend has to be claimed.
	expire_at: BlockNumber,
	/// The status of the payout/claim.
	status: PaymentState<PaymentId>,
}

/// Index of an approved treasury spend.
pub type SpendIndex = u32;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		dispatch_context::with_context,
		pallet_prelude::*,
		traits::tokens::{ConversionFromAssetBalance, PaymentStatus},
	};
	use frame_system::pallet_prelude::{ensure_signed, OriginFor};

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		/// The staking balance.
		type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

		/// Origin from which rejections must come.
		type RejectOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The overarching event type.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Period between successive spends.
		#[pallet::constant]
		type SpendPeriod: Get<BlockNumberFor<Self, I>>;

		/// Percentage of spare funds (if any) that are burnt per spend period.
		#[pallet::constant]
		type Burn: Get<Permill>;

		/// The treasury's pallet id, used for deriving its sovereign account ID.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// Handler for the unbalanced decrease when treasury funds are burned.
		type BurnDestination: OnUnbalanced<NegativeImbalanceOf<Self, I>>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// Runtime hooks to external pallet using treasury to compute spend funds.
		type SpendFunds: SpendFunds<Self, I>;

		/// DEPRECATED: associated with `spend_local` call and will be removed in May 2025.
		/// Refer to <https://github.com/paritytech/polkadot-sdk/pull/5961> for migration to `spend`.
		///
		/// The maximum number of approvals that can wait in the spending queue.
		///
		/// NOTE: This parameter is also used within the Bounties Pallet extension if enabled.
		#[pallet::constant]
		type MaxApprovals: Get<u32>;

		/// The origin required for approving spends from the treasury outside of the proposal
		/// process. The `Success` value is the maximum amount in a native asset that this origin
		/// is allowed to spend at a time.
		type SpendOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = BalanceOf<Self, I>>;

		/// Type parameter representing the asset kinds to be spent from the treasury.
		type AssetKind: Parameter + MaxEncodedLen;

		/// Type parameter used to identify the beneficiaries eligible to receive treasury spends.
		type Beneficiary: Parameter + MaxEncodedLen;

		/// Converting trait to take a source type and convert to [`Self::Beneficiary`].
		type BeneficiaryLookup: StaticLookup<Target = Self::Beneficiary>;

		/// Type for processing spends of [Self::AssetKind] in favor of [`Self::Beneficiary`].
		type Paymaster: Pay<Beneficiary = Self::Beneficiary, AssetKind = Self::AssetKind>;

		/// Type for converting the balance of an [Self::AssetKind] to the balance of the native
		/// asset, solely for the purpose of asserting the result against the maximum allowed spend
		/// amount of the [`Self::SpendOrigin`].
		type BalanceConverter: ConversionFromAssetBalance<
			<Self::Paymaster as Pay>::Balance,
			Self::AssetKind,
			BalanceOf<Self, I>,
		>;

		/// The period during which an approved treasury spend has to be claimed.
		#[pallet::constant]
		type PayoutPeriod: Get<BlockNumberFor<Self, I>>;

		/// Helper type for benchmarks.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: ArgumentsFactory<Self::AssetKind, Self::Beneficiary>;

		/// Provider for the block number. Normally this is the `frame_system` pallet.
		type BlockNumberProvider: BlockNumberProvider;
	}

	/// DEPRECATED: associated with `spend_local` call and will be removed in May 2025.
	/// Refer to <https://github.com/paritytech/polkadot-sdk/pull/5961> for migration to `spend`.
	///
	/// Number of proposals that have been made.
	#[pallet::storage]
	pub type ProposalCount<T, I = ()> = StorageValue<_, ProposalIndex, ValueQuery>;

	/// DEPRECATED: associated with `spend_local` call and will be removed in May 2025.
	/// Refer to <https://github.com/paritytech/polkadot-sdk/pull/5961> for migration to `spend`.
	///
	/// Proposals that have been made.
	#[pallet::storage]
	pub type Proposals<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		ProposalIndex,
		Proposal<T::AccountId, BalanceOf<T, I>>,
		OptionQuery,
	>;

	/// The amount which has been reported as inactive to Currency.
	#[pallet::storage]
	pub type Deactivated<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BalanceOf<T, I>, ValueQuery>;

	/// DEPRECATED: associated with `spend_local` call and will be removed in May 2025.
	/// Refer to <https://github.com/paritytech/polkadot-sdk/pull/5961> for migration to `spend`.
	///
	/// Proposal indices that have been approved but not yet awarded.
	#[pallet::storage]
	pub type Approvals<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BoundedVec<ProposalIndex, T::MaxApprovals>, ValueQuery>;

	/// The count of spends that have been made.
	#[pallet::storage]
	pub(crate) type SpendCount<T, I = ()> = StorageValue<_, SpendIndex, ValueQuery>;

	/// Spends that have been approved and being processed.
	// Hasher: Twox safe since `SpendIndex` is an internal count based index.
	#[pallet::storage]
	pub type Spends<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		SpendIndex,
		SpendStatus<
			T::AssetKind,
			AssetBalanceOf<T, I>,
			T::Beneficiary,
			BlockNumberFor<T, I>,
			<T::Paymaster as Pay>::Id,
		>,
		OptionQuery,
	>;

	/// The blocknumber for the last triggered spend period.
	#[pallet::storage]
	pub(crate) type LastSpendPeriod<T, I = ()> = StorageValue<_, BlockNumberFor<T, I>, OptionQuery>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		#[serde(skip)]
		_config: core::marker::PhantomData<(T, I)>,
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> BuildGenesisConfig for GenesisConfig<T, I> {
		fn build(&self) {
			// Create Treasury account
			let account_id = Pallet::<T, I>::account_id();
			let min = T::Currency::minimum_balance();
			if T::Currency::free_balance(&account_id) < min {
				let _ = T::Currency::make_free_balance_be(&account_id, min);
			}
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// We have ended a spend period and will now allocate funds.
		Spending { budget_remaining: BalanceOf<T, I> },
		/// Some funds have been allocated.
		Awarded { proposal_index: ProposalIndex, award: BalanceOf<T, I>, account: T::AccountId },
		/// Some of our funds have been burnt.
		Burnt { burnt_funds: BalanceOf<T, I> },
		/// Spending has finished; this is the amount that rolls over until next spend.
		Rollover { rollover_balance: BalanceOf<T, I> },
		/// Some funds have been deposited.
		Deposit { value: BalanceOf<T, I> },
		/// A new spend proposal has been approved.
		SpendApproved {
			proposal_index: ProposalIndex,
			amount: BalanceOf<T, I>,
			beneficiary: T::AccountId,
		},
		/// The inactive funds of the pallet have been updated.
		UpdatedInactive { reactivated: BalanceOf<T, I>, deactivated: BalanceOf<T, I> },
		/// A new asset spend proposal has been approved.
		AssetSpendApproved {
			index: SpendIndex,
			asset_kind: T::AssetKind,
			amount: AssetBalanceOf<T, I>,
			beneficiary: T::Beneficiary,
			valid_from: BlockNumberFor<T, I>,
			expire_at: BlockNumberFor<T, I>,
		},
		/// An approved spend was voided.
		AssetSpendVoided { index: SpendIndex },
		/// A payment happened.
		Paid { index: SpendIndex, payment_id: <T::Paymaster as Pay>::Id },
		/// A payment failed and can be retried.
		PaymentFailed { index: SpendIndex, payment_id: <T::Paymaster as Pay>::Id },
		/// A spend was processed and removed from the storage. It might have been successfully
		/// paid or it may have expired.
		SpendProcessed { index: SpendIndex },
	}

	/// Error for the treasury pallet.
	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// No proposal, bounty or spend at that index.
		InvalidIndex,
		/// Too many approvals in the queue.
		TooManyApprovals,
		/// The spend origin is valid but the amount it is allowed to spend is lower than the
		/// amount to be spent.
		InsufficientPermission,
		/// Proposal has not been approved.
		ProposalNotApproved,
		/// The balance of the asset kind is not convertible to the balance of the native asset.
		FailedToConvertBalance,
		/// The spend has expired and cannot be claimed.
		SpendExpired,
		/// The spend is not yet eligible for payout.
		EarlyPayout,
		/// The payment has already been attempted.
		AlreadyAttempted,
		/// There was some issue with the mechanism of payment.
		PayoutError,
		/// The payout was not yet attempted/claimed.
		NotAttempted,
		/// The payment has neither failed nor succeeded yet.
		Inconclusive,
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<SystemBlockNumberFor<T>> for Pallet<T, I> {
		/// ## Complexity
		/// - `O(A)` where `A` is the number of approvals
		fn on_initialize(_do_not_use_local_block_number: SystemBlockNumberFor<T>) -> Weight {
			let block_number = T::BlockNumberProvider::current_block_number();
			let pot = Self::pot();
			let deactivated = Deactivated::<T, I>::get();
			if pot != deactivated {
				T::Currency::reactivate(deactivated);
				T::Currency::deactivate(pot);
				Deactivated::<T, I>::put(&pot);
				Self::deposit_event(Event::<T, I>::UpdatedInactive {
					reactivated: deactivated,
					deactivated: pot,
				});
			}

			// Check to see if we should spend some funds!
			let last_spend_period = LastSpendPeriod::<T, I>::get()
				// This unwrap should only occur one time on any blockchain.
				// `update_last_spend_period` will populate the `LastSpendPeriod` storage if it is
				// empty.
				.unwrap_or_else(|| Self::update_last_spend_period());
			let blocks_since_last_spend_period = block_number.saturating_sub(last_spend_period);
			let safe_spend_period = T::SpendPeriod::get().max(BlockNumberFor::<T, I>::one());

			// Safe because of `max(1)` above.
			let (spend_periods_passed, extra_blocks) = (
				blocks_since_last_spend_period / safe_spend_period,
				blocks_since_last_spend_period % safe_spend_period,
			);
			let new_last_spend_period = block_number.saturating_sub(extra_blocks);
			if spend_periods_passed > BlockNumberFor::<T, I>::zero() {
				Self::spend_funds(spend_periods_passed, new_last_spend_period)
			} else {
				Weight::zero()
			}
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(_: SystemBlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()?;
			Ok(())
		}
	}

	#[derive(Default)]
	struct SpendContext<Balance> {
		spend_in_context: BTreeMap<Balance, Balance>,
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Propose and approve a spend of treasury funds.
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::SpendOrigin`] with the `Success` value being at least `amount`.
		///
		/// ### Details
		/// NOTE: For record-keeping purposes, the proposer is deemed to be equivalent to the
		/// beneficiary.
		///
		/// ### Parameters
		/// - `amount`: The amount to be transferred from the treasury to the `beneficiary`.
		/// - `beneficiary`: The destination account for the transfer.
		///
		/// ## Events
		///
		/// Emits [`Event::SpendApproved`] if successful.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::spend_local())]
		#[deprecated(
			note = "The `spend_local` call will be removed by May 2025. Migrate to the new flow and use the `spend` call."
		)]
		#[allow(deprecated)]
		pub fn spend_local(
			origin: OriginFor<T>,
			#[pallet::compact] amount: BalanceOf<T, I>,
			beneficiary: AccountIdLookupOf<T>,
		) -> DispatchResult {
			let max_amount = T::SpendOrigin::ensure_origin(origin)?;
			ensure!(amount <= max_amount, Error::<T, I>::InsufficientPermission);

			with_context::<SpendContext<BalanceOf<T, I>>, _>(|v| {
				let context = v.or_default();

				// We group based on `max_amount`, to distinguish between different kind of
				// origins. (assumes that all origins have different `max_amount`)
				//
				// Worst case is that we reject some "valid" request.
				let spend = context.spend_in_context.entry(max_amount).or_default();

				// Ensure that we don't overflow nor use more than `max_amount`
				if spend.checked_add(&amount).map(|s| s > max_amount).unwrap_or(true) {
					Err(Error::<T, I>::InsufficientPermission)
				} else {
					*spend = spend.saturating_add(amount);

					Ok(())
				}
			})
			.unwrap_or(Ok(()))?;

			let beneficiary = T::Lookup::lookup(beneficiary)?;
			#[allow(deprecated)]
			let proposal_index = ProposalCount::<T, I>::get();
			#[allow(deprecated)]
			Approvals::<T, I>::try_append(proposal_index)
				.map_err(|_| Error::<T, I>::TooManyApprovals)?;
			let proposal = Proposal {
				proposer: beneficiary.clone(),
				value: amount,
				beneficiary: beneficiary.clone(),
				bond: Default::default(),
			};
			#[allow(deprecated)]
			Proposals::<T, I>::insert(proposal_index, proposal);
			#[allow(deprecated)]
			ProposalCount::<T, I>::put(proposal_index + 1);

			Self::deposit_event(Event::SpendApproved { proposal_index, amount, beneficiary });
			Ok(())
		}

		/// Force a previously approved proposal to be removed from the approval queue.
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::RejectOrigin`].
		///
		/// ## Details
		///
		/// The original deposit will no longer be returned.
		///
		/// ### Parameters
		/// - `proposal_id`: The index of a proposal
		///
		/// ### Complexity
		/// - O(A) where `A` is the number of approvals
		///
		/// ### Errors
		/// - [`Error::ProposalNotApproved`]: The `proposal_id` supplied was not found in the
		///   approval queue, i.e., the proposal has not been approved. This could also mean the
		///   proposal does not exist altogether, thus there is no way it would have been approved
		///   in the first place.
		#[pallet::call_index(4)]
		#[pallet::weight((T::WeightInfo::remove_approval(), DispatchClass::Operational))]
		#[deprecated(
			note = "The `remove_approval` call will be removed by May 2025. It associated with the deprecated `spend_local` call."
		)]
		#[allow(deprecated)]
		pub fn remove_approval(
			origin: OriginFor<T>,
			#[pallet::compact] proposal_id: ProposalIndex,
		) -> DispatchResult {
			T::RejectOrigin::ensure_origin(origin)?;

			#[allow(deprecated)]
			Approvals::<T, I>::try_mutate(|v| -> DispatchResult {
				if let Some(index) = v.iter().position(|x| x == &proposal_id) {
					v.remove(index);
					Ok(())
				} else {
					Err(Error::<T, I>::ProposalNotApproved.into())
				}
			})?;

			Ok(())
		}

		/// Propose and approve a spend of treasury funds.
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::SpendOrigin`] with the `Success` value being at least
		/// `amount` of `asset_kind` in the native asset. The amount of `asset_kind` is converted
		/// for assertion using the [`Config::BalanceConverter`].
		///
		/// ## Details
		///
		/// Create an approved spend for transferring a specific `amount` of `asset_kind` to a
		/// designated beneficiary. The spend must be claimed using the `payout` dispatchable within
		/// the [`Config::PayoutPeriod`].
		///
		/// ### Parameters
		/// - `asset_kind`: An indicator of the specific asset class to be spent.
		/// - `amount`: The amount to be transferred from the treasury to the `beneficiary`.
		/// - `beneficiary`: The beneficiary of the spend.
		/// - `valid_from`: The block number from which the spend can be claimed. It can refer to
		///   the past if the resulting spend has not yet expired according to the
		///   [`Config::PayoutPeriod`]. If `None`, the spend can be claimed immediately after
		///   approval.
		///
		/// ## Events
		///
		/// Emits [`Event::AssetSpendApproved`] if successful.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::spend())]
		pub fn spend(
			origin: OriginFor<T>,
			asset_kind: Box<T::AssetKind>,
			#[pallet::compact] amount: AssetBalanceOf<T, I>,
			beneficiary: Box<BeneficiaryLookupOf<T, I>>,
			valid_from: Option<BlockNumberFor<T, I>>,
		) -> DispatchResult {
			let max_amount = T::SpendOrigin::ensure_origin(origin)?;
			let beneficiary = T::BeneficiaryLookup::lookup(*beneficiary)?;

			let now = T::BlockNumberProvider::current_block_number();
			let valid_from = valid_from.unwrap_or(now);
			let expire_at = valid_from.saturating_add(T::PayoutPeriod::get());
			ensure!(expire_at > now, Error::<T, I>::SpendExpired);

			let native_amount =
				T::BalanceConverter::from_asset_balance(amount, *asset_kind.clone())
					.map_err(|_| Error::<T, I>::FailedToConvertBalance)?;

			ensure!(native_amount <= max_amount, Error::<T, I>::InsufficientPermission);

			with_context::<SpendContext<BalanceOf<T, I>>, _>(|v| {
				let context = v.or_default();
				// We group based on `max_amount`, to distinguish between different kind of
				// origins. (assumes that all origins have different `max_amount`)
				//
				// Worst case is that we reject some "valid" request.
				let spend = context.spend_in_context.entry(max_amount).or_default();

				// Ensure that we don't overflow nor use more than `max_amount`
				if spend.checked_add(&native_amount).map(|s| s > max_amount).unwrap_or(true) {
					Err(Error::<T, I>::InsufficientPermission)
				} else {
					*spend = spend.saturating_add(native_amount);
					Ok(())
				}
			})
			.unwrap_or(Ok(()))?;

			let index = SpendCount::<T, I>::get();
			Spends::<T, I>::insert(
				index,
				SpendStatus {
					asset_kind: *asset_kind.clone(),
					amount,
					beneficiary: beneficiary.clone(),
					valid_from,
					expire_at,
					status: PaymentState::Pending,
				},
			);
			SpendCount::<T, I>::put(index + 1);

			Self::deposit_event(Event::AssetSpendApproved {
				index,
				asset_kind: *asset_kind,
				amount,
				beneficiary,
				valid_from,
				expire_at,
			});
			Ok(())
		}

		/// Claim a spend.
		///
		/// ## Dispatch Origin
		///
		/// Must be signed
		///
		/// ## Details
		///
		/// Spends must be claimed within some temporal bounds. A spend may be claimed within one
		/// [`Config::PayoutPeriod`] from the `valid_from` block.
		/// In case of a payout failure, the spend status must be updated with the `check_status`
		/// dispatchable before retrying with the current function.
		///
		/// ### Parameters
		/// - `index`: The spend index.
		///
		/// ## Events
		///
		/// Emits [`Event::Paid`] if successful.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::payout())]
		pub fn payout(origin: OriginFor<T>, index: SpendIndex) -> DispatchResult {
			ensure_signed(origin)?;
			let mut spend = Spends::<T, I>::get(index).ok_or(Error::<T, I>::InvalidIndex)?;
			let now = T::BlockNumberProvider::current_block_number();
			ensure!(now >= spend.valid_from, Error::<T, I>::EarlyPayout);
			ensure!(spend.expire_at > now, Error::<T, I>::SpendExpired);
			ensure!(
				matches!(spend.status, PaymentState::Pending | PaymentState::Failed),
				Error::<T, I>::AlreadyAttempted
			);

			let id = T::Paymaster::pay(&spend.beneficiary, spend.asset_kind.clone(), spend.amount)
				.map_err(|_| Error::<T, I>::PayoutError)?;

			spend.status = PaymentState::Attempted { id };
			Spends::<T, I>::insert(index, spend);

			Self::deposit_event(Event::<T, I>::Paid { index, payment_id: id });

			Ok(())
		}

		/// Check the status of the spend and remove it from the storage if processed.
		///
		/// ## Dispatch Origin
		///
		/// Must be signed.
		///
		/// ## Details
		///
		/// The status check is a prerequisite for retrying a failed payout.
		/// If a spend has either succeeded or expired, it is removed from the storage by this
		/// function. In such instances, transaction fees are refunded.
		///
		/// ### Parameters
		/// - `index`: The spend index.
		///
		/// ## Events
		///
		/// Emits [`Event::PaymentFailed`] if the spend payout has failed.
		/// Emits [`Event::SpendProcessed`] if the spend payout has succeed.
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::check_status())]
		pub fn check_status(origin: OriginFor<T>, index: SpendIndex) -> DispatchResultWithPostInfo {
			use PaymentState as State;
			use PaymentStatus as Status;

			ensure_signed(origin)?;
			let mut spend = Spends::<T, I>::get(index).ok_or(Error::<T, I>::InvalidIndex)?;
			let now = T::BlockNumberProvider::current_block_number();

			if now > spend.expire_at && !matches!(spend.status, State::Attempted { .. }) {
				// spend has expired and no further status update is expected.
				Spends::<T, I>::remove(index);
				Self::deposit_event(Event::<T, I>::SpendProcessed { index });
				return Ok(Pays::No.into())
			}

			let payment_id = match spend.status {
				State::Attempted { id } => id,
				_ => return Err(Error::<T, I>::NotAttempted.into()),
			};

			match T::Paymaster::check_payment(payment_id) {
				Status::Failure => {
					spend.status = PaymentState::Failed;
					Spends::<T, I>::insert(index, spend);
					Self::deposit_event(Event::<T, I>::PaymentFailed { index, payment_id });
				},
				Status::Success | Status::Unknown => {
					Spends::<T, I>::remove(index);
					Self::deposit_event(Event::<T, I>::SpendProcessed { index });
					return Ok(Pays::No.into())
				},
				Status::InProgress => return Err(Error::<T, I>::Inconclusive.into()),
			}
			return Ok(Pays::Yes.into())
		}

		/// Void previously approved spend.
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::RejectOrigin`].
		///
		/// ## Details
		///
		/// A spend void is only possible if the payout has not been attempted yet.
		///
		/// ### Parameters
		/// - `index`: The spend index.
		///
		/// ## Events
		///
		/// Emits [`Event::AssetSpendVoided`] if successful.
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::void_spend())]
		pub fn void_spend(origin: OriginFor<T>, index: SpendIndex) -> DispatchResult {
			T::RejectOrigin::ensure_origin(origin)?;
			let spend = Spends::<T, I>::get(index).ok_or(Error::<T, I>::InvalidIndex)?;
			ensure!(
				matches!(spend.status, PaymentState::Pending | PaymentState::Failed),
				Error::<T, I>::AlreadyAttempted
			);

			Spends::<T, I>::remove(index);
			Self::deposit_event(Event::<T, I>::AssetSpendVoided { index });
			Ok(())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	// Add public immutables and private mutables.

	/// The account ID of the treasury pot.
	///
	/// This actually does computation. If you need to keep using it, then make sure you cache the
	/// value and only call this once.
	pub fn account_id() -> T::AccountId {
		T::PalletId::get().into_account_truncating()
	}

	// Backfill the `LastSpendPeriod` storage, assuming that no configuration has changed
	// since introducing this code. Used specifically for a migration-less switch to populate
	// `LastSpendPeriod`.
	fn update_last_spend_period() -> BlockNumberFor<T, I> {
		let block_number = T::BlockNumberProvider::current_block_number();
		let spend_period = T::SpendPeriod::get().max(BlockNumberFor::<T, I>::one());
		let time_since_last_spend = block_number % spend_period;
		// If it happens that this logic runs directly on a spend period block, we need to backdate
		// to the last spend period so a spend still occurs this block.
		let last_spend_period = if time_since_last_spend.is_zero() {
			block_number.saturating_sub(spend_period)
		} else {
			// Otherwise, this is the last time we had a spend period.
			block_number.saturating_sub(time_since_last_spend)
		};
		LastSpendPeriod::<T, I>::put(last_spend_period);
		last_spend_period
	}

	/// Public function to proposal_count storage.
	#[deprecated(
		note = "This function will be removed by May 2025. Configure pallet to use PayFromAccount for Paymaster type instead"
	)]
	pub fn proposal_count() -> ProposalIndex {
		#[allow(deprecated)]
		ProposalCount::<T, I>::get()
	}

	/// Public function to proposals storage.
	#[deprecated(
		note = "This function will be removed by May 2025. Configure pallet to use PayFromAccount for Paymaster type instead"
	)]
	pub fn proposals(index: ProposalIndex) -> Option<Proposal<T::AccountId, BalanceOf<T, I>>> {
		#[allow(deprecated)]
		Proposals::<T, I>::get(index)
	}

	/// Public function to approvals storage.
	#[deprecated(
		note = "This function will be removed by May 2025. Configure pallet to use PayFromAccount for Paymaster type instead"
	)]
	#[allow(deprecated)]
	pub fn approvals() -> BoundedVec<ProposalIndex, T::MaxApprovals> {
		Approvals::<T, I>::get()
	}

	/// Spend some money! returns number of approvals before spend.
	pub fn spend_funds(
		spend_periods_passed: BlockNumberFor<T, I>,
		new_last_spend_period: BlockNumberFor<T, I>,
	) -> Weight {
		LastSpendPeriod::<T, I>::put(new_last_spend_period);
		let mut total_weight = Weight::zero();

		let mut budget_remaining = Self::pot();
		Self::deposit_event(Event::Spending { budget_remaining });
		let account_id = Self::account_id();

		let mut missed_any = false;
		let mut imbalance = PositiveImbalanceOf::<T, I>::zero();
		#[allow(deprecated)]
		let proposals_len = Approvals::<T, I>::mutate(|v| {
			let proposals_approvals_len = v.len() as u32;
			v.retain(|&index| {
				// Should always be true, but shouldn't panic if false or we're screwed.
				if let Some(p) = Proposals::<T, I>::get(index) {
					if p.value <= budget_remaining {
						budget_remaining -= p.value;
						Proposals::<T, I>::remove(index);

						// return their deposit.
						let err_amount = T::Currency::unreserve(&p.proposer, p.bond);
						debug_assert!(err_amount.is_zero());

						// provide the allocation.
						imbalance.subsume(T::Currency::deposit_creating(&p.beneficiary, p.value));

						Self::deposit_event(Event::Awarded {
							proposal_index: index,
							award: p.value,
							account: p.beneficiary,
						});
						false
					} else {
						missed_any = true;
						true
					}
				} else {
					false
				}
			});
			proposals_approvals_len
		});

		total_weight += T::WeightInfo::on_initialize_proposals(proposals_len);

		// Call Runtime hooks to external pallet using treasury to compute spend funds.
		T::SpendFunds::spend_funds(
			&mut budget_remaining,
			&mut imbalance,
			&mut total_weight,
			&mut missed_any,
		);

		if !missed_any && !T::Burn::get().is_zero() {
			// Get the amount of treasury that should be left after potentially multiple spend
			// periods have passed.
			let one_minus_burn = T::Burn::get().left_from_one();
			let percent_left =
				one_minus_burn.saturating_pow(spend_periods_passed.unique_saturated_into());
			let new_budget_remaining = percent_left * budget_remaining;
			let burn = budget_remaining.saturating_sub(new_budget_remaining);
			budget_remaining = new_budget_remaining;

			let (debit, credit) = T::Currency::pair(burn);
			imbalance.subsume(debit);
			T::BurnDestination::on_unbalanced(credit);
			Self::deposit_event(Event::Burnt { burnt_funds: burn })
		}

		// Must never be an error, but better to be safe.
		// proof: budget_remaining is account free balance minus ED;
		// Thus we can't spend more than account free balance minus ED;
		// Thus account is kept alive; qed;
		if let Err(problem) =
			T::Currency::settle(&account_id, imbalance, WithdrawReasons::TRANSFER, KeepAlive)
		{
			print("Inconsistent state - couldn't settle imbalance for funds spent by treasury");
			// Nothing else to do here.
			drop(problem);
		}

		Self::deposit_event(Event::Rollover { rollover_balance: budget_remaining });

		total_weight
	}

	/// Return the amount of money in the pot.
	// The existential deposit is not part of the pot so treasury account never gets deleted.
	pub fn pot() -> BalanceOf<T, I> {
		T::Currency::free_balance(&Self::account_id())
			// Must never be less than 0 but better be safe.
			.saturating_sub(T::Currency::minimum_balance())
	}

	/// Ensure the correctness of the state of this pallet.
	#[cfg(any(feature = "try-runtime", test))]
	fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		Self::try_state_proposals()?;
		Self::try_state_spends()?;

		Ok(())
	}

	/// ### Invariants of proposal storage items
	///
	/// 1. [`ProposalCount`] >= Number of elements in [`Proposals`].
	/// 2. Each entry in [`Proposals`] should be saved under a key strictly less than current
	/// [`ProposalCount`].
	/// 3. Each [`ProposalIndex`] contained in [`Approvals`] should exist in [`Proposals`].
	/// Note, that this automatically implies [`Approvals`].count() <= [`Proposals`].count().
	#[cfg(any(feature = "try-runtime", test))]
	fn try_state_proposals() -> Result<(), sp_runtime::TryRuntimeError> {
		let current_proposal_count = ProposalCount::<T, I>::get();
		ensure!(
			current_proposal_count as usize >= Proposals::<T, I>::iter().count(),
			"Actual number of proposals exceeds `ProposalCount`."
		);

		Proposals::<T, I>::iter_keys().try_for_each(|proposal_index| -> DispatchResult {
			ensure!(
				current_proposal_count as u32 > proposal_index,
				"`ProposalCount` should by strictly greater than any ProposalIndex used as a key for `Proposals`."
			);
			Ok(())
		})?;

		Approvals::<T, I>::get()
			.iter()
			.try_for_each(|proposal_index| -> DispatchResult {
				ensure!(
					Proposals::<T, I>::contains_key(proposal_index),
					"Proposal indices in `Approvals` must also be contained in `Proposals`."
				);
				Ok(())
			})?;

		Ok(())
	}

	/// ## Invariants of spend storage items
	///
	/// 1. [`SpendCount`] >= Number of elements in [`Spends`].
	/// 2. Each entry in [`Spends`] should be saved under a key strictly less than current
	/// [`SpendCount`].
	/// 3. For each spend entry contained in [`Spends`] we should have spend.expire_at
	/// > spend.valid_from.
	#[cfg(any(feature = "try-runtime", test))]
	fn try_state_spends() -> Result<(), sp_runtime::TryRuntimeError> {
		let current_spend_count = SpendCount::<T, I>::get();
		ensure!(
			current_spend_count as usize >= Spends::<T, I>::iter().count(),
			"Actual number of spends exceeds `SpendCount`."
		);

		Spends::<T, I>::iter_keys().try_for_each(|spend_index| -> DispatchResult {
			ensure!(
				current_spend_count > spend_index,
				"`SpendCount` should by strictly greater than any SpendIndex used as a key for `Spends`."
			);
			Ok(())
		})?;

		Spends::<T, I>::iter().try_for_each(|(_index, spend)| -> DispatchResult {
			ensure!(
				spend.valid_from < spend.expire_at,
				"Spend cannot expire before it becomes valid."
			);
			Ok(())
		})?;

		Ok(())
	}
}

impl<T: Config<I>, I: 'static> OnUnbalanced<NegativeImbalanceOf<T, I>> for Pallet<T, I> {
	fn on_nonzero_unbalanced(amount: NegativeImbalanceOf<T, I>) {
		let numeric_amount = amount.peek();

		// Must resolve into existing but better to be safe.
		let _ = T::Currency::resolve_creating(&Self::account_id(), amount);

		Self::deposit_event(Event::Deposit { value: numeric_amount });
	}
}

/// TypedGet implementation to get the AccountId of the Treasury.
pub struct TreasuryAccountId<R>(PhantomData<R>);
impl<R> sp_runtime::traits::TypedGet for TreasuryAccountId<R>
where
	R: crate::Config,
{
	type Type = <R as frame_system::Config>::AccountId;
	fn get() -> Self::Type {
		crate::Pallet::<R>::account_id()
	}
}
