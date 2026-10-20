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

//! Which state entries the access list finds cold or hot, on both VMs.

use crate::{
	AccountInfo, BalanceOf, CallResources, Code, Config, ExecConfig, ReentrancyProtection, Weight,
	WeightInfo, WeightToken,
	access_list::{
		Access, AccessListMetrics, CallItems, CodeLoadItems, InstantiateItems, KeyFamily,
		StorageOp, TransferItems, Warmth, WarmthSummary,
	},
	call_builder::CallSetup,
	exec::{BuiltinPrecompileExt, PrecompileExt},
	limits,
	precompiles::Precompile,
	test_utils::{ALICE, ALICE_ADDR, DJANGO_ADDR, EVE_ADDR, builder::Contract},
	tests::{
		ExtBuilder, INoInfo, IWithInfo, NoInfo, RuntimeOrigin, Test, WithInfo,
		access_list_metrics_of, builder,
	},
	vm::RuntimeCosts,
};
use alloy_core::sol_types::SolCall;
use frame_support::{assert_ok, traits::fungible::Mutate};
use pallet_revive_fixtures::{
	CallEach, CallThenRevert, Caller, Counter, Factory, FixtureType, Recurse, SendBack,
	compile_module_with_type,
};
use pretty_assertions::assert_eq;
use sp_core::{H160, H256, U256};
use test_case::test_case;

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn a_storage_write_after_a_read_is_hot(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Counter", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let metrics_of = |data: Vec<u8>| access_list_metrics_of_call(addr, data);

		let write_only = metrics_of(Counter::setNumberCall { newNumber: 1 }.abi_encode());
		let read_then_write = metrics_of(Counter::incrementCall {}.abi_encode());

		assert_eq!(read_then_write.cold, write_only.cold, "the slot is touched cold exactly once");
		assert_eq!(read_then_write.hot, write_only.hot + 1, "the read warms the slot");
	});
}

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

		let data_call = Caller::dataCall {}.abi_encode();
		let plain = |data: Vec<u8>| {
			Caller::normalCall {
				_callee: target.0.into(),
				_value: 0,
				_data: data.into(),
				_gas: u64::MAX,
			}
			.abi_encode()
		};
		let delegate = |data: Vec<u8>| {
			Caller::delegateCall { _callee: target.0.into(), _data: data.into(), _gas: u64::MAX }
				.abi_encode()
		};
		let static_call = |data: Vec<u8>| {
			Caller::staticCallCall { _callee: target.0.into(), _data: data.into(), _gas: u64::MAX }
				.abi_encode()
		};
		let metrics_of = |call: Vec<u8>| access_list_metrics_of_call(caller, call);

		let plain_entries = CallItems::plain_entry_count();
		let delegate_entries = CallItems::delegate_entry_count();
		// The address mapping, which a plain call reads and a delegate call skips.
		let mapping_entry = 1;

		let plain_only = metrics_of(plain(data_call.clone()));
		let plain_then_plain = metrics_of(plain(plain(data_call.clone())));
		let plain_then_delegate = metrics_of(plain(delegate(data_call.clone())));
		let delegate_only = metrics_of(delegate(data_call.clone()));
		let delegate_then_delegate = metrics_of(delegate(delegate(data_call.clone())));
		let delegate_then_plain = metrics_of(delegate(plain(data_call.clone())));
		let static_only = metrics_of(static_call(data_call));

		assert_eq!(static_only, plain_only, "a static call touches what a plain call touches");

		assert_eq!(delegate_only.cold, plain_only.cold - mapping_entry);
		assert_eq!(plain_then_plain.cold, plain_only.cold);
		assert_eq!(plain_then_delegate.cold, plain_only.cold);
		assert_eq!(delegate_then_delegate.cold, delegate_only.cold);
		assert_eq!(delegate_then_plain.cold, delegate_only.cold + mapping_entry);

		assert_eq!(plain_then_plain.hot, plain_only.hot + plain_entries);
		assert_eq!(plain_then_delegate.hot, plain_only.hot + delegate_entries);
		assert_eq!(delegate_then_delegate.hot, delegate_only.hot + delegate_entries);
		assert_eq!(delegate_then_plain.hot, delegate_only.hot + delegate_entries);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn call_past_the_depth_limit_pays_by_target_warmth(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Recurse", fixture_type).unwrap();
	// The two VMs differ once the depth limit denies a call, though both charge the same for it:
	// EVM keeps the frame running, so its touch stays, while PVM traps the frame and its touch is
	// rolled back.
	let call_entries = CallItems::new(H160::zero(), false).entry_count();
	let entries_kept_after_denial =
		if fixture_type == FixtureType::Resolc { 0 } else { call_entries as usize };
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		// The innermost frame already sits at the depth limit, so the call it makes to
		// `final_target` is denied. The zero address stands for making no such call at all.
		let recurse_to_the_limit_then_call = |final_target: H160| {
			access_list_metrics_of_call(
				addr,
				Recurse::recurseCall {
					callsLeft: limits::CALL_STACK_DEPTH,
					finalTarget: final_target.0.into(),
				}
				.abi_encode(),
			)
		};

		let final_call_allowed = recurse_to_the_limit_then_call(H160::zero());
		let final_call_denied_warm = recurse_to_the_limit_then_call(addr);
		let final_call_denied = recurse_to_the_limit_then_call(H160::from_low_u64_be(0xdead));

		assert_eq!(
			final_call_denied_warm.hot,
			final_call_allowed.hot + call_entries,
			"a denied call pays hot for a target already in the list: its mapping and account info",
		);
		assert_eq!(final_call_denied_warm.cold, final_call_allowed.cold);
		assert_eq!(
			final_call_denied.cold,
			final_call_allowed.cold + call_entries,
			"a denied call pays cold for the same two entries when the target is not in the list",
		);
		assert_eq!(final_call_denied.hot, final_call_allowed.hot);
		assert_eq!(final_call_denied.size, final_call_allowed.size + entries_kept_after_denial);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn a_call_back_into_the_target_is_hot_only_after_call_or_eth_call(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Recurse", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
		let call_back_once =
			Recurse::recurseCall { callsLeft: 1, finalTarget: H160::zero().0.into() }.abi_encode();
		let call_entries = CallItems::new(addr, false).entry_count();

		let bare_call = access_list_metrics_of(|| {
			builder::bare_call(addr).data(call_back_once.clone()).build_and_unwrap_result();
		});
		let call = access_list_metrics_of(|| {
			assert_ok!(builder::call(addr).data(call_back_once.clone()).build());
		});
		let eth_call = access_list_metrics_of(|| {
			assert_ok!(builder::eth_call(addr).data(call_back_once).build());
		});

		assert_eq!(call.hot, bare_call.hot + call_entries);
		assert_eq!(
			call.cold, bare_call.cold,
			"both add the target's entries once: call up front, bare_call at the call back",
		);
		assert_eq!(eth_call, call);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn a_transfer_back_to_the_sender_is_hot_only_after_call_or_eth_call(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("SendBack", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
		let _ = crate::Pallet::<Test>::set_evm_balance(&addr, 100_000_000_000u128.into());
		let value_sent: BalanceOf<Test> = 1_000;
		let access_list_metrics_of_transfer = |run: &dyn Fn(Vec<u8>)| {
			let send_back = |amount| SendBack::sendBackCall { amount }.abi_encode();
			let without_value = access_list_metrics_of(|| run(send_back(0)));
			let with_value = access_list_metrics_of(|| run(send_back(1_000_000)));
			AccessListMetrics {
				size: with_value.size - without_value.size,
				cold: with_value.cold - without_value.cold,
				hot: with_value.hot - without_value.hot,
			}
		};
		let bare_call = access_list_metrics_of_transfer(&|data| {
			let result = builder::bare_call(addr)
				.native_value(value_sent)
				.data(data)
				.build_and_unwrap_result();
			assert!(!result.did_revert());
		});
		let call = access_list_metrics_of_transfer(&|data| {
			assert_ok!(builder::call(addr).value(value_sent).data(data).build());
		});
		let eth_call = access_list_metrics_of_transfer(&|data| {
			let value = crate::Pallet::<Test>::convert_native_to_evm(value_sent);
			assert_ok!(builder::eth_call(addr).value(value).data(data).build());
		});

		assert_eq!(
			call.hot,
			bare_call.hot + bare_call.cold,
			"call adds both parties' entries up front, so the transfer back finds them all hot",
		);
		assert_eq!(bare_call.hot, 1, "only ALICE's account info, which the call itself accessed");
		assert_eq!(eth_call, call);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn a_chained_delegation_call_warms_the_target_it_checks(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let (caller_addr, [reverter_addr]) = deploy_call_each_and_targets::<1>(fixture_type);
		// DJANGO -> EVE -> ALICE: a call to DJANGO traps after reading EVE's account info.
		AccountInfo::<Test>::set_delegation(&EVE_ADDR, Some(ALICE_ADDR), &ALICE).unwrap();
		AccountInfo::<Test>::set_delegation(&DJANGO_ADDR, Some(EVE_ADDR), &ALICE).unwrap();
		let django_call = (DJANGO_ADDR, Vec::new());
		let reverted_django_call = (
			reverter_addr,
			CallThenRevert::callThenRevertCall { target: DJANGO_ADDR.0.into() }.abi_encode(),
		);
		// The call's own entries, plus EVE's account info that the chained check reads.
		let django_call_entries = CallItems::new(DJANGO_ADDR, false).entry_count() + 1;

		let django_only = access_list_metrics_of_calls(caller_addr, &[django_call.clone()]);
		let django_then_django =
			access_list_metrics_of_calls(caller_addr, &[django_call.clone(), django_call.clone()]);
		assert_eq!(django_then_django.cold, django_only.cold);
		assert_eq!(
			django_then_django.hot, django_call_entries,
			"the caller reads DJANGO's and EVE's entries before DJANGO would run, so they stay hot",
		);

		let reverted_only =
			access_list_metrics_of_calls(caller_addr, &[reverted_django_call.clone()]);
		let reverted_then_django =
			access_list_metrics_of_calls(caller_addr, &[reverted_django_call, django_call]);
		assert_eq!(
			reverted_then_django.cold - reverted_only.cold,
			django_call_entries,
			"the reverting caller drops DJANGO's entries and EVE's account info",
		);
		assert_eq!(reverted_then_django.hot, 0, "the next call to DJANGO finds nothing hot");
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn a_call_to_a_precompile_warms_nothing(fixture_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr: caller, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();
		let _ = crate::Pallet::<Test>::set_evm_balance(&caller, 100_000_000_000u128.into());

		let identity = INoInfo::identityCall { number: 1 }.abi_encode();
		let dummy = IWithInfo::dummyCall {}.abi_encode();
		let no_info = NoInfo::<Test>::MATCHER.base_address();
		let with_info = WithInfo::<Test>::MATCHER.base_address();
		// The caller's call entries and code, the only entries the transaction warms.
		let caller_entries = CallItems::plain_entry_count();

		for (case, precompile, data, value) in [
			("no contract info", no_info, identity.clone(), 0),
			("contract info", with_info, dummy, 0),
			("value transfer", no_info, identity, 1_000_000),
		] {
			let metrics = access_list_metrics_of_call(
				caller,
				Caller::normalCall {
					_callee: precompile.into(),
					_value: value,
					_data: data.into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			);
			assert_eq!(
				metrics,
				AccessListMetrics { size: caller_entries as usize, cold: caller_entries, hot: 0 },
				"{case}",
			);
		}
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn callee_revert_drops_only_its_own_touches(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let (caller, [reverter, inner]) = deploy_call_each_and_targets::<2>(fixture_type);
		let call_inner_then_revert =
			CallThenRevert::callThenRevertCall { target: inner.0.into() }.abi_encode();
		let call_entries = CallItems::new(H160::zero(), false).entry_count();
		let code_entries = CodeLoadItems::new(H256::zero()).entry_count();

		let no_revert = access_list_metrics_of_calls(caller, &[(reverter, Vec::new())]);
		let revert =
			access_list_metrics_of_calls(caller, &[(reverter, call_inner_then_revert.clone())]);
		assert_eq!(revert.cold - no_revert.cold, call_entries, "`inner`'s entries are cold");
		assert_eq!(
			revert.hot - no_revert.hot,
			code_entries,
			"`inner`'s code is hot, shared with the callee",
		);
		assert_eq!(revert.size, no_revert.size, "the revert removes `inner`'s entries");

		let revert_then_reverter = access_list_metrics_of_calls(
			caller,
			&[(reverter, call_inner_then_revert), (reverter, Vec::new())],
		);
		assert_eq!(revert_then_reverter.cold, revert.cold);
		assert_eq!(
			revert_then_reverter.hot - revert.hot,
			CallItems::plain_entry_count(),
			"the second call to the callee is all hot",
		);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn shared_code_is_hot_for_the_second_contract(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let (caller, [first, second]) = deploy_call_each_and_targets::<2>(fixture_type);

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
			CodeLoadItems::new(H256::zero()).entry_count(),
			"the second contract's code is hot, shared with the first",
		);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn a_value_transfer_finds_both_account_infos_hot(fixture_type: FixtureType) {
	let (caller_code, _) = compile_module_with_type("Caller", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr: caller, .. } =
			builder::bare_instantiate(Code::Upload(caller_code)).build_and_unwrap_contract();
		let _ = crate::Pallet::<Test>::set_evm_balance(&caller, 100_000_000_000u128.into());

		let eoa = H160::from([0xfe; 20]);
		let call_with_value = |value: u64| {
			access_list_metrics_of_call(
				caller,
				Caller::normalCall {
					_callee: eoa.0.into(),
					_value: value,
					_data: Vec::<u8>::new().into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
		};

		let zero_value = call_with_value(0);
		let with_value = call_with_value(1_000_000);

		let transfer_entries = TransferItems { from: caller, to: eoa, dust: false }.entry_count();
		let extra_cold = with_value.cold - zero_value.cold;
		let extra_hot = with_value.hot - zero_value.hot;

		assert_eq!(extra_hot, 2, "the sender's and the callee's account info are hot");
		assert_eq!(
			extra_cold,
			transfer_entries - extra_hot,
			"the rest of the value-transfer entries are newly touched",
		);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn a_contract_creation_warms_the_new_contract(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		// solc stores `ReceivingChild`'s code on the first contract creation; resolc uploaded it.
		let stores_code = matches!(fixture_type, FixtureType::Solc);
		let code_entries =
			if stores_code { CodeLoadItems::new(H256::zero()).entry_count() } else { 0 };
		let expected_extra_entries =
			(InstantiateItems { address: H160::zero(), deployer: H160::zero() }.entry_count() +
				code_entries) as usize;

		let factory = deploy_factory(fixture_type);
		let size_after = |data: Vec<u8>| access_list_metrics_of_call(factory, data).size;

		let created_size = size_after(Factory::createCall {}.abi_encode());
		let failed_size = size_after(Factory::createRevertingCall {}.abi_encode());

		assert_eq!(created_size - failed_size, expected_extra_entries);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn a_contract_creation_pays_for_registering_its_entries(fixture_type: FixtureType) {
	enum AlreadyHot {
		Nothing,
		NewContract(H160),
		StoredCode(H256),
	}

	let create_weight = |already_hot| {
		ExtBuilder::default().build().execute_with(|| {
			let factory = deploy_factory(fixture_type);
			let mut setup = CallSetup::<Test>::default();
			let (mut ext, _) = setup.ext();
			let hot_entries = match already_hot {
				AlreadyHot::Nothing => 0,
				AlreadyHot::NewContract(child) => ext
					.warm(InstantiateItems { address: child, deployer: factory })
					.summary()
					.total(),
				AlreadyHot::StoredCode(hash) => {
					ext.warm(CodeLoadItems::with_info_op(hash, StorageOp::Write)).summary().total()
				},
			};
			let before = ext.frame_meter().weight_consumed();
			ext.call(
				&CallResources::NoLimits,
				&factory,
				U256::zero(),
				Factory::createCall {}.abi_encode(),
				ReentrancyProtection::AllowReentry,
				false,
			)
			.unwrap();
			let consumed = ext.frame_meter().weight_consumed().saturating_sub(before);
			let child: H160 =
				Factory::createCall::abi_decode_returns(&ext.last_frame_output().data)
					.unwrap()
					.0
					.0
					.into();
			let code_hash = AccountInfo::<Test>::load_contract(&child)
				.expect("the factory creates it")
				.code_hash;
			(consumed, hot_entries, child, code_hash)
		})
	};
	let registration_weight = |warmth| {
		<RuntimeCosts as WeightToken<Test>>::weight(&RuntimeCosts::AccessListRegistration(
			WarmthSummary::new(KeyFamily::PalletStorageSlot).add(warmth, StorageOp::Read),
		))
	};
	// Only the registration charge depends on whether the entries are already hot.
	let saving_per_hot_entry = registration_weight(Warmth::new_cold_revertible())
		.saturating_sub(registration_weight(Warmth::new_read_paid()));

	let (all_cold_weight, _, child, stored_code_hash) = create_weight(AlreadyHot::Nothing);
	let (new_contract_hot_weight, new_contract_entries, ..) =
		create_weight(AlreadyHot::NewContract(child));
	assert_eq!(
		new_contract_hot_weight + saving_per_hot_entry.saturating_mul(new_contract_entries.into()),
		all_cold_weight,
	);

	if fixture_type == FixtureType::Solc {
		let (stored_code_hot_weight, stored_code_entries, ..) =
			create_weight(AlreadyHot::StoredCode(stored_code_hash));
		assert_eq!(
			stored_code_hot_weight +
				saving_per_hot_entry.saturating_mul(stored_code_entries.into()),
			all_cold_weight,
		);
	}
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn a_failed_contract_creation_leaves_its_address_cold(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let expected_failed_extra_entries = if fixture_type == FixtureType::Resolc {
			// The caller loads the code by hash in its own frame, so the revert does not drop it.
			CodeLoadItems::new(H256::zero()).entry_count() as usize
		} else {
			0
		};

		let factory = deploy_factory(fixture_type);
		let size_after = |data: Vec<u8>| access_list_metrics_of_call(factory, data).size;

		let noop_size = size_after(Factory::noopCall {}.abi_encode());
		let failed_size = size_after(Factory::createRevertingCall {}.abi_encode());

		assert_eq!(failed_size - noop_size, expected_failed_extra_entries);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn newly_stored_code_is_hot_for_the_next_call(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let call_entries = CallItems::new(H160::zero(), false).entry_count();
		let code_entries = CodeLoadItems::new(H256::zero()).entry_count();
		let (first_expected_hot, second_expected_hot) = if fixture_type == FixtureType::Resolc {
			// Instantiating from a code hash loads the code, which warms it, in both transactions.
			(code_entries + call_entries, code_entries + call_entries)
		} else {
			// Only the first CREATE stores the code, which warms it.
			(code_entries + call_entries, call_entries)
		};

		let factory = deploy_factory(fixture_type);
		let create_then_call = || {
			access_list_metrics_of_call(
				factory,
				Factory::createThenCallCall { value: 0u64.try_into().unwrap() }.abi_encode(),
			)
		};

		let first = create_then_call();
		let second = create_then_call();

		assert_eq!(first.hot, first_expected_hot);
		assert_eq!(second.hot, second_expected_hot);
		assert_eq!(first.cold, second.cold);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn a_created_contract_is_hot_for_the_rest_of_the_transaction(fixture_type: FixtureType) {
	ExtBuilder::default().build().execute_with(|| {
		let (expected_child_call_cold, expected_child_call_hot) =
			if fixture_type == FixtureType::Resolc {
				// Instantiating from a code hash loads the code, which warms it.
				(0, 4) // hot: OriginalAccount, AccountInfo, CodeInfo, PristineCode
			} else {
				// The code is already stored, so CREATE never reads it and it stays cold.
				(2, 2) // cold: CodeInfo, PristineCode; hot: OriginalAccount, AccountInfo
			};
		let expected_transfer_cold = 0; // the create already wrote the deployer's Account
		let expected_transfer_hot = 4; // both parties' Account and AccountInfo

		let factory = deploy_factory(fixture_type);
		let metrics_of = |data: Vec<u8>| access_list_metrics_of_call(factory, data);
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

		assert_eq!(zero_value_call.cold - create_only.cold, expected_child_call_cold);
		assert_eq!(zero_value_call.hot - create_only.hot, expected_child_call_hot);
		assert_eq!(value_call.cold - zero_value_call.cold, expected_transfer_cold);
		assert_eq!(value_call.hot - zero_value_call.hot, expected_transfer_hot);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn a_call_to_a_created_contract_is_charged_hot(fixture_type: FixtureType) {
	let with_new_factory = |run: &dyn Fn(H160) -> Weight| {
		ExtBuilder::default().build().execute_with(|| run(deploy_factory(fixture_type)))
	};
	let invoke_factory =
		|factory_addr: H160, data: Vec<u8>| builder::bare_call(factory_addr).data(data).build();

	let new_factory_call_weight = |data: Vec<u8>| {
		with_new_factory(&|factory_addr| invoke_factory(factory_addr, data.clone()).weight_consumed)
	};

	let create_weight = new_factory_call_weight(Factory::createCall {}.abi_encode());
	let create_then_call_weight = new_factory_call_weight(
		Factory::createThenCallCall { value: 0u64.try_into().unwrap() }.abi_encode(),
	);
	let noop_weight = new_factory_call_weight(Factory::noopCall {}.abi_encode());
	let call_child_weight = with_new_factory(&|factory_addr| {
		let create = invoke_factory(factory_addr, Factory::createCall {}.abi_encode());
		let child = Factory::createCall::abi_decode_returns(&create.result.unwrap().data).unwrap();
		invoke_factory(factory_addr, Factory::callChildCall { child }.abi_encode()).weight_consumed
	});

	let hot_call_weight = create_then_call_weight - create_weight;
	let cold_call_weight = call_child_weight - noop_weight;

	assert!(
		hot_call_weight.all_lt(cold_call_weight),
		"hot={hot_call_weight:?} cold={cold_call_weight:?}",
	);
	assert_eq!(hot_call_weight.proof_size(), 0);

	let hot_bench_saving = <Test as Config>::WeightInfo::seal_call(0, 0, 0)
		.saturating_sub(<Test as Config>::WeightInfo::seal_call_hot());
	assert!(
		cold_call_weight.ref_time() >= hot_call_weight.ref_time() + hot_bench_saving.ref_time(),
		"cold={cold_call_weight:?} hot={hot_call_weight:?} hot_bench_saving={hot_bench_saving:?}",
	);
}

fn deploy_call_each_and_targets<const N: usize>(fixture_type: FixtureType) -> (H160, [H160; N]) {
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

/// Calls `contract` with `data` and [`ExecConfig::first_frame_reads_paid`] set, and returns the
/// call's access-list metrics.
fn access_list_metrics_of_call(contract: H160, data: Vec<u8>) -> AccessListMetrics {
	access_list_metrics_of(|| {
		builder::bare_call(contract)
			.data(data)
			.exec_config(ExecConfig::new_substrate_tx().with_first_frame_reads_paid())
			.build_and_unwrap_result();
	})
}

/// Makes `calls` from `caller` in one transaction and returns its access-list metrics.
fn access_list_metrics_of_calls(caller: H160, calls: &[(H160, Vec<u8>)]) -> AccessListMetrics {
	let targets = calls.iter().map(|(target, _)| target.0.into()).collect();
	let data = calls.iter().map(|(_, data)| data.clone().into()).collect();
	access_list_metrics_of_call(caller, CallEach::callEachCall { targets, data }.abi_encode())
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

/// Compares `call` and `eth_call` with the dry runs that estimate their weight.
mod dry_run {
	use super::*;
	use crate::{
		DispatchError, ExecReturnValue,
		evm::{CreateCallMode, GenericTransaction, TYPE_EIP1559, fees::InfoT},
		runtime_api::{CallInputPayload, dry_run_call},
		test_utils::{BOB_ADDR, deposit_limit},
		tests::sol::eth_extrinsic_revert,
		tracing::{Tracing, trace},
	};
	use frame_support::{
		storage::{TransactionOutcome, with_transaction},
		traits::fungible::Balanced,
	};
	use pretty_assertions::assert_eq;
	use sp_runtime::traits::Dispatchable;
	use test_case::test_case;

	/// Records the weight the contract execution used.
	///
	/// A dispatch reports it with its base weight added, and the eth dry run reports only the peak,
	/// so it is read from the meter instead.
	#[derive(Default)]
	struct WeightTracer(Option<Weight>);

	impl Tracing for WeightTracer {
		fn exit_child_span(&mut self, _output: &ExecReturnValue, _gas_used: u64, weight: Weight) {
			self.0 = Some(weight);
		}

		fn exit_child_span_with_error(
			&mut self,
			_error: DispatchError,
			_gas_used: u64,
			weight: Weight,
		) {
			self.0 = Some(weight);
		}
	}

	/// Runs `call` and rolls back its state changes, returning its result and the weight its
	/// execution used.
	fn execution_weight_of<R>(call: impl FnOnce() -> R) -> (R, Weight) {
		let mut tracer = WeightTracer::default();
		let result = with_transaction(|| {
			TransactionOutcome::Rollback(Ok::<_, DispatchError>(trace(&mut tracer, call)))
		})
		.unwrap();
		(result, tracer.0.expect("a contract call reports its weight when its frame exits"))
	}

	/// A dispatchable that calls a contract, with the value it sends.
	#[derive(Clone, Copy, Debug)]
	enum CallKind {
		/// `call`, estimated through the `call` runtime API.
		Call(BalanceOf<Test>),
		/// `eth_call`, estimated by `dry_run_eth_transact`.
		EthCall(U256),
	}

	/// Dry runs `call` to `dest`, then runs it with the limits the dry run reports.
	/// Returns the dry run's weight, and the call's weight or its error.
	fn dry_run_and_call(
		call: CallKind,
		dest: H160,
		data: Vec<u8>,
	) -> (Weight, Result<Weight, DispatchError>) {
		let (dry_run_weight, (result, call_weight)) = match call {
			CallKind::Call(value) => {
				let (dry_run, dry_run_weight) = execution_weight_of(|| {
					dry_run_call::<Test>(CallInputPayload {
						origin: ALICE,
						dest,
						value,
						gas_limit: None,
						storage_deposit_limit: Some(deposit_limit::<Test>()),
						input_data: data.clone(),
					})
				});
				let weight_limit = dry_run.contract_result.weight_required;
				let storage_deposit_limit =
					dry_run.contract_result.max_storage_deposit.charge_or_zero();
				let call = execution_weight_of(|| {
					builder::call(dest)
						.value(value)
						.weight_limit(weight_limit)
						.storage_deposit_limit(storage_deposit_limit)
						.data(data)
						.build()
				});
				(dry_run_weight, call)
			},
			CallKind::EthCall(value) => {
				let transaction = GenericTransaction {
					from: Some(ALICE_ADDR),
					to: Some(dest),
					value: Some(value),
					input: data.into(),
					..Default::default()
				};
				let (dry_run, dry_run_weight) = execution_weight_of(|| {
					crate::Pallet::<Test>::dry_run_eth_transact(
						transaction.clone(),
						None,
						true,
						None,
					)
				});
				let dry_run = dry_run.expect("the dry run succeeds");
				// `ALICE_ADDR` cannot sign, so the call uses the dry run's transaction length.
				let call_info = GenericTransaction {
					gas: Some(dry_run.eth_gas),
					gas_price: Some(crate::Pallet::<Test>::evm_base_fee()),
					chain_id: Some(<Test as Config>::ChainId::get().into()),
					r#type: Some(TYPE_EIP1559.into()),
					..transaction
				}
				.into_call::<Test>(CreateCallMode::DryRun)
				.expect("the estimated gas covers the transaction's base cost");
				let call = execution_weight_of(|| {
					// A real transaction fills this pot from its gas before `eth_call` runs.
					<Test as Config>::FeeInfo::deposit_txfee(<Test as Config>::Currency::issue(
						call_info.tx_fee + call_info.storage_deposit,
					));
					let origin = RuntimeOrigin::from(crate::Origin::<Test>::EthTransaction(ALICE));
					let result = call_info.call.dispatch(origin);
					match eth_extrinsic_revert() {
						Some(error) => Err(error.into()),
						None => result,
					}
				});
				(dry_run_weight, call)
			},
		};
		(dry_run_weight, result.map(|_| call_weight).map_err(|err| err.error))
	}

	#[test]
	#[ignore = "without first_frame_reads_paid, a bare_call dry run estimates below call"]
	fn a_call_fits_a_bare_call_dry_run_estimate() {
		let (code, _) = compile_module_with_type("SendBack", FixtureType::Solc).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000_000_000);
			let addr =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract().addr;
			let data = pallet_revive_fixtures::SendBack::sendBackCall { amount: 0 }.abi_encode();

			// What `ReviveApi::call` runs.
			let dry_run = builder::bare_call(addr)
				.native_value(1_000)
				.data(data.clone())
				.exec_config(ExecConfig::new_substrate_tx().with_dry_run(None))
				.build();

			// The `call` dispatchable, limited to the dry run's estimate.
			assert_ok!(
				builder::call(addr)
					.value(1_000)
					.weight_limit(dry_run.weight_required)
					.data(data)
					.build()
			);
		});
	}

	#[test_case(FixtureType::Solc)]
	#[test_case(FixtureType::Resolc)]
	fn a_call_pays_what_its_dry_run_estimates(fixture_type: FixtureType) {
		struct TestCase {
			name: &'static str,
			dest: H160,
			value: BalanceOf<Test>,
			data: Vec<u8>,
		}

		let [send_back_code, recurse_code, caller_code, call_each_code, reverter_code] =
			["SendBack", "Recurse", "Caller", "CallEach", "CallThenRevert"]
				.map(|name| compile_module_with_type(name, fixture_type).unwrap().0);
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 1_000_000_000_000);
			let other_caller_contract =
				builder::bare_instantiate(Code::Upload(caller_code.clone()))
					.salt(Some([1; 32]))
					.build_and_unwrap_contract()
					.addr;
			let [send_back_contract, recurse, caller_contract, call_each, reverter] =
				[send_back_code, recurse_code, caller_code, call_each_code, reverter_code].map(
					|code| {
						builder::bare_instantiate(Code::Upload(code))
							.build_and_unwrap_contract()
							.addr
					},
				);

			let _ = crate::Pallet::<Test>::set_evm_balance(
				&send_back_contract,
				100_000_000_000u128.into(),
			);
			let factory = deploy_factory(fixture_type);
			AccountInfo::<Test>::set_delegation(&EVE_ADDR, Some(recurse), &ALICE).unwrap();
			let value_sent: BalanceOf<Test> = 1_000;
			let value_sent_in_wei = crate::Pallet::<Test>::convert_native_to_evm(value_sent);
			let value_with_dust = value_sent_in_wei + U256::one();

			let send_back =
				|amount: U256| SendBack::sendBackCall { amount: amount.as_u64() }.abi_encode();
			let call_back_once =
				Recurse::recurseCall { callsLeft: 1, finalTarget: H160::zero().0.into() }
					.abi_encode();
			let delegate_to_itself = Caller::delegateCall {
				_callee: caller_contract.0.into(),
				_data: Caller::dataCall {}.abi_encode().into(),
				_gas: u64::MAX,
			};
			let callback_from_other = Caller::normalCall {
				_callee: other_caller_contract.0.into(),
				_value: 0,
				_data: Caller::normalCall {
					_callee: caller_contract.0.into(),
					_value: 0,
					_data: Caller::dataCall {}.abi_encode().into(),
					_gas: u64::MAX,
				}
				.abi_encode()
				.into(),
				_gas: u64::MAX,
			};
			let call_back_after_a_reverted_one = CallEach::callEachCall {
				targets: vec![reverter.0.into(), call_each.0.into()],
				data: vec![
					CallThenRevert::callThenRevertCall { target: call_each.0.into() }
						.abi_encode()
						.into(),
					CallEach::callEachCall { targets: vec![], data: vec![] }.abi_encode().into(),
				],
			};
			let test_cases = [
				TestCase {
					name: "a call back into the contract",
					dest: recurse,
					value: 0,
					data: call_back_once.clone(),
				},
				TestCase {
					name: "a call back into an EIP-7702 delegated account",
					dest: EVE_ADDR,
					value: 0,
					data: call_back_once,
				},
				TestCase {
					name: "a call back into the contract after a reverted call back",
					dest: call_each,
					value: 0,
					data: call_back_after_a_reverted_one.abi_encode(),
				},
				TestCase {
					name: "a delegate call into the contract",
					dest: caller_contract,
					value: 0,
					data: delegate_to_itself.abi_encode(),
				},
				TestCase {
					name: "a callback from another contract",
					dest: caller_contract,
					value: 0,
					data: callback_from_other.abi_encode(),
				},
				TestCase {
					name: "a call back to the sender without value",
					dest: send_back_contract,
					value: value_sent,
					data: send_back(U256::zero()),
				},
				TestCase {
					name: "a transfer back to the sender",
					dest: send_back_contract,
					value: value_sent,
					data: send_back(value_sent_in_wei),
				},
				TestCase {
					name: "a dust transfer back to the sender",
					dest: send_back_contract,
					value: value_sent,
					data: send_back(value_with_dust),
				},
				TestCase {
					name: "a withdrawal by the sender",
					dest: send_back_contract,
					value: 0,
					data: send_back(value_sent_in_wei),
				},
				TestCase {
					name: "a payment forwarded to another account",
					dest: send_back_contract,
					value: value_sent,
					data: SendBack::sendToCall {
						to: BOB_ADDR.0.into(),
						amount: value_sent_in_wei.as_u64(),
					}
					.abi_encode(),
				},
				TestCase {
					name: "a contract created and funded with the value sent",
					dest: factory,
					value: value_sent,
					data: Factory::createAndFundCall {}.abi_encode(),
				},
			];

			for TestCase { name, dest, value, data } in test_cases {
				let evm_value = crate::Pallet::<Test>::convert_native_to_evm(value);
				for call in [CallKind::Call(value), CallKind::EthCall(evm_value)] {
					let (dry_run_weight, call_result) = dry_run_and_call(call, dest, data.clone());
					assert_eq!(call_result, Ok(dry_run_weight), "{call:?}: {name}");
				}
			}

			// `call` takes a native value, so only `eth_call` can send dust.
			for (name, data) in [
				("a dust transfer back after a value with dust", send_back(value_with_dust)),
				("a call back without value after a value with dust", send_back(U256::zero())),
			] {
				let call = CallKind::EthCall(value_with_dust);
				let (dry_run_weight, call_result) =
					dry_run_and_call(call, send_back_contract, data);
				assert_eq!(call_result, Ok(dry_run_weight), "{call:?}: {name}");
			}
		});
	}
}
