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

//! Which state entries the access list finds cold or hot, on both vms.

use crate::{
	BalanceOf, Code, Config,
	access_list::{Access, AccessListMetrics, CallItems, CodeLoadItems, CreateItems},
	limits,
	test_utils::{ALICE, builder::Contract},
	tests::{ExtBuilder, RuntimeOrigin, Test, access_list_metrics_of, builder},
};
use alloy_core::sol_types::SolCall;
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{
	CallEach, CallThenRevert, Caller, Counter, Factory, FixtureType, Recurse,
	compile_module_with_type,
};
use pretty_assertions::assert_eq;
use sp_core::{H160, H256};
use test_case::test_case;

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn call_and_delegate_reuse_target_warmth(caller_type: FixtureType, target_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", caller_type).unwrap();
	let (target_code, _) = compile_module_with_type("Caller", target_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr: caller, .. } = builder::bare_instantiate(Code::Upload(caller_code))
			.salt(Some([1; 32]))
			.build_and_unwrap_contract();
		let Contract { addr: target, .. } = builder::bare_instantiate(Code::Upload(target_code))
			.salt(Some([2; 32]))
			.build_and_unwrap_contract();

		let noop_call = Caller::dataCall {}.abi_encode();

		let call_target_with = |data: Vec<u8>| {
			access_list_metrics_of(|| {
				builder::bare_call(caller)
					.data(
						Caller::normalCall {
							_callee: target.0.into(),
							_value: 0,
							_data: data.into(),
							_gas: u64::MAX,
						}
						.abi_encode(),
					)
					.build_and_unwrap_result();
			})
		};

		let plain_only = call_target_with(noop_call.clone());

		let plain_then_plain = call_target_with(
			Caller::normalCall {
				_callee: target.0.into(),
				_value: 0,
				_data: noop_call.clone().into(),
				_gas: u64::MAX,
			}
			.abi_encode(),
		);

		let plain_then_delegate = call_target_with(
			Caller::delegateCall {
				_callee: target.0.into(),
				_data: noop_call.into(),
				_gas: u64::MAX,
			}
			.abi_encode(),
		);

		assert_eq!(
			plain_then_plain.cold, plain_only.cold,
			"a warm plain re-call adds no new cold touch",
		);
		assert_eq!(
			plain_then_delegate.cold, plain_only.cold,
			"a warm delegate adds no new cold touch",
		);
		assert_eq!(
			plain_then_plain.hot,
			plain_only.hot + CallItems::plain_entries(),
			"the plain re-call re-reads the target's account and code hot",
		);
		assert_eq!(
			plain_then_delegate.hot,
			plain_only.hot + CallItems::delegate_entries(),
			"the delegate re-reads account info and code hot, but not the original account",
		);
	});
}

#[test_case(FixtureType::Solc;   "evm")]
#[test_case(FixtureType::Resolc; "pvm")]
fn a_denied_call_keeps_its_target_only_if_the_frame_runs_on(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Recurse", fixture_type).unwrap();
	// The two VMs differ once the depth limit denies a call, though both charge the same for it:
	// EVM keeps the frame running, so its touch stays, while PVM traps the frame and its touch is
	// rolled back.
	let target_entries_kept = if fixture_type == FixtureType::Resolc {
		0
	} else {
		CallItems::new(H160::zero(), false).entry_count() as usize
	};
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let recurse_to_the_limit_then_call = |final_target: H160| {
			access_list_metrics_of(|| {
				builder::bare_call(addr)
					.data(
						Recurse::recurseCall {
							callsLeft: limits::CALL_STACK_DEPTH,
							finalTarget: final_target.0.into(),
						}
						.abi_encode(),
					)
					.build_and_unwrap_result();
			})
		};

		let no_denied_call = recurse_to_the_limit_then_call(H160::zero()).size;
		let denied_call = recurse_to_the_limit_then_call(H160::from_low_u64_be(0xdead)).size;
		assert_eq!(
			denied_call,
			no_denied_call + target_entries_kept,
			"the denied call leaves its target's entries only if the frame runs on",
		);
	});
}

