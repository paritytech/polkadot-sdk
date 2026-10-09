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
	AccountInfo, Code, Config, Error, ExecConfig, Pallet,
	evm::{CallTrace, CallTracer, fees::InfoT},
	test_utils::{self, ALICE, BOB, BOB_ADDR, builder::Contract},
	tests::{ExtBuilder, GasScale, RuntimeOrigin, Test, builder},
	tracing::trace,
};
use alloy_core::{
	primitives::U256,
	sol_types::{SolCall, SolConstructor, SolValue},
};
use codec::Encode;
use frame_support::{
	assert_ok,
	traits::fungible::{Balanced, Mutate},
};
use pallet_revive_fixtures::{
	BumpableCounter, CreatingReceiver, CreatingReceiver::Creation, FixtureType,
	NestedWritingReceiver, NestedWritingReceiver::Nesting, StipendSender, StipendTest,
	TerminatingReceiver, TerminatingReceiver::Termination, ValueForwardingReceiver,
	WarmWriteSender, compile_module, compile_module_with_type,
};
use sp_core::H160;
use test_case::{test_case, test_matrix};

/// Runs `test` under the given gas scale, then restores the previous scale.
fn with_gas_scale<R>(scale: u32, test: impl FnOnce() -> R) -> R {
	test_utils::with_parameter(GasScale::get, GasScale::set, scale, test)
}

/// Calls `isCallWithGasDenied(gas_limit)` on `sender` and returns the call trace.
fn trace_call_with_gas(sender: H160, gas_limit: u64) -> CallTrace {
	let mut tracer = CallTracer::new(Default::default());
	trace(&mut tracer, || {
		builder::bare_call(sender)
			.data(StipendSender::isCallWithGasDeniedCall { gasLimit: gas_limit }.abi_encode())
			.build_and_unwrap_result()
	});
	tracer.collect_trace().unwrap()
}

/// Returns the innermost frame of `call_trace`.
fn innermost_frame(mut call_trace: CallTrace) -> CallTrace {
	while let Some(nested) = call_trace.calls.pop() {
		call_trace = nested;
	}
	call_trace
}

