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
	BalanceOf, Code, Config, Error, EthBlockBuilderFirstValues, GenesisConfig, Origin, Pallet,
	PristineCode, assert_refcount,
	call_builder::VmBinaryModule,
	debug::DebugSettings,
	evm::{PrestateTracer, PrestateTracerConfig},
	test_utils::{ALICE, ALICE_ADDR, BOB, builder::Contract},
	tests::{
		AllowEvmBytecode, DebugFlag, ExtBuilder, RuntimeOrigin, Test, builder,
		test_utils::{contract_base_deposit, ensure_stored, get_contract},
	},
	tracing::trace,
	weightinfo_extension::OnFinalizeBlockParts,
};
use alloy_core::sol_types::{SolCall, SolInterface};
use frame_support::{
	assert_err, assert_noop, assert_ok, dispatch::GetDispatchInfo, traits::fungible::Mutate,
};
use pallet_revive_fixtures::{Fibonacci, FixtureType, NestedCounter, compile_module_with_type};
use pallet_revive_types::runtime_api::*;
use pretty_assertions::assert_eq;
use sp_runtime::Weight;
use test_case::test_case;

use revm::bytecode::opcode::*;

mod arithmetic;
mod bitwise;
mod block_info;
mod contract;
mod control;
mod host;
mod memory;
mod stack;
mod system;
mod terminate;
mod terminate_storage;
mod tx_info;

fn make_initcode_from_runtime_code(runtime_code: &Vec<u8>) -> Vec<u8> {
	let runtime_code_len = runtime_code.len();
	assert!(runtime_code_len < 256, "runtime code length must be less than 256 bytes");
	let mut init_code: Vec<u8> = vec![
		vec![PUSH1, 0x80_u8],
		vec![PUSH1, 0x40_u8],
		vec![MSTORE],
		vec![PUSH1, 0x40_u8],
		vec![MLOAD],
		vec![PUSH1, runtime_code_len as u8],
		vec![PUSH1, 0x13_u8],
		vec![DUP3],
		vec![CODECOPY],
		vec![PUSH1, runtime_code_len as u8],
		vec![SWAP1],
		vec![RETURN],
		vec![INVALID],
	]
	.into_iter()
	.flatten()
	.collect();
	init_code.extend(runtime_code);
	init_code
}

/// Init code for a contract that `EXTCODECOPY`s `copy_len` bytes from the address in calldata.
fn make_extcodecopy_reader(copy_len: u32) -> Vec<u8> {
	let [b0, b1, b2, b3] = copy_len.to_be_bytes();
	make_initcode_from_runtime_code(&vec![
		PUSH4,
		b0,
		b1,
		b2,
		b3,           // size: bytes to copy
		PUSH0,        // code offset
		PUSH0,        // destination memory offset
		PUSH0,        // calldata offset of the target address
		CALLDATALOAD, // load the target address
		EXTCODECOPY,
		STOP,
	])
}

#[test]
fn basic_evm_flow_works() {
	let (code, init_hash) = compile_module_with_type("Fibonacci", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		for i in 1u8..=2 {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } = builder::bare_instantiate(Code::Upload(code.clone()))
				.salt(Some([i; 32]))
				.build_and_unwrap_contract();

			// check the code exists
			let contract = get_contract(&addr);
			ensure_stored(contract.code_hash);
			let deposit = contract_base_deposit(&addr);
			assert_eq!(contract.total_deposit(), deposit);
			assert_refcount!(contract.code_hash, i as u64);

			let result = builder::bare_call(addr)
				.data(Fibonacci::FibonacciCalls::fib(Fibonacci::fibCall { n: 10u64 }).abi_encode())
				.build_and_unwrap_result();
			let decoded = Fibonacci::fibCall::abi_decode_returns(&result.data).unwrap();
			assert_eq!(55u64, decoded);
		}

		// init code is not stored
		assert!(!PristineCode::<Test>::contains_key(init_hash));
	});
}

#[test]
fn basic_evm_flow_tracing_works() {
	use crate::{
		evm::{CallTrace, CallTracer, CallType},
		tracing::trace,
	};
	let (code, _) = compile_module_with_type("Fibonacci", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let mut tracer = CallTracer::new(Default::default());
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr, .. } = trace(&mut tracer, || {
			builder::bare_instantiate(Code::Upload(code.clone()))
				.salt(None)
				.build_and_unwrap_contract()
		});

		let contract = get_contract(&addr);
		let runtime_code = PristineCode::<Test>::get(contract.code_hash).unwrap();

		let call_trace = tracer.collect_trace().unwrap();
		assert_eq!(
			call_trace,
			CallTrace {
				from: ALICE_ADDR,
				call_type: CallType::Create,
				to: addr,
				input: code.into(),
				output: runtime_code.into(),
				value: Some(crate::U256::zero()),
				gas: call_trace.gas,
				gas_used: call_trace.gas_used,
				..Default::default()
			}
		);

		let mut call_tracer = CallTracer::new(Default::default());
		let result = trace(&mut call_tracer, || {
			builder::bare_call(addr)
				.data(Fibonacci::FibonacciCalls::fib(Fibonacci::fibCall { n: 10u64 }).abi_encode())
				.build_and_unwrap_result()
		});

		let decoded = Fibonacci::fibCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(55u64, decoded);

		let call_trace = call_tracer.collect_trace().unwrap();
		assert_eq!(
			call_trace,
			CallTrace {
				call_type: CallType::Call,
				from: ALICE_ADDR,
				to: addr,
				input: Fibonacci::FibonacciCalls::fib(Fibonacci::fibCall { n: 10u64 })
					.abi_encode()
					.into(),
				output: result.data.into(),
				value: Some(crate::U256::zero()),
				gas: call_trace.gas,
				gas_used: call_trace.gas_used,
				..Default::default()
			},
		);
	});
}

/// Calling a delegated EOA should trace as CallType::Call (not DelegateCall).
#[test]
fn delegated_eoa_call_tracing_works() {
	use crate::{
		evm::{CallTrace, CallTracer, CallType},
		tests::{dummy_evm_contract, eip7702::DelegationTestSetup},
	};

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: target_addr, .. } =
			builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
				.build_and_unwrap_contract();

		let setup = DelegationTestSetup::default();
		setup.authorize(target_addr);

		let mut tracer = CallTracer::new(Default::default());
		let _ = trace(&mut tracer, || {
			builder::bare_call(setup.signer.address).build_and_unwrap_result()
		});

		let call_trace = tracer.collect_trace().unwrap();
		assert_eq!(
			call_trace,
			CallTrace {
				call_type: CallType::Call,
				from: ALICE_ADDR,
				to: setup.signer.address,
				value: Some(crate::U256::zero()),
				gas: call_trace.gas,
				gas_used: call_trace.gas_used,
				..Default::default()
			}
		);
	});
}

/// Prestate-tracer (prestate mode) must surface a delegated EOA's `code` as the
/// 23-byte EIP-7702 indicator `0xef0100 || target`. This is the channel most users
/// (foundry / hardhat traces, Tenderly, etc.) inspect to confirm a delegation
/// took effect.
#[test]
fn delegated_eoa_prestate_tracing_returns_indicator() {
	use crate::{
		evm::{PrestateTrace, PrestateTracer, PrestateTracerConfig},
		tests::{dummy_evm_contract, eip7702::DelegationTestSetup},
	};

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: target_addr, .. } =
			builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
				.build_and_unwrap_contract();

		let setup = DelegationTestSetup::default();
		setup.authorize(target_addr);

		let mut tracer = PrestateTracer::<Test>::new(PrestateTracerConfig {
			diff_mode: false,
			disable_storage: true,
			disable_code: false,
		});
		let _ = trace(&mut tracer, || {
			builder::bare_call(setup.signer.address).build_and_unwrap_result()
		});

		let mut indicator = vec![0xefu8, 0x01, 0x00];
		indicator.extend_from_slice(target_addr.as_bytes());

		match tracer.collect_trace() {
			PrestateTrace::Prestate(accounts) => {
				let info = accounts
					.get(&setup.signer.address)
					.expect("delegated EOA should be in prestate trace");
				let code = info.code.as_ref().expect("delegated EOA should report code");
				assert_eq!(
					code.0, indicator,
					"prestate trace code should be the 23-byte delegation indicator",
				);
			},
			other => panic!("expected Prestate mode, got {:?}", other),
		}
	});
}

/// Prestate-tracer must surface a precompile's `code` as its stub, matching what
/// eth_getCode and EXTCODESIZE/EXTCODEHASH/EXTCODECOPY report for the same address.
/// Primitive precompiles have an empty stub and report no code.
#[test]
fn precompile_prestate_tracing_returns_code_stub() {
	use crate::{H160, evm::PrestateTrace, precompiles::EVM_REVERT};
	use pallet_revive_uapi::{SYSTEM_PRECOMPILE_ADDR, precompiles::system::ISystem};

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let traced_code = |addr: H160, data: Vec<u8>| {
			let mut tracer = PrestateTracer::<Test>::new(PrestateTracerConfig {
				diff_mode: false,
				disable_storage: true,
				disable_code: false,
			});
			let result = trace(&mut tracer, || {
				builder::bare_call(addr).data(data).build_and_unwrap_result()
			});
			assert!(!result.did_revert(), "call to {addr:?} reverted");

			match tracer.collect_trace() {
				PrestateTrace::Prestate(accounts) => accounts
					.get(&addr)
					.expect("precompile should be in prestate trace")
					.code
					.clone()
					.map(|code| code.0),
				other => panic!("expected Prestate mode, got {:?}", other),
			}
		};

		assert_eq!(
			traced_code(H160(SYSTEM_PRECOMPILE_ADDR), ISystem::minimumBalanceCall {}.abi_encode()),
			Some(EVM_REVERT.to_vec()),
			"prestate trace code should be the precompile's code stub",
		);
		assert_eq!(
			traced_code(H160::from_low_u64_be(1), vec![]),
			None,
			"primitive precompile should report no code",
		);
	});
}

/// Prestate-tracer (diff mode) must surface the indicator in the pre-state when
/// the traced call mutates the delegated EOA (otherwise diff mode correctly
/// filters unchanged addresses out). Uses a Counter target + setNumber so the
/// authority's storage changes, forcing the address to appear in both halves.
#[test]
fn delegated_eoa_prestate_diff_tracing_includes_indicator() {
	use crate::{
		evm::{PrestateTrace, PrestateTracer, PrestateTracerConfig},
		precompiles::alloy::sol_types::SolCall,
		tests::eip7702::DelegationTestSetup,
	};
	use pallet_revive_fixtures::{Counter, FixtureType, compile_module_with_type};

	let (counter_code, _) = compile_module_with_type("Counter", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: target_addr, .. } =
			builder::bare_instantiate(Code::Upload(counter_code)).build_and_unwrap_contract();

		let setup = DelegationTestSetup::default();
		setup.authorize(target_addr);

		let mut tracer = PrestateTracer::<Test>::new(PrestateTracerConfig {
			diff_mode: true,
			disable_storage: false,
			disable_code: false,
		});
		let _ = trace(&mut tracer, || {
			builder::bare_call(setup.signer.address)
				.data(Counter::setNumberCall { newNumber: 42u64 }.abi_encode())
				.build_and_unwrap_result()
		});

		let mut indicator = vec![0xefu8, 0x01, 0x00];
		indicator.extend_from_slice(target_addr.as_bytes());

		match tracer.collect_trace() {
			PrestateTrace::DiffMode { pre, post: _ } => {
				let pre_info = pre
					.get(&setup.signer.address)
					.expect("delegated EOA whose storage changed should be in pre-state diff");
				let pre_code = pre_info.code.as_ref().expect("pre-state should include code");
				assert_eq!(
					pre_code.0, indicator,
					"diff pre-state code must be the delegation indicator",
				);
			},
			other => panic!("expected DiffMode, got {:?}", other),
		}
	});
}

