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

use crate::{vm::evm::Halt, Config, Error};
use alloc::vec::Vec;
use core::ops::{ControlFlow, Range};

/// EVM memory implementation
#[derive(Debug, Clone)]
pub struct Memory<T> {
	data: Vec<u8>,
	_phantom: core::marker::PhantomData<T>,
}

impl<T: Config> Memory<T> {
	/// Create a new empty memory
	pub fn new() -> Self {
		Self { data: Vec::with_capacity(4 * 1024), _phantom: core::marker::PhantomData }
	}

	/// Get a slice of memory for the given range
	///
	/// # Panics
	///
	/// Panics on out of bounds,if the range is non-empty.
	pub fn slice(&self, range: Range<usize>) -> &[u8] {
		if range.is_empty() {
			return &[]
		}
		&self.data[range]
	}

	/// Get a mutable slice of memory for the given range
	///
	/// # Panics
	///
	/// Panics on out of bounds.
	pub fn slice_mut(&mut self, offset: usize, len: usize) -> &mut [u8] {
		&mut self.data[offset..offset + len]
	}

	/// Get the current memory size in bytes
	pub fn size(&self) -> usize {
		self.data.len()
	}

	/// Resize memory to accommodate the given offset and length
	pub fn resize(&mut self, offset: usize, len: usize) -> ControlFlow<Halt> {
		let current_len = self.data.len();
		let target_len = revm::interpreter::num_words(offset.saturating_add(len)) * 32;
		if target_len > crate::limits::EVM_MEMORY_BYTES as usize {
			log::debug!(target: crate::LOG_TARGET, "check memory bounds failed: offset={offset} target_len={target_len} current_len={current_len}");
			return ControlFlow::Break(Error::<T>::OutOfGas.into());
		}

		if target_len > current_len {
			self.data.resize(target_len, 0);
		}

		ControlFlow::Continue(())
	}

	/// Set memory at the given `offset`
	///
	/// # Panics
	///
	/// Panics on out of bounds.
	pub fn set(&mut self, offset: usize, data: &[u8]) {
		if !data.is_empty() {
			self.data[offset..offset + data.len()].copy_from_slice(data);
		}
	}

	/// Set memory from data. Our memory offset+len is expected to be correct but we
	/// are doing bound checks on data/data_offset/len and zeroing parts that is not copied.
	///
	/// # Panics
	///
	/// Panics on out of bounds.
	pub fn set_data(&mut self, memory_offset: usize, data_offset: usize, len: usize, data: &[u8]) {
		if data_offset >= data.len() {
			// nullify all memory slots
			self.slice_mut(memory_offset, len).fill(0);
			return;
		}
		let data_end = core::cmp::min(data_offset + len, data.len());
		let data_len = data_end - data_offset;

		self.slice_mut(memory_offset, data_len)
			.copy_from_slice(&data[data_offset..data_end]);

		// nullify rest of memory slots
		// Safety: Memory is assumed to be valid. And it is commented where that assumption is
		// made
		self.slice_mut(memory_offset + data_len, len - data_len).fill(0);
	}

	/// Returns a byte slice of the memory region at the given offset.
	///
	/// # Panics
	///
	/// Panics on out of bounds.
	pub fn slice_len(&self, offset: usize, size: usize) -> &[u8] {
		&self.data[offset..offset + size]
	}

	/// Copy data within memory from src to dst
	///
	/// # Panics
	///
	/// Panics if range is out of scope of allocated memory.
	pub fn copy(&mut self, dst: usize, src: usize, len: usize) {
		self.data.copy_within(src..src + len, dst);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::tests::Test;

	#[test]
	fn test_memory_resize() {
		let mut memory = Memory::<Test>::new();
		assert_eq!(memory.size(), 0);

		assert!(memory.resize(0, 100).is_continue());
		assert_eq!(memory.size(), 128); // Should be word-aligned (4 words * 32 bytes)

		// Resizing to smaller size should not shrink
		assert!(memory.resize(0, 50).is_continue());
		assert_eq!(memory.size(), 128); // Should stay the same
	}

	#[test]
	fn test_set_get() {
		let mut memory = Memory::<Test>::new();
		memory.data.resize(100, 0);

		let data = b"Hello, World!";
		memory.set(10, data);

		assert_eq!(memory.slice(10..10 + data.len()), data);
	}

	#[test]
	fn test_memory_copy() {
		let mut memory = Memory::<Test>::new();
		memory.data.resize(100, 0);

		// Set some initial data
		memory.set(0, b"Hello");
		memory.set(10, b"World");

		// Copy "Hello" to position 20
		memory.copy(20, 0, 5);

		assert_eq!(memory.slice(20..25), b"Hello");
		assert_eq!(memory.slice(0..5), b"Hello"); // Original should still be there
	}

	#[test]
	fn test_set_data() {
		let mut memory = Memory::<Test>::new();
		memory.data.resize(100, 0);

		let source_data = b"Hello World";

		memory.set_data(5, 0, 5, source_data);
		assert_eq!(memory.slice(5..10), b"Hello");

		memory.set_data(15, 6, 5, source_data);
		assert_eq!(memory.slice(15..20), b"World");
	}
}
