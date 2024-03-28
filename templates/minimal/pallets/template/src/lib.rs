//! A shell pallet built with [`polkadot_sdk_frame`].

#![cfg_attr(not(feature = "std"), no_std)]

use polkadot_sdk_frame::prelude::*;

// Re-export all pallet parts, this is needed to properly import the pallet into the runtime.
pub use pallet::*;

#[polkadot_sdk_frame::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);
}
