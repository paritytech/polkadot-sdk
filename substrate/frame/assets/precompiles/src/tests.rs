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

// Regression test: `deposit_event` in lib.rs must pass `data.len()` (32 bytes for
// every ERC-20 event emitted by this precompile) — not `topics.len()` (always 3) —
// to the `len` field of `RuntimeCosts::DepositEvent`. The two are independent
// arguments with different per-unit weights, so swapping them silently undercharges
// the per-byte event cost on every Transfer/Approval.
//
// A bare-call `transfer` charges exactly `WeightInfo::transfer() + DepositEvent`,
// so we can assert the consumed weight against that sum. With the bug, the actual
// consumed weight is lower by `DepositEvent{len:32} - DepositEvent{len:3}` and the
// equality fails.
#[test]
fn deposit_event_charges_data_byte_length() {
	use pallet_revive::precompiles::Token;

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(PRECOMPILE_ADDRESS_PREFIX));
		let from = 123456789;
		let to = 987654321;
		Balances::make_free_balance_be(&from, 100);
		Balances::make_free_balance_be(&to, 100);
		let to_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&to);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, from, true, 1));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(from), asset_id, from, 100));

		let data =
			IERC20::transferCall { to: to_addr.0.into(), value: U256::from(10) }.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
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
		assert!(result.result.is_ok(), "transfer call failed: {:?}", result.result);

		let expected =
			<() as pallet_assets::WeightInfo>::transfer().saturating_add(<RuntimeCosts as Token<
				Test,
			>>::weight(
				&RuntimeCosts::DepositEvent { num_topic: 3, len: 32 },
			));
		assert_eq!(
			result.weight_consumed, expected,
			"transfer weight does not match WeightInfo::transfer() + \
			 DepositEvent{{num_topic: 3, len: 32}} — deposit_event has likely \
			 regressed to charging len=topics.len() instead of len=data.len()",
		);
	});
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

/// `transfer` and `transferFrom` must NOT saturate on `value > Balance::MAX` —
/// unlike `approve`/`permit`, transfers move exact amounts and silently clamping
/// would produce partial transfers the caller never asked for. This pins the
/// asymmetry so a future contributor "fixing" the inconsistency doesn't quietly
/// flip transfers to saturation.
#[test]
fn transfer_and_transfer_from_revert_on_overflow_no_saturation() {
	use alloy::sol_types::{Revert, SolError};

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(PRECOMPILE_ADDRESS_PREFIX));
		let from = 123456789;
		let to = 987654321;
		Balances::make_free_balance_be(&from, 100);
		Balances::make_free_balance_be(&to, 100);
		let from_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&from);
		let to_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&to);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, from, true, 1));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(from), asset_id, from, 100));

		// Authorise the spender for `transferFrom` — with a tiny allowance, since
		// the test never reaches the allowance check (the conversion reverts first).
		let data =
			IERC20::approveCall { spender: to_addr.0.into(), value: U256::from(50) }.abi_encode();
		assert!(pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(from),
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
		.flags
		.is_empty());

		let assert_reverts_with = |caller: u64, data: Vec<u8>, label: &str| {
			let exec = pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(caller),
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
			.expect("must not trap");
			assert!(exec.did_revert(), "{label} must revert on overflow");
			let decoded = Revert::abi_decode(&exec.data).expect("Error(string) revert");
			assert_eq!(
				decoded.reason, "Balance conversion failed",
				"{label} must revert at the U256→Balance boundary, not deeper"
			);
		};

		let transfer_data =
			IERC20::transferCall { to: to_addr.0.into(), value: U256::MAX }.abi_encode();
		assert_reverts_with(from, transfer_data, "transfer(uint256.max)");

		let transfer_from_data = IERC20::transferFromCall {
			from: from_addr.0.into(),
			to: to_addr.0.into(),
			value: U256::MAX,
		}
		.abi_encode();
		assert_reverts_with(to, transfer_from_data, "transferFrom(_, _, uint256.max)");

		// Balances unchanged — nothing moved.
		assert_eq!(Assets::balance(asset_id, from), 100);
		assert_eq!(Assets::balance(asset_id, to), 0);
	});
}

