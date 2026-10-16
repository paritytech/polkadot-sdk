// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The `input` host call: reading the refine's input data from within the runtime.

#[cfg(not(substrate_runtime))]
use sp_jam_runtime_interface::FunctionContext;

sp_jam_runtime_interface::host_calls! {
	/// Read the input data into the `buffer_len` bytes at `buffer`, returning the full length
	/// of the data.
	///
	/// The node side is wired by the later executor PR; master's `Externalities` has no
	/// input-data accessor, so the node body traps for now.
	#[index = 340]
	fn ext_input_read(ctx: &mut dyn FunctionContext, buffer: *mut u8, buffer_len: u64) -> u32 {
		let _ = (buffer, buffer_len);
		Err("input::read is provided by the executor".into())
	}
}

/// Read the input data into `buffer`, returning the full length of the data.
///
/// The node side is wired by the later executor PR; this traps for now.
pub fn read(buffer: &mut [u8]) -> u32 {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointer and length come from a live slice.
	return unsafe { ext_input_read(buffer.as_mut_ptr(), buffer.len() as u64) };
	#[cfg(not(substrate_runtime))]
	{
		let _ = buffer;
		unimplemented!("input::read is provided by the executor")
	}
}
