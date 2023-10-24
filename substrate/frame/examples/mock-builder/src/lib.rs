//! Silly pallet to show how to test it with mock-builder
//!
//! The pallet allows to create auctions once a certain deposit is reached and some time has been
//! passed from the last deposit.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{
		pallet_prelude::*,
		traits::{Currency, ReservableCurrency, Time},
	};
	use frame_system::pallet_prelude::*;
	use polkadot_runtime_common::traits::Auctioneer;

	type MomentOf<T> = <<T as Config>::Time as Time>::Moment;
	type BalanceOf<T> =
		<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Time: Time;
		type Currency: ReservableCurrency<Self::AccountId>;
		type Auction: Auctioneer<BlockNumberFor<Self>, LeasePeriod = BlockNumberFor<Self>>;

		#[pallet::constant]
		type ExpectedAmount: Get<BalanceOf<Self>>;

		#[pallet::constant]
		type WaitingTime: Get<MomentOf<Self>>;

		#[pallet::constant]
		type Period: Get<BlockNumberFor<Self>>;
	}

	#[pallet::storage]
	pub type LastDeposit<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, MomentOf<T>>;

	#[pallet::error]
	pub enum Error<T> {
		NotEnoughDeposit,
		NotEnoughWaiting,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::default())]
		pub fn make_reserve(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			T::Currency::reserve(&who, amount)?;
			LastDeposit::<T>::insert(who, T::Time::now());

			Ok(())
		}

		/// To create an auction we need to fullfull the following non-sense conditions:
		/// - origin has T::ExpectedAmount reserved.
		/// - T::WaitingTime has passed from the last make_deposit call.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::default())]
		pub fn create_auction(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// Check reserve
			let current = T::Currency::reserved_balance(&who);
			ensure!(current >= T::ExpectedAmount::get(), Error::<T>::NotEnoughDeposit);

			// Check time
			let now = T::Time::now();
			let ready_at = T::WaitingTime::get() + LastDeposit::<T>::get(who).unwrap_or(now);
			ensure!(now >= ready_at, Error::<T>::NotEnoughWaiting);

			let block = frame_system::Pallet::<T>::block_number();
			T::Auction::new_auction(block, T::Period::get())?;

			Ok(())
		}
	}
}
