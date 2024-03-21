#[frame::pallet(dev_mode)]
pub mod pallet {
	use frame::prelude::*;

	#[docify::export]
	pub type Balance = u128;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[docify::export]
	/// Single storage item, of type `Balance`.
	#[pallet::storage]
	pub type TotalIssuance<T: Config> = StorageValue<_, Balance>;

	#[docify::export]
	/// A mapping from `T::AccountId` to `Balance`
	#[pallet::storage]
	pub type Balances<T: Config> = StorageMap<_, _, T::AccountId, Balance>;

	#[docify::export(impl_pallet)]
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// An unsafe mint that can be called by anyone. Not a great idea.
		pub fn mint_unsafe(
			origin: T::RuntimeOrigin,
			dest: T::AccountId,
			amount: Balance,
		) -> DispatchResult {
			// ensure that this is a signed account, but we don't really check `_anyone`.
			let _anyone = ensure_signed(origin)?;

			// update the balances map. Notice how all `<T: Config>` remains as `<T>`.
			Balances::<T>::mutate(dest, |b| *b = Some(b.unwrap_or(0) + amount));
			// update total issuance.
			TotalIssuance::<T>::mutate(|t| *t = Some(t.unwrap_or(0) + amount));

			Ok(())
		}

		/// Transfer `amount` from `origin` to `dest`.
		pub fn transfer(
			origin: T::RuntimeOrigin,
			dest: T::AccountId,
			amount: Balance,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;

			// ensure sender has enough balance, and if so, calculate what is left after `amount`.
			let sender_balance = Balances::<T>::get(&sender).ok_or("NonExistentAccount")?;
			if sender_balance < amount {
				return Err("NotEnoughBalance".into())
			}
			let reminder = sender_balance - amount;

			// update sender and dest balances.
			Balances::<T>::mutate(dest, |b| *b = Some(b.unwrap_or(0) + amount));
			Balances::<T>::insert(&sender, reminder);

			Ok(())
		}
	}

	#[allow(unused)]
	impl<T: Config> Pallet<T> {
		#[docify::export]
		pub fn transfer_better(
			origin: T::RuntimeOrigin,
			dest: T::AccountId,
			amount: Balance,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;

			let sender_balance = Balances::<T>::get(&sender).ok_or("NonExistentAccount")?;
			ensure!(sender_balance >= amount, "NotEnoughBalance");
			let reminder = sender_balance - amount;

			// .. snip
			Ok(())
		}

		#[docify::export]
		/// Transfer `amount` from `origin` to `dest`.
		pub fn transfer_better_checked(
			origin: T::RuntimeOrigin,
			dest: T::AccountId,
			amount: Balance,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;

			let sender_balance = Balances::<T>::get(&sender).ok_or("NonExistentAccount")?;
			let reminder = sender_balance.checked_sub(amount).ok_or("NotEnoughBalance")?;

			// .. snip
			Ok(())
		}
	}
}
