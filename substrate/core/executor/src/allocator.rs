// Copyright 2017-2019 Parity Technologies (UK) Ltd.
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

//! This module implements a freeing-bump allocator.
//! See more details at https://github.com/paritytech/substrate/issues/1615.

use log::trace;
use wasmi::MemoryRef;
use wasmi::memory_units::Bytes;

// The pointers need to be aligned to 8 bytes.
const ALIGNMENT: u32 = 8;

// The pointer returned by `allocate()` needs to fulfill the alignment
// requirement. In our case a pointer will always be a multiple of
// 8, as long as the first pointer is aligned to 8 bytes.
// This is because all pointers will contain a 8 byte prefix (the list
// index) and then a subsequent item of 2^x bytes, where x = [3..24].
const N: usize = 22;
const MAX_POSSIBLE_ALLOCATION: u32 = 16777216; // 2^24 bytes

pub struct FreeingBumpHeapAllocator {
	bumper: u32,
	heads: [u32; N],
	heap: MemoryRef,
	max_heap_size: u32,
	ptr_offset: u32,
	total_size: u32,
}

impl FreeingBumpHeapAllocator {

	/// Creates a new allocation heap which follows a freeing-bump strategy.
	/// The maximum size which can be allocated at once is 16 MiB.
	///
	/// # Arguments
	///
	/// * `ptr_offset` - The pointers returned by `allocate()` start from this
	///   offset on. The pointer offset needs to be aligned to a multiple of 8,
	///   hence a padding might be added to align `ptr_offset` properly.
	///
	/// * `heap_size` - The size available to this heap instance (in bytes) for
	///   allocating memory.
	///
	/// * `heap` - A `MemoryRef` to the available `MemoryInstance` which is
	///   used as the heap.
	///
	pub fn new(mem: MemoryRef) -> Self {
		let current_size: Bytes = mem.current_size().into();
		let current_size = current_size.0 as u32;
		let used_size = mem.used_size().0 as u32;
		let heap_size = current_size - used_size;

		let mut ptr_offset = used_size;
		let padding = ptr_offset % ALIGNMENT;
		if padding != 0 {
			ptr_offset += ALIGNMENT - padding;
		}

		FreeingBumpHeapAllocator {
			bumper: 0,
			heads: [0; N],
			heap: mem,
			max_heap_size: heap_size,
			ptr_offset: ptr_offset,
			total_size: 0,
		}
	}

	/// Gets requested number of bytes to allocate and returns a pointer.
	/// The maximum size which can be allocated at once is 16 MiB.
	pub fn allocate(&mut self, size: u32) -> u32 {
		if size > MAX_POSSIBLE_ALLOCATION {
			return 0;
		}

		let size = size.max(8);
		let item_size = size.next_power_of_two();
		if item_size + 8 + self.total_size > self.max_heap_size {
			return 0;
		}

		let list_index = (item_size.trailing_zeros() - 3) as usize;
		let ptr: u32 = if self.heads[list_index] != 0 {
			// Something from the free list
			let item = self.heads[list_index];
			self.heads[list_index] = FreeingBumpHeapAllocator::le_bytes_to_u32(self.get_heap_4bytes(item));
			item + 8
		} else {
			// Nothing to be freed. Bump.
			self.bump(item_size + 8) + 8
		};

		for i in 1..8 { self.set_heap(ptr - i, 255); }

		self.set_heap(ptr - 8, list_index as u8);

		self.total_size = self.total_size + item_size + 8;
		trace!(target: "wasm-heap", "Heap size is {} bytes after allocation", self.total_size);

		self.ptr_offset + ptr
	}

	/// Deallocates the space which was allocated for a pointer.
	pub fn deallocate(&mut self, ptr: u32) {
		let ptr = ptr - self.ptr_offset;

		let list_index = self.get_heap_byte(ptr - 8) as usize;
		for i in 1..8 { debug_assert!(self.get_heap_byte(ptr - i) == 255); }
		let tail = self.heads[list_index];
		self.heads[list_index] = ptr - 8;

		let mut slice = self.get_heap_4bytes(ptr - 8);
		FreeingBumpHeapAllocator::write_u32_into_le_bytes(tail, &mut slice);
		self.set_heap_4bytes(ptr - 8, slice);

		let item_size = FreeingBumpHeapAllocator::get_item_size_from_index(list_index);
		self.total_size = self.total_size.checked_sub(item_size as u32 + 8).unwrap_or(0);
		trace!(target: "wasm-heap", "Heap size is {} bytes after deallocation", self.total_size);
	}

	fn bump(&mut self, n: u32) -> u32 {
		let res = self.bumper;
		self.bumper += n;
		res
	}

	fn le_bytes_to_u32(arr: [u8; 4]) -> u32 {
		let bytes = [arr[0], arr[1], arr[2], arr[3]];
		unsafe { std::mem::transmute::<[u8; 4], u32>(bytes) }.to_le()
	}

