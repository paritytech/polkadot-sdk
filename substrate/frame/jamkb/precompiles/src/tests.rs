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
use frame_support::{
	assert_ok,
	traits::fungible::{Inspect as _, InspectHold as _},
	weights::{Weight, WeightMeter},
};
use pallet_jamkb::{
	transport::{IncomingTransfer, MessageSendError, OnIncomingTransfers},
	AllocationState,
};
use pallet_revive::{
	precompiles::{
		alloy::{
			self,
			primitives::{Address, FixedBytes},
			sol_types::{Revert, SolCall, SolError},
		},
		AddressMatcher, H160,
	},
	AddressMapper as _, Code, ExecConfig, ExecReturnValue, TransactionLimits, U256,
};

alloy::sol! {
	/// The part of the `Caller` fixture contract the tests use.
	interface ICaller {
		function staticCall(address callee, bytes data, uint64 gas)
			external view returns (bool success, bytes output);
		function delegate(address callee, bytes data, uint64 gas)
			external returns (bool success, bytes output);
		function normal(address callee, uint64 value, bytes data, uint64 gas)
			external returns (bool success, bytes output);
	}
}

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

fn memo_words(memo: [u8; 128]) -> [FixedBytes<32>; 4] {
	core::array::from_fn(|i| FixedBytes::from_slice(&memo[i * 32..(i + 1) * 32]))
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
		assert_eq!(allocation.approver, FixedBytes::<32>::ZERO);
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
		// A rejected redemption records no operation and burns nothing.
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		let redeem =
			|amount| IJamkb::redeemCall { amount, target: 0, memo: memo_words(MEMO) }.abi_encode();
		assert_eq!(call_revert(&ALICE, redeem(50)), "TargetIsParachainService");
		assert_eq!(pallet_jamkb::Operations::<Test>::count(), 0);
		assert_eq!(Assets::balance(JAMKB, &ALICE), 100);

		// The transport refuses after the burn: pallet-revive rolls the burn back with the
		// revert, without a storage layer of the precompile's own.
		SendFailure::set(Some(MessageSendError::QueueFull));
		let redeem =
			IJamkb::redeemCall { amount: 50, target: SERVICE, memo: memo_words(MEMO) }.abi_encode();
		assert_eq!(call_revert(&ALICE, redeem), "TransportQueueFull");
		SendFailure::set(None);
		assert_eq!(pallet_jamkb::Operations::<Test>::count(), 0);
		assert_eq!(pallet_jamkb::UserOperations::<Test>::get(), 0);
		assert_eq!(Assets::balance(JAMKB, &ALICE), 100);
		assert_eq!(Assets::total_supply(JAMKB), CAP);
	});
}

/// Deploys the `Caller` fixture contract as `ALICE` and returns its address.
fn deploy_caller() -> H160 {
	let (code, _) = pallet_revive_fixtures::compile_module_with_type(
		"Caller",
		pallet_revive_fixtures::FixtureType::Solc,
	)
	.expect("the Caller fixture compiles");
	pallet_revive::Pallet::<Test>::bare_instantiate(
		RuntimeOrigin::signed(ALICE),
		U256::zero(),
		TransactionLimits::WeightAndDeposit { weight_limit: Weight::MAX, deposit_limit: u128::MAX },
		Code::Upload(code),
		vec![],
		None,
		&ExecConfig::new_substrate_tx(),
	)
	.result
	.expect("the Caller fixture deploys")
	.addr
}

/// Has the `Caller` fixture at `caller` run `call` (an `ICaller` call) and returns whether its
/// inner call to the precompile succeeded, with that call's output.
fn through_caller(caller: H160, call: Vec<u8>) -> (bool, Vec<u8>) {
	let result = pallet_revive::Pallet::<Test>::bare_call(
		RuntimeOrigin::signed(ALICE),
		caller,
		U256::zero(),
		TransactionLimits::WeightAndDeposit { weight_limit: Weight::MAX, deposit_limit: u128::MAX },
		call,
		&ExecConfig::new_substrate_tx(),
	)
	.result
	.expect("the call to the Caller fixture is executed");
	assert!(!result.did_revert(), "the Caller fixture reverted");
	// `staticCall` and `delegate` return the same `(bool, bytes)`.
	let ret = ICaller::delegateCall::abi_decode_returns(&result.data)
		.expect("the return value decodes as (bool, bytes)");
	(ret.success, ret.output.to_vec())
}

