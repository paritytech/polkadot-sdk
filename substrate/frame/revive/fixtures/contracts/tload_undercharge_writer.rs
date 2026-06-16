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

//! Test fixture that writes a non-32-byte transient value at the `Key::Fix([0; 32])` slot,
//! then delegate-calls an EVM contract whose runtime code is `TLOAD(0); MSTORE(0, val); RETURN`.
//!
//! The safe `api::set_storage` always passes `key.len() as u32` so it can't reach the
//! `key_len == SENTINEL` branch in the pallet's `decode_key`, which is the only path that
//! lands a value under `Key::Fix(...)` (the namespace EVM TLOAD/TSTORE read). To exercise the
//! cross-VM TLOAD undercharge regression, we re-declare the raw `set_storage` syscall here and
//! invoke it with `key_len = SENTINEL` and an arbitrary `value_len`.

#![no_std]
#![no_main]
include!("../panic_handler.rs");

use uapi::{CallFlags, HostFn, HostFnImpl as api, input};

#[polkavm_derive::polkavm_import]
extern "C" {
	fn set_storage(
		flags: u32,
		key_ptr: *const u8,
		key_len: u32,
		value_ptr: *const u8,
		value_len: u32,
	) -> u32;
}

const SENTINEL: u32 = u32::MAX;
const TRANSIENT_FLAG: u32 = 0x0000_0001;

// Static value buffer sized for `limits::TRANSIENT_STORAGE_BYTES` so any requested length fits.
static VALUE_BUFFER: [u8; 4096] = [0xAAu8; 4096];

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	input!(
		callee: &[u8; 20],
		value_len: u32,
	);

	let key = [0u8; 32];

	// `value_len == 0` means "skip the write" — used by the regression test to get a baseline
	// where EVM TLOAD reads `None` (not an empty value, which would trap). For non-zero
	// `value_len` we do the direct syscall — `key_len = SENTINEL` routes to `Key::Fix(key)`
	// in the pallet, and `value_len` is arbitrary, so we land a non-32-byte value at the
	// EVM-visible slot.
	if value_len != 0 {
		unsafe {
			let _ = set_storage(
				TRANSIENT_FLAG,
				key.as_ptr(),
				SENTINEL,
				VALUE_BUFFER.as_ptr(),
				value_len,
			);
		}
	}

	// Delegate-call into the EVM TLOAD reader. Delegate semantics keep this contract's
	// storage namespace, so the EVM TLOAD reads the transient we just wrote.
	let _ = api::delegate_call(
		CallFlags::empty(),
		callee,
		u64::MAX,
		u64::MAX,
		&[u8::MAX; 32],
		&[],
		None,
	);
}
