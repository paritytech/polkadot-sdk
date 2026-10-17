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
	foreign_assets::pallet::Pallet as ForeignAssetsPallet,
	mock::{
		new_test_ext, Assets, AssetsHolder, Balances, RuntimeEvent, RuntimeHoldReason,
		RuntimeOrigin, System, Test,
	},
	permit,
	test_helpers::{
		assert_contract_event, set_prefix_in_address, setup_asset_for_prefix, token_address,
		ICaller, PRECOMPILE_ADDRESS_PREFIX, PRECOMPILE_ADDRESS_PREFIX_FOREIGN,
	},
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

// A fresh `approve` consumes exactly `allowance() + approve_transfer() + DepositEvent{3, 32}`.
// Asserting that sum pins the `DepositEvent` charge to `len = data.len()` (32), guarding against
// it regressing to `topics.len()` (3), which would undercharge the per-byte event cost.
#[test]
fn deposit_event_charges_data_byte_length() {
	use pallet_revive::precompiles::Token;

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(PRECOMPILE_ADDRESS_PREFIX));
		let owner = 123456789;
		let spender = 987654321;
		Balances::make_free_balance_be(&owner, 100);
		let spender_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&spender);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, 100));

		let data = IERC20::approveCall { spender: spender_addr.0.into(), value: U256::from(10) }
			.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(owner),
			asset_addr,
			0u32.into(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: u128::MAX,
			},
			data,
			&ExecConfig::new_substrate_tx(),
		);
		assert!(result.result.is_ok(), "approve call failed: {:?}", result.result);

		let expected = <() as pallet_assets::WeightInfo>::allowance()
			.saturating_add(<() as pallet_assets::WeightInfo>::approve_transfer())
			.saturating_add(<RuntimeCosts as Token<Test>>::weight(&RuntimeCosts::DepositEvent {
				num_topic: 3,
				len: 32,
			}));
		assert_eq!(
			result.weight_consumed, expected,
			"approve weight does not match allowance() + approve_transfer() + \
			 DepositEvent{{num_topic: 3, len: 32}} — deposit_event has likely \
			 regressed to charging len=topics.len() instead of len=data.len()",
		);
	});
}

// The mirrored `Transfer` has two destinations: the buffer, whose insert the `transfer()`
// benchmark measures with the callback wired, and — inside an ethereum transaction — this call's
// own receipt, which no pallet benchmark reaches. The `DepositEvent` charge pays for the second,
// so a `transfer` must consume exactly `transfer() + DepositEvent{3, 32}`.
#[test]
fn transfer_charges_receipt_capture() {
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
				deposit_limit: u128::MAX,
			},
			data,
			&ExecConfig::new_substrate_tx(),
		);
		assert!(result.result.is_ok(), "transfer call failed: {:?}", result.result);

		let receipt_capture = <RuntimeCosts as Token<Test>>::weight(&RuntimeCosts::DepositEvent {
			num_topic: 3,
			len: 32,
		});
		let expected =
			<() as pallet_assets::WeightInfo>::transfer().saturating_add(receipt_capture);
		assert_eq!(
			result.weight_consumed, expected,
			"transfer weight does not match transfer() + DepositEvent{{num_topic: 3, len: 32}}",
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

// `token_address` is the inverse of `asset_id_from_address`: every id must survive the
// round-trip id -> address -> id, including the boundaries.
#[test]
fn token_address_round_trips_through_extractor() {
	for id in [0u32, 1, 1337, 0xDEAD_BEEF, u32::MAX] {
		let address =
			Erc20TransferLogsCallback::<Test, InlineIdConfig<0x0120>>::token_address(&id).unwrap();
		let extracted =
			<InlineIdConfig<0x0120> as AssetPrecompileConfig>::AssetIdExtractor::asset_id_from_address(
				&address.0,
			)
			.unwrap();
		assert_eq!(extracted, id, "round-trip failed for asset id {id}");
	}
}

// The foreign callback derives the token address through the live id->index map (resolved at emit
// time), exactly as the foreign ERC-20 precompile is addressed. A registered asset must round-trip
// id -> address -> id; an unregistered asset has no address, so `token_address` yields `None`.
#[test]
fn foreign_token_address_resolves_through_map() {
	new_test_ext().execute_with(|| {
		let asset_id = 1337u32;
		// Unregistered: no mapping, no address.
		assert!(Erc20TransferLogsCallback::<Test, ForeignIdConfig<0x0220, Test>>::token_address(
			&asset_id
		)
		.is_none());

		let index = ForeignAssetsPallet::<Test>::insert_asset_mapping(&asset_id).unwrap();
		let address =
			Erc20TransferLogsCallback::<Test, ForeignIdConfig<0x0220, Test>>::token_address(
				&asset_id,
			)
			.unwrap();
		// The address carries the allocated index, not the asset id.
		assert_eq!(u32::from_be_bytes(address.0[..4].try_into().unwrap()), index);
		// ... and resolves back to the asset id through the same map the precompile uses.
		let extracted =
			<ForeignIdConfig<0x0220, Test> as AssetPrecompileConfig>::AssetIdExtractor::asset_id_from_address(
				&address.0,
			)
			.unwrap();
		assert_eq!(extracted, asset_id);
	});
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
				deposit_limit: u128::MAX,
			},
			data,
			&ExecConfig::new_substrate_tx(),
		);

		// The log is mirrored by `Erc20TransferLogsCallback` at the callback's token address (asset
		// id 0, trust-backed prefix), regardless of which precompile alias was called.
		assert_contract_event(
			H160::from(set_prefix_in_address(PRECOMPILE_ADDRESS_PREFIX)),
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

// EIP-20 requires a zero-value transfer to be treated as a normal transfer and fire the `Transfer`
// event. `do_transfer` no-ops on zero and fires no callback, so the precompile emits the log
// itself; this asserts it is emitted (value 0, canonical token address) and balances are unchanged.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn precompile_zero_value_transfer_emits_log(asset_index: u16) {
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

		let data = IERC20::transferCall { to: to_addr.0.into(), value: U256::ZERO }.abi_encode();

		pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(from),
			asset_addr,
			0u32.into(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: u128::MAX,
			},
			data,
			&ExecConfig::new_substrate_tx(),
		);

		// Emitted at the address the caller invoked, as EIP-20 requires: a `Transfer` log for
		// token X belongs at token X. The mock is the only place these can differ — it gives one
		// assets instance two precompile aliases, whereas a runtime pairs each instance with
		// exactly one `ERC20` and one matching callback config.
		assert_contract_event(
			asset_addr,
			IERC20Events::Transfer(IERC20::Transfer {
				from: from_addr.0.into(),
				to: to_addr.0.into(),
				value: U256::ZERO,
			}),
		);

		// Zero transfer moves nothing.
		assert_eq!(Assets::balance(asset_id, from), 100);
		assert_eq!(Assets::balance(asset_id, to), 0);
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
				deposit_limit: u128::MAX,
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
				deposit_limit: u128::MAX,
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
				deposit_limit: u128::MAX,
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
				deposit_limit: u128::MAX,
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
				deposit_limit: u128::MAX,
			},
			data,
			&ExecConfig::new_substrate_tx(),
		);
		assert_eq!(Assets::balance(asset_id, owner), 90);
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), 15);
		assert_eq!(Assets::balance(asset_id, other), 10);

		// Mirrored by `Erc20TransferLogsCallback` at the callback's token address, not
		// `asset_addr`.
		assert_contract_event(
			H160::from(set_prefix_in_address(PRECOMPILE_ADDRESS_PREFIX)),
			IERC20Events::Transfer(IERC20::Transfer {
				from: owner_addr.0.into(),
				to: other_addr.0.into(),
				value: U256::from(10),
			}),
		);
	});
}

