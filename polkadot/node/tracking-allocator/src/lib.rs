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

use core::{
	alloc::{GlobalAlloc, Layout},
	ops::{Deref, DerefMut},
};
use std::{
	cell::UnsafeCell,
	ptr::null_mut,
	sync::atomic::{AtomicBool, Ordering},
};

struct Spinlock<T> {
	lock: AtomicBool,
	data: UnsafeCell<T>,
}

struct SpinlockGuard<'a, T: 'a> {
	lock: &'a Spinlock<T>,
}

// SAFETY: We require that the data inside of the `SpinLock` is `Send`, so it can be sent
// and accessed by any thread as long as it's accessed by only one thread at a time.
// The `SpinLock` provides an exclusive lock over it, so it guarantees that multiple
// threads cannot access it at the same time, hence it implements `Sync` (that is, it can be
// accessed concurrently from multiple threads, even though the `T` itself might not
// necessarily be `Sync` too).
unsafe impl<T: Send> Sync for Spinlock<T> {}

impl<T> Spinlock<T> {
	pub const fn new(t: T) -> Spinlock<T> {
		Spinlock { lock: AtomicBool::new(false), data: UnsafeCell::new(t) }
	}

	#[inline]
	pub fn lock(&self) -> SpinlockGuard<T> {
		loop {
			// Try to acquire the lock.
			if self
				.lock
				.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
				.is_ok()
			{
				return SpinlockGuard { lock: self }
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

	// SAFETY: It should be only called from the guard's destructor. Calling it explicitly while
	// the guard is alive is undefined behavior, as it breaks the security contract of `Deref` and
	// `DerefMut`, which implies that lock is held at the moment of dereferencing.
	#[inline]
	unsafe fn unlock(&self) {
		self.lock.store(false, Ordering::Release);
	}
}

impl<T> Deref for SpinlockGuard<'_, T> {
	type Target = T;

	fn deref(&self) -> &T {
		// SAFETY: It is safe to dereference a guard to the `UnsafeCell` underlying data as the
		// presence of the guard means the data is already locked.
		unsafe { &*self.lock.data.get() }
	}
}

impl<T> DerefMut for SpinlockGuard<'_, T> {
	fn deref_mut(&mut self) -> &mut T {
		// SAFETY: Same as for `Deref::deref`.
		unsafe { &mut *self.lock.data.get() }
	}
}

impl<T> Drop for SpinlockGuard<'_, T> {
	fn drop(&mut self) {
		// SAFETY: Calling `unlock` is only safe when it's guaranteed no guard outlives the
		// unlocking point; here, the guard is dropped, so it is safe.
		unsafe { self.lock.unlock() }
	}
}

struct TrackingAllocatorData {
	current: isize,
	peak: isize,
	limit: isize,
	failure_handler: Option<Box<dyn Fn() + Send>>,
}

impl TrackingAllocatorData {
	fn start_tracking(
		mut guard: SpinlockGuard<Self>,
		limit: isize,
		failure_handler: Option<Box<dyn Fn() + Send>>,
	) {
		guard.current = 0;
		guard.peak = 0;
		guard.limit = limit;
		// Cannot drop it yet, as it would trigger a deallocation
		let old_handler = guard.failure_handler.take();
		guard.failure_handler = failure_handler;
		drop(guard);
		drop(old_handler);
	}

	fn end_tracking(mut guard: SpinlockGuard<Self>) -> isize {
		let peak = guard.peak;
		guard.limit = 0;
		// Cannot drop it yet, as it would trigger a deallocation
		let old_handler = guard.failure_handler.take();
		drop(guard);
		drop(old_handler);
		peak
	}

	#[inline]
	fn track_and_check_limits(
		mut guard: SpinlockGuard<Self>,
		alloc: isize,
	) -> Option<SpinlockGuard<Self>> {
		guard.current += alloc;
		if guard.current > guard.peak {
			guard.peak = guard.current;
		}
		if guard.limit == 0 || guard.peak <= guard.limit {
			None
		} else {
			Some(guard)
		}
	}
}

static ALLOCATOR_DATA: Spinlock<TrackingAllocatorData> =
	Spinlock::new(TrackingAllocatorData { current: 0, peak: 0, limit: 0, failure_handler: None });

pub struct TrackingAllocator<A: GlobalAlloc>(pub A);

impl<A: GlobalAlloc> TrackingAllocator<A> {
	/// Start tracking memory allocations and deallocations.
	///
	/// # Safety
	///
	/// Failure handler is called with the allocator being in the locked state. Thus, no
	/// allocations or deallocations are allowed inside the failure handler; otherwise, a
	/// deadlock will occur.
	pub unsafe fn start_tracking(
		&self,
		limit: Option<isize>,
		failure_handler: Option<Box<dyn Fn() + Send>>,
	) {
		TrackingAllocatorData::start_tracking(
			ALLOCATOR_DATA.lock(),
			limit.unwrap_or(0),
			failure_handler,
		);
	}

	/// End tracking and return the peak allocation value in bytes (as `isize`). Peak allocation
	/// value is not guaranteed to be neither non-zero nor positive.
	pub fn end_tracking(&self) -> isize {
		TrackingAllocatorData::end_tracking(ALLOCATOR_DATA.lock())
	}
}

#[cold]
#[inline(never)]
unsafe fn fail_allocation(guard: SpinlockGuard<TrackingAllocatorData>) -> *mut u8 {
	if let Some(failure_handler) = &guard.failure_handler {
		failure_handler()
	}
	null_mut()
}

unsafe impl<A: GlobalAlloc> GlobalAlloc for TrackingAllocator<A> {
	// SAFETY:
	// * The wrapped methods are as safe as the underlying allocator implementation is

	#[inline]
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		let guard = ALLOCATOR_DATA.lock();
		if let Some(guard) =
			TrackingAllocatorData::track_and_check_limits(guard, layout.size() as isize)
		{
			fail_allocation(guard)
		} else {
			self.0.alloc(layout)
		}
	}

	#[inline]
	unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
		let guard = ALLOCATOR_DATA.lock();
		if let Some(guard) =
			TrackingAllocatorData::track_and_check_limits(guard, layout.size() as isize)
		{
			fail_allocation(guard)
		} else {
			self.0.alloc_zeroed(layout)
		}
	}

	#[inline]
	unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
		let guard = ALLOCATOR_DATA.lock();
		TrackingAllocatorData::track_and_check_limits(guard, -(layout.size() as isize));
		self.0.dealloc(ptr, layout)
	}

	#[inline]
	unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
		let guard = ALLOCATOR_DATA.lock();
		if let Some(guard) = TrackingAllocatorData::track_and_check_limits(
			guard,
			(new_size as isize) - (layout.size() as isize),
		) {
			fail_allocation(guard)
		} else {
			self.0.realloc(ptr, layout, new_size)
		}
	}
}
