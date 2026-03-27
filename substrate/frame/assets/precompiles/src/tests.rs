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
	alloy::hex,
	mock::{new_test_ext, Assets, Balances, RuntimeEvent, RuntimeOrigin, System, Test},
	permit,
};
use alloy::primitives::U256;
use frame_support::{
	assert_ok,
	traits::{Currency, Get},
};
use pallet_revive::{precompiles::TransactionLimits, Code, ExecConfig};
use sp_core::H160;
use sp_runtime::Weight;
use test_case::test_case;

const PRECOMPILE_ADDRESS_PREFIX: u16 = 0x0120;
const PRECOMPILE_ADDRESS_PREFIX_FOREIGN: u16 = 0x0220;

fn set_prefix_in_address(prefix: u16) -> [u8; 20] {
	let mut addr = hex::const_decode_to_array(b"0000000000000000000000000000000000000000").unwrap();
	addr[16..18].copy_from_slice(&prefix.to_be_bytes());
	addr
}

fn assert_contract_event(contract: H160, event: IERC20Events) {
	let (topics, data) = event.into_log_data().split();
	let topics = topics.into_iter().map(|v| H256(v.0)).collect::<Vec<_>>();
	System::assert_has_event(RuntimeEvent::Revive(pallet_revive::Event::ContractEmitted {
		contract,
		data: data.to_vec(),
		topics,
	}));
}

fn setup_asset_for_prefix(asset_id: u32, prefix: u16) {
	if prefix == PRECOMPILE_ADDRESS_PREFIX_FOREIGN {
		pallet::Pallet::<Test>::insert_asset_mapping(&asset_id)
			.expect("Failed to insert asset mapping");
	}
}

#[test]
fn asset_id_extractor_works() {
	let address: [u8; 20] =
		hex::const_decode_to_array(b"0000053900000000000000000000000001200000").unwrap();
	assert!(InlineIdConfig::<0x0120>::MATCHER.matches(&address));
	assert_eq!(
		<InlineIdConfig<0x0120> as AssetPrecompileConfig>::AssetIdExtractor::asset_id_from_address(
			&address
		)
		.unwrap(),
		1337u32
	);
}

#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn precompile_transfer_works(asset_index: u16) {
	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));

		let from = 123456789;
		let to = 987654321;

		Balances::make_free_balance_be(&from, 100);
		Balances::make_free_balance_be(&to, 100);

		let from_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&from);
		let to_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&to);
		setup_asset_for_prefix(asset_id, asset_index);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, from, true, 1));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(from), asset_id, from, 100));

		let data =
			IERC20::transferCall { to: to_addr.0.into(), value: U256::from(10) }.abi_encode();

		pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(from),
			asset_addr,
			0u32.into(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: u64::MAX,
			},
			data,
			&ExecConfig::new_substrate_tx(),
		);

		assert_contract_event(
			asset_addr,
			IERC20Events::Transfer(IERC20::Transfer {
				from: from_addr.0.into(),
				to: to_addr.0.into(),
				value: U256::from(10),
			}),
		);

		assert_eq!(Assets::balance(asset_id, from), 90);
		assert_eq!(Assets::balance(asset_id, to), 10);
	});
}

#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn total_supply_works(asset_index: u16) {
	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));

		let owner = 123456789;

		Balances::make_free_balance_be(&owner, 100);
		setup_asset_for_prefix(asset_id, asset_index);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, 1000));

		let data = IERC20::totalSupplyCall {}.abi_encode();

		let data = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(owner),
			asset_addr,
			0u32.into(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: u64::MAX,
			},
			data,
			&ExecConfig::new_substrate_tx(),
		)
		.result
		.unwrap()
		.data;

		let ret = IERC20::totalSupplyCall::abi_decode_returns(&data).unwrap();
		assert_eq!(ret, U256::from(1000));
	});
}

#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn balance_of_works(asset_index: u16) {
	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let owner = 123456789;

		setup_asset_for_prefix(asset_id, asset_index);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, 1000));

		let account = <Test as pallet_revive::Config>::AddressMapper::to_address(&owner).0.into();
		let data = IERC20::balanceOfCall { account }.abi_encode();

		let data = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(owner),
			asset_addr,
			0u32.into(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: u64::MAX,
			},
			data,
			&ExecConfig::new_substrate_tx(),
		)
		.result
		.unwrap()
		.data;

		let ret = IERC20::balanceOfCall::abi_decode_returns(&data).unwrap();
		assert_eq!(ret, U256::from(1000));
	});
}

