// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Tracking/limiting global allocator. Calculates the peak allocation between two checkpoints for
//! the whole process. Accepts an optional limit and a failure handler which is called if the limit
//! is overflown.

use core::alloc::{GlobalAlloc, Layout};
use std::{
	ptr::null_mut,
	sync::atomic::{AtomicBool, Ordering},
};

struct TrackingAllocatorData {
	lock: AtomicBool,
	current: isize,
	peak: isize,
	limit: isize,
	failure_handler: Option<Box<dyn Fn()>>,
}

impl TrackingAllocatorData {
	#[inline]
	fn lock(&self) {
		loop {
			// Try to acquire the lock.
			if self
				.lock
				.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
				.is_ok()
			{
				break
			}
			// We failed to acquire the lock; wait until it's unlocked.
			//
			// In theory this should result in less coherency traffic as unlike `compare_exchange`
			// it is a read-only operation, so multiple cores can execute it simultaneously
			// without taking an exclusive lock over the cache line.
			while self.lock.load(Ordering::Relaxed) {
				std::hint::spin_loop();
			}
		}
	}

	#[inline]
	fn unlock(&self) {
		self.lock.store(false, Ordering::Release);
	}

	fn start_tracking(&mut self, limit: isize, failure_handler: Option<Box<dyn Fn()>>) {
		self.lock();
		self.current = 0;
		self.peak = 0;
		self.limit = limit;
		// Cannot drop it yet, as it would trigger a deallocation
		let old_handler = self.failure_handler.take();
		self.failure_handler = failure_handler;
		self.unlock();
		core::mem::drop(old_handler);
	}

	fn end_tracking(&mut self) -> isize {
		self.lock();
		let peak = self.peak;
		self.limit = 0;
		// Cannot drop it yet, as it would trigger a deallocation
		let old_handler = self.failure_handler.take();
		self.unlock();
		core::mem::drop(old_handler);
		peak
	}

	#[inline]
	fn track(&mut self, alloc: isize) -> bool {
		self.lock();
		self.current += alloc;
		if self.current > self.peak {
			self.peak = self.current;
		}
		let within_limits = self.peak <= self.limit;
		if within_limits {
			self.unlock()
		}
		within_limits
	}
}

static mut ALLOCATOR_DATA: TrackingAllocatorData = TrackingAllocatorData {
	lock: AtomicBool::new(false),
	current: 0,
	peak: 0,
	limit: 0,
	failure_handler: None,
};

pub struct TrackingAllocator<A: GlobalAlloc>(pub A);

impl<A: GlobalAlloc> TrackingAllocator<A> {
	// SAFETY:
	// * The following functions write to `static mut`. That is safe as the critical section inside
	//   is isolated by an exclusive lock.

	/// Start tracking
	/// SAFETY: Failure handler is called with the allocator being in the locked state. Thus, no
	/// allocations or deallocations are allowed inside the failure handler; otherwise, a
	/// deadlock will occur.
	pub unsafe fn start_tracking(
		&self,
		limit: Option<isize>,
		failure_handler: Option<Box<dyn Fn()>>,
	) {
		ALLOCATOR_DATA.start_tracking(limit.unwrap_or(isize::MAX), failure_handler);
	}

	/// End tracking and return the peak allocation value in bytes (as `isize`). Peak allocation
	/// value is not guaranteed to be neither non-zero nor positive.
	pub fn end_tracking(&self) -> isize {
		unsafe { ALLOCATOR_DATA.end_tracking() }
	}
}

#[cold]
#[inline(never)]
unsafe fn fail_allocation() -> *mut u8 {
	if let Some(failure_handler) = &ALLOCATOR_DATA.failure_handler {
		failure_handler()
	}
	ALLOCATOR_DATA.unlock();
	null_mut()
}

unsafe impl<A: GlobalAlloc> GlobalAlloc for TrackingAllocator<A> {
	// SAFETY:
	// * The wrapped methods are as safe as the underlying allocator implementation is

	#[inline]
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		if ALLOCATOR_DATA.track(layout.size() as isize) {
			self.0.alloc(layout)
		} else {
			fail_allocation()
		}
	}

	#[inline]
	unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
		if ALLOCATOR_DATA.track(layout.size() as isize) {
			self.0.alloc_zeroed(layout)
		} else {
			fail_allocation()
		}
	}

	#[inline]
	unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) -> () {
		ALLOCATOR_DATA.track(-(layout.size() as isize));
		self.0.dealloc(ptr, layout)
	}

	#[inline]
	unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
		if ALLOCATOR_DATA.track((new_size as isize) - (layout.size() as isize)) {
			self.0.realloc(ptr, layout, new_size)
		} else {
			fail_allocation()
		}
	}
}