/// Regression: EIP-7702 authority state changes must be visible to the prestate diff tracer.
///
/// `process_authorizations` mutates account state (code, nonce, deposit) without going through
/// any of the EVM hooks (`enter_child_span`, `read_account`, `balance_read`, ...). Without an
/// explicit notification, the tracer never sees the authority address, so a revoke of an existing
/// delegation is invisible to clients consuming the diff — the post block lacks the `code: null`
/// entry that Geth produces for the same transaction.
///
/// This test triggers an authority-only state change inside the trace scope (revoke, no further
/// EVM call) and asserts the authority appears in `post` with cleared code. Without the fix, the
/// authority is missing from the diff entirely.
#[test]
fn prestate_diff_includes_authority_when_eip7702_revokes_delegation() {
	use crate::{
		evm::{PrestateTrace, PrestateTracer, PrestateTracerConfig},
		tests::{dummy_evm_contract, eip7702::DelegationTestSetup},
	};
	use sp_core::H160;

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: target_addr, .. } =
			builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
				.build_and_unwrap_contract();

		let setup = DelegationTestSetup::default();
		setup.authorize(target_addr);
		assert!(crate::AccountInfo::<Test>::is_delegated(&setup.signer.address));

		let mut tracer = PrestateTracer::<Test>::new(PrestateTracerConfig {
			diff_mode: true,
			disable_storage: true,
			disable_code: false,
		});

		let _ = trace(&mut tracer, || {
			let revoke_auth = setup.sign_authorization(H160::zero());
			setup.process(&[revoke_auth]);
		});

		match tracer.collect_trace() {
			PrestateTrace::DiffMode { pre, post } => {
				let pre_info = pre.get(&setup.signer.address).expect(
					"authority must appear in pre-state diff with its pre-revoke delegation indicator",
				);
				let pre_code = pre_info.code.as_ref().expect("pre.code must capture the indicator");
				let mut indicator = vec![0xefu8, 0x01, 0x00];
				indicator.extend_from_slice(target_addr.as_bytes());
				assert_eq!(
					pre_code.0, indicator,
					"pre.code must be the pre-revoke delegation indicator, not the post-revoke (empty) state",
				);

				let post_info = post.get(&setup.signer.address).expect(
					"authority must appear in post-state diff after revoke (code cleared, nonce bumped)",
				);
				assert!(
					post_info.code.is_none(),
					"post.code must be None (delegation cleared), got {:?}",
					post_info.code,
				);
			},
			other => panic!("expected DiffMode, got {:?}", other),
		}
	});
}

/// Symmetric to [`prestate_diff_includes_authority_when_eip7702_revokes_delegation`] but for the
/// set direction: an EIP-7702 tx that creates a fresh delegation on an authority without prior
/// code must surface `pre.code == None` and `post.code == Some(0xef0100 || target)`. Without the
/// fix in `process_authorizations`, the authority is missing from the trace entirely.
#[test]
fn prestate_diff_includes_authority_when_eip7702_sets_delegation() {
	use crate::{
		evm::{PrestateTrace, PrestateTracer, PrestateTracerConfig},
		tests::{dummy_evm_contract, eip7702::DelegationTestSetup},
	};

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: target_addr, .. } =
			builder::bare_instantiate(Code::Upload(dummy_evm_contract()))
				.build_and_unwrap_contract();

		// Fresh authority — funded, but no existing delegation.
		let setup = DelegationTestSetup::default();
		assert!(!crate::AccountInfo::<Test>::is_delegated(&setup.signer.address));

		let mut tracer = PrestateTracer::<Test>::new(PrestateTracerConfig {
			diff_mode: true,
			disable_storage: true,
			disable_code: false,
		});

		let _ = trace(&mut tracer, || {
			let auth = setup.sign_authorization(target_addr);
			setup.process(&[auth]);
		});

		match tracer.collect_trace() {
			PrestateTrace::DiffMode { pre, post } => {
				let pre_info = pre
					.get(&setup.signer.address)
					.expect("authority must appear in pre-state diff before delegation is set");
				assert!(
					pre_info.code.is_none(),
					"pre.code must be None (no prior delegation), got {:?}",
					pre_info.code,
				);

				let post_info = post
					.get(&setup.signer.address)
					.expect("authority must appear in post-state diff after delegation is set");
				let post_code =
					post_info.code.as_ref().expect("post.code must carry the indicator");
				let mut indicator = vec![0xefu8, 0x01, 0x00];
				indicator.extend_from_slice(target_addr.as_bytes());
				assert_eq!(
					post_code.0, indicator,
					"post.code must be the 23-byte delegation indicator pointing at target",
				);
			},
			other => panic!("expected DiffMode, got {:?}", other),
		}
	});
}

/// EVM `sload` must charge proportionally to the actual byte size of the storage
/// value, not just the EVM word size of 32. The trie's storage values can exceed
/// 32 bytes when a PVM contract sharing the same namespace (via delegatecall) wrote
/// them — a fixed 32-byte charge would undercharge the proof space consumed by the
/// read. The fix charges `STORAGE_BYTES` upfront and refunds the unused portion
/// based on the actual length read (mirroring `get_storage` in PVM).
///
/// Setup: deploy Solc-compiled `Counter`, then directly inject values of different
/// sizes into its storage at slot 0 (simulating a PVM contract writing there via
/// shared namespace). Calling `number()` compiles down to `SLOAD(0)`; the 256-byte
/// read traps (length mismatch) but the gas consumed up to that point reflects the
/// actual read size.
///
/// Without the fix, both 32-byte and 256-byte cases would consume the same gas.
/// With the fix, the 256-byte case consumes strictly more.
#[test]
fn sload_charges_for_actual_storage_value_size() {
	use crate::exec::Key;
	use pallet_revive_fixtures::Counter;

	let (counter_code, _) = compile_module_with_type("Counter", FixtureType::Solc).unwrap();

	let measure_with_value_len = |len: usize| -> u128 {
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(counter_code.clone()))
					.build_and_unwrap_contract();

			// Inject a value of the requested length directly into the contract's
			// storage trie at slot 0 — simulates a PVM contract writing there via
			// shared namespace (delegatecall).
			let info = get_contract(&addr);
			info.write(&Key::Fix([0u8; 32]), Some(vec![0xAAu8; len]), None, false).unwrap();

			// Counter::number() compiles to SLOAD(0). For len != 32 it traps with
			// ContractTrapped after the read; we still observe the gas consumed.
			let result = builder::bare_call(addr).data(Counter::numberCall {}.abi_encode()).build();
			result.gas_consumed
		})
	};

	let gas_32: u128 = measure_with_value_len(32);
	let gas_256: u128 = measure_with_value_len(256);

	// With the fix, the 256-byte read costs strictly more than the 32-byte read.
	// Without the fix (legacy `GetStorage(32)`), the two would be equal.
	assert!(
		gas_256 > gas_32,
		"sload must charge more for larger storage values: gas_256={gas_256}, gas_32={gas_32}",
	);
}

/// Regression: EVM TLOAD must charge gas proportional to the actual length of the transient
/// value it reads. We inject a value at the EVM-visible `Key::Fix([0; 32])` slot via
/// `ExecConfig::test_env_transient_storage` — simulating a PVM contract writing a non-32-byte
/// value there through the shared namespace (delegatecall) — then call a contract whose runtime
/// is `TLOAD(0)`.
///
/// Comparing a `None` read (slot empty → zero) against a 32-byte read keeps both runs on the
/// same, non-trapping control-flow path, so the only difference in gas is TLOAD's `adjust_weight`
/// to the actual length read: ~974 gas with the fix, ~1 without. Mirrors
/// `sload_charges_for_actual_storage_value_size`.
#[test]
fn tload_charges_for_actual_transient_value_size() {
	use crate::{ExecConfig, exec::Key, limits, transient_storage::TransientStorage};
	use core::cell::RefCell;

	// EVM runtime that reads transient slot 0 and returns it as a 32-byte response:
	//   PUSH0 TLOAD PUSH0 MSTORE PUSH1 0x20 PUSH0 RETURN
	let tload_runtime: Vec<u8> = vec![PUSH0, TLOAD, PUSH0, MSTORE, PUSH1, 0x20, PUSH0, RETURN];
	let tload_code = make_initcode_from_runtime_code(&tload_runtime);

	let measure = |inject: Option<usize>| -> u128 {
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			let Contract { addr, account_id } =
				builder::bare_instantiate(Code::Upload(tload_code.clone()))
					.build_and_unwrap_contract();

			// Pre-populate the EVM-visible transient slot under the callee's namespace. The write
			// updates the backing store immediately, and the frame's `start_transaction`
			// checkpoints the journal *after* this entry, so it survives any in-call rollback.
			let mut transient = TransientStorage::<Test>::new(limits::TRANSIENT_STORAGE_BYTES);
			if let Some(len) = inject {
				transient
					.write(&account_id, &Key::Fix([0u8; 32]), Some(vec![0xAAu8; len]), false)
					.unwrap();
			}
			let mut exec_config = ExecConfig::new_substrate_tx();
			exec_config.test_env_transient_storage = Some(RefCell::new(transient));

			builder::bare_call(addr).exec_config(exec_config).build().gas_consumed
		})
	};

	let delta = measure(Some(32)).saturating_sub(measure(None));

	assert!(
		delta >= 100,
		"TLOAD must charge more for a 32-byte read than for a None read: delta={delta} \
		 (expected ~974 with fix, ~1 without)",
	);
}

#[test]
fn extcodecopy_charges_for_actual_code_size() {
	// Copy a single byte, so the charge is driven by the target's code size, not the copy length.
	let reader_code = make_extcodecopy_reader(1);

	let measure = |code_size: u32| -> u128 {
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			let target_code = VmBinaryModule::evm_init_code_for_runtime_size(code_size).code;
			let Contract { addr: target_addr, .. } =
				builder::bare_instantiate(Code::Upload(target_code)).build_and_unwrap_contract();

			let Contract { addr: reader_addr, .. } =
				builder::bare_instantiate(Code::Upload(reader_code.clone()))
					.build_and_unwrap_contract();

			let mut input = vec![0u8; 12];
			input.extend_from_slice(target_addr.as_bytes());

			let result = builder::bare_call(reader_addr).data(input).build();
			result.result.unwrap();
			result.gas_consumed
		})
	};

	let gas_small = measure(64);
	let gas_large = measure(20_000);

	assert!(
		gas_large > gas_small,
		"extcodecopy must charge more for a larger target contract: \
		 gas_large={gas_large}, gas_small={gas_small}",
	);
}

#[test]
fn extcodecopy_charges_for_copy_length_beyond_code_size() {
	const TARGET_CODE_SIZE: u32 = 64;

	let measure = |copy_len: u32| -> u128 {
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			let target_code = VmBinaryModule::evm_init_code_for_runtime_size(TARGET_CODE_SIZE).code;
			let Contract { addr: target_addr, .. } =
				builder::bare_instantiate(Code::Upload(target_code)).build_and_unwrap_contract();

			let Contract { addr: reader_addr, .. } =
				builder::bare_instantiate(Code::Upload(make_extcodecopy_reader(copy_len)))
					.build_and_unwrap_contract();

			let mut input = vec![0u8; 12];
			input.extend_from_slice(target_addr.as_bytes());

			let result = builder::bare_call(reader_addr).data(input).build();
			result.result.unwrap();
			result.gas_consumed
		})
	};

	let gas_at_code_size = measure(TARGET_CODE_SIZE);
	let gas_max = measure(crate::limits::EVM_MEMORY_BYTES);

	assert!(
		gas_max > gas_at_code_size,
		"extcodecopy must charge for the copy length when it exceeds the code size: \
		 gas_max={gas_max}, gas_at_code_size={gas_at_code_size}",
	);
}

/// Regression test for paritytech/contract-issues#278 — nested-call variant.
///
/// `Stack::call`'s no-code branch (the path taken when a running contract
/// makes an external call into an account with no code, e.g.
/// `payable(addr).transfer(...)` or `addr.call{value: ...}("")` to an EOA)
/// invokes `exit_child_span` with `Default::default()` for both `gas_used`
/// and `weight_consumed`. The frame meter does charge an existential
/// deposit when the destination is fresh, so the inner `CallTrace` should
/// report non-zero `gas_used`, but today it reports zero. The top-level
/// `Stack::run_call` no-code branch has the same shape and is fixed
/// separately; this test pins down the nested case.
#[test]
fn call_tracing_records_consumption_for_nested_transfer_to_eoa() {
	use crate::evm::{CallTracer, CallType};
	use pallet_revive_fixtures::Caller;
	use sp_core::H160;

	let (caller_code, _) = compile_module_with_type("Caller", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr: caller, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();

		// Pre-fund the caller contract so it has enough balance for the
		// inner value transfer. Pre-funding directly (rather than via
		// `evm_value` on `bare_call`) avoids dust/conversion complications.
		let _ = Pallet::<Test>::set_evm_balance(&caller, 100_000_000_000u128.into());

		// A fresh EOA with no code. The contract's sub-call into this address
		// hits the no-code branch in `Stack::call`, which charges an
		// existential deposit through the frame meter.
		let eoa = H160::from([0xfe; 20]);

		let mut tracer = CallTracer::new(Default::default());
		trace(&mut tracer, || {
			builder::bare_call(caller)
				.data(
					Caller::normalCall {
						_callee: eoa.0.into(),
						_value: 1_000_000,
						_data: Vec::<u8>::new().into(),
						_gas: u64::MAX,
					}
					.abi_encode(),
				)
				.build_and_unwrap_result();
		});

		// Sanity: the value actually arrived.
		assert!(Pallet::<Test>::evm_balance(&eoa) >= 1_000_000.into());

		let trace = tracer.collect_trace().unwrap();
		let inner =
			trace.calls.first().expect("CallTrace must contain the contract → EOA sub-call");
		assert_eq!(inner.to, eoa, "sub-call destination must be the EOA");
		assert_eq!(inner.call_type, CallType::Call, "sub-call must be a regular CALL");
		assert!(
			inner.gas_used > 0,
			"inner call to a fresh EOA must report non-zero gas_used; got {} — see issue #278",
			inner.gas_used,
		);
	});
}

