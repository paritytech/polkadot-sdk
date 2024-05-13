#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::traits::fungible;
/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// <https://docs.substrate.io/reference/frame-pallets/>
pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub type BalanceOf<T> = <<T as Config>::NativeBalance as fungible::Inspect<
	<T as frame_system::Config>::AccountId,
>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{pallet_prelude::*, traits::fungible};
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::Convert;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_aura::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Type to access the Balances Pallet.
		type NativeBalance: fungible::Inspect<Self::AccountId>
			+ fungible::Mutate<Self::AccountId>
			+ fungible::hold::Inspect<Self::AccountId>
			+ fungible::hold::Mutate<Self::AccountId>
			+ fungible::freeze::Inspect<Self::AccountId>
			+ fungible::freeze::Mutate<Self::AccountId>;

		/// A conversion which takes an authority id, and returns the associated account id.
		type AuthorityToAccount: Convert<Self::AuthorityId, Self::AccountId>;
	}

	// This storage only appears in tests, and is used to control the fake
	// block author for testing. Whatever this storage is set to, will be
	// the block author.
	#[cfg(test)]
	#[pallet::storage]
	pub type TestBlockAuthor<T: Config> = StorageValue<_, T::AccountId>;

	// The pallet's runtime storage items.
	// https://docs.substrate.io/main-docs/build/runtime-storage/
	#[pallet::storage]
	// Learn more about declaring storage items:
	// https://docs.substrate.io/main-docs/build/runtime-storage/#declaring-storage-items
	pub type Something<T> = StorageValue<_, u32>;

	// Pallets use events to inform users when important changes are made.
	// https://docs.substrate.io/main-docs/build/events-errors/
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Event documentation should end with an array that provides descriptive names for event
		/// parameters. [something, who]
		SomethingStored { something: u32, who: T::AccountId },
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// Too many authorities for Aura's limits.
		TooManyAuthorities,
	}

	// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// An example of directly updating the authorities for Aura.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::default())]
		pub fn force_change_authorities(
			origin: OriginFor<T>,
			who: T::AuthorityId,
		) -> DispatchResult {
			ensure_root(origin)?;
			let mut authorities = BoundedVec::<T::AuthorityId, T::MaxAuthorities>::default();
			authorities.try_push(who).map_err(|_| Error::<T>::TooManyAuthorities)?;
			pallet_aura::Pallet::<T>::change_authorities(authorities);
			Ok(())
		}

		/// An example dispatchable that takes a singles value as a parameter, writes the value to
		/// storage and emits an event. This function must be dispatched by a signed extrinsic.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::default())]
		pub fn do_something(origin: OriginFor<T>, something: u32) -> DispatchResult {
			// Check that the extrinsic was signed and get the signer.
			// This function will return an error if the extrinsic is not signed.
			// https://docs.substrate.io/main-docs/build/origins/
			let who = ensure_signed(origin)?;

			// Update storage.
			<Something<T>>::put(something);

			// Emit an event.
			Self::deposit_event(Event::SomethingStored { something, who });
			// Return a successful DispatchResultWithPostInfo
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		// A function to get you an account id for the current block author.
		pub fn find_author() -> Option<T::AccountId> {
			// Here is some really hacky test code which allows you to control the block author
			// for tests. You can manage this with `set_test_block_author()` provided by this crate.
			#[cfg(test)]
			{
				return TestBlockAuthor::<T>::get()
			}

			// On a real blockchain, you get the author from aura.
			#[cfg(not(test))]
			{
				use frame_support::traits::FindAuthor;
				let digest = frame_system::Pallet::<T>::digest();
				let pre_runtime_digests = digest.logs.iter().filter_map(|d| d.as_pre_runtime());
				let maybe_authority_id =
					pallet_aura::AuraAuthorId::<T>::find_author(pre_runtime_digests);
				maybe_authority_id.map(T::AuthorityToAccount::convert)
			}
		}

		// A helper function for tests to set the block author that will be returned
		// when calling `find_author` in tests.
		#[cfg(test)]
		pub fn set_test_author(who: T::AccountId) {
			TestBlockAuthor::<T>::put(who);
		}
	}
}
