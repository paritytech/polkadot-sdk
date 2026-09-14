// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Runtime API for PVQ (PolkaVM Query).
//!
//! Defines [`PvqApi`] for executing PolkaVM programs within the runtime.
//!
//! # Guest ABI
//!
//! The `args` parameter to [`PvqApi::execute_query`] is SCALE-encoded as:
//!
//! ```text
//! [selector: u32][arg1][arg2]...[argN]
//! ```
//!
//! Where `selector` identifies the runtime extension to invoke.
//!
//! # Safety
//!
//! Guest programs run sandboxed with gas metering. Invalid programs return [`PvqError`].

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
