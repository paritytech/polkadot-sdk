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
	mock::{new_test_ext, Assets, Balances, RuntimeOrigin, Test},
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

		// Emitted at the callback's canonical token address (asset id 0, trust-backed prefix),
		// regardless of which precompile alias was called, matching the non-zero path.
		assert_contract_event(
			H160::from(set_prefix_in_address(PRECOMPILE_ADDRESS_PREFIX)),
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

		// Emitted at the callback's canonical token address, regardless of the alias called.
		assert_contract_event(
			H160::from(set_prefix_in_address(PRECOMPILE_ADDRESS_PREFIX)),
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

		Balances::make_free_balance_be(&owner, 100);
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