// EIP-20 counterpart of `precompile_zero_value_transfer_emits_log` for the approved-transfer path:
// a zero-value `transferFrom` is a no-op in `do_transfer_approved` and fires no callback, so the
// precompile must still emit the `Transfer` log itself.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn precompile_zero_value_transfer_from_emits_log(asset_index: u16) {
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

		// Give the spender an allowance so the zero-value `transferFrom` is authorised.
		call_approve(owner, asset_addr, spender_addr, U256::from(25));

		let data = IERC20::transferFromCall {
			from: owner_addr.0.into(),
			to: other_addr.0.into(),
			value: U256::ZERO,
		}
		.abi_encode();

		pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(spender),
			asset_addr,
			0u32.into(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: u128::MAX,
			},
			data,
			&ExecConfig::new_substrate_tx(),
		);

		// Emitted at the address the caller invoked; see
		// `precompile_zero_value_transfer_emits_log`.
		assert_contract_event(
			asset_addr,
			IERC20Events::Transfer(IERC20::Transfer {
				from: owner_addr.0.into(),
				to: other_addr.0.into(),
				value: U256::ZERO,
			}),
		);

		// Nothing moved.
		assert_eq!(Assets::balance(asset_id, owner), 100);
		assert_eq!(Assets::balance(asset_id, other), 0);
	});
}

/// Helper to call approve via the precompile. Returns the bare call result.
fn raw_approve(
	owner: u64,
	asset_addr: H160,
	spender_addr: H160,
	value: U256,
) -> pallet_revive::ContractResult<pallet_revive::ExecReturnValue, u128> {
	let data = IERC20::approveCall { spender: spender_addr.0.into(), value }.abi_encode();
	pallet_revive::Pallet::<Test>::bare_call(
		RuntimeOrigin::signed(owner),
		asset_addr,
		0u32.into(),
		TransactionLimits::WeightAndDeposit { weight_limit: Weight::MAX, deposit_limit: u128::MAX },
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

		let deposit: u128 = <Test as pallet_assets::Config>::ApprovalDeposit::get();
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

		let deposit: u128 = <Test as pallet_assets::Config>::ApprovalDeposit::get();

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

		let deposit: u128 = <Test as pallet_assets::Config>::ApprovalDeposit::get();

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

		let deposit: u128 = <Test as pallet_assets::Config>::ApprovalDeposit::get();

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
		Balances::make_free_balance_be(&deployer, 1_000_000_000_000_000u128);

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
				deposit_limit: u128::MAX,
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
				deposit_limit: u128::MAX,
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
		Balances::make_free_balance_be(&deployer, 1_000_000_000_000_000u128);

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
				deposit_limit: u128::MAX,
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
				deposit_limit: u128::MAX,
			},
			calldata,
			&ExecConfig::new_substrate_tx(),
		)
		.result
		.expect("outer call must succeed");

		let ret = ICaller::delegateCall::abi_decode_returns(&result.data)
			.expect("return must decode as (bool, bytes)");
		assert!(!ret.success, "DELEGATECALL to asset precompile must be rejected");
		use alloy::sol_types::{Revert, SolError};
		let decoded = Revert::abi_decode(&ret.output).expect("Error(string) revert");
		assert_eq!(decoded.reason, "illegal to call this pre-compile via delegate call");
	});
}

/// `approve(spender, type(uint256).max)` is the universal "infinite allowance" idiom in EVM
/// tooling (MetaMask, Uniswap, every DEX router). `U256::MAX` doesn't fit in the runtime
/// `Balance`, so the precompile must saturate the *stored* allowance at `Balance::MAX`
/// rather than revert at the conversion. The `Approval` event still carries the raw
/// `call.value` (`U256::MAX`) so EVM wallets and indexers recognise the canonical
/// "Unlimited approval" sentinel.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn approve_saturates_on_uint256_max(asset_index: u16) {
	use frame_support::traits::fungibles::approvals::Inspect;

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let owner = 123456789u64;
		let spender = 987654321u64;
		Balances::make_free_balance_be(&owner, 100);

		let owner_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&owner);
		let spender_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&spender);

		setup_asset_for_prefix(asset_id, asset_index);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));

		call_approve(owner, asset_addr, spender_addr, U256::MAX);

		// Stored allowance is saturated to `Balance::MAX`.
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), u128::MAX);

		// Event carries the raw `call.value`, not the saturated stored amount.
		assert_contract_event(
			asset_addr,
			IERC20Events::Approval(IERC20::Approval {
				owner: owner_addr.0.into(),
				spender: spender_addr.0.into(),
				value: U256::MAX,
			}),
		);
	});
}

