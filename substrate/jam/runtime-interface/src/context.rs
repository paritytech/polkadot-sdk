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

use alloc::{format, string::String, vec, vec::Vec};

/// Access to the guest memory from the node side of a host call.
///
/// Pointers are addresses in the guest memory.
pub trait FunctionContext {
	/// Reads `dest.len()` bytes at `ptr` into `dest`.
	fn read_memory_into(&mut self, ptr: *const u8, dest: &mut [u8]) -> Result<(), String>;

	/// Writes `data` at `ptr`.
	fn write_memory(&mut self, ptr: *mut u8, data: &[u8]) -> Result<(), String>;
}

impl dyn FunctionContext + '_ {
	/// Reads `len` bytes at `ptr`.
	pub fn read_memory(&mut self, ptr: *const u8, len: u64) -> Result<Vec<u8>, String> {
		let len = u32::try_from(len)
			.map_err(|_| format!("length {len} exceeds the guest address space"))?;
		let mut data = vec![0; len as usize];
		self.read_memory_into(ptr, &mut data)?;
		Ok(data)
	}

	/// Reads `N` bytes at `ptr`.
	pub fn read_array<const N: usize>(&mut self, ptr: *const u8) -> Result<[u8; N], String> {
		let mut data = [0; N];
		self.read_memory_into(ptr, &mut data)?;
		Ok(data)
	}

	/// Runs `f` on a copy of the `len` bytes at `ptr` and writes the copy back.
	///
	/// Bytes `f` doesn't touch keep their value.
	pub fn with_buffer<R>(
		&mut self,
		ptr: *mut u8,
		len: u64,
		f: impl FnOnce(&mut [u8]) -> R,
	) -> Result<R, String> {
		let mut buffer = self.read_memory(ptr.cast_const(), len)?;
		let result = f(&mut buffer);
		self.write_memory(ptr, &buffer)?;
		Ok(result)
	}
}
