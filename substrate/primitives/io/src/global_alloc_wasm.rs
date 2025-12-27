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

use core::{
	alloc::{GlobalAlloc, Layout},
	ptr,
};

/// The type used to store the offset between the real pointer and the returned pointer.
type Offset = u16;

/// The length of [`Offset`].
const OFFSET_LENGTH: usize = core::mem::size_of::<Offset>();

/// Allocator used by Substrate from within the runtime.
///
/// The allocator needs to align the returned pointer to given layout. We assume that on the host
/// side the freeing-bump allocator is used with a fixed alignment of `8` and a `HEADER_SIZE` of
/// `8`. The freeing-bump allocator is storing the header in the 8 bytes before the actual pointer
/// returned by `alloc`. The problem is that the runtime not only sees pointers allocated by this
/// `RuntimeAllocator`, but also pointers allocated by the host. The header is stored as a
/// little-endian `u64`. The allocation header consists of 8 bytes. The first four bytes (as written
/// in memory) are used to store the order of the allocation (or the link to the next slot, if
/// unallocated). Then the least significant bit of the next byte determines whether a given slot is
/// occupied or free, and the last three bytes are unused.
///
/// The `RuntimeAllocator` aligns the pointer to the required alignment before returning it to the
/// user code. As we are assuming the freeing-bump allocator that already aligns by `8` by default,
/// we only need to take care of alignments above `8`. The offset is stored in two bytes before the
/// pointer that we return to the user. Depending on the alignment, we may write into the header,
/// but given the assumptions above this should be no problem.
struct RuntimeAllocator;

#[global_allocator]
static ALLOCATOR: RuntimeAllocator = RuntimeAllocator;

unsafe impl GlobalAlloc for RuntimeAllocator {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		let align = layout.align();
		let size = layout.size();

		// Allocate for the required size, plus a potential alignment.
		//
		// As the host side already aligns the pointer by `8`, we only need to account for any
		// excess.
		let ptr = crate::allocator::malloc((size + align.saturating_sub(8)) as u32);

		// Calculate the required alignment.
		let ptr_offset = ptr.align_offset(align);

		// Should never happen, but just to be sure.
		if ptr_offset > u16::MAX as usize || ptr.is_null() {
			return ptr::null_mut()
		}

		// Align the pointer.
		let ptr = ptr.add(ptr_offset);

		unsafe {
			(ptr.sub(OFFSET_LENGTH) as *mut Offset).write_unaligned(ptr_offset as Offset);
		}

		ptr
	}

	unsafe fn dealloc(&self, ptr: *mut u8, _: Layout) {
		let offset = unsafe { (ptr.sub(OFFSET_LENGTH) as *const Offset).read_unaligned() };

		crate::allocator::free(ptr.sub(offset as usize))
	}
}