#[test]
fn eth_contract_too_large() {
	// Create EVM init code that is one byte larger than the EIP-3860 limit.
	// We take valid init code and pad it with STOP opcodes after the RETURN instruction
	// (unreachable but makes the init code blob itself exceed MAX_INITCODE_SIZE).
	let mut code = VmBinaryModule::evm_init_code_for_runtime_size(0).code;
	code.resize(revm::primitives::eip3860::MAX_INITCODE_SIZE + 1, revm::bytecode::opcode::STOP);

	for (allow_unlimited_contract_size, debug_flag) in
		[(true, false), (true, true), (false, false), (false, true)]
	{
		// Set the DebugEnabled flag to the desired value for this iteration of the test.
		DebugFlag::set(debug_flag);

		// Initialize genesis config with allow_unlimited_contract_size
		let genesis_config = GenesisConfig::<Test> {
			debug_settings: Some(
				DebugSettings::default()
					.set_allow_unlimited_contract_size(allow_unlimited_contract_size),
			),
			..Default::default()
		};

		ExtBuilder::default()
			.genesis_config(Some(genesis_config))
			.build()
			.execute_with(|| {
				let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

				let result = builder::bare_instantiate(Code::Upload(code.clone())).build();

				if allow_unlimited_contract_size && debug_flag {
					// The contract is too large, but the DebugEnabled flag is set and
					// allow_unlimited_contract_size is true.
					assert_ok!(result.result);
				} else {
					// The contract is too large and either the DebugEnabled flag is not set or
					// allow_unlimited_contract_size is false.
					assert_err!(result.result, <Error<Test>>::BlobTooLarge);
				}
			});
	}
}

#[test]
fn upload_evm_runtime_code_works() {
	use crate::{
		Pallet, TransactionMeter,
		exec::Executable,
		primitives::ExecConfig,
		storage::{AccountInfo, ContractInfo},
	};

	let (runtime_code, _runtime_hash) =
		compile_module_with_type("Fibonacci", FixtureType::SolcRuntime).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let deployer = ALICE;
		let deployer_addr = ALICE_ADDR;
		let _ = Pallet::<Test>::set_evm_balance(&deployer_addr, 1_000_000_000.into());

		let uploaded_blob = Pallet::<Test>::try_upload_code(
			deployer,
			runtime_code.clone(),
			crate::vm::BytecodeType::Evm,
			&mut TransactionMeter::new_from_limits(Weight::MAX, BalanceOf::<Test>::MAX).unwrap(),
			&ExecConfig::new_substrate_tx(),
		)
		.unwrap();

		let contract_address = crate::address::create1(&deployer_addr, 0u32.into());

		let contract_info =
			ContractInfo::<Test>::new(&contract_address, 0u32.into(), *uploaded_blob.code_hash())
				.unwrap();
		AccountInfo::<Test>::insert_contract(&contract_address, contract_info);

		// Call the contract and verify it works
		let result = builder::bare_call(contract_address)
			.data(Fibonacci::FibonacciCalls::fib(Fibonacci::fibCall { n: 10u64 }).abi_encode())
			.build_and_unwrap_result();
		let decoded = Fibonacci::fibCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(55u64, decoded, "Contract should correctly compute fibonacci(10)");
	});
}

#[test]
fn extcodecopy_out_of_range_offset_zero_fills() {
	// Regression test for #12643: a `code_offset` of 2^64 overflows `usize` and used to halt.
	// Copy 32 bytes of our own code from that offset and return them; expect all zeroes.
	#[rustfmt::skip]
	let runtime_code = vec![
		PUSH1, 0x20,                                          // size = 32
		PUSH9, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // code offset = 2^64
		PUSH1, 0x00,                                          // dest offset = 0
		ADDRESS,                                              // copy from self
		EXTCODECOPY,
		PUSH1, 0x20,                                          // return size = 32
		PUSH1, 0x00,                                          // return offset = 0
		RETURN,
	];

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let init_code = make_initcode_from_runtime_code(&runtime_code);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(init_code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr).build_and_unwrap_result();
		assert!(!result.did_revert());
		assert_eq!(result.data, vec![0u8; 32]);
	});
}

#[test]
fn upload_and_remove_code_works_for_evm() {
	let (code, code_hash) = compile_module_with_type("Dummy", FixtureType::SolcRuntime).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = Pallet::<Test>::set_evm_balance(&ALICE_ADDR, 5_000_000_000u64.into());

		// Ensure the code is not already stored.
		assert!(!PristineCode::<Test>::contains_key(&code_hash));

		// Upload the code.
		assert_ok!(Pallet::<Test>::upload_code(RuntimeOrigin::signed(ALICE), code, 1000u128));

		// Ensure the contract was stored.
		ensure_stored(code_hash);

		// Remove the code.
		assert_ok!(Pallet::<Test>::remove_code(RuntimeOrigin::signed(ALICE), code_hash));

		// Ensure the code is no longer stored.
		assert!(!PristineCode::<Test>::contains_key(&code_hash));
	});
}

#[test]
fn upload_fails_if_evm_bytecode_disabled() {
	let (code, _) = compile_module_with_type("Dummy", FixtureType::SolcRuntime).unwrap();

	AllowEvmBytecode::set(false); // Disable support for EVM bytecode.
	ExtBuilder::default().build().execute_with(|| {
		// Upload should fail since support for EVM bytecode is disabled.
		assert_err!(
			Pallet::<Test>::upload_code(RuntimeOrigin::signed(ALICE), code, 1000u128),
			<Error<Test>>::CodeRejected
		);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn dust_work_with_child_calls(fixture_type: FixtureType) {
	use pallet_revive_fixtures::CallSelfWithDust;
	let (code, _) = compile_module_with_type("CallSelfWithDust", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code.clone())).build_and_unwrap_contract();

		let value = 1_000_000_000.into();
		builder::bare_call(addr)
			.data(
				CallSelfWithDust::CallSelfWithDustCalls::call(CallSelfWithDust::callCall {})
					.abi_encode(),
			)
			.evm_value(value)
			.build_and_unwrap_result();

		assert_eq!(crate::Pallet::<Test>::evm_balance(&addr), value);
	});
}

#[test]
fn prestate_diff_mode_tracing_works() {
	use alloy_core::hex;

	struct TestCase {
		config: PrestateTracerConfig,
		expected_instantiate_trace_json: &'static str,
		expected_call_trace_json: &'static str,
	}

	let (counter_code, _) = compile_module_with_type("NestedCounter", FixtureType::Solc).unwrap();
	let (contract_runtime_code, _) =
		compile_module_with_type("NestedCounter", FixtureType::SolcRuntime).unwrap();
	let (child_runtime_code, _) =
		compile_module_with_type("Counter", FixtureType::SolcRuntime).unwrap();

	let test_cases = [
		TestCase {
			config: PrestateTracerConfig {
				diff_mode: false,
				disable_storage: false,
				disable_code: false,
			},
			expected_instantiate_trace_json: r#"{
					"{{ALICE_ADDR}}": {
						"balance": "{{ALICE_BALANCE_PRE}}"
					}
				}"#,
			expected_call_trace_json: r#"{
					"{{ALICE_ADDR}}": {
						"balance": "{{ALICE_BALANCE_POST}}",
						"nonce": 1
					},
					"{{CONTRACT_ADDR}}": {
						"balance": "0x0",
						"nonce": 2,
						"code": "{{CONTRACT_CODE}}",
						"storage": {
							"0x0000000000000000000000000000000000000000000000000000000000000000": "{{SLOT0_PACKED_7}}"
						}
					},
					"{{CHILD_ADDR}}": {
						"balance": "0x0",
						"nonce": 1,
						"code": "{{CHILD_CODE}}",
						"storage": {
							"0x0000000000000000000000000000000000000000000000000000000000000000": "0x000000000000000000000000000000000000000000000000000000000000000a"
						}
					}
				}"#,
		},
		TestCase {
			config: PrestateTracerConfig {
				diff_mode: true,
				disable_storage: false,
				disable_code: false,
			},
			expected_instantiate_trace_json: r#"{
					"pre": {
						"{{ALICE_ADDR}}": {
							"balance": "{{ALICE_BALANCE_PRE}}"
						}
					},
					"post": {
						"{{ALICE_ADDR}}": {
							"balance": "{{ALICE_BALANCE_POST}}",
							"nonce": 1
						},
						"{{CONTRACT_ADDR}}": {
							"balance": "0x0",
							"nonce": 2,
							"code": "{{CONTRACT_CODE}}",
							"storage": {
								"0x0000000000000000000000000000000000000000000000000000000000000000": "{{SLOT0_PACKED_7}}"
							}
						},
						"{{CHILD_ADDR}}": {
							"balance": "0x0",
							"nonce": 1,
							"code": "{{CHILD_CODE}}",
							"storage": {
								"0x0000000000000000000000000000000000000000000000000000000000000000": "0x000000000000000000000000000000000000000000000000000000000000000a"
							}
						}
					}
				}"#,
			expected_call_trace_json: r#"{
					"pre": {
						"{{CONTRACT_ADDR}}": {
							"balance": "0x0",
							"nonce": 2,
							"code": "{{CONTRACT_CODE}}",
							"storage": {
								"0x0000000000000000000000000000000000000000000000000000000000000000": "{{SLOT0_PACKED_7}}"
							}
						},
						"{{CHILD_ADDR}}": {
							"balance": "0x0",
							"nonce": 1,
							"code": "{{CHILD_CODE}}",
							"storage": {
								"0x0000000000000000000000000000000000000000000000000000000000000000": "0x000000000000000000000000000000000000000000000000000000000000000a"
							}
						}
					},
					"post": {
						"{{CONTRACT_ADDR}}": {
							"storage": {
								"0x0000000000000000000000000000000000000000000000000000000000000000": "{{SLOT0_PACKED_8}}"
							}
						},
						"{{CHILD_ADDR}}": {
							"storage": {
								"0x0000000000000000000000000000000000000000000000000000000000000000": "0x0000000000000000000000000000000000000000000000000000000000000007"
							}
						}
					}
				}"#,
		},
	];

	for test_case in test_cases {
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000_000_000);

			let contract_addr = crate::address::create1(&ALICE_ADDR, 0u64);
			let child_addr = crate::address::create1(&contract_addr, 1u64);

			// Compute balances
			let alice_balance_pre = Pallet::<Test>::convert_native_to_evm(
				1_000_000_000_000 - Pallet::<Test>::min_balance(),
			);

			let replace_placeholders = |json: &str| -> String {
				let alice_balance_post = Pallet::<Test>::evm_balance(&ALICE_ADDR);

				// Packed slot 0: [4 zero bytes][uint64 number BE][20-byte address]
				let slot0_packed = |number: u64| -> String {
					let mut slot = [0u8; 32];
					slot[4..12].copy_from_slice(&number.to_be_bytes());
					slot[12..32].copy_from_slice(child_addr.as_bytes());
					format!("0x{}", hex::encode(slot))
				};

				json.replace("{{ALICE_ADDR}}", &format!("{:#x}", ALICE_ADDR))
					.replace("{{CONTRACT_ADDR}}", &format!("{:#x}", contract_addr))
					.replace("{{CHILD_ADDR}}", &format!("{:#x}", child_addr))
					.replace("{{ALICE_BALANCE_PRE}}", &format!("{:#x}", alice_balance_pre))
					.replace("{{ALICE_BALANCE_POST}}", &format!("{:#x}", alice_balance_post))
					.replace(
						"{{CONTRACT_CODE}}",
						&format!("0x{}", hex::encode(&contract_runtime_code)),
					)
					.replace("{{CHILD_CODE}}", &format!("0x{}", hex::encode(&child_runtime_code)))
					.replace("{{SLOT0_PACKED_7}}", &slot0_packed(7))
					.replace("{{SLOT0_PACKED_8}}", &slot0_packed(8))
			};

			let mut tracer = PrestateTracer::<Test>::new(test_case.config.clone());
			let Contract { addr: contract_addr_actual, .. } = trace(&mut tracer, || {
				builder::bare_instantiate(Code::Upload(counter_code.clone()))
					.salt(None)
					.build_and_unwrap_contract()
			});
			assert_eq!(contract_addr, contract_addr_actual, "contract address mismatch");

			let instantiate_trace = tracer.collect_trace();

			let expected_json = replace_placeholders(test_case.expected_instantiate_trace_json);
			let expected_trace: PrestateTraceV1 = serde_json::from_str(&expected_json).unwrap();
			assert_eq!(
				PrestateTraceV1::from(instantiate_trace),
				expected_trace,
				"unexpected instantiate trace for {:?}",
				test_case.config
			);

			let mut tracer = PrestateTracer::<Test>::new(test_case.config.clone());
			trace(&mut tracer, || {
				builder::bare_call(contract_addr)
					.data(
						NestedCounter::NestedCounterCalls::nestedNumber(
							NestedCounter::nestedNumberCall {},
						)
						.abi_encode(),
					)
					.build_and_unwrap_result();
			});

			let call_trace = tracer.collect_trace();
			let expected_json = replace_placeholders(test_case.expected_call_trace_json);
			let expected_trace: PrestateTraceV1 = serde_json::from_str(&expected_json).unwrap();
			assert_eq!(
				PrestateTraceV1::from(call_trace),
				expected_trace,
				"unexpected call trace for {:?}",
				test_case.config
			);
		});
	}
}