fn static_call(caller: H160, data: Vec<u8>) -> (bool, Vec<u8>) {
	let call = ICaller::staticCallCall {
		callee: Address::from(precompile().0),
		data: data.into(),
		gas: u64::MAX,
	};
	through_caller(caller, call.abi_encode())
}

fn plain_call(caller: H160, data: Vec<u8>) -> (bool, Vec<u8>) {
	let call = ICaller::normalCall {
		callee: Address::from(precompile().0),
		value: 0,
		data: data.into(),
		gas: u64::MAX,
	};
	through_caller(caller, call.abi_encode())
}

fn delegate_call(caller: H160, data: Vec<u8>) -> (bool, Vec<u8>) {
	let call = ICaller::delegateCall {
		callee: Address::from(precompile().0),
		data: data.into(),
		gas: u64::MAX,
	};
	through_caller(caller, call.abi_encode())
}

#[test]
fn state_changing_calls_are_denied_under_staticcall() {
	new_test_ext(0).execute_with(|| {
		let caller = deploy_caller();
		let id = approve_release(100, 10);
		let contract = <Test as pallet_revive::Config>::AddressMapper::to_account_id(&caller);
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), contract.clone(), 100));
		// Booked, so that a `claim` would otherwise succeed.
		pallet_jamkb::Unclaimed::<Test>::insert(&ALICE, 30);
		let beneficiary: [u8; 32] = *ALICE.as_ref();

		for data in [
			IJamkb::executeReleaseCall { allocationId: id }.abi_encode(),
			IJamkb::redeemCall { amount: 50, target: SERVICE, memo: memo_words(MEMO) }.abi_encode(),
			IJamkb::claimCall { beneficiary: beneficiary.into() }.abi_encode(),
			IJamkb::cancelAllocationCall { allocationId: id }.abi_encode(),
		] {
			let (success, _) = static_call(caller, data);
			assert!(!success, "a state-changing call succeeded under STATICCALL");
		}
		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Approved);
		assert_eq!(pallet_jamkb::Operations::<Test>::count(), 0);
		assert_eq!(Assets::balance(JAMKB, &contract), 100);
		assert_eq!(pallet_jamkb::Unclaimed::<Test>::get(&ALICE), 30);
		assert_eq!(Assets::balance(JAMKB, &ALICE), 0);

		let (success, output) =
			static_call(caller, IJamkb::allocationCall { allocationId: id }.abi_encode());
		assert!(success, "a read failed under STATICCALL");
		let allocation =
			IJamkb::allocationCall::abi_decode_returns(&output).expect("the allocation decodes");
		assert_eq!(allocation.id, id);
		assert_eq!(allocation.state, IJamkb::AllocationState::Approved);
	});
}

#[test]
fn delegatecall_is_rejected_for_every_selector() {
	new_test_ext(0).execute_with(|| {
		let caller = deploy_caller();
		let id = approve_release(100, 10);
		// Booked, so that a `claim` would otherwise succeed.
		pallet_jamkb::Unclaimed::<Test>::insert(&ALICE, 30);
		let beneficiary: [u8; 32] = *ALICE.as_ref();

		for data in [
			IJamkb::executeReleaseCall { allocationId: id }.abi_encode(),
			IJamkb::redeemCall { amount: 50, target: SERVICE, memo: memo_words(MEMO) }.abi_encode(),
			IJamkb::claimCall { beneficiary: beneficiary.into() }.abi_encode(),
			IJamkb::cancelAllocationCall { allocationId: id }.abi_encode(),
			IJamkb::allocationCall { allocationId: id }.abi_encode(),
			IJamkb::operationStateCall { opId: 1 }.abi_encode(),
			IJamkb::claimableOfCall { beneficiary: beneficiary.into() }.abi_encode(),
			IJamkb::currentTimeslotCall {}.abi_encode(),
		] {
			let (success, _) = delegate_call(caller, data);
			assert!(!success, "a DELEGATECALL to the precompile succeeded");
		}
		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Approved);
		assert_eq!(pallet_jamkb::Unclaimed::<Test>::get(&ALICE), 30);

		// Control: the same selectors succeed through a plain CALL.
		for data in [
			IJamkb::claimCall { beneficiary: beneficiary.into() }.abi_encode(),
			IJamkb::allocationCall { allocationId: id }.abi_encode(),
			IJamkb::operationStateCall { opId: 1 }.abi_encode(),
			IJamkb::claimableOfCall { beneficiary: beneficiary.into() }.abi_encode(),
			IJamkb::currentTimeslotCall {}.abi_encode(),
		] {
			let (success, _) = plain_call(caller, data);
			assert!(success, "a plain CALL to the precompile failed");
		}
		assert_eq!(Assets::balance(JAMKB, &ALICE), 30);
		assert_eq!(pallet_jamkb::Unclaimed::<Test>::get(&ALICE), 0);
	});
}

