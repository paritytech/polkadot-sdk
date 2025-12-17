#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use pvq_primitives::PvqResult;

sp_api::decl_runtime_apis! {
	/// Runtime API for PVQ (PolkaVM Query).
	pub trait PvqApi {
		/// Execute a PVQ program with SCALE-encoded call data.
		///
		/// # Arguments
		///
		/// * `program`: PolkaVM bytecode of the guest program.
		/// * `args`: SCALE-encoded call data for the PVQ guest ABI.
		///   See the crate-level docs for the expected layout.
		/// * `gas_limit`: Optional execution gas limit. If `None`, the runtime applies its
		///   default limit/boundary.
		///
		/// # Returns
		///
		/// A [`PvqResult`], where `Ok` contains the guest's response bytes and `Err` indicates
		/// execution or validation failure.
		fn execute_query(program: Vec<u8>, args: Vec<u8>, gas_limit: Option<i64>) -> PvqResult;

		/// Return PVQ extensions metadata as an opaque byte blob.
		///
		/// The encoding and schema are defined by the runtime. See the crate-level docs for a
		/// recommended structure.
		fn metadata() -> Vec<u8>;
	}
}
