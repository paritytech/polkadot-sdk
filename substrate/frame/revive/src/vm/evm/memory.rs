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

use crate::{
	Config, Error, RuntimeCosts,
	vm::{Ext, evm::Halt},
};
use alloc::vec::Vec;
use core::ops::{ControlFlow, Range};

/// EVM memory implementation
#[derive(Debug)]
pub struct Memory<T> {
	data: Vec<u8>,
	/// Requested capacity. Allocator-provided spare space must not change the charging schedule.
	capacity: usize,
	_phantom: core::marker::PhantomData<T>,
}

impl<T: Config> Memory<T> {
	/// Create a new empty memory
	pub fn new() -> Self {
		Self {
			data: Vec::with_capacity(4 * 1024),
			capacity: 4 * 1024,
			_phantom: core::marker::PhantomData,
		}
	}

	/// Get a slice of memory for the given range
	///
	/// # Panics
	///
	/// Panics on out of bounds,if the range is non-empty.
	pub fn slice(&self, range: Range<usize>) -> &[u8] {
		if range.is_empty() {
			return &[];
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

	fn get_word(&self, offset: usize) -> &[u8; 32] {
		self.data[offset..offset + 32].try_into().unwrap()
	}

	/// Get the current memory size in bytes
	pub fn size(&self) -> usize {
		self.data.len()
	}

	/// Resize memory to accommodate the given offset and length.
	///
	/// Charge before allocating, copying, or zeroing. Capacity grows in powers of two up to the
	/// memory limit, independently of the allocator's growth strategy or spare capacity.
	pub fn resize(
		&mut self,
		offset: usize,
		len: usize,
		ext: &mut impl Ext<T = T>,
	) -> ControlFlow<Halt> {
		if len == 0 {
			return ControlFlow::Continue(());
		}
		let end = offset.saturating_add(len);
		if end > crate::limits::EVM_MEMORY_BYTES as usize {
			log::debug!(target: crate::LOG_TARGET, "check memory bounds failed: offset={offset} len={len}");
			return ControlFlow::Break(Error::<T>::OutOfGas.into());
		}

		let current_len = self.data.len();
		let target_len = end.next_multiple_of(32);
		if target_len <= current_len {
			return ControlFlow::Continue(());
		}

		let allocation =
			if target_len > self.capacity { target_len.next_power_of_two() } else { 0 };
		ext.charge_or_halt(RuntimeCosts::EvmMemoryGrow {
			words: ((target_len - current_len) / 32) as u32,
			allocation_words: (allocation / 32) as u32,
		})?;

		if allocation != 0 {
			self.reallocate((allocation / 32) as u32)?;
		}
		self.grow((target_len / 32) as u32)
	}

	/// Expose zeroed memory through `words` without allocating.
	///
	/// The caller charges for growth first. The requested size must fit the existing capacity.
	fn grow(&mut self, words: u32) -> ControlFlow<Halt> {
		if words > (self.capacity / 32) as u32 {
			return ControlFlow::Break(Error::<T>::OutOfGas.into());
		}
		let len = words as usize * 32;
		if len > self.data.len() {
			self.data.resize(len, 0);
		}
		ControlFlow::Continue(())
	}

	/// Replace the allocation with room for `words`, preserving live bytes without zeroing.
	///
	/// The caller charges for allocation and copying first. Production capacity at least doubles,
	/// so live bytes occupy at most half the requested replacement capacity.
	fn reallocate(&mut self, words: u32) -> ControlFlow<Halt> {
		if words > crate::limits::EVM_MEMORY_BYTES / 32 || words <= (self.capacity / 32) as u32 {
			return ControlFlow::Break(Error::<T>::OutOfGas.into());
		}
		let capacity = words as usize * 32;
		let mut data = Vec::with_capacity(capacity);
		data.extend_from_slice(&self.data);
		self.data = data;
		self.capacity = capacity;
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

	/// Charge for copying the given memory region out, then copy it into a new output buffer.
	///
	/// The caller ensures the region is exposed.
	///
	/// # Panics
	///
	/// Panics on out of bounds.
	pub fn copy_out(
		&self,
		offset: usize,
		len: usize,
		ext: &mut impl Ext<T = T>,
	) -> ControlFlow<Halt, Vec<u8>> {
		ext.charge_or_halt(RuntimeCosts::EvmMemoryCopyOut(len as u32))?;
		ControlFlow::Continue(self.copy_out_uncharged(offset, len))
	}

	/// Allocate an output buffer and copy the given memory region into it.
	///
	/// The caller charges for copying first.
	fn copy_out_uncharged(&self, offset: usize, len: usize) -> Vec<u8> {
		self.slice_len(offset, len).to_vec()
	}

	/// Copy data within memory from src to dst
	///
	/// # Panics
	///
	/// Panics if range is out of scope of allocated memory.
	pub fn copy(&mut self, dst: usize, src: usize, len: usize) {
		self.data.copy_within(src..src + len, dst);
	}

	/// Returns a snapshot of the memory in 32-byte chunks, limited to the specified number of
	/// chunks.
	pub fn snapshot(&self, limit: usize) -> Vec<crate::evm::Bytes> {
		let mut memory_bytes = Vec::new();
		let words_to_read = core::cmp::min(self.size().div_ceil(32), limit);

		for i in 0..words_to_read {
			let word = self.get_word(i * 32);
			memory_bytes.push(crate::evm::Bytes(word.to_vec()));
		}

		memory_bytes
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl<T: Config> Memory<T> {
	/// [`Self::grow`] without a charge, for the `evm_memory_grow` benchmark.
	pub fn bench_grow(&mut self, words: u32) -> ControlFlow<Halt> {
		self.grow(words)
	}

	/// [`Self::reallocate`] without a charge, for the `evm_memory_reallocate` benchmark.
	pub fn bench_reallocate(&mut self, words: u32) -> ControlFlow<Halt> {
		self.reallocate(words)
	}

	/// [`Self::copy_out`] without a charge, for the `evm_memory_copy_out` benchmark.
	pub fn bench_copy_out(&self, offset: usize, len: usize) -> Vec<u8> {
		self.copy_out_uncharged(offset, len)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{exec::PrecompileExt, tests::Test};

	#[test]
	fn test_memory_resize() {
		crate::tests::ExtBuilder::default().build().execute_with(|| {
			let mut setup = crate::call_builder::CallSetup::<Test>::new(
				crate::call_builder::VmBinaryModule::evm_noop(0),
			);
			let (mut ext, _) = setup.ext();
			let mut memory = Memory::<Test>::new();
			assert_eq!(memory.size(), 0);

			assert!(memory.resize(0, 100, &mut ext).is_continue());
			assert_eq!(memory.size(), 128);
			let charged = ext.frame_meter().weight_consumed();
			assert!(memory.resize(0, 50, &mut ext).is_continue());
			assert_eq!(memory.size(), 128);
			assert_eq!(ext.frame_meter().weight_consumed(), charged);
			assert!(memory.resize(usize::MAX, 0, &mut ext).is_continue());
			assert!(memory.resize(usize::MAX, 32, &mut ext).is_break());
			assert_eq!(memory.size(), 128);
			assert_eq!(ext.frame_meter().weight_consumed(), charged);
		});
	}

	#[test]
	fn growth_is_charged_before_memory_changes() {
		crate::tests::ExtBuilder::default().build().execute_with(|| {
			let mut setup = crate::call_builder::CallSetup::<Test>::new(
				crate::call_builder::VmBinaryModule::evm_noop(0),
			);
			let (mut ext, _) = setup.ext();
			let mut memory = Memory::<Test>::new();
			assert!(memory.resize(0, 32, &mut ext).is_continue());
			memory.set(0, &[0xa5; 32]);

			for target in [4096, 4128, 4160, 8192, 8224, crate::limits::EVM_MEMORY_BYTES as usize] {
				let previous = memory.size();
				let allocation =
					if target > memory.capacity { target.next_power_of_two() } else { 0 };
				let cost = RuntimeCosts::EvmMemoryGrow {
					words: ((target - previous) / 32) as u32,
					allocation_words: (allocation / 32) as u32,
				};
				let before = ext.frame_meter().weight_consumed();
				assert!(memory.resize(0, target, &mut ext).is_continue());
				assert_eq!(
					ext.frame_meter().weight_consumed().saturating_sub(before),
					<RuntimeCosts as crate::metering::Token<Test>>::weight(&cost),
				);
				assert_eq!(memory.slice(0..32), &[0xa5; 32]);
				assert!(memory.slice(previous..target).iter().all(|&byte| byte == 0));
			}
		});
	}

	#[test]
	fn failed_growth_keeps_memory_unchanged() {
		crate::tests::ExtBuilder::default().build().execute_with(|| {
			let mut setup = crate::call_builder::CallSetup::<Test>::new(
				crate::call_builder::VmBinaryModule::evm_noop(0),
			);
			let (mut ext, _) = setup.ext();
			let mut memory = Memory::<Test>::new();
			assert!(memory.resize(0, 32, &mut ext).is_continue());
			memory.set(0, &[0xa5; 32]);
			let address = memory.data.as_ptr();
			let meter = ext
				.frame_meter()
				.new_nested(&crate::exec::CallResources::WeightDeposit {
					weight: crate::Weight::from_parts(100_000_000, 0),
					deposit_limit: 0,
				})
				.expect("finite weight limit fits within the parent meter; qed");
			*ext.frame_meter_mut() = meter;
			ext.frame_meter_mut().consume_all_weight();

			// Failure within capacity and failure requiring allocation must both leave memory
			// intact.
			for target in [64, 4128] {
				assert_eq!(
					memory.resize(0, target, &mut ext),
					ControlFlow::Break(Error::<Test>::OutOfGas.into()),
				);
				assert_eq!(memory.size(), 32);
				assert_eq!(memory.capacity, 4096);
				assert_eq!(memory.data.as_ptr(), address);
				assert_eq!(memory.slice(0..32), &[0xa5; 32]);
			}
		});
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
