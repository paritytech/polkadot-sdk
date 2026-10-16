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

//! The design §4.3 service imports: `set_parent_head_hash`, `set_head`, `send_upward_message`
//! and `report_error`.
//!
//! These are provided by the Parachain Service refine to the child PVM. They are runtime-only
//! imports: there is no node side, only the fixed PolkaVM indices.

sp_jam_runtime_interface::host_calls! {
	/// Sets the parent head hash; `hash_ptr` points to 32 bytes.
	#[index = 200]
	fn ext_service_set_parent_head_hash(hash_ptr: *const u8);

	/// Sets the head (the SCALE-encoded header of the block being built) to the `len` bytes
	/// at `ptr`.
	#[index = 201]
	fn ext_service_set_head(ptr: *const u8, len: u64);

	/// Sends an upward message: the SCALE-encoded `UpwardMessage` in the `len` bytes at
	/// `ptr`.
	#[index = 202]
	fn ext_service_send_upward_message(ptr: *const u8, len: u64);

	/// Reports a fatal error with the message in the `len` bytes at `ptr` and traps; never
	/// returns.
	#[index = 203]
	fn ext_service_report_error(ptr: *const u8, len: u64) -> u64;
}

/// Sets the parent head hash; `hash_ptr` points to 32 bytes.
///
/// # Safety
///
/// Raw PolkaVM import; only callable inside a JAM PolkaVM execution. `hash_ptr` must be
/// readable for 32 bytes.
#[cfg(substrate_runtime)]
pub unsafe fn set_parent_head_hash(hash_ptr: *const u8) {
	// SAFETY: forwarded from the caller; the host reads 32 bytes from `hash_ptr`.
	unsafe { ext_service_set_parent_head_hash(hash_ptr) }
}

/// Sets the head (the SCALE-encoded header of the block being built) to the `len` bytes at
/// `ptr`.
///
/// # Safety
///
/// Raw PolkaVM import; only callable inside a JAM PolkaVM execution. `ptr` must be readable
/// for `len` bytes.
#[cfg(substrate_runtime)]
pub unsafe fn set_head(ptr: *const u8, len: u64) {
	// SAFETY: forwarded from the caller; the host reads `len` bytes from `ptr`.
	unsafe { ext_service_set_head(ptr, len) }
}

/// Sends an upward message: the SCALE-encoded `UpwardMessage` in the `len` bytes at `ptr`.
///
/// # Safety
///
/// Raw PolkaVM import; only callable inside a JAM PolkaVM execution. `ptr` must be readable
/// for `len` bytes.
#[cfg(substrate_runtime)]
pub unsafe fn send_upward_message(ptr: *const u8, len: u64) {
	// SAFETY: forwarded from the caller; the host reads `len` bytes from `ptr`.
	unsafe { ext_service_send_upward_message(ptr, len) }
}

/// Reports a fatal error with the message in the `len` bytes at `ptr` and traps; never
/// returns.
///
/// # Safety
///
/// Raw PolkaVM import; only callable inside a JAM PolkaVM execution. `ptr` must be readable
/// for `len` bytes.
#[cfg(substrate_runtime)]
pub unsafe fn report_error(ptr: *const u8, len: u64) -> ! {
	// SAFETY: forwarded from the caller; the host reads `len` bytes from `ptr` and traps.
	let _ = unsafe { ext_service_report_error(ptr, len) };
	crate::jam::unreachable()
}

#[cfg(all(test, not(substrate_runtime)))]
mod tests {
	#[test]
	fn service_indices_match_spec() {
		assert_eq!(
			super::HOST_CALL_INDICES,
			&[
				(200, "ext_service_set_parent_head_hash"),
				(201, "ext_service_set_head"),
				(202, "ext_service_send_upward_message"),
				(203, "ext_service_report_error"),
			][..]
		);
	}
}