	fn write_u32_into_le_bytes(bytes: u32, slice: &mut [u8]) {
		let bytes: [u8; 4] = unsafe { std::mem::transmute::<u32, [u8; 4]>(bytes.to_le()) };
		for i in 0..4 { slice[i] = bytes[i]; }
	}

	fn get_item_size_from_index(index: usize) -> usize {
		// we shift 1 by three places, since the first possible item size is 8
		1 << 3 << index
	}

	fn get_heap_4bytes(&mut self, ptr: u32) -> [u8; 4] {
		let mut arr = [0u8; 4];
		self.heap.get_into(self.ptr_offset + ptr, &mut arr).unwrap();
		arr
	}

	fn get_heap_byte(&mut self, ptr: u32) -> u8 {
		let mut arr = [0u8; 1];
		self.heap.get_into(self.ptr_offset + ptr, &mut arr).unwrap();
		arr[0]
	}

	fn set_heap(&mut self, ptr: u32, value: u8) {
		self.heap.set(self.ptr_offset + ptr, &[value]).unwrap()
	}

	fn set_heap_4bytes(&mut self, ptr: u32, value: [u8; 4]) {
		self.heap.set(self.ptr_offset + ptr, &value).unwrap()
	}

}

#[cfg(test)]
mod tests {
	use super::*;
	use wasmi::MemoryInstance;
	use wasmi::memory_units::*;

	const PAGE_SIZE: u32 = 65536;

	fn set_offset(mem: MemoryRef, offset: usize) {
		let offset: Vec<u8> = vec![255; offset];
		mem.set(0, &offset).unwrap();
	}

	#[test]
	fn should_allocate_properly() {
		// given
		let mem = MemoryInstance::alloc(Pages(1), None).unwrap();
		let mut heap = FreeingBumpHeapAllocator::new(mem);

		// when
		let ptr = heap.allocate(1);

		// then
		assert_eq!(ptr, 8);
	}

	#[test]
	fn should_always_align_pointers_to_multiples_of_8() {
		// given
		let mem = MemoryInstance::alloc(Pages(1), None).unwrap();
		set_offset(mem.clone(), 13);
		let mut heap = FreeingBumpHeapAllocator::new(mem);

		// when
		let ptr = heap.allocate(1);

		// then
		// the pointer must start at the next multiple of 8 from 13
		// + the prefix of 8 bytes.
		assert_eq!(ptr, 24);
	}

	#[test]
	fn should_increment_pointers_properly() {
		// given
		let mem = MemoryInstance::alloc(Pages(1), None).unwrap();
		let mut heap = FreeingBumpHeapAllocator::new(mem);

		// when
		let ptr1 = heap.allocate(1);
		let ptr2 = heap.allocate(9);
		let ptr3 = heap.allocate(1);

		// then
		// a prefix of 8 bytes is prepended to each pointer
		assert_eq!(ptr1, 8);

		// the prefix of 8 bytes + the content of ptr1 padded to the lowest possible
		// item size of 8 bytes + the prefix of ptr1
		assert_eq!(ptr2, 24);

		// ptr2 + its content of 16 bytes + the prefix of 8 bytes
		assert_eq!(ptr3, 24 + 16 + 8);
	}

	#[test]
	fn should_free_properly() {
		// given
		let mem = MemoryInstance::alloc(Pages(1), None).unwrap();
		let mut heap = FreeingBumpHeapAllocator::new(mem);
		let ptr1 = heap.allocate(1);
		// the prefix of 8 bytes is prepended to the pointer
		assert_eq!(ptr1, 8);

		let ptr2 = heap.allocate(1);
		// the prefix of 8 bytes + the content of ptr 1 is prepended to the pointer
		assert_eq!(ptr2, 24);

		// when
		heap.deallocate(ptr2);

		// then
		// then the heads table should contain a pointer to the
		// prefix of ptr2 in the leftmost entry
		assert_eq!(heap.heads[0], ptr2 - 8);
	}

	#[test]
	fn should_deallocate_and_reallocate_properly() {
		// given
		let mem = MemoryInstance::alloc(Pages(1), None).unwrap();
		set_offset(mem.clone(), 13);
		let padded_offset = 16;
		let mut heap = FreeingBumpHeapAllocator::new(mem);

		let ptr1 = heap.allocate(1);
		// the prefix of 8 bytes is prepended to the pointer
		assert_eq!(ptr1, padded_offset + 8);

		let ptr2 = heap.allocate(9);
		// the padded_offset + the previously allocated ptr (8 bytes prefix +
		// 8 bytes content) + the prefix of 8 bytes which is prepended to the
		// current pointer
		assert_eq!(ptr2, padded_offset + 16 + 8);

		// when
		heap.deallocate(ptr2);
		let ptr3 = heap.allocate(9);

		// then
		// should have re-allocated
		assert_eq!(ptr3, padded_offset + 16 + 8);
		assert_eq!(heap.heads, [0; N]);
	}

