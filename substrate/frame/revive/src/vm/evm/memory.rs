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

//! Custom EVM memory implementation using standard Vec<u8>

use crate::vm::evm::{Halt, HaltReason};
use alloc::vec::Vec;
use core::ops::{ControlFlow, Range};

/// EVM memory implementation using standard Vec<u8> and sp_core::U256
#[derive(Debug, Clone)]
pub struct Memory(Vec<u8>);

impl Memory {
	/// Create a new empty memory
	pub fn new() -> Self {
		Self(Vec::new())
	}

	/// Get a slice of memory for the given range
	///
	/// # Panics
	///
	/// Panics on out of bounds.
	pub fn slice(&self, range: Range<usize>) -> &[u8] {
		&self.0[range]
	}

	/// Returns a byte slice of the memory region at the given offset.
	///
	/// # Panics
	///
	/// Panics on out of bounds.
	pub fn slice_mut(&mut self, offset: usize, len: usize) -> &mut [u8] {
		&mut self.0[offset..offset + len]
	}

	/// Get the current memory size in bytes
	pub fn size(&self) -> usize {
		self.0.len()
	}

	/// Resize memory to accommodate the given offset and length
	pub fn resize(&mut self, offset: usize, len: usize) -> ControlFlow<Halt> {
		let current_len = self.0.len();
		let target_len = revm::interpreter::num_words(offset.saturating_add(len)) * 32;
		if target_len as u32 > crate::limits::code::BASELINE_MEMORY_LIMIT {
			log::debug!(target: crate::LOG_TARGET, "check memory bounds failed: offset={offset} target_len={target_len} current_len={current_len}");
			return ControlFlow::Break(HaltReason::MemoryOOG.into());
		}

		if target_len > current_len {
			self.0.resize(target_len, 0);
		};

		ControlFlow::Continue(())
	}

	/// Set memory at the given offset with the provided data
	pub fn set(&mut self, offset: usize, data: &[u8]) {
		if data.is_empty() {
			return;
		}
		let end = offset.saturating_add(data.len());
		if end > self.0.len() {
			self.0.resize(end, 0);
		}
		self.0[offset..end].copy_from_slice(data);
	}

	/// Set data in memory from another memory's global slice
	pub fn set_data(&mut self, offset: usize, data_offset: usize, len: usize, data: &[u8]) {
		if len > 0 && data_offset < data.len() {
			let copy_len = core::cmp::min(len, data.len() - data_offset);
			let source = &data[data_offset..data_offset + copy_len];
			self.set(offset, source);
		}
	}

	pub fn slice_len(&self, offset: usize, len: usize) -> &[u8] {
		self.0.get(offset..offset.saturating_add(len)).unwrap_or(&[])
	}

	/// Copy data within memory from src to dst
	pub fn copy(&mut self, dst: usize, src: usize, len: usize) {
		if len == 0 {
			return;
		}

		let max_offset = core::cmp::max(dst.saturating_add(len), src.saturating_add(len));
		if max_offset > self.0.len() {
			self.0.resize(max_offset, 0);
		}

		// Handle overlapping memory regions correctly
		if dst != src {
			self.0.copy_within(src..src + len, dst);
		}
	}
}

impl Default for Memory {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_memory_resize() {
		let mut memory = Memory::new();
		assert_eq!(memory.size(), 0);

		assert!(memory.resize(0, 100).is_continue());
		assert_eq!(memory.size(), 128); // Should be word-aligned (4 words * 32 bytes)

		// Resizing to smaller size should not shrink
		assert!(memory.resize(0, 50).is_continue());
		assert_eq!(memory.size(), 128); // Should stay the same
	}

	#[test]
	fn test_set_get() {
		let mut memory = Memory::new();

		let data = b"Hello, World!";
		memory.set(10, data);

		assert_eq!(memory.slice(10..10 + data.len()), data);
		assert_eq!(memory.size(), 10 + data.len());
	}

	#[test]
	fn test_memory_copy() {
		let mut memory = Memory::new();

		// Set some initial data
		memory.set(0, b"Hello");
		memory.set(10, b"World");

		// Copy "Hello" to position 20
		memory.copy(20, 0, 5);

		assert_eq!(memory.slice(20..25), b"Hello");
		assert_eq!(memory.slice(0..5), b"Hello"); // Original should still be there
	}

	#[test]
	fn test_overlapping_copy() {
		let mut memory = Memory::new();

		memory.set(0, b"HelloWorld");

		// Overlapping copy - move "World" to overlap with "Hello"
		memory.copy(2, 5, 5);

		assert_eq!(memory.slice(0..10), b"HeWorldrld");
	}

	#[test]
	fn test_set_data() {
		let mut memory = Memory::new();
		let source_data = b"Hello World";

		memory.set_data(5, 0, 5, source_data);
		assert_eq!(memory.slice(5..10), b"Hello");

		memory.set_data(15, 6, 5, source_data);
		assert_eq!(memory.slice(15..20), b"World");
	}
}
