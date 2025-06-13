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

use core::alloc::{GlobalAlloc, Layout};

/// Allocator used by Substrate from within the runtime.
struct RuntimeAllocator;

#[global_allocator]
static ALLOCATOR: RuntimeAllocator = RuntimeAllocator;

unsafe impl GlobalAlloc for RuntimeAllocator {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		let size = layout.size();
        let align = layout.align();

		let alloc_size = size + align; // this holds one byte too much since align is at least 1

		let ptr = crate::allocator::malloc(alloc_size as u32);

		// size = 10, align = 4
		// -------------|-- 14 --
		//       malloc ^

		// Align the pointer
		let missing_bytes = align - ((ptr as usize + 1) % align);

		if missing_bytes > 255 {
			panic!("works");
		}
		//let power = find_power_of_two(align);
		//*(ptr as *mut u8) = power;
		let aligned_ptr = ptr.add(1).add(missing_bytes);

		*((aligned_ptr as usize - 1) as *mut u8) = missing_bytes as u8; // unsafe u8 -> usize case

		if (aligned_ptr as usize) % align != 0 {
			panic!("not aligned stupid");
		}

		if *((aligned_ptr as usize - 1) as *mut u8) != missing_bytes as u8 {
			panic!("missing bytes written wrong");
		}

		// Return the aligned pointer
		aligned_ptr as *mut u8
	}

	unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
		if ptr == core::ptr::null_mut() {
			panic!("Pointer that was passed to dealloc is null");
		}

		if (ptr as usize) % layout.align() != 0 {
			panic!("wrong dealloc align")
		}

		let too_much_bytes = *((ptr as usize - 1) as *mut u8);

		let header_ptr = ptr.sub(too_much_bytes as usize);
		crate::allocator::free(header_ptr)
	}
}

/// Find first one bit from the right to left
fn find_power_of_two(mut n: usize) -> u8 {
	let mut i = 0;

	while n > 0 {
		n >>= 1;
		i += 1;
	}
	
	i
}