/// Deploys a `StipendSender` of `fixture_type` that targets `receiver`, and returns its address.
fn deploy_stipend_sender(fixture_type: FixtureType, receiver: H160) -> H160 {
	let (code, _) = compile_module_with_type("StipendSender", fixture_type).unwrap();
	builder::bare_instantiate(Code::Upload(code))
		.constructor_data(
			StipendSender::constructorCall { _receiver: receiver.0.into() }.abi_encode(),
		)
		.build_and_unwrap_contract()
		.addr
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

#[test_case(FixtureType::Solc;   "solc")]
#[test_case(FixtureType::Resolc; "resolc")]
fn evm_call_stipend_is_added_to_static_and_delegate_calls(sender_type: FixtureType) {
	// On PVM the stipend alone cannot load the callee's code, so the receivers are always EVM
	// contracts.
	let (empty_receiver_code, _) =
		compile_module_with_type("DoNothingReceiver", FixtureType::Solc).unwrap();
	let (writing_receiver_code, _) =
		compile_module_with_type("WritingReceiver", FixtureType::Solc).unwrap();
	let (warm_sender_code, _) = compile_module_with_type("WarmWriteSender", sender_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);
		let Contract { addr: empty_receiver, .. } =
			builder::bare_instantiate(Code::Upload(empty_receiver_code))
				.build_and_unwrap_contract();
		let Contract { addr: writing_receiver, .. } =
			builder::bare_instantiate(Code::Upload(writing_receiver_code))
				.build_and_unwrap_contract();
		let stipend_sender = deploy_stipend_sender(sender_type, empty_receiver);
		let Contract { addr: warm_sender, .. } =
			builder::bare_instantiate(Code::Upload(warm_sender_code)).build_and_unwrap_contract();
		let writing_receiver = writing_receiver.0.into();

		let is_denied = |sender: H160, call_data: Vec<u8>, value: u128| {
			let result = builder::bare_call(sender)
				.data(call_data)
				.evm_value(value.into())
				.build_and_unwrap_result();
			bool::abi_decode(&result.data).unwrap()
		};
		let value_call_denied = |gas_limit: u64| {
			let call = StipendSender::isCallWithGasDeniedCall { gasLimit: gas_limit };
			is_denied(stipend_sender, call.abi_encode(), 1_000_000)
		};
		let static_read_denied = |gas_limit: u64| {
			let call = WarmWriteSender::isWarmStaticReadDeniedCall {
				receiver: writing_receiver,
				gasLimit: gas_limit,
			};
			is_denied(warm_sender, call.abi_encode(), 0)
		};
		let delegate_call_denied = |gas_limit: u64, data: Vec<u8>| {
			let call = WarmWriteSender::isWarmDelegateCallDeniedCall {
				receiver: writing_receiver,
				gasLimit: gas_limit,
				data: data.into(),
			};
			is_denied(warm_sender, call.abi_encode(), 0)
		};
		let delegate_read_denied = |gas_limit: u64| {
			delegate_call_denied(gas_limit, BumpableCounter::counterCall {}.abi_encode())
		};
		let delegate_write_denied = |gas_limit: u64| {
			delegate_call_denied(gas_limit, BumpableCounter::bumpCall {}.abi_encode())
		};

		assert!(!value_call_denied(1), "the stipend should be enough for an empty receive");
		assert!(
			!static_read_denied(2300),
			"a static call with a gas limit of 2300 gets the stipend"
		);
		assert_eq!(
			[static_read_denied(2299), static_read_denied(2301)],
			[true; 2],
			"a static call with any other gas limit gets no stipend"
		);

		if sender_type == FixtureType::Solc {
			assert!(
				!delegate_read_denied(2300),
				"an EVM delegate call with a gas limit of 2300 gets the stipend"
			);
			assert!(
				!delegate_write_denied(2300),
				"an EVM delegate call with a gas limit of 2300 is not stipend-limited"
			);
		} else {
			assert!(delegate_read_denied(2300), "a PVM delegate call gets no stipend");
		}
		assert_eq!(
			[delegate_read_denied(2299), delegate_read_denied(2301)],
			[true; 2],
			"a delegate call with any other gas limit gets no stipend"
		);
		assert_eq!(
			[static_read_denied(10_000_000_000), delegate_read_denied(10_000_000_000)],
			[false; 2],
			"delegate and static work with enough gas"
		);
	});
}

#[test]
fn receiver_of_a_transfer_cannot_drain_the_sender() {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(StipendTest::testTransferReentrantDrainCall {}.abi_encode())
			.evm_value(1_000_000_u128.into())
			.build();

		assert!(!result.result.unwrap().did_revert());
	});
}

#[test]
fn receiver_of_a_send_cannot_drain_the_sender() {
	let (code, _) = compile_module_with_type("StipendTest", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ =
			<Test as Config>::Currency::set_balance(&crate::test_utils::ALICE, 10_000_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(StipendTest::testSendReentrantDrainCall {}.abi_encode())
			.evm_value(1_000_000_u128.into())
			.exec_config(ExecConfig::new_substrate_tx().with_first_frame_reads_paid())
			.build();

		assert!(!result.result.unwrap().did_revert());
	});
}

