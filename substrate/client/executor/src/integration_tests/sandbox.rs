// Copyright 2018-2020 Parity Technologies (UK) Ltd.
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

use super::{TestExternalities, call_in_wasm};
use crate::WasmExecutionMethod;

use codec::Encode;
use sc_runtime_test::WASM_BINARY;
use test_case::test_case;
use wabt;

#[test_case(WasmExecutionMethod::Interpreted)]
#[cfg_attr(feature = "wasmtime", test_case(WasmExecutionMethod::Compiled))]
fn sandbox_should_work(wasm_method: WasmExecutionMethod) {
	let mut ext = TestExternalities::default();
	let mut ext = ext.ext();
	let test_code = WASM_BINARY;

	let code = wabt::wat2wasm(r#"
		(module
			(import "env" "assert" (func $assert (param i32)))
			(import "env" "inc_counter" (func $inc_counter (param i32) (result i32)))
			(func (export "call")
				(drop
					(call $inc_counter (i32.const 5))
				)

				(call $inc_counter (i32.const 3))
				;; current counter value is on the stack

				;; check whether current == 8
				i32.const 8
				i32.eq

				call $assert
			)
		)
		"#).unwrap().encode();

	assert_eq!(
		call_in_wasm(
			"test_sandbox",
			&code,
			wasm_method,
			&mut ext,
			&test_code[..],
			8,
		).unwrap(),
		true.encode(),
	);
}

#[test_case(WasmExecutionMethod::Interpreted)]
#[cfg_attr(feature = "wasmtime", test_case(WasmExecutionMethod::Compiled))]
fn sandbox_trap(wasm_method: WasmExecutionMethod) {
	let mut ext = TestExternalities::default();
	let mut ext = ext.ext();
	let test_code = WASM_BINARY;

	let code = wabt::wat2wasm(r#"
		(module
			(import "env" "assert" (func $assert (param i32)))
			(func (export "call")
				i32.const 0
				call $assert
			)
		)
		"#).unwrap();

	assert_eq!(
		call_in_wasm(
			"test_sandbox",
			&code,
			wasm_method,
			&mut ext,
			&test_code[..],
			8,
		).unwrap(),
		vec![0],
	);
}

#[test_case(WasmExecutionMethod::Interpreted)]
#[cfg_attr(feature = "wasmtime", test_case(WasmExecutionMethod::Compiled))]
#[should_panic(expected = "Allocator ran out of space")]
fn sandbox_should_trap_when_heap_exhausted(wasm_method: WasmExecutionMethod) {
	let mut ext = TestExternalities::default();
	let mut ext = ext.ext();
	let test_code = WASM_BINARY;

	let code = wabt::wat2wasm(r#"
		(module
			(import "env" "assert" (func $assert (param i32)))
			(func (export "call")
				i32.const 0
				call $assert
			)
		)
		"#).unwrap().encode();

	call_in_wasm(
		"test_exhaust_heap",
		&code,
		wasm_method,
		&mut ext,
		&test_code[..],
		8,
	).unwrap();
}

#[test_case(WasmExecutionMethod::Interpreted)]
#[cfg_attr(feature = "wasmtime", test_case(WasmExecutionMethod::Compiled))]
fn start_called(wasm_method: WasmExecutionMethod) {
	let mut ext = TestExternalities::default();
	let mut ext = ext.ext();
	let test_code = WASM_BINARY;

	let code = wabt::wat2wasm(r#"
		(module
			(import "env" "assert" (func $assert (param i32)))
			(import "env" "inc_counter" (func $inc_counter (param i32) (result i32)))

			;; Start function
			(start $start)
			(func $start
				;; Increment counter by 1
				(drop
					(call $inc_counter (i32.const 1))
				)
			)

			(func (export "call")
				;; Increment counter by 1. The current value is placed on the stack.
				(call $inc_counter (i32.const 1))

				;; Counter is incremented twice by 1, once there and once in `start` func.
				;; So check the returned value is equal to 2.
				i32.const 2
				i32.eq
				call $assert
			)
		)
		"#).unwrap().encode();

	assert_eq!(
		call_in_wasm(
			"test_sandbox",
			&code,
			wasm_method,
			&mut ext,
			&test_code[..],
			8,
		).unwrap(),
		true.encode(),
	);
}

#[test_case(WasmExecutionMethod::Interpreted)]
#[cfg_attr(feature = "wasmtime", test_case(WasmExecutionMethod::Compiled))]
fn invoke_args(wasm_method: WasmExecutionMethod) {
	let mut ext = TestExternalities::default();
	let mut ext = ext.ext();
	let test_code = WASM_BINARY;

	let code = wabt::wat2wasm(r#"
		(module
			(import "env" "assert" (func $assert (param i32)))

			(func (export "call") (param $x i32) (param $y i64)
				;; assert that $x = 0x12345678
				(call $assert
					(i32.eq
						(get_local $x)
						(i32.const 0x12345678)
					)
				)

				(call $assert
					(i64.eq
						(get_local $y)
						(i64.const 0x1234567887654321)
					)
				)
			)
		)
		"#).unwrap().encode();

	assert_eq!(
		call_in_wasm(
			"test_sandbox_args",
			&code,
			wasm_method,
			&mut ext,
			&test_code[..],
			8,
		).unwrap(),
		true.encode(),
	);
}

#[test_case(WasmExecutionMethod::Interpreted)]
#[cfg_attr(feature = "wasmtime", test_case(WasmExecutionMethod::Compiled))]
fn return_val(wasm_method: WasmExecutionMethod) {
	let mut ext = TestExternalities::default();
	let mut ext = ext.ext();
	let test_code = WASM_BINARY;

	let code = wabt::wat2wasm(r#"
		(module
			(func (export "call") (param $x i32) (result i32)
				(i32.add
					(get_local $x)
					(i32.const 1)
				)
			)
		)
		"#).unwrap().encode();

	assert_eq!(
		call_in_wasm(
			"test_sandbox_return_val",
			&code,
			wasm_method,
			&mut ext,
			&test_code[..],
			8,
		).unwrap(),
		true.encode(),
	);
}

#[test_case(WasmExecutionMethod::Interpreted)]
#[cfg_attr(feature = "wasmtime", test_case(WasmExecutionMethod::Compiled))]
fn unlinkable_module(wasm_method: WasmExecutionMethod) {
	let mut ext = TestExternalities::default();
	let mut ext = ext.ext();
	let test_code = WASM_BINARY;

	let code = wabt::wat2wasm(r#"
		(module
			(import "env" "non-existent" (func))

			(func (export "call")
			)
		)
		"#).unwrap().encode();

	assert_eq!(
		call_in_wasm(
			"test_sandbox_instantiate",
			&code,
			wasm_method,
			&mut ext,
			&test_code[..],
			8,
		).unwrap(),
		1u8.encode(),
	);
}

#[test_case(WasmExecutionMethod::Interpreted)]
#[cfg_attr(feature = "wasmtime", test_case(WasmExecutionMethod::Compiled))]
fn corrupted_module(wasm_method: WasmExecutionMethod) {
	let mut ext = TestExternalities::default();
	let mut ext = ext.ext();
	let test_code = WASM_BINARY;

	// Corrupted wasm file
	let code = vec![0u8, 0, 0, 0, 1, 0, 0, 0].encode();

	assert_eq!(
		call_in_wasm(
			"test_sandbox_instantiate",
			&code,
			wasm_method,
			&mut ext,
			&test_code[..],
			8,
		).unwrap(),
		1u8.encode(),
	);
}

#[test_case(WasmExecutionMethod::Interpreted)]
#[cfg_attr(feature = "wasmtime", test_case(WasmExecutionMethod::Compiled))]
fn start_fn_ok(wasm_method: WasmExecutionMethod) {
	let mut ext = TestExternalities::default();
	let mut ext = ext.ext();
	let test_code = WASM_BINARY;

	let code = wabt::wat2wasm(r#"
		(module
			(func (export "call")
			)

			(func $start
			)

			(start $start)
		)
		"#).unwrap().encode();

	assert_eq!(
		call_in_wasm(
			"test_sandbox_instantiate",
			&code,
			wasm_method,
			&mut ext,
			&test_code[..],
			8,
		).unwrap(),
		0u8.encode(),
	);
}

#[test_case(WasmExecutionMethod::Interpreted)]
#[cfg_attr(feature = "wasmtime", test_case(WasmExecutionMethod::Compiled))]
fn start_fn_traps(wasm_method: WasmExecutionMethod) {
	let mut ext = TestExternalities::default();
	let mut ext = ext.ext();
	let test_code = WASM_BINARY;

	let code = wabt::wat2wasm(r#"
		(module
			(func (export "call")
			)

			(func $start
				unreachable
			)

			(start $start)
		)
		"#).unwrap().encode();

	assert_eq!(
		call_in_wasm(
			"test_sandbox_instantiate",
			&code,
			wasm_method,
			&mut ext,
			&test_code[..],
			8,
		).unwrap(),
		2u8.encode(),
	);
}
