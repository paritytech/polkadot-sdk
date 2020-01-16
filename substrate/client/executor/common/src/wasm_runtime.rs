// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Definitions for a wasm runtime.

use crate::error::Error;
use sp_wasm_interface::Function;

/// A trait that defines an abstract wasm runtime.
///
/// This can be implemented by an execution engine.
pub trait WasmRuntime {
	/// Attempt to update the number of heap pages available during execution.
	///
	/// Returns false if the update cannot be applied. The function is guaranteed to return true if
	/// the heap pages would not change from its current value.
	fn update_heap_pages(&mut self, heap_pages: u64) -> bool;

	/// Return the host functions that are registered for this Wasm runtime.
	fn host_functions(&self) -> &[&'static dyn Function];

	/// Call a method in the Substrate runtime by name. Returns the encoded result on success.
	fn call(&mut self, method: &str, data: &[u8]) -> Result<Vec<u8>, Error>;
}
