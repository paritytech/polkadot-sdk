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

//! Tests for the runtime interface traits and proc macros.
//!
//! This crate uses V2 entry points (runtime-side allocation). Tests for V1 host-side
//! allocation strategies (AllocateAndReturn*) live in `test-wasm-deprecated`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use sp_runtime_interface::{
	pass_by::{
		ConvertAndReturnAs, PassAs, PassFatPointerAndDecodeSlice, PassFatPointerAndRead,
		PassFatPointerAndReadWrite, PassPointerAndRead, PassPointerAndReadCopy,
		PassPointerAndWrite, ReturnAs,
	},
	runtime_interface,
};

#[cfg(not(feature = "std"))]
use core::mem;

use alloc::{vec, vec::Vec};
use sp_core::wasm_export_functions;
use sp_io::RIIntOption;

// Include the WASM binary
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

/// Wasm binary unwrapped. If built with `SKIP_WASM_BUILD`, the function panics.
#[cfg(feature = "std")]
pub fn wasm_binary_unwrap() -> &'static [u8] {
	WASM_BINARY.expect(
		"Development wasm binary is not available. Testing is only \
						supported with the flag disabled.",
	)
}

/// Used in the `test_array_as_mutable_reference` test.
const TEST_ARRAY: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];

#[runtime_interface]
pub trait TestApi {
	// ---- Host functions that don't allocate on the host side ----

	/// Set the storage at key with value.
	fn set_storage(
		&mut self,
		key: PassFatPointerAndRead<&[u8]>,
		data: PassFatPointerAndRead<&[u8]>,
	) {
		self.place_storage(key.to_vec(), Some(data.to_vec()));
	}

	/// Copy `hello` into the given mutable reference
	fn return_value_into_mutable_reference(&self, data: PassFatPointerAndReadWrite<&mut [u8]>) {
		let res = "hello";
		data[..res.len()].copy_from_slice(res.as_bytes());
	}

	/// Take and fill mutable array.
	fn array_as_mutable_reference(data: PassPointerAndWrite<&mut [u8; 16], 16>) {
		data.copy_from_slice(&TEST_ARRAY);
	}

	/// A function that is called with invalid utf8 data from the runtime.
	///
	/// This also checks that we accept `_` (wild card) argument names.
	fn invalid_utf8_data(_: PassFatPointerAndRead<&str>) {}

	/// Overwrite the native implementation in wasm. The native implementation always returns
	/// `false` and the replacement function will return always `true`.
	fn overwrite_native_function_implementation() -> bool {
		false
	}

	fn test_versioning(&self, data: u32) -> bool {
		data == 42 || data == 50
	}

	#[version(2)]
	fn test_versioning(&self, data: u32) -> bool {
		data == 42
	}

	fn test_versioning_register_only(&self, data: u32) -> bool {
		data == 80
	}

	#[version(2, register_only)]
	fn test_versioning_register_only(&self, data: u32) -> bool {
		data == 42
	}

	// ---- V1 marshalling strategies (no host alloc needed) ----

	fn pass_pointer_and_read_copy(value: PassPointerAndReadCopy<[u8; 3], 3>) {
		assert_eq!(value, [1, 2, 3]);
	}

	fn pass_pointer_and_read(value: PassPointerAndRead<&[u8; 3], 3>) {
		assert_eq!(value, &[1, 2, 3]);
	}

	fn pass_fat_pointer_and_read(value: PassFatPointerAndRead<&[u8]>) {
		assert_eq!(value, [1, 2, 3]);
	}

	fn pass_fat_pointer_and_read_write(value: PassFatPointerAndReadWrite<&mut [u8]>) {
		assert_eq!(value, [1, 2, 3]);
		value.copy_from_slice(&[4, 5, 6]);
	}

	fn pass_pointer_and_write(value: PassPointerAndWrite<&mut [u8; 3], 3>) {
		assert_eq!(*value, [0, 0, 0]);
		*value = [1, 2, 3];
	}

	fn pass_by_codec(value: sp_runtime_interface::pass_by::PassFatPointerAndDecode<Vec<u16>>) {
		assert_eq!(value, [1, 2, 3]);
	}

	fn pass_slice_ref_by_codec(value: PassFatPointerAndDecodeSlice<&[u16]>) {
		assert_eq!(value, [1, 2, 3]);
	}

	fn pass_as(value: PassAs<Opaque, u32>) {
		assert_eq!(value.0, 123);
	}

	fn return_as() -> ReturnAs<Opaque, u32> {
		Opaque(123)
	}

	// ---- V2 marshalling strategies (runtime-side allocation) ----

	/// Test PassFatPointerAndWrite: host writes into a runtime-provided buffer.
	#[raw_api]
	fn return_input(
		data: PassFatPointerAndRead<&[u8]>,
		out: sp_runtime_interface::pass_by::PassFatPointerAndWrite<&mut [u8]>,
	) -> u32 {
		let copy_len = data.len().min(out.len());
		out[..copy_len].copy_from_slice(&data[..copy_len]);
		data.len() as u32
	}

	/// Wrapper: developer-friendly interface for return_input.
	#[wrapper]
	fn return_input(data: Vec<u8>) -> Vec<u8> {
		let mut out = vec![0u8; data.len()];
		let len = return_input__raw(&data, &mut out) as usize;
		out.truncate(len);
		out
	}

