pub mod migration {
    use super::*;
    use frame_support::traits::StorageVersion;
    use frame_support::dispatch::VersionedCall;
    use frame_support::storage::migration::move_storage_from_pallet;

    pub fn migrate<T: Config>() -> frame_support::weights::Weight {
        let mut weight = 0;

        // Migrate CallIndices
        if StorageVersion::get::<Pallet<T>>() == 0 {
            move_storage_from_pallet(
                b"CallIndices",
                b"Lottery",
                b"CallIndices",
                |old_data: Vec<<T as Config>::RuntimeCall>| {
                    let new_data: Vec<VersionedCall<<T as Config>::RuntimeCall>> = old_data.into_iter().map(|call| {
                        VersionedCall::new(call, <T as frame_system::Config>::Version::get().transaction_version)
                    }).collect();
                    new_data
                },
            );
            weight += T::DbWeight::get().reads_writes(1, 1);
        }

        // Update storage version
        StorageVersion::new(1).put::<Pallet<T>>();
        weight += T::DbWeight::get().writes(1);

        weight
    }
}