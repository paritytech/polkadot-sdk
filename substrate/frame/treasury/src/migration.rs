use super::*;
use alloc::collections::BTreeSet;
use core::marker::PhantomData;
use frame_support::traits::UncheckedOnRuntimeUpgrade;

/// The log target for this pallet.
const LOG_TARGET: &str = "runtime::treasury";

mod v1 {
	use super::*;
	pub struct MigrateToV1Impl<T, I>(PhantomData<(T, I)>);

	impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for MigrateToV1Impl<T, I> {
		fn on_runtime_upgrade() -> frame_support::weights::Weight {
			let mut approval_index = BTreeSet::new();
			for approval in Approvals::<T, I>::get().iter() {
				approval_index.insert(*approval);
			}

			let mut proposals_released = 0;
			for (proposal_index, p) in Proposals::<T, I>::iter() {
				if !approval_index.contains(&proposal_index) {
					let err_amount = T::Currency::unreserve(&p.proposer, p.bond);
					debug_assert!(err_amount.is_zero());
					Proposals::<T, I>::remove(proposal_index);
					log::info!(
						target: LOG_TARGET,
						"Released bond amount of {:?} to proposer {:?}",
						p.bond,
						p.proposer,
					);
					proposals_released += 1;
				}
			}

			log::info!(
				target: LOG_TARGET,
				"Storage migration v1 for pallet-treasury finished, released {} proposal bonds.",
				proposals_released,
			);

			// calculate and return migration weights
			let approvals_read = 1;
			T::DbWeight::get()
				.reads_writes(proposals_released as u64 + approvals_read, proposals_released as u64)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			Ok((Proposals::<T, I>::count() as u32).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let old_count = u32::decode(&mut &state[..]).expect("Known good");
			let new_count = Regions::<T>::iter_values().count() as u32;

			ensure!(
				old_count <= new_count,
				"Proposals after migration should be less or equal to old proposals"
			);
			Ok(())
		}
	}
}

/// Migrate the pallet storage from `0` to `1`.
pub type MigrateV0ToV1<T, I> = frame_support::migrations::VersionedMigration<
	0,
	1,
	v1::MigrateToV1Impl<T, I>,
	Pallet<T, I>,
	<T as frame_system::Config>::DbWeight,
>;