	/// Test ConvertAndReturnAs: return an `Option<u32>` as `i64`.
	fn return_option_value(
		&self,
		data: u32,
	) -> ConvertAndReturnAs<Option<u32>, RIIntOption<u32>, i64> {
		if data == 0 {
			None
		} else {
			Some(data * 2)
		}
	}

	/// Test PassPointerAndWrite with `#[raw_api]`/`#[wrapper]`.
	#[raw_api]
	fn get_and_return_array(
		data: PassPointerAndReadCopy<[u8; 34], 34>,
		out: PassPointerAndWrite<&mut [u8; 16], 16>,
	) {
		out.copy_from_slice(&data[..16]);
	}

	/// Wrapper for get_and_return_array.
	#[wrapper]
	fn get_and_return_array(data: [u8; 34]) -> [u8; 16] {
		let mut out = [0u8; 16];
		get_and_return_array__raw(data, &mut out);
		out
	}
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Opaque(u32);

impl From<Opaque> for u32 {
	fn from(value: Opaque) -> Self {
		value.0
	}
}

impl TryFrom<u32> for Opaque {
	type Error = ();
	fn try_from(value: u32) -> Result<Self, Self::Error> {
		Ok(Opaque(value))
	}
}

/// This function is not used, but we require it for the compiler to include `sp-io`.
/// `sp-io` is required for its panic and oom handler.
#[no_mangle]
pub fn import_sp_io() {
	sp_io::misc::print_utf8(&[]);
}

wasm_export_functions! {
	fn test_return_data() {
		let input = vec![1, 2, 3, 4, 5, 6];
		let res = test_api::return_input(input.clone());

		assert_eq!(input, res);
	}

	fn test_set_storage() {
		let key = "hello";
		let value = "world";

		test_api::set_storage(key.as_bytes(), value.as_bytes());
	}

	fn test_return_value_into_mutable_reference() {
		let mut data = vec![1, 2, 3, 4, 5, 6];

		test_api::return_value_into_mutable_reference(&mut data);

		let expected = "hello";
		assert_eq!(expected.as_bytes(), &data[..expected.len()]);
	}

	fn test_get_and_return_array() {
		let mut input = unsafe { mem::MaybeUninit::<[u8; 34]>::zeroed().assume_init() };
		input.copy_from_slice(&[
			24, 3, 23, 20, 2, 16, 32, 1, 12, 26, 27, 8, 29, 31, 6, 5, 4, 19, 10, 28, 34, 21, 18, 33, 9,
			13, 22, 25, 15, 11, 30, 7, 14, 17,
		]);

		let res = test_api::get_and_return_array(input);

		assert_eq!(&res, &input[..16]);
	}

	fn test_array_as_mutable_reference() {
		let mut array = [0u8; 16];
		test_api::array_as_mutable_reference(&mut array);

		assert_eq!(array, TEST_ARRAY);
	}

	fn test_invalid_utf8_data_should_return_an_error() {
		let data = vec![0, 159, 146, 150];
		// I'm an evil hacker, trying to hack!
		let data_str = unsafe { alloc::str::from_utf8_unchecked(&data) };

		test_api::invalid_utf8_data(data_str);
	}

	fn test_overwrite_native_function_implementation() {
		fn new_implementation() -> bool {
			true
		}

		// Check native implementation
		assert!(!test_api::overwrite_native_function_implementation());

		let _guard = test_api::host_overwrite_native_function_implementation
			.replace_implementation(new_implementation);

		assert!(test_api::overwrite_native_function_implementation());
	}

	fn test_versioning_works() {
		// we fix new api to accept only 42 as a proper input
		// as opposed to sp-runtime-interface-test-wasm-deprecated::test_api::verify_input
		// which accepted 42 and 50.
		assert!(test_api::test_versioning(42));

		assert!(!test_api::test_versioning(50));
		assert!(!test_api::test_versioning(102));
	}

	fn test_versioning_register_only_works() {
		// Ensure that we will import the version of the runtime interface function that
		// isn't tagged with `register_only`.
		assert!(!test_api::test_versioning_register_only(42));
		assert!(test_api::test_versioning_register_only(80));
	}

	fn test_v2_marshalling_strategies() {
		// Strategies that don't allocate on the host side:
		test_api::pass_pointer_and_read_copy([1_u8, 2, 3]);
		test_api::pass_pointer_and_read(&[1_u8, 2, 3]);
		test_api::pass_fat_pointer_and_read(&[1_u8, 2, 3][..]);
		{
			let mut slice = [1_u8, 2, 3];
			test_api::pass_fat_pointer_and_read_write(&mut slice);
			assert_eq!(slice, [4_u8, 5, 6]);
		}
		{
			let mut slice = [9_u8, 9, 9];
			test_api::pass_pointer_and_write(&mut slice);
			assert_eq!(slice, [1_u8, 2, 3]);
		}
		test_api::pass_by_codec(vec![1_u16, 2, 3]);
		test_api::pass_slice_ref_by_codec(&[1_u16, 2, 3][..]);
		test_api::pass_as(Opaque(123));
		assert_eq!(test_api::return_as(), Opaque(123));

		// V2-specific strategies:
		assert_eq!(test_api::return_option_value(5), Some(10));
		assert_eq!(test_api::return_option_value(0), None);

		let input = vec![10, 20, 30, 40, 50];
		let res = test_api::return_input(input.clone());
		assert_eq!(input, res);

		let mut arr = [0u8; 34];
		arr[0..4].copy_from_slice(&[99, 88, 77, 66]);
		let res = test_api::get_and_return_array(arr);
		assert_eq!(&res, &arr[..16]);
	}
}
