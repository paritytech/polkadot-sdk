// SPDX-License-Identifier: Apache-2.0
// Minimal PPN test contract: stores or clears a 32-byte value under a fixed
// key.
//
// Call ABI: 1 selector byte + (optionally) 32 payload bytes
//   selector == 0: set storage[KEY] = next 32 input bytes
//   selector == 1: clear storage[KEY] (no payload)
//
// Implementation note: the only "clear" primitive `pallet-revive-uapi`
// exposes is `set_storage_or_clear(flags, key, value)` — it treats an
// all-zero value as a clear. So storing an actual all-zero value is
// indistinguishable from clearing here, which is fine for the test (we use
// a non-zero pattern when we want to assert "stored").

#![no_std]
#![no_main]

include!("../panic_handler.rs");

use uapi::{HostFn, HostFnImpl as api, StorageFlags};

const KEY: [u8; 32] = [0xab; 32];

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	let size = api::call_data_size() as usize;
	if size == 0 {
		return;
	}

	let mut sel = [0u8; 1];
	api::call_data_copy(&mut sel, 0);

	match sel[0] {
		0 => {
			// Selector 0: set. Expect 32 bytes of payload after the selector.
			let mut value = [0u8; 32];
			if size >= 33 {
				api::call_data_copy(&mut value, 1);
			}
			api::set_storage_or_clear(StorageFlags::empty(), &KEY, &value);
		}
		1 => {
			// Selector 1: clear by writing the zero value.
			let zero = [0u8; 32];
			api::set_storage_or_clear(StorageFlags::empty(), &KEY, &zero);
		}
		_ => {
			// Unknown selector — no-op.
		}
	}
}
