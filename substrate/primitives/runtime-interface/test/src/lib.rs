// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Integration tests for runtime interface primitives
#![cfg(test)]

use sp_runtime_interface::*;
use sp_runtime_interface_test_wasm::{WASM_BINARY, test_api::HostFunctions};
use sp_wasm_interface::HostFunctions as HostFunctionsT;

type TestExternalities = sp_state_machine::TestExternalities<sp_core::Blake2Hasher, u64>;

fn call_wasm_method<HF: HostFunctionsT>(method: &str) -> TestExternalities {
	let mut ext = TestExternalities::default();
	let mut ext_ext = ext.ext();

	sc_executor::call_in_wasm::<
		_,
		(
			HF,
			sp_io::SubstrateHostFunctions,
			sc_executor::deprecated_host_interface::SubstrateExternals
		)
	>(
		method,
		&[],
		sc_executor::WasmExecutionMethod::Interpreted,
		&mut ext_ext,
		&WASM_BINARY[..],
		8,
	).expect(&format!("Executes `{}`", method));

	ext
}

#[test]
fn test_return_data() {
	call_wasm_method::<HostFunctions>("test_return_data");
}

#[test]
fn test_return_option_data() {
	call_wasm_method::<HostFunctions>("test_return_option_data");
}

#[test]
fn test_set_storage() {
	let mut ext = call_wasm_method::<HostFunctions>("test_set_storage");

	let expected = "world";
	assert_eq!(expected.as_bytes(), &ext.ext().storage("hello".as_bytes()).unwrap()[..]);
}

#[test]
fn test_return_value_into_mutable_reference() {
	call_wasm_method::<HostFunctions>("test_return_value_into_mutable_reference");
}

#[test]
fn test_get_and_return_array() {
	call_wasm_method::<HostFunctions>("test_get_and_return_array");
}

#[test]
fn test_array_as_mutable_reference() {
	call_wasm_method::<HostFunctions>("test_array_as_mutable_reference");
}

#[test]
fn test_return_input_public_key() {
	call_wasm_method::<HostFunctions>("test_return_input_public_key");
}

#[test]
#[should_panic(
	expected = "Other(\"Instantiation: Export ext_test_api_return_input_version_1 not found\")"
)]
fn host_function_not_found() {
	call_wasm_method::<()>("test_return_data");
}

#[test]
#[should_panic(
	expected =
		"FunctionExecution(\"ext_test_api_invalid_utf8_data_version_1\", \
		\"Invalid utf8 data provided\")"
)]
fn test_invalid_utf8_data_should_return_an_error() {
	call_wasm_method::<HostFunctions>("test_invalid_utf8_data_should_return_an_error");
}

#[test]
fn test_overwrite_native_function_implementation() {
	call_wasm_method::<HostFunctions>("test_overwrite_native_function_implementation");
}