/// OpenZeppelin "infinite allowance" sentinel: when the stored allowance is
/// at `Balance::MAX`, `transferFrom` must not decrement it. Without this,
/// `approve(uint256.max)` would store `Balance::MAX` (via saturation) and
/// then chip away per-transfer — Uniswap / MetaMask would eventually hit
/// `Unapproved` even though the user thought their approval was permanent.
#[test]
fn transfer_from_does_not_decrement_max_allowance() {
	use frame_support::traits::fungibles::approvals::Inspect;

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(PRECOMPILE_ADDRESS_PREFIX));
		let owner = 123456789;
		let spender = 987654321;
		let recipient = 111222333;
		Balances::make_free_balance_be(&owner, 100);
		Balances::make_free_balance_be(&spender, 100);
		Balances::make_free_balance_be(&recipient, 100);

		let owner_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&owner);
		let spender_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&spender);
		let recipient_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&recipient);

		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, 100));

		// Saturating approve via the canonical uint256.max idiom.
		let data =
			IERC20::approveCall { spender: spender_addr.0.into(), value: U256::MAX }.abi_encode();
		let result = pallet_revive::Pallet::<Test>::bare_call(
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
		assert!(!result.result.unwrap().did_revert(), "approve(uint256.max) must succeed");
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), u64::MAX);

		// Spend a few times. Allowance must stay at MAX after every call.
		for i in 1..=3u64 {
			let data = IERC20::transferFromCall {
				from: owner_addr.0.into(),
				to: recipient_addr.0.into(),
				value: U256::from(10u64),
			}
			.abi_encode();
			let result = pallet_revive::Pallet::<Test>::bare_call(
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
			assert!(
				!result.result.unwrap().did_revert(),
				"transferFrom #{i} must succeed under sentinel allowance"
			);
			assert_eq!(
				Assets::allowance(asset_id, &owner, &spender),
				u64::MAX,
				"sentinel allowance must not decrement after transferFrom #{i}"
			);
			assert_eq!(Assets::balance(asset_id, &recipient), (10 * i));
		}
		assert_eq!(Assets::balance(asset_id, &owner), 100 - 30);
	});
}

/// Pins the common (non-sentinel) `transfer_from` path: with no approval row,
/// the call must revert with `Unapproved`. Guards against the sentinel-bypass
/// refactor accidentally letting unauthorised callers transfer (e.g. if a
/// future change confused "no approval" with "sentinel approval").
#[test]
fn transfer_from_reverts_with_unapproved_when_no_allowance() {
	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(PRECOMPILE_ADDRESS_PREFIX));
		let owner = 123456789;
		let spender = 987654321;
		let recipient = 111222333;
		Balances::make_free_balance_be(&owner, 100);
		Balances::make_free_balance_be(&spender, 100);
		Balances::make_free_balance_be(&recipient, 100);

		let owner_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&owner);
		let recipient_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&recipient);

		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, 100));

		// No approve call: spender has zero allowance, no approval row in storage.
		let data = IERC20::transferFromCall {
			from: owner_addr.0.into(),
			to: recipient_addr.0.into(),
			value: U256::from(10u64),
		}
		.abi_encode();
		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(spender),
			asset_addr,
			0u32.into(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: u64::MAX,
			},
			data,
			&ExecConfig::new_substrate_tx(),
		)
		.result;
		// `do_transfer_approved` returns the pallet's `Unapproved` DispatchError,
		// which surfaces as a trap (Module error), not a Solidity-style revert.
		// Either path means the call failed; pin that it carries the right reason.
		let err = result.expect_err("transferFrom must fail with no allowance");
		assert!(format!("{err:?}").contains("Unapproved"), "unexpected failure reason: {err:?}",);

		// Balances unchanged.
		assert_eq!(Assets::balance(asset_id, &owner), 100);
		assert_eq!(Assets::balance(asset_id, &recipient), 0);
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
		function delegate(address callee, bytes data, uint64 gas) external returns (bool success, bytes output);
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