#[test]
fn evm_call_stipend_allows_reentry_from_a_value_transfer_or_send() {
	// On PVM the stipend alone cannot pay for the reentrant call.
	let (probe_code, _) = compile_module_with_type("ReentrancyProbe", FixtureType::Solc).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);
		let Contract { addr: probe, .. } =
			builder::bare_instantiate(Code::Upload(probe_code)).build_and_unwrap_contract();
		let addr = deploy_stipend_sender(FixtureType::Solc, probe);

		let run = |call: Vec<u8>| {
			builder::bare_call(addr)
				.data(call)
				.evm_value(1_000_000.into())
				.exec_config(ExecConfig::new_substrate_tx().with_first_frame_reads_paid())
				.build_and_unwrap_result()
		};
		let denied = |call: Vec<u8>| bool::abi_decode(&run(call).data).unwrap();

		assert!(
			!denied(StipendSender::isCallWithGasDeniedCall { gasLimit: 1 }.abi_encode()),
			"the stipend should be enough for reentry"
		);
		assert!(!run(StipendSender::transferToReceiverCall {}.abi_encode()).did_revert());
		assert!(!denied(StipendSender::isSendDeniedCall {}.abi_encode()));
	});
}

#[test_case(FixtureType::Solc;   "solc")]
#[test_case(FixtureType::Resolc; "resolc")]
fn evm_call_stipend_allows_reentry_from_every_call_shape(fixture_type: FixtureType) {
	let (probe_code, _) = compile_module_with_type("ReentrancyProbe", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);
		let Contract { addr: probe, .. } =
			builder::bare_instantiate(Code::Upload(probe_code)).build_and_unwrap_contract();
		let addr = deploy_stipend_sender(fixture_type, probe);

		let value = 1_000_000_u128;
		let run = |evm_value: u128, call: Vec<u8>| {
			let result = builder::bare_call(addr)
				.data(call)
				.evm_value(evm_value.into())
				.build_and_unwrap_result();
			assert!(!result.did_revert(), "the call into StipendSender should not revert");
			bool::abi_decode(&result.data).unwrap()
		};

		assert!(
			run(value, StipendSender::isSelfSendAllowedCall {}.abi_encode()),
			"self send should be allowed"
		);

		// The raised gas scale gives a zero-value `send` enough to reenter on PVM too.
		let (value_call_denied, static_call_denied, zero_value_send_denied) =
			with_gas_scale(2_000_000_000, || {
				let value_call_denied = run(
					value,
					StipendSender::isCallWithGasDeniedCall { gasLimit: 2300 }.abi_encode(),
				);
				let static_call_denied = run(
					0,
					StipendSender::isStaticCallWithGasDeniedCall { gasLimit: 2300 }.abi_encode(),
				);
				let zero_value_send_denied =
					run(0, StipendSender::isSendDeniedCall {}.abi_encode());
				(value_call_denied, static_call_denied, zero_value_send_denied)
			});
		assert!(!value_call_denied, "a value call may reenter");
		assert!(!static_call_denied, "a static call may reenter");
		assert!(!zero_value_send_denied, "a zero-value `send` may reenter");
	});
}

#[test]
fn evm_call_stipend_keeps_strict_reentrancy() {
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
	BySend,
	ByCallWithGas(u64),
}

/// The execution mode of the transaction that calls `WarmWriteSender`.
#[derive(Clone, Copy)]
enum ExecutionMode {
	Substrate,
	Ethereum,
}

/// Returns whether a `WarmWriteSender` call to a fresh receiver was denied, and its counter.
fn call_warm_write_sender(
	fixture_type: FixtureType,
	receiver_fixture: &str,
	value: u128,
	warm_write: WarmWrite,
	execution_mode: ExecutionMode,
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
		let mut tracer = CallTracer::new(Default::default());
		trace(&mut tracer, || match execution_mode {
			ExecutionMode::Substrate => {
				builder::bare_call(sender)
					.data(call_data)
					.evm_value(value.into())
					.build_and_unwrap_result();
			},
			ExecutionMode::Ethereum => {
				<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(
					1_000_000_000_000,
				));
				assert_ok!(builder::eth_call(sender).data(call_data).value(value.into()).build());
			},
		});
		let denied = bool::abi_decode(&tracer.collect_trace().unwrap().output.0).unwrap();
		let sender_counter = builder::bare_call(sender)
			.data(WarmWriteSender::counterCall {}.abi_encode())
			.build_and_unwrap_result();
		assert_eq!(
			U256::abi_decode(&sender_counter.data).unwrap(),
			U256::from(1),
			"the sender should write after the call, since only the receiver is denied writes"
		);
		let receiver_counter = builder::bare_call(receiver)
			.data(BumpableCounter::counterCall {}.abi_encode())
			.build_and_unwrap_result();
		(denied, U256::abi_decode(&receiver_counter.data).unwrap())
	})
}

