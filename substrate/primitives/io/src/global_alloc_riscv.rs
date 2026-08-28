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

//! The PolkaVM (JAM) runtime allocator, backed by [`picoalloc`], mirroring
//! [`crate::global_alloc_wasm`]: `picoalloc` manages a free-list heap inside an address space
//! grown by the JAM `grow_heap` host call. The heap ceiling is the Gray Paper's own limit:
//! `HEAP_SIZE` below is exactly that maximum (`b - a`); growing beyond it is a silent no-op,
//! which surfaces here as an allocation failure.

use core::{
	alloc::{GlobalAlloc, Layout},
	cell::UnsafeCell,
	ptr::NonNull,
};

#[polkavm_derive::polkavm_import]
extern "C" {
	/// Grows the heap by `size` bytes, returning the previous heap end address, or zero if the
	/// allocation failed. When called with `size == 0`, returns the current heap end without
	/// growing. Exceeding the Gray Paper's heap ceiling `b` is a silent no-op.
	///
	/// Index 1 is fixed by the Gray Paper (`Ω_Gemini`), not chosen here.
	#[polkavm_import(index = 1)]
	fn grow_heap(size: usize) -> usize;
}

/// The maximum heap size the Gray Paper allows for a service with empty read-only data and
/// stack: `(b - a) * 2^12` with `a = (2·2^16)/2^12` and `b = (2^32 − 3·2^16 − 2^24)/2^12`,
/// i.e. `2^32 − 5·2^16 − 2^24`.
const HEAP_SIZE: usize = (1 << 32) - 5 * (1 << 16) - (1 << 24);

/// Allocator used by Substrate from within the runtime.
struct RuntimeAllocator;

#[global_allocator]
static ALLOCATOR: RuntimeAllocator = RuntimeAllocator;

impl picoalloc::Env for RuntimeAllocator {
	fn total_space(&self) -> picoalloc::Size {
		picoalloc::Size::from_bytes_usize(HEAP_SIZE)
			.expect("HEAP_SIZE is below the design limit of the allocator; qed")
	}

	unsafe fn allocate_address_space(&mut self) -> *mut u8 {
		let current = unsafe { grow_heap(0) };
		let aligned = current.next_multiple_of(32) as *mut u8;
		// The heap starts right after the already-built stack and data sections, so the new
		// address space starts at the current heap end, aligned to the 32-byte boundary `picoalloc`
		// needs.
		if unsafe { grow_heap(aligned.addr() - current) } == 0 {
			return core::ptr::null_mut();
		}
		aligned
	}

	unsafe fn expand_memory_until(&mut self, base: *mut u8, size: picoalloc::Size) -> bool {
		let current = unsafe { grow_heap(0) };
		let Some(requested_end) = base.addr().checked_add(size.bytes() as usize) else {
			return false;
		};

		if requested_end <= current {
			return true;
		}

		// `grow_heap` returns the previous heap end, or zero if no space is left (the Gray
		// Paper clamps at the ceiling `b` instead of signalling an error).
		unsafe { grow_heap(requested_end - current) != 0 }
	}

	unsafe fn free_address_space(&mut self, _base: *mut u8) {}
}

/// The local allocator used to manage the local heap.
struct LocalAllocator(UnsafeCell<picoalloc::Allocator<RuntimeAllocator>>);

// SAFETY: This is runtime-only, and runtimes are single-threaded, so this is safe.
unsafe impl Send for LocalAllocator {}

// SAFETY: This is runtime-only, and runtimes are single-threaded, so this is safe.
unsafe impl Sync for LocalAllocator {}

static LOCAL_ALLOCATOR: LocalAllocator =
	LocalAllocator(UnsafeCell::new(picoalloc::Allocator::new(RuntimeAllocator)));

fn local_allocator() -> &'static mut picoalloc::Allocator<RuntimeAllocator> {
	// SAFETY: This is only called when allocating memory, and the allocator
	// doesn't trigger itself recursively, so only a single
	// &mut will ever exist at the same time.
	unsafe { &mut *LOCAL_ALLOCATOR.0.get() }
}

unsafe impl GlobalAlloc for RuntimeAllocator {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		// These should never fail, but let's do proper error checking anyway.
		let Some(align) = picoalloc::Size::from_bytes_usize(layout.align()) else {
			return core::ptr::null_mut();
		};

		let Some(size) = picoalloc::Size::from_bytes_usize(layout.size()) else {
			return core::ptr::null_mut();
		};

		if let Some(pointer) = local_allocator().alloc(align, size) {
			pointer.as_ptr()
		} else {
			core::ptr::null_mut()
		}
	}

	unsafe fn dealloc(&self, ptr: *mut u8, _: Layout) {
		// SAFETY: Pointers only come from the local heap.
		unsafe { local_allocator().free(NonNull::new_unchecked(ptr)) }
	}

	unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
		let Some(align) = picoalloc::Size::from_bytes_usize(layout.align()) else {
			return core::ptr::null_mut();
		};

		let Some(size) = picoalloc::Size::from_bytes_usize(layout.size()) else {
			return core::ptr::null_mut();
		};

		if let Some(pointer) = local_allocator().alloc_zeroed(align, size) {
			return pointer.as_ptr();
		} else {
			core::ptr::null_mut()
		}
	}

	unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
		let Some(align) = picoalloc::Size::from_bytes_usize(layout.align()) else {
			return core::ptr::null_mut();
		};

		let Some(new_size_s) = picoalloc::Size::from_bytes_usize(new_size) else {
			return core::ptr::null_mut();
		};

		// SAFETY: Pointers only come from the local heap.
		if let Some(pointer) =
			unsafe { local_allocator().realloc(NonNull::new_unchecked(ptr), align, new_size_s) }
		{
			pointer.as_ptr()
		} else {
			core::ptr::null_mut()
		}
	}
}