#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn approval_works(asset_index: u16) {
	use frame_support::traits::fungibles::approvals::Inspect;

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));

		let owner = 123456789;
		let spender = 987654321;
		let other = 1122334455;

		Balances::make_free_balance_be(&owner, 100);
		Balances::make_free_balance_be(&spender, 100);
		Balances::make_free_balance_be(&other, 100);

		let owner_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&owner);
		let spender_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&spender);
		let other_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&other);

		setup_asset_for_prefix(asset_id, asset_index);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, 100));

		let data = IERC20::approveCall { spender: spender_addr.0.into(), value: U256::from(25) }
			.abi_encode();

		pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(owner),
			asset_addr,
			0u32.into(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: u64::MAX,
			},
			data,
			&ExecConfig::new_substrate_tx(),
		);

		assert_contract_event(
			asset_addr,
			IERC20Events::Approval(IERC20::Approval {
				owner: owner_addr.0.into(),
				spender: spender_addr.0.into(),
				value: U256::from(25),
			}),
		);

		let data =
			IERC20::allowanceCall { owner: owner_addr.0.into(), spender: spender_addr.0.into() }
				.abi_encode();

		let data = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(owner),
			asset_addr,
			0u32.into(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: u64::MAX,
			},
			data,
			&ExecConfig::new_substrate_tx(),
		)
		.result
		.unwrap()
		.data;

		let ret = IERC20::allowanceCall::abi_decode_returns(&data).unwrap();
		assert_eq!(ret, U256::from(25));

		let data = IERC20::transferFromCall {
			from: owner_addr.0.into(),
			to: other_addr.0.into(),
			value: U256::from(10),
		}
		.abi_encode();

		pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(spender),
			asset_addr,
			0u32.into(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: u64::MAX,
			},
			data,
			&ExecConfig::new_substrate_tx(),
		);
		assert_eq!(Assets::balance(asset_id, owner), 90);
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), 15);
		assert_eq!(Assets::balance(asset_id, other), 10);

		assert_contract_event(
			asset_addr,
			IERC20Events::Transfer(IERC20::Transfer {
				from: owner_addr.0.into(),
				to: other_addr.0.into(),
				value: U256::from(10),
			}),
		);
	});
}

/// Helper to call approve via the precompile. Returns the bare call result.
fn raw_approve(
	owner: u64,
	asset_addr: H160,
	spender_addr: H160,
	value: U256,
) -> pallet_revive::ContractResult<pallet_revive::ExecReturnValue, u64> {
	let data = IERC20::approveCall { spender: spender_addr.0.into(), value }.abi_encode();
	pallet_revive::Pallet::<Test>::bare_call(
		RuntimeOrigin::signed(owner),
		asset_addr,
		0u32.into(),
		TransactionLimits::WeightAndDeposit { weight_limit: Weight::MAX, deposit_limit: u64::MAX },
		data,
		&ExecConfig::new_substrate_tx(),
	)
}

/// Helper to call approve via the precompile, asserting success.
fn call_approve(owner: u64, asset_addr: H160, spender_addr: H160, value: U256) {
	let result = raw_approve(owner, asset_addr, spender_addr, value);
	assert!(result.result.is_ok(), "approve precompile call failed: {:?}", result);
	assert!(!result.result.unwrap().did_revert(), "approve call reverted");
}

#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn approve_set_and_revoke(asset_index: u16) {
	use frame_support::traits::fungibles::approvals::Inspect;

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));

		let owner = 123456789u64;
		let spender = 987654321u64;

		Balances::make_free_balance_be(&owner, 100);
		Balances::make_free_balance_be(&spender, 100);

		let spender_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&spender);

		setup_asset_for_prefix(asset_id, asset_index);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, 100));

		let deposit: u64 = <Test as pallet_assets::Config>::ApprovalDeposit::get();
		assert_eq!(Balances::reserved_balance(&owner), 0);

		// First approve: set allowance to 100 (from zero — allowed).
		call_approve(owner, asset_addr, spender_addr, U256::from(100));
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), 100);
		assert_eq!(Balances::reserved_balance(&owner), deposit);

		// Approve to 0: must revoke the allowance entirely and unreserve the deposit.
		call_approve(owner, asset_addr, spender_addr, U256::from(0));
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), 0);
		assert_eq!(Balances::reserved_balance(&owner), 0);

		// Re-approve to 50 after zeroing — allowed, deposit reserved again.
		call_approve(owner, asset_addr, spender_addr, U256::from(50));
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), 50);
		assert_eq!(Balances::reserved_balance(&owner), deposit);
	});
}

