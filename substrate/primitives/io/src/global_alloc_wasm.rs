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
	cell::UnsafeCell,
	ptr::NonNull,
};

/// Allocator used by Substrate from within the runtime.
struct RuntimeAllocator;

#[global_allocator]
static ALLOCATOR: RuntimeAllocator = RuntimeAllocator;

/// The size of the local heap.
///
/// This should be as big as possible, but it should still leave enough space
/// under the maximum memory usage limit to allow the host allocator to service the host calls.
const LOCAL_HEAP_SIZE: usize = 64 * 1024 * 1024;
const LOCAL_HEAP_S: picoalloc::Size = picoalloc::Size::from_bytes_usize(LOCAL_HEAP_SIZE).unwrap();

#[repr(align(32))] // `picoalloc` requires 32-byte alignment of the heap
struct LocalHeap(UnsafeCell<[u8; LOCAL_HEAP_SIZE]>);

// SAFETY: This is runtime-only, and runtimes are single-threaded, so this is safe.
unsafe impl Send for LocalHeap {}

// SAFETY: This is runtime-only, and runtimes are single-threaded, so this is safe.
unsafe impl Sync for LocalHeap {}

// Preallocate a bunch of space statically for use by the local allocator.
//
// This should be relatively cheap as long as all of this space is full of zeros,
// since none of this memory will be physically paged-in until it's actually used.
static LOCAL_HEAP: LocalHeap = LocalHeap(UnsafeCell::new([0; LOCAL_HEAP_SIZE]));

impl picoalloc::Env for RuntimeAllocator {
	fn total_space(&self) -> picoalloc::Size {
		LOCAL_HEAP_S
	}

	unsafe fn allocate_address_space(&mut self) -> *mut u8 {
		LOCAL_HEAP.0.get().cast()
	}

	unsafe fn expand_memory_until(&mut self, _base: *mut u8, size: picoalloc::Size) -> bool {
		size <= LOCAL_HEAP_S
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

/// Checks whether a given pointer came from the local allocator.
fn is_local_pointer(ptr: *mut u8) -> bool {
	ptr.addr() >= LOCAL_HEAP.0.get().addr() &&
		ptr.addr() < LOCAL_HEAP.0.get().addr() + LOCAL_HEAP_SIZE
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

		// Try to allocate memory from the local pool, and only fall back
		// to the host allocator if this fails.
		if let Some(pointer) = local_allocator().alloc(align, size) {
			pointer.as_ptr()
		} else {
			crate::global_alloc_wasm_legacy::HostAllocator.alloc(layout)
		}
	}

	unsafe fn dealloc(&self, ptr: *mut u8, _: Layout) {
		if is_local_pointer(ptr) {
			// SAFETY: We've checked that the pointer is from the local allocator.
			unsafe { local_allocator().free(NonNull::new_unchecked(ptr)) }
		} else {
			crate::global_alloc_wasm_legacy::HostAllocator.dealloc(ptr)
		}
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
		}

		// The local allocator is full, so fall back to the host allocator.

		let size = layout.size();
		let ptr = crate::global_alloc_wasm_legacy::HostAllocator.alloc(layout);
		if !ptr.is_null() {
			// SAFETY: as allocation succeeded, the region from `ptr`
			// of size `size` is guaranteed to be valid for writes.
			unsafe { core::ptr::write_bytes(ptr, 0, size) };
		}
		ptr
	}

	unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
		if is_local_pointer(ptr) {
			let Some(align) = picoalloc::Size::from_bytes_usize(layout.align()) else {
				return core::ptr::null_mut();
			};

			let Some(new_size_s) = picoalloc::Size::from_bytes_usize(new_size) else {
				return core::ptr::null_mut();
			};

			// If the pointer comes from the local allocator try to efficiently reallocate it.
			// If possible this will (unlike the host allocator) resize the allocation in-place.

			// SAFETY: We've checked that the pointer is from the local allocator.
			if let Some(pointer) =
				unsafe { local_allocator().realloc(NonNull::new_unchecked(ptr), align, new_size_s) }
			{
				return pointer.as_ptr();
			}
		}

		// The pointer was allocated by the host, or the local allocator is full.
		// Fall back to the default `realloc` implementation.

		// SAFETY: the caller must ensure that the `new_size` does not overflow.
		// `layout.align()` comes from a `Layout` and is thus guaranteed to be valid.
		let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
		// SAFETY: the caller must ensure that `new_layout` is greater than zero.
		let new_ptr = unsafe { self.alloc(new_layout) };
		if !new_ptr.is_null() {
			// SAFETY: the previously allocated block cannot overlap the newly allocated block.
			// The safety contract for `dealloc` must be upheld by the caller.
			unsafe {
				core::ptr::copy_nonoverlapping(
					ptr,
					new_ptr,
					core::cmp::min(layout.size(), new_size),
				);
				self.dealloc(ptr, layout);
			}
		}
		new_ptr
	}
}
