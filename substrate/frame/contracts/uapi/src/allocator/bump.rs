// Copyright (C) Parity Technologies (UK) Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! A simple bump allocator.
//!
//! Its goal to have a much smaller footprint than the admittedly more full-featured
//! `wee_alloc` allocator which is currently being used by ink! smart contracts.
//!
//! The heap which is used by this allocator is built from pages of Wasm memory (each page
//! is `64KiB`). We will request new pages of memory as needed until we run out of memory,
//! at which point we will crash with an `OOM` error instead of freeing any memory.

use core::alloc::{
    GlobalAlloc,
    Layout,
};

/// A page in Wasm is `64KiB`
const PAGE_SIZE: usize = 64 * 1024;

static mut INNER: Option<InnerAlloc> = None;

/// A bump allocator suitable for use in a Wasm environment.
pub struct BumpAllocator;

unsafe impl GlobalAlloc for BumpAllocator {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if INNER.is_none() {
            INNER = Some(InnerAlloc::new());
        };
        match INNER
            .as_mut()
            .expect("We just set the value above; qed")
            .alloc(layout)
        {
            Some(start) => start as *mut u8,
            None => core::ptr::null_mut(),
        }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // A new page in Wasm is guaranteed to already be zero initialized, so we can just
        // use our regular `alloc` call here and save a bit of work.
        //
        // See: https://webassembly.github.io/spec/core/exec/modules.html#growing-memories
        self.alloc(layout)
    }

    #[inline]
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[cfg_attr(feature = "std", derive(Debug, Copy, Clone))]
struct InnerAlloc {
    /// Points to the start of the next available allocation.
    next: usize,

    /// The address of the upper limit of our heap.
    upper_limit: usize,
}

impl InnerAlloc {
    fn new() -> Self {
        Self {
            next: Self::heap_start(),
            upper_limit: Self::heap_end(),
        }
    }

    cfg_if::cfg_if! {
        if #[cfg(test)] {
            fn heap_start() -> usize {
                0
            }

            fn heap_end() -> usize {
                0
            }

            /// Request a `pages` number of page sized sections of Wasm memory. Each page is `64KiB` in size.
            ///
            /// Returns `None` if a page is not available.
            ///
            /// This implementation is only meant to be used for testing, since we cannot (easily)
            /// test the `wasm32` implementation.
            fn request_pages(&mut self, _pages: usize) -> Option<usize> {
                Some(self.upper_limit)
            }
        } else if #[cfg(feature = "std")] {
            fn heap_start() -> usize {
                0
            }

            fn heap_end() -> usize {
                0
            }

            fn request_pages(&mut self, _pages: usize) -> Option<usize> {
                unreachable!(
                    "This branch is only used to keep the compiler happy when building tests, and
                     should never actually be called outside of a test run."
                )
            }
        } else if #[cfg(target_arch = "wasm32")] {
            fn heap_start() -> usize {
                extern "C" {
                    static __heap_base: usize;
                }
                // # SAFETY
                //
                // The `__heap_base` symbol is defined by the wasm linker and is guaranteed
                // to point to the start of the heap.
                let heap_start =  unsafe { &__heap_base as *const usize as usize };
                // if the symbol isn't found it will resolve to 0
                // for that to happen the rust compiler or linker need to break or change
                assert_ne!(heap_start, 0, "Can't find `__heap_base` symbol.");
                heap_start
            }

            fn heap_end() -> usize {
                // Cannot overflow on this architecture
                core::arch::wasm32::memory_size(0) * PAGE_SIZE
            }

            /// Request a `pages` number of pages of Wasm memory. Each page is `64KiB` in size.
            ///
            /// Returns `None` if a page is not available.
            fn request_pages(&mut self, pages: usize) -> Option<usize> {
                let prev_page = core::arch::wasm32::memory_grow(0, pages);
                if prev_page == usize::MAX {
                    return None;
                }

                // Cannot overflow on this architecture
                Some(prev_page * PAGE_SIZE)
            }
        } else if #[cfg(target_arch = "riscv32")] {
            const fn heap_start() -> usize {
                // Placeholder value until we specified our riscv VM
                0x7000_0000
            }

            const fn heap_end() -> usize {
                // Placeholder value until we specified our riscv VM
                // Let's just assume a cool megabyte of mem for now
                0x7000_0400
            }

            fn request_pages(&mut self, _pages: usize) -> Option<usize> {
                // On riscv the memory can't be grown
                None
            }
        } else {
            core::compile_error!("ink! only supports wasm32 and riscv32");
        }
    }

    /// Tries to allocate enough memory on the heap for the given `Layout`. If there is
    /// not enough room on the heap it'll try and grow it by a page.
    ///
    /// Note: This implementation results in internal fragmentation when allocating across
    /// pages.
    fn alloc(&mut self, layout: Layout) -> Option<usize> {
        let alloc_start = self.next;

        let aligned_size = layout.pad_to_align().size();
        let alloc_end = alloc_start.checked_add(aligned_size)?;

        if alloc_end > self.upper_limit {
            let required_pages = required_pages(aligned_size)?;
            let page_start = self.request_pages(required_pages)?;

            self.upper_limit = required_pages
                .checked_mul(PAGE_SIZE)
                .and_then(|pages| page_start.checked_add(pages))?;
            self.next = page_start.checked_add(aligned_size)?;

            Some(page_start)
        } else {
            self.next = alloc_end;
            Some(alloc_start)
        }
    }
}

