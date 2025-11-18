// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.
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
	mock::*,
	IPreimage::{self},
};
use frame_support::{
	assert_noop, assert_ok,
	traits::{fungible::InspectHold, PreimageProvider},
};
use pallet_revive::{
	precompiles::alloy::{hex, sol_types::SolInterface},
	ExecConfig, ExecReturnValue, Weight, H160, U256,
};

fn call_precompile(
	from: AccountId,
	encoded_call: Vec<u8>,
) -> Result<ExecReturnValue, sp_runtime::DispatchError> {
	let precompile_addr = H160::from(
		hex::const_decode_to_array(b"00000000000000000000000000000000000D0000").unwrap(),
	);

	let result = pallet_revive::Pallet::<Test>::bare_call(
		RuntimeOrigin::signed(from),
		precompile_addr,
		U256::zero(),
		Weight::MAX,
		u128::MAX,
		encoded_call,
		ExecConfig::new_substrate_tx(),
	);

	return result.result
}

fn call_and_check_success(from: AccountId, encoded_call: Vec<u8>) -> bool {
	let return_value = match call_precompile(from, encoded_call) {
		Ok(value) => value,
		Err(err) => panic!("PreimagePrecompile call failed with error: {err:?}"),
	};
	!return_value.did_revert()
}

fn call_and_expect_revert(from: AccountId, encoded_call: Vec<u8>) -> bool {
	let result = call_precompile(from, encoded_call);
	match result {
		Ok(value) => value.did_revert(),
		Err(_) => true,
	}
}

fn encode_note_preimage_call(preimage: Vec<u8>) -> Vec<u8> {
	let call_params = IPreimage::notePreimageCall { preImage: preimage.into() };
	let call = IPreimage::IPreimageCalls::notePreimage(call_params);
	call.abi_encode()
}

fn encode_unnote_preimage_call(hash: [u8; 32]) -> Vec<u8> {
	let call_params = IPreimage::unnotePreimageCall { hash: hash.into() };
	let call = IPreimage::IPreimageCalls::unnotePreimage(call_params);
	call.abi_encode()
}

#[test]
fn user_note_preimage_works() {
	new_test_ext().execute_with(|| {
		let preimage = vec![1u8];
		let hash = hashed(&preimage);

		let encoded_call = encode_note_preimage_call(preimage.clone());
		assert!(call_and_check_success(ALICE, encoded_call.clone()));
		assert_eq!(Balances::balance_on_hold(&PreimageHoldReason::get(), &ALICE), 3);

		assert!(Preimage::have_preimage(&hash));
		assert_eq!(Preimage::get_preimage(&hash), Some(preimage));

		// Already noted error
		assert!(call_and_expect_revert(ALICE, encoded_call.clone()));

		// Insufficient funds error
		assert!(call_and_expect_revert(CHARLIE, encoded_call));
	})
}

#[test]
fn manager_note_preimage_works() {
	new_test_ext().execute_with(|| {
		let preimage = vec![1u8];
		let hash = hashed(&preimage);

		let encoded_call = encode_note_preimage_call(preimage.clone());
		assert!(call_and_check_success(BOB, encoded_call.clone()));

		assert_eq!(Balances::reserved_balance(BOB), 0);

		assert!(Preimage::have_preimage(&hash));
		assert_eq!(Preimage::get_preimage(&hash), Some(preimage));

		assert!(call_and_check_success(BOB, encoded_call));
	});
}

#[test]
fn user_unnote_preimage_works() {
	new_test_ext().execute_with(|| {
		let preimage = vec![1u8];
		let hash = hashed(&preimage);

		let encoded_note_call = encode_note_preimage_call(preimage.clone());
		assert!(call_and_check_success(ALICE, encoded_note_call.clone()));

		let encoded_call = encode_unnote_preimage_call(hash.into());

		// Not authorized error
		assert!(call_and_expect_revert(CHARLIE, encoded_call.clone()));

		// Not noted error
		let Not noted_hash: [u8; 32] = hashed([2u8]).into();
		let invalid_encoded_unnote_call = encode_unnote_preimage_call(Not noted_hash);
		assert!(call_and_expect_revert(ALICE, invalid_encoded_unnote_call));

		assert!(call_and_check_success(ALICE, encoded_call.clone()));

		// Not noted error
		assert!(call_and_expect_revert(ALICE, encoded_call));

		assert!(!Preimage::have_preimage(&hash));
		assert_eq!(Preimage::get_preimage(&hash), None);
	});
}

#[test]
fn manager_unnote_preimage_works() {
	new_test_ext().execute_with(|| {
		let preimage = vec![1u8];
		let hash = hashed(&preimage);

		let encoded_note_call = encode_note_preimage_call(preimage.clone());
		assert!(call_and_check_success(BOB, encoded_note_call.clone()));

		let encoded_call = encode_unnote_preimage_call(hash.into());
		assert!(call_and_check_success(BOB, encoded_call.clone()));

		// Not noted error
		assert!(call_and_expect_revert(BOB, encoded_call));

		assert!(!Preimage::have_preimage(&hash));
		assert_eq!(Preimage::get_preimage(&hash), None);
	});
}

#[test]
fn manager_unnote_user_preimage_works() {
	new_test_ext().execute_with(|| {
		let preimage = vec![1u8];
		let hash = hashed(&preimage);

		let encoded_note_call = encode_note_preimage_call(preimage.clone());
		assert!(call_and_check_success(ALICE, encoded_note_call.clone()));

		let encoded_call = encode_unnote_preimage_call(hash.into());
		assert!(call_and_check_success(BOB, encoded_call.clone()));

		assert!(!Preimage::have_preimage(&hash));
		assert_eq!(Preimage::get_preimage(&hash), None);
	});
}

#[test]
fn requested_then_user_noted_preimage_is_free() {
	new_test_ext().execute_with(|| {
		let preimage = vec![1u8];
		let hash = hashed(&preimage);

		let prev_balance = Balances::free_balance(ALICE);

		assert_ok!(Preimage::request_preimage(RuntimeOrigin::signed(BOB), hash));

		let encoded_call = encode_note_preimage_call(preimage.clone());
		assert!(call_and_check_success(ALICE, encoded_call.clone()));

		assert_eq!(Balances::reserved_balance(ALICE), 0);
		assert_eq!(Balances::free_balance(ALICE), prev_balance);

		assert!(Preimage::have_preimage(&hash));
		assert_eq!(Preimage::get_preimage(&hash), Some(preimage));
	});
}
