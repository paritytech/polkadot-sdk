use super::*;
// use codec::{Decode, Encode};
use core::marker::PhantomData;
use frame_support::{traits::{Get, UncheckedOnRuntimeUpgrade}, storage_alias};

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
#[cfg(feature = "try-runtime")]
use frame_support::ensure;

mod v1 {
    use super::*;

	pub struct MigrateToV1Impl<T>(PhantomData<T>);
    
    #[storage_alias]
    type ChildBountyDescriptions<T: Config + pallet_bounties::Config> =
            StorageMap<Pallet<T>, Twox64Concat, BountyIndex, BoundedVec<u8, <T as pallet_bounties::Config>::MaximumReasonLength>>;
    
    #[storage_alias]
    type ChildrenCuratorFees<T: Config> =
            StorageMap<Pallet<T>, Twox64Concat, BountyIndex, BalanceOf<T>, ValueQuery>;

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateToV1Impl<T> {
		fn on_runtime_upgrade() -> frame_support::weights::Weight {
			let mut reads = 0u64;
			let mut writes = 0u64;
			for (parent_bounty_id, old_child_bounty_id) in ChildBounties::<T>::iter_keys() {
				reads += 1;
				let bounty_description = v1::ChildBountyDescriptions::<T>::take(old_child_bounty_id);
				writes += 1;
				let bounty_curator_fee = v1::ChildrenCuratorFees::<T>::take(old_child_bounty_id);
				writes += 1;
				let new_child_bounty_id = ParentTotalChildBounties::<T>::get(parent_bounty_id);
				reads += 1;
				ParentTotalChildBounties::<T>::insert(parent_bounty_id, new_child_bounty_id.saturating_add(1));
				writes += 1;
				// should always be Some
				writes += 1;
				if let Some(taken) = ChildBounties::<T>::take(parent_bounty_id, old_child_bounty_id) {
					writes += 1;
					ChildBounties::<T>::insert(parent_bounty_id, new_child_bounty_id, taken);
				}
				if let Some(bounty_description) = bounty_description {
					writes += 1;
					super::super::ChildBountyDescriptions::<T>::insert(parent_bounty_id, new_child_bounty_id, bounty_description);
				}
				writes += 1;
				super::super::ChildrenCuratorFees::<T>::insert(parent_bounty_id, new_child_bounty_id, bounty_curator_fee);
			}

			T::DbWeight::get().reads_writes(reads, writes)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			Ok(().encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			Ok(())
		}
	}
}

/// Migrate the pallet storage from `0` to `1`.
pub type MigrateV0ToV1<T> = frame_support::migrations::VersionedMigration<
	0,
	1,
	v1::MigrateToV1Impl<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;