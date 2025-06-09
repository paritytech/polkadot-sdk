#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame::pallet]
pub mod pallet {
	extern crate alloc;
	use frame::prelude::*;

	#[pallet::storage]
	#[pallet::getter(fn preset)]
	#[pallet::unbounded]
	pub type Preset<T: Config> = StorageValue<_, alloc::string::String, OptionQuery>;

	#[pallet::genesis_config]
	#[derive(DefaultNoBound, DebugNoBound, CloneNoBound, PartialEqNoBound, EqNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub preset: alloc::string::String,
		pub _marker: core::marker::PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			Preset::<T>::put(self.preset.clone());
		}
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);
}
