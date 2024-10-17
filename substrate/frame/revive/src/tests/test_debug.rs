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
	debug::{CallInterceptor, ExecResult, ExportedFunction, Tracer},
	primitives::ExecReturnValue,
	test_utils::*,
};
use frame_support::traits::Currency;
use std::cell::RefCell;

thread_local! {
	static INTERCEPTED_ADDRESS: RefCell<Option<sp_core::H160>> = RefCell::new(None);
}

pub struct TestDebug;

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

/// We can only run the tests if we have a riscv toolchain installed
#[cfg(feature = "riscv")]
mod run_tests {
	use super::*;
	use pretty_assertions::assert_eq;

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
				Tracer::Disabled,
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
