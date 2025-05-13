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

//! The operational pallet for the Asset Hub, designed to manage and facilitate the migration of
//! subsystems such as Governance, Staking, Balances from the Relay Chain to the Asset Hub. This
//! pallet works alongside its counterpart, `pallet_rc_migrator`, which handles migration
//! processes on the Relay Chain side.
//!
//! This pallet is responsible for controlling the initiation, progression, and completion of the
//! migration process, including managing its various stages and transferring the necessary data.
//! The pallet directly accesses the storage of other pallets for read/write operations while
//! maintaining compatibility with their existing APIs.
//!
//! To simplify development and avoid the need to edit the original pallets, this pallet may
//! duplicate private items such as storage entries from the original pallets. This ensures that the
//! migration logic can be implemented without altering the original implementations.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
pub mod weights;

pub use pallet::*;
pub use weights::WeightInfo;

use cumulus_primitives_core::ParaId;
use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible::{InspectFreeze, Mutate, MutateFreeze, MutateHold, Unbalanced},
		tokens::Preservation,
		Defensive, LockableCurrency, ReservableCurrency,
	},
};
use frame_system::pallet_prelude::*;
use pallet_balances::AccountData;
use sp_runtime::{traits::BlockNumberProvider, AccountId32};
use sp_std::prelude::*;

/// The log target of this pallet.
pub const LOG_TARGET: &str = "runtime::ah-migrator";

