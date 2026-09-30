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

use crate::{
	Code, Config, Error, Pallet,
	test_utils::{ALICE, builder::Contract},
	tests::{ExtBuilder, GasScale, RuntimeOrigin, Test, builder},
};
use alloy_core::{
	primitives::U256,
	sol_types::{SolCall, SolConstructor, SolValue},
};
use codec::Encode;
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{
	FixtureType, NestedWritingReceiver, NestedWritingReceiver::Nesting, StipendSender, StipendTest,
	WarmWriteSender, WritingReceiver, compile_module, compile_module_with_type,
};
use test_case::{test_case, test_matrix};

/// Runs `test` under the given gas scale, then restores the default scale.
fn with_gas_scale<R>(scale: u32, test: impl FnOnce() -> R) -> R {
	struct RestoreGasScale(u32);
	impl Drop for RestoreGasScale {
		fn drop(&mut self) {
			GasScale::set(self.0);
		}
	}
	let _restore = RestoreGasScale(GasScale::get());
	GasScale::set(scale);
	test()
}

#[test]
fn evm_call_stipends_work_for_transfers() {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(StipendTest::testTransferCall {}.abi_encode())
			.evm_value(1_000_000_u128.into())
			.build();

		assert!(!result.result.unwrap().did_revert());
	});
}

#[test]
fn evm_call_stipends_work_for_sends() {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(StipendTest::testSendCall {}.abi_encode())
			.evm_value(1_000_000_u128.into())
			.build();

		assert!(!result.result.unwrap().did_revert());
	});
}

#[test]
fn evm_call_stipends_work_for_transfer_zero() {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(StipendTest::testTransferZeroCall {}.abi_encode())
			.build();

		assert!(!result.result.unwrap().did_revert());
	});
}

#[test]
fn evm_call_stipends_work_for_send_zero() {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(StipendTest::testSendZeroCall {}.abi_encode())
			.build();

		assert!(!result.result.unwrap().did_revert());
	});
}

#[test]
fn evm_call_stipends_work_for_calls() {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(StipendTest::testCallCall {}.abi_encode())
			.evm_value(1_000_000_u128.into())
			.build();

		assert!(!result.result.unwrap().did_revert());
	});
}

#[test]
fn evm_call_stipend_prevents_transfer_reentrancy() {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(StipendTest::testTransferReentrancyCall {}.abi_encode())
			.evm_value(1_000_000_u128.into())
			.build();

		assert!(!result.result.unwrap().did_revert());
	});
}

#[test]
fn evm_call_stipend_prevents_send_reentrancy() {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(StipendTest::testSendReentrancyCall {}.abi_encode())
			.evm_value(1_000_000_u128.into())
			.build();

		assert!(!result.result.unwrap().did_revert());
	});
}

#[test_case(FixtureType::Solc;   "solc")]
#[test_case(FixtureType::Resolc; "resolc")]
fn evm_call_stipend_denies_reentrancy_for_transfer_and_send_only(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("StipendSender", fixture_type).unwrap();
	let (probe_code, _) = compile_module_with_type("ReentrancyProbe", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);
		let Contract { addr: probe, .. } =
			builder::bare_instantiate(Code::Upload(probe_code)).build_and_unwrap_contract();
		let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code))
			.constructor_data(
				StipendSender::constructorCall { _probe: probe.0.into() }.abi_encode(),
			)
			.build_and_unwrap_contract();

		let value = 1_000_000_u128;
		let run = |evm_value: u128, call: Vec<u8>| {
			let result = builder::bare_call(addr)
				.data(call)
				.evm_value(evm_value.into())
				.build_and_unwrap_result();
			assert!(!result.did_revert(), "the call into StipendSender should not revert");
			bool::abi_decode(&result.data).unwrap()
		};

		// On PVM the stipend alone cannot pay for the reentrant call.
		if fixture_type == FixtureType::Solc {
			assert!(
				run(value, StipendSender::isTransferDeniedCall {}.abi_encode()),
				"transfer forwards only the stipend, so its callee must not reenter"
			);
			assert!(
				run(value, StipendSender::isSendDeniedCall {}.abi_encode()),
				"send forwards only the stipend, so its callee must not reenter"
			);
		}

		assert!(
			run(value, StipendSender::isSelfSendAllowedCall {}.abi_encode()),
			"self send should be allowed"
		);

		// The raised gas scale gives a zero-value `send` enough to write on PVM too.
		let (value_calls, zero_value_send) = with_gas_scale(2_000_000_000, || {
			let value_calls = [1, 2300].map(|gas_limit| {
				run(
					value,
					StipendSender::isCallWithGasDeniedCall { gasLimit: gas_limit }.abi_encode(),
				)
			});
			let zero_value_send = run(0, StipendSender::isSendDeniedCall {}.abi_encode());
			(value_calls, zero_value_send)
		});
		assert_eq!(value_calls, [false, false], "not a `send`, so it may reenter");
		assert!(zero_value_send, "a zero-value `send` should not reenter");
	});
}

