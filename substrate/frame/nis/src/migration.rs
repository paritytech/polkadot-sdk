use crate::*;
use core::marker::PhantomData;
use frame_support::{
	pallet_prelude::*,
	traits::{Get, OnRuntimeUpgrade},
};
use sp_runtime::Weight;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;

type SystemBlockNumberFor<T> = frame_system::pallet_prelude::BlockNumberFor<T>;

pub mod v0 {
	use super::*;
	use frame_support::{pallet_prelude::OptionQuery, storage_alias, Blake2_128Concat};

	#[storage_alias]
	pub type Receipts<T: Config> =
		StorageMap<Pallet<T>, Blake2_128Concat, ReceiptIndex, OldReceiptRecordOf<T>, OptionQuery>;
	pub type OldReceiptRecordOf<T> = ReceiptRecord<
		<T as frame_system::Config>::AccountId,
		SystemBlockNumberFor<T>,
		BalanceOf<T>,
	>;

	#[storage_alias]
	pub type Summary<T: Config> = StorageValue<Pallet<T>, OldSummaryRecordOf<T>, ValueQuery>;
	pub type OldSummaryRecordOf<T> = SummaryRecord<SystemBlockNumberFor<T>, BalanceOf<T>>;
}

pub mod switch_block_number_provider {
	use super::*;

	/// The log target.
	const TARGET: &'static str = "runtime::nis::migration::change_block_number_provider";

	pub trait BlockNumberConversion<Old, New> {
		fn convert_block_number(block_number: Old) -> New;
	}

	pub struct MigrateBlockNumber<T, BlockConverter>(PhantomData<T>, PhantomData<BlockConverter>);
	impl<
			T: Config,
			BlockConverter: BlockNumberConversion<SystemBlockNumberFor<T>, BlockNumberFor<T>>,
		> OnRuntimeUpgrade for MigrateBlockNumber<T, BlockConverter>
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
				ensure!(
					new_receipt.expiry == expected_expiry,
					"Receipt expiry conversion failed"
				);

				// Verify other fields unchanged
				ensure!(
					new_receipt.proportion == old_receipt.proportion &&
						new_receipt.owner == old_receipt.owner,
					"Receipt fields corrupted"
				);
			}

			// Verify Summary migration
			let new_summary = Summary::<T>::get();
			let expected_last_period = BlockConverter::convert_block_number(old_summary.last_period);
			ensure!(
				new_summary.last_period == expected_last_period,
				"Summary conversion failed"
			);
			log::info!(target: TARGET, "All reciept record expiry period and summary record thaw period begining migrated to new blok number provider");
			Ok(())
		}
	}

	pub fn migrate_block_number<T, BlockConverter>() -> Weight
	where
		BlockConverter: BlockNumberConversion<SystemBlockNumberFor<T>, BlockNumberFor<T>>,
		T: Config,
	{
		let on_chain_version = Pallet::<T>::on_chain_storage_version();
		let mut weight = T::DbWeight::get().reads(1);
		log::info!(
			target: TARGET,
			"running migration with onchain storage version {:?}", on_chain_version
		);

		if on_chain_version == 0 {
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
			weight.saturating_add(T::DbWeight::get().reads_writes(1, 1))
		} else {
			log::info!(target: TARGET, "skipping migration from on-chain version {:?} to change_block_number_provider", on_chain_version);
			weight
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		migration::switch_block_number_provider::{migrate_block_number, BlockNumberConversion},
		mock::{Test, *},
	};
	use sp_runtime::Perquintill;

	#[test]
	fn migration_works_with_receipts_and_summary() {
		ExtBuilder::default().build_and_execute(|| {
			pub struct TestConverter;

			impl BlockNumberConversion<SystemBlockNumberFor<Test>, BlockNumberFor<Test>> for TestConverter {
				fn convert_block_number(
					block_number: SystemBlockNumberFor<Test>,
				) -> BlockNumberFor<Test> {
					block_number as u64
				}
			}

			let receipt1 = v0::OldReceiptRecordOf::<Test> {
				proportion: Perquintill::from_percent(10),
				owner: Some((1, 40)),
				expiry: 5,
			};
			v0::Receipts::<Test>::insert(1, receipt1.clone());

			let receipt2 = v0::OldReceiptRecordOf::<Test> {
				proportion: Perquintill::from_percent(20),
				owner: Some((2, 40)),
				expiry: 10,
			};
			v0::Receipts::<Test>::insert(2, receipt2.clone());

			// Set old summary
			let old_summary = v0::OldSummaryRecordOf::<Test> {
				proportion_owed: Perquintill::zero(),
				index: 0,
				thawed: Perquintill::zero(),
				last_period: 15,
				receipts_on_hold: 2,
			};
			v0::Summary::<Test>::put(old_summary.clone());

			let _weights = migrate_block_number::<Test, TestConverter>();

			// Check migrated receipts
			let new_receipt1 = Receipts::<Test>::get(1).unwrap();
			assert_eq!(new_receipt1.expiry, 5);

			let new_receipt2 = Receipts::<Test>::get(2).unwrap();
			assert_eq!(new_receipt2.expiry, 10);

			// Check migrated summary
			let new_summary = Summary::<Test>::get();
			assert_eq!(new_summary.last_period, 15);
			assert_eq!(new_summary.receipts_on_hold, old_summary.receipts_on_hold);
		})
	}
}
