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

//! `memcpy`, `memcmp` and `bcmp` for the PolkaVM (JAM) runtime.
//!
//! PolkaVM charges gas per basic block, from what an out-of-order core would need to run the
//! block on its own: every block pays the full latency of its loads, while independent loads in
//! the same block overlap. The `compiler_builtins` versions loop over one byte (`memcmp`) or one
//! word (`memcpy`) per block, so each byte or word pays a whole load latency. The versions here
//! move four words per block, handle the rest of the words and the bytes after them with at
//! most one block per size class, and `memcmp` finds the first differing byte of a word without
//! looping over its bytes.
//!
//! The accesses are volatile: the compiler can neither turn the loops back into calls of these
//! functions, nor split the loads of one block into a chain of blocks with one load each.
//!
//! `compiler_builtins` defines its symbols weak, so these take their place.

use core::{
	ffi::c_int,
	ptr::{read_volatile as load, write_volatile as store},
};

const WORD: usize = core::mem::size_of::<u64>();

/// Copy `n` bytes from `src` to `dst`.
///
/// # Safety
///
/// `src` must be valid for reads and `dst` for writes of `n` bytes, and the two must not overlap.
#[no_mangle]
pub unsafe extern "C" fn memcpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
	let (mut d, mut s) = (dst, src);
	let end = dst.add(n);
	if (d as usize ^ s as usize) % WORD == 0 {
		while d != end && d as usize % WORD != 0 {
			store(d, load(s));
			(d, s) = (d.add(1), s.add(1));
		}
		let (mut dw, mut sw) = (d.cast::<u64>(), s.cast::<u64>());
		let mut words = (end as usize - d as usize) / WORD;
		while words >= 4 {
			let (a, b, c, e) = (load(sw), load(sw.add(1)), load(sw.add(2)), load(sw.add(3)));
			store(dw, a);
			store(dw.add(1), b);
			store(dw.add(2), c);
			store(dw.add(3), e);
			(dw, sw, words) = (dw.add(4), sw.add(4), words - 4);
		}
		if words >= 2 {
			let (a, b) = (load(sw), load(sw.add(1)));
			store(dw, a);
			store(dw.add(1), b);
			(dw, sw, words) = (dw.add(2), sw.add(2), words - 2);
		}
		if words != 0 {
			store(dw, load(sw));
			(dw, sw) = (dw.add(1), sw.add(1));
		}
		// `d` is word aligned now, and so is `s`: what is left fits in one aligned `u32`, `u16`
		// and `u8`.
		(d, s) = (dw.cast(), sw.cast());
		if end as usize - d as usize >= 4 {
			store(d.cast::<u32>(), load(s.cast::<u32>()));
			(d, s) = (d.add(4), s.add(4));
		}
		if end as usize - d as usize >= 2 {
			store(d.cast::<u16>(), load(s.cast::<u16>()));
			(d, s) = (d.add(2), s.add(2));
		}
		if d != end {
			store(d, load(s));
		}
		return dst;
	}
	while end as usize - d as usize >= 4 {
		let (a, b, c, e) = (load(s), load(s.add(1)), load(s.add(2)), load(s.add(3)));
		store(d, a);
		store(d.add(1), b);
		store(d.add(2), c);
		store(d.add(3), e);
		(d, s) = (d.add(4), s.add(4));
	}
	while d != end {
		store(d, load(s));
		(d, s) = (d.add(1), s.add(1));
	}
	dst
}

/// The `memcmp` result for `a` and `b` read little endian from the same offset: the difference
/// of their first differing bytes, or 0 if they are equal.
#[inline(always)]
fn compare_words(a: u64, b: u64) -> c_int {
	let differ = a ^ b;
	if differ == 0 {
		return 0;
	}
	let shift = differ.trailing_zeros() & !7;
	c_int::from((a >> shift) as u8) - c_int::from((b >> shift) as u8)
}

/// Compare `n` bytes at `s1` and `s2`.
///
/// # Safety
///
/// Both pointers must be valid for reads of `n` bytes.
#[no_mangle]
pub unsafe extern "C" fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> c_int {
	let (mut p, mut q) = (s1, s2);
	let end = s1.add(n);
	if (p as usize ^ q as usize) % WORD == 0 {
		while p != end && p as usize % WORD != 0 {
			let (a, b) = (load(p), load(q));
			if a != b {
				return c_int::from(a) - c_int::from(b);
			}
			(p, q) = (p.add(1), q.add(1));
		}
		let (mut pw, mut qw) = (p.cast::<u64>(), q.cast::<u64>());
		let mut words = (end as usize - p as usize) / WORD;
		// Skip four equal words at a time; the word that differs is found below.
		while words >= 4 {
			let (a0, a1, a2, a3) = (load(pw), load(pw.add(1)), load(pw.add(2)), load(pw.add(3)));
			let (b0, b1, b2, b3) = (load(qw), load(qw.add(1)), load(qw.add(2)), load(qw.add(3)));
			if (a0 ^ b0) | (a1 ^ b1) | (a2 ^ b2) | (a3 ^ b3) != 0 {
				break;
			}
			(pw, qw, words) = (pw.add(4), qw.add(4), words - 4);
		}
		while words != 0 {
			let result = compare_words(load(pw), load(qw));
			if result != 0 {
				return result;
			}
			(pw, qw, words) = (pw.add(1), qw.add(1), words - 1);
		}
		// `p` and `q` are word aligned now: what is left fits in one aligned `u32`, `u16` and
		// `u8`.
		(p, q) = (pw.cast(), qw.cast());
		if end as usize - p as usize >= 4 {
			let result = compare_words(load(p.cast::<u32>()).into(), load(q.cast::<u32>()).into());
			if result != 0 {
				return result;
			}
			(p, q) = (p.add(4), q.add(4));
		}
		if end as usize - p as usize >= 2 {
			let result = compare_words(load(p.cast::<u16>()).into(), load(q.cast::<u16>()).into());
			if result != 0 {
				return result;
			}
			(p, q) = (p.add(2), q.add(2));
		}
	}
	while p != end {
		let (a, b) = (load(p), load(q));
		if a != b {
			return c_int::from(a) - c_int::from(b);
		}
		(p, q) = (p.add(1), q.add(1));
	}
	0
}

/// Compare `n` bytes at `s1` and `s2` for equality.
///
/// # Safety
///
/// Both pointers must be valid for reads of `n` bytes.
#[no_mangle]
pub unsafe extern "C" fn bcmp(s1: *const u8, s2: *const u8, n: usize) -> c_int {
	memcmp(s1, s2, n)
}