#[test_case(FixtureType::Solc;   "evm")]
#[test_case(FixtureType::Resolc; "pvm")]
fn call_past_the_depth_limit_pays_by_target_warmth(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Recurse", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		// The innermost frame already sits at the depth limit, so the call it makes to
		// `final_target` is denied. The zero address stands for making no such call at all.
		let recurse_to_the_limit_then_call = |final_target: H160| {
			access_list_metrics_of(|| {
				builder::bare_call(addr)
					.data(
						Recurse::recurseCall {
							callsLeft: limits::CALL_STACK_DEPTH,
							finalTarget: final_target.0.into(),
						}
						.abi_encode(),
					)
					.build_and_unwrap_result();
			})
		};

		let no_denied_call = recurse_to_the_limit_then_call(H160::zero());
		let warm_target = recurse_to_the_limit_then_call(addr);
		let cold_target = recurse_to_the_limit_then_call(H160::from_low_u64_be(0xdead));

		assert_eq!(
			warm_target.hot - no_denied_call.hot,
			2,
			"a denied call pays hot for a target already in the list: its mapping and account info",
		);
		assert_eq!(
			warm_target.cold, no_denied_call.cold,
			"a target already in the list costs the denied call nothing cold",
		);
		assert_eq!(
			cold_target.cold - no_denied_call.cold,
			2,
			"a denied call pays cold for the same two entries when the target is not in the list",
		);
		assert_eq!(
			cold_target.hot, no_denied_call.hot,
			"a target outside the list costs the denied call nothing hot",
		);
	});
}

#[test_case(FixtureType::Solc;   "evm")]
#[test_case(FixtureType::Resolc; "pvm")]
fn value_transfer_warms_the_account(fixture_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr: caller, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();
		// Fund the caller so it can forward value.
		let _ = crate::Pallet::<Test>::set_evm_balance(&caller, 100_000_000_000u128.into());

		let eoa = H160::from([0xfe; 20]);
		let call_with_value = |value: u64| {
			access_list_metrics_of(|| {
				builder::bare_call(caller)
					.data(
						Caller::normalCall {
							_callee: eoa.0.into(),
							_value: value,
							_data: Vec::<u8>::new().into(),
							_gas: u64::MAX,
						}
						.abi_encode(),
					)
					.build_and_unwrap_result();
			})
		};

		let zero_value = call_with_value(0);
		let with_value = call_with_value(1_000_000);

		let value_transfer_only = CallItems::transfer_entries();
		let extra_cold = with_value.cold - zero_value.cold;
		let extra_hot = with_value.hot - zero_value.hot;

		assert_eq!(
			extra_hot, 2,
			"two are already warm: the sender's account, and the callee's account info",
		);
		assert_eq!(
			extra_cold,
			value_transfer_only - extra_hot,
			"the rest of the value-transfer state is newly touched",
		);
	});
}

#[test_case(FixtureType::Solc;   "evm")]
#[test_case(FixtureType::Resolc; "pvm")]
fn storage_reread_is_hot(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Counter", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let metrics_after = |data: Vec<u8>| {
			access_list_metrics_of(|| {
				builder::bare_call(addr).data(data).build_and_unwrap_result();
			})
		};

		// A single write to the slot: touched cold once, never hot.
		let write_only = metrics_after(Counter::setNumberCall { newNumber: 1 }.abi_encode());

		let read_then_write = metrics_after(Counter::incrementCall {}.abi_encode());

		assert_eq!(read_then_write.cold, write_only.cold, "the slot is touched cold exactly once",);
		assert_eq!(
			read_then_write.hot,
			write_only.hot + 1,
			"the read warms the slot, so the following write is hot",
		);
	});
}

/// Deploys a `CallEach` and `N` `CallThenRevert`s.
fn deploy_caller_and_targets<const N: usize>(fixture_type: FixtureType) -> (H160, [H160; N]) {
	let (caller_code, _) = compile_module_with_type("CallEach", fixture_type).unwrap();
	let (target_code, _) = compile_module_with_type("CallThenRevert", fixture_type).unwrap();
	let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
	let caller = builder::bare_instantiate(Code::Upload(caller_code))
		.build_and_unwrap_contract()
		.addr;
	let targets = core::array::from_fn(|index| {
		builder::bare_instantiate(Code::Upload(target_code.clone()))
			.salt(Some([index as u8 + 1; 32]))
			.build_and_unwrap_contract()
			.addr
	});
	(caller, targets)
}

/// Makes `calls` from `caller` in one transaction and returns its access-list metrics.
fn access_list_metrics_of_calls(caller: H160, calls: &[(H160, Vec<u8>)]) -> AccessListMetrics {
	let targets = calls.iter().map(|(target, _)| target.0.into()).collect();
	let data = calls.iter().map(|(_, data)| data.clone().into()).collect();
	access_list_metrics_of(|| {
		builder::bare_call(caller)
			.data(CallEach::callEachCall { targets, data }.abi_encode())
			.build_and_unwrap_result();
	})
}

