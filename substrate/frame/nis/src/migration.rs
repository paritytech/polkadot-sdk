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

use crate::*;
use core::marker::PhantomData;
use frame::prelude::*;

#[cfg(feature = "try-runtime")]
use frame::try_runtime::TryRuntimeError;

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;

pub mod v0 {
	use super::*;

	#[frame::storage_alias]
	pub type Receipts<T: Config> =
		StorageMap<Pallet<T>, Blake2_128Concat, ReceiptIndex, OldReceiptRecordOf<T>, OptionQuery>;
	pub type OldReceiptRecordOf<T> = ReceiptRecord<
		<T as frame_system::Config>::AccountId,
		SystemBlockNumberFor<T>,
		BalanceOf<T>,
	>;

	#[frame::storage_alias]
	pub type Summary<T: Config> = StorageValue<Pallet<T>, OldSummaryRecordOf<T>, ValueQuery>;
	pub type OldSummaryRecordOf<T> = SummaryRecord<SystemBlockNumberFor<T>, BalanceOf<T>>;
}

pub mod switch_block_number_provider {
	use super::*;

	/// The log target.
	const TARGET: &'static str = "runtime::nis::migration::change_block_number_provider";

	pub trait BlockNumberConversion<T: Config> {
		fn convert_block_number(block_number: SystemBlockNumberFor<T>) -> ProvidedBlockNumber<T>;
	}

	pub struct MigrateBlockNumber<T, BlockConverter>(PhantomData<T>, PhantomData<BlockConverter>);
	impl<T: Config, BlockConverter: BlockNumberConversion<T>> OnRuntimeUpgrade
		for MigrateBlockNumber<T, BlockConverter>
	{
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			let old_receipts = v0::Receipts::<T>::iter().collect::<Vec<_>>();
			let old_summary = v0::Summary::<T>::get();
			Ok((old_receipts, old_summary).encode())
		}

		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();
			weight.saturating_accrue(migrate_block_number::<T, BlockConverter>());
			weight
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			// Decode pre-upgrade state
			let (old_receipts, old_summary): (
				Vec<(ReceiptIndex, v0::OldReceiptRecordOf<T>)>,
				v0::OldSummaryRecordOf<T>,
			) = Decode::decode(&mut &state[..]).expect("pre_upgrade data must decode");

			// Assert the count of receipts after migration matches the original count
			ensure!(
				Receipts::<T>::iter().count() == old_receipts.len(),
				"Receipt count mismatch after migration"
			);

			// Verify Receipts migration
			for (index, old_receipt) in old_receipts {
				let expected_expiry = BlockConverter::convert_block_number(old_receipt.expiry);
				let new_receipt =
					Receipts::<T>::get(index).ok_or("Receipt missing after migration")?;

				// Verify expiry conversion
				ensure!(new_receipt.expiry == expected_expiry, "Receipt expiry conversion failed");

				// Verify other fields unchanged
				ensure!(
					new_receipt.proportion == old_receipt.proportion &&
						new_receipt.owner == old_receipt.owner,
					"Receipt fields corrupted"
				);
			}

			// Verify Summary migration
			let new_summary = Summary::<T>::get();
			let expected_last_period =
				BlockConverter::convert_block_number(old_summary.last_period);
			ensure!(new_summary.last_period == expected_last_period, "Summary conversion failed");
			log::info!(target: TARGET, "All reciept record expiry period and summary record thaw period begining migrated to new blok number provider");
			Ok(())
		}
	}

	pub fn migrate_block_number<T, BlockConverter>() -> Weight
	where
		BlockConverter: BlockNumberConversion<T>,
		T: Config,
	{
		let mut weight = Weight::zero();

		Receipts::<T>::translate(|index, old_receipt: v0::OldReceiptRecordOf<T>| {
			weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
			log::info!(target: TARGET, "migrating reciept record expiry #{:?}", &index);
			let new_expiry = BlockConverter::convert_block_number(old_receipt.expiry);
			Some(ReceiptRecord {
				proportion: old_receipt.proportion,
				owner: old_receipt.owner,
				expiry: new_expiry,
			})
		});

		// Read old value
		let old_summary = v0::Summary::<T>::get();

		let new_last_period = BlockConverter::convert_block_number(old_summary.last_period);
		// Convert to new format
		let new_summary = SummaryRecord {
			proportion_owed: old_summary.proportion_owed,
			index: old_summary.index,
			thawed: old_summary.thawed,
			last_period: new_last_period,
			receipts_on_hold: old_summary.receipts_on_hold,
		};
		log::info!(target: TARGET, "migrating summary record, current thaw period's beginning.");
		// Write new value
		Summary::<T>::put(new_summary);
		// Return weight (adjust based on operations)
		weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));
		weight
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		migration::switch_block_number_provider::{BlockNumberConversion, MigrateBlockNumber},
		mock::{Test, *},
	};

	pub struct TestConverter;

	impl BlockNumberConversion<Test> for TestConverter {
		fn convert_block_number(
			block_number: SystemBlockNumberFor<Test>,
		) -> ProvidedBlockNumber<Test> {
			block_number
		}
	}

	fn setup_old_state() {
		// Setup old receipt structure
		let receipt1 = v0::OldReceiptRecordOf::<Test> {
			proportion: Perquintill::from_percent(10),
			owner: Some((1, 40)),
			expiry: 5,
		};
		v0::Receipts::<Test>::insert(1, receipt1);

		let receipt2 = v0::OldReceiptRecordOf::<Test> {
			proportion: Perquintill::from_percent(20),
			owner: Some((2, 40)),
			expiry: 10,
		};
		v0::Receipts::<Test>::insert(2, receipt2);

		// Setup old summary
		let old_summary = v0::OldSummaryRecordOf::<Test> {
			proportion_owed: Perquintill::zero(),
			index: 0,
			thawed: Perquintill::zero(),
			last_period: 15,
			receipts_on_hold: 2,
		};
		v0::Summary::<Test>::put(old_summary);
	}

	#[test]
	fn migration_works_with_receipts_and_summary() {
		ExtBuilder::default().build_and_execute(|| {
			setup_old_state();

			// Capture pre-upgrade state
			#[cfg(feature = "try-runtime")]
			let pre_state = MigrateBlockNumber::<Test, TestConverter>::pre_upgrade()
				.expect("Pre-upgrade should succeed");

			// Execute migration
			let _weight = MigrateBlockNumber::<Test, TestConverter>::on_runtime_upgrade();

			// Verify post-upgrade state
			#[cfg(feature = "try-runtime")]
			MigrateBlockNumber::<Test, TestConverter>::post_upgrade(pre_state)
				.expect("Post-upgrade checks should pass");

			// Additional sanity checks
			assert_eq!(
				Receipts::<Test>::get(1).unwrap().expiry,
				5,
				"Expiry should have conversion offset applied"
			);
			assert_eq!(
				Summary::<Test>::get().last_period,
				15,
				"Summary period should have conversion offset"
			);
		});
	}

	#[test]
	fn handles_empty_state_correctly() {
		ExtBuilder::default().build_and_execute(|| {
			// Test with no existing receipts
			#[cfg(feature = "try-runtime")]
			let pre_state = MigrateBlockNumber::<Test, TestConverter>::pre_upgrade()
				.expect("Pre-upgrade with empty state should work");

			let _weight = MigrateBlockNumber::<Test, TestConverter>::on_runtime_upgrade();

			#[cfg(feature = "try-runtime")]
			MigrateBlockNumber::<Test, TestConverter>::post_upgrade(pre_state)
				.expect("Post-upgrade with empty state should validate");
		});
	}
}
