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
	arch::wasm32,
	cell::UnsafeCell,
	ptr::NonNull,
};

/// Allocator used by Substrate from within the runtime.
struct RuntimeAllocator;

#[global_allocator]
static ALLOCATOR: RuntimeAllocator = RuntimeAllocator;

const WASM_PAGE_SIZE: usize = 64 * 1024;

extern "C" {
	static __heap_base: u8;
}

#[inline(always)]
fn aligned_heap_base() -> *mut u8 {
	// SAFETY: Wasmtime must export the symbol at correct address (end of data segment)
	let base = unsafe { &__heap_base as *const u8 as usize };
	((base + 31) & !31) as *mut u8
}

impl picoalloc::Env for RuntimeAllocator {
	fn total_space(&self) -> picoalloc::Size {
		// Compute the maximum virtual space available for the heap.
		// We do not pre-grow memory here; we only advertise a virtual cap. Actual memory
		// is grown on demand in `expand_memory_until` and will be clamped by the VM's
		// configured maximum.
		let base = aligned_heap_base() as usize;
		// Keep the end strictly below 4GiB and aligned to 32 bytes.
		let end = usize::MAX & !31;
		if base >= end {
			return picoalloc::Size::from_bytes_usize(0)
				.expect("Conversion from u32 to Size never fails on wasm32");
		}
		let total = (end - base) & !31;
		picoalloc::Size::from_bytes_usize(total)
			.expect("Conversion from u32 to Size never fails on wasm32")
	}

	unsafe fn allocate_address_space(&mut self) -> *mut u8 {
		aligned_heap_base()
	}

	unsafe fn expand_memory_until(&mut self, base: *mut u8, size: picoalloc::Size) -> bool {
		let base_offset = base as usize;
		let Some(requested_end) = base_offset.checked_add(size.bytes() as usize) else {
			return false;
		};

		let current_pages = wasm32::memory_size(0) as usize;
		let current_end = current_pages * WASM_PAGE_SIZE;

		if requested_end <= current_end {
			return true;
		}

		let grow_bytes = requested_end - current_end;
		let grow_pages = (grow_bytes + WASM_PAGE_SIZE - 1) / WASM_PAGE_SIZE;

		wasm32::memory_grow(0, grow_pages) != usize::MAX
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
	// doesn't trigger the global allocator recursively, so only a single
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

		// First try the local allocator. Use its `alloc_zeroed` as its
		// smart enough to not unnecessarily zero-fill the memory if it's
		// the very first allocation which touches this region of the heap.
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