#[test_matrix(
	[FixtureType::Solc, FixtureType::Resolc],
	[
		("WritingReceiver", 2),
		("ClearingReceiver", 0),
		("PrecompileClearingReceiver", 0),
		("PrecompileTakingReceiver", 0)
	]
)]
fn stipend_check_denies_hot_slot_writes_from_a_zero_value_send(
	fixture_type: FixtureType,
	(receiver_fixture, counter_after_write): (&str, u64),
) {
	let call = |warm_write| {
		call_warm_write_sender(
			fixture_type,
			receiver_fixture,
			0,
			warm_write,
			ExecutionMode::Substrate,
		)
	};
	let (with_other_gas, send_result) = with_gas_scale(20_000_000, || {
		let with_other_gas =
			[2299, 2301].map(|gas_limit| call(WarmWrite::ByCallWithGas(gas_limit)));
		(with_other_gas, call(WarmWrite::BySend))
	});
	assert_eq!(
		with_other_gas,
		[(false, U256::from(counter_after_write)); 2],
		"only a `send` denies storage writes"
	);
	assert_eq!(send_result, (true, U256::from(1)));
}

// These receivers' writes fit in the stipend, so only the check can deny them.
#[test_matrix(
	[("WritingReceiver", 2), ("ClearingReceiver", 0)],
	[ExecutionMode::Substrate, ExecutionMode::Ethereum]
)]
fn stipend_check_denies_hot_slot_writes_from_a_value_send(
	(receiver_fixture, counter_after_write): (&str, u64),
	execution_mode: ExecutionMode,
) {
	let call = |warm_write| {
		call_warm_write_sender(
			FixtureType::Solc,
			receiver_fixture,
			1_000_000,
			warm_write,
			execution_mode,
		)
	};
	assert_eq!(
		call(WarmWrite::ByCallWithGas(1)),
		(false, U256::from(counter_after_write)),
		"the write should fit in the stipend"
	);
	assert_eq!(call(WarmWrite::BySend), (true, U256::from(1)));
}

#[test_matrix(
	[FixtureType::Solc, FixtureType::Resolc],
	[Nesting::Call, Nesting::DelegateCall]
)]
fn stipend_check_is_inherited_by_nested_frames(fixture_type: FixtureType, nesting: Nesting) {
	let (target_code, _) = compile_module_with_type("WritingReceiver", fixture_type).unwrap();
	let (receiver_code, _) =
		compile_module_with_type("NestedWritingReceiver", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);
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
		let sender = deploy_stipend_sender(fixture_type, receiver);

		let is_denied = |gas_limit: u64| {
			let result = builder::bare_call(sender)
				.data(StipendSender::isCallWithGasDeniedCall { gasLimit: gas_limit }.abi_encode())
				.build_and_unwrap_result();
			bool::abi_decode(&result.data).unwrap()
		};

		let (with_2300_gas, with_other_gas) =
			with_gas_scale(50_000_000, || (is_denied(2300), [is_denied(2299), is_denied(2301)]));
		assert!(with_2300_gas, "a nested frame should also deny storage writes");
		assert_eq!(with_other_gas, [false, false], "only a `send` denies storage writes");
	});
}

/// Where `ValueForwardingReceiver` sends value.
enum Beneficiary {
	AccountWithoutCode,
	Contract,
}

