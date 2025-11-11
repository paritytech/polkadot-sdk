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

//! Staking Async Pallet migration from v17 to v18.

use crate::{
	BalanceOf, Config, Ledger, Pallet, StakingLedger, UnbondingQueueParams, UnlockChunk, migrations::PALLET_MIGRATIONS_ID, pallet::pallet::ElectableStashes, weights::WeightInfo
};
use alloc::{collections::BTreeMap, vec::Vec};
use codec::{Decode, Encode};
use core::fmt::Debug;
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::*,
	weights::WeightMeter,
};

pub(crate) mod v17 {
	use crate::{BalanceOf, Config, Pallet};
	use codec::{HasCompact, MaxEncodedLen};
	use frame_support::{pallet_prelude::*, storage_alias};
	use sp_staking::EraIndex;

	#[derive(
		PartialEq, Eq, Clone, Encode, Decode, DecodeWithMemTracking, Debug, TypeInfo, MaxEncodedLen,
	)]
	pub struct UnlockChunk<Balance: HasCompact + MaxEncodedLen> {
		/// Amount of funds to be unlocked.
		#[codec(compact)]
		pub(crate) value: Balance,
		/// Era number at which point it'll be unlocked.
		#[codec(compact)]
		pub(crate) era: EraIndex,
	}

	#[derive(
		PartialEqNoBound,
		EqNoBound,
		CloneNoBound,
		Encode,
		Decode,
		DebugNoBound,
		TypeInfo,
		MaxEncodedLen,
	)]
	#[scale_info(skip_type_params(T))]
	pub struct StakingLedger<T: Config> {
		pub stash: T::AccountId,
		#[codec(compact)]
		pub total: BalanceOf<T>,
		#[codec(compact)]
		pub active: BalanceOf<T>,
		pub unlocking: BoundedVec<UnlockChunk<BalanceOf<T>>, T::MaxUnlockingChunks>,
		#[codec(skip)]
		pub(crate) controller: Option<T::AccountId>,
	}

	#[storage_alias]
	pub type Ledger<T: Config> = StorageMap<
		Pallet<T>,
		Blake2_128Concat,
		<T as frame_system::Config>::AccountId,
		StakingLedger<T>,
	>;

	#[storage_alias]
	pub type ElectableStashes<T: Config> = StorageValue<
		Pallet<T>,
		BoundedBTreeSet<<T as frame_system::Config>::AccountId, <T as Config>::MaxValidatorSet>,
		ValueQuery,
	>;
}

/// Operations to be performed during this migration.
#[derive(
	PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, scale_info::TypeInfo, MaxEncodedLen,
)]
pub enum MigrationSteps<T: Config> {
	/// Migrate staking ledger storage. The cursor indicates the last processed account.
	/// If None, start from the beginning.
	MigrateStakingLedger { cursor: Option<T::AccountId> },
	/// Migrate electable stashes storage.
	MigrateElectableStashes,
	/// Changes the storage version to 18.
	ChangeStorageVersion,
	/// No more operations to be performed.
	Noop,
}

pub struct LazyMigrationV17ToV18<T: Config>(PhantomData<T>);

impl<T: Config + Debug> LazyMigrationV17ToV18<T> {
	pub(crate) fn do_migrate_staking_ledger(
		meter: &mut WeightMeter,
		cursor: &mut Option<T::AccountId>,
	) {
		let max_chunks = <T as Config>::MaxUnlockingChunks::get();
		let required =
			<T as Config>::WeightInfo::migration_from_v17_to_v18_migrate_staking_ledger_step(
				max_chunks,
			);

		let mut iter = if let Some(acc) = cursor.clone() {
			v17::Ledger::<T>::iter_from(v17::Ledger::<T>::hashed_key_for(acc))
		} else {
			v17::Ledger::<T>::iter()
		};

		let max_bonding_duration = UnbondingQueueParams::<T>::get().max_time;
		while meter.can_consume(required) {
			if let Some((acc, old_ledger)) = iter.next() {
				meter.consume(<T as Config>::WeightInfo::migration_from_v17_to_v18_migrate_staking_ledger_step(old_ledger.unlocking.len() as u32));
				let new_unlocking = old_ledger
					.unlocking
					.iter()
					.map(|c| UnlockChunk {
						value: c.value,
						era: c.era.saturating_sub(max_bonding_duration),
						previous_unbonded_stake: u32::MAX.into(),
					})
					.collect::<Vec<_>>();
				Ledger::<T>::insert(
					acc.clone(),
					StakingLedger {
						stash: old_ledger.stash,
						total: old_ledger.total,
						active: old_ledger.active,
						unlocking: new_unlocking
							.try_into()
							.expect("Array lengths should be the same; qed"),
						controller: None,
					},
				);
				*cursor = Some(acc)
			} else {
				*cursor = None;
				break;
			}
		}
	}

