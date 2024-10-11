use frame_support::pallet_macros::{import_section, pallet_section};

#[pallet_section]
pub mod storage {
    #[pallet::storage]
    pub type TotalIssuance1<T> = StorageValue<_, u64, ValueQuery>;
    #[pallet::storage]
    pub type TotalIssuance2<T> = StorageValue<_, u64, ValueQuery>;
}

#[import_section(storage)]
#[frame_support::pallet]
pub mod pallet {
    use frame_support::pallet_prelude::*;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(7);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
    }
}

fn main() {
    println!("Pallet section storage works!");
}