/// Boundary: saturation must trigger for *any* `U256 > Balance::MAX`, not only the exact
/// `U256::MAX` sentinel. Guards against a regression that would scope saturation to the
/// `call.value == U256::MAX` literal — routers that compute "infinite allowance" as
/// `U256::MAX - k` for small `k` would still need to work.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn approve_saturates_above_balance_max(asset_index: u16) {
	use frame_support::traits::fungibles::approvals::Inspect;

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let owner = 123456789u64;
		let spender = 987654321u64;
		Balances::make_free_balance_be(&owner, 100);

		let spender_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&spender);

		setup_asset_for_prefix(asset_id, asset_index);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));

		// Smallest `U256` that doesn't fit in the mock's `Balance` (u128).
		let just_over = U256::from(u128::MAX) + U256::from(1u64);
		call_approve(owner, asset_addr, spender_addr, just_over);
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), u128::MAX);
	});
}

/// Asymmetry pin: `transfer` and `transferFrom` move exact amounts, so an overflowing
/// `value` must revert at the `U256 → Balance` boundary rather than silently
/// transferring `Balance::MAX`. Only allowance writes (`approve` / `permit`) saturate.
#[test]
fn transfer_and_transfer_from_revert_on_overflow() {
	use alloy::sol_types::{Revert, SolError};

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(PRECOMPILE_ADDRESS_PREFIX));
		let from = 123456789u64;
		let to = 987654321u64;
		Balances::make_free_balance_be(&from, 100);
		Balances::make_free_balance_be(&to, 100);
		let from_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&from);
		let to_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&to);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, from, true, 1));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(from), asset_id, from, 100));

		// Authorise the spender with a small finite allowance so the `transferFrom`
		// path reaches the value conversion before any approval check.
		call_approve(from, asset_addr, to_addr, U256::from(50u64));

		let assert_reverts_with = |caller: u64, data: Vec<u8>, label: &str| {
			let exec = pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(caller),
				asset_addr,
				0u32.into(),
				TransactionLimits::WeightAndDeposit {
					weight_limit: Weight::MAX,
					deposit_limit: u128::MAX,
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
				"{label} must revert at the U256 -> Balance boundary",
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

		// Nothing moved.
		assert_eq!(Assets::balance(asset_id, from), 100);
		assert_eq!(Assets::balance(asset_id, to), 0);
	});
}

/// No on-chain sentinel: after `approve(uint256.max)` (which saturates to `Balance::MAX`),
/// each `transferFrom` still decrements the stored allowance. This pins the deliberate
/// departure from OpenZeppelin's `_spendAllowance` skip-on-`type(uint256).max` rule — on
/// this chain there is no allowance-state inspection that can distinguish a saturated
/// `uint256.max` approval from a finite `Balance::MAX` approval, so we don't try.
/// `Balance::MAX` is large enough that this is operationally indistinguishable from
/// infinite for any realistic transfer cadence.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn transfer_from_decrements_normally_after_max_approve(asset_index: u16) {
	use frame_support::traits::fungibles::approvals::Inspect;

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let owner = 123456789u64;
		let spender = 987654321u64;
		let recipient = 111222333u64;
		Balances::make_free_balance_be(&owner, 100);
		Balances::make_free_balance_be(&spender, 100);
		Balances::make_free_balance_be(&recipient, 100);

		let owner_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&owner);
		let spender_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&spender);
		let recipient_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&recipient);

		setup_asset_for_prefix(asset_id, asset_index);
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, 100));

		call_approve(owner, asset_addr, spender_addr, U256::MAX);
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), u128::MAX);

		// Each `transferFrom` decrements the saturated allowance by the spent amount.
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
				deposit_limit: u128::MAX,
			},
			data,
			&ExecConfig::new_substrate_tx(),
		);
		assert!(!result.result.unwrap().did_revert(), "transferFrom must succeed");
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), u128::MAX - 10);
		assert_eq!(Assets::balance(asset_id, &recipient), 10);
	});
}

// The `Erc20TransferLogsCallback` callback (wired as `CallbackHandle` in the mock) mirrors plain
// substrate asset operations — no precompile involved — as canonical ERC-20 `Transfer` logs
// at the asset's precompile address. Mint = from 0x0, burn = to 0x0, per ERC-20 convention.
#[test]
fn plain_asset_operations_emit_erc20_transfer_logs() {
	new_test_ext().execute_with(|| {
		let asset_id = 5u32;
		let owner = 1u64;
		let user = 2u64;
		let token = token_address(PRECOMPILE_ADDRESS_PREFIX, asset_id);
		let owner_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&owner);
		let user_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&user);

		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));

		// Mint: Transfer(0x0 -> owner).
		assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, 100));
		assert_contract_event(
			token,
			IERC20Events::Transfer(IERC20::Transfer {
				from: alloy::primitives::Address::ZERO,
				to: owner_addr.0.into(),
				value: U256::from(100),
			}),
		);

		// Plain extrinsic transfer: Transfer(owner -> user).
		assert_ok!(Assets::transfer(RuntimeOrigin::signed(owner), asset_id, user, 40));
		assert_contract_event(
			token,
			IERC20Events::Transfer(IERC20::Transfer {
				from: owner_addr.0.into(),
				to: user_addr.0.into(),
				value: U256::from(40),
			}),
		);

		// Burn: Transfer(user -> 0x0).
		assert_ok!(Assets::burn(RuntimeOrigin::signed(owner), asset_id, user, 40));
		assert_contract_event(
			token,
			IERC20Events::Transfer(IERC20::Transfer {
				from: user_addr.0.into(),
				to: alloy::primitives::Address::ZERO,
				value: U256::from(40),
			}),
		);
	});
}

