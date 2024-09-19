//!Types, and traits to integrate pallet-revive with EVM.
#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

pub mod api;
pub mod runtime;

#[cfg(feature = "std")]
pub use secp256k1;