/// After a partial `transferFrom`, the allowance is reduced but the storage entry (with its
/// deposit) remains. Revoking via `approve(spender, 0)` must remove that entry and unreserve
/// the deposit — not just zero the amount. This matters because the precompile's cancel path
/// directly removes the `Approvals` entry; if it only checked the allowance amount it could
/// leave a dangling entry with a locked deposit.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn approve_revoke_after_partial_transfer(asset_index: u16) {
	use frame_support::traits::fungibles::approvals::Inspect;

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));

		let owner = 123456789u64;
		let spender = 987654321u64;
		let dest = 1122334455u64;

		Balances::make_free_balance_be(&owner, 100);
		Balances::make_free_balance_be(&spender, 100);
		Balances::make_free_balance_be(&dest, 100);

		let spender_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&spender);

		setup_asset_for_prefix(asset_id, asset_index);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, 100));

		let deposit: u64 = <Test as pallet_assets::Config>::ApprovalDeposit::get();

		// Approve 100.
		call_approve(owner, asset_addr, spender_addr, U256::from(100));
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), 100);
		assert_eq!(Balances::reserved_balance(&owner), deposit);

		// Spender uses 60 via transfer_approved, leaving 40 remaining.
		assert_ok!(Assets::transfer_approved(
			RuntimeOrigin::signed(spender),
			asset_id,
			owner,
			dest,
			60
		));
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), 40);
		// Deposit is still held — the approval entry still exists.
		assert_eq!(Balances::reserved_balance(&owner), deposit);

		// Revoke the remaining allowance via approve(0).
		call_approve(owner, asset_addr, spender_addr, U256::from(0));
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), 0);
		// Deposit must be unreserved and entry removed.
		assert_eq!(Balances::reserved_balance(&owner), 0);
	});
}

#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn approve_revoke_rejected_on_frozen_asset(asset_index: u16) {
	use frame_support::traits::fungibles::approvals::Inspect;

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));

		let owner = 123456789u64;
		let spender = 987654321u64;

		Balances::make_free_balance_be(&owner, 100);
		Balances::make_free_balance_be(&spender, 100);

		let spender_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&spender);

		setup_asset_for_prefix(asset_id, asset_index);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, 100));

		let deposit: u64 = <Test as pallet_assets::Config>::ApprovalDeposit::get();

		// Approve 100 while the asset is live.
		call_approve(owner, asset_addr, spender_addr, U256::from(100));
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), 100);
		assert_eq!(Balances::reserved_balance(&owner), deposit);

		// Freeze the asset.
		assert_ok!(Assets::freeze_asset(RuntimeOrigin::signed(owner), asset_id));

		// Revoking via approve(0) must fail — asset is not live.
		let result = raw_approve(owner, asset_addr, spender_addr, U256::from(0));
		let reverted = result.result.as_ref().map_or(true, |v| v.did_revert());
		assert!(reverted, "revoke on frozen asset should be rejected");

		// Allowance and deposit must remain unchanged.
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), 100);
		assert_eq!(Balances::reserved_balance(&owner), deposit);
	});
}

/// Directly overwriting a non-zero allowance with a different non-zero value must use set
/// semantics (cancel + re-approve). The allowance must equal the new value — not the sum of
/// old and new — and only a single deposit should be reserved.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn approve_nonzero_to_nonzero(asset_index: u16) {
	use frame_support::traits::fungibles::approvals::Inspect;

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));

		let owner = 123456789u64;
		let spender = 987654321u64;

		Balances::make_free_balance_be(&owner, 100);
		Balances::make_free_balance_be(&spender, 100);

		let spender_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&spender);

		setup_asset_for_prefix(asset_id, asset_index);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, 100));

		let deposit: u64 = <Test as pallet_assets::Config>::ApprovalDeposit::get();

		// Approve 100 (0 → 100).
		call_approve(owner, asset_addr, spender_addr, U256::from(100));
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), 100);
		assert_eq!(Balances::reserved_balance(&owner), deposit);

		// Overwrite with 50 directly (100 → 50), no zeroing in between.
		call_approve(owner, asset_addr, spender_addr, U256::from(50));
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), 50);
		// Deposit reserved exactly once — cancel unreserved the old one, approve re-reserved.
		assert_eq!(Balances::reserved_balance(&owner), deposit);

		// Overwrite upward (50 → 200) to confirm it works in both directions.
		call_approve(owner, asset_addr, spender_addr, U256::from(200));
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), 200);
		assert_eq!(Balances::reserved_balance(&owner), deposit);
	});
}