#[test]
fn delegatecall_is_rejected() {
	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(PRECOMPILE_ADDRESS_PREFIX));
		let deployer = 123456789u64;
		Balances::make_free_balance_be(&deployer, 1_000_000_000_000_000u64);

		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, deployer, true, 1));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(deployer), asset_id, deployer, 1000));

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

		let calldata = ICaller::delegateCall {
			callee: alloy::primitives::Address::from(asset_addr.0),
			data: IERC20::totalSupplyCall {}.abi_encode().into(),
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
		.expect("outer call must succeed");

		let ret = ICaller::delegateCall::abi_decode_returns(&result.data)
			.expect("return must decode as (bool, bytes)");
		assert!(!ret.success, "DELEGATECALL to asset precompile must be rejected");
	});
}

/// `approve(spender, type(uint256).max)` is the universal "infinite allowance"
/// idiom in EVM tooling. The U256 max value doesn't fit in the runtime's
/// `Balance` (`u64` in the mock); the precompile must saturate at
/// `Balance::MAX` rather than revert at the conversion.
///
/// Pins both halves of the contract:
///   1. The call itself succeeds (no trap, no revert).
///   2. The on-chain allowance reads back as `Balance::MAX`, both via direct pallet-state read and
///      via the precompile's `allowance()` selector.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn approve_saturates_on_uint256_max(asset_index: u16) {
	use frame_support::traits::fungibles::approvals::Inspect;

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));

		let owner = 123456789;
		let spender = 987654321;
		Balances::make_free_balance_be(&owner, 100);

		let owner_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&owner);
		let spender_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&spender);

		setup_asset_for_prefix(asset_id, asset_index);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));

		let data =
			IERC20::approveCall { spender: spender_addr.0.into(), value: U256::MAX }.abi_encode();
		let result = pallet_revive::Pallet::<Test>::bare_call(
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
		assert!(result.result.is_ok(), "approve(uint256.max) must not trap: {:?}", result.result);
		assert!(
			!result.result.expect("checked above").did_revert(),
			"approve(uint256.max) must not revert"
		);

		// The Approval event must carry the raw `call.value` (here, `U256::MAX`),
		// not the saturated stored allowance — matches OZ convention and
		// preserves the `value == uint256.max` "Unlimited approval" sentinel
		// that wallets and indexers recognize. See `approve`'s doc-comment.
		assert_contract_event(
			asset_addr,
			IERC20Events::Approval(IERC20::Approval {
				owner: owner_addr.0.into(),
				spender: spender_addr.0.into(),
				value: U256::MAX,
			}),
		);

		// Direct pallet read: allowance must be saturated to Balance::MAX.
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), u64::MAX);

		// Read-back via the precompile must ABI-encode `Balance::MAX` lifted to U256
		// (this is NOT `U256::MAX` — the chain's max balance is smaller).
		let data =
			IERC20::allowanceCall { owner: owner_addr.0.into(), spender: spender_addr.0.into() }
				.abi_encode();
		let bytes = pallet_revive::Pallet::<Test>::bare_call(
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
		.expect("allowance() must succeed")
		.data;
		let ret = IERC20::allowanceCall::abi_decode_returns(&bytes).unwrap();
		assert_eq!(ret, U256::from(u64::MAX));
	});
}