#[test]
fn eth_substrate_call_dispatches_successfully() {
	use frame_support::traits::fungible::Inspect;
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000);
		let _ = <Test as Config>::Currency::set_balance(&BOB, 100);

		let transfer_call =
			crate::tests::RuntimeCall::Balances(pallet_balances::Call::transfer_allow_death {
				dest: BOB,
				value: 50,
			});

		assert!(EthBlockBuilderFirstValues::<Test>::get().is_none());

		assert_ok!(Pallet::<Test>::eth_substrate_call(
			Origin::EthTransaction(ALICE).into(),
			Box::new(transfer_call),
			vec![]
		));

		// Verify balance changed
		assert_eq!(<Test as Config>::Currency::balance(&ALICE), 950);
		assert_eq!(<Test as Config>::Currency::balance(&BOB), 150);

		assert!(EthBlockBuilderFirstValues::<Test>::get().is_some());
	});
}

#[test]
fn eth_substrate_call_requires_eth_origin() {
	ExtBuilder::default().build().execute_with(|| {
		let inner_call = frame_system::Call::remark { remark: vec![] };

		// Should fail with non-EthTransaction origin
		assert_noop!(
			Pallet::<Test>::eth_substrate_call(
				RuntimeOrigin::signed(ALICE),
				Box::new(inner_call.into()),
				vec![]
			),
			sp_runtime::traits::BadOrigin
		);
	});
}

#[test]
fn eth_substrate_call_tracks_weight_correctly() {
	use crate::weights::WeightInfo;
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 1000);

		let inner_call = frame_system::Call::remark { remark: vec![0u8; 100] };
		let transaction_encoded = vec![0u8; 200];
		let transaction_encoded_len = transaction_encoded.len() as u32;

		let result = Pallet::<Test>::eth_substrate_call(
			Origin::EthTransaction(ALICE).into(),
			Box::new(inner_call.clone().into()),
			transaction_encoded,
		);

		assert_ok!(result);
		let post_info = result.unwrap();

		let overhead = <Test as Config>::WeightInfo::eth_substrate_call(transaction_encoded_len)
			.saturating_add(<Test as Config>::WeightInfo::on_finalize_block_per_tx(
				transaction_encoded_len,
			));
		let expected_weight = overhead.saturating_add(inner_call.get_dispatch_info().call_weight);
		assert!(
			expected_weight == post_info.actual_weight.unwrap(),
			"expected_weight ({}) should be == actual_weight ({})",
			expected_weight,
			post_info.actual_weight.unwrap(),
		);
	});
}

/// Tests execution tracing for both EVM and PVM.
///
/// Each test case runs for both Solc (EVM) and Resolc (PVM).
///
/// The EVM trace is compared against a snapshot in `src/tests/json_trace/`: every opcode is
/// traced, so the step sequence is determined by the bytecode.
///
/// The PVM trace only contains syscalls, and which syscalls a contract makes is up to the
/// compiler: resolc releases add and remove them. It is therefore not compared against a
/// snapshot. `verify_pvm_trace` checks what the tracer is responsible for instead: the call
/// structure, the fuel steps around every syscall, the return data and the result.
///
/// Gas consistency is verified for consecutive steps:
/// - For EVM: gas - gas_cost == next_step.gas (exact, every opcode is traced)
/// - For PVM: gas - gas_cost >= next_step.gas (inequality, PVM instructions between syscalls
///   consume additional gas that isn't individually traced)
#[test]
fn execution_tracing_works() {
	use crate::{
		evm::{Bytes, ExecutionTrace, ExecutionTracer, ExecutionTracerConfig},
		tracing::trace,
	};
	use pallet_revive_fixtures::{Callee, Caller};

	/// A PVM step that enters or leaves a call frame.
	#[derive(Debug, PartialEq, Eq)]
	enum Frame {
		Call,
		DelegateCall,
		Return,
	}

	struct TestCase {
		name: &'static str,
		setup: Box<dyn Fn(FixtureType) -> ExecutionTrace>,
		expected_evm_trace: &'static str,
		/// Every step of the PVM trace that enters or leaves a frame, with its depth.
		pvm_frames: &'static [(u16, Frame)],
		/// The `limit` the tracer was configured with, if any.
		step_limit: Option<usize>,
		/// The output of the traced call.
		return_value: Vec<u8>,
		/// The output of the sub call. Unused by cases which make none.
		callee_output: Vec<u8>,
	}

	let echo_output = Callee::echoCall::abi_encode_returns(&42u64);

	let test_cases: Vec<TestCase> = vec![
		TestCase {
			name: "Fibonacci",
			setup: Box::new(|fixture_type| {
				let (code, _) = compile_module_with_type("Fibonacci", fixture_type).unwrap();
				let Contract { addr, .. } =
					builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

				let config = ExecutionTracerConfig {
					enable_return_data: true,
					limit: Some(5),
					..Default::default()
				};
				let mut tracer = ExecutionTracer::new(config);
				trace(&mut tracer, || {
					builder::bare_call(addr)
						.data(Fibonacci::fibCall { n: 3 }.abi_encode())
						.build_and_unwrap_result()
				});
				tracer.collect_trace()
			}),
			expected_evm_trace: include_str!("json_trace/fibonacci_evm.json"),
			pvm_frames: &[],
			step_limit: Some(5),
			return_value: Fibonacci::fibCall::abi_encode_returns(&2u64),
			callee_output: Vec::new(),
		},
		TestCase {
			name: "CALL",
			setup: Box::new(|fixture_type| {
				let (callee_code, _) = compile_module_with_type("Callee", fixture_type).unwrap();
				let Contract { addr: callee, .. } =
					builder::bare_instantiate(Code::Upload(callee_code))
						.build_and_unwrap_contract();

				let (caller_code, _) = compile_module_with_type("Caller", fixture_type).unwrap();
				let Contract { addr: caller, .. } =
					builder::bare_instantiate(Code::Upload(caller_code))
						.build_and_unwrap_contract();

				let config =
					ExecutionTracerConfig { enable_return_data: true, ..Default::default() };
				let mut tracer = ExecutionTracer::new(config);
				trace(&mut tracer, || {
					builder::bare_call(caller)
						.data(
							Caller::normalCall {
								_callee: callee.0.into(),
								_value: 0,
								_data: Callee::echoCall { _data: 42u64 }.abi_encode().into(),
								_gas: u64::MAX,
							}
							.abi_encode(),
						)
						.build_and_unwrap_result()
				});
				tracer.collect_trace()
			}),
			expected_evm_trace: include_str!("json_trace/call_evm.json"),
			pvm_frames: &[(1, Frame::Call), (2, Frame::Return), (1, Frame::Return)],
			step_limit: None,
			return_value: Caller::normalCall::abi_encode_returns(&Caller::normalReturn {
				success: true,
				output: echo_output.clone().into(),
			}),
			callee_output: echo_output.clone(),
		},
		TestCase {
			name: "DELEGATECALL",
			setup: Box::new(|fixture_type| {
				let (callee_code, _) = compile_module_with_type("Callee", fixture_type).unwrap();
				let Contract { addr: callee, .. } =
					builder::bare_instantiate(Code::Upload(callee_code))
						.build_and_unwrap_contract();

				let (caller_code, _) = compile_module_with_type("Caller", fixture_type).unwrap();
				let Contract { addr: caller, .. } =
					builder::bare_instantiate(Code::Upload(caller_code))
						.build_and_unwrap_contract();

				let config =
					ExecutionTracerConfig { enable_return_data: true, ..Default::default() };
				let mut tracer = ExecutionTracer::new(config);
				trace(&mut tracer, || {
					builder::bare_call(caller)
						.data(
							Caller::delegateCall {
								_callee: callee.0.into(),
								_data: Callee::echoCall { _data: 42u64 }.abi_encode().into(),
								_gas: u64::MAX,
							}
							.abi_encode(),
						)
						.build_and_unwrap_result()
				});
				tracer.collect_trace()
			}),
			expected_evm_trace: include_str!("json_trace/delegatecall_evm.json"),
			pvm_frames: &[(1, Frame::DelegateCall), (2, Frame::Return), (1, Frame::Return)],
			step_limit: None,
			return_value: Caller::delegateCall::abi_encode_returns(&Caller::delegateReturn {
				success: true,
				output: echo_output.clone().into(),
			}),
			callee_output: echo_output,
		},
	];

	/// Normalizes an EVM trace by zeroing out all dynamic values for stable comparisons.
	fn normalize_trace(trace: &ExecutionTraceV1) -> ExecutionTraceV1 {
		use frame_support::weights::Weight;

		let mut normalized = trace.clone();
		normalized.gas = 0;
		normalized.weight_consumed = Weight::zero();
		normalized.base_call_weight = Weight::zero();

		for step in &mut normalized.struct_logs {
			step.gas = 0;
			step.gas_cost = 0;
			step.weight_cost = Weight::zero();

			let ExecutionStepKindV1::EVMOpcode { stack, .. } = &mut step.kind else {
				panic!("an EVM trace only contains opcodes: {step:?}");
			};
			for val in stack.iter_mut() {
				*val = Bytes::from(vec![0u8]);
			}
		}
		normalized
	}

	/// Verifies the parts of a PVM trace that do not depend on the compiler.
	///
	/// The tracer records a `PvmFuel` step for the instructions executed before every syscall
	/// and at the end of a frame. A frame's steps therefore alternate between fuel and syscall,
	/// starting and ending with fuel. The steps of a sub call sit between the calling syscall
	/// and the fuel step that follows it.
	fn verify_pvm_trace(trace: &ExecutionTraceV1, case: &TestCase) {
		let name = case.name;

		assert!(!trace.failed, "{name}: the call must succeed");
		assert_eq!(trace.return_value.0, case.return_value, "{name}: wrong return value");
		if let Some(limit) = case.step_limit {
			assert_eq!(trace.struct_logs.len(), limit, "{name}: the step limit was not applied");
		}

		let mut frames = Vec::new();
		// Whether the next step of the frame at `depth - 1` has to be a fuel step.
		let mut expects_fuel: Vec<bool> = Vec::new();
		// Whether the next step may be one level deeper: the first step and the one after a call.
		let mut enters_frame = true;
		let mut sub_call_returned = false;

		for (i, step) in trace.struct_logs.iter().enumerate() {
			let ExecutionStepKindV1::PVMSyscall { op, .. } = &step.kind else {
				panic!("{name}: step {i} is not a syscall: {step:?}");
			};
			assert_eq!(step.error, None, "{name}: step {i} failed");

			let depth = usize::from(step.depth);
			if depth > expects_fuel.len() {
				assert_eq!(depth, expects_fuel.len() + 1, "{name}: step {i} skips a depth");
				assert!(enters_frame, "{name}: step {i} enters a frame without a call");
				expects_fuel.push(true);
			} else if depth < expects_fuel.len() {
				assert_eq!(depth + 1, expects_fuel.len(), "{name}: step {i} skips a depth");
				assert_eq!(
					expects_fuel.pop(),
					Some(false),
					"{name}: the frame left at step {i} does not end with a fuel step",
				);
				sub_call_returned = true;
			}

			let is_fuel = matches!(op, PolkavmSyscallV1::PvmFuel);
			assert_eq!(
				is_fuel,
				expects_fuel[depth - 1],
				"{name}: step {i} breaks the fuel and syscall alternation: {op:?}",
			);
			expects_fuel[depth - 1] = !is_fuel;
			assert!(is_fuel || step.gas_cost > 0, "{name}: step {i} is free: {op:?}");

			let expected_return_data =
				if sub_call_returned { case.callee_output.as_slice() } else { &[] };
			assert_eq!(step.return_data.0, expected_return_data, "{name}: step {i} return data");

			// Which of the two syscalls a call compiles to depends on the resolc version.
			let frame = match op {
				PolkavmSyscallV1::Call | PolkavmSyscallV1::CallEvm => Some(Frame::Call),
				PolkavmSyscallV1::DelegateCall | PolkavmSyscallV1::DelegateCallEvm => {
					Some(Frame::DelegateCall)
				},
				PolkavmSyscallV1::SealReturn => Some(Frame::Return),
				_ => None,
			};
			enters_frame = matches!(frame, Some(Frame::Call | Frame::DelegateCall));
			frames.extend(frame.map(|frame| (step.depth, frame)));
		}

		assert_eq!(frames, case.pvm_frames, "{name}: wrong call structure");
		// A truncated trace ends wherever the limit was reached.
		if case.step_limit.is_none() {
			assert_eq!(expects_fuel, [false], "{name}: the trace does not end with a fuel step");
		}
	}

	/// Verifies gas consistency for execution traces.
	///
	/// Gas equations:
	/// - gasCost = opcode_cost only (excluding forwarded gas for call/create opcodes)
	/// - For consecutive steps at the same depth:
	///   - EVM: gas - gas_cost == next_step.gas (exact)
	///   - PVM: gas - gas_cost >= next_step.gas (inequality, PVM instructions consume extra gas)
	fn verify_gas_consistency(trace: &ExecutionTrace, is_evm: bool, name: &str) {
		// Verify consecutive steps at the same depth
		let same_depth_violations: Vec<_> = trace
			.struct_logs
			.iter()
			.zip(trace.struct_logs.iter().skip(1))
			.enumerate()
			.filter(|(_, (curr, next))| curr.depth == next.depth)
			.filter_map(|(i, (curr, next))| {
				let expected = curr.gas.saturating_sub(curr.gas_cost);
				let valid = if is_evm { expected == next.gas } else { next.gas <= expected };
				(!valid).then_some((i, curr.depth, curr.gas, curr.gas_cost, expected, next.gas))
			})
			.collect();

		assert!(
			same_depth_violations.is_empty(),
			"{name}: same-depth gas violations (step, depth, gas, gas_cost, expected, actual): {same_depth_violations:?}",
		);
	}

	for test_case in test_cases {
		for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
			ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
				let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

				let actual_trace = (test_case.setup)(fixture_type);
				let is_evm = matches!(fixture_type, FixtureType::Solc | FixtureType::SolcRuntime);
				let name = test_case.name;
				let vm_type = if is_evm { "EVM" } else { "PVM" };

				if is_evm {
					let expected: ExecutionTraceV1 =
						serde_json::from_str(test_case.expected_evm_trace).unwrap_or_else(|e| {
							panic!("{name} ({vm_type}): failed to parse expected JSON: {e}")
						});
					// Normalize both traces for comparison (zeroes out dynamic values)
					let normalized_actual = normalize_trace(&actual_trace.clone().into());
					let normalized_expected = normalize_trace(&expected);
					assert_eq!(
						normalized_actual, normalized_expected,
						"{name} ({vm_type}): trace mismatch"
					);
				} else {
					verify_pvm_trace(&actual_trace.clone().into(), &test_case);
				}

				verify_gas_consistency(&actual_trace, is_evm, &format!("{name} ({vm_type})"));
			});
		}
	}
}

