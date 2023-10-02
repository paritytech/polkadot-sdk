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
use sp_runtime::traits::AccountIdConversion;
use frame_support::traits::OnRuntimeUpgrade;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

pub mod versioned {
	use super::*;

	/// Wrapper over `MigrateToV1` with convenience version checks.
	///
	/// This migration would add a new StorageDoubleMap `ReservedAmounts` and initialise it with
	/// the current deposit of existing leases.
	pub type ToV1<T> = frame_support::migrations::VersionedMigration<
		0,
		1,
		v1::MigrateToV1<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

mod v1 {
	use super::*;

	/// This migration would restrict reward account of pools to go below ED by doing a named
	/// freeze on all the existing pools.
	pub struct MigrateToV1<T>(sp_std::marker::PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for MigrateToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			todo!("migrate to v1")
		}
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			todo!("migrate to v1 pre check")
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_data: Vec<u8>) -> Result<(), TryRuntimeError> {
			todo!("migrate to v1 post check")
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
