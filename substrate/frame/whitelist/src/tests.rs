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

// Tests for Whitelist Pallet

use crate::mock::*;
use codec::Encode;
use frame::{
	testing_prelude::*,
	traits::{QueryPreimage, StorePreimage},
};

#[test]
fn test_whitelist_call_and_remove() {
	new_test_ext().execute_with(|| {
		let call = RuntimeCall::System(frame_system::Call::remark { remark: vec![] });
		let encoded_call = call.encode();
		let call_hash = <Test as frame_system::Config>::Hashing::hash(&encoded_call[..]);

		assert_noop!(
			Whitelist::remove_whitelisted_call(RuntimeOrigin::root(), call_hash),
			crate::Error::<Test>::CallIsNotWhitelisted,
		);

		assert_noop!(
			Whitelist::whitelist_call(RuntimeOrigin::signed(1), call_hash),
			DispatchError::BadOrigin,
		);

		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));

		assert!(Preimage::is_requested(&call_hash));

		assert_noop!(
			Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash),
			crate::Error::<Test>::CallAlreadyWhitelisted,
		);

		assert_noop!(
			Whitelist::remove_whitelisted_call(RuntimeOrigin::signed(1), call_hash),
			DispatchError::BadOrigin,
		);

		assert_ok!(Whitelist::remove_whitelisted_call(RuntimeOrigin::root(), call_hash));

		assert!(!Preimage::is_requested(&call_hash));

		assert_noop!(
			Whitelist::remove_whitelisted_call(RuntimeOrigin::root(), call_hash),
			crate::Error::<Test>::CallIsNotWhitelisted,
		);
	});
}

#[test]
fn test_whitelist_call_and_execute() {
	new_test_ext().execute_with(|| {
		let call = RuntimeCall::System(frame_system::Call::remark_with_event { remark: vec![1] });
		let call_weight = call.get_dispatch_info().call_weight;
		let encoded_call = call.encode();
		let call_encoded_len = encoded_call.len() as u32;
		let call_hash = <Test as frame_system::Config>::Hashing::hash(&encoded_call[..]);

		assert_noop!(
			Whitelist::dispatch_whitelisted_call(
				RuntimeOrigin::root(),
				call_hash,
				call_encoded_len,
				call_weight
			),
			crate::Error::<Test>::CallIsNotWhitelisted,
		);

		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));

		assert_noop!(
			Whitelist::dispatch_whitelisted_call(
				RuntimeOrigin::signed(1),
				call_hash,
				call_encoded_len,
				call_weight
			),
			DispatchError::BadOrigin,
		);

		assert_noop!(
			Whitelist::dispatch_whitelisted_call(
				RuntimeOrigin::root(),
				call_hash,
				call_encoded_len,
				call_weight
			),
			crate::Error::<Test>::UnavailablePreImage,
		);

		assert_ok!(Preimage::note(encoded_call.into()));

		assert!(Preimage::is_requested(&call_hash));

		assert_noop!(
			Whitelist::dispatch_whitelisted_call(
				RuntimeOrigin::root(),
				call_hash,
				call_encoded_len,
				call_weight - Weight::from_parts(1, 0)
			),
			crate::Error::<Test>::InvalidCallWeightWitness,
		);

		assert_ok!(Whitelist::dispatch_whitelisted_call(
			RuntimeOrigin::root(),
			call_hash,
			call_encoded_len,
			call_weight
		));

		assert!(!Preimage::is_requested(&call_hash));

		assert_noop!(
			Whitelist::dispatch_whitelisted_call(
				RuntimeOrigin::root(),
				call_hash,
				call_encoded_len,
				call_weight
			),
			crate::Error::<Test>::CallIsNotWhitelisted,
		);
	});
}

#[test]
fn test_whitelist_call_and_execute_failing_call() {
	new_test_ext().execute_with(|| {
		let call = RuntimeCall::Whitelist(crate::Call::dispatch_whitelisted_call {
			call_hash: Default::default(),
			call_encoded_len: Default::default(),
			call_weight_witness: Weight::zero(),
		});
		let call_weight = call.get_dispatch_info().call_weight;
		let encoded_call = call.encode();
		let call_encoded_len = encoded_call.len() as u32;
		let call_hash = <Test as frame_system::Config>::Hashing::hash(&encoded_call[..]);

		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));
		assert_ok!(Preimage::note(encoded_call.into()));
		assert!(Preimage::is_requested(&call_hash));
		assert_ok!(Whitelist::dispatch_whitelisted_call(
			RuntimeOrigin::root(),
			call_hash,
			call_encoded_len,
			call_weight
		));
		assert!(!Preimage::is_requested(&call_hash));
	});
}

#[test]
fn test_whitelist_call_and_execute_without_note_preimage() {
	new_test_ext().execute_with(|| {
		let call = Box::new(RuntimeCall::System(frame_system::Call::remark_with_event {
			remark: vec![1],
		}));
		let call_hash = <Test as frame_system::Config>::Hashing::hash_of(&call);

		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));
		assert!(Preimage::is_requested(&call_hash));

		assert_ok!(Whitelist::dispatch_whitelisted_call_with_preimage(
			RuntimeOrigin::root(),
			call.clone()
		));

		assert!(!Preimage::is_requested(&call_hash));

		assert_noop!(
			Whitelist::dispatch_whitelisted_call_with_preimage(RuntimeOrigin::root(), call),
			crate::Error::<Test>::CallIsNotWhitelisted,
		);
	});
}