#[test]
fn redeem_burns_the_callers_units() {
	new_test_ext(0).execute_with(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));

		let op = call_ok(
			&ALICE,
			IJamkb::redeemCall { amount: 60, target: SERVICE, memo: memo_words(MEMO) },
		);
		assert_eq!(Assets::balance(JAMKB, &ALICE), 40);
		assert_eq!(Assets::total_supply(JAMKB), CAP - 60);
		match Jamkb::operation(op).unwrap().payload {
			pallet_jamkb::OperationPayload::Redemption { holder, target, amount, memo } => {
				assert_eq!((holder, target, amount, memo), (ALICE, SERVICE, 60, MEMO));
			},
			other => panic!("not a redemption: {other:?}"),
		}

		confirm(op);
		poll();
		assert_eq!(Assets::balance(JAMKB, &ALICE), 40);
		assert_eq!(Assets::total_supply(JAMKB), CAP - 60);
	});
}

#[test]
fn claim_mints_a_voluntary_return_its_beneficiary_could_not_receive() {
	new_test_ext(100).execute_with(|| {
		use frame_support::traits::fungible::Mutate;
		let mut memo = [0u8; 128];
		memo[..32].copy_from_slice(CAROL.as_ref());
		let transfer =
			IncomingTransfer { source: SERVICE, amount: 30, to_supervisor_balance: false, memo };
		assert_eq!(
			<Jamkb as OnIncomingTransfers<Balance>>::on_incoming_transfers(
				vec![transfer].try_into().expect("one transfer fits the bucket"),
				&mut WeightMeter::new()
			),
			Ok(())
		);

		let carol: [u8; 32] = *CAROL.as_ref();
		assert_eq!(call_ok(&ALICE, IJamkb::claimableOfCall { beneficiary: carol.into() }), 30);
		assert_eq!(
			call_revert(&ALICE, IJamkb::claimCall { beneficiary: carol.into() }.abi_encode()),
			"CannotCreate"
		);

		assert_ok!(Balances::mint_into(&CAROL, 1_000_000_000_000));
		assert_eq!(call_ok(&ALICE, IJamkb::claimCall { beneficiary: carol.into() }), 30);
		assert_eq!(Assets::balance(JAMKB, &CAROL), 30);
		assert_eq!(call_ok(&ALICE, IJamkb::claimableOfCall { beneficiary: carol.into() }), 0);
		assert_eq!(
			call_revert(&ALICE, IJamkb::claimCall { beneficiary: carol.into() }.abi_encode()),
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
fn initialize_maps_the_unfunded_custody_account() {
	new_uninitialized_ext().execute_with(|| {
		type Mapper = <Test as pallet_revive::Config>::AddressMapper;
		let custody = Jamkb::custody_account();
		assert_eq!(Balances::total_balance(&custody), 0);
		assert!(!Mapper::is_mapped(&custody));

		assert_ok!(Jamkb::initialize(RuntimeOrigin::root()));

		assert_eq!(Mapper::to_account_id(&Mapper::to_address(&custody)), custody);
		assert_eq!(Balances::total_balance_on_hold(&custody), 0);
		assert_eq!(Balances::total_balance(&custody), 0);
	});
}

#[test]
fn initialize_accepts_a_custody_account_that_is_already_mapped() {
	new_uninitialized_ext().execute_with(|| {
		type Mapper = <Test as pallet_revive::Config>::AddressMapper;
		let custody = Jamkb::custody_account();
		assert_ok!(Mapper::map_no_deposit_unchecked(&custody));

		assert_ok!(Jamkb::initialize(RuntimeOrigin::root()));
		assert_eq!(Mapper::to_account_id(&Mapper::to_address(&custody)), custody);
	});
}

#[test]
fn current_timeslot_is_the_pallets() {
	new_test_ext(0).execute_with(|| {
		CurrentTimeslot::set(77);
		assert_eq!(call_ok(&ALICE, IJamkb::currentTimeslotCall {}), 77);
	});
}
