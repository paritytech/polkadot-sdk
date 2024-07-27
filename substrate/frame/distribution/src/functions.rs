pub use super::*;
impl<T: Config> Pallet<T> {


	pub fn pot_account() -> T::AccountId{
		// Get Pot account
		let pot_id = T::PotId::get();
		let pot_account: T::AccountId = pot_id.into_account_truncating();
		pot_account
	}

	

    /// Series of checks on the Pot, to ensure that we have enough funds
	/// before executing a spending
	pub fn pot_check(amount: BalanceOf<T>) -> DispatchResult {
		
		// Get Pot account		
		let pot_account: T::AccountId = Self::pot_account();

		// Check that the Pot as enough funds for the transfer
        let balance = T::NativeBalance::balance(&pot_account);
        let minimum_balance = T::NativeBalance::minimum_balance();
		let remaining_balance = balance.saturating_sub(amount);

		ensure!(remaining_balance > minimum_balance, Error::<T>::InsufficientPotReserves);
		ensure!(balance > amount, Error::<T>::InsufficientPotReserves);
		Ok(())
	}


	/// Funds transfer from the Pot to a project account
	pub fn spending(
		amount: BalanceOf<T>,
		beneficiary: T::AccountId,
		spending_index: u32,
	) -> DispatchResult {

		// Get Pot account
		let pot_account: T::AccountId = Self::pot_account();

		//Operate the transfer
		let result = T::NativeBalance::transfer(
			&pot_account,
			&beneficiary,
			amount,
			Preservation::Preserve,
		)
		.map_err(|_| Error::<T>::TransferFailed);

		// ToDo 
		// Change the spending status accordingly


		Ok(())
	}

	// ToDo
	// At the beginning of every Epoch, populate the `Spendings` storage from the `Projects` storage (populated by an external process/pallet)
	// make sure that there is enough funds before creating a new `SpendingInfo`, and `ProjectInfo`
	// corresponding to a created `SpendingInfo` should be removed from the `Projects` storage.
	// This is also a good place to lock the funds for created `SpendingInfos`.


}