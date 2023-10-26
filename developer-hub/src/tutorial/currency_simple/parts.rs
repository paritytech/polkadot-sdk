//! Reusable parts of the currency pallet in different versions.

use frame::prelude::*;

pub(crate) mod storage {
	use super::*;

	#[pallet_section]
	pub(crate) mod storage {
		#[docify::export]
		/// Single storage item, of type `Balance`.
		#[pallet::storage]
		pub type TotalIssuance<T: Config> = StorageValue<_, Balance>;

		#[docify::export]
		/// A mapping from `T::AccountId` to `Balance`
		#[pallet::storage]
		pub type Balances<T: Config> = StorageMap<_, _, T::AccountId, Balance>;
	}
}

pub(crate) mod pallet {
	use super::*;

	#[pallet_section]
	pub(crate) mod pallet {
		#[pallet::pallet]
		pub struct Pallet<T>(_);
	}
}