pub type BalanceOf<T> = <T as pallet_balances::Config>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config:
		frame_system::Config<AccountData = AccountData<u128>, AccountId = AccountId32>
		+ pallet_balances::Config<Balance = u128>
		+ pallet_timestamp::Config<Moment = u64> // Needed for testing
	{
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Native asset type.
		type Currency: Mutate<Self::AccountId, Balance = u128>
			+ MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>
			+ InspectFreeze<Self::AccountId, Id = Self::FreezeIdentifier>
			+ MutateFreeze<Self::AccountId>
			+ Unbalanced<Self::AccountId>
			+ ReservableCurrency<Self::AccountId, Balance = u128>
			+ LockableCurrency<Self::AccountId, Balance = u128>;

		/// Access the block number of the Relay Chain.
		type RcBlockNumberProvider: BlockNumberProvider<BlockNumber = BlockNumberFor<Self>>;

		/// The Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	/// Amount of balance that was reserved for winning a lease auction.
	///
	/// `unreserve_lease_deposit` can be permissionlessly called once the block number passed to
	/// unreserve the deposit. It is implicitly called by `withdraw_crowdloan_contribution`.
	///  
	/// The account here can either be a crowdloan account or a solo bidder. If it is a crowdloan
	/// account, then the summed up contributions for it in the contributions map will equate the
	/// reserved balance here.
	///
	/// The keys are as follows:
	/// - Block number after which the deposit can be unreserved.
	/// - The para_id of the lease slot.
	/// - The account that will have the balance unreserved.
	/// - The balance to be unreserved.
	#[pallet::storage]
	pub type RcLeaseReserve<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Twox64Concat, BlockNumberFor<T>>,
			NMapKey<Twox64Concat, ParaId>,
			NMapKey<Twox64Concat, T::AccountId>,
		),
		BalanceOf<T>,
		OptionQuery,
	>;

	/// Amount of balance that a contributor made towards a crowdloan.
	///
	/// `withdraw_crowdloan_contribution` can be permissionlessly called once the block number
	/// passed to unlock the balance for a specific account.
	///
	/// The keys are as follows:
	/// - Block number after which the balance can be unlocked.
	/// - The para_id of the crowdloan.
	/// - The account that made the contribution.
	///
	/// The value is (fund_pot, balance). The contribution pot is the second key in the
	/// `RcCrowdloanContribution` storage.
	#[pallet::storage]
	pub type RcCrowdloanContribution<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Twox64Concat, BlockNumberFor<T>>,
			NMapKey<Twox64Concat, ParaId>,
			NMapKey<Twox64Concat, T::AccountId>,
		),
		(T::AccountId, BalanceOf<T>),
		OptionQuery,
	>;

	/// The reserve that was taken to create a crowdloan.
	///
	/// This is normally 500 DOT and can be refunded as last step after all
	/// `RcCrowdloanContribution`s of this loan have been withdrawn.
	///
	/// Keys:
	/// - Block number after which this can be unreserved
	/// - The para_id of the crowdloan
	/// - The account that will have the balance unreserved
	#[pallet::storage]
	pub type RcCrowdloanReserve<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Twox64Concat, BlockNumberFor<T>>,
			NMapKey<Twox64Concat, ParaId>,
			NMapKey<Twox64Concat, T::AccountId>,
		),
		BalanceOf<T>,
		OptionQuery,
	>;

	#[pallet::error]
	pub enum Error<T> {
		/// Either no lease deposit or already unreserved.
		NoLeaseReserve,
		/// Either no crowdloan contribution or already withdrawn.
		NoCrowdloanContribution,
		/// Either no crowdloan reserve or already unreserved.
		NoCrowdloanReserve,
		/// Failed to withdraw crowdloan contribution.
		FailedToWithdrawCrowdloanContribution,
		/// Block number is not yet reached.
		NotYet,
		/// Not all contributions are withdrawn.
		ContributionsRemaining,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Some lease reserve could not be unreserved and needs manual cleanup.
		LeaseUnreserveRemaining {
			depositor: T::AccountId,
			para_id: ParaId,
			remaining: BalanceOf<T>,
		},

		/// Some amount for a crowdloan reserve could not be unreserved and needs manual cleanup.
		CrowdloanUnreserveRemaining {
			depositor: T::AccountId,
			para_id: ParaId,
			remaining: BalanceOf<T>,
		},
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Unreserve the deposit that was taken for creating a crowdloan.
		///
		/// This can be called by any signed origin. It unreserves the lease deposit on the account
		/// that won the lease auction. It can be unreserved once all leases expired. Note that it
		/// will be called automatically from `withdraw_crowdloan_contribution` for the matching
		/// crowdloan account.
		///
		/// Solo bidder accounts that won lease auctions can use this to unreserve their amount.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::unreserve_lease_deposit())]
		pub fn unreserve_lease_deposit(
			origin: OriginFor<T>,
			block: BlockNumberFor<T>,
			depositor: Option<T::AccountId>,
			para_id: ParaId,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			let depositor = depositor.unwrap_or(sender);

			Self::do_unreserve_lease_deposit(block, depositor, para_id).map_err(Into::into)
		}

		/// Withdraw the contribution of a finished crowdloan.
		///
		/// A crowdloan contribution can be withdrawn if either:
		/// - The crowdloan failed to in an auction and timed out
		/// - Won an auction and all leases expired
		///
		/// Can be called by any signed origin.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::withdraw_crowdloan_contribution())]
		pub fn withdraw_crowdloan_contribution(
			origin: OriginFor<T>,
			block: BlockNumberFor<T>,
			depositor: Option<T::AccountId>,
			para_id: ParaId,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			let depositor = depositor.unwrap_or(sender);

			Self::do_withdraw_crowdloan_contribution(block, depositor, para_id).map_err(Into::into)
		}

		/// Unreserve the deposit that was taken for creating a crowdloan.
		///
		/// This can be called once either:
		/// - The crowdloan failed to win an auction and timed out
		/// - Won an auction, all leases expired and all contributions are withdrawn
		///
		/// Can be called by any signed origin. The condition that all contributions are withdrawn
		/// is in place since the reserve acts as a storage deposit.
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::unreserve_crowdloan_reserve())]
		pub fn unreserve_crowdloan_reserve(
			origin: OriginFor<T>,
			block: BlockNumberFor<T>,
			depositor: Option<T::AccountId>,
			para_id: ParaId,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			let depositor = depositor.unwrap_or(sender);

			Self::do_unreserve_crowdloan_reserve(block, depositor, para_id).map_err(Into::into)
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn do_unreserve_lease_deposit(
			block: BlockNumberFor<T>,
			depositor: T::AccountId,
			para_id: ParaId,
		) -> Result<(), Error<T>> {
			ensure!(block <= T::RcBlockNumberProvider::current_block_number(), Error::<T>::NotYet);
			let balance = RcLeaseReserve::<T>::take((block, para_id, &depositor))
				.ok_or(Error::<T>::NoLeaseReserve)?;

			let remaining = <T as Config>::Currency::unreserve(&depositor, balance);
			if remaining > 0 {
				defensive!("Should be able to unreserve all");
				Self::deposit_event(Event::LeaseUnreserveRemaining {
					depositor,
					remaining,
					para_id,
				});
			}

			Ok(())
		}

		pub fn do_withdraw_crowdloan_contribution(
			block: BlockNumberFor<T>,
			depositor: T::AccountId,
			para_id: ParaId,
		) -> Result<(), Error<T>> {
			ensure!(block <= T::RcBlockNumberProvider::current_block_number(), Error::<T>::NotYet);
			let (pot, contribution) =
				RcCrowdloanContribution::<T>::take((block, para_id, &depositor))
					.ok_or(Error::<T>::NoCrowdloanContribution)?;

			// Maybe this is the first one to withdraw and we need to unreserve it from the pot
			match Self::do_unreserve_lease_deposit(block, pot.clone(), para_id) {
				Ok(()) => (),
				Err(Error::<T>::NoLeaseReserve) => (), // fine
				Err(e) => return Err(e),
			}

			// Ideally this does not fail. But if it does, then we keep it for manual inspection.
			let transferred = <T as Config>::Currency::transfer(
				&pot,
				&depositor,
				contribution,
				Preservation::Preserve,
			)
			.defensive()
			.map_err(|_| Error::<T>::FailedToWithdrawCrowdloanContribution)?;
			defensive_assert!(transferred == contribution);
			// Need to reactivate since we deactivated it here https://github.com/paritytech/polkadot-sdk/blob/04847d515ef56da4d0801c9b89a4241dfa827b33/polkadot/runtime/common/src/crowdloan/mod.rs#L793
			<T as Config>::Currency::reactivate(transferred);

			Ok(())
		}

		pub fn do_unreserve_crowdloan_reserve(
			block: BlockNumberFor<T>,
			depositor: T::AccountId,
			para_id: ParaId,
		) -> Result<(), Error<T>> {
			ensure!(block <= T::RcBlockNumberProvider::current_block_number(), Error::<T>::NotYet);
			ensure!(
				Self::contributions_withdrawn(block, para_id),
				Error::<T>::ContributionsRemaining
			);
			let amount = RcCrowdloanReserve::<T>::take((block, para_id, &depositor))
				.ok_or(Error::<T>::NoCrowdloanReserve)?;

			let remaining = <T as Config>::Currency::unreserve(&depositor, amount);
			if remaining > 0 {
				defensive!("Should be able to unreserve all");
				Self::deposit_event(Event::CrowdloanUnreserveRemaining {
					depositor,
					remaining,
					para_id,
				});
			}

			Ok(())
		}

		// TODO Test this
		fn contributions_withdrawn(block: BlockNumberFor<T>, para_id: ParaId) -> bool {
			let mut contrib_iter = RcCrowdloanContribution::<T>::iter_prefix((block, para_id));
			contrib_iter.next().is_none()
		}
	}
}