/// Boundary test: saturation must trigger for *any* `U256 > Balance::MAX`, not
/// only the exact `U256::MAX` sentinel. Guards against a regression that would
/// scope saturation to `call.value == U256::MAX`, which would create a sharp
/// edge for routers that compute "infinite allowance" as `U256::MAX - 1` or
/// other large near-max sentinels.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn approve_saturates_just_above_balance_max(asset_index: u16) {
	use frame_support::traits::fungibles::approvals::Inspect;

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));

		let owner = 123456789;
		let spender = 987654321;
		Balances::make_free_balance_be(&owner, 100);

		setup_asset_for_prefix(asset_id, asset_index);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));

		// Smallest U256 that doesn't fit in the mock's `Balance` (u64).
		let just_over: U256 = U256::from(u64::MAX) + U256::from(1u64);
		let spender_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&spender);
		let data =
			IERC20::approveCall { spender: spender_addr.0.into(), value: just_over }.abi_encode();
		let result = pallet_revive::Pallet::<Test>::bare_call(
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
		assert!(
			result.result.is_ok(),
			"approve(Balance::MAX + 1) must not trap: {:?}",
			result.result
		);
		assert!(
			!result.result.expect("checked above").did_revert(),
			"approve(Balance::MAX + 1) must not revert"
		);

		assert_eq!(Assets::allowance(asset_id, &owner, &spender), u64::MAX);

		// Event carries the raw `call.value`, even though storage saturated.
		assert_contract_event(
			asset_addr,
			IERC20Events::Approval(IERC20::Approval {
				owner: <Test as pallet_revive::Config>::AddressMapper::to_address(&owner).0.into(),
				spender: spender_addr.0.into(),
				value: just_over,
			}),
		);
	});
}

/// Cancel-then-overwrite path with saturation. `approve_saturates_on_uint256_max`
/// only exercises the `current == 0` branch; this pins the other branch where
/// an existing non-zero allowance is cancelled before the saturated re-approval
/// is written, plus the worst-case weight refund logic that branch carries.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn approve_saturates_when_overwriting_existing_allowance(asset_index: u16) {
	use frame_support::traits::fungibles::approvals::Inspect;

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let owner = 123456789;
		let spender = 987654321;
		Balances::make_free_balance_be(&owner, 100);
		let spender_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&spender);

		setup_asset_for_prefix(asset_id, asset_index);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));

		// Establish a non-zero allowance first.
		call_approve(owner, asset_addr, spender_addr, U256::from(25));
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), 25);

		// Overwrite with uint256.max. Goes through the cancel-first branch and
		// must still saturate.
		call_approve(owner, asset_addr, spender_addr, U256::MAX);
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), u64::MAX);

		assert_contract_event(
			asset_addr,
			IERC20Events::Approval(IERC20::Approval {
				owner: <Test as pallet_revive::Config>::AddressMapper::to_address(&owner).0.into(),
				spender: spender_addr.0.into(),
				value: U256::MAX,
			}),
		);
	});
}

