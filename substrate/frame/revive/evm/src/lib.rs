//!Types, and traits to integrate pallet-revive with EVM.
#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

/// EVM JSON-RPC API types.
pub mod api;

/// Runtime utilities for integrating pallet-revive with the EVM.
pub mod runtime;