#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn approve_zero_on_nonexistent_is_noop(asset_index: u16) {
	use frame_support::traits::fungibles::approvals::Inspect;

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));

		let owner = 123456789u64;
		let spender = 987654321u64;

		Balances::make_free_balance_be(&owner, 100);
		Balances::make_free_balance_be(&spender, 100);

		let spender_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&spender);

		setup_asset_for_prefix(asset_id, asset_index);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, 100));

		// Setting zero when no approval exists should succeed silently.
		call_approve(owner, asset_addr, spender_addr, U256::from(0));
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), 0);
		assert_eq!(Balances::reserved_balance(&owner), 0);
	});
}

alloy::sol! {
	interface ICaller {
		function staticCall(address callee, bytes data, uint64 gas) external view returns (bool success, bytes output);
	}
}

/// Tests that DOMAIN_SEPARATOR succeeds when invoked via STATICCALL (`is_read_only = true`).
///
/// This guards against regressions where a storage write is accidentally introduced into
/// `domain_separator()` (e.g. a lazy-init inside `pallet_assets::name()`), which would
/// cause the call to fail under STATICCALL silently without this test.
///
/// The test deploys the `Caller` fixture contract which uses the `STATICCALL` opcode to
/// forward the `DOMAIN_SEPARATOR()` selector to the precompile, then verifies the
/// returned value matches the expected separator.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn domain_separator_is_staticcall_compatible(asset_index: u16) {
	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let deployer = 555u64;

		// Provide enough balance to cover the EVM contract storage deposit.
		Balances::make_free_balance_be(&deployer, 1_000_000_000_000_000u64);

		// Create asset and set a name so domain separator is non-trivial.
		setup_asset_for_prefix(asset_id, asset_index);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, deployer, true, 1));
		assert_ok!(Assets::force_set_metadata(
			RuntimeOrigin::root(),
			asset_id,
			b"Static Token".to_vec(),
			b"STK".to_vec(),
			18,
			false,
		));

		// Deploy the Caller fixture contract.
		let (init_code, _) = pallet_revive_fixtures::compile_module_with_type(
			"Caller",
			pallet_revive_fixtures::FixtureType::Solc,
		)
		.expect("Caller fixture must be compiled");
		let caller_addr = pallet_revive::Pallet::<Test>::bare_instantiate(
			RuntimeOrigin::signed(deployer),
			0u32.into(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: u64::MAX,
			},
			Code::Upload(init_code),
			vec![],
			None,
			&ExecConfig::new_substrate_tx(),
		)
		.result
		.expect("Caller deployment must succeed")
		.addr;

		// Call Caller.staticCall(asset_addr, DOMAIN_SEPARATOR_selector, gas).
		let domain_sep_calldata = IERC20::DOMAIN_SEPARATORCall {}.abi_encode();
		let calldata = ICaller::staticCallCall {
			callee: alloy::primitives::Address::from(asset_addr.0),
			data: domain_sep_calldata.into(),
			gas: u64::MAX,
		}
		.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(deployer),
			caller_addr,
			0u32.into(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: u64::MAX,
			},
			calldata,
			&ExecConfig::new_substrate_tx(),
		)
		.result
		.expect("call to Caller.staticCall must succeed")
		.data;

		let ret = ICaller::staticCallCall::abi_decode_returns(&result)
			.expect("return must decode as (bool, bytes)");
		assert!(ret.success, "STATICCALL to DOMAIN_SEPARATOR must succeed (view-safe function)");

		let expected =
			permit::Pallet::<Test>::compute_domain_separator(&asset_addr, b"Static Token");
		assert_eq!(
			&ret.output[..],
			expected.as_bytes(),
			"domain separator returned via STATICCALL must match direct computation"
		);
	});
}