/// `permit(spender, type(uint256).max, …)` is the gasless infinite-allowance
/// pathway — the entire reason `permit()` exists in the EIP-2612 surface for
/// wallet/router integrations. It must saturate the *stored* allowance at
/// `Balance::MAX` rather than revert at the `U256 → Balance` conversion,
/// matching `approve`'s behaviour.
///
/// Also pins the event policy: `Approval.value` carries the raw signed
/// `call.value` (`U256::MAX`), matching ERC-20 / OZ convention so EVM tooling
/// (wallets, indexers) keeps recognizing the "Unlimited approval" sentinel.
#[test]
fn permit_saturates_on_uint256_max() {
	use frame_support::traits::fungibles::approvals::Inspect;
	use sp_core::{ecdsa, Pair as _};

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(PRECOMPILE_ADDRESS_PREFIX));

		// Hardhat Account 0 private key — well-known test key, public on the
		// internet, never use for anything other than tests.
		let secret: [u8; 32] = hex::const_decode_to_array(
			b"ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
		)
		.unwrap();
		let pair = ecdsa::Pair::from_seed_slice(&secret).expect("valid secret");

		// Derive owner H160 by signing an arbitrary digest and recovering the
		// uncompressed pubkey — sidesteps decompressing `pair.public()` by hand.
		let dummy = [0u8; 32];
		let dummy_sig = pair.sign_prehashed(&dummy).0;
		let pubkey = sp_io::crypto::secp256k1_ecdsa_recover(&dummy_sig, &dummy)
			.ok()
			.expect("recover from valid signature");
		let owner_h160 = H160::from_slice(&sp_io::hashing::keccak_256(&pubkey)[12..]);

		let spender_h160 = H160::from_low_u64_be(0x9876_5432);
		let owner_account =
			<Test as pallet_revive::Config>::AddressMapper::to_account_id(&owner_h160);
		let spender_account =
			<Test as pallet_revive::Config>::AddressMapper::to_account_id(&spender_h160);

		// The relayer / caller is some third party — permit's whole point is
		// that the signature authorises the change, not the caller.
		let relayer: u64 = 555_555;
		Balances::make_free_balance_be(&owner_account, 100);
		Balances::make_free_balance_be(&relayer, 1_000_000);

		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner_account, true, 1));

		// Build the EIP-712 digest. `pallet_assets::name` returns the default
		// empty vec when no metadata row exists, which is what the precompile
		// passes to `use_permit` — match that here.
		let value_bytes: [u8; 32] = U256::MAX.to_be_bytes();
		let deadline_value = U256::from(u64::MAX);
		let deadline_bytes: [u8; 32] = deadline_value.to_be_bytes();
		let nonce = sp_core::U256::zero();
		let asset_name = pallet_assets::Pallet::<Test>::name(asset_id);

		let digest = permit::Pallet::<Test>::permit_digest(
			&asset_addr,
			&asset_name,
			&owner_h160,
			&spender_h160,
			&value_bytes,
			&nonce,
			&deadline_bytes,
		);

		// Sign. `sign_prehashed` returns [r(32) || s(32) || recovery_id(1)];
		// libsecp256k1 produces low-s by default so no malleability fixup needed.
		let sig = pair.sign_prehashed(&digest).0;
		let mut r = [0u8; 32];
		let mut s = [0u8; 32];
		r.copy_from_slice(&sig[0..32]);
		s.copy_from_slice(&sig[32..64]);
		let v = sig[64] + 27; // Ethereum convention: recovery_id ∈ {0,1} → v ∈ {27,28}

		let data = IERC20::permitCall {
			owner: owner_h160.0.into(),
			spender: spender_h160.0.into(),
			value: U256::MAX,
			deadline: deadline_value,
			v,
			r: r.into(),
			s: s.into(),
		}
		.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(relayer),
			asset_addr,
			0u32.into(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: u64::MAX,
			},
			data,
			&ExecConfig::new_substrate_tx(),
		);
		assert!(result.result.is_ok(), "permit(uint256.max) must not trap: {:?}", result.result);
		assert!(
			!result.result.expect("checked above").did_revert(),
			"permit(uint256.max) must not revert"
		);

		// Allowance must be saturated to Balance::MAX (u64 in the mock).
		assert_eq!(Assets::allowance(asset_id, &owner_account, &spender_account), u64::MAX);

		// Event carries the raw signed `value` (U256::MAX), not the saturated
		// stored allowance — same policy as `approve`.
		assert_contract_event(
			asset_addr,
			IERC20Events::Approval(IERC20::Approval {
				owner: owner_h160.0.into(),
				spender: spender_h160.0.into(),
				value: U256::MAX,
			}),
		);
	});
}

