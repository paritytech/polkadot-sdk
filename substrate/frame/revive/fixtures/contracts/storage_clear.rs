#![no_std]
#![no_main]

#[allow(unused_imports)]
use common::unwrap_output;
use uapi::{HostFn, HostFnImpl as api, StorageFlags};

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
	const KEY: [u8; 32] = [1u8; 32];
	const VALUE_A: [u8; 32] = [4u8; 32];
	const ZERO: [u8; 32] = [0u8; 32];

	api::clear_storage(StorageFlags::empty(), &KEY);

	assert_eq!(api::contains_storage(StorageFlags::empty(), &KEY), None);

	let existing = api::set_storage_or_clear(StorageFlags::empty(), &KEY, &VALUE_A);
	assert_eq!(existing, None);

	let mut stored: [u8; 32] = [0u8; 32];
	api::get_storage_or_zero(StorageFlags::empty(), &KEY, &mut stored);
	assert_eq!(stored, VALUE_A);

	let existing = api::set_storage_or_clear(StorageFlags::empty(), &KEY, &ZERO);
	assert_eq!(existing, Some(32));

	let mut cleared: [u8; 32] = [1u8; 32];
	api::get_storage_or_zero(StorageFlags::empty(), &KEY, &mut cleared);
	assert_eq!(cleared, ZERO);
}
