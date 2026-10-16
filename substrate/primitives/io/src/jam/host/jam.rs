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

//! The design §4.3 PVM imports: `gas`, `grow_heap`, `fetch`, `historical_lookup` and `export`.
//!
//! These are provided by the Parachain Service refine to the child PVM. They are runtime-only
//! imports: there is no node side, only the fixed PolkaVM indices.

sp_jam_runtime_interface::host_calls! {
	/// Returns the remaining gas.
	#[index = 0]
	fn ext_pvm_gas() -> u64;

	/// Grows the heap by `size` bytes, returning the previous heap end address, or zero if
	/// the allocation failed. When called with `size == 0`, returns the current heap end
	/// without growing.
	#[index = 1]
	fn ext_pvm_grow_heap(size: usize) -> usize;

	/// Fetches `buffer_len` bytes of the invocation data of kind `kind` (parameterized by
	/// `a` and `b`), starting at `offset`, into `buffer`. Returns the full length of the
	/// fetchable data.
	#[index = 2]
	fn ext_pvm_fetch(
		buffer: *mut u8,
		offset: u64,
		buffer_len: u64,
		kind: u64,
		a: u64,
		b: u64,
	) -> u64;

	/// Looks up a preimage of `hash_ptr` (32 bytes) in the historical state of service
	/// `service_id`, writing up to `out_len` bytes starting at `offset` into `out`. Returns
	/// the full length of the preimage, or `u64::MAX` if there is none.
	#[index = 7]
	fn ext_pvm_historical_lookup(
		service_id: u64,
		hash_ptr: *const u8,
		out: *mut u8,
		offset: u64,
		out_len: u64,
	) -> u64;

	/// Exports `buffer_len` bytes from `buffer` as a segment. Returns the start address of
	/// the exported segment.
	#[index = 8]
	fn ext_pvm_export(buffer: *const u8, buffer_len: u64) -> u64;
}

/// Returns the remaining gas of the current invocation.
///
/// # Safety
///
/// Raw PolkaVM import; only callable inside a JAM PolkaVM execution.
#[cfg(substrate_runtime)]
pub unsafe fn gas() -> u64 {
	// SAFETY: raw import without any pointers.
	unsafe { ext_pvm_gas() }
}

/// Grows the heap by `size` bytes, returning the previous heap end address, or zero if the
/// allocation failed. When called with `size == 0`, returns the current heap end without
/// growing.
///
/// # Safety
///
/// Raw PolkaVM import; only callable inside a JAM PolkaVM execution.
#[cfg(substrate_runtime)]
pub unsafe fn grow_heap(size: usize) -> usize {
	// SAFETY: raw import without any pointers.
	unsafe { ext_pvm_grow_heap(size) }
}

/// Fetches `buffer_len` bytes of the invocation data of kind `kind` (parameterized by `a` and
/// `b`), starting at `offset`, into `buffer`. Returns the full length of the fetchable data.
///
/// # Safety
///
/// Raw PolkaVM import; only callable inside a JAM PolkaVM execution. `buffer` must be valid for
/// writes of `buffer_len` bytes.
#[cfg(substrate_runtime)]
pub unsafe fn fetch(
	buffer: *mut u8,
	offset: u64,
	buffer_len: u64,
	kind: u64,
	a: u64,
	b: u64,
) -> u64 {
	// SAFETY: forwarded from the caller; the host writes at most `buffer_len` bytes into
	// `buffer`.
	unsafe { ext_pvm_fetch(buffer, offset, buffer_len, kind, a, b) }
}

/// Looks up a preimage of `hash_ptr` (32 bytes) in the historical state of service
/// `service_id`, writing up to `out_len` bytes starting at `offset` into `out`. Returns the
/// full length of the preimage, or `u64::MAX` if there is none.
///
/// # Safety
///
/// Raw PolkaVM import; only callable inside a JAM PolkaVM execution. `hash_ptr` must be
/// readable for 32 bytes and `out` must be valid for writes of `out_len` bytes.
#[cfg(substrate_runtime)]
pub unsafe fn historical_lookup(
	service_id: u64,
	hash_ptr: *const u8,
	out: *mut u8,
	offset: u64,
	out_len: u64,
) -> u64 {
	// SAFETY: forwarded from the caller; the host reads 32 bytes from `hash_ptr` and writes at
	// most `out_len` bytes into `out`.
	unsafe { ext_pvm_historical_lookup(service_id, hash_ptr, out, offset, out_len) }
}

/// Exports `buffer_len` bytes from `buffer` as a segment. Returns the start address of the
/// exported segment.
///
/// # Safety
///
/// Raw PolkaVM import; only callable inside a JAM PolkaVM execution. `buffer` must be readable
/// for `buffer_len` bytes.
#[cfg(substrate_runtime)]
pub unsafe fn export(buffer: *const u8, buffer_len: u64) -> u64 {
	// SAFETY: forwarded from the caller; the host reads `buffer_len` bytes from `buffer`.
	unsafe { ext_pvm_export(buffer, buffer_len) }
}

#[cfg(all(test, not(substrate_runtime)))]
mod tests {
	#[test]
	fn pvm_indices_match_spec() {
		assert_eq!(
			super::HOST_CALL_INDICES,
			&[
				(0, "ext_pvm_gas"),
				(1, "ext_pvm_grow_heap"),
				(2, "ext_pvm_fetch"),
				(7, "ext_pvm_historical_lookup"),
				(8, "ext_pvm_export"),
			][..]
		);
	}
}
