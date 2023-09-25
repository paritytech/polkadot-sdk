//! # Shell Pallet
//!
//! A pallet with minimal functionality to use as a starting point when creating a new FRAME pallet.
//! WIP

// We make sure this pallet uses `no_std` for compiling to Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

// Re-export pallet items so that they can be accessed from the crate namespace.
pub use pallet::*;

/// All pallet logic is defined in its own module and must be annotated by the `pallet` attribute.
/// The macros in a FRAME pallet are used to build the `Pallet` struct which implements traits, methods and callable functions a pallet author creates.
/// With this pattern, a pallet can call internal functions using `Self` for e.g.:
/// ```nocompile
/// Self::deposit_event(..)
/// Self::internal_function(..) // assuming we have some function called `internal_function`
/// ```
///
/// A pallet can also use types and public functions from another pallet, for e.g.:
/// ```nocompile
/// use pallet::*;
/// ```

#[frame_support::pallet]
pub mod pallet {
	// Import various useful types required by all FRAME pallets.
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	// The `Pallet` struct serves as a placeholder to implement traits, methods and dispatchables in
	// this pallet.
	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// The pallet's configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		
		// Types that your pallet depends on go here.
		// See this example pallet on how you may want to configure your pallet:
		// https://paritytech.github.io/polkadot-sdk/master/pallet_default_config_example/index.html

	}

	/// The pallet's callable functions.
	#[pallet::call]
	impl<T: Config> Pallet<T> {

		// Your pallet's callable functions go here.
		// Read the reference material on the different callable functions you can create here.
		
	}
}
