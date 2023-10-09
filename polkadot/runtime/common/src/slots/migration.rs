// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use super::*;
use crate::crowdloan;
use frame_support::traits::OnRuntimeUpgrade;
use sp_runtime::traits::AccountIdConversion;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

/// The current storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

pub mod versioned {
	use super::*;

	/// Wrapper over `MigrateToV1` with convenience version checks.
	///
	/// This migration would move lease reserves into named holds.
	pub type ToV1<T, OldCurrency> = frame_support::migrations::VersionedMigration<
		0,
		1,
		v1::MigrateToV1<T, OldCurrency>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

mod v1 {
	use super::*;
	use frame_support::traits::ReservableCurrency;
	use sp_std::collections::btree_map::BTreeMap;

	#[cfg(feature = "try-runtime")]
	use frame_support::traits::fungible::InspectHold;

	/// Balance type of OldCurrency.
	pub type OldBalanceOf<T, OldCurrency> = <OldCurrency as frame_support::traits::Currency<
		<T as frame_system::Config>::AccountId,
	>>::Balance;

	/// Alias to leases storage map with old currency.
	#[frame_support::storage_alias]
	pub type OldLeases<T: Config, OldCurrency> = StorageMap<
		Pallet<T>,
		Twox64Concat,
		ParaId,
		Vec<Option<(<T as frame_system::Config>::AccountId, OldBalanceOf<T, OldCurrency>)>>,
		ValueQuery,
	>;

	/// This migration would move funds for lease from reserved to hold.
	pub struct MigrateToV1<T, OldCurrency>(sp_std::marker::PhantomData<(T, OldCurrency)>);

	impl<T, OldCurrency> OnRuntimeUpgrade for MigrateToV1<T, OldCurrency>
	where
		T: Config,
		OldCurrency: ReservableCurrency<<T as frame_system::Config>::AccountId>,
		BalanceOf<T>: From<OldCurrency::Balance>,
	{
		fn on_runtime_upgrade() -> Weight {
			// useful for calculating weights later
			let mut migrated = 0u64;
			let mut leases_count = 0u64;

			for (_, lease_periods) in OldLeases::<T, OldCurrency>::iter() {
				leases_count += 1;
				let mut deposit_held: BTreeMap<T::AccountId, OldBalanceOf<T, OldCurrency>> =
					BTreeMap::new();

				// go through each lease and find the lease deposit required for each leaser.
				lease_periods.iter().for_each(|lease| {
					if let Some((who, amount)) = lease {
						deposit_held
							.entry(who.clone())
							.and_modify(|deposit| *deposit = *amount.max(deposit))
							.or_insert(*amount);
					}
				});

				deposit_held.iter().for_each(|(leaser, deposit)| {
					OldCurrency::unreserve(leaser, *deposit);
					let hold_result = Pallet::<T>::hold(leaser, BalanceOf::<T>::from(*deposit));
					defensive_assert!(
						hold_result.is_ok(),
						"hold should not fail, since we just unreserved the same amount"
					);
					migrated += 1;
				})
			}

			T::DbWeight::get().reads_writes(
				// reads: leases_count
				leases_count,
				// writes = migrated * (unreserve + hold)
				migrated.saturating_mul(2),
			)
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_data: Vec<u8>) -> Result<(), TryRuntimeError> {
			// Build a set of pairs of (para, who) that have a lease.
			let mut para_leasers =
				sp_std::collections::btree_set::BTreeSet::<(ParaId, T::AccountId)>::new();
			for (para, lease_periods) in Leases::<T>::iter() {
				lease_periods.into_iter().for_each(|maybe_lease| {
					if let Some((who, _)) = maybe_lease {
						para_leasers.insert((para, who));
					}
				});
			}

			// for each pair assert hold amount is what we expect
			para_leasers.iter().try_for_each(|(para, who)| -> Result<(), TryRuntimeError> {
				// fixme(ank4n) there is a case where an account has a hold for multiple para-ids..
				let actual_hold =
					T::Currency::balance_on_hold(&HoldReason::LeaseDeposit.into(), who);
				let expected_hold = Pallet::<T>::deposit_held(*para, who);

				ensure!(
					actual_hold == expected_hold,
					"ReservedAmount value not same as actual reserved balance"
				);

				Ok(())
			})
		}
	}
}

/// Migrations for using fund index to create fund accounts instead of para ID.
pub mod slots_crowdloan_index_migration {
	use super::*;

	// The old way we generated fund accounts.
	fn old_fund_account_id<T: Config + crowdloan::Config>(index: ParaId) -> T::AccountId {
		<T as crowdloan::Config>::PalletId::get().into_sub_account_truncating(index)
	}

	pub fn pre_migrate<T: Config + crowdloan::Config>() -> Result<(), &'static str> {
		for (para_id, leases) in Leases::<T>::iter() {
			let old_fund_account = old_fund_account_id::<T>(para_id);

			for (who, _amount) in leases.iter().flatten() {
				if *who == old_fund_account {
					let crowdloan =
						crowdloan::Funds::<T>::get(para_id).ok_or("no crowdloan found")?;
					log::info!(
						target: "runtime",
						"para_id={:?}, old_fund_account={:?}, fund_id={:?}, leases={:?}",
						para_id, old_fund_account, crowdloan.fund_index, leases,
					);
					break
				}
			}
		}

		Ok(())
	}

	pub fn migrate<T: Config + crowdloan::Config>() -> frame_support::weights::Weight {
		let mut weight = Weight::zero();

		for (para_id, mut leases) in Leases::<T>::iter() {
			weight = weight.saturating_add(T::DbWeight::get().reads(2));
			// the para id must have a crowdloan
			if let Some(fund) = crowdloan::Funds::<T>::get(para_id) {
				let old_fund_account = old_fund_account_id::<T>(para_id);
				let new_fund_account = crowdloan::Pallet::<T>::fund_account_id(fund.fund_index);

				// look for places the old account is used, and replace with the new account.
				for (who, _amount) in leases.iter_mut().flatten() {
					if *who == old_fund_account {
						*who = new_fund_account.clone();
					}
				}

				// insert the changes.
				weight = weight.saturating_add(T::DbWeight::get().writes(1));
				Leases::<T>::insert(para_id, leases);
			}
		}

		weight
	}

	pub fn post_migrate<T: Config + crowdloan::Config>() -> Result<(), &'static str> {
		for (para_id, leases) in Leases::<T>::iter() {
			let old_fund_account = old_fund_account_id::<T>(para_id);
			log::info!(target: "runtime", "checking para_id: {:?}", para_id);
			// check the old fund account doesn't exist anywhere.
			for (who, _amount) in leases.iter().flatten() {
				if *who == old_fund_account {
					panic!("old fund account found after migration!");
				}
			}
		}
		Ok(())
	}
}