	#[test]
	fn should_build_linked_list_of_free_areas_properly() {
		// given
		let mem = MemoryInstance::alloc(Pages(1), None).unwrap();
		let mut heap = FreeingBumpHeapAllocator::new(mem);

		let ptr1 = heap.allocate(8);
		let ptr2 = heap.allocate(8);
		let ptr3 = heap.allocate(8);

		// when
		heap.deallocate(ptr1);
		heap.deallocate(ptr2);
		heap.deallocate(ptr3);

		// then
		let mut expected = [0; N];
		expected[0] = ptr3 - 8;
		assert_eq!(heap.heads, expected);

		let ptr4 = heap.allocate(8);
		assert_eq!(ptr4, ptr3);

		expected[0] = ptr2 - 8;
		assert_eq!(heap.heads, expected);
	}

	#[test]
	fn should_not_allocate_if_too_large() {
		// given
		let mem = MemoryInstance::alloc(Pages(1), Some(Pages(1))).unwrap();
		set_offset(mem.clone(), 13);
		let mut heap = FreeingBumpHeapAllocator::new(mem);

		// when
		let ptr = heap.allocate(PAGE_SIZE - 13);

		// then
		assert_eq!(ptr, 0);
	}

	#[test]
	fn should_not_allocate_if_full() {
		// given
		let mem = MemoryInstance::alloc(Pages(1), Some(Pages(1))).unwrap();
		let mut heap = FreeingBumpHeapAllocator::new(mem);
		let ptr1 = heap.allocate((PAGE_SIZE / 2) - 8);
		assert_eq!(ptr1, 8);

		// when
		let ptr2 = heap.allocate(PAGE_SIZE / 2);

		// then
		// there is no room for another half page incl. its 8 byte prefix
		assert_eq!(ptr2, 0);
	}

	#[test]
	fn should_allocate_max_possible_allocation_size() {
		// given
		let pages_needed = (MAX_POSSIBLE_ALLOCATION as usize / PAGE_SIZE as usize) + 1;
		let mem = MemoryInstance::alloc(Pages(pages_needed), Some(Pages(pages_needed))).unwrap();
		let mut heap = FreeingBumpHeapAllocator::new(mem);

		// when
		let ptr = heap.allocate(MAX_POSSIBLE_ALLOCATION);

		// then
		assert_eq!(ptr, 8);
	}

	#[test]
	fn should_not_allocate_if_requested_size_too_large() {
		// given
		let mem = MemoryInstance::alloc(Pages(1), None).unwrap();
		let mut heap = FreeingBumpHeapAllocator::new(mem);

		// when
		let ptr = heap.allocate(MAX_POSSIBLE_ALLOCATION + 1);

		// then
		assert_eq!(ptr, 0);
	}

	#[test]
	fn should_include_prefixes_in_total_heap_size() {
		// given
		let mem = MemoryInstance::alloc(Pages(1), None).unwrap();
		set_offset(mem.clone(), 1);
		let mut heap = FreeingBumpHeapAllocator::new(mem);

		// when
		// an item size of 16 must be used then
		heap.allocate(9);

		// then
		assert_eq!(heap.total_size, 8 + 16);
	}

	#[test]
	fn should_calculate_total_heap_size_to_zero() {
		// given
		let mem = MemoryInstance::alloc(Pages(1), None).unwrap();
		set_offset(mem.clone(), 13);
		let mut heap = FreeingBumpHeapAllocator::new(mem);

		// when
		let ptr = heap.allocate(42);
		assert_eq!(ptr, 16 + 8);
		heap.deallocate(ptr);

		// then
		assert_eq!(heap.total_size, 0);
	}

	#[test]
	fn should_calculate_total_size_of_zero() {
		// given
		let mem = MemoryInstance::alloc(Pages(1), None).unwrap();
		set_offset(mem.clone(), 19);
		let mut heap = FreeingBumpHeapAllocator::new(mem);

		// when
		for _ in 1..10 {
			let ptr = heap.allocate(42);
			heap.deallocate(ptr);
		}

		// then
		assert_eq!(heap.total_size, 0);
	}

	#[test]
	fn should_write_u32_correctly_into_le() {
		// given
		let mut heap = vec![0; 5];

		// when
		FreeingBumpHeapAllocator::write_u32_into_le_bytes(1, &mut heap[0..4]);

		// then
		assert_eq!(heap, [1, 0, 0, 0, 0]);
	}

	#[test]
	fn should_write_u32_max_correctly_into_le() {
		// given
		let mut heap = vec![0; 5];

		// when
		FreeingBumpHeapAllocator::write_u32_into_le_bytes(u32::max_value(), &mut heap[0..4]);

		// then
		assert_eq!(heap, [255, 255, 255, 255, 0]);
	}

	#[test]
	fn should_get_item_size_from_index() {
		// given
		let index = 0;

		// when
		let item_size = FreeingBumpHeapAllocator::get_item_size_from_index(index);

		// then
		assert_eq!(item_size, 8);
	}

	#[test]
	fn should_get_max_item_size_from_index() {
		// given
		let index = 21;

		// when
		let item_size = FreeingBumpHeapAllocator::get_item_size_from_index(index);

		// then
		assert_eq!(item_size as u32, MAX_POSSIBLE_ALLOCATION);
	}

}
