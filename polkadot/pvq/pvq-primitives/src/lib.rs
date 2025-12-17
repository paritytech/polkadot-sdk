//! PVQ primitive types shared across PVQ crates.
//!
//! This crate intentionally contains no helpers; it only defines the basic types.
//!
//! - [`PvqResponse`]: raw bytes returned from a PVQ program.
//! - [`PvqResult`]: `Result<PvqResponse, PvqError>`.
//! - [`PvqError`]: `String` with `std` (default), compact enum without `std`.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
#[cfg(not(feature = "std"))]
use codec::{Decode, Encode};
#[cfg(not(feature = "std"))]
use scale_info::TypeInfo;

/// PVQ execution result (`Ok` bytes, `Err` failure).
pub type PvqResult = Result<PvqResponse, PvqError>;

/// Raw bytes returned from a PVQ program.
pub type PvqResponse = Vec<u8>;

/// PVQ execution error (human-readable message).
#[cfg(feature = "std")]
pub type PvqError = String;

/// Error type returned by PVQ execution (compact `no_std` representation).
#[cfg(not(feature = "std"))]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum PvqError {
	/// Failed to decode the query input.
	FailedToDecode,
	/// The PVQ program format is invalid.
	InvalidPvqProgramFormat,
	/// The query exceeds the configured weight/gas limit.
	QueryExceedsWeightLimit,
	/// A trap occurred during execution.
	Trap,
	/// A memory access error occurred.
	MemoryAccessError,
	/// A host call error occurred.
	HostCallError,
	/// Execution was stepped (e.g. single-step / debug mode).
	Step,
	/// An unknown error occurred.
	Other,
}