/// Storage opcodes charge a worst-case amount upfront and refund the remainder via
/// `adjust_weight`, so `weight_consumed` is not monotonic within a step. A step's cost is
/// `(exit - entry) - child`, which is only exact if every refund lands inside the window that
/// charged it; one escaping would show up as a gap between consecutive steps.
#[test]
fn storage_refunds_stay_inside_the_step_that_charged_them() {
	use crate::evm::{ExecutionStepKind, ExecutionTracer, ExecutionTracerConfig};
	use pallet_revive_fixtures::Counter;

	let (code, _) = compile_module_with_type("Counter", FixtureType::Solc).unwrap();

	ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		// `increment` is SLOAD then SSTORE, so both refunding paths run.
		let mut tracer = ExecutionTracer::new(ExecutionTracerConfig::default());
		trace(&mut tracer, || {
			builder::bare_call(addr)
				.data(Counter::incrementCall {}.abi_encode())
				.build_and_unwrap_result()
		});
		let trace = tracer.collect_trace();

		assert!(
			trace.struct_logs.iter().any(|step| matches!(
				&step.kind,
				ExecutionStepKind::EVMOpcode { op, .. } if *op == SSTORE || *op == SLOAD
			)),
			"the fixture must actually touch storage for this to test anything",
		);

		// Consecutive EVM steps at the same depth must leave the meter continuous.
		let breaks = trace
			.struct_logs
			.iter()
			.zip(trace.struct_logs.iter().skip(1))
			.filter(|(step, next)| step.depth == next.depth)
			.filter(|(step, next)| step.gas.saturating_sub(step.gas_cost) != next.gas)
			.map(|(step, next)| (step.gas, step.gas_cost, next.gas))
			.collect::<Vec<_>>();
		assert!(breaks.is_empty(), "gas discontinuity (gas, gas_cost, next.gas): {breaks:?}");

		let summed: u64 = trace.struct_logs.iter().map(|step| step.gas_cost).sum();
		assert!(
			summed <= trace.gas,
			"steps account for {summed} gas but the transaction used {}",
			trace.gas,
		);
	});
}

/// Truncation drops steps; it must not change the ones it keeps, and what it keeps must never
/// sum past what the transaction spent.
///
/// Runs the real interpreter: the unit tests drive `Tracing` directly and so assert the
/// enter/exit pairing rather than exercise it. Truncation must land inside the nested call,
/// so several limits are swept.
#[test]
fn truncation_does_not_alter_the_steps_it_keeps() {
	use crate::evm::{ExecutionTracer, ExecutionTracerConfig};
	use pallet_revive_fixtures::{Callee, Caller};

	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let trace_with_limit = |limit: Option<u64>| {
			ExtBuilder::default().existential_deposit(200).build().execute_with(|| {
				let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

				let (callee_code, _) = compile_module_with_type("Callee", fixture_type).unwrap();
				let Contract { addr: callee, .. } =
					builder::bare_instantiate(Code::Upload(callee_code))
						.build_and_unwrap_contract();
				let (caller_code, _) = compile_module_with_type("Caller", fixture_type).unwrap();
				let Contract { addr: caller, .. } =
					builder::bare_instantiate(Code::Upload(caller_code))
						.build_and_unwrap_contract();

				let mut tracer =
					ExecutionTracer::new(ExecutionTracerConfig { limit, ..Default::default() });
				trace(&mut tracer, || {
					builder::bare_call(caller)
						.data(
							Caller::normalCall {
								_callee: callee.0.into(),
								_value: 0,
								_data: Callee::echoCall { _data: 42u64 }.abi_encode().into(),
								_gas: u64::MAX,
							}
							.abi_encode(),
						)
						.build_and_unwrap_result()
				});
				tracer.collect_trace()
			})
		};

		// Sweep limits relative to the untruncated length rather than hard-coding step counts.
		let full = trace_with_limit(None);
		let steps = full.struct_logs.len() as u64;
		assert!(steps > 8, "{fixture_type:?}: expected a multi-step trace, got {steps}");
		assert!(
			full.struct_logs.iter().any(|step| step.depth > 0),
			"{fixture_type:?}: the fixture must make a nested call for this to be a real test",
		);

		for limit in [steps / 4, steps / 2, steps * 3 / 4] {
			let trace = trace_with_limit(Some(limit));
			assert_eq!(trace.struct_logs.len() as u64, limit, "{fixture_type:?}: truncated");

			for (index, (truncated, full)) in
				trace.struct_logs.iter().zip(full.struct_logs.iter()).enumerate()
			{
				assert_eq!(
					(truncated.gas_cost, truncated.weight_cost),
					(full.gas_cost, full.weight_cost),
					"{fixture_type:?} limit {limit}: step {index} changed under truncation",
				);
			}

			let summed: u64 = trace.struct_logs.iter().map(|step| step.gas_cost).sum();
			assert!(
				summed <= trace.gas,
				"{fixture_type:?} limit {limit}: steps account for {summed} gas \
				 but the transaction used {}",
				trace.gas,
			);
		}
	}
}

/// Where a contract's log ends up, per entry point and per outcome, for a Solc and a Resolc build
/// of the same contract.
///
/// A log from a frame that took effect is on the ethereum transaction's receipt when there is one
/// and a substrate-only event otherwise. A log from a frame that reverted reaches no receipt: not
/// the transaction's, not the block's synthetic one for logs emitted outside any ethereum
/// transaction.
mod emitter_logs {
	use crate::{
		Code, Config, EthBlockBuilderFirstValues, EthBlockBuilderIR, Origin, Pallet,
		evm::{block_hash::LogsBloom, fees::InfoT},
		test_utils::{ALICE, WEIGHT_LIMIT, deposit_limit},
		tests::{
			Contracts, ExtBuilder, RuntimeCall, RuntimeEvent, System, Test, builder, pallet_dummy,
		},
	};
	use alloy_core::sol_types::{SolCall, SolEvent};
	use frame_support::{
		assert_ok,
		traits::{
			Hooks,
			fungible::{Balanced, Mutate},
		},
	};
	use pallet_revive_fixtures::{Emitter, FixtureType, compile_module_with_type};
	use pretty_assertions::assert_eq;
	use sp_core::{H160, H256};
	use test_case::test_case;

	fn emitted() -> H256 {
		H256(Emitter::Emitted::SIGNATURE_HASH.0)
	}

	fn doomed() -> H256 {
		H256(Emitter::Doomed::SIGNATURE_HASH.0)
	}