	pub(crate) fn change_storage_version(meter: &mut WeightMeter) -> MigrationSteps<T> {
		let required = T::DbWeight::get().reads_writes(0, 1);
		if meter.try_consume(required).is_ok() {
			StorageVersion::new(Self::id().version_to as u16).put::<Pallet<T>>();
			MigrationSteps::Noop
		} else {
			MigrationSteps::ChangeStorageVersion
		}
	}

	pub(crate) fn migrate_electable_stashes(meter: &mut WeightMeter) -> MigrationSteps<T> {
		let required = T::DbWeight::get().reads_writes(1, 2);
		if meter.try_consume(required).is_ok() {
			let orig = v17::ElectableStashes::<T>::take();
			let new_electable_stashes = orig
				.iter()
				.map(|acc| (acc.clone(), BalanceOf::<T>::zero()))
				.collect::<BTreeMap<<T as frame_system::Config>::AccountId, BalanceOf<T>>>();
			ElectableStashes::<T>::set(
				new_electable_stashes
					.try_into()
					.expect("The number of elements should be the same; qed"),
			);
			MigrationSteps::ChangeStorageVersion
		} else {
			MigrationSteps::MigrateElectableStashes
		}
	}

	pub(crate) fn migrate_staking_ledger(
		meter: &mut WeightMeter,
		mut cursor: Option<T::AccountId>,
	) -> MigrationSteps<T> {
		Self::do_migrate_staking_ledger(meter, &mut cursor);
		match cursor {
			None => Self::migrate_electable_stashes(meter),
			Some(checkpoint) => MigrationSteps::MigrateStakingLedger { cursor: Some(checkpoint) },
		}
	}
}