/// Cancel-then-overwrite path for `permit`. The existing
/// `permit_saturates_on_uint256_max` test only exercises the `current == 0`
/// branch; this pins the cancel-first branch inside the `with_transaction`
/// block, which has the most state mutations (cancel → approve → emit) and
/// the highest regression risk.
#[test]
fn permit_saturates_when_overwriting_existing_allowance() {
	use frame_support::traits::fungibles::approvals::Inspect;
	use sp_core::{ecdsa, Pair as _};

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(PRECOMPILE_ADDRESS_PREFIX));

		let secret: [u8; 32] = hex::const_decode_to_array(
			b"ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
		)
		.unwrap();
		let pair = ecdsa::Pair::from_seed_slice(&secret).expect("valid secret");

		let dummy = [0u8; 32];
		let dummy_sig = pair.sign_prehashed(&dummy).0;
		let pubkey = sp_io::crypto::secp256k1_ecdsa_recover(&dummy_sig, &dummy)
			.ok()
			.expect("recover from valid signature");
		let owner_h160 = H160::from_slice(&sp_io::hashing::keccak_256(&pubkey)[12..]);

		let spender_h160 = H160::from_low_u64_be(0x9876_5432);
		let owner_account =
			<Test as pallet_revive::Config>::AddressMapper::to_account_id(&owner_h160);
		let spender_account =
			<Test as pallet_revive::Config>::AddressMapper::to_account_id(&spender_h160);

		let relayer: u64 = 555_555;
		Balances::make_free_balance_be(&owner_account, 100);
		Balances::make_free_balance_be(&relayer, 1_000_000);

		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner_account, true, 1));

		// Seed a non-zero allowance so the permit goes through the cancel-first
		// branch instead of the fresh-approve branch.
		assert_ok!(Assets::approve_transfer(
			RuntimeOrigin::signed(owner_account),
			asset_id.into(),
			spender_account,
			25,
		));
		assert_eq!(Assets::allowance(asset_id, &owner_account, &spender_account), 25);

		// Sign a permit for U256::MAX with nonce 0 (still the first permit).
		let value_bytes: [u8; 32] = U256::MAX.to_be_bytes();
		let deadline_value = U256::from(u64::MAX);
		let deadline_bytes: [u8; 32] = deadline_value.to_be_bytes();
		let nonce = sp_core::U256::zero();
		let asset_name = pallet_assets::Pallet::<Test>::name(asset_id);

		let digest = permit::Pallet::<Test>::permit_digest(
			&asset_addr,
			&asset_name,
			&owner_h160,
			&spender_h160,
			&value_bytes,
			&nonce,
			&deadline_bytes,
		);
		let sig = pair.sign_prehashed(&digest).0;
		let mut r = [0u8; 32];
		let mut s = [0u8; 32];
		r.copy_from_slice(&sig[0..32]);
		s.copy_from_slice(&sig[32..64]);
		let v = sig[64] + 27;

		let data = IERC20::permitCall {
			owner: owner_h160.0.into(),
			spender: spender_h160.0.into(),
			value: U256::MAX,
			deadline: deadline_value,
			v,
			r: r.into(),
			s: s.into(),
		}
		.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(relayer),
			asset_addr,
			0u32.into(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: u64::MAX,
			},
			data,
			&ExecConfig::new_substrate_tx(),
		);
		assert!(result.result.is_ok(), "permit must not trap: {:?}", result.result);
		assert!(!result.result.expect("checked above").did_revert(), "permit must not revert");

		// Cancel-first branch worked: old allowance (25) was cancelled and the
		// new one was set, saturated to Balance::MAX.
		assert_eq!(Assets::allowance(asset_id, &owner_account, &spender_account), u64::MAX);

		assert_contract_event(
			asset_addr,
			IERC20Events::Approval(IERC20::Approval {
				owner: owner_h160.0.into(),
				spender: spender_h160.0.into(),
				value: U256::MAX,
			}),
		);
	});
}

