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
use codec::Encode;
use frame_support::{assert_noop, assert_ok, dispatch::GetDispatchInfo};
use sp_runtime::{
	testing::UintAuthorityId,
	traits::{Applyable, Checkable},
	transaction_validity::TransactionSource,
	DispatchResultWithInfo,
};

fn member_key(n: u8) -> MemberKey {
	let mut key = [0u8; 32];
	key[0] = n;
	key
}

/// Helper to insert a coin directly for testing.
fn insert_test_coin(owner: u64, exponent: CoinExponent, age: u16) {
	CoinsByOwner::<Test>::insert(owner, Coin { value_exponent: exponent, age });
}

/// Create a valid signature for the given coin.
/// UintAuthorityId verifies by checking signature.0 == signer.
fn coin_signature(coin: u64) -> UintAuthorityId {
	UintAuthorityId(coin)
}

/// Dispatch a call through the authorize transaction pipeline.
fn dispatch_authorized(
	call: RuntimeCall,
) -> DispatchResultWithInfo<frame_support::dispatch::PostDispatchInfo> {
	let ext = crate::mock::TransactionExtension::new();
	let uxt = UncheckedExtrinsic::new_transaction(call.clone(), ext);
	let checked = Checkable::check(uxt, &frame_system::ChainContext::<Test>::default())
		.expect("check should succeed");
	let info = call.get_dispatch_info();
	let len = call.encoded_size();
	checked
		.validate::<Test>(TransactionSource::External, &info, len)
		.expect("validate should succeed");
	checked.apply::<Test>(&info, len).expect("apply should succeed")
}

#[test]
fn transfer_works() {
	new_test_ext().execute_with(|| {
		let from: u64 = 100;
		let to: u64 = 101;

		// Insert a coin with age 0
		insert_test_coin(from, 5, 0);

		// Transfer should work (authorized by coin signature)
		let call = RuntimeCall::PrivatePayment(crate::Call::transfer {
			coin: from,
			to,
			signature: coin_signature(from),
		});
		assert_ok!(dispatch_authorized(call));

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
		let from: u64 = 100;
		let to: u64 = 101;

		// Authorization fails because coin doesn't exist
		let call = RuntimeCall::PrivatePayment(crate::Call::transfer {
			coin: from,
			to,
			signature: coin_signature(from),
		});
		let ext = crate::mock::TransactionExtension::new();
		let uxt = UncheckedExtrinsic::new_transaction(call.clone(), ext);
		// Check passes for unsigned transactions
		let checked = Checkable::check(uxt, &frame_system::ChainContext::<Test>::default())
			.expect("check should succeed");
		let info = call.get_dispatch_info();
		let len = call.encoded_size();
		// Validate should fail because authorize_coin_call returns error for nonexistent coin
		let result = checked.validate::<Test>(TransactionSource::External, &info, len);
		assert!(result.is_err());
	});
}

#[test]
fn transfer_fails_for_old_coin() {
	new_test_ext().execute_with(|| {
		let from: u64 = 100;
		let to: u64 = 101;

		// Insert a coin at max age
		insert_test_coin(from, 5, 10);

		let call = RuntimeCall::PrivatePayment(crate::Call::transfer {
			coin: from,
			to,
			signature: coin_signature(from),
		});
		// Authorization passes (coin exists), but dispatch should fail
		let result = dispatch_authorized(call);
		assert!(result.is_err());
		assert_eq!(
			result.unwrap_err().error,
			Error::<Test>::CoinTooOldToTransfer.into()
		);
	});
}

#[test]
fn split_works() {
	new_test_ext().execute_with(|| {
		let from: u64 = 100;
		let to1: u64 = 101;
		let to2: u64 = 102;

		// Insert a $0.04 coin (exponent 2)
		insert_test_coin(from, 2, 0);

		// Split into two $0.02 coins (exponent 1) - authorized by coin signature
		let call = RuntimeCall::PrivatePayment(crate::Call::split {
			coin: from,
			split_into: vec![(1, vec![to1, to2])],
			signature: coin_signature(from),
		});
		assert_ok!(dispatch_authorized(call));

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
		let from: u64 = 100;
		let to1: u64 = 101;

		// Insert a $0.04 coin (exponent 2)
		insert_test_coin(from, 2, 0);

		// Try to split into one $0.02 coin (value mismatch)
		let call = RuntimeCall::PrivatePayment(crate::Call::split {
			coin: from,
			split_into: vec![(1, vec![to1])],
			signature: coin_signature(from),
		});
		let result = dispatch_authorized(call);
		assert!(result.is_err());
		assert_eq!(result.unwrap_err().error, Error::<Test>::InvalidSplitAmount.into());
	});
}