// The foreign wiring shape (`(ForeignAssetId, Erc20TransferLogsCallback<ForeignIdConfig>)`, as
// Asset Hub Westend wires its foreign-assets instance): `created` allocates the id->index
// mapping, and a balance change then mirrors `Transfer(0x0, owner)` at the address derived
// through that mapping — the ordering the wiring depends on.
#[test]
fn foreign_wiring_emits_at_foreign_derived_address() {
	use pallet_assets::AssetsCallback;
	type ForeignWiring =
		(ForeignAssetId<Test>, Erc20TransferLogsCallback<Test, ForeignIdConfig<0x0220, Test>>);
	new_test_ext().execute_with(|| {
		let asset_id = 7u32;
		let owner = 1u64;
		let owner_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&owner);

		assert_ok!(ForeignWiring::created(&asset_id, &owner));
		let index = ForeignAssetsPallet::<Test>::asset_index_of(&asset_id)
			.expect("`created` allocates the mapping");
		ForeignWiring::issued(&asset_id, &owner, 55);

		assert_contract_event(
			token_address(PRECOMPILE_ADDRESS_PREFIX_FOREIGN, index),
			IERC20Events::Transfer(IERC20::Transfer {
				from: alloy::primitives::Address::ZERO,
				to: owner_addr.0.into(),
				value: U256::from(55),
			}),
		);
	});
}

// The `fungibles::Balanced` imbalance paths (e.g. paying tx fees in an asset) are mirrored too:
// withdraw = Transfer(who -> 0x0), deposit = Transfer(0x0 -> who).
#[test]
fn balanced_paths_emit_erc20_transfer_logs() {
	use frame_support::traits::{
		fungibles::Balanced,
		tokens::{Fortitude, Precision, Preservation},
	};
	new_test_ext().execute_with(|| {
		let asset_id = 5u32;
		let owner = 1u64;
		let user = 2u64;
		let token = token_address(PRECOMPILE_ADDRESS_PREFIX, asset_id);
		let owner_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&owner);
		let user_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&user);

		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, 100));

		// Withdraw (debit): Transfer(owner -> 0x0).
		let credit = <Assets as Balanced<u64>>::withdraw(
			asset_id,
			&owner,
			40,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Polite,
		)
		.expect("withdraw succeeds");
		assert_contract_event(
			token,
			IERC20Events::Transfer(IERC20::Transfer {
				from: owner_addr.0.into(),
				to: alloy::primitives::Address::ZERO,
				value: U256::from(40),
			}),
		);

		// Deposit (credit) via resolve into `user`: Transfer(0x0 -> user).
		assert_ok!(<Assets as Balanced<u64>>::resolve(&user, credit));
		assert_contract_event(
			token,
			IERC20Events::Transfer(IERC20::Transfer {
				from: alloy::primitives::Address::ZERO,
				to: user_addr.0.into(),
				value: U256::from(40),
			}),
		);
	});
}

fn erc20_call<C: SolCall>(token: H160, caller: u64, call: C) -> C::Return {
	let data = pallet_revive::Pallet::<Test>::bare_call(
		RuntimeOrigin::signed(caller),
		token,
		0u32.into(),
		TransactionLimits::WeightAndDeposit { weight_limit: Weight::MAX, deposit_limit: u128::MAX },
		call.abi_encode(),
		&ExecConfig::new_substrate_tx(),
	)
	.result
	.unwrap()
	.data;
	C::abi_decode_returns(&data).unwrap()
}

fn contract_log_count(token: H160) -> usize {
	System::events()
		.iter()
		.filter(|record| {
			matches!(
				&record.event,
				RuntimeEvent::Revive(pallet_revive::Event::ContractEmitted { contract, .. })
					if *contract == token
			)
		})
		.count()
}

// A `fungibles::MutateHold` hold, which a revive storage deposit paid in PGAS drives, moves
// balance between the free and held portions of one account. `balanceOf` reports free plus held,
// so such a hold does not move it and needs no log, keeping `balanceOf` and `totalSupply`
// reconstructible from the `Transfer` stream alone.
#[test]
fn same_account_holds_do_not_move_balance_of() {
	use frame_support::traits::{fungibles::MutateHold, tokens::Precision};
	new_test_ext().execute_with(|| {
		// `TestAccountMapper` round-trips an address through a `u64`, which keeps only its low
		// bytes, so only the zero-index token address survives a `bare_call` in this mock.
		let asset_id = 0u32;
		let owner = 1u64;
		let reason = RuntimeHoldReason::Revive(pallet_revive::HoldReason::StorageDepositReserve);
		let token = token_address(PRECOMPILE_ADDRESS_PREFIX, asset_id);
		let owner_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&owner);
		let account_of = |who: &sp_core::H160| -> alloy::primitives::Address { who.0.into() };

		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));

		// Mint: Transfer(0x0 -> owner).
		assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, 100));
		assert_contract_event(
			token,
			IERC20Events::Transfer(IERC20::Transfer {
				from: alloy::primitives::Address::ZERO,
				to: account_of(&owner_addr),
				value: U256::from(100),
			}),
		);

		// A hold and a partial release only move balance between the free and held portions of
		// `owner`, so they emit no log beyond the mint's ...
		assert_ok!(<AssetsHolder as MutateHold<u64>>::hold(asset_id, &reason, &owner, 30));
		assert_ok!(<AssetsHolder as MutateHold<u64>>::release(
			asset_id,
			&reason,
			&owner,
			10,
			Precision::Exact,
		));
		assert_eq!(contract_log_count(token), 1);

		// ... and leave both `balanceOf` and `totalSupply` where the log stream puts them, even
		// though 20 of the owner's 100 is on hold.
		assert_eq!(
			erc20_call(token, owner, IERC20::balanceOfCall { account: account_of(&owner_addr) }),
			U256::from(100),
		);
		assert_eq!(erc20_call(token, owner, IERC20::totalSupplyCall {}), U256::from(100));
	});
}