/// Boundary test mirroring `approve_saturates_just_above_balance_max`:
/// saturation must trigger for any `U256 > Balance::MAX`, not only the exact
/// `U256::MAX` sentinel. Guards against a regression that would scope
/// `permit`'s saturation to `call.value == U256::MAX`.
#[test]
fn permit_saturates_just_above_balance_max() {
	use frame_support::traits::fungibles::approvals::Inspect;
	use sp_core::{ecdsa, Pair as _};

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(PRECOMPILE_ADDRESS_PREFIX));

		let secret: [u8; 32] = hex::const_decode_to_array(
			b"ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
		)
		.unwrap();
		let pair = ecdsa::Pair::from_seed_slice(&secret).expect("valid secret");

		let dummy = [0u8; 32];
		let dummy_sig = pair.sign_prehashed(&dummy).0;
		let pubkey = sp_io::crypto::secp256k1_ecdsa_recover(&dummy_sig, &dummy)
			.ok()
			.expect("recover from valid signature");
		let owner_h160 = H160::from_slice(&sp_io::hashing::keccak_256(&pubkey)[12..]);

		let spender_h160 = H160::from_low_u64_be(0x9876_5432);
		let owner_account =
			<Test as pallet_revive::Config>::AddressMapper::to_account_id(&owner_h160);
		let spender_account =
			<Test as pallet_revive::Config>::AddressMapper::to_account_id(&spender_h160);

		let relayer: u64 = 555_555;
		Balances::make_free_balance_be(&owner_account, 100);
		Balances::make_free_balance_be(&relayer, 1_000_000);

		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner_account, true, 1));

		// Smallest U256 that doesn't fit in the mock's `Balance` (u64).
		let just_over: U256 = U256::from(u64::MAX) + U256::from(1u64);
		let value_bytes: [u8; 32] = just_over.to_be_bytes();
		let deadline_value = U256::from(u64::MAX);
		let deadline_bytes: [u8; 32] = deadline_value.to_be_bytes();
		let nonce = sp_core::U256::zero();
		let asset_name = pallet_assets::Pallet::<Test>::name(asset_id);

		let digest = permit::Pallet::<Test>::permit_digest(
			&asset_addr,
			&asset_name,
			&owner_h160,
			&spender_h160,
			&value_bytes,
			&nonce,
			&deadline_bytes,
		);
		let sig = pair.sign_prehashed(&digest).0;
		let mut r = [0u8; 32];
		let mut s = [0u8; 32];
		r.copy_from_slice(&sig[0..32]);
		s.copy_from_slice(&sig[32..64]);
		let v = sig[64] + 27;

		let data = IERC20::permitCall {
			owner: owner_h160.0.into(),
			spender: spender_h160.0.into(),
			value: just_over,
			deadline: deadline_value,
			v,
			r: r.into(),
			s: s.into(),
		}
		.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(relayer),
			asset_addr,
			0u32.into(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: u64::MAX,
			},
			data,
			&ExecConfig::new_substrate_tx(),
		);
		assert!(
			result.result.is_ok(),
			"permit(Balance::MAX + 1) must not trap: {:?}",
			result.result
		);
		assert!(
			!result.result.expect("checked above").did_revert(),
			"permit(Balance::MAX + 1) must not revert"
		);

		assert_eq!(Assets::allowance(asset_id, &owner_account, &spender_account), u64::MAX);

		// Event carries the raw signed value, not the saturated storage value.
		assert_contract_event(
			asset_addr,
			IERC20Events::Approval(IERC20::Approval {
				owner: owner_h160.0.into(),
				spender: spender_h160.0.into(),
				value: just_over,
			}),
		);
	});
}

