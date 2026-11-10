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

//! Tests for the pallet-jamkb precompile, called through pallet-revive.

use crate::{mock::*, IJamkb, JAMKB_PRECOMPILE};
use alloy_core::sol_types::{Revert, SolCall, SolError};
use frame_support::{
	assert_ok,
	weights::{Weight, WeightMeter},
};
use pallet_jamkb::{
	transport::{IncomingTransfer, OnIncomingTransfers},
	AllocationState,
};
use pallet_revive::{
	precompiles::{AddressMatcher, H160},
	ExecConfig, ExecReturnValue, TransactionLimits, U256,
};

const MEMO: [u8; 128] = [0xAB; 128];

fn precompile() -> H160 {
	let matcher = AddressMatcher::Fixed(core::num::NonZero::new(JAMKB_PRECOMPILE).unwrap());
	H160::from(matcher.base_address())
}

/// Calls the precompile as `who` and returns what it returned.
fn call(who: &AccountId, data: Vec<u8>) -> ExecReturnValue {
	pallet_revive::Pallet::<Test>::bare_call(
		RuntimeOrigin::signed(who.clone()),
		precompile(),
		U256::zero(),
		TransactionLimits::WeightAndDeposit { weight_limit: Weight::MAX, deposit_limit: u128::MAX },
		data,
		&ExecConfig::new_substrate_tx(),
	)
	.result
	.expect("the call is executed")
}

/// Calls the precompile as `who` and decodes the return value of `C`.
fn call_ok<C: SolCall>(who: &AccountId, call_data: C) -> C::Return {
	let result = call(who, call_data.abi_encode());
	assert!(!result.did_revert(), "reverted: {:?}", Revert::abi_decode(&result.data));
	C::abi_decode_returns(&result.data).expect("the return value decodes")
}

/// Calls the precompile as `who`, expects a revert and returns its reason.
fn call_revert(who: &AccountId, data: Vec<u8>) -> String {
	let result = call(who, data);
	assert!(result.did_revert(), "the call did not revert");
	Revert::abi_decode(&result.data).expect("a revert reason").reason
}

fn approve_release(amount: Balance, valid_for: u32) -> u64 {
	assert_ok!(Jamkb::approve_release(
		RuntimeOrigin::root(),
		SERVICE,
		amount,
		MEMO,
		None,
		valid_for
	));
	pallet_jamkb::LastAllocationId::<Test>::get()
}

fn memo_words(memo: [u8; 128]) -> [alloy_core::primitives::FixedBytes<32>; 4] {
	core::array::from_fn(|i| {
		alloy_core::primitives::FixedBytes::from_slice(&memo[i * 32..(i + 1) * 32])
	})
}

#[test]
fn execute_release_records_the_delivery_and_operation_state_follows_it() {
	new_test_ext(0).execute_with(|| {
		let id = approve_release(100, 10);

		let op = call_ok(&ALICE, IJamkb::executeReleaseCall { allocationId: id });
		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Delivering);
		assert_eq!(
			call_ok(&BOB, IJamkb::operationStateCall { opId: op }),
			IJamkb::OperationState::Requested
		);

		confirm(op);
		assert_eq!(
			call_ok(&BOB, IJamkb::operationStateCall { opId: op }),
			IJamkb::OperationState::Confirmed
		);
		poll();
		assert_eq!(
			call_ok(&BOB, IJamkb::operationStateCall { opId: op }),
			IJamkb::OperationState::NotFound
		);

		let allocation = call_ok(&BOB, IJamkb::allocationCall { allocationId: id });
		assert_eq!(allocation.id, id);
		assert_eq!(allocation.state, IJamkb::AllocationState::Delivered);
		assert_eq!(allocation.mode, IJamkb::AllocationMode::Permanent);
		assert_eq!(allocation.target, SERVICE);
		assert_eq!(allocation.amount, 100);
		assert_eq!(allocation.approver, alloy_core::primitives::FixedBytes::<32>::ZERO);
		assert_eq!(allocation.deliveredAt, 2);
		assert_eq!(allocation.duration, 0);
		assert_eq!(allocation.validFrom, 1);
		assert_eq!(allocation.expiresAt, 11);
	});
}

