use frame::prelude::*;

#[pallet_section]
mod error {
	#[docify::export]
	#[pallet::error]
	pub enum Error {
		/// Account is non-existent
		NonExistentAccount,
		/// Account does not have enough balance
		NotEnoughBalance,
	}
}
