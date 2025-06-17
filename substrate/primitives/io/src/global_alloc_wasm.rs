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

/// The length of the offset that is stored at the end of our allocation.
const OFFSET_LENGTH: usize = 4;

/// Allocator used by Substrate from within the runtime.
///
/// The allocator needs to align the returned pointer to given layout. We assume that on the host
/// side the freeing-bump allocator is used with a fixed alignment of `8` and a `HEADER_SIZE` of
/// `8`. The freeing-bump allocator is storing the header in the 8 bytes before the actual pointer
/// returned by `alloc`. The problem is that the runtime not only sees pointers allocated by this
/// `RuntimeAllocator`, but also pointers allocated by the host. To distinguish between a pointer
/// allocated on the host side and a pointer allocated by `RuntimeAllocator`, we store a special tag
/// `b10000000` at the last byte of the header. The header is stored as a little-endian `u64`.
/// `0x00000001_00000000` is the representation of an occupied header (aka when it is used, which is
/// the case after calling `alloc`). So, the most significant byte should be stored at `ptr - 1` and
/// should always be `0` by default. This allows us to distinguish on what allocated the pointer.
///
/// The `RuntimeAllocator` aligns the pointer to the required alignment before returning it to the
/// user code. The offset is stored at the end of the allocated area in `OFFSET_LENGTH` bytes. When
/// deallocating a pointer, we check if the pointer was allocated by the `RuntimeAllocator`. If this
/// is the case, the pointer is corrected with the stored `offset`. This is required for the host
/// side to recognize the pointer.
struct RuntimeAllocator;

#[global_allocator]
static ALLOCATOR: RuntimeAllocator = RuntimeAllocator;

unsafe impl GlobalAlloc for RuntimeAllocator {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		let align = layout.align();
		let size = layout.size();

		// Allocate for the required size, plus a potential alignment and our offset.
		let ptr = crate::allocator::malloc((size + align + OFFSET_LENGTH) as u32);

		let ptr_offset = ptr.align_offset(align);
		let ptr = ptr.add(ptr_offset);

		// Tag this pointer as being allocated by the `RuntimeAllocator`.
		*ptr.sub(1) = 1u8 << 7;

		unsafe {
			let offset = (ptr_offset as u32).to_ne_bytes();
			ptr::copy(offset.as_ptr(), ptr.add(size), OFFSET_LENGTH);
		}

		ptr
	}

	unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
		// Check if the `RuntimeAllocator` allocated this pointer.
		let custom_offset = *ptr.sub(1) & 1u8 << 7 != 0;

		let ptr = if custom_offset {
			let mut offset: [u8; OFFSET_LENGTH] = [0; OFFSET_LENGTH];
			unsafe {
				ptr::copy(ptr.add(layout.size()), offset.as_mut_ptr(), OFFSET_LENGTH);
			}

			let offset = u32::from_ne_bytes(offset);

			ptr.sub(offset as usize)
		} else {
			ptr
		};

		crate::allocator::free(ptr)
	}
}
