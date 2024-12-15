use super::*;
use frame_support::traits::OnRuntimeUpgrade;
use sp_runtime::{traits::BlockNumberProvider, Saturating};
use log;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

pub mod v1 {
	use super::*;
	use frame_support::{pallet_prelude::*, weights::Weight};

	pub struct MigrateToV1<T>(core::marker::PhantomData<T>);
	impl<T: Config> OnRuntimeUpgrade for MigrateToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let in_code_version = Pallet::<T>::in_code_storage_version();
			let on_chain_version = Pallet::<T>::on_chain_storage_version();
			
			log::info!(
					target: LOG_TARGET,
					"Running migration with in-code storage version {:?} / onchain {:?}",
					in_code_version,
					on_chain_version
			);

			let mut call_count = 0u64;
			if on_chain_version == 0 && in_code_version == 1 {
				<Receipts<T>>::translate::<ReceiptRecordOf<T>, _>(|_, v0| {
					call_count.saturating_inc();
					Some(ReceiptRecord {
						proportion: v0.proportion,
						owner: v0.owner,
						expiry: <T as Config>::BlockNumberProvider::current_block_number(),
					})
				});

				in_code_version.put::<Pallet<T>>();

				T::DbWeight::get().reads_writes(
					// Reads: Get Calls + Get Version
					call_count.saturating_add(1),
					// Writes: Change Clock + Set version
					call_count.saturating_add(1),
				)
			} else {
				log::info!(
						target: LOG_TARGET,
						"Migration did not execute. This probably should be removed"
				);
				T::DbWeight::get().reads(1)
			}
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			use sp_runtime::TryRuntimeError;

			if std::any::TypeId::of::<<T as Config>::BlockNumberProvider>() !=
				std::any::TypeId::of::<frame_system::pallet_prelude::BlockNumberFor<T>>()
			{
				// If it's still using System, no migration needed, just return Ok
				log::info!(
						target: LOG_TARGET,
						"Using a different clock than System."
				);
				return Ok(Default::default())
			} else {
				return Err(TryRuntimeError::Other("No need to migrate"));
			}
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_prev_count: Vec<u8>) -> Result<(), TryRuntimeError> {
			ensure!(Pallet::<T>::on_chain_storage_version() >= 1, "wrong storage version");
			Ok(())
		}
	}
}
