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

use super::*;

use crate::{
	debug::{CallInterceptor, CallSpan, ExecResult, ExportedFunction, Tracing},
	primitives::ExecReturnValue,
	test_utils::*,
};
use frame_support::traits::Currency;
use sp_core::H160;
use std::cell::RefCell;

#[derive(Clone, PartialEq, Eq, Debug)]
struct DebugFrame {
	contract_address: sp_core::H160,
	call: ExportedFunction,
	input: Vec<u8>,
	result: Option<Vec<u8>>,
}

thread_local! {
	static DEBUG_EXECUTION_TRACE: RefCell<Vec<DebugFrame>> = RefCell::new(Vec::new());
	static INTERCEPTED_ADDRESS: RefCell<Option<sp_core::H160>> = RefCell::new(None);
}

pub struct TestDebug;
pub struct TestCallSpan {
	contract_address: sp_core::H160,
	call: ExportedFunction,
	input: Vec<u8>,
}

impl Tracing<Test> for TestDebug {
	type CallSpan = TestCallSpan;

	fn new_call_span(
		contract_address: &crate::H160,
		entry_point: ExportedFunction,
		input_data: &[u8],
	) -> TestCallSpan {
		DEBUG_EXECUTION_TRACE.with(|d| {
			d.borrow_mut().push(DebugFrame {
				contract_address: *contract_address,
				call: entry_point,
				input: input_data.to_vec(),
				result: None,
			})
		});
		TestCallSpan {
			contract_address: *contract_address,
			call: entry_point,
			input: input_data.to_vec(),
		}
	}
}

impl CallInterceptor<Test> for TestDebug {
	fn intercept_call(
		contract_address: &sp_core::H160,
		_entry_point: ExportedFunction,
		_input_data: &[u8],
	) -> Option<ExecResult> {
		INTERCEPTED_ADDRESS.with(|i| {
			if i.borrow().as_ref() == Some(contract_address) {
				Some(Ok(ExecReturnValue { flags: ReturnFlags::REVERT, data: vec![] }))
			} else {
				None
			}
		})
	}
}

impl CallSpan for TestCallSpan {
	fn after_call(self, output: &ExecReturnValue) {
		DEBUG_EXECUTION_TRACE.with(|d| {
			d.borrow_mut().push(DebugFrame {
				contract_address: self.contract_address,
				call: self.call,
				input: self.input,
				result: Some(output.data.clone()),
			})
		});
	}
}

/// We can only run the tests if we have a riscv toolchain installed
#[cfg(feature = "riscv")]
mod run_tests {
	use super::*;
	use pretty_assertions::assert_eq;

	#[test]
	fn debugging_works() {
		let (wasm_caller, _) = compile_module("call").unwrap();
		let (wasm_callee, _) = compile_module("store_call").unwrap();

		fn current_stack() -> Vec<DebugFrame> {
			DEBUG_EXECUTION_TRACE.with(|stack| stack.borrow().clone())
		}

		fn deploy(wasm: Vec<u8>) -> H160 {
			Contracts::bare_instantiate(
				RuntimeOrigin::signed(ALICE),
				0,
				GAS_LIMIT,
				deposit_limit::<Test>(),
				Code::Upload(wasm),
				vec![],
				Some([0u8; 32]),
				DebugInfo::Skip,
				CollectEvents::Skip,
			)
			.result
			.unwrap()
			.addr
		}

		fn constructor_frame(contract_address: &H160, after: bool) -> DebugFrame {
			DebugFrame {
				contract_address: *contract_address,
				call: ExportedFunction::Constructor,
				input: vec![],
				result: if after { Some(vec![]) } else { None },
			}
		}

		fn call_frame(contract_address: &H160, args: Vec<u8>, after: bool) -> DebugFrame {
			DebugFrame {
				contract_address: *contract_address,
				call: ExportedFunction::Call,
				input: args,
				result: if after { Some(vec![]) } else { None },
			}
		}

		ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
			let _ = Balances::deposit_creating(&ALICE, 1_000_000);

			assert_eq!(current_stack(), vec![]);

			let addr_caller = deploy(wasm_caller);
			let addr_callee = deploy(wasm_callee);

			assert_eq!(
				current_stack(),
				vec![
					constructor_frame(&addr_caller, false),
					constructor_frame(&addr_caller, true),
					constructor_frame(&addr_callee, false),
					constructor_frame(&addr_callee, true),
				]
			);

			let main_args = (100u32, &addr_callee.clone()).encode();
			let inner_args = (100u32).encode();

			assert_ok!(Contracts::call(
				RuntimeOrigin::signed(ALICE),
				addr_caller,
				0,
				GAS_LIMIT,
				deposit_limit::<Test>(),
				main_args.clone()
			));

			let stack_top = current_stack()[4..].to_vec();
			assert_eq!(
				stack_top,
				vec![
					call_frame(&addr_caller, main_args.clone(), false),
					call_frame(&addr_callee, inner_args.clone(), false),
					call_frame(&addr_callee, inner_args, true),
					call_frame(&addr_caller, main_args, true),
				]
			);
		});
	}

	#[test]
	fn call_interception_works() {
		let (wasm, _) = compile_module("dummy").unwrap();

		ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
			let _ = Balances::deposit_creating(&ALICE, 1_000_000);

			let account_id = Contracts::bare_instantiate(
				RuntimeOrigin::signed(ALICE),
				0,
				GAS_LIMIT,
				deposit_limit::<Test>(),
				Code::Upload(wasm),
				vec![],
				// some salt to ensure that the address of this contract is unique among all tests
				Some([0x41; 32]),
				DebugInfo::Skip,
				CollectEvents::Skip,
			)
			.result
			.unwrap()
			.addr;

			// no interception yet
			assert_ok!(Contracts::call(
				RuntimeOrigin::signed(ALICE),
				account_id,
				0,
				GAS_LIMIT,
				deposit_limit::<Test>(),
				vec![],
			));

			// intercept calls to this contract
			INTERCEPTED_ADDRESS.with(|i| *i.borrow_mut() = Some(account_id));

			assert_err_ignore_postinfo!(
				Contracts::call(
					RuntimeOrigin::signed(ALICE),
					account_id,
					0,
					GAS_LIMIT,
					deposit_limit::<Test>(),
					vec![],
				),
				<Error<Test>>::ContractReverted,
			);
		});
	}
}
