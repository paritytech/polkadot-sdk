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

//! The storage precompile, called through a contract's `delegatecall`.

use crate::{
	Code, Config, H160,
	evm::decode_revert_reason,
	precompiles::STATIC_CALL_DENIED,
	test_utils::{ALICE, builder::Contract},
	tests::{ExtBuilder, Test, builder},
};
use alloy_core::{primitives::U256, sol_types::SolCall};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{FixtureType, StoragePrecompileUser, compile_module_with_type};
use pretty_assertions::assert_eq;
use test_case::test_case;

fn deploy(fixture_type: FixtureType) -> H160 {
	let (code, _) = compile_module_with_type("StoragePrecompileUser", fixture_type).unwrap();
	let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
	let Contract { addr, .. } =
		builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
	addr
}

fn call<C: SolCall>(addr: H160, call: C) -> C::Return {
	let result = builder::bare_call(addr).data(call.abi_encode()).build_and_unwrap_result();
	assert!(!result.did_revert(), "call reverted: {:?}", decode_revert_reason(&result.data));
	C::abi_decode_returns(&result.data).unwrap()
}

fn set(addr: H160, v: u64) {
	call(addr, StoragePrecompileUser::setCall { v: U256::from(v) });
}

fn value(addr: H160) -> U256 {
	call(addr, StoragePrecompileUser::valueCall {})
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn take_storage_returns_and_removes_value(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type);
		set(addr, 42);

		let taken = call(addr, StoragePrecompileUser::takeCall {});

		assert_eq!(taken.as_ref(), U256::from(42).to_be_bytes::<32>().as_slice());
		assert_eq!(value(addr), U256::ZERO);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn clear_storage_removes_value(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type);
		set(addr, 7);

		let cleared = call(addr, StoragePrecompileUser::clearCall {});

		assert!(cleared.containedKey);
		assert_eq!(cleared.valueLen, U256::from(32));
		assert_eq!(value(addr), U256::ZERO);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn take_and_clear_in_static_call_revert_and_keep_value(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let addr = deploy(fixture_type);
		set(addr, 5);
		let expected = format!("revert: {STATIC_CALL_DENIED}");

		let take = call(addr, StoragePrecompileUser::takeInStaticCallCall {});
		assert!(!take.ok);
		assert_eq!(decode_revert_reason(&take.ret), Some(expected.clone()));

		let clear = call(addr, StoragePrecompileUser::clearInStaticCallCall {});
		assert!(!clear.ok);
		assert_eq!(decode_revert_reason(&clear.ret), Some(expected));

		assert_eq!(value(addr), U256::from(5));
	});
}
