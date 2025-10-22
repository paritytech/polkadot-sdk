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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use sp_runtime_interface::{
	pass_by::{
		AllocateAndReturnByCodec, AllocateAndReturnFatPointer, AllocateAndReturnPointer, PassAs,
		PassFatPointerAndDecode, PassFatPointerAndDecodeSlice, PassFatPointerAndRead,
		PassFatPointerAndReadWrite, PassPointerAndRead, PassPointerAndReadCopy,
		PassPointerAndWrite, ReturnAs,
	},
	runtime_interface,
};

#[cfg(not(feature = "std"))]
use core::mem;

use alloc::{vec, vec::Vec};
use sp_core::{sr25519::Public, wasm_export_functions};

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
	/// Returns the input data as result.
	fn return_input(data: PassFatPointerAndRead<Vec<u8>>) -> AllocateAndReturnFatPointer<Vec<u8>> {
		data
	}

	/// Returns 16kb data.
	///
	/// # Note
	///
	/// We return a `Vec<u32>` because this will use the code path that uses SCALE
	/// to pass the data between native/wasm. (`Vec<u8>` is passed without encoding the
	/// data)
	fn return_16kb() -> AllocateAndReturnByCodec<Vec<u32>> {
		vec![0; 4 * 1024]
	}

	fn return_option_vec() -> AllocateAndReturnByCodec<Option<Vec<u8>>> {
		let mut vec = Vec::new();
		vec.resize(16 * 1024, 0xAA);
		Some(vec)
	}

	fn return_option_bytes() -> AllocateAndReturnByCodec<Option<bytes::Bytes>> {
		let mut vec = Vec::new();
		vec.resize(16 * 1024, 0xAA);
		Some(vec.into())
	}

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

	/// Returns the input data wrapped in an `Option` as result.
	fn return_option_input(
		data: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Option<Vec<u8>>> {
		Some(data)
	}

	/// Get an array as input and returns a subset of this array.
	fn get_and_return_array(
		data: PassPointerAndReadCopy<[u8; 34], 34>,
	) -> AllocateAndReturnPointer<[u8; 16], 16> {
		let mut res = [0u8; 16];
		res.copy_from_slice(&data[..16]);
		res
	}

	/// Take and fill mutable array.
	fn array_as_mutable_reference(data: PassPointerAndWrite<&mut [u8; 16], 16>) {
		data.copy_from_slice(&TEST_ARRAY);
	}

	/// Returns the given public key as result.
	fn return_input_public_key(
		key: PassPointerAndReadCopy<Public, 32>,
	) -> AllocateAndReturnPointer<Public, 32> {
		key
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

	/// Returns the input values as tuple.
	fn return_input_as_tuple(
		a: PassFatPointerAndRead<Vec<u8>>,
		b: u32,
		c: PassFatPointerAndDecode<Option<Vec<u32>>>,
		d: u8,
	) -> AllocateAndReturnByCodec<(Vec<u8>, u32, Option<Vec<u32>>, u8)> {
		(a, b, c, d)
	}

	// Host functions for testing every marshaling strategy:

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

	fn pass_by_codec(value: PassFatPointerAndDecode<Vec<u16>>) {
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

	fn allocate_and_return_pointer() -> AllocateAndReturnPointer<[u8; 3], 3> {
		[1, 2, 3]
	}

	fn allocate_and_return_fat_pointer() -> AllocateAndReturnFatPointer<Vec<u8>> {
		vec![1, 2, 3]
	}

	fn allocate_and_return_by_codec() -> AllocateAndReturnByCodec<Vec<u16>> {
		vec![1, 2, 3]
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

	fn test_return_option_data() {
		let input = vec![1, 2, 3, 4, 5, 6];
		let res = test_api::return_option_input(input.clone());

		assert_eq!(Some(input), res);
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

	fn test_return_input_public_key() {
		let key = Public::try_from(
			&[
				1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
				25, 26, 27, 28, 29, 30, 31, 32,
			][..],
		).unwrap();
		let ret_key = test_api::return_input_public_key(key.clone());

		let key_data: &[u8] = key.as_ref();
		let ret_key_data: &[u8] = ret_key.as_ref();
		assert_eq!(key_data, ret_key_data);
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

	fn test_vec_return_value_memory_is_freed() {
		let mut len = 0;
		for _ in 0..1024 {
			len += test_api::return_16kb().len();
		}
		assert_eq!(1024 * 1024 * 4, len);
	}

	fn test_encoded_return_value_memory_is_freed() {
		let mut len = 0;
		for _ in 0..1024 {
			len += test_api::return_option_input(vec![0; 16 * 1024]).map(|v| v.len()).unwrap();
		}
		assert_eq!(1024 * 1024 * 16, len);
	}

	fn test_array_return_value_memory_is_freed() {
		let mut len = 0;
		for _ in 0..1024 * 1024 {
			len += test_api::get_and_return_array([0; 34])[1];
		}
		assert_eq!(0, len);
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

	fn test_return_input_as_tuple() {
		let a = vec![1, 3, 4, 5];
		let b = 10000;
		let c = Some(vec![2, 3]);
		let d = 5;

		let res = test_api::return_input_as_tuple(a.clone(), b, c.clone(), d);

		assert_eq!(a, res.0);
		assert_eq!(b, res.1);
		assert_eq!(c, res.2);
		assert_eq!(d, res.3);
	}

	fn test_return_option_vec() {
		test_api::return_option_vec();
	}

	fn test_return_option_bytes() {
		test_api::return_option_bytes();
	}

	fn test_marshalling_strategies() {
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
		assert_eq!(test_api::allocate_and_return_pointer(), [1_u8, 2, 3]);
		assert_eq!(test_api::allocate_and_return_fat_pointer(), vec![1_u8, 2, 3]);
		assert_eq!(test_api::allocate_and_return_by_codec(), vec![1_u16, 2, 3]);
	}
}