	/// Deploy the fixture into a block that is ready to take ethereum transactions.
	fn deploy(fixture_type: FixtureType) -> H160 {
		let (code, _) = compile_module_with_type("Emitter", fixture_type).unwrap();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(
			1_000_000_000_000,
		));
		Contracts::on_initialize(1);
		builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract().addr
	}

	/// The topics of every `ContractEmitted` event `contract` deposited, in order.
	fn contract_emitted_topics(contract: H160) -> Vec<Vec<H256>> {
		System::events()
			.into_iter()
			.filter_map(|record| match record.event {
				RuntimeEvent::Contracts(crate::Event::ContractEmitted {
					contract: emitter,
					topics,
					..
				}) if emitter == contract => Some(topics),
				_ => None,
			})
			.collect()
	}

	/// The error the ethereum transaction failed with, if it failed.
	fn eth_extrinsic_revert() -> Option<sp_runtime::DispatchError> {
		System::events().into_iter().find_map(|record| match record.event {
			RuntimeEvent::Contracts(crate::Event::EthExtrinsicRevert { dispatch_error }) => {
				Some(dispatch_error)
			},
			_ => None,
		})
	}

	fn bloom_of(contract: H160, topic: H256) -> [u8; 256] {
		let mut bloom = LogsBloom::new();
		bloom.accrue_log(&contract, &[topic]);
		bloom.bloom
	}

	fn occurrences(haystack: &[u8], word: &H256) -> usize {
		haystack.windows(32).filter(|window| *window == word.0).count()
	}

	/// What the block committed to its receipts.
	struct Committed {
		transactions: usize,
		bloom: [u8; 256],
		/// The first transaction's receipt as RLP, when there was one.
		first_receipt: Option<Vec<u8>>,
		synthetic_transaction: bool,
	}

	fn finalize_block() -> Committed {
		let first_receipt = EthBlockBuilderFirstValues::<Test>::get().map(|(_, r)| r);
		Contracts::on_finalize(1);
		let block = Pallet::<Test>::eth_block();
		Committed {
			transactions: block.transactions.len(),
			bloom: block.logs_bloom.0,
			first_receipt,
			synthetic_transaction: Pallet::<Test>::eth_synthetic_transaction().is_some(),
		}
	}

	fn assert_no_receipt_path(committed: &Committed) {
		assert_eq!(committed.transactions, 0, "no ethereum transaction was in the block");
		assert_eq!(committed.bloom, [0u8; 256], "so nothing is in the block's bloom");
		assert!(!committed.synthetic_transaction, "and nothing was buffered for a synthetic one");
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn native_call_logs_are_events_only(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			let addr = deploy(fixture_type);

			builder::bare_call(addr)
				.data(Emitter::emitValueCall { value: 7 }.abi_encode())
				.build_and_unwrap_result();

			assert_eq!(contract_emitted_topics(addr), vec![vec![emitted()]]);
			assert!(EthBlockBuilderIR::<Test>::get().gas_info.is_empty(), "no receipt is open");
			assert_no_receipt_path(&finalize_block());
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn reverted_native_call_leaves_no_log(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			let addr = deploy(fixture_type);

			let result = builder::bare_call(addr)
				.data(Emitter::emitThenRevertCall { value: 7 }.abi_encode())
				.build_and_unwrap_result();

			assert!(result.did_revert());
			assert!(contract_emitted_topics(addr).is_empty(), "the event rolled back");
			assert_no_receipt_path(&finalize_block());
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn native_sub_call_revert_drops_only_its_own_log(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			let addr = deploy(fixture_type);

			let result = builder::bare_call(addr)
				.data(Emitter::emitAndCallRevertingCall { kept: 1, dropped: 2 }.abi_encode())
				.build_and_unwrap_result();

			assert!(!result.did_revert());
			assert_eq!(contract_emitted_topics(addr), vec![vec![emitted()]]);
			assert_no_receipt_path(&finalize_block());
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn ethereum_call_logs_land_on_the_receipt(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			let addr = deploy(fixture_type);

			assert_ok!(
				builder::eth_call(addr)
					.data(Emitter::emitValueCall { value: 7 }.abi_encode())
					.build()
			);

			assert_eq!(eth_extrinsic_revert(), None);
			assert_eq!(contract_emitted_topics(addr), vec![vec![emitted()]]);
			let committed = finalize_block();
			assert_eq!(committed.transactions, 1);
			assert_eq!(committed.bloom, bloom_of(addr, emitted()));
			let receipt = committed.first_receipt.expect("one transaction, one receipt");
			assert_eq!(occurrences(&receipt, &emitted()), 1);
			assert!(!committed.synthetic_transaction, "a frame's log is never buffered");
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn reverted_ethereum_call_receipt_has_no_logs(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			let addr = deploy(fixture_type);

			assert_ok!(
				builder::eth_call(addr)
					.data(Emitter::emitThenRevertCall { value: 7 }.abi_encode())
					.build()
			);

			assert_eq!(eth_extrinsic_revert(), Some(crate::Error::<Test>::ContractReverted.into()));
			assert!(contract_emitted_topics(addr).is_empty(), "the event rolled back");
			let committed = finalize_block();
			assert_eq!(committed.transactions, 1, "the failed transaction is in the block");
			assert_eq!(committed.bloom, [0u8; 256], "with an empty bloom");
			let receipt = committed.first_receipt.expect("one transaction, one receipt");
			assert_eq!(occurrences(&receipt, &doomed()), 0, "and no log in its receipt");
			assert!(!committed.synthetic_transaction);
		});
	}

	/// The frames succeed and log, then the transaction fails settling its fee: the logs go with
	/// the state the frames committed.
	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn ethereum_call_failing_after_its_frames_leaves_no_log(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			let addr = deploy(fixture_type);
			let _ = <Test as Config>::FeeInfo::withdraw_txfee(
				<Test as Config>::FeeInfo::remaining_txfee(),
			);

			assert_ok!(
				builder::eth_call(addr)
					.data(Emitter::emitValueCall { value: 7 }.abi_encode())
					.build()
			);

			assert_eq!(eth_extrinsic_revert(), Some(crate::Error::<Test>::TxFeeOverdraw.into()));
			assert!(contract_emitted_topics(addr).is_empty(), "the event rolled back");
			let committed = finalize_block();
			assert_eq!(committed.transactions, 1);
			assert_eq!(committed.bloom, [0u8; 256]);
			let receipt = committed.first_receipt.expect("one transaction, one receipt");
			assert_eq!(occurrences(&receipt, &emitted()), 0);
			assert!(!committed.synthetic_transaction);
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn ethereum_sub_call_revert_drops_only_its_own_log(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			let addr = deploy(fixture_type);

			assert_ok!(
				builder::eth_call(addr)
					.data(Emitter::emitAndCallRevertingCall { kept: 1, dropped: 2 }.abi_encode())
					.build()
			);

			assert_eq!(eth_extrinsic_revert(), None);
			assert_eq!(contract_emitted_topics(addr), vec![vec![emitted()]]);
			let committed = finalize_block();
			assert_eq!(committed.transactions, 1);
			assert_eq!(
				committed.bloom,
				bloom_of(addr, emitted()),
				"the sub-call's log is not in it"
			);
			let receipt = committed.first_receipt.expect("one transaction, one receipt");
			assert_eq!(occurrences(&receipt, &emitted()), 1);
			assert_eq!(occurrences(&receipt, &doomed()), 0);
			assert!(!committed.synthetic_transaction);
		});
	}

	/// A log committed by a descendant frame goes with the ancestor that reverts above it, not
	/// with the frame that emitted it: the descendant's storage write, its log and the
	/// reverting ancestor's own log are all gone, the outer frame's log stays.
	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn ancestor_revert_discards_successful_descendant_logs(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			let addr = deploy(fixture_type);

			assert_ok!(
				builder::eth_call(addr)
					.data(
						Emitter::emitAndCallRevertingChainCall { kept: 1, dropped: 2 }.abi_encode()
					)
					.build()
			);

			assert_eq!(eth_extrinsic_revert(), None);
			assert_eq!(contract_emitted_topics(addr), vec![vec![emitted()]]);
			let touched = builder::bare_call(addr)
				.data(Emitter::touchedCall {}.abi_encode())
				.build_and_unwrap_result();
			assert_eq!(
				Emitter::touchedCall::abi_decode_returns(&touched.data).unwrap(),
				0,
				"the descendant's storage write rolled back"
			);
			let committed = finalize_block();
			assert_eq!(committed.transactions, 1);
			assert_eq!(
				committed.bloom,
				bloom_of(addr, emitted()),
				"neither the descendant's nor the ancestor's log is in it"
			);
			let receipt = committed.first_receipt.expect("one transaction, one receipt");
			assert_eq!(occurrences(&receipt, &emitted()), 1);
			assert_eq!(occurrences(&receipt, &doomed()), 0);
			assert!(!committed.synthetic_transaction);
		});
	}

	/// A rollback layer above the frames that still succeeds: `batch_all` rolls back `first`
	/// when the call after it fails, and the `batch` around it returns `Ok` regardless.
	fn batch_rolling_back(first: RuntimeCall) -> RuntimeCall {
		RuntimeCall::Utility(pallet_utility::Call::batch {
			calls: vec![RuntimeCall::Utility(pallet_utility::Call::batch_all {
				calls: vec![first, RuntimeCall::Dummy(pallet_dummy::Call::fail {})],
			})],
		})
	}

	/// The transaction succeeded, its receipt is the block's only one, and `topic` is in neither
	/// the receipt nor the bloom.
	fn assert_status_1_receipt_without(topic: H256) {
		assert_eq!(eth_extrinsic_revert(), None, "the transaction succeeded");
		let committed = finalize_block();
		assert_eq!(committed.transactions, 1);
		assert_eq!(committed.bloom, [0u8; 256], "the rolled-back log is not in the bloom");
		let receipt = committed.first_receipt.expect("one transaction, one receipt");
		assert_eq!(occurrences(&receipt, &topic), 0, "nor in the receipt");
		assert!(!committed.synthetic_transaction);
	}

	/// A mirrored balance change has no frame: its log goes straight onto the open receipt. When
	/// the `batch_all` it ran in rolls back, the balance change and its event are gone, and the
	/// receipt must not keep a `Transfer` that never happened.
	#[test]
	fn rolled_back_batch_all_keeps_a_mirrored_log_off_the_receipt() {
		ExtBuilder::default().build().execute_with(|| {
			deploy(FixtureType::Solc);
			let mirror = H160::repeat_byte(0xa5);
			let topic = H256::repeat_byte(0x11);

			assert_ok!(Pallet::<Test>::eth_substrate_call(
				Origin::EthTransaction(ALICE).into(),
				Box::new(batch_rolling_back(RuntimeCall::Dummy(pallet_dummy::Call::emit_log {
					contract: mirror,
					topic,
				}))),
				vec![],
			));

			assert!(contract_emitted_topics(mirror).is_empty(), "the event rolled back");
			assert_status_1_receipt_without(topic);
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn rolled_back_batch_all_keeps_a_frame_log_off_the_receipt(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			let addr = deploy(fixture_type);
			let call = RuntimeCall::Contracts(crate::Call::call {
				dest: addr,
				value: 0,
				weight_limit: WEIGHT_LIMIT,
				storage_deposit_limit: deposit_limit::<Test>(),
				data: Emitter::emitValueCall { value: 7 }.abi_encode(),
			});

			assert_ok!(Pallet::<Test>::eth_substrate_call(
				Origin::EthTransaction(ALICE).into(),
				Box::new(batch_rolling_back(call)),
				vec![],
			));

			assert!(contract_emitted_topics(addr).is_empty(), "the event rolled back");
			assert_status_1_receipt_without(emitted());
		});
	}
}

/// The logs of an ethereum transaction take up at most `limits::MAX_RECEIPT_LOGS_BYTES` of its
/// receipt, for a Solc and a Resolc build of the same contract and for logs mirrored from outside
/// any frame.
mod receipt_size_limit {
	use crate::{
		Code, Config, EthBlockBuilderFirstValues, EthBlockBuilderIR, EthTransactError, Origin,
		Pallet, RUNTIME_PALLETS_ADDR, RuntimeCosts, WeightToken,
		evm::{
			Block, CallTracer, CallTracerConfig, GenericTransaction, TransactionSigned,
			block_hash::AccumulateReceipt, fees::InfoT,
		},
		limits::MAX_RECEIPT_LOGS_BYTES,
		precompiles::Precompile,
		test_utils::{ALICE, ALICE_ADDR},
		tests::{
			ASSET_TRANSFER_LOG_ADDR, Assets, Contracts, ExtBuilder, MirrorAssetTransfers,
			PGAS_ASSET_ID, RuntimeCall, RuntimeEvent, RuntimeOrigin, System, Test, builder,
			initialize_block, pallet_dummy,
			precompiles::{INoInfo, NoInfo},
		},
		tracing::trace,
	};
	use alloy_core::{
		primitives::Address,
		rlp::Decodable,
		sol_types::{SolCall, SolConstructor},
	};
	use codec::Encode;
	use frame_support::{
		assert_ok,
		dispatch::{DispatchResultWithPostInfo, PostDispatchInfo},
		traits::{
			Hooks,
			fungible::{Balanced, Mutate},
		},
		weights::Weight,
	};
	use pallet_revive_fixtures::{
		FixtureType, LogVolume, LogVolumeConstructor, compile_module_with_type,
	};
	use pretty_assertions::assert_eq;
	use sp_core::{H160, H256};
	use sp_runtime::{DispatchError, traits::Dispatchable};
	use test_case::test_case;

	/// The data of a log without topics that takes up 65,536 bytes of the receipt: 32 of them
	/// fill it exactly.
	const LOG_DATA: u32 = 65_508;

	/// Logs of `LOG_DATA` bytes the receipt holds besides the last one a call emits.
	const FULL_LOGS: u32 = 31;

	/// Enough weight for a receipt full of logs.
	const WEIGHT_LIMIT: Weight = Weight::from_parts(1_500_000_000_000, 10 * 1024 * 1024);

	/// Enough gas for a receipt full of logs.
	const ETH_GAS_LIMIT: u128 = 1_000_000_000_000_000;