#[test]
fn load_recycler_with_coin_works() {
	new_test_ext().execute_with(|| {
		let coin_id: u64 = 100;
		let member = member_key(1);

		// Insert a coin with sufficient age
		insert_test_coin(coin_id, 5, 5);

		// Authorized by coin signature
		let call = RuntimeCall::PrivatePayment(crate::Call::load_recycler_with_coin {
			coin: coin_id,
			member_key: member,
			signature: coin_signature(coin_id),
		});
		assert_ok!(dispatch_authorized(call));

		// Coin should be removed
		assert!(CoinsByOwner::<Test>::get(coin_id).is_none());

		// Voucher should be created
		assert!(RecyclerVouchers::<Test>::get(member).is_some());
	});
}

#[test]
fn load_recycler_fails_for_young_coin() {
	new_test_ext().execute_with(|| {
		let coin_id: u64 = 100;
		let member = member_key(1);

		// Insert a coin with age 0 (below minimum)
		insert_test_coin(coin_id, 5, 0);

		let call = RuntimeCall::PrivatePayment(crate::Call::load_recycler_with_coin {
			coin: coin_id,
			member_key: member,
			signature: coin_signature(coin_id),
		});
		let result = dispatch_authorized(call);
		assert!(result.is_err());
		assert_eq!(result.unwrap_err().error, Error::<Test>::CoinTooYoungToRecycle.into());
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
		let dest: u64 = 200;

		// First load the recycler (authorized by coin signature)
		let coin_id: u64 = 100;
		insert_test_coin(coin_id, 5, 5);
		let call = RuntimeCall::PrivatePayment(crate::Call::load_recycler_with_coin {
			coin: coin_id,
			member_key: member,
			signature: coin_signature(coin_id),
		});
		assert_ok!(dispatch_authorized(call));

		// Create a claim token
		let claim_token = RecyclerClaimToken::Paid {
			ring_index: 0,
			proof: BoundedVec::try_from(vec![0u8; 32]).unwrap(),
		};

		// Unload into a new coin (uses ensure_none, not authorize)
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
		let dest: u64 = 200;

		// Load two coins into the recycler (authorized by coin signatures)
		let coin1: u64 = 100;
		let coin2: u64 = 101;
		insert_test_coin(coin1, 5, 5);
		insert_test_coin(coin2, 5, 5);

		let call1 = RuntimeCall::PrivatePayment(crate::Call::load_recycler_with_coin {
			coin: coin1,
			member_key: member1,
			signature: coin_signature(coin1),
		});
		assert_ok!(dispatch_authorized(call1));

		let call2 = RuntimeCall::PrivatePayment(crate::Call::load_recycler_with_coin {
			coin: coin2,
			member_key: member2,
			signature: coin_signature(coin2),
		});
		assert_ok!(dispatch_authorized(call2));

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
		let coin_id: u64 = 100;
		let member = member_key(1);

		insert_test_coin(coin_id, 5, 0);

		// Authorized by coin signature
		let call = RuntimeCall::PrivatePayment(crate::Call::pay_for_recycler_claim_token_in_coin {
			coin: coin_id,
			member_key: member,
			signature: coin_signature(coin_id),
		});
		assert_ok!(dispatch_authorized(call));

		// Coin should be burned
		assert!(CoinsByOwner::<Test>::get(coin_id).is_none());

		// Member key should be in paid token ring
		assert!(PaidTokenRing::<Test>::get().contains(&member));
	});
}

#[test]
fn claim_token_cannot_be_reused() {
	new_test_ext().execute_with(|| {
		let member = member_key(1);
		let dest1: u64 = 200;
		let dest2: u64 = 201;

		// Load the recycler (authorized by coin signature)
		let coin_id: u64 = 100;
		insert_test_coin(coin_id, 5, 5);
		let call = RuntimeCall::PrivatePayment(crate::Call::load_recycler_with_coin {
			coin: coin_id,
			member_key: member,
			signature: coin_signature(coin_id),
		});
		assert_ok!(dispatch_authorized(call));

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

		// Load another voucher (authorized by coin signature)
		let member2 = member_key(2);
		let coin2: u64 = 101;
		insert_test_coin(coin2, 5, 5);
		let call2 = RuntimeCall::PrivatePayment(crate::Call::load_recycler_with_coin {
			coin: coin2,
			member_key: member2,
			signature: coin_signature(coin2),
		});
		assert_ok!(dispatch_authorized(call2));

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
