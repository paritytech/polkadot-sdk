use super::*;
use codec::{Decode, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use frame_support::traits::UncheckedOnRuntimeUpgrade;
use sp_runtime::Weight;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

type SystemBlockNumberFor<T> = frame_system::pallet_prelude::BlockNumberFor<T>;

pub mod v0 {
	use super::*;
	use frame_support::{pallet_prelude::OptionQuery, storage_alias, Blake2_128Concat};

	#[storage_alias]
	pub type Receipts<T: Config> =
		StorageMap<Pallet<T>, Blake2_128Concat, ReceiptIndex, ReceiptRecordOf<T>, OptionQuery>;
	pub type ReceiptRecordOf<T> = ReceiptRecord<
		<T as frame_system::Config>::AccountId,
		SystemBlockNumberFor<T>,
		BalanceOf<T>,
	>;
}

pub mod v1 {
	use super::*;
	pub fn get_all_receipt_expiries<T: Config>() -> Vec<(ReceiptIndex, SystemBlockNumberFor<T>)> {
		let mut receipt_expiry = Vec::new();

		// Using for_each to collect the index and expiry values
		v0::Receipts::<T>::iter().for_each(|(index, receipt)| {
			// Collecting both the index and the expiry block number as a tuple
			receipt_expiry.push((index, receipt.expiry));
		});

		receipt_expiry
	}

	pub trait BlockToRelayHeightConversion<T: Config> {
		fn convert_block_number_to_relay_height(
			block_number: SystemBlockNumberFor<T>,
		) -> BlockNumberFor<T>;
	}

	pub struct MigrateToRBN<T, BlockConversion>(PhantomData<T>, PhantomData<BlockConversion>);
	impl<T: Config, BlockConversion: BlockToRelayHeightConversion<T>> UncheckedOnRuntimeUpgrade
		for MigrateToRBN<T, BlockConversion>
	{
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			// Collect the receipt expiry data, convert it, and prepare for encoding
			let updated_expiry_records: Vec<(ReceiptIndex, BlockNumberFor<T>)> =
				get_all_receipt_expiries::<T>()
					.into_iter()
					.map(|(index, expiry)| {
						let updated_expiry =
							BlockConversion::convert_block_number_to_relay_height(expiry);
						(index, updated_expiry)
					})
					.collect();

			// Encode the updated records and return them
			Ok(updated_expiry_records.encode())
		}

		fn on_runtime_upgrade() -> Weight {
			log::info!(
					target: LOG_TARGET,
					"Running migration to change reciept expiry to new clock.",
			);

			let mut call_count = 0u64;

			// Iterate over v0::Receipts and update the expiry in Receipts
			v0::Receipts::<T>::iter().for_each(|(index, v0_receipt)| {
				// Use mutate to modify the value directly in the storage
				Receipts::<T>::mutate(index, |maybe_receipt| {
					if let Some(receipt) = maybe_receipt {
						// Update the expiry field
						receipt.expiry = BlockConversion::convert_block_number_to_relay_height(
							v0_receipt.expiry,
						);

						// Increment the call count
						call_count = call_count.saturating_add(2);
					}
				});
			});

			call_count.into()
		}
	}
}