#[test]
fn permissionless_dispatch_with_preimage_works() {
	new_test_ext().execute_with(|| {
		let call = Box::new(RuntimeCall::System(frame_system::Call::remark_with_event {
			remark: vec![1],
		}));
		let call_hash = <Test as frame_system::Config>::Hashing::hash_of(&call);

		// Not yet whitelisted: rejected even via `Authorized`.
		assert_noop!(
			Whitelist::dispatch_whitelisted_call_with_preimage(
				RuntimeOrigin::from(frame_system::RawOrigin::Authorized),
				call.clone()
			),
			crate::Error::<Test>::CallIsNotWhitelisted,
		);

		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));

		// Whitelisted: dispatchable via `Authorized`.
		assert_ok!(Whitelist::dispatch_whitelisted_call_with_preimage(
			RuntimeOrigin::from(frame_system::RawOrigin::Authorized),
			call.clone()
		));

		// The hash is consumed; a replay is rejected.
		assert_noop!(
			Whitelist::dispatch_whitelisted_call_with_preimage(
				RuntimeOrigin::from(frame_system::RawOrigin::Authorized),
				call
			),
			crate::Error::<Test>::CallIsNotWhitelisted,
		);
	});
}

#[test]
fn authorize_callback_admits_whitelisted_and_rejects_unknown() {
	use frame::deps::sp_runtime::transaction_validity::{
		InvalidTransaction, TransactionSource, TransactionValidityError,
	};
	new_test_ext().execute_with(|| {
		let call = Box::new(RuntimeCall::System(frame_system::Call::remark { remark: vec![] }));
		let call_hash = <Test as frame_system::Config>::Hashing::hash_of(&call);

		// Not whitelisted: rejected at the pool.
		assert_eq!(
			crate::Pallet::<Test>::authorize_dispatch_whitelisted_call_with_preimage(
				TransactionSource::External,
				&call,
			),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
		);

		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));

		// Whitelisted: admitted, and the validity provides the call hash as its tag.
		let (valid, refund) =
			crate::Pallet::<Test>::authorize_dispatch_whitelisted_call_with_preimage(
				TransactionSource::External,
				&call,
			)
			.expect("whitelisted submission is admitted");
		assert_eq!(valid.provides, vec![call_hash.encode()]);
		assert_eq!(refund, Weight::zero());
	});
}

#[test]
fn authorize_rejects_when_runtime_not_opted_in() {
	use crate::mock::no_auth;
	use frame::deps::sp_runtime::transaction_validity::{
		InvalidTransaction, TransactionSource, TransactionValidityError,
	};
	no_auth::new_test_ext().execute_with(|| {
		let call =
			Box::new(no_auth::RuntimeCall::System(frame_system::Call::remark { remark: vec![] }));
		let call_hash = <no_auth::Runtime as frame_system::Config>::Hashing::hash_of(&call);

		// Hash is whitelisted, so only the opt-in probe can reject.
		assert_ok!(no_auth::Whitelist::whitelist_call(no_auth::RuntimeOrigin::root(), call_hash));

		assert_eq!(
			crate::Pallet::<no_auth::Runtime>::authorize_dispatch_whitelisted_call_with_preimage(
				TransactionSource::External,
				&call,
			),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
		);
	});
}

#[test]
fn test_whitelist_call_and_execute_decode_consumes_all() {
	new_test_ext().execute_with(|| {
		let call = RuntimeCall::System(frame_system::Call::remark_with_event { remark: vec![1] });
		let call_weight = call.get_dispatch_info().call_weight;
		let mut call = call.encode();
		// Appending something does not make the encoded call invalid.
		// This tests that the decode function consumes all data.
		call.extend(call.clone());
		let call_encoded_len = call.len() as u32;

		let call_hash = <Test as frame_system::Config>::Hashing::hash(&call[..]);

		assert_ok!(Preimage::note(call.into()));
		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));

		assert_noop!(
			Whitelist::dispatch_whitelisted_call(
				RuntimeOrigin::root(),
				call_hash,
				call_encoded_len,
				call_weight
			),
			crate::Error::<Test>::UndecodableCall,
		);
	});
}

#[test]
fn authorize_dispatch_whitelisted_call_uses_hash_argument() {
	use frame::deps::sp_runtime::transaction_validity::{
		InvalidTransaction, TransactionSource, TransactionValidityError,
	};
	new_test_ext().execute_with(|| {
		let call_hash = <Test as frame_system::Config>::Hashing::hash_of(&1u8);

		// Unknown hash: rejected at the pool.
		assert_eq!(
			crate::Pallet::<Test>::authorize_dispatch_whitelisted_call(
				TransactionSource::External,
				&call_hash,
				&0,
				&Weight::zero(),
			),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
		);

		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));

		// Whitelisted: admitted with the hash as the validity tag.
		let (valid, _) = crate::Pallet::<Test>::authorize_dispatch_whitelisted_call(
			TransactionSource::External,
			&call_hash,
			&0,
			&Weight::zero(),
		)
		.expect("whitelisted submission is admitted");
		assert_eq!(valid.provides, vec![call_hash.encode()]);
	});
}
