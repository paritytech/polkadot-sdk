use crate::*;
use core::marker::PhantomData;
use frame_support::{
	pallet_prelude::*,
	traits::{Get, UncheckedOnRuntimeUpgrade},
};
use sp_runtime::{Saturating, Weight};

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

/// The log target.
const TARGET: &'static str = "runtime::nis::migration::v1";

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

pub mod v1 {
	use super::*;

	pub trait BlockToRelayHeightConversion<T: Config> {
		fn convert_block_number_to_relay_height(
			block_number: SystemBlockNumberFor<T>,
		) -> BlockNumberFor<T>;
	}

	pub struct MigrateV0ToV1<T, BlockConversion>(PhantomData<T>, PhantomData<BlockConversion>);
	impl<T: Config, BlockConversion: BlockToRelayHeightConversion<T>> UncheckedOnRuntimeUpgrade
		for MigrateV0ToV1<T, BlockConversion>
	{
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			let old_receipts = v0::Receipts::<T>::iter().collect::<Vec<_>>();
			log::info!(target: TARGET, "reciept expirys will be migrated.");
			let old_summary = v0::Summary::<T>::get();
			log::info!(target: TARGET, "The current thaw period's beginning will be migrated.");
			Ok((old_receipts, old_summary).encode())
		}

		fn on_runtime_upgrade() -> Weight {
			let mut call_count = 0u64;

			// Check if migration is needed (old storage version)
			if StorageVersion::get::<Pallet<T>>() == 0 {
				Receipts::<T>::translate(|index, old_receipt: v0::OldReceiptRecordOf<T>| {
					call_count.saturating_inc();
					log::info!(target: TARGET, "migrating reciept record expiry #{:?}", &index);
					let new_expiry =
						BlockConversion::convert_block_number_to_relay_height(old_receipt.expiry);
					Some(ReceiptRecord {
						proportion: old_receipt.proportion,
						owner: old_receipt.owner,
						expiry: new_expiry,
					})
				});

				// Read old value
				let old_summary = v0::Summary::<T>::get();

				let new_last_period =
					BlockConversion::convert_block_number_to_relay_height(old_summary.last_period);
				// Convert to new format
				let new_summary = SummaryRecord {
					proportion_owed: old_summary.proportion_owed,
					index: old_summary.index,
					thawed: old_summary.thawed,
					last_period: new_last_period,
					receipts_on_hold: old_summary.receipts_on_hold,
				};
				// Write new value
				Summary::<T>::put(new_summary);
				// Update storage version
				StorageVersion::new(1).put::<Pallet<T>>();
				// Return weight (adjust based on operations)
				T::DbWeight::get().reads_writes(call_count + 1u64, call_count + 2u64)
			} else {
				log::info!(target: TARGET, "nill");
				call_count.into()
			}
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			// Decode pre-upgrade state
			let (old_receipts, old_summary): (
				Vec<(ReceiptIndex, v0::OldReceiptRecordOf<T>)>,
				v0::OldSummaryRecordOf<T>,
			) = Decode::decode(&mut &state[..]).expect("pre_upgrade data must decode");

			// Verify Receipts migration
			for (index, old_receipt) in old_receipts {
				let new_receipt =
					Receipts::<T>::get(index).ok_or("Receipt missing after migration")?;

				// Verify expiry conversion
				let expected_expiry =
					BlockConversion::convert_block_number_to_relay_height(old_receipt.expiry);
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
			let old_summary_last_period =
				BlockConversion::convert_block_number_to_relay_height(old_summary.last_period);
			ensure!(
				new_summary.last_period == old_summary_last_period,
				"Summary conversion failed"
			);

			// Verify storage version
			ensure!(StorageVersion::get::<Pallet<T>>() == 1, "Storage version not updated");

			Ok(())
		}
	}
}

/// Migrate the pallet storage from `0` to `1`.
pub type MigrateV0ToV1<T, BlockConversion> = frame_support::migrations::VersionedMigration<
	0,
	1,
	v1::MigrateV0ToV1<T, BlockConversion>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;
