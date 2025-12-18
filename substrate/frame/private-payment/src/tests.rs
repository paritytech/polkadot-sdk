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

//! Tests for the private payment pallet.

use crate::{mock::*, *};
use frame_support::{assert_noop, assert_ok};

fn coin_key(n: u8) -> PublicKey {
	let mut key = [0u8; 32];
	key[0] = n;
	key
}

fn member_key(n: u8) -> MemberKey {
	let mut key = [0u8; 32];
	key[0] = n;
	key
}

/// Helper to insert a coin directly for testing.
fn insert_test_coin(owner: PublicKey, exponent: CoinExponent, age: u16) {
	CoinsByOwner::<Test>::insert(owner, Coin { value_exponent: exponent, age });
}

#[test]
fn transfer_works() {
	new_test_ext().execute_with(|| {
		let from = coin_key(1);
		let to = coin_key(2);

		// Insert a coin with age 0
		insert_test_coin(from, 5, 0);

		// Transfer should work
		assert_ok!(PrivatePayment::transfer(RuntimeOrigin::signed(1), from, to));

		// Check coin moved and age incremented
		assert!(CoinsByOwner::<Test>::get(from).is_none());
		let coin = CoinsByOwner::<Test>::get(to).unwrap();
		assert_eq!(coin.value_exponent, 5);
		assert_eq!(coin.age, 1);
	});
}

#[test]
fn transfer_fails_for_nonexistent_coin() {
	new_test_ext().execute_with(|| {
		let from = coin_key(1);
		let to = coin_key(2);

		assert_noop!(
			PrivatePayment::transfer(RuntimeOrigin::signed(1), from, to),
			Error::<Test>::CoinNotFound
		);
	});
}

#[test]
fn transfer_fails_for_old_coin() {
	new_test_ext().execute_with(|| {
		let from = coin_key(1);
		let to = coin_key(2);

		// Insert a coin at max age
		insert_test_coin(from, 5, 10);

		assert_noop!(
			PrivatePayment::transfer(RuntimeOrigin::signed(1), from, to),
			Error::<Test>::CoinTooOldToTransfer
		);
	});
}

#[test]
fn split_works() {
	new_test_ext().execute_with(|| {
		let from = coin_key(1);
		let to1 = coin_key(2);
		let to2 = coin_key(3);

		// Insert a $0.04 coin (exponent 2)
		insert_test_coin(from, 2, 0);

		// Split into two $0.02 coins (exponent 1)
		assert_ok!(PrivatePayment::split(
			RuntimeOrigin::signed(1),
			from,
			vec![(1, vec![to1, to2])]
		));

		// Check original removed
		assert!(CoinsByOwner::<Test>::get(from).is_none());

		// Check new coins created with incremented age
		let coin1 = CoinsByOwner::<Test>::get(to1).unwrap();
		assert_eq!(coin1.value_exponent, 1);
		assert_eq!(coin1.age, 1);

		let coin2 = CoinsByOwner::<Test>::get(to2).unwrap();
		assert_eq!(coin2.value_exponent, 1);
		assert_eq!(coin2.age, 1);
	});
}

#[test]
fn split_fails_with_invalid_sum() {
	new_test_ext().execute_with(|| {
		let from = coin_key(1);
		let to1 = coin_key(2);

		// Insert a $0.04 coin (exponent 2)
		insert_test_coin(from, 2, 0);

		// Try to split into one $0.02 coin (value mismatch)
		assert_noop!(
			PrivatePayment::split(RuntimeOrigin::signed(1), from, vec![(1, vec![to1])]),
			Error::<Test>::InvalidSplitAmount
		);
	});
}

#[test]
fn load_recycler_with_coin_works() {
	new_test_ext().execute_with(|| {
		let coin = coin_key(1);
		let member = member_key(1);

		// Insert a coin with sufficient age
		insert_test_coin(coin, 5, 5);

		assert_ok!(PrivatePayment::load_recycler_with_coin(RuntimeOrigin::signed(1), coin, member));

		// Coin should be removed
		assert!(CoinsByOwner::<Test>::get(coin).is_none());

		// Voucher should be created
		assert!(RecyclerVouchers::<Test>::get(member).is_some());
	});
}

#[test]
fn load_recycler_fails_for_young_coin() {
	new_test_ext().execute_with(|| {
		let coin = coin_key(1);
		let member = member_key(1);

		// Insert a coin with age 0 (below minimum)
		insert_test_coin(coin, 5, 0);

		assert_noop!(
			PrivatePayment::load_recycler_with_coin(RuntimeOrigin::signed(1), coin, member),
			Error::<Test>::CoinTooYoungToRecycle
		);
	});
}

