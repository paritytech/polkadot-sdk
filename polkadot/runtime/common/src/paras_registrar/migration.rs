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
use frame_support::traits::{Contains, OnRuntimeUpgrade};

pub mod v1 {
	use super::*;

	#[derive(Encode, Decode)]
	pub struct ParaInfoV1<Account, Balance> {
		manager: Account,
		deposit: Balance,
		locked: bool,
	}

	pub struct VersionUncheckedMigrateToV1<T, UnlockParaIds>(
		sp_std::marker::PhantomData<(T, UnlockParaIds)>,
	);
	impl<T: Config, UnlockParaIds: Contains<ParaId>> OnRuntimeUpgrade
		for VersionUncheckedMigrateToV1<T, UnlockParaIds>
	{
		fn on_runtime_upgrade() -> Weight {
			let mut count = 0u64;
			Paras::<T>::translate::<ParaInfoV1<T::AccountId, BalanceOf<T>>, _>(|key, v1| {
				count.saturating_inc();
				Some(ParaInfo {
					manager: v1.manager.clone(),
					deposit: v1.deposit,
					locked: if UnlockParaIds::contains(&key) { None } else { Some(v1.locked) },
					billing_account: v1.manager,
					pending_refund: None,
				})
			});

			log::info!(target: "runtime::registrar", "Upgraded {} storages to version 1", count);
			T::DbWeight::get().reads_writes(count, count)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			Ok((Paras::<T>::iter_keys().count() as u32).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let old_count = u32::decode(&mut &state[..]).expect("Known good");
			let new_count = Paras::<T>::iter_values().count() as u32;

			ensure!(old_count == new_count, "Paras count should not change");
			Ok(())
		}
	}

	pub type MigrateToV1<T, UnlockParaIds> = frame_support::migrations::VersionedMigration<
		0,
		1,
		VersionUncheckedMigrateToV1<T, UnlockParaIds>,
		super::Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

pub mod v2 {
	use super::*;

	#[derive(Encode, Decode)]
	pub struct ParaInfoV2<Account, Balance> {
		manager: Account,
		deposit: Balance,
		locked: Option<bool>,
	}

	pub struct VersionUncheckedMigrateToV2<T, UnlockParaIds>(
		sp_std::marker::PhantomData<(T, UnlockParaIds)>,
	);
	impl<T: Config, UnlockParaIds: Contains<ParaId>> OnRuntimeUpgrade
		for VersionUncheckedMigrateToV2<T, UnlockParaIds>
	{
		fn on_runtime_upgrade() -> Weight {
			let mut count = 0u64;
			Paras::<T>::translate::<ParaInfoV2<T::AccountId, BalanceOf<T>>, _>(|_key, v2| {
				count.saturating_inc();
				Some(ParaInfo {
					manager: v2.manager.clone(),
					deposit: v2.deposit,
					locked: v2.locked,
					billing_account: v2.manager,
					pending_refund: None,
				})
			});

			log::info!(target: "runtime::registrar", "Upgraded {} storages to version 2", count);
			T::DbWeight::get().reads_writes(count, count)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			Ok((Paras::<T>::iter_keys().count() as u32).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let old_count = u32::decode(&mut &state[..]).expect("Known good");
			let new_count = Paras::<T>::iter_values().count() as u32;

			ensure!(old_count == new_count, "Paras count should not change");

			Paras::<T>::iter_keys().try_for_each(|para_id| -> Result<(), _> {
				let info = Paras::<T>::get(para_id).unwrap();
				ensure!(
					info.billing_account == info.manager,
					"The billing account must be set to the para manager"
				);
				ensure!(info.pending_refund.is_none(), "There should be no pending refund");

				Ok(())
			})
		}
	}

	pub type MigrateToV2<T, UnlockParaIds> = frame_support::migrations::VersionedMigration<
		1,
		2,
		VersionUncheckedMigrateToV2<T, UnlockParaIds>,
		super::Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}