// The hold paths that move value between accounts reach pallet-assets below the `Mutate` impl the
// callbacks hang off, so they report through `pallet-assets-holder`. A revive storage deposit paid
// in PGAS drives all three, which is why they have to keep `balanceOf` and `totalSupply`
// reconstructible from the `Transfer` stream.
#[test]
fn cross_account_hold_paths_emit_transfer_logs() {
	use frame_support::traits::{
		fungibles::MutateHold,
		tokens::{Fortitude, Precision, Preservation, Restriction},
	};
	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let owner = 1u64;
		let dest = 2u64;
		let reason = RuntimeHoldReason::Revive(pallet_revive::HoldReason::StorageDepositReserve);
		let token = token_address(PRECOMPILE_ADDRESS_PREFIX, asset_id);
		let owner_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&owner);
		let dest_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&dest);
		let account_of = |who: &sp_core::H160| -> alloy::primitives::Address { who.0.into() };

		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, 100));

		// Charging a deposit: owner's free balance becomes dest's held balance.
		assert_ok!(<AssetsHolder as MutateHold<u64>>::transfer_and_hold(
			asset_id,
			&reason,
			&owner,
			&dest,
			30,
			Precision::Exact,
			Preservation::Expendable,
			Fortitude::Polite,
		));
		assert_contract_event(
			token,
			IERC20Events::Transfer(IERC20::Transfer {
				from: account_of(&owner_addr),
				to: account_of(&dest_addr),
				value: U256::from(30),
			}),
		);

		// Refunding part of it: dest's held balance becomes owner's free balance.
		assert_ok!(<AssetsHolder as MutateHold<u64>>::transfer_on_hold(
			asset_id,
			&reason,
			&dest,
			&owner,
			10,
			Precision::Exact,
			Restriction::Free,
			Fortitude::Polite,
		));
		assert_contract_event(
			token,
			IERC20Events::Transfer(IERC20::Transfer {
				from: account_of(&dest_addr),
				to: account_of(&owner_addr),
				value: U256::from(10),
			}),
		);

		// Burning the rest of the hold, which moves `totalSupply` too.
		assert_ok!(<AssetsHolder as MutateHold<u64>>::burn_held(
			asset_id,
			&reason,
			&dest,
			20,
			Precision::Exact,
			Fortitude::Polite,
		));
		assert_contract_event(
			token,
			IERC20Events::Transfer(IERC20::Transfer {
				from: account_of(&dest_addr),
				to: alloy::primitives::Address::ZERO,
				value: U256::from(20),
			}),
		);

		// Mint plus the three moves, and nothing beyond them.
		assert_eq!(contract_log_count(token), 4);

		// What the log stream says the balances are is what the precompile reports.
		assert_eq!(
			erc20_call(token, owner, IERC20::balanceOfCall { account: account_of(&owner_addr) }),
			U256::from(80),
		);
		assert_eq!(
			erc20_call(token, owner, IERC20::balanceOfCall { account: account_of(&dest_addr) }),
			U256::from(0),
		);
		assert_eq!(erc20_call(token, owner, IERC20::totalSupplyCall {}), U256::from(80));
	});
}

/// Helper to call `transfer` via the precompile. Returns the bare call result.
fn raw_transfer(
	from: u64,
	asset_addr: H160,
	to_addr: H160,
	value: U256,
) -> pallet_revive::ContractResult<pallet_revive::ExecReturnValue, u128> {
	let data = IERC20::transferCall { to: to_addr.0.into(), value }.abi_encode();
	pallet_revive::Pallet::<Test>::bare_call(
		RuntimeOrigin::signed(from),
		asset_addr,
		0u32.into(),
		TransactionLimits::WeightAndDeposit { weight_limit: Weight::MAX, deposit_limit: u128::MAX },
		data,
		&ExecConfig::new_substrate_tx(),
	)
}

/// Helper to call `transferFrom` via the precompile. Returns the bare call result.
fn raw_transfer_from(
	spender: u64,
	asset_addr: H160,
	from_addr: H160,
	to_addr: H160,
	value: U256,
) -> pallet_revive::ContractResult<pallet_revive::ExecReturnValue, u128> {
	let data = IERC20::transferFromCall { from: from_addr.0.into(), to: to_addr.0.into(), value }
		.abi_encode();
	pallet_revive::Pallet::<Test>::bare_call(
		RuntimeOrigin::signed(spender),
		asset_addr,
		0u32.into(),
		TransactionLimits::WeightAndDeposit { weight_limit: Weight::MAX, deposit_limit: u128::MAX },
		data,
		&ExecConfig::new_substrate_tx(),
	)
}

/// Creates asset `asset_id` with `min_balance`, owned and fully minted to `owner`.
fn setup_asset_with_min_balance(
	asset_id: u32,
	asset_index: u16,
	owner: u64,
	min_balance: u128,
	mint: u128,
) {
	setup_asset_for_prefix(asset_id, asset_index);
	assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, min_balance));
	assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, mint));
}

/// Exactness pin for `transfer`: an ERC-20 `transfer(to, value)` either moves exactly
/// `value` or moves nothing at all. It must never sweep the sender's sub-`min_balance`
/// remainder into `to` on top of `value`, because the `Transfer` log carries `value`
/// and every indexer, router and accounting contract reconciles balances against it.
///
/// `min_balance = 10`, sender holds `100`, `transfer(95)` would leave `5` — non-zero and
/// below `min_balance`. `prep_debit` promotes the debit to `100` and, with
/// `burn_dust: false`, `prep_credit` hands all `100` to `to`.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn transfer_never_moves_more_than_value(asset_index: u16) {
	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let from = 123456789u64;
		let to = 987654321u64;
		Balances::make_free_balance_be(&from, 1000);
		Balances::make_free_balance_be(&to, 1000);
		let to_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&to);
		setup_asset_with_min_balance(asset_id, asset_index, from, 10, 100);

		let exec = raw_transfer(from, asset_addr, to_addr, U256::from(95u64))
			.result
			.expect("must not trap");

		if exec.did_revert() {
			// Refusing the dust-producing transfer is an acceptable outcome: amounts stay
			// exact and nothing moved.
			assert_eq!(Assets::balance(asset_id, from), 100);
			assert_eq!(Assets::balance(asset_id, to), 0);
		} else {
			// If it succeeds it must move exactly `value`.
			assert_eq!(
				Assets::balance(asset_id, to),
				95,
				"recipient credited more than `value` — sub-min_balance dust was swept into \
				 the transfer while the Transfer log reports `value`",
			);
			assert_eq!(Assets::balance(asset_id, from), 5);
		}
	});
}

