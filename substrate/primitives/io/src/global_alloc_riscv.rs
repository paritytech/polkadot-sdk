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

#[polkavm_derive::polkavm_import]
extern "C" {
	/// Grows the heap by `size` bytes. Returns the previous heap end address,
	/// or zero if the allocation failed. When called with `size == 0`, returns
	/// the current heap end without growing.
	fn grow_heap(size: usize) -> usize;
}

/// A basic leaking allocator backed by the `grow_heap` host call.
///
/// This is a temporary shim: `sbrk` was removed from the `jam_v1` instruction
/// set (GP 0.8.0) and replaced with a `grow_heap` host call. A proper
/// implementation will be provided later.
struct LeakingAllocator;

#[global_allocator]
static ALLOCATOR: LeakingAllocator = LeakingAllocator;

unsafe impl GlobalAlloc for LeakingAllocator {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		let pointer = unsafe { grow_heap(0) };
		let padding = (-(pointer as isize)) as usize & (layout.align() - 1);
		let size = layout.size().wrapping_add(padding);
		if unsafe { grow_heap(size) } == 0 {
			return core::ptr::null_mut();
		}
		(pointer + padding) as *mut u8
	}

	unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}