/// `name()`, `symbol()`, and `decimals()` must NOT revert when an asset has
/// no metadata row — real ERC-20s never revert on introspection. The legitimate
/// case is a foreign asset registered before metadata is set; reverting there
/// breaks wallet/indexer token-discovery flows.
///
/// Pins the call-stable contract: all three selectors succeed, return defaults
/// (`""`, `""`, `0`), and do not revert.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn metadata_returns_defaults_when_unset(asset_index: u16) {
	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let owner = 123456789;

		// Create the asset but do NOT call `force_set_metadata`.
		setup_asset_for_prefix(asset_id, asset_index);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));

		let call_view = |data: Vec<u8>| -> Vec<u8> {
			let result = pallet_revive::Pallet::<Test>::bare_call(
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
			let exec = result.result.expect("metadata view must not trap");
			assert!(
				!exec.did_revert(),
				"metadata view must not revert: 0x{}",
				hex::encode(&exec.data)
			);
			exec.data
		};

		let name_bytes = call_view(IERC20::nameCall {}.abi_encode());
		assert_eq!(IERC20::nameCall::abi_decode_returns(&name_bytes).unwrap(), "");

		let symbol_bytes = call_view(IERC20::symbolCall {}.abi_encode());
		assert_eq!(IERC20::symbolCall::abi_decode_returns(&symbol_bytes).unwrap(), "");

		// Trade-off pinned: we deliberately report `0` rather than substituting
		// an ERC-20-idiomatic `18` (or 12, or any other guess). See the doc
		// comment on `decimals` for why — the precompile must not invent a
		// value the chain itself doesn't have. Chain operators must call
		// `force_set_metadata` before exposing an asset to EVM tooling, or
		// wallets will display balances `10^decimals` larger than intended.
		let decimals_bytes = call_view(IERC20::decimalsCall {}.abi_encode());
		assert_eq!(IERC20::decimalsCall::abi_decode_returns(&decimals_bytes).unwrap(), 0u8);
	});
}

/// Non-UTF-8 metadata must revert at `name()` / `symbol()`, not lossy-decode.
///
/// If we lossy-decoded, the wallet's EIP-712 domain separator (hashed over
/// the UTF-8 encoding of the returned string, with U+FFFD substitutions)
/// would diverge from the on-chain `compute_domain_separator` which hashes
/// the raw stored bytes — every `permit()` against such an asset would
/// then revert at signer recovery with a misleading "Signer does not match
/// owner". Reverting at introspection keeps the failure attributable to
/// the metadata, not to the wallet.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn metadata_non_utf8_reverts(asset_index: u16) {
	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let owner = 123456789;

		setup_asset_for_prefix(asset_id, asset_index);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));
		// 0xFF, 0xFE are not valid UTF-8 starter bytes.
		assert_ok!(Assets::force_set_metadata(
			RuntimeOrigin::root(),
			asset_id,
			vec![0xFF, 0xFE],
			vec![0xFF, 0xFE],
			6,
			false,
		));

		let call_view = |data: Vec<u8>| -> pallet_revive::ExecReturnValue {
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
			)
			.result
			.expect("metadata view must not trap")
		};

		// Pin the exact revert strings — these are the pre-PR values and
		// callers may string-match against them. Changing them would be a
		// silent behavior break in the failing path.
		use alloy::sol_types::{Revert, SolError};

		let name_exec = call_view(IERC20::nameCall {}.abi_encode());
		assert!(name_exec.did_revert(), "name() must revert on non-UTF-8 metadata");
		assert_eq!(
			Revert::abi_decode(&name_exec.data).expect("Error(string)").reason,
			"Invalid UTF-8 in name",
		);

		let symbol_exec = call_view(IERC20::symbolCall {}.abi_encode());
		assert!(symbol_exec.did_revert(), "symbol() must revert on non-UTF-8 metadata");
		assert_eq!(
			Revert::abi_decode(&symbol_exec.data).expect("Error(string)").reason,
			"Invalid UTF-8 in symbol",
		);

		// decimals() is unaffected by UTF-8 — it returns the stored u8 verbatim.
		let decimals_exec = call_view(IERC20::decimalsCall {}.abi_encode());
		assert!(!decimals_exec.did_revert(), "decimals() must succeed regardless of name/symbol");
		assert_eq!(IERC20::decimalsCall::abi_decode_returns(&decimals_exec.data).unwrap(), 6u8);
	});
}
