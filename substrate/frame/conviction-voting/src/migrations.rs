use frame_support::{migration::remove_storage_prefix, pallet_prelude::*, traits::OnRuntimeUpgrade};

pub struct RemoveZeroBalanceRecords<T>(PhantomData<T>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for RemoveZeroBalanceRecords<T, I> {
    fn on_runtime_upgrade() -> Weight {
        // Remove zero-balance records from VotingFor
        VotingFor::<T, I>::translate(|_who, _class, voting| {
            if voting.is_empty() {
                None
            } else {
                Some(voting)
            }
        });

        // Remove zero-balance records from ClassLocksFor
        ClassLocksFor::<T, I>::translate(|_who, locks| {
            let new_locks = locks.into_iter().filter(|(_, balance)| !balance.is_zero()).collect();
            if new_locks.is_empty() {
                None
            } else {
                Some(new_locks)
            }
        });

        // Return the weight consumed by the migration
        T::DbWeight::get().reads_writes(2, 2)
    }
}