#[test_case(FixtureType::Solc;   "evm")]
#[test_case(FixtureType::Resolc; "pvm")]
fn callee_revert_drops_only_its_own_touches(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let (caller, [reverter, inner]) = deploy_caller_and_targets::<2>(fixture_type);
		let call_inner_then_revert =
			CallThenRevert::callThenRevertCall { target: inner.0.into() }.abi_encode();
		let call_items_count = CallItems::new(H160::zero(), false).entry_count();
		let code_load_count = CodeLoadItems { hash: H256::zero() }.entry_count();

		let no_revert = access_list_metrics_of_calls(caller, &[(reverter, Vec::new())]);
		let revert =
			access_list_metrics_of_calls(caller, &[(reverter, call_inner_then_revert.clone())]);
		assert_eq!(revert.cold - no_revert.cold, call_items_count, "`inner`'s entries are cold",);
		assert_eq!(
			revert.hot - no_revert.hot,
			code_load_count,
			"`inner`'s code is hot, shared with the callee",
		);
		assert_eq!(revert.size, no_revert.size, "the revert removes `inner`'s entries");

		let revert_then_reverter = access_list_metrics_of_calls(
			caller,
			&[(reverter, call_inner_then_revert), (reverter, Vec::new())],
		);
		assert_eq!(
			revert_then_reverter.cold, revert.cold,
			"the second call to the callee adds nothing cold",
		);
		assert_eq!(
			revert_then_reverter.hot - revert.hot,
			CallItems::plain_entries(),
			"the second call to the callee is all hot",
		);
	});
}

#[test_case(FixtureType::Solc;   "evm")]
#[test_case(FixtureType::Resolc; "pvm")]
fn shared_code_is_hot_for_the_second_contract(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let (caller, [first, second]) = deploy_caller_and_targets::<2>(fixture_type);

		let first_only = access_list_metrics_of_calls(caller, &[(first, Vec::new())]);
		let both =
			access_list_metrics_of_calls(caller, &[(first, Vec::new()), (second, Vec::new())]);

		assert_eq!(
			both.cold - first_only.cold,
			CallItems::new(H160::zero(), false).entry_count(),
			"the second contract's entries are cold",
		);
		assert_eq!(
			both.hot - first_only.hot,
			CodeLoadItems { hash: H256::zero() }.entry_count(),
			"the second contract's code is hot, shared with the first",
		);
	});
}

fn deploy_factory(fixture_type: FixtureType) -> H160 {
	let (factory_code, _) = compile_module_with_type("Factory", fixture_type).unwrap();
	let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000_000);
	if fixture_type == FixtureType::Resolc {
		for child in ["ReceivingChild", "RevertingChild"] {
			let (child_code, _) = compile_module_with_type(child, fixture_type).unwrap();
			crate::Pallet::<Test>::upload_code(
				RuntimeOrigin::signed(ALICE.clone()),
				child_code,
				<BalanceOf<Test>>::MAX,
			)
			.unwrap();
		}
	}
	let factory = builder::bare_instantiate(Code::Upload(factory_code))
		.build_and_unwrap_contract()
		.addr;
	let _ = crate::Pallet::<Test>::set_evm_balance(&factory, 100_000_000_000u128.into());
	factory
}

#[test_case(FixtureType::Solc;   "evm")]
#[test_case(FixtureType::Resolc; "pvm")]
fn a_contract_creation_warms_the_new_contract(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		// solc stores `ReceivingChild`'s code on the first contract creation; resolc uploaded it.
		let stores_code = matches!(fixture_type, FixtureType::Solc);
		let expected_extra_entries = CreateItems::warming_summary(stores_code).total() as usize;

		let factory = deploy_factory(fixture_type);
		let size_after = |data: Vec<u8>| {
			access_list_metrics_of(|| {
				builder::bare_call(factory).data(data).build_and_unwrap_result();
			})
			.size
		};

		let created_size = size_after(Factory::createCall {}.abi_encode());
		let failed_size = size_after(Factory::createRevertingCall {}.abi_encode());

		let actual_extra_entries = created_size - failed_size;
		assert_eq!(actual_extra_entries, expected_extra_entries);
	});
}