/// Calculates the number of pages of memory needed for an allocation of `size` bytes.
///
/// This function rounds up to the next page. For example, if we have an allocation of
/// `size = PAGE_SIZE / 2` this function will indicate that one page is required to
/// satisfy the allocation.
#[inline]
fn required_pages(size: usize) -> Option<usize> {
    size.checked_add(PAGE_SIZE - 1)
        .and_then(|num| num.checked_div(PAGE_SIZE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn can_alloc_no_bytes() {
        let mut inner = InnerAlloc::new();

        let layout = Layout::new::<()>();
        assert_eq!(inner.alloc(layout), Some(0));

        let expected_limit =
            PAGE_SIZE * required_pages(layout.pad_to_align().size()).unwrap();
        assert_eq!(inner.upper_limit, expected_limit);

        let expected_alloc_start = size_of::<()>();
        assert_eq!(inner.next, expected_alloc_start);
    }

    #[test]
    fn can_alloc_a_byte() {
        let mut inner = InnerAlloc::new();

        let layout = Layout::new::<u8>();
        assert_eq!(inner.alloc(layout), Some(0));

        let expected_limit =
            PAGE_SIZE * required_pages(layout.pad_to_align().size()).unwrap();
        assert_eq!(inner.upper_limit, expected_limit);

        let expected_alloc_start = size_of::<u8>();
        assert_eq!(inner.next, expected_alloc_start);
    }

    #[test]
    fn can_alloc_a_foobarbaz() {
        let mut inner = InnerAlloc::new();

        struct FooBarBaz {
            _foo: u32,
            _bar: u128,
            _baz: (u16, bool),
        }

        let layout = Layout::new::<FooBarBaz>();
        let mut total_size = 0;

        let allocations = 3;
        for _ in 0..allocations {
            assert!(inner.alloc(layout).is_some());
            total_size += layout.pad_to_align().size();
        }

        let expected_limit = PAGE_SIZE * required_pages(total_size).unwrap();
        assert_eq!(inner.upper_limit, expected_limit);

        let expected_alloc_start = allocations * size_of::<FooBarBaz>();
        assert_eq!(inner.next, expected_alloc_start);
    }

    #[test]
    fn can_alloc_across_pages() {
        let mut inner = InnerAlloc::new();

        struct Foo {
            _foo: [u8; PAGE_SIZE - 1],
        }

        // First, let's allocate a struct which is _almost_ a full page
        let layout = Layout::new::<Foo>();
        assert_eq!(inner.alloc(layout), Some(0));

        let expected_limit =
            PAGE_SIZE * required_pages(layout.pad_to_align().size()).unwrap();
        assert_eq!(inner.upper_limit, expected_limit);

        let expected_alloc_start = size_of::<Foo>();
        assert_eq!(inner.next, expected_alloc_start);

        // Now we'll allocate two bytes which will push us over to the next page
        let layout = Layout::new::<u16>();
        assert_eq!(inner.alloc(layout), Some(PAGE_SIZE));

        let expected_limit = 2 * PAGE_SIZE;
        assert_eq!(inner.upper_limit, expected_limit);

        // Notice that we start the allocation on the second page, instead of making use
        // of the remaining byte on the first page
        let expected_alloc_start = PAGE_SIZE + size_of::<u16>();
        assert_eq!(inner.next, expected_alloc_start);
    }

    #[test]
    fn can_alloc_multiple_pages() {
        let mut inner = InnerAlloc::new();

        struct Foo {
            _foo: [u8; 2 * PAGE_SIZE],
        }

        let layout = Layout::new::<Foo>();
        assert_eq!(inner.alloc(layout), Some(0));

        let expected_limit =
            PAGE_SIZE * required_pages(layout.pad_to_align().size()).unwrap();
        assert_eq!(inner.upper_limit, expected_limit);

        let expected_alloc_start = size_of::<Foo>();
        assert_eq!(inner.next, expected_alloc_start);

        // Now we want to make sure that the state of our allocator is correct for any
        // subsequent allocations
        let layout = Layout::new::<u8>();
        assert_eq!(inner.alloc(layout), Some(2 * PAGE_SIZE));

        let expected_limit = 3 * PAGE_SIZE;
        assert_eq!(inner.upper_limit, expected_limit);

        let expected_alloc_start = 2 * PAGE_SIZE + size_of::<u8>();
        assert_eq!(inner.next, expected_alloc_start);
    }
}

#[cfg(all(test, feature = "ink-fuzz-tests"))]
mod fuzz_tests {
    use super::*;
    use quickcheck::{
        quickcheck,
        TestResult,
    };
    use std::mem::size_of;

    #[quickcheck]
    fn should_allocate_arbitrary_sized_bytes(n: usize) -> TestResult {
        let mut inner = InnerAlloc::new();

        // If we're going to end up creating an invalid `Layout` we don't want to use
        // these test inputs.
        let layout = match Layout::from_size_align(n, size_of::<usize>()) {
            Ok(l) => l,
            Err(_) => return TestResult::discard(),
        };

        let size = layout.pad_to_align().size();
        assert_eq!(
            inner.alloc(layout),
            Some(0),
            "The given pointer for the allocation doesn't match."
        );

        let expected_alloc_start = size;
        assert_eq!(
            inner.next, expected_alloc_start,
            "Our next allocation doesn't match where it should start."
        );

        let expected_limit = PAGE_SIZE * required_pages(size).unwrap();
        assert_eq!(
            inner.upper_limit, expected_limit,
            "The upper bound of our heap doesn't match."
        );

        TestResult::passed()
    }

    #[quickcheck]
    fn should_allocate_regardless_of_alignment_size(
        n: usize,
        align: usize,
    ) -> TestResult {
        let aligns = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512];
        let align = aligns[align % aligns.len()];

        let mut inner = InnerAlloc::new();

        // If we're going to end up creating an invalid `Layout` we don't want to use
        // these test inputs.
        let layout = match Layout::from_size_align(n, align) {
            Ok(l) => l,
            Err(_) => return TestResult::discard(),
        };

        let size = layout.pad_to_align().size();
        assert_eq!(
            inner.alloc(layout),
            Some(0),
            "The given pointer for the allocation doesn't match."
        );

        let expected_alloc_start = size;
        assert_eq!(
            inner.next, expected_alloc_start,
            "Our next allocation doesn't match where it should start."
        );

        let expected_limit = PAGE_SIZE * required_pages(size).unwrap();
        assert_eq!(
            inner.upper_limit, expected_limit,
            "The upper bound of our heap doesn't match."
        );

        TestResult::passed()
    }

    /// The idea behind this fuzz test is to check a series of allocation sequences. For
    /// example, we maybe have back to back runs as follows:
    ///
    /// 1. `vec![1, 2, 3]`
    /// 2. `vec![4, 5, 6, 7]`
    /// 3. `vec![8]`
    ///
    /// Each of the vectors represents one sequence of allocations. Within each sequence
    /// the individual size of allocations will be randomly selected by `quickcheck`.
    #[quickcheck]
    fn should_allocate_arbitrary_byte_sequences(sequence: Vec<isize>) -> TestResult {
        let mut inner = InnerAlloc::new();

        if sequence.is_empty() {
            return TestResult::discard()
        }

        // We don't want any negative numbers so we can be sure our conversions to `usize`
        // later are valid
        if !sequence.iter().all(|n| n.is_positive()) {
            return TestResult::discard()
        }

        // We can't just use `required_pages(Iterator::sum())` here because it ends up
        // underestimating the pages due to the ceil rounding at each step
        let pages_required = sequence
            .iter()
            .fold(0, |acc, &x| acc + required_pages(x as usize).unwrap());
        let max_pages = required_pages(usize::MAX - PAGE_SIZE + 1).unwrap();

        // We know this is going to end up overflowing, we'll check this case in a
        // different test
        if pages_required > max_pages {
            return TestResult::discard()
        }

        let mut expected_alloc_start = 0;
        let mut total_bytes_requested = 0;
        let mut total_bytes_fragmented = 0;

        for alloc in sequence {
            let layout = Layout::from_size_align(alloc as usize, size_of::<usize>());
            let layout = match layout {
                Ok(l) => l,
                Err(_) => return TestResult::discard(),
            };

            let size = layout.pad_to_align().size();

            let current_page_limit = PAGE_SIZE * required_pages(inner.next).unwrap();
            let is_too_big_for_current_page = inner.next + size > current_page_limit;

            if is_too_big_for_current_page {
                let fragmented_in_current_page = current_page_limit - inner.next;
                total_bytes_fragmented += fragmented_in_current_page;

                // We expect our next allocation to be aligned to the start of the next
                // page boundary
                expected_alloc_start = inner.upper_limit;
            }

            assert_eq!(
                inner.alloc(layout),
                Some(expected_alloc_start),
                "The given pointer for the allocation doesn't match."
            );
            total_bytes_requested += size;

            expected_alloc_start = total_bytes_requested + total_bytes_fragmented;
            assert_eq!(
                inner.next, expected_alloc_start,
                "Our next allocation doesn't match where it should start."
            );

            let pages_required = required_pages(expected_alloc_start).unwrap();
            let expected_limit = PAGE_SIZE * pages_required;
            assert_eq!(
                inner.upper_limit, expected_limit,
                "The upper bound of our heap doesn't match."
            );
        }

        TestResult::passed()
    }

    // For this test we have sequences of allocations which will eventually overflow the
    // maximum amount of pages (in practice this means our heap will be OOM).
    //
    // We don't care about the allocations that succeed (those are checked in other
    // tests), we just care that eventually an allocation doesn't success.
    #[quickcheck]
    fn should_not_allocate_arbitrary_byte_sequences_which_eventually_overflow(
        sequence: Vec<isize>,
    ) -> TestResult {
        let mut inner = InnerAlloc::new();

        if sequence.is_empty() {
            return TestResult::discard()
        }

        // We don't want any negative numbers so we can be sure our conversions to `usize`
        // later are valid
        if !sequence.iter().all(|n| n.is_positive()) {
            return TestResult::discard()
        }

        // We can't just use `required_pages(Iterator::sum())` here because it ends up
        // underestimating the pages due to the ceil rounding at each step
        let pages_required = sequence
            .iter()
            .fold(0, |acc, &x| acc + required_pages(x as usize).unwrap());
        let max_pages = required_pages(usize::MAX - PAGE_SIZE + 1).unwrap();

        // We want to explicitly test for the case where a series of allocations
        // eventually runs out of pages of memory
        if pages_required <= max_pages {
            return TestResult::discard()
        }

        let mut results = vec![];
        for alloc in sequence {
            let layout = Layout::from_size_align(alloc as usize, size_of::<usize>());
            let layout = match layout {
                Ok(l) => l,
                Err(_) => return TestResult::discard(),
            };

            results.push(inner.alloc(layout));
        }

        // Ensure that at least one of the allocations ends up overflowing our
        // calculations.
        assert!(
            results.iter().any(|r| r.is_none()),
            "Expected an allocation to overflow our heap, but this didn't happen."
        );

        TestResult::passed()
    }
}