#[test]
fn a_failed_call_reverts_with_the_pallet_error_name_and_leaves_nothing_behind() {
	new_test_ext(0).execute_with(|| {
		assert_eq!(
			call_revert(&ALICE, IJamkb::executeReleaseCall { allocationId: 9 }.abi_encode()),
			"UnknownAllocation"
		);
		assert_eq!(
			call_revert(&ALICE, IJamkb::allocationCall { allocationId: 9 }.abi_encode()),
			"UnknownAllocation"
		);
		// A rejected redemption records no operation and holds nothing.
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		let redeem =
			|amount| IJamkb::redeemCall { amount, target: 0, memo: memo_words(MEMO) }.abi_encode();
		assert_eq!(call_revert(&ALICE, redeem(50)), "TargetIsParachainService");
		assert_eq!(pallet_jamkb::Operations::<Test>::count(), 0);
		assert_eq!(Assets::balance(JAMKB, &ALICE), 100);
	});
}

#[test]
fn redeem_holds_the_callers_units() {
	new_test_ext(0).execute_with(|| {
		use frame_support::traits::fungibles::InspectHold;
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));

		let op = call_ok(
			&ALICE,
			IJamkb::redeemCall { amount: 60, target: SERVICE, memo: memo_words(MEMO) },
		);
		let reason = RuntimeHoldReason::Jamkb(pallet_jamkb::HoldReason::Releasing);
		assert_eq!(AssetsHolder::balance_on_hold(JAMKB, &reason, &ALICE), 60);
		match Jamkb::operation(op).unwrap().payload {
			pallet_jamkb::OperationPayload::Redemption { holder, target, amount, memo } => {
				assert_eq!((holder, target, amount, memo), (ALICE, SERVICE, 60, MEMO));
			},
			other => panic!("not a redemption: {other:?}"),
		}

		confirm(op);
		poll();
		assert_eq!(Assets::balance(JAMKB, &ALICE), 40);
		assert_eq!(Assets::balance(JAMKB, &Jamkb::released_account()), 60);
	});
}

#[test]
fn claim_pays_out_a_voluntary_return() {
	new_test_ext(100).execute_with(|| {
		let mut memo = [0u8; 128];
		memo[..32].copy_from_slice(BOB.as_ref());
		let transfer =
			IncomingTransfer { source: SERVICE, amount: 30, to_supervisor_balance: false, memo };
		assert_eq!(
			<Jamkb as OnIncomingTransfers<Balance>>::on_incoming_transfers(
				vec![transfer],
				&mut WeightMeter::new()
			),
			Ok(())
		);

		let bob: [u8; 32] = *BOB.as_ref();
		assert_eq!(call_ok(&ALICE, IJamkb::claimableOfCall { beneficiary: bob.into() }), 30);
		assert_eq!(call_ok(&ALICE, IJamkb::claimCall { beneficiary: bob.into() }), 30);
		assert_eq!(call_ok(&ALICE, IJamkb::claimableOfCall { beneficiary: bob.into() }), 0);
		assert_eq!(
			call_revert(&ALICE, IJamkb::claimCall { beneficiary: bob.into() }.abi_encode()),
			"NothingToClaim"
		);
	});
}

#[test]
fn cancel_allocation_by_a_contract_waits_for_the_expiry() {
	new_test_ext(0).execute_with(|| {
		let id = approve_release(100, 10);
		assert_eq!(
			call_revert(&ALICE, IJamkb::cancelAllocationCall { allocationId: id }.abi_encode()),
			"BadOrigin"
		);
		CurrentTimeslot::set(11);
		call_ok(&ALICE, IJamkb::cancelAllocationCall { allocationId: id });
		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Closed);
	});
}

#[test]
fn current_timeslot_is_the_pallets() {
	new_test_ext(0).execute_with(|| {
		CurrentTimeslot::set(77);
		assert_eq!(call_ok(&ALICE, IJamkb::currentTimeslotCall {}), 77);
	});
}