#[test_matrix(
	[FixtureType::Solc, FixtureType::Resolc],
	[Beneficiary::AccountWithoutCode, Beneficiary::Contract]
)]
fn stipend_check_denies_moving_value(fixture_type: FixtureType, beneficiary: Beneficiary) {
	const FORWARDED_VALUE: u64 = 1_000_000;
	let (receiver_code, _) =
		compile_module_with_type("ValueForwardingReceiver", fixture_type).unwrap();
	let (contract_code, _) =
		compile_module_with_type("DoNothingReceiver", FixtureType::Solc).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);
		let beneficiary = match beneficiary {
			Beneficiary::AccountWithoutCode => {
				let _ = <Test as Config>::Currency::set_balance(&BOB, 10_000_000_000_000);
				BOB_ADDR
			},
			Beneficiary::Contract => {
				builder::bare_instantiate(Code::Upload(contract_code))
					.build_and_unwrap_contract()
					.addr
			},
		};
		let Contract { addr: receiver, .. } =
			builder::bare_instantiate(Code::Upload(receiver_code))
				.native_value(1_000)
				.constructor_data(
					ValueForwardingReceiver::constructorCall {
						_beneficiary: beneficiary.0.into(),
						_amount: U256::from(FORWARDED_VALUE),
					}
					.abi_encode(),
				)
				.build_and_unwrap_contract();
		let sender = deploy_stipend_sender(fixture_type, receiver);

		// Returns the receiver's error and the value the beneficiary received.
		let forward_with_gas = |gas_limit: u64| {
			let balance_before = Pallet::<Test>::evm_balance(&beneficiary);
			let receiver_trace = trace_call_with_gas(sender, gas_limit).calls.pop().unwrap();
			let received = Pallet::<Test>::evm_balance(&beneficiary) - balance_before;
			(receiver_trace.error, received.as_u64())
		};

		let (with_2300_gas, with_other_gas) = with_gas_scale(50_000_000, || {
			(forward_with_gas(2300), [forward_with_gas(2299), forward_with_gas(2301)])
		});
		assert_eq!(
			with_2300_gas,
			(Some("OutOfGas".into()), 0),
			"the receiver of a `send` should not move value"
		);
		assert_eq!(
			with_other_gas,
			[(None, FORWARDED_VALUE), (None, FORWARDED_VALUE)],
			"only a `send` denies moving value"
		);
	});
}

#[test_matrix(
	[FixtureType::Solc, FixtureType::Resolc],
	[Creation::Create, Creation::Create2]
)]
fn stipend_check_denies_creating_contracts(fixture_type: FixtureType, creation: Creation) {
	let (receiver_code, _) = compile_module_with_type("CreatingReceiver", fixture_type).unwrap();
	let (child_code, _) = compile_module_with_type("DoNothingReceiver", fixture_type).unwrap();

	// Returns the receiver's error and whether the contract it creates exists.
	let create = |gas_limit: u64| {
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);
			if fixture_type == FixtureType::Resolc {
				Pallet::<Test>::upload_code(
					RuntimeOrigin::signed(ALICE),
					child_code.clone(),
					u128::MAX,
				)
				.unwrap();
			}
			let Contract { addr: receiver, .. } =
				builder::bare_instantiate(Code::Upload(receiver_code.clone()))
					.constructor_data(
						CreatingReceiver::constructorCall { _creation: creation }.abi_encode(),
					)
					.build_and_unwrap_contract();
			let sender = deploy_stipend_sender(fixture_type, receiver);

			let receiver_trace = trace_call_with_gas(sender, gas_limit).calls.pop().unwrap();
			let created = receiver_trace
				.calls
				.first()
				.is_some_and(|creation| AccountInfo::<Test>::is_contract(&creation.to));
			(receiver_trace.error, created)
		})
	};

	let (with_2300_gas, with_other_gas) =
		with_gas_scale(50_000_000, || (create(2300), [create(2299), create(2301)]));
	assert_eq!(
		with_2300_gas,
		(Some("OutOfGas".into()), false),
		"the receiver of a `send` should not create a contract"
	);
	assert_eq!(
		with_other_gas,
		[(None, true), (None, true)],
		"only a `send` denies creating a contract"
	);
}