/// Exactness pin for `transferFrom`: the spender must never move more than the allowance
/// it spends. Same setup as `transfer_never_moves_more_than_value`, but the sweep is
/// additionally an allowance overrun — `do_transfer_approved` decrements the approval by
/// `amount` while `transfer_and_die` moves `amount + dust`.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn transfer_from_never_moves_more_than_allowance_spent(asset_index: u16) {
	use frame_support::traits::fungibles::approvals::Inspect;

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let owner = 123456789u64;
		let spender = 555555555u64;
		let to = 987654321u64;
		for who in [owner, spender, to] {
			Balances::make_free_balance_be(&who, 1000);
		}
		let owner_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&owner);
		let to_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&to);
		setup_asset_with_min_balance(asset_id, asset_index, owner, 10, 100);
		assert_ok!(Assets::approve_transfer(RuntimeOrigin::signed(owner), asset_id, spender, 200));

		let exec = raw_transfer_from(spender, asset_addr, owner_addr, to_addr, U256::from(95u64))
			.result
			.expect("must not trap");

		let moved = 100 - Assets::balance(asset_id, owner);
		let spent = 200 - Assets::allowance(asset_id, &owner, &spender);

		if exec.did_revert() {
			assert_eq!(moved, 0);
			assert_eq!(spent, 0);
			assert_eq!(Assets::balance(asset_id, to), 0);
		} else {
			assert_eq!(
				moved, spent,
				"spender moved {moved} but only {spent} allowance was consumed — \
				 sub-min_balance dust escaped the allowance accounting",
			);
			assert_eq!(Assets::balance(asset_id, to), 95);
		}
	});
}

/// Control for the two exactness pins: when the sender keeps at least `min_balance` the
/// dust path is never reached, so this asserts the same invariants unconditionally and
/// proves the setup itself is sound.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn transfer_is_exact_when_remainder_covers_min_balance(asset_index: u16) {
	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let from = 123456789u64;
		let to = 987654321u64;
		Balances::make_free_balance_be(&from, 1000);
		Balances::make_free_balance_be(&to, 1000);
		let from_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&from);
		let to_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&to);
		setup_asset_with_min_balance(asset_id, asset_index, from, 10, 100);

		// Leaves exactly `min_balance` behind.
		let exec = raw_transfer(from, asset_addr, to_addr, U256::from(90u64))
			.result
			.expect("must not trap");
		assert!(!exec.did_revert(), "transfer leaving exactly min_balance must succeed");
		assert_eq!(Assets::balance(asset_id, from), 10);
		assert_eq!(Assets::balance(asset_id, to), 90);
		// Mirrored by `Erc20TransferLogsCallback` at the callback's token address, not
		// `asset_addr`.
		assert_contract_event(
			H160::from(set_prefix_in_address(PRECOMPILE_ADDRESS_PREFIX)),
			IERC20Events::Transfer(IERC20::Transfer {
				from: from_addr.0.into(),
				to: to_addr.0.into(),
				value: U256::from(90u64),
			}),
		);
	});
}

/// Full-balance transfers must keep working: `transfer(balanceOf(sender))` leaves a zero
/// remainder, so there is no dust to sweep and the amount is already exact. Any guard
/// against the dust sweep must not regress this — reaping the sender's asset account is
/// expected here.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn transfer_of_full_balance_is_allowed(asset_index: u16) {
	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let from = 123456789u64;
		let to = 987654321u64;
		Balances::make_free_balance_be(&from, 1000);
		Balances::make_free_balance_be(&to, 1000);
		let from_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&from);
		let to_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&to);
		setup_asset_with_min_balance(asset_id, asset_index, from, 10, 100);

		let exec = raw_transfer(from, asset_addr, to_addr, U256::from(100u64))
			.result
			.expect("must not trap");
		assert!(!exec.did_revert(), "full-balance transfer must succeed");
		assert_eq!(Assets::balance(asset_id, from), 0);
		assert_eq!(Assets::balance(asset_id, to), 100);
		// Mirrored by `Erc20TransferLogsCallback` at the callback's token address, not
		// `asset_addr`.
		assert_contract_event(
			H160::from(set_prefix_in_address(PRECOMPILE_ADDRESS_PREFIX)),
			IERC20Events::Transfer(IERC20::Transfer {
				from: from_addr.0.into(),
				to: to_addr.0.into(),
				value: U256::from(100u64),
			}),
		);
	});
}

/// Same for `transferFrom`: spending the whole balance through an allowance is exact.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn transfer_from_of_full_balance_is_allowed(asset_index: u16) {
	use frame_support::traits::fungibles::approvals::Inspect;

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let owner = 123456789u64;
		let spender = 555555555u64;
		let to = 987654321u64;
		for who in [owner, spender, to] {
			Balances::make_free_balance_be(&who, 1000);
		}
		let owner_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&owner);
		let to_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&to);
		setup_asset_with_min_balance(asset_id, asset_index, owner, 10, 100);
		assert_ok!(Assets::approve_transfer(RuntimeOrigin::signed(owner), asset_id, spender, 200));

		let exec = raw_transfer_from(spender, asset_addr, owner_addr, to_addr, U256::from(100u64))
			.result
			.expect("must not trap");
		assert!(!exec.did_revert(), "full-balance transferFrom must succeed");
		assert_eq!(Assets::balance(asset_id, owner), 0);
		assert_eq!(Assets::balance(asset_id, to), 100);
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), 100);
	});
}

