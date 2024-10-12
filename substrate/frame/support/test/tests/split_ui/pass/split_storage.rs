use frame_support::pallet_macros::import_section;

mod storage;

#[import_section(storage::storage)]
#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(8);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn increment_value(_origin: OriginFor<T>) -> DispatchResult {
			Value::<T>::mutate(|v| {
				v.saturating_add(1)
			});
			Ok(())
		}
	}
}

fn main() {
}