	/// Deploy the fixture into a block that is ready to take ethereum transactions.
	fn deploy(fixture_type: FixtureType) -> H160 {
		let (code, _) = compile_module_with_type("LogVolume", fixture_type).unwrap();
		let _ = <Test as Config>::Currency::set_balance(&ALICE, ETH_GAS_LIMIT * 10);
		<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(
			ETH_GAS_LIMIT * 10,
		));
		Contracts::on_initialize(1);
		builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract().addr
	}

	fn emit_logs(count: u32, last_size: u32) -> Vec<u8> {
		LogVolume::emitLogsCall { count, size: LOG_DATA, lastSize: last_size }.abi_encode()
	}

	fn emit_logs_guarded(count: u32, last_size: u32) -> Vec<u8> {
		LogVolume::emitLogsGuardedCall { count, size: LOG_DATA, lastSize: last_size }.abi_encode()
	}

	/// Run `data` against `addr` as an ethereum transaction with gas for a receipt full of logs.
	fn eth_call(addr: H160, data: Vec<u8>) -> DispatchResultWithPostInfo {
		builder::eth_call(addr)
			.weight_limit(WEIGHT_LIMIT)
			.eth_gas_limit(ETH_GAS_LIMIT.into())
			.data(data)
			.build()
	}

	/// The bytes the `Outer` log of `emitLogsGuarded` takes up in the receipt.
	fn outer_log_len(addr: H160) -> u32 {
		AccumulateReceipt::encoded_log_len(&addr, &[0u8; 32], &[H256::zero()]) as u32
	}

	/// The data of every `ContractEmitted` event `contract` deposited, in order.
	fn contract_emitted_data(contract: H160) -> Vec<Vec<u8>> {
		System::events()
			.into_iter()
			.filter_map(|record| match record.event {
				RuntimeEvent::Contracts(crate::Event::ContractEmitted {
					contract: emitter,
					data,
					..
				}) if emitter == contract => Some(data),
				_ => None,
			})
			.collect()
	}

	/// The error the ethereum transaction failed with, if it failed.
	fn eth_extrinsic_revert() -> Option<DispatchError> {
		System::events().into_iter().find_map(|record| match record.event {
			RuntimeEvent::Contracts(crate::Event::EthExtrinsicRevert { dispatch_error }) => {
				Some(dispatch_error)
			},
			_ => None,
		})
	}

	/// Finalize the block holding a single ethereum transaction and return that transaction's
	/// receipt, after checking that the block's `receipts_root` commits to it.
	fn finalize_single_receipt() -> alloy_consensus::ReceiptWithBloom {
		let (_, receipt) =
			EthBlockBuilderFirstValues::<Test>::get().expect("one transaction, one receipt");
		Contracts::on_finalize(1);
		let block = Pallet::<Test>::eth_block();
		assert_eq!(block.transactions.len(), 1);
		assert_eq!(block.receipts_root.0, Block::compute_trie_root(&[receipt.clone()]).0);
		// A typed transaction's receipt starts with the type byte (EIP-2718).
		let mut body = if receipt[0] < 0xc0 { &receipt[1..] } else { &receipt[..] };
		alloy_consensus::ReceiptWithBloom::decode(&mut body).unwrap()
	}

	fn dry_run(addr: H160, input: Vec<u8>) -> Result<Vec<u8>, EthTransactError> {
		Pallet::<Test>::dry_run_eth_transact(
			GenericTransaction {
				from: Some(ALICE_ADDR),
				to: Some(addr),
				input: input.into(),
				..Default::default()
			},
			None,
			false,
			None,
		)
		.map(|info| info.data)
	}

	#[test]
	fn log_data_fills_the_receipt_exactly() {
		let log_len =
			AccumulateReceipt::encoded_log_len(&H160::zero(), &[0u8; LOG_DATA as usize], &[]);
		assert_eq!(log_len * (FULL_LOGS as usize + 1), MAX_RECEIPT_LOGS_BYTES as usize);
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn receipt_at_the_limit_is_kept(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			let addr = deploy(fixture_type);

			assert_ok!(eth_call(addr, emit_logs(FULL_LOGS, LOG_DATA)));

			assert_eq!(eth_extrinsic_revert(), None);
			assert_eq!(contract_emitted_data(addr).len(), FULL_LOGS as usize + 1);
			let receipt = finalize_single_receipt();
			assert!(receipt.receipt.status.coerce_status());
			assert_eq!(receipt.receipt.logs.len(), FULL_LOGS as usize + 1);
			for log in &receipt.receipt.logs {
				assert_eq!(log.address.0.0, addr.0);
				assert!(log.data.topics().is_empty());
				assert_eq!(log.data.data.len(), LOG_DATA as usize);
			}
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn log_past_the_limit_fails_the_frame(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			let addr = deploy(fixture_type);

			let post_info = eth_call(addr, emit_logs(FULL_LOGS, LOG_DATA + 1))
				.expect("the failed transaction is still included");

			assert_eq!(eth_extrinsic_revert(), Some(crate::Error::<Test>::ReceiptTooLarge.into()));
			assert!(contract_emitted_data(addr).is_empty(), "the events rolled back");
			let full_logs_weight =
				<RuntimeCosts as WeightToken<Test>>::weight(&RuntimeCosts::DepositEvent {
					num_topic: 0,
					len: LOG_DATA,
				})
				.saturating_mul(FULL_LOGS.into());
			let actual_weight = post_info.actual_weight.expect("an eth_call reports its weight");
			assert!(
				actual_weight.all_gte(full_logs_weight),
				"the logs emitted before the failing one are paid for: {actual_weight} < \
				 {full_logs_weight}",
			);
			let receipt = finalize_single_receipt();
			assert!(!receipt.receipt.status.coerce_status());
			assert!(receipt.receipt.logs.is_empty());
		});
	}

	/// A sub-call whose logs fill the receipt next to its caller's log succeeds.
	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn sub_call_filling_the_receipt_succeeds(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			let addr = deploy(fixture_type);
			let last_size = LOG_DATA - outer_log_len(addr);

			assert_ok!(eth_call(addr, emit_logs_guarded(FULL_LOGS, last_size)));

			assert_eq!(eth_extrinsic_revert(), None);
			assert_eq!(contract_emitted_data(addr).len(), FULL_LOGS as usize + 2);
			let receipt = finalize_single_receipt();
			assert!(receipt.receipt.status.coerce_status());
			assert_eq!(receipt.receipt.logs.len(), FULL_LOGS as usize + 2);
		});
	}

	/// The frame that goes past the limit fails like on any other error, and its caller can
	/// handle that: the caller's own log stays.
	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn caller_survives_a_sub_call_past_the_limit(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			let addr = deploy(fixture_type);
			let last_size = LOG_DATA - outer_log_len(addr) + 1;

			assert_ok!(eth_call(addr, emit_logs_guarded(FULL_LOGS, last_size)));

			assert_eq!(eth_extrinsic_revert(), None);
			assert_eq!(contract_emitted_data(addr).len(), 1, "only the caller's log");
			let receipt = finalize_single_receipt();
			assert!(receipt.receipt.status.coerce_status());
			assert_eq!(receipt.receipt.logs.len(), 1);
			assert_eq!(receipt.receipt.logs[0].data.data.len(), 32);
		});
	}

	/// A dry run applies the limit as inclusion does, so an estimate reports the same outcome.
	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn dry_run_applies_the_limit(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			let addr = deploy(fixture_type);

			assert_ok!(dry_run(addr, emit_logs(FULL_LOGS, LOG_DATA)));
			let err = dry_run(addr, emit_logs(FULL_LOGS, LOG_DATA + 1)).unwrap_err();
			assert!(
				matches!(&err, EthTransactError::Message(msg) if msg.contains("ReceiptTooLarge")),
				"{err:?}",
			);
			let last_size = LOG_DATA - outer_log_len(addr);
			let ok = dry_run(addr, emit_logs_guarded(FULL_LOGS, last_size)).unwrap();
			assert!(LogVolume::emitLogsGuardedCall::abi_decode_returns(&ok).unwrap());
			let ok = dry_run(addr, emit_logs_guarded(FULL_LOGS, last_size + 1)).unwrap();
			assert!(!LogVolume::emitLogsGuardedCall::abi_decode_returns(&ok).unwrap());
		});
	}

	/// The address the logs of `mirror_logs` are attributed to.
	const MIRROR: H160 = H160([0xa5; 20]);

	/// A runtime call that mirrors one log from `MIRROR` per entry of `data_lens`, with that many
	/// bytes of data.
	fn mirror_log_sizes(data_lens: Vec<u32>) -> Box<RuntimeCall> {
		Box::new(RuntimeCall::Dummy(pallet_dummy::Call::emit_logs { contract: MIRROR, data_lens }))
	}

	/// A runtime call that mirrors `count` logs of `LOG_DATA` bytes from `MIRROR`.
	fn mirror_logs(count: u32) -> Box<RuntimeCall> {
		mirror_log_sizes(vec![LOG_DATA; count as usize])
	}

	#[test]
	fn mirrored_logs_at_the_limit_are_kept() {
		ExtBuilder::default().build().execute_with(|| {
			deploy(FixtureType::Solc);
			let mirror = MIRROR;

			assert_ok!(Pallet::<Test>::eth_substrate_call(
				Origin::EthTransaction(ALICE).into(),
				mirror_logs(FULL_LOGS + 1),
				vec![],
			));

			assert_eq!(eth_extrinsic_revert(), None);
			assert_eq!(contract_emitted_data(mirror).len(), FULL_LOGS as usize + 1);
			let receipt = finalize_single_receipt();
			assert_eq!(receipt.receipt.logs.len(), FULL_LOGS as usize + 1);
		});
	}

	/// A mirrored log outside any frame that does not fit fails the transaction.
	#[test]
	fn mirrored_log_past_the_limit_fails_the_transaction() {
		ExtBuilder::default().build().execute_with(|| {
			deploy(FixtureType::Solc);
			let mirror = MIRROR;

			assert_ok!(Pallet::<Test>::eth_substrate_call(
				Origin::EthTransaction(ALICE).into(),
				mirror_logs(FULL_LOGS + 2),
				vec![],
			));

			assert_eq!(eth_extrinsic_revert(), Some(crate::Error::<Test>::ReceiptTooLarge.into()));
			assert!(contract_emitted_data(mirror).is_empty(), "the events rolled back");
			let receipt = finalize_single_receipt();
			assert!(!receipt.receipt.status.coerce_status());
			assert!(receipt.receipt.logs.is_empty());
		});
	}

	/// Outside an ethereum transaction, mirrored logs go to the block's synthetic transaction,
	/// whose receipt has the same limit. A log that does not fit stays a substrate-only event.
	#[test]
	fn buffered_logs_stop_at_the_limit() {
		ExtBuilder::default().build().execute_with(|| {
			let mirror = MIRROR;
			Contracts::on_initialize(1);

			assert_ok!(pallet_dummy::Pallet::<Test>::emit_logs(
				RuntimeOrigin::signed(ALICE),
				mirror,
				vec![LOG_DATA; FULL_LOGS as usize + 2],
			));

			assert_eq!(contract_emitted_data(mirror).len(), FULL_LOGS as usize + 2);
			Contracts::on_finalize(1);
			let synthetic = Pallet::<Test>::eth_synthetic_transaction()
				.expect("the buffered logs make up a synthetic transaction");
			assert_eq!(synthetic.log_event_indices.len(), FULL_LOGS as usize + 1);

			// The next block starts with an empty buffer.
			initialize_block(2);
			Contracts::on_initialize(2);
			assert_ok!(pallet_dummy::Pallet::<Test>::emit_logs(
				RuntimeOrigin::signed(ALICE),
				mirror,
				vec![LOG_DATA],
			));
			Contracts::on_finalize(2);
			let synthetic = Pallet::<Test>::eth_synthetic_transaction()
				.expect("the next block buffers its log");
			assert_eq!(synthetic.log_event_indices.len(), 1);
		});
	}

	/// The bytes a log without topics and with 32 bytes of data takes up in the receipt, as the
	/// logs of `MirrorAssetTransfersCallback` and of `mirror_from_frame(vec![32])` do.
	fn mirrored_log_len() -> u32 {
		AccumulateReceipt::encoded_log_len(&MIRROR, &[0u8; 32], &[]) as u32
	}

	/// The precompile and the call data that make the test runtime's precompile mirror one log
	/// from `MIRROR` per entry of `data_lens`, inside the precompile's frame.
	fn mirror_from_frame(data_lens: Vec<u32>) -> (Address, alloy_core::primitives::Bytes) {
		let precompile = Address::from(NoInfo::<Test>::MATCHER.base_address());
		let call = INoInfo::callRuntimeCall { call: mirror_log_sizes(data_lens).encode().into() };
		(precompile, call.abi_encode().into())
	}

	/// Call data for `emitLogsThenCall`: `FULL_LOGS` logs of `LOG_DATA` bytes and one of
	/// `last_size` bytes, then a call that mirrors a log with 32 bytes of data inside the
	/// precompile's frame.
	fn emit_logs_then_mirror(last_size: u32) -> Vec<u8> {
		let (target, data) = mirror_from_frame(vec![32]);
		LogVolume::emitLogsThenCallCall {
			count: FULL_LOGS,
			size: LOG_DATA,
			lastSize: last_size,
			target,
			data,
		}
		.abi_encode()
	}

	/// The value of `LogVolume::stored` at `addr`.
	fn stored(addr: H160) -> u64 {
		let stored = builder::bare_call(addr)
			.data(LogVolume::storedCall {}.abi_encode())
			.build_and_unwrap_result();
		LogVolume::storedCall::abi_decode_returns(&stored.data).unwrap().to()
	}

	/// The addresses of the receipt's logs, in order.
	fn log_addresses(receipt: &alloy_consensus::ReceiptWithBloom) -> Vec<H160> {
		receipt.receipt.logs.iter().map(|log| H160(log.address.0.0)).collect()
	}

	/// The PGAS `ALICE` holds outside any hold.
	fn alice_pgas() -> u128 {
		Assets::balance(PGAS_ASSET_ID, &ALICE)
	}

	/// The `eth_call` the `eth_call` helper dispatches.
	fn eth_call_extrinsic(addr: H160, data: Vec<u8>) -> RuntimeCall {
		RuntimeCall::Contracts(crate::Call::eth_call {
			dest: addr,
			value: 0u32.into(),
			weight_limit: WEIGHT_LIMIT,
			eth_gas_limit: ETH_GAS_LIMIT.into(),
			data,
			transaction_encoded: TransactionSigned::TransactionLegacySigned(Default::default())
				.signed_payload(),
			effective_gas_price: 0u32.into(),
			encoded_len: 0,
			authorization_list: vec![],
		})
	}

	/// Check that the block's only transaction is billed in its receipt the fee `call` is charged
	/// for `post_info`, and no storage deposit.
	fn assert_receipt_gas_is_the_fee(call: &RuntimeCall, post_info: PostDispatchInfo) {
		let info = <Test as Config>::FeeInfo::dispatch_info(call);
		let fee = <Test as Config>::FeeInfo::compute_actual_fee(0, &info, &Ok(post_info));
		let (gas, rest) =
			Pallet::<Test>::convert_native_to_evm(fee).div_mod(Pallet::<Test>::evm_base_fee());
		let gas = if rest.is_zero() { gas } else { gas.saturating_add(1u32.into()) };
		let gas_info = EthBlockBuilderIR::<Test>::get().gas_info;
		assert_eq!(gas_info.len(), 1);
		assert_eq!(gas_info[0].gas_used, gas);
	}

	/// Enough PGAS for `ALICE` to pay every storage deposit of a test.
	const ALICE_PGAS: u128 = 1_000_000_000_000_000;

	/// A storage deposit paid in PGAS is mirrored as a log after the contract returned, outside
	/// any frame. With room left for it, it is the receipt's last log.
	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn mirrored_deposit_log_that_fits_is_kept(fixture_type: FixtureType) {
		ExtBuilder::default()
			.with_pgas_balances(vec![(ALICE, ALICE_PGAS)])
			.build()
			.execute_with(|| {
				let addr = deploy(fixture_type);
				MirrorAssetTransfers::set(true);
				let pgas_before = alice_pgas();
				let last_size = LOG_DATA - mirrored_log_len();

				assert_ok!(eth_call(
					addr,
					LogVolume::storeAndEmitLogsCall {
						count: FULL_LOGS,
						size: LOG_DATA,
						lastSize: last_size
					}
					.abi_encode(),
				));

				assert_eq!(eth_extrinsic_revert(), None);
				assert!(alice_pgas() < pgas_before, "the deposit is paid in PGAS");
				assert_eq!(stored(addr), 1);
				let receipt = finalize_single_receipt();
				assert!(receipt.receipt.status.coerce_status());
				let mut expected = vec![addr; FULL_LOGS as usize + 1];
				expected.push(ASSET_TRANSFER_LOG_ADDR);
				assert_eq!(log_addresses(&receipt), expected);
			});
	}

	/// A mirrored storage deposit log that does not fit fails the transaction once the deposits
	/// are paid. The deposit, the storage write and every log roll back, and the receipt bills
	/// only the fee of the failed transaction.
	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn mirrored_deposit_log_past_the_limit_fails_the_call(fixture_type: FixtureType) {
		ExtBuilder::default()
			.with_pgas_balances(vec![(ALICE, ALICE_PGAS)])
			.build()
			.execute_with(|| {
				let addr = deploy(fixture_type);
				MirrorAssetTransfers::set(true);
				let pgas_before = alice_pgas();
				let last_size = LOG_DATA - mirrored_log_len() + 1;
				let data = LogVolume::storeAndEmitLogsCall {
					count: FULL_LOGS,
					size: LOG_DATA,
					lastSize: last_size,
				}
				.abi_encode();

				let post_info =
					eth_call(addr, data.clone()).expect("the failed transaction is still included");

				assert_eq!(
					eth_extrinsic_revert(),
					Some(crate::Error::<Test>::ReceiptTooLarge.into())
				);
				assert_eq!(alice_pgas(), pgas_before, "the deposit rolled back");
				assert_eq!(stored(addr), 0, "the storage write rolled back");
				assert!(contract_emitted_data(addr).is_empty(), "the events rolled back");
				assert!(contract_emitted_data(ASSET_TRANSFER_LOG_ADDR).is_empty());
				assert_receipt_gas_is_the_fee(&eth_call_extrinsic(addr, data), post_info);
				let receipt = finalize_single_receipt();
				assert!(!receipt.receipt.status.coerce_status());
				assert!(receipt.receipt.logs.is_empty());
			});
	}

	/// An instantiation fails the same way when a mirrored storage deposit log does not fit, and
	/// leaves no contract behind.
	///
	/// The code deposit is paid, and its log mirrored, before the contract returns: for PVM code
	/// before the constructor runs, for EVM code when the constructor returns the runtime code.
	/// The test leaves room for that log, so that the first log that does not fit is one mirrored
	/// after the contract returned.
	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn mirrored_deposit_log_past_the_limit_fails_the_instantiation(fixture_type: FixtureType) {
		let ext = || ExtBuilder::default().with_pgas_balances(vec![(ALICE, ALICE_PGAS)]).build();
		let next_address = || {
			let nonce: u64 = System::account_nonce(&ALICE).into();
			crate::address::create1(&ALICE_ADDR, nonce.saturating_sub(1))
		};

		ext().execute_with(|| {
			deploy(fixture_type);
			MirrorAssetTransfers::set(true);
			let addr = next_address();

			assert_ok!(eth_instantiate(fixture_type, LOG_DATA - 3 * mirrored_log_len()).0);

			assert_eq!(eth_extrinsic_revert(), None);
			assert!(!Pallet::<Test>::code(&addr).is_empty());
			let receipt = finalize_single_receipt();
			let logs = log_addresses(&receipt);
			assert_eq!(logs.iter().filter(|log| **log == addr).count(), FULL_LOGS as usize + 1);
			assert_eq!(
				logs.iter().filter(|log| **log == ASSET_TRANSFER_LOG_ADDR).count(),
				2,
				"the code deposit and, after the contract returned, the contract's deposit",
			);
			assert_eq!(logs.last(), Some(&ASSET_TRANSFER_LOG_ADDR));
		});

		ext().execute_with(|| {
			deploy(fixture_type);
			MirrorAssetTransfers::set(true);
			let addr = next_address();
			let pgas_before = alice_pgas();

			let (result, call) =
				eth_instantiate(fixture_type, LOG_DATA - 2 * mirrored_log_len() + 1);
			let post_info = result.expect("the failed transaction is still included");

			assert_eq!(eth_extrinsic_revert(), Some(crate::Error::<Test>::ReceiptTooLarge.into()));
			assert!(Pallet::<Test>::code(&addr).is_empty(), "no contract is left behind");
			assert_eq!(alice_pgas(), pgas_before, "the deposits rolled back");
			assert_receipt_gas_is_the_fee(&call, post_info);
			let receipt = finalize_single_receipt();
			assert!(!receipt.receipt.status.coerce_status());
			assert!(receipt.receipt.logs.is_empty());
		});
	}

	/// Once a log did not fit, a later log that would fit is refused as well. Were it kept, it
	/// would commit the receipt again without the overflow and the transaction would succeed
	/// with a log missing.
	#[test]
	fn log_after_an_overflow_fails_the_transaction() {
		ExtBuilder::default().build().execute_with(|| {
			deploy(FixtureType::Solc);
			// 32 logs that leave room for an empty log but not for one of `LOG_DATA` bytes.
			let filler = LOG_DATA - 64;
			let room = MAX_RECEIPT_LOGS_BYTES as usize -
				(FULL_LOGS as usize + 1) *
					AccumulateReceipt::encoded_log_len(
						&MIRROR,
						&vec![0u8; filler as usize],
						&[],
					);
			assert!(
				room < AccumulateReceipt::encoded_log_len(&MIRROR, &[0u8; LOG_DATA as usize], &[])
			);
			assert!(room >= AccumulateReceipt::encoded_log_len(&MIRROR, &[], &[]));
			let mut data_lens = vec![filler; FULL_LOGS as usize + 1];
			data_lens.extend([LOG_DATA, 0]);

			assert_ok!(Pallet::<Test>::eth_substrate_call(
				Origin::EthTransaction(ALICE).into(),
				mirror_log_sizes(data_lens),
				vec![],
			));

			assert_eq!(eth_extrinsic_revert(), Some(crate::Error::<Test>::ReceiptTooLarge.into()));
			assert!(contract_emitted_data(MIRROR).is_empty(), "the events rolled back");
			let receipt = finalize_single_receipt();
			assert!(!receipt.receipt.status.coerce_status());
			assert!(receipt.receipt.logs.is_empty());
		});
	}

	/// A log the runtime mirrors inside a frame fails that frame when it does not fit, and the
	/// caller can handle the failure: its own logs and its later storage write are kept.
	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn caller_survives_a_mirrored_log_past_the_limit(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			let addr = deploy(fixture_type);

			assert_ok!(eth_call(addr, emit_logs_then_mirror(LOG_DATA - mirrored_log_len())));

			assert_eq!(eth_extrinsic_revert(), None);
			assert_eq!(stored(addr), 1, "the precompile call succeeded");
			let receipt = finalize_single_receipt();
			let mut expected = vec![addr; FULL_LOGS as usize + 1];
			expected.push(MIRROR);
			assert_eq!(log_addresses(&receipt), expected);
		});

		ExtBuilder::default().build().execute_with(|| {
			let addr = deploy(fixture_type);

			assert_ok!(eth_call(addr, emit_logs_then_mirror(LOG_DATA - mirrored_log_len() + 1)));

			assert_eq!(eth_extrinsic_revert(), None);
			assert_eq!(stored(addr), 2, "the precompile call failed and the caller went on");
			assert!(contract_emitted_data(MIRROR).is_empty(), "the failed frame's log rolled back");
			let receipt = finalize_single_receipt();
			assert!(receipt.receipt.status.coerce_status());
			assert_eq!(log_addresses(&receipt), vec![addr; FULL_LOGS as usize + 1]);
		});
	}

	/// The call tracer records only the logs the receipt kept, for a log emitted by a contract and
	/// for one the runtime mirrors.
	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn tracer_records_only_logs_that_fit(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			let addr = deploy(fixture_type);
			let new_tracer =
				|| CallTracer::new(CallTracerConfig { with_logs: true, only_top_call: false });

			let mut tracer = new_tracer();
			let _ = trace(&mut tracer, || eth_call(addr, emit_logs(FULL_LOGS, LOG_DATA + 1)));
			let call_trace = tracer.collect_trace().expect("the call is traced");
			assert_eq!(call_trace.logs.len(), FULL_LOGS as usize);

			let mut tracer = new_tracer();
			let _ = trace(&mut tracer, || {
				eth_call(addr, emit_logs_then_mirror(LOG_DATA - mirrored_log_len() + 1))
			});
			let call_trace = tracer.collect_trace().expect("the call is traced");
			assert_eq!(call_trace.logs.len(), FULL_LOGS as usize + 1);
			assert_eq!(call_trace.calls.len(), 1, "the call into the precompile");
			assert!(call_trace.calls[0].logs.is_empty());
		});
	}

	/// Run `LogVolumeConstructor` as an ethereum transaction that deploys it. Also returns the
	/// dispatched call.
	fn eth_instantiate(
		fixture_type: FixtureType,
		last_size: u32,
	) -> (DispatchResultWithPostInfo, RuntimeCall) {
		let (mut code, _) = compile_module_with_type("LogVolumeConstructor", fixture_type).unwrap();
		let args = LogVolumeConstructor::constructorCall {
			count: FULL_LOGS,
			size: LOG_DATA,
			lastSize: last_size,
		}
		.abi_encode();
		// EVM constructors take their arguments appended to the init code.
		let data = match fixture_type {
			FixtureType::Solc => {
				code.extend(args);
				vec![]
			},
			_ => args,
		};
		let call = RuntimeCall::Contracts(crate::Call::eth_instantiate_with_code {
			value: 0u32.into(),
			weight_limit: WEIGHT_LIMIT,
			eth_gas_limit: ETH_GAS_LIMIT.into(),
			code,
			data,
			transaction_encoded: TransactionSigned::Transaction4844Signed(Default::default())
				.signed_payload(),
			effective_gas_price: 0u32.into(),
			encoded_len: 0,
		});
		(call.clone().dispatch(Origin::EthTransaction(ALICE).into()), call)
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn constructor_logs_count_against_the_limit(fixture_type: FixtureType) {
		ExtBuilder::default().build().execute_with(|| {
			deploy(fixture_type);

			assert_ok!(eth_instantiate(fixture_type, LOG_DATA).0);
			assert_eq!(eth_extrinsic_revert(), None);
			let receipt = finalize_single_receipt();
			assert_eq!(receipt.receipt.logs.len(), FULL_LOGS as usize + 1);
		});

		ExtBuilder::default().build().execute_with(|| {
			deploy(fixture_type);

			assert_ok!(eth_instantiate(fixture_type, LOG_DATA + 1).0);
			assert_eq!(eth_extrinsic_revert(), Some(crate::Error::<Test>::ReceiptTooLarge.into()));
			let receipt = finalize_single_receipt();
			assert!(receipt.receipt.logs.is_empty());
		});
	}

	/// A dry run of a runtime call applies the limit to the logs the call mirrors.
	#[test]
	fn dry_run_of_a_runtime_call_applies_the_limit() {
		ExtBuilder::default().build().execute_with(|| {
			deploy(FixtureType::Solc);
			let dry_run_pallets = |count| {
				Pallet::<Test>::dry_run_eth_transact(
					GenericTransaction {
						from: Some(ALICE_ADDR),
						to: Some(RUNTIME_PALLETS_ADDR),
						input: mirror_logs(count).encode().into(),
						..Default::default()
					},
					None,
					false,
					None,
				)
			};

			assert_ok!(dry_run_pallets(FULL_LOGS + 1));
			let err = dry_run_pallets(FULL_LOGS + 2).unwrap_err();
			assert!(
				matches!(&err, EthTransactError::Message(msg) if msg.contains("ReceiptTooLarge")),
				"{err:?}",
			);
		});
	}
}