impl<T: Config + Debug> SteppedMigration for LazyMigrationV17ToV18<T> {
	type Cursor = MigrationSteps<T>;
	type Identifier = MigrationId<20>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 17, version_to: 18 }
	}

	fn step(
		maybe_cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		if Pallet::<T>::on_chain_storage_version() != Self::id().version_from as u16 {
			return Ok(None);
		}

		let cursor = maybe_cursor.unwrap_or(MigrationSteps::MigrateStakingLedger { cursor: None });
		log::info!("Running migration at step: {:?}", cursor);

		let new_cursor = match cursor {
			MigrationSteps::MigrateStakingLedger { cursor: checkpoint } =>
				Self::migrate_staking_ledger(meter, checkpoint),
			MigrationSteps::MigrateElectableStashes => Self::migrate_electable_stashes(meter),
			MigrationSteps::ChangeStorageVersion => Self::change_storage_version(meter),
			MigrationSteps::Noop => MigrationSteps::Noop,
		};

		match new_cursor {
			MigrationSteps::Noop => {
				log::info!("Migration from v17 to v18 fully complete!");
				Ok(None)
			},
			_ => {
				log::info!("Migration from v17 to v18 not completed yet: {:?}", new_cursor);
				Ok(Some(new_cursor))
			},
		}
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		let prev_electable_stashes = v17::ElectableStashes::<T>::get().into_inner();
		let prev_ledgers = v17::Ledger::<T>::iter().collect::<BTreeMap<_, _>>();
		Ok((prev_electable_stashes, prev_ledgers).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(prev: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use codec::Decode;

		ensure!(
			Pallet::<T>::on_chain_storage_version() ==
				StorageVersion::new(Self::id().version_to as u16),
			"Migration post-upgrade failed: the storage version is not the expected one"
		);

		let (prev_electable_stashes, prev_ledgers) = <(
			alloc::collections::BTreeSet<T::AccountId>,
			BTreeMap<T::AccountId, v17::StakingLedger<T>>,
		)>::decode(&mut &prev[..])
		.expect("Failed to decode the previous storage state");

		let new_electable_stashes = ElectableStashes::<T>::get();
		ensure!(
			new_electable_stashes.len() == prev_electable_stashes.len(),
			"Migration failed: the number of electable stashes is not the same"
		);
		for (acc, amount) in new_electable_stashes.into_inner() {
			ensure!(
				amount.is_zero(),
				"Migration failed: the stake for the stash account is not zero after the migration"
			);
			ensure!(
				prev_electable_stashes.get(&acc).is_some(),
				"Migration failed: the electable stash is missing in the previous storage state"
			);
		}

		let new_ledgers = Ledger::<T>::iter().collect::<BTreeMap<_, _>>();
		ensure!(
			new_ledgers.len() == prev_ledgers.len(),
			"Migration failed: the number of staking ledgers is not the same"
		);
		for (acc, ledger) in new_ledgers.into_iter() {
			if let Some(prev_ledger) = prev_ledgers.get(&acc) {
				ensure!(
					ledger.total == prev_ledger.total,
					"Migration failed: the ledger's total stake is not the same"
				);
				ensure!(
					ledger.stash == prev_ledger.stash,
					"Migration failed: the ledger's stash is not the same"
				);
				ensure!(
					ledger.controller == prev_ledger.controller,
					"Migration failed: the ledger's controller is not the same"
				);
				ensure!(
					ledger.active == prev_ledger.active,
					"Migration failed: the ledger's active stake is not the same"
				);
				ensure!(
					ledger.unlocking.len() == prev_ledger.unlocking.len(),
					"Migration failed: different number of unlocking chunks"
				);
				for (i, chunk) in ledger.unlocking.iter().enumerate() {
					let old_chunk = &prev_ledger.unlocking[i];
					ensure!(
						chunk.era ==
							old_chunk
								.era
								.saturating_sub(UnbondingQueueParams::<T>::get().max_time),
						"Migration failed: mismatch in chunk's era"
					);
					ensure!(
						chunk.value == old_chunk.value,
						"Migration failed: mismatch in chunk's value"
					);
					ensure!(
						chunk.previous_unbonded_stake == u32::MAX.into(),
						"Migration failed: previous unbonded stake in chunk is not zero"
					);
				}
			} else {
				panic!("Ledger not found in the previous storage state: {:?}", acc);
			}
		}

		Ok(())
	}
}

#[cfg(all(test, not(feature = "runtime-benchmarks")))]
mod tests {
	use super::*;
	use crate::mock::*;
	use frame_support::{migrations::MultiStepMigrator, traits::OnRuntimeUpgrade};
	use std::collections::BTreeSet;

	#[test]
	fn migration_of_many_elements_should_work() {
		ExtBuilder::default().try_state(false).has_stakers(false).build_and_execute(|| {
			let users = 1000;
			assert_eq!(UnbondingQueueParams::<Test>::get().max_time, 3);

			StorageVersion::new(17).put::<Pallet<Test>>();
			assert_eq!(Pallet::<Test>::on_chain_storage_version(), 17);
			Session::roll_until_active_era(10);
			let max_chunks = <Test as Config>::MaxUnlockingChunks::get();

			for i in 1..=users {
				let mut chunks = vec![];
				for _ in 0..max_chunks {
					chunks.push(v17::UnlockChunk { value: 1000, era: 10 });
				}
				v17::Ledger::<Test>::insert(
					i,
					v17::StakingLedger {
						stash: i,
						total: (max_chunks as u128) * 1000 + 300,
						active: 300,
						unlocking: chunks.try_into().unwrap(),
						controller: None,
					},
				);
			}

			let total_electable_stashes = <Test as Config>::MaxValidatorSet::get() as u64;
			let mut electable_stashes = BTreeSet::new();
			(1..=total_electable_stashes).for_each(|i| {
				electable_stashes.insert(i);
			});
			v17::ElectableStashes::<Test>::set(electable_stashes.clone().try_into().unwrap());

			// Perform the migration.
			let initial_block = System::block_number();
			AllPalletsWithSystem::on_runtime_upgrade();
			while <Migrator as MultiStepMigrator>::ongoing() {
				let block = System::block_number();
				assert!(
					block - initial_block <= 100,
					"Migration should not take more than 100 blocks"
				);
				Session::roll_next();
			}
			assert!(
				System::block_number() >= initial_block + 2,
				"Migration did not last more than two blocks"
			);

			// Check the results after the migration.
			assert_eq!(Pallet::<Test>::on_chain_storage_version(), StorageVersion::new(18));

			let expected_stashes = electable_stashes
				.into_iter()
				.map(|acc| (acc, 0u128))
				.collect::<BTreeMap<_, _>>();
			assert_eq!(ElectableStashes::<Test>::get().into_inner(), expected_stashes);

			Ledger::<Test>::iter().for_each(|(acc, ledger)| {
				assert_eq!(ledger.stash, acc);
				assert_eq!(ledger.controller, None);
				assert_eq!(ledger.total, (max_chunks * 1000 + 300).into());
				assert_eq!(ledger.active, 300);
				for unlocking in ledger.unlocking.into_iter() {
					assert_eq!(unlocking.value, 1000);
					assert_eq!(unlocking.previous_unbonded_stake, u32::MAX.into());
					assert_eq!(unlocking.era, 10 - 3);
				}
			})
		});
	}
}