/// The precompile's `Transfer` log and `pallet_assets`' own `Transferred` event describe
/// the same movement, so they must agree on the amount. `Transferred` carries `credit`
/// (what actually landed in `dest`) while the log carries `call.value`, which is how the
/// dust sweep becomes observable from two different indexing surfaces at once.
///
/// `min_balance = 10`, sender holds `100`, `transfer(90)` leaves exactly `10` — at, not
/// below, `min_balance` — so this stays outside the dust window and must succeed; a value
/// inside the window (e.g. 95) would revert before either surface reports anything, which
/// would make the agreement assertion below vacuous.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn transfer_log_agrees_with_pallet_event(asset_index: u16) {
	use crate::mock::{RuntimeEvent, System};

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let from = 123456789u64;
		let to = 987654321u64;
		Balances::make_free_balance_be(&from, 1000);
		Balances::make_free_balance_be(&to, 1000);
		let to_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&to);
		setup_asset_with_min_balance(asset_id, asset_index, from, 10, 100);

		let exec = raw_transfer(from, asset_addr, to_addr, U256::from(90u64))
			.result
			.expect("must not trap");
		assert!(!exec.did_revert(), "a transfer outside the dust window must not revert");

		let pallet_amount = System::events()
			.into_iter()
			.find_map(|record| match record.event {
				RuntimeEvent::Assets(pallet_assets::Event::Transferred { amount, .. }) => {
					Some(amount)
				},
				_ => None,
			})
			.expect("pallet_assets::Event::Transferred must be emitted");
		assert_eq!(
			pallet_amount, 90,
			"pallet_assets reported a transfer of {pallet_amount} while the ERC-20 \
			 Transfer log reports 90",
		);
	});
}

/// A dust-producing `transfer` must revert with a legible reason rather than silently
/// moving more than `value`, and must leave no state behind — no balance change and no
/// `Transfer` log.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn transfer_reverts_when_it_would_sweep_remainder(asset_index: u16) {
	use crate::mock::{RuntimeEvent, System};
	use alloy::sol_types::{Revert, SolError};

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let from = 123456789u64;
		let to = 987654321u64;
		Balances::make_free_balance_be(&from, 1000);
		Balances::make_free_balance_be(&to, 1000);
		let to_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&to);
		setup_asset_with_min_balance(asset_id, asset_index, from, 10, 100);
		// Setup's `mint` mirrors a `Transfer` log of its own; only the call's logs matter.
		System::reset_events();

		let exec = raw_transfer(from, asset_addr, to_addr, U256::from(95u64))
			.result
			.expect("must not trap");
		assert!(exec.did_revert(), "dust-producing transfer must revert");
		let decoded = Revert::abi_decode(&exec.data).expect("Error(string) revert");
		assert_eq!(decoded.reason, "Transfer would leave sender below minimum balance");

		assert_eq!(Assets::balance(asset_id, from), 100);
		assert_eq!(Assets::balance(asset_id, to), 0);
		assert!(
			!System::events().iter().any(|record| matches!(
				record.event,
				RuntimeEvent::Revive(pallet_revive::Event::ContractEmitted { .. })
			)),
			"no Transfer log may survive the revert",
		);
	});
}

/// Same guard on the `transferFrom` path: revert, allowance untouched, approval deposit
/// still held.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn transfer_from_reverts_when_it_would_sweep_remainder(asset_index: u16) {
	use alloy::sol_types::{Revert, SolError};
	use frame_support::traits::fungibles::approvals::Inspect;

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let owner = 123456789u64;
		let spender = 555555555u64;
		let to = 987654321u64;
		for who in [owner, spender, to] {
			Balances::make_free_balance_be(&who, 1000);
		}
		let owner_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&owner);
		let to_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&to);
		setup_asset_with_min_balance(asset_id, asset_index, owner, 10, 100);
		assert_ok!(Assets::approve_transfer(RuntimeOrigin::signed(owner), asset_id, spender, 200));

		let exec = raw_transfer_from(spender, asset_addr, owner_addr, to_addr, U256::from(95u64))
			.result
			.expect("must not trap");
		assert!(exec.did_revert(), "dust-producing transferFrom must revert");
		let decoded = Revert::abi_decode(&exec.data).expect("Error(string) revert");
		assert_eq!(decoded.reason, "Transfer would leave sender below minimum balance");

		assert_eq!(Assets::balance(asset_id, owner), 100);
		assert_eq!(Assets::balance(asset_id, to), 0);
		assert_eq!(Assets::allowance(asset_id, &owner, &spender), 200);
	});
}

/// A self-transfer moves nothing, but is held to the same rules as any other: a `value` that
/// would strand a sub-`min_balance` remainder reverts rather than logging a `Transfer` for more
/// than `value`, and one that strands nothing logs exactly `value`.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn self_transfer_is_held_to_the_same_rules(asset_index: u16) {
	use alloy::sol_types::{Revert, SolError};

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let from = 123456789u64;
		Balances::make_free_balance_be(&from, 1000);
		let from_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&from);
		setup_asset_with_min_balance(asset_id, asset_index, from, 10, 100);

		let exec = raw_transfer(from, asset_addr, from_addr, U256::from(95u64))
			.result
			.expect("must not trap");
		assert!(exec.did_revert(), "dust-producing self-transfer must revert");
		let decoded = Revert::abi_decode(&exec.data).expect("Error(string) revert");
		assert_eq!(decoded.reason, "Transfer would leave sender below minimum balance");
		assert_eq!(Assets::balance(asset_id, from), 100);

		let exec = raw_transfer(from, asset_addr, from_addr, U256::from(90u64))
			.result
			.expect("must not trap");
		assert!(!exec.did_revert(), "self-transfer stranding nothing must succeed");
		assert_eq!(Assets::balance(asset_id, from), 100);
		// Mirrored by `Erc20TransferLogsCallback` at the callback's token address, not
		// `asset_addr`.
		assert_contract_event(
			H160::from(set_prefix_in_address(PRECOMPILE_ADDRESS_PREFIX)),
			IERC20Events::Transfer(IERC20::Transfer {
				from: from_addr.0.into(),
				to: from_addr.0.into(),
				value: U256::from(90u64),
			}),
		);
	});
}