#[test]
fn evm_call_stipend_does_not_weaken_strict_reentrancy() {
	// The fixture calls `call_evm` with empty flags, so the caller asks for `Strict`.
	let (code, _) = compile_module("call_with_gas").unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data((addr, 0u64).encode())
			.evm_value(1_000_000.into())
			.build()
			.result;

		assert_eq!(
			result.map(|_| ()),
			Err(Error::<Test>::ReentranceDenied.into()),
			"`ReentrancyProtection::Strict` should always deny self calls"
		);
	});
}

/// How `WarmWriteSender` reaches the receiver after warming its slot.
enum WarmWrite {
	/// The shape the stipend check guards.
	BySend,
	/// An explicit gas limit is not that shape, so the check does not apply.
	ByCallWithGas(u64),
}

/// Returns whether a `WarmWriteSender` call to a fresh receiver was denied, and its counter.
fn call_warm_write_sender(
	fixture_type: FixtureType,
	receiver_fixture: &str,
	value: u128,
	warm_write: WarmWrite,
) -> (bool, U256) {
	let (receiver_code, _) = compile_module_with_type(receiver_fixture, fixture_type).unwrap();
	let (sender_code, _) = compile_module_with_type("WarmWriteSender", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);
		let Contract { addr: receiver, .. } =
			builder::bare_instantiate(Code::Upload(receiver_code)).build_and_unwrap_contract();
		let Contract { addr: sender, .. } =
			builder::bare_instantiate(Code::Upload(sender_code)).build_and_unwrap_contract();
		let receiver_address = receiver.0.into();
		let call_data = match warm_write {
			WarmWrite::BySend => {
				WarmWriteSender::isWarmWriteDeniedCall { receiver: receiver_address }.abi_encode()
			},
			WarmWrite::ByCallWithGas(gas_limit) => WarmWriteSender::isWarmWriteDeniedWithGasCall {
				receiver: receiver_address,
				gasLimit: gas_limit,
			}
			.abi_encode(),
		};
		let result = builder::bare_call(sender)
			.data(call_data)
			.evm_value(value.into())
			.build_and_unwrap_result();
		let counter = builder::bare_call(receiver)
			.data(WritingReceiver::counterCall {}.abi_encode())
			.build_and_unwrap_result();
		(bool::abi_decode(&result.data).unwrap(), U256::abi_decode(&counter.data).unwrap())
	})
}

#[test_matrix(
	[FixtureType::Solc, FixtureType::Resolc],
	["WritingReceiver", "ClearingReceiver", "PrecompileClearingReceiver", "PrecompileTakingReceiver"]
)]
fn stipend_check_denies_hot_slot_writes_from_a_zero_value_send(
	fixture_type: FixtureType,
	receiver_fixture: &str,
) {
	let send_result = with_gas_scale(20_000_000, || {
		call_warm_write_sender(fixture_type, receiver_fixture, 0, WarmWrite::BySend)
	});
	assert_eq!(send_result, (true, U256::from(1)));
}

// These receivers' writes fit in the stipend, so only the check can refuse them.
#[test_case(FixtureType::Solc,   "WritingReceiver",  2; "solc, writing")]
#[test_case(FixtureType::Solc,   "ClearingReceiver", 0; "solc, clearing")]
#[test_case(FixtureType::Resolc, "ClearingReceiver", 0; "resolc, clearing")]
fn stipend_check_denies_hot_slot_writes_from_a_value_send(
	fixture_type: FixtureType,
	receiver_fixture: &str,
	counter_after_write: u64,
) {
	assert_eq!(
		call_warm_write_sender(
			fixture_type,
			receiver_fixture,
			1_000_000,
			WarmWrite::ByCallWithGas(1)
		),
		(false, U256::from(counter_after_write)),
		"the write should fit in the stipend"
	);

	assert_eq!(
		call_warm_write_sender(fixture_type, receiver_fixture, 1_000_000, WarmWrite::BySend),
		(true, U256::from(1)),
		"a write from a value `send` should be rejected by the EIP-2200 check"
	);
}