#[test_matrix(
	[FixtureType::Solc, FixtureType::Resolc],
	[Termination::SelfDestruct, Termination::SystemPrecompile]
)]
fn stipend_check_denies_terminating_contracts(fixture_type: FixtureType, termination: Termination) {
	let (receiver_code, _) = compile_module_with_type("TerminatingReceiver", fixture_type).unwrap();

	// Returns the innermost frame's error, the value the beneficiary received, and the receiver's
	// balance before the call.
	let terminate = |gas_limit: u64| {
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);
			let _ = <Test as Config>::Currency::set_balance(&BOB, 10_000_000_000_000);
			let Contract { addr: receiver, .. } =
				builder::bare_instantiate(Code::Upload(receiver_code.clone()))
					.native_value(1_000)
					.constructor_data(
						TerminatingReceiver::constructorCall {
							_termination: termination,
							_beneficiary: BOB_ADDR.0.into(),
						}
						.abi_encode(),
					)
					.build_and_unwrap_contract();
			let sender = deploy_stipend_sender(fixture_type, receiver);
			let receiver_balance = Pallet::<Test>::evm_balance(&receiver);
			let balance_before = Pallet::<Test>::evm_balance(&BOB_ADDR);

			let call_trace = innermost_frame(trace_call_with_gas(sender, gas_limit));
			let received = Pallet::<Test>::evm_balance(&BOB_ADDR) - balance_before;
			(call_trace.error, received, receiver_balance)
		})
	};

	let (with_2300_gas, with_other_gas) =
		with_gas_scale(50_000_000, || (terminate(2300), [terminate(2299), terminate(2301)]));
	let (error, received, _) = with_2300_gas;
	assert_eq!(
		(error.as_deref(), received.is_zero()),
		(Some("OutOfGas"), true),
		"the receiver of a `send` should not terminate"
	);
	for (error, received, receiver_balance) in with_other_gas {
		assert_eq!(
			(error, received),
			(None, receiver_balance),
			"only a `send` denies terminating a contract"
		);
	}
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
			"a `send` denies storage writes even under `Strict`"
		);
		assert_eq!(with_other_gas, [Ok(()), Ok(())], "only a `send` denies storage writes");
	});
}

#[test_matrix(
	[FixtureType::Solc, FixtureType::Resolc],
	[
		"WritingReceiver",
		"PrecompileClearingReceiver",
		"PrecompileTakingReceiver",
		"ReentrantWritingReceiver"
	]
)]
fn stipend_check_denies_writes_with_out_of_gas(fixture_type: FixtureType, receiver_fixture: &str) {
	let (receiver_code, _) = compile_module_with_type(receiver_fixture, fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);

		let Contract { addr: receiver, .. } =
			builder::bare_instantiate(Code::Upload(receiver_code)).build_and_unwrap_contract();
		let sender = deploy_stipend_sender(fixture_type, receiver);

		let write_error = |gas_limit: u64| {
			let call_trace = with_gas_scale(20_000_000, || trace_call_with_gas(sender, gas_limit));
			innermost_frame(call_trace).error
		};

		assert_eq!(write_error(2300).as_deref(), Some("OutOfGas"));
		assert_eq!(write_error(2301), None);
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
			WarmWrite::ByCallWithGas(10_000_000_000),
			ExecutionMode::Substrate,
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
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 10_000_000_000_000);
		let Contract { addr: receiver, .. } =
			builder::bare_instantiate(Code::Upload(receiver_code)).build_and_unwrap_contract();
		let sender = deploy_stipend_sender(fixture_type, receiver);

		let result = with_gas_scale(20_000_000, || {
			builder::bare_call(sender)
				.data(StipendSender::isSendDeniedCall {}.abi_encode())
				.build_and_unwrap_result()
		});

		assert!(!bool::abi_decode(&result.data).unwrap(), "EIP-2200 exempts transient storage");
	});
}