/// Zero-value transfers are a normal ERC-20 idiom and move nothing. They must not revert
/// — not even from a sender whose balance already sits below `min_balance`.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn zero_value_transfer_is_never_rejected(asset_index: u16) {
	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let from = 123456789u64;
		let to = 987654321u64;
		Balances::make_free_balance_be(&from, 1000);
		Balances::make_free_balance_be(&to, 1000);
		let to_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&to);
		setup_asset_with_min_balance(asset_id, asset_index, from, 10, 100);

		let exec = raw_transfer(from, asset_addr, to_addr, U256::ZERO)
			.result
			.expect("must not trap");
		assert!(!exec.did_revert(), "transfer(to, 0) must not revert");
		assert_eq!(Assets::balance(asset_id, from), 100);
		assert_eq!(Assets::balance(asset_id, to), 0);

		// `force_set_metadata`-free way to put the sender below `min_balance`: raise the
		// asset's minimum after the fact. The sender now holds less than the minimum, which
		// is exactly the state a remainder-only check would reject.
		assert_ok!(Assets::force_asset_status(
			RuntimeOrigin::root(),
			asset_id,
			from,
			from,
			from,
			from,
			1_000,
			true,
			false,
		));
		let exec = raw_transfer(from, asset_addr, to_addr, U256::ZERO)
			.result
			.expect("must not trap");
		assert!(!exec.did_revert(), "transfer(to, 0) must not revert below min_balance either");
		assert_eq!(Assets::balance(asset_id, from), 100);
	});
}

/// A `transfer` larger than the sender's balance reverts as `BalanceLow`, not as the
/// remainder-sweep revert.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn transfer_above_balance_still_fails_on_funds(asset_index: u16) {
	use pallet_revive::precompiles::alloy::sol_types::{Revert, SolError};

	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let from = 123456789u64;
		let to = 987654321u64;
		Balances::make_free_balance_be(&from, 1000);
		Balances::make_free_balance_be(&to, 1000);
		let to_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&to);
		setup_asset_with_min_balance(asset_id, asset_index, from, 10, 100);

		let exec = raw_transfer(from, asset_addr, to_addr, U256::from(150u64))
			.result
			.expect("overspending transfer must not trap");
		assert!(exec.did_revert(), "overspending transfer must revert");
		let decoded = Revert::abi_decode(&exec.data).expect("Error(string) revert");
		assert_eq!(decoded.reason, "Balance too low");
		assert_eq!(Assets::balance(asset_id, from), 100);
		assert_eq!(Assets::balance(asset_id, to), 0);
	});
}

/// XCM accounting depends on this exactness too, not just contracts.
///
/// `assets_common::ERC20Transactor` (used by Asset Hub's `AssetTransactors`) implements
/// `withdraw_asset` as an `IERC20::transfer` from the user to a checking account and then
/// credits the XCM holding register with the *requested* amount. `deposit_asset` is the
/// mirror image, transferring out of the checking account. `ERC20Matcher` accepts any
/// local `AccountKey20`, so an asset precompile address routes through it.
///
/// A dust sweep on the way in silently strands the remainder in the checking account
/// (holding is credited less than the checking account received); a sweep on the way out
/// hands the checking account's residue to whoever happens to deposit last. Both break the
/// invariant this test pins: the checking account's balance moves by exactly the amount
/// the holding register is credited.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn checking_account_round_trip_is_exact(asset_index: u16) {
	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let user = 123456789u64;
		let checking = 4242424242u64;
		Balances::make_free_balance_be(&user, 1000);
		Balances::make_free_balance_be(&checking, 1000);
		let checking_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&checking);
		setup_asset_with_min_balance(asset_id, asset_index, user, 10, 100);

		// `withdraw_asset`: move `amount` into the checking account. The XCM holding
		// register is credited with `amount`, so the checking account must not gain more.
		let amount = 95u64;
		let exec = raw_transfer(user, asset_addr, checking_addr, U256::from(amount))
			.result
			.expect("must not trap");
		let credited = Assets::balance(asset_id, checking);
		if exec.did_revert() {
			assert_eq!(credited, 0, "a rejected withdraw must not credit the checking account");
			assert_eq!(Assets::balance(asset_id, user), 100);
		} else {
			assert_eq!(
				credited, amount as u128,
				"checking account gained {credited} while XCM holding was credited {amount} — \
				 the difference is stranded in the checking account",
			);
		}
	});
}

/// `min_balance = 1` assets have no dust window at all: any non-zero remainder already
/// satisfies the minimum, so the guard is inert. Pins that the guard costs existing
/// `min_balance = 1` deployments nothing, including the transfer that empties the
/// sender's account.
#[test_case(PRECOMPILE_ADDRESS_PREFIX)]
#[test_case(PRECOMPILE_ADDRESS_PREFIX_FOREIGN)]
fn guard_is_inert_for_min_balance_one(asset_index: u16) {
	new_test_ext().execute_with(|| {
		let asset_id = 0u32;
		let asset_addr = H160::from(set_prefix_in_address(asset_index));
		let from = 123456789u64;
		let to = 987654321u64;
		Balances::make_free_balance_be(&from, 1000);
		Balances::make_free_balance_be(&to, 1000);
		let to_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&to);
		setup_asset_with_min_balance(asset_id, asset_index, from, 1, 100);

		// The last transfer empties the sender, which reaps the asset account.
		for value in [1u64, 98, 1] {
			let exec = raw_transfer(from, asset_addr, to_addr, U256::from(value))
				.result
				.expect("must not trap");
			assert!(!exec.did_revert(), "transfer({value}) must succeed at min_balance = 1");
		}
		assert_eq!(Assets::balance(asset_id, from), 0);
		assert_eq!(Assets::balance(asset_id, to), 100);
	});
}
