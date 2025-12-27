// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;

pub use pallet::*;
pub use weights::WeightInfo;

use codec::DecodeAll;
use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible::{Inspect, InspectFreeze, Mutate, MutateFreeze, MutateHold, Unbalanced},
		tokens::{Fortitude, IdAmount, Precision, Preservation},
		Defensive, LockableCurrency, ReservableCurrency, WithdrawReasons as LockWithdrawReasons,
	},
};
use frame_system::pallet_prelude::*;
use pallet_balances::{AccountData, BalanceLock, Reasons as LockReasons};
use sp_application_crypto::ByteArray;
use sp_runtime::{traits::BlockNumberProvider, AccountId32};
use sp_std::prelude::*;

/// The log target of this pallet.
pub const LOG_TARGET: &str = "runtime::ah-ops";

pub type BalanceOf<T> = <T as pallet_balances::Config>::Balance;
pub type DerivationIndex = u16;
pub type ParaId = u16;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config:
		frame_system::Config<AccountData = AccountData<u128>, AccountId = AccountId32>
		+ pallet_balances::Config<Balance = u128>
		+ pallet_timestamp::Config<Moment = u64> // Needed for testing
	{
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
		/// Passed account IDs are not matching unmigrated child and sibling accounts.
		WrongSovereignTranslation,
		/// The account is not a derived account.
		WrongDerivedTranslation,
		/// Account cannot be migrated since it is not a sovereign parachain account.
		NotSovereign,
		/// Internal error, please bug report.
		InternalError,
		/// The migrated account would get reaped in the process.
		WouldReap,
		/// Failed to put a hold on an account.
		FailedToPutHold,
		/// Failed to release a hold from an account.
		FailedToReleaseHold,
		/// Failed to thaw a frozen balance.
		FailedToThaw,
		/// Failed to set a freeze on an account.
		FailedToSetFreeze,
		/// Failed to transfer a balance.
		FailedToTransfer,
		/// Failed to reserve a balance.
		FailedToReserve,
		/// Failed to unreserve the full balance.
		CannotUnreserve,
		/// The from and to accounts are identical.
		AccountIdentical,
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

		/// A sovereign parachain account has been migrated from its child to sibling
		/// representation.
		SovereignMigrated {
			/// The parachain ID that had its account migrated.
			para_id: ParaId,
			/// The old account that was migrated out of.
			from: T::AccountId,
			/// The new account that was migrated into.
			to: T::AccountId,
			/// Set if this account was derived from a para sovereign account.
			derivation_index: Option<DerivationIndex>,
		},

		/// An amount of fungible balance was put on hold.
		HoldPlaced { account: T::AccountId, amount: BalanceOf<T>, reason: T::RuntimeHoldReason },

		/// An amount of fungible balance was released from its hold.
		HoldReleased { account: T::AccountId, amount: BalanceOf<T>, reason: T::RuntimeHoldReason },
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

		/// Try to migrate a parachain sovereign child account to its respective sibling.
		///
		/// Takes the old and new account and migrates it only if they are as expected. An event of
		/// `SovereignMigrated` will be emitted if the account was migrated successfully.
		///
		/// Callable by any signed origin.
		#[pallet::call_index(3)]
		#[pallet::weight(T::DbWeight::get().reads_writes(15, 15)
					.saturating_add(Weight::from_parts(0, 50_000)))]
		pub fn migrate_parachain_sovereign_acc(
			origin: OriginFor<T>,
			from: T::AccountId,
			to: T::AccountId,
		) -> DispatchResult {
			ensure_root(origin)?;

			Self::do_migrate_parachain_sovereign_derived_acc(&from, &to, None).map_err(Into::into)
		}

		/// Try to migrate a parachain sovereign child account to its respective sibling.
		///
		/// Takes the old and new account and migrates it only if they are as expected. An event of
		/// `SovereignMigrated` will be emitted if the account was migrated successfully.
		///
		/// Callable by any signed origin.
		#[pallet::call_index(5)]
		#[pallet::weight(T::DbWeight::get().reads_writes(15, 15)
					.saturating_add(Weight::from_parts(0, 50_000)))]
		pub fn migrate_parachain_sovereign_derived_acc(
			origin: OriginFor<T>,
			from: T::AccountId,
			to: T::AccountId,
			derivation: (T::AccountId, DerivationIndex),
		) -> DispatchResult {
			ensure_root(origin)?;

			Self::do_migrate_parachain_sovereign_derived_acc(&from, &to, Some(derivation))
				.map_err(Into::into)
		}

		/// Force unreserve a named or unnamed reserve.
		#[pallet::call_index(4)]
		#[pallet::weight(T::DbWeight::get().reads_writes(10, 10)
					.saturating_add(Weight::from_parts(0, 50_000)))]
		pub fn force_unreserve(
			origin: OriginFor<T>,
			account: T::AccountId,
			amount: BalanceOf<T>,
			reason: Option<T::RuntimeHoldReason>,
		) -> DispatchResult {
			ensure_root(origin)?;

			Self::do_force_unreserve(account, amount, reason).map_err(Into::into)
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

		pub fn do_migrate_parachain_sovereign_derived_acc(
			from: &T::AccountId,
			to: &T::AccountId,
			derivation: Option<(T::AccountId, DerivationIndex)>,
		) -> Result<(), Error<T>> {
			if frame_system::Account::<T>::get(from) == Default::default() {
				// Nothing to do if the account does not exist
				return Ok(());
			}
			if from == to {
				return Err(Error::<T>::AccountIdentical);
			}
			pallet_balances::Pallet::<T>::ensure_upgraded(from); // prevent future headache

			let (translated_acc, para_id, index) = if let Some((parent, index)) = derivation {
				let (parent_translated, para_id) =
					Self::try_rc_sovereign_derived_to_ah(from, &parent, index)?;
				(parent_translated, para_id, Some(index))
			} else {
				let (translated_acc, para_id) = Self::try_translate_rc_sovereign_to_ah(from)?;
				(translated_acc, para_id, None)
			};
			ensure!(translated_acc == *to, Error::<T>::WrongSovereignTranslation);

			// Release all locks
			let locks: Vec<BalanceLock<T::Balance>> =
				pallet_balances::Locks::<T>::get(from).into_inner();
			for lock in &locks {
				let () = <T as Config>::Currency::remove_lock(lock.id, from);
			}

			// Thaw all the freezes
			let freezes: Vec<IdAmount<T::FreezeIdentifier, T::Balance>> =
				pallet_balances::Freezes::<T>::get(from).into();

			for freeze in &freezes {
				let () = <T as Config>::Currency::thaw(&freeze.id, from)
					.map_err(|_| Error::<T>::FailedToThaw)?;
			}

			// Release all holds
			let holds: Vec<IdAmount<T::RuntimeHoldReason, T::Balance>> =
				pallet_balances::Holds::<T>::get(from).into();

			for IdAmount { id, amount } in &holds {
				let _ = <T as Config>::Currency::release(id, from, *amount, Precision::Exact)
					.map_err(|_| Error::<T>::FailedToReleaseHold)?;
				Self::deposit_event(Event::HoldReleased {
					account: from.clone(),
					amount: *amount,
					reason: *id,
				});
			}

			// Unreserve unnamed reserves
			let unnamed_reserve = <T as Config>::Currency::reserved_balance(from);
			let missing = <T as Config>::Currency::unreserve(from, unnamed_reserve);
			defensive_assert!(missing == 0, "Should have unreserved the full amount");

			// Set consumer refs to zero
			let consumers = frame_system::Pallet::<T>::consumers(from);
			frame_system::Account::<T>::mutate(from, |acc| {
				acc.consumers = 0;
			});
			// We dont handle sufficients and there should be none
			ensure!(frame_system::Pallet::<T>::sufficients(from) == 0, Error::<T>::InternalError);

			// Sanity check
			let total = <T as Config>::Currency::total_balance(from);
			let reducible = <T as Config>::Currency::reducible_balance(
				from,
				Preservation::Expendable,
				Fortitude::Polite,
			);
			defensive_assert!(
				total >= <T as Config>::Currency::minimum_balance(),
				"Must have at least ED"
			);
			defensive_assert!(total == reducible, "Total balance should be reducible");

			// Now the actual balance transfer to the new account
			<T as Config>::Currency::transfer(from, to, total, Preservation::Expendable)
				.defensive()
				.map_err(|_| Error::<T>::FailedToTransfer)?;

			// Apply consumer refs
			frame_system::Account::<T>::mutate(to, |acc| {
				acc.consumers += consumers;
			});

			// Reapply the holds
			for hold in &holds {
				<T as Config>::Currency::hold(&hold.id, to, hold.amount)
					.map_err(|_| Error::<T>::FailedToPutHold)?;
				// Somehow there are no events for this being emitted... so we emit our own.
				Self::deposit_event(Event::HoldPlaced {
					account: to.clone(),
					amount: hold.amount,
					reason: hold.id,
				});
			}

			// Reapply the reserve
			<T as Config>::Currency::reserve(to, unnamed_reserve)
				.defensive()
				.map_err(|_| Error::<T>::FailedToReserve)?;

			// Reapply the locks
			for lock in &locks {
				let reasons = map_lock_reason(lock.reasons);
				<T as Config>::Currency::set_lock(lock.id, to, lock.amount, reasons);
			}
			// Reapply the freezes
			for freeze in &freezes {
				<T as Config>::Currency::set_freeze(&freeze.id, to, freeze.amount)
					.map_err(|_| Error::<T>::FailedToSetFreeze)?;
			}

			defensive_assert!(
				frame_system::Account::<T>::get(from) == Default::default(),
				"Must reap old account"
			);
			// If new account would die from this, then lets rather not do it and check it manually.
			ensure!(
				frame_system::Account::<T>::get(to) != Default::default(),
				Error::<T>::WouldReap
			);

			Self::deposit_event(Event::SovereignMigrated {
				para_id,
				from: from.clone(),
				to: to.clone(),
				derivation_index: index,
			});

			Ok(())
		}

		pub fn do_force_unreserve(
			account: T::AccountId,
			amount: BalanceOf<T>,
			reason: Option<T::RuntimeHoldReason>,
		) -> Result<(), Error<T>> {
			if let Some(reason) = reason {
				<T as Config>::Currency::release(&reason, &account, amount, Precision::Exact)
					.map_err(|_| Error::<T>::FailedToReleaseHold)?;
				Self::deposit_event(Event::HoldReleased {
					account: account.clone(),
					amount,
					reason,
				});
			} else {
				let remaining = <T as Config>::Currency::unreserve(&account, amount);
				if remaining > 0 {
					return Err(Error::<T>::CannotUnreserve);
				}
			}

			Ok(())
		}

		/// Try to translate a Parachain sovereign account to the Parachain AH sovereign account.
		///
		/// Returns:
		/// - `Ok(None)` if the account is not a Parachain sovereign account
		/// - `Ok(Some((ah_account, para_id)))` with the translated account and the para id
		/// - `Err(())` otherwise
		///
		/// The way that this normally works is through the configured
		/// `SiblingParachainConvertsVia`: <https://github.com/polkadot-fellows/runtimes/blob/7b096c14c2b16cc81ca4e2188eea9103f120b7a4/system-parachains/asset-hubs/asset-hub-polkadot/src/xcm_config.rs#L93-L94>
		/// it passes the `Sibling` type into it which has type-ID `sibl`:
		/// <https://github.com/paritytech/polkadot-sdk/blob/c10e25aaa8b8afd8665b53f0a0b02e4ea44caa77/polkadot/parachain/src/primitives.rs#L272-L274>
		/// This type-ID gets used by the converter here:
		/// <https://github.com/paritytech/polkadot-sdk/blob/7ecf3f757a5d6f622309cea7f788e8a547a5dce8/polkadot/xcm/xcm-builder/src/location_conversion.rs#L314>
		/// and eventually ends up in the encoding here
		/// <https://github.com/paritytech/polkadot-sdk/blob/cdf107de700388a52a17b2fb852c98420c78278e/substrate/primitives/runtime/src/traits/mod.rs#L1997-L1999>
		/// The `para` conversion is likewise with `ChildParachainConvertsVia` and the `para`
		/// type-ID <https://github.com/paritytech/polkadot-sdk/blob/c10e25aaa8b8afd8665b53f0a0b02e4ea44caa77/polkadot/parachain/src/primitives.rs#L162-L164>
		pub fn try_translate_rc_sovereign_to_ah(
			from: &AccountId32,
		) -> Result<(AccountId32, ParaId), Error<T>> {
			let raw = from.to_raw_vec();

			// Must start with "para"
			let Some(raw) = raw.strip_prefix(b"para") else {
				return Err(Error::<T>::NotSovereign);
			};
			// Must end with 26 zero bytes
			let Some(raw) = raw.strip_suffix(&[0u8; 26]) else {
				return Err(Error::<T>::NotSovereign);
			};
			let para_id = u16::decode_all(&mut &raw[..]).map_err(|_| Error::<T>::InternalError)?;

			// Translate to AH sibling account
			let mut ah_raw = [0u8; 32];
			ah_raw[0..4].copy_from_slice(b"sibl");
			ah_raw[4..6].copy_from_slice(&para_id.encode());

			Ok((ah_raw.into(), para_id))
		}

		/// Same as `try_translate_rc_sovereign_to_ah` but for derived accounts.
		pub fn try_rc_sovereign_derived_to_ah(
			from: &AccountId32,
			parent: &AccountId32,
			index: DerivationIndex,
		) -> Result<(AccountId32, ParaId), Error<T>> {
			// check the derivation proof
			{
				let derived = pallet_utility::derivative_account_id(parent.clone(), index);
				ensure!(derived == *from, Error::<T>::WrongDerivedTranslation);
			}

			let (parent_translated, para_id) = Self::try_translate_rc_sovereign_to_ah(parent)?;
			let parent_translated_derived =
				pallet_utility::derivative_account_id(parent_translated, index);
			Ok((parent_translated_derived, para_id))
		}
	}
}

/// Backward mapping from <https://github.com/paritytech/polkadot-sdk/blob/74a5e1a242274ddaadac1feb3990fc95c8612079/substrate/frame/balances/src/types.rs#L38>
pub fn map_lock_reason(reasons: LockReasons) -> LockWithdrawReasons {
	match reasons {
		LockReasons::All => LockWithdrawReasons::TRANSACTION_PAYMENT | LockWithdrawReasons::RESERVE,
		LockReasons::Fee => LockWithdrawReasons::TRANSACTION_PAYMENT,
		LockReasons::Misc => LockWithdrawReasons::TIP,
	}
}
