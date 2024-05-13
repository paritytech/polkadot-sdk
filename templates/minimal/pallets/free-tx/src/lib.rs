#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{pallet_prelude::*, traits::fungible};
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

use frame_support::dispatch::GetDispatchInfo;
use sp_runtime::traits::Dispatchable;

#[frame_support::pallet]
pub mod pallet {
	use crate::*;
	use frame_system::{pallet_prelude::*, RawOrigin};
	use sp_std::prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Type to access the Balances Pallet.
		type NativeBalance: fungible::Inspect<Self::AccountId>
			+ fungible::Mutate<Self::AccountId>
			+ fungible::hold::Inspect<Self::AccountId>
			+ fungible::hold::Mutate<Self::AccountId>
			+ fungible::freeze::Inspect<Self::AccountId>
			+ fungible::freeze::Mutate<Self::AccountId>;

		/// A type representing all calls available in your runtime.
		type RuntimeCall: Parameter
			+ Dispatchable<RuntimeOrigin = Self::RuntimeOrigin>
			+ GetDispatchInfo;
	}

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
		/// Events should be documented.
		TxSuccess,
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// Error names should be descriptive.
		TxFailed,
		/// Errors should have helpful documentation associated with them.
		StorageOverflow,
	}

	// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// An example dispatchable that charges no fee if successful.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::default())]
		pub fn free_tx(origin: OriginFor<T>, success: bool) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin)?;

			// If this line fails, the user ends up paying a fee.
			ensure!(success, Error::<T>::TxFailed);

			// Deposit a basic event.
			Self::deposit_event(Event::<T>::TxSuccess);

			// This line tells the runtime to refund any fee taken, making the tx free.
			Ok(Pays::No.into())
		}

		/// An example of re-dispatching a call
		#[pallet::call_index(1)]
		#[pallet::weight(call.get_dispatch_info().weight)]
		pub fn redispatch(
			origin: OriginFor<T>,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// Re-dispatch some call on behalf of the caller.
			let res = call.dispatch(RawOrigin::Signed(who).into());

			// Turn the result from the `dispatch` into our expected `DispatchResult` type.
			res.map(|_| ()).map_err(|e| e.error)
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Get the weight of a call.
	pub fn call_weight(call: <T as Config>::RuntimeCall) -> Weight {
		call.get_dispatch_info().weight
	}
}