#[test_matrix(
	[FixtureType::Solc, FixtureType::Resolc],
	[Nesting::Call, Nesting::DelegateCall, Nesting::Create]
)]
fn stipend_check_is_inherited_by_nested_frames(fixture_type: FixtureType, nesting: Nesting) {
	let (target_code, _) = compile_module_with_type("WritingReceiver", fixture_type).unwrap();
	let (receiver_code, _) =
		compile_module_with_type("NestedWritingReceiver", fixture_type).unwrap();
	let (sender_code, _) = compile_module_with_type("StipendSender", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);
		if fixture_type == FixtureType::Resolc {
			let (child_code, _) =
				compile_module_with_type("CounterStartingAtOne", fixture_type).unwrap();
			Pallet::<Test>::upload_code(RuntimeOrigin::signed(ALICE), child_code, u128::MAX)
				.unwrap();
		}
		let Contract { addr: target, .. } =
			builder::bare_instantiate(Code::Upload(target_code)).build_and_unwrap_contract();
		let Contract { addr: receiver, .. } =
			builder::bare_instantiate(Code::Upload(receiver_code))
				.constructor_data(
					NestedWritingReceiver::constructorCall {
						_target: target.0.into(),
						_nesting: nesting,
					}
					.abi_encode(),
				)
				.build_and_unwrap_contract();
		let Contract { addr: sender, .. } = builder::bare_instantiate(Code::Upload(sender_code))
			.constructor_data(
				StipendSender::constructorCall { _probe: receiver.0.into() }.abi_encode(),
			)
			.build_and_unwrap_contract();
		let is_denied = |gas_limit: u64| {
			let result = builder::bare_call(sender)
				.data(StipendSender::isCallWithGasDeniedCall { gasLimit: gas_limit }.abi_encode())
				.build_and_unwrap_result();
			bool::abi_decode(&result.data).unwrap()
		};

		let (with_2300_gas, with_other_gas) =
			with_gas_scale(50_000_000, || (is_denied(2300), [is_denied(2299), is_denied(2301)]));
		assert!(with_2300_gas, "a nested frame should inherit the mark");
		assert_eq!(with_other_gas, [false, false], "only a `send` gets marked");
	});
}

#[test_case(FixtureType::Solc;   "solc")]
#[test_case(FixtureType::Resolc; "resolc")]
fn stipend_check_applies_under_strict_reentrancy(fixture_type: FixtureType) {
	// The fixture calls `call_evm` with empty flags, so the caller asks for `Strict`.
	let (caller_code, _) = compile_module("call_with_gas").unwrap();
	let (receiver_code, _) = compile_module_with_type("WritingReceiver", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);
		let Contract { addr: caller, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();
		let Contract { addr: receiver, .. } =
			builder::bare_instantiate(Code::Upload(receiver_code)).build_and_unwrap_contract();
		let call_with_gas = |gas: u64| {
			builder::bare_call(caller)
				.data((receiver, gas).encode())
				.build()
				.result
				.map(|_| ())
		};

		let (with_2300_gas, with_other_gas) = with_gas_scale(20_000_000, || {
			(call_with_gas(2300), [call_with_gas(2299), call_with_gas(2301)])
		});
		assert_eq!(
			with_2300_gas,
			Err(Error::<Test>::ContractTrapped.into()),
			"a `send` gets marked even under `Strict`"
		);
		assert_eq!(with_other_gas, [Ok(()), Ok(())], "only a `send` gets marked");
	});
}

#[test_matrix(
	[FixtureType::Solc, FixtureType::Resolc],
	[
		("WritingReceiver", 2),
		("ClearingReceiver", 0),
		("PrecompileClearingReceiver", 0),
		("PrecompileTakingReceiver", 0)
	],
	[0, 1_000_000]
)]
fn stipend_check_ignores_calls_with_an_explicit_gas_limit(
	fixture_type: FixtureType,
	(receiver_fixture, counter_after_write): (&str, u64),
	value: u128,
) {
	assert_eq!(
		call_warm_write_sender(
			fixture_type,
			receiver_fixture,
			value,
			WarmWrite::ByCallWithGas(10_000_000_000)
		),
		(false, U256::from(counter_after_write))
	);
}

#[test_matrix(
	[FixtureType::Solc, FixtureType::Resolc],
	[
		"TransientWritingReceiver",
		"TransientClearingReceiver",
		"TransientPrecompileClearingReceiver",
		"TransientPrecompileTakingReceiver"
	]
)]
fn stipend_check_allows_transient_storage_writes(
	fixture_type: FixtureType,
	receiver_fixture: &str,
) {
	let (receiver_code, _) = compile_module_with_type(receiver_fixture, fixture_type).unwrap();
	let (sender_code, _) = compile_module_with_type("StipendSender", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);
		let Contract { addr: receiver, .. } =
			builder::bare_instantiate(Code::Upload(receiver_code)).build_and_unwrap_contract();
		let Contract { addr: sender, .. } = builder::bare_instantiate(Code::Upload(sender_code))
			.constructor_data(
				StipendSender::constructorCall { _probe: receiver.0.into() }.abi_encode(),
			)
			.build_and_unwrap_contract();

		let result = with_gas_scale(20_000_000, || {
			builder::bare_call(sender)
				.data(StipendSender::isSendDeniedCall {}.abi_encode())
				.build_and_unwrap_result()
		});

		assert!(!bool::abi_decode(&result.data).unwrap(), "EIP-2200 exempts transient storage");
	});
}
