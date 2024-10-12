use frame_support::pallet_macros::pallet_section;

#[pallet_section]
mod storage {
	#[pallet::storage]
	pub type Value<T> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	pub type Map<T> = StorageMap<_, _, u32, u32, ValueQuery>;
}