#[test_case(FixtureType::Solc;   "evm")]
#[test_case(FixtureType::Resolc; "pvm")]
fn a_failed_contract_creation_leaves_its_address_cold(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let expected_extra_entries = CreateItems { address: H160::zero() }.entry_count() as usize;
		let expected_failed_extra_entries = if fixture_type == FixtureType::Resolc {
			// The caller loads the code by hash, outside the reverting frame, so it stays warm.
			CodeLoadItems { hash: H256::zero() }.entry_count() as usize
		} else {
			0
		};

		let factory = deploy_factory(fixture_type);
		let size_after = |data: Vec<u8>| {
			access_list_metrics_of(|| {
				builder::bare_call(factory).data(data).build_and_unwrap_result();
			})
			.size
		};
		// Stores `ReceivingChild`'s code, so the successful contract creation below stores nothing.
		builder::bare_call(factory)
			.data(Factory::createCall {}.abi_encode())
			.build_and_unwrap_result();

		let noop_size = size_after(Factory::noopCall {}.abi_encode());
		let created_size = size_after(Factory::createCall {}.abi_encode());
		let failed_size = size_after(Factory::createRevertingCall {}.abi_encode());

		let actual_extra_entries = created_size - failed_size;
		assert_eq!(
			actual_extra_entries, expected_extra_entries,
			"a failed constructor leaves the entries at its address cold",
		);
		let actual_failed_extra_entries = failed_size - noop_size;
		assert_eq!(
			actual_failed_extra_entries, expected_failed_extra_entries,
			"only the code the caller loaded stays warm after the constructor reverts",
		);
	});
}

#[test]
fn newly_stored_code_is_hot_for_the_next_call() {
	ExtBuilder::default().build().execute_with(|| {
		let created_account_info = 1; // the child's AccountInfo, warmed by the contract creation
		let code_load = CodeLoadItems { hash: H256::zero() }.entry_count();
		let expected_stores_the_code_hot = code_load + created_account_info;
		let expected_finds_the_code_stored_hot = created_account_info;

		let factory = deploy_factory(FixtureType::Solc);
		let create_then_call = || {
			access_list_metrics_of(|| {
				builder::bare_call(factory)
					.data(
						Factory::createThenCallCall { value: 0u64.try_into().unwrap() }
							.abi_encode(),
					)
					.build_and_unwrap_result();
			})
		};

		let stores_the_code = create_then_call();
		let finds_the_code_stored = create_then_call();

		// Both find the child's AccountInfo hot; only the first stores and warms the code.
		assert_eq!(stores_the_code.hot, expected_stores_the_code_hot,);
		assert_eq!(finds_the_code_stored.hot, expected_finds_the_code_stored_hot,);
		assert_eq!(
			stores_the_code.cold, finds_the_code_stored.cold,
			"both transactions touch the same entries",
		);
	});
}

#[test_case(FixtureType::Solc;   "evm")]
#[test_case(FixtureType::Resolc; "pvm")]
fn a_created_contract_is_warm_for_the_rest_of_the_transaction(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let (expected_child_call_cold, expected_child_call_hot) =
			if fixture_type == FixtureType::Resolc {
				// Instantiating from a code hash loads the code, which warms it.
				(1, 3) // cold: OriginalAccount; hot: AccountInfo, CodeInfo, CodeBlob
			} else {
				// The code is already stored, so CREATE never reads it and it stays cold.
				(3, 1) // cold: OriginalAccount, CodeInfo, CodeBlob; hot: AccountInfo
			};
		let expected_transfer_cold = 1; // creator's Account
		let expected_transfer_hot = 3; // child's Account, AccountInfo; creator's AccountInfo

		let factory = deploy_factory(fixture_type);
		let metrics_of = |data: Vec<u8>| {
			access_list_metrics_of(|| {
				builder::bare_call(factory).data(data).build_and_unwrap_result();
			})
		};
		let create_then_call = |value: u64| {
			metrics_of(
				Factory::createThenCallCall { value: value.try_into().unwrap() }.abi_encode(),
			)
		};
		// Stores `ReceivingChild`'s code, so each contract creation below finds it stored.
		builder::bare_call(factory)
			.data(Factory::createCall {}.abi_encode())
			.build_and_unwrap_result();

		let create_only = metrics_of(Factory::createCall {}.abi_encode());
		let zero_value_call = create_then_call(0);
		let value_call = create_then_call(1_000_000);

		let actual_child_call_cold = zero_value_call.cold - create_only.cold;
		let actual_child_call_hot = zero_value_call.hot - create_only.hot;
		let actual_transfer_cold = value_call.cold - zero_value_call.cold;
		let actual_transfer_hot = value_call.hot - zero_value_call.hot;

		assert_eq!(actual_child_call_cold, expected_child_call_cold);
		assert_eq!(actual_child_call_hot, expected_child_call_hot);
		assert_eq!(actual_transfer_cold, expected_transfer_cold);
		assert_eq!(actual_transfer_hot, expected_transfer_hot);
	});
}