#[test]
fn load_recycler_with_external_asset_works() {
	new_test_ext().execute_with(|| {
		let member = member_key(1);

		assert_ok!(PrivatePayment::load_recycler_with_external_asset(
			RuntimeOrigin::signed(2),
			5,
			member
		));

		// Voucher should be created
		assert!(RecyclerVouchers::<Test>::get(member).is_some());
	});
}

#[test]
fn unload_recycler_into_coin_works() {
	new_test_ext().execute_with(|| {
		let member = member_key(1);
		let dest = coin_key(10);

		// First load the recycler
		let coin = coin_key(1);
		insert_test_coin(coin, 5, 5);
		assert_ok!(PrivatePayment::load_recycler_with_coin(RuntimeOrigin::signed(1), coin, member));

		// Create a claim token
		let claim_token = RecyclerClaimToken::Paid {
			ring_index: 0,
			proof: BoundedVec::try_from(vec![0u8; 32]).unwrap(),
		};

		// Unload into a new coin
		assert_ok!(PrivatePayment::unload_recycler_into_coin(
			RuntimeOrigin::none(),
			claim_token,
			vec![member],
			5,
			0,
			dest
		));

		// New coin should exist with age 0
		let new_coin = CoinsByOwner::<Test>::get(dest).unwrap();
		assert_eq!(new_coin.value_exponent, 5);
		assert_eq!(new_coin.age, 0);
	});
}

#[test]
fn unload_recycler_consolidates_vouchers() {
	new_test_ext().execute_with(|| {
		let member1 = member_key(1);
		let member2 = member_key(2);
		let dest = coin_key(10);

		// Load two coins into the recycler
		let coin1 = coin_key(1);
		let coin2 = coin_key(2);
		insert_test_coin(coin1, 5, 5);
		insert_test_coin(coin2, 5, 5);

		assert_ok!(PrivatePayment::load_recycler_with_coin(
			RuntimeOrigin::signed(1),
			coin1,
			member1
		));
		assert_ok!(PrivatePayment::load_recycler_with_coin(
			RuntimeOrigin::signed(1),
			coin2,
			member2
		));

		// Create a claim token
		let claim_token = RecyclerClaimToken::Paid {
			ring_index: 0,
			proof: BoundedVec::try_from(vec![0u8; 32]).unwrap(),
		};

		// Unload 2 vouchers into one coin (consolidation)
		assert_ok!(PrivatePayment::unload_recycler_into_coin(
			RuntimeOrigin::none(),
			claim_token,
			vec![member1, member2],
			5,
			0,
			dest
		));

		// New coin should have exponent 6 (2 * 2^5 = 2^6)
		let new_coin = CoinsByOwner::<Test>::get(dest).unwrap();
		assert_eq!(new_coin.value_exponent, 6);
		assert_eq!(new_coin.age, 0);
	});
}

#[test]
fn pay_for_claim_token_in_coin_works() {
	new_test_ext().execute_with(|| {
		let coin = coin_key(1);
		let member = member_key(1);

		insert_test_coin(coin, 5, 0);

		assert_ok!(PrivatePayment::pay_for_recycler_claim_token_in_coin(
			RuntimeOrigin::signed(1),
			coin,
			member
		));

		// Coin should be burned
		assert!(CoinsByOwner::<Test>::get(coin).is_none());

		// Member key should be in paid token ring
		assert!(PaidTokenRing::<Test>::get().contains(&member));
	});
}

#[test]
fn claim_token_cannot_be_reused() {
	new_test_ext().execute_with(|| {
		let member = member_key(1);
		let dest1 = coin_key(10);
		let dest2 = coin_key(11);

		// Load the recycler
		let coin = coin_key(1);
		insert_test_coin(coin, 5, 5);
		assert_ok!(PrivatePayment::load_recycler_with_coin(RuntimeOrigin::signed(1), coin, member));

		// Create a claim token
		let claim_token = RecyclerClaimToken::Paid {
			ring_index: 0,
			proof: BoundedVec::try_from(vec![0u8; 32]).unwrap(),
		};

		// First use should succeed
		assert_ok!(PrivatePayment::unload_recycler_into_coin(
			RuntimeOrigin::none(),
			claim_token.clone(),
			vec![member],
			5,
			0,
			dest1
		));

		// Load another voucher
		let member2 = member_key(2);
		let coin2 = coin_key(2);
		insert_test_coin(coin2, 5, 5);
		assert_ok!(PrivatePayment::load_recycler_with_coin(
			RuntimeOrigin::signed(1),
			coin2,
			member2
		));

		// Second use should fail
		assert_noop!(
			PrivatePayment::unload_recycler_into_coin(
				RuntimeOrigin::none(),
				claim_token,
				vec![member2],
				5,
				0,
				dest2
			),
			Error::<Test>::ClaimTokenAlreadyUsed
		);
	});
}
