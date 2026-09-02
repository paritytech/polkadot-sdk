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

use crate::{mock::*, Event};
use codec::Encode;
use frame::{
	deps::frame_support::dispatch::DispatchInfo,
	testing_prelude::*,
	traits::{QueryPreimage, StorePreimage},
};

fn run_to_block(n: u64) {
	while System::block_number() < n {
		System::set_block_number(System::block_number() + 1);
	}
}

fn events() -> Vec<Event<Test>> {
	let result = System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| if let RuntimeEvent::Whitelist(inner) = e { Some(inner) } else { None })
		.collect::<Vec<_>>();

	System::reset_events();

	result
}

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
		let call = RuntimeCall::System(frame_system::Call::remark { remark: vec![1] });
		let call_weight = call.get_dispatch_info().call_weight;
		let encoded_call = call.encode();
		let call_encoded_len = encoded_call.len() as u32;
		let call_hash = <Test as frame_system::Config>::Hashing::hash(&encoded_call[..]);

		assert_ok!(Whitelist::dispatch_whitelisted_call(
			RuntimeOrigin::root(),
			call_hash,
			call_encoded_len,
			call_weight
		),);

		assert!(events().iter().any(|event| {
			match event {
				Event::<Test>::DispatchDeferred { call_hash: hash } => hash == &call_hash,
				_ => false,
			}
		}));

		assert_noop!(
			Whitelist::dispatch_whitelisted_call(
				RuntimeOrigin::root(),
				call_hash,
				call_encoded_len,
				call_weight
			),
			crate::Error::<Test>::AlreadyDeferred,
		);

		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));

		assert!(Preimage::is_requested(&call_hash));

		// Use signed Origin after dispatch has been defeered
		assert_noop!(
			Whitelist::dispatch_whitelisted_call(
				RuntimeOrigin::signed(1),
				call_hash,
				call_encoded_len,
				call_weight
			),
			crate::Error::<Test>::UnavailablePreImage,
		);

		// Use root after dispatch has been defeered
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

		let post_dispatch_events = events();

		assert!(
			post_dispatch_events.iter().any(|event| {
				matches!(event, Event::<Test>::CallWhitelisted { call_hash: hash } if hash == &call_hash)
			}),
			"Expected CallWhitelisted event"
		);

		assert!(
			post_dispatch_events.iter().any(|event| {
				matches!(
					event,
					Event::<Test>::WhitelistedCallDispatched {
						call_hash: hash,
						result: Ok(_)
					} if hash == &call_hash
				)
			}),
			"Expected WhitelistedCallDispatched with Ok result"
		);

		assert!(!Preimage::is_requested(&call_hash));
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
		let call = Box::new(RuntimeCall::System(frame_system::Call::remark { remark: vec![1] }));
		let call_hash = <Test as frame_system::Config>::Hashing::hash_of(&call);

		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));

		assert!(Preimage::is_requested(&call_hash));

		assert_ok!(Whitelist::dispatch_whitelisted_call_with_preimage(
			RuntimeOrigin::root(),
			call.clone()
		));

		assert!(events().iter().any(|event| {
			match event {
				Event::<Test>::WhitelistedCallDispatched { call_hash: hash, result } => {
					hash == &call_hash && result.is_ok()
				},
				_ => false,
			}
		}));

		assert!(!Preimage::is_requested(&call_hash));

		assert_ok!(Whitelist::dispatch_whitelisted_call_with_preimage(
			RuntimeOrigin::root(),
			call.clone()
		));

		// Deferring via `dispatch_whitelisted_call_with_preimage` no longer notes the preimage, so
		// the deferral itself registers no preimage request. The relayer below still executes the
		// call by re-supplying its bytes, not by fetching a stored preimage.
		assert!(!Preimage::is_requested(&call_hash));

		assert!(events().iter().any(|event| {
			match event {
				Event::<Test>::DispatchDeferred { call_hash: hash } => hash == &call_hash,
				_ => false,
			}
		}));

		// The deferred call must be whitelisted before a relayer may execute it.
		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));

		assert_ok!(Whitelist::dispatch_whitelisted_call_with_preimage(
			RuntimeOrigin::signed(1),
			call
		));

		let emitted_events = events();

		assert!(emitted_events.iter().any(|event| {
			matches!(
				event,
				Event::<Test>::WhitelistedCallDispatched {
					call_hash: hash,
					result: Ok(PostDispatchInfo { actual_weight: None, pays_fee: Pays::Yes })
				} if hash == &call_hash
			)
		}));

		assert!(emitted_events.iter().any(|event| {
			matches!(
				event,
				Event::<Test>::DeferredDispatchExecuted { call_hash: hash, who: 1 }
				if hash == &call_hash
			)
		}));

		assert!(!Preimage::is_requested(&call_hash));
	});
}

#[test]
fn permissionless_dispatch_with_preimage_works() {
	new_test_ext().execute_with(|| {
		let call = Box::new(RuntimeCall::System(frame_system::Call::remark_with_event {
			remark: vec![1],
		}));
		let call_hash = <Test as frame_system::Config>::Hashing::hash_of(&call);

		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));
		assert!(Preimage::is_requested(&call_hash));

		// Whitelisted: dispatchable via `Authorized`.
		let post = Whitelist::dispatch_whitelisted_call_with_preimage(
			RuntimeOrigin::from(frame_system::RawOrigin::Authorized),
			call,
		)
		.expect("whitelisted dispatch succeeds");
		// Unsigned submission: no account to charge, so `Pays::Yes` costs nothing.
		assert_eq!(post.pays_fee, Pays::Yes);

		// The hash is consumed.
		assert!(!crate::WhitelistedCall::<Test>::contains_key(call_hash));
		assert!(!Preimage::is_requested(&call_hash));
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

		// Whitelisted: admitted, tagged by hash, length and weight.
		let (valid, refund) =
			crate::Pallet::<Test>::authorize_dispatch_whitelisted_call_with_preimage(
				TransactionSource::External,
				&call,
			)
			.expect("whitelisted submission is admitted");
		assert_eq!(
			valid.provides,
			vec![(call_hash, call.encoded_size() as u32, call.get_dispatch_info().call_weight)
				.encode()]
		);
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

/// Submit `call` as a general transaction: no signature, no account to charge.
fn charge_transaction_payment_weight() -> Weight {
	use pallet_transaction_payment::WeightInfo;
	<<Test as pallet_transaction_payment::Config>::WeightInfo>::charge_transaction_payment()
}

fn submit_unsigned(
	call: RuntimeCall,
) -> (
	DispatchInfo,
	Result<ValidTransaction, TransactionValidityError>,
	Result<DispatchResultWithPostInfo, TransactionValidityError>,
) {
	use frame::deps::sp_runtime::{
		traits::{Applyable, Checkable},
		transaction_validity::TransactionSource,
	};

	let tx = UncheckedExtrinsic::new_transaction(
		call,
		(
			frame_system::AuthorizeCall::<Test>::new(),
			pallet_transaction_payment::ChargeTransactionPayment::<Test>::from(0),
		),
	);
	let info = tx.get_dispatch_info();
	let len = tx.using_encoded(|e| e.len());

	let checked = Checkable::check(tx, &frame_system::ChainContext::<Test>::default())
		.expect("general transaction has no signature to check");

	let validity = checked.validate::<Test>(TransactionSource::External, &info, len);
	let applied = checked.apply::<Test>(&info, len);

	(info, validity, applied)
}

#[test]
fn unsigned_dispatch_is_authorized_by_the_extension() {
	new_test_ext().execute_with(|| {
		// `force_set_balance` is root-only, so its effect witnesses the inner dispatch origin.
		let call = Box::new(RuntimeCall::Balances(pallet_balances::Call::force_set_balance {
			who: 1,
			new_free: 1000,
		}));
		let call_hash = <Test as frame_system::Config>::Hashing::hash_of(&call);
		let outer = RuntimeCall::Whitelist(crate::Call::dispatch_whitelisted_call_with_preimage {
			call: call.clone(),
		});

		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));

		let (info, validity, applied) = submit_unsigned(outer);

		// The `authorize` callback ran through the extension.
		assert_eq!(
			validity.expect("whitelisted submission is admitted").provides,
			vec![(call_hash, call.encoded_size() as u32, call.get_dispatch_info().call_weight)
				.encode()]
		);
		// `None` became `Authorized`, so the body takes the privileged branch.
		assert_ok!(applied.expect("transaction is valid"));
		assert!(events().iter().any(|event| {
			matches!(
				event,
				Event::<Test>::WhitelistedCallDispatched { call_hash: hash, result: Ok(_) }
				if hash == &call_hash
			)
		}));
		assert_eq!(Balances::free_balance(1), 1000);
		assert!(!crate::WhitelistedCall::<Test>::contains_key(call_hash));

		// Weighed by `weight_of_authorize`, on top of the payment extension.
		assert_eq!(
			info.extension_weight,
			<() as crate::WeightInfo>::authorize_dispatch_whitelisted_call_with_preimage(
				call.encoded_size() as u32
			)
			.saturating_add(charge_transaction_payment_weight()),
		);
	});
}

#[test]
fn unsigned_dispatch_with_hash_is_authorized_by_the_extension() {
	new_test_ext().execute_with(|| {
		let call = RuntimeCall::Balances(pallet_balances::Call::force_set_balance {
			who: 1,
			new_free: 1000,
		});
		let call_weight = call.get_dispatch_info().call_weight;
		let encoded = call.encode();
		let call_encoded_len = encoded.len() as u32;
		let call_hash = <Test as frame_system::Config>::Hashing::hash(&encoded[..]);

		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));
		assert_ok!(Preimage::note(encoded.into()));

		let outer = RuntimeCall::Whitelist(crate::Call::dispatch_whitelisted_call {
			call_hash,
			call_encoded_len,
			call_weight_witness: call_weight,
		});
		let (info, validity, applied) = submit_unsigned(outer);

		assert_eq!(
			validity.expect("whitelisted submission is admitted").provides,
			vec![(call_hash, call_encoded_len, call_weight).encode()]
		);
		assert_ok!(applied.expect("transaction is valid"));
		assert_eq!(Balances::free_balance(1), 1000);
		assert!(!crate::WhitelistedCall::<Test>::contains_key(call_hash));

		assert_eq!(
			info.extension_weight,
			<() as crate::WeightInfo>::authorize_dispatch_whitelisted_call(call_encoded_len)
				.saturating_add(charge_transaction_payment_weight()),
		);
	});
}

#[test]
fn unsigned_dispatch_with_a_low_weight_witness_is_rejected_at_the_pool() {
	use frame::deps::sp_runtime::transaction_validity::{
		InvalidTransaction, TransactionValidityError,
	};
	new_test_ext().execute_with(|| {
		let call = RuntimeCall::System(frame_system::Call::remark { remark: vec![1] });
		let call_weight = call.get_dispatch_info().call_weight;
		let encoded = call.encode();
		let call_encoded_len = encoded.len() as u32;
		let call_hash = <Test as frame_system::Config>::Hashing::hash(&encoded[..]);

		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));
		assert_ok!(Preimage::note(encoded.into()));

		let outer = RuntimeCall::Whitelist(crate::Call::dispatch_whitelisted_call {
			call_hash,
			call_encoded_len,
			call_weight_witness: call_weight - Weight::from_parts(1, 0),
		});

		// Never enters the pool, so it cannot occupy a block with a free in-block failure.
		let (_, validity, applied) = submit_unsigned(outer);
		assert_eq!(validity, Err(TransactionValidityError::Invalid(InvalidTransaction::Call)));
		assert_eq!(applied, Err(TransactionValidityError::Invalid(InvalidTransaction::Call)));
		assert!(crate::WhitelistedCall::<Test>::contains_key(call_hash));
	});
}

#[test]
fn permissionless_submission_never_creates_a_deferral() {
	use frame::deps::sp_runtime::transaction_validity::{
		InvalidTransaction, TransactionValidityError,
	};
	new_test_ext().execute_with(|| {
		let call = RuntimeCall::System(frame_system::Call::remark { remark: vec![3] });
		let call_weight = call.get_dispatch_info().call_weight;
		let encoded = call.encode();
		let call_encoded_len = encoded.len() as u32;
		let call_hash = <Test as frame_system::Config>::Hashing::hash(&encoded[..]);

		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));
		assert_ok!(Preimage::note(encoded.into()));

		let outer = || {
			RuntimeCall::Whitelist(crate::Call::dispatch_whitelisted_call {
				call_hash,
				call_encoded_len,
				call_weight_witness: call_weight,
			})
		};

		let (_, _, applied) = submit_unsigned(outer());
		assert_ok!(applied.expect("transaction is valid"));
		assert!(!crate::WhitelistedCall::<Test>::contains_key(call_hash));

		// Dispatching drops the preimage too, so re-note it to leave the whitelist check as the
		// only gate on the second submission.
		assert_ok!(Preimage::note(call.encode().into()));
		assert_eq!(<Preimage as QueryPreimage>::len(&call_hash), Some(call_encoded_len));

		// A second submission in the same block re-runs `authorize` through `apply`, so it is
		// rejected on the consumed hash rather than reaching the deferral arm of the body.
		let (_, validity, applied) = submit_unsigned(outer());
		let rejected = TransactionValidityError::Invalid(InvalidTransaction::Call);
		assert_eq!(validity, Err(rejected));
		assert_eq!(applied, Err(rejected));
		assert!(!crate::DeferredDispatch::<Test>::contains_key(call_hash));

		// The privileged origin does take that arm, so the assertion above is not vacuous.
		assert_ok!(Whitelist::dispatch_whitelisted_call(
			RuntimeOrigin::root(),
			call_hash,
			call_encoded_len,
			call_weight
		));
		assert!(crate::DeferredDispatch::<Test>::contains_key(call_hash));
	});
}

#[test]
fn authorized_dispatch_is_free_but_the_privileged_and_relayer_paths_are_not() {
	new_test_ext().execute_with(|| {
		let call = Box::new(RuntimeCall::System(frame_system::Call::remark { remark: vec![7] }));
		let call_hash = <Test as frame_system::Config>::Hashing::hash_of(&call);
		let outer = RuntimeCall::Whitelist(crate::Call::dispatch_whitelisted_call_with_preimage {
			call: call.clone(),
		});

		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 1, 1_000_000));
		let issuance = Balances::total_issuance();

		// The body reports `Pays::Yes`, but an `Authorized` origin has no signer, so
		// `ChargeTransactionPayment` withdraws nothing.
		let (_, _, applied) = submit_unsigned(outer);
		let post = applied.expect("transaction is valid").expect("dispatch succeeds");
		assert_eq!(post.pays_fee, Pays::Yes);
		assert_eq!(Balances::total_issuance(), issuance);
		assert_eq!(Balances::free_balance(1), 1_000_000);

		// The relayer path is explicitly free, the privileged path is not.
		assert_eq!(
			Whitelist::dispatch_whitelisted_call_with_preimage(RuntimeOrigin::root(), call.clone())
				.expect("deferral succeeds")
				.pays_fee,
			Pays::Yes,
		);
		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));
		assert_eq!(
			Whitelist::dispatch_whitelisted_call_with_preimage(RuntimeOrigin::signed(1), call)
				.expect("relayed dispatch succeeds")
				.pays_fee,
			Pays::No,
		);
	});
}

#[test]
fn unsigned_dispatch_of_unwhitelisted_call_is_rejected_at_the_pool() {
	use frame::deps::sp_runtime::transaction_validity::{
		InvalidTransaction, TransactionValidityError,
	};
	new_test_ext().execute_with(|| {
		let call = Box::new(RuntimeCall::System(frame_system::Call::remark { remark: vec![4] }));
		let outer =
			RuntimeCall::Whitelist(crate::Call::dispatch_whitelisted_call_with_preimage { call });

		// Never whitelisted: rejected at the pool, never applied.
		let (_, validity, applied) = submit_unsigned(outer);
		assert_eq!(validity, Err(TransactionValidityError::Invalid(InvalidTransaction::Call)));
		assert_eq!(applied, Err(TransactionValidityError::Invalid(InvalidTransaction::Call)));
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
		let call = RuntimeCall::System(frame_system::Call::remark { remark: vec![1] });
		let call_weight = call.get_dispatch_info().call_weight;
		let encoded = call.encode();
		let call_encoded_len = encoded.len() as u32;
		let call_hash = <Test as frame_system::Config>::Hashing::hash(&encoded[..]);

		let authorize = |len| {
			crate::Pallet::<Test>::authorize_dispatch_whitelisted_call(
				TransactionSource::External,
				&call_hash,
				&len,
				&call_weight,
			)
		};
		let rejected = Err(TransactionValidityError::Invalid(InvalidTransaction::Call));

		// Hash not whitelisted: rejected at the pool.
		assert_eq!(authorize(call_encoded_len), rejected);

		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));

		// Whitelisted but no preimage: rejected.
		assert_eq!(authorize(call_encoded_len), rejected);

		assert_ok!(Preimage::note(encoded.into()));

		// Wrong length witness: rejected.
		assert_eq!(authorize(call_encoded_len + 1), rejected);

		// Whitelisted with preimage: admitted, tagged by hash, length and weight.
		let (valid, _) = authorize(call_encoded_len).expect("whitelisted submission is admitted");
		assert_eq!(valid.provides, vec![(call_hash, call_encoded_len, call_weight).encode()]);
	});
}

#[test]
fn mis_witnessed_dispatch_is_rejected_at_the_pool() {
	use frame::deps::sp_runtime::transaction_validity::{
		InvalidTransaction, TransactionSource, TransactionValidityError,
	};
	new_test_ext().execute_with(|| {
		let call = RuntimeCall::System(frame_system::Call::remark { remark: vec![1] });
		let call_weight = call.get_dispatch_info().call_weight;
		let encoded = call.encode();
		let call_encoded_len = encoded.len() as u32;
		let call_hash = <Test as frame_system::Config>::Hashing::hash(&encoded[..]);

		assert_ok!(Preimage::note(encoded.into()));
		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));

		let authorize = |witness| {
			crate::Pallet::<Test>::authorize_dispatch_whitelisted_call(
				TransactionSource::External,
				&call_hash,
				&call_encoded_len,
				&witness,
			)
		};

		// A witness below the call weight would fail in-block with nobody charged, so it is
		// rejected here instead.
		assert_eq!(
			authorize(call_weight - Weight::from_parts(1, 0)),
			Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
		);

		// The exact witness is admitted, and so is an over-witnessed one.
		let honest = authorize(call_weight).expect("whitelisted submission is admitted").0.provides;
		let over_witnessed = authorize(call_weight + Weight::from_parts(1, 1))
			.expect("whitelisted submission is admitted")
			.0
			.provides;

		// The tag comes off the decoded call, so an over-witnessed submission cannot claim a
		// slot of its own.
		assert_eq!(over_witnessed, honest);

		// The inline variant reads the same values off the call, so it shares the slot too.
		let (inline, _) = crate::Pallet::<Test>::authorize_dispatch_whitelisted_call_with_preimage(
			TransactionSource::External,
			&Box::new(call),
		)
		.expect("whitelisted submission is admitted");
		assert_eq!(inline.provides, honest);
	});
}

#[test]
fn test_deferred_dispatch_failed_inner_call() {
	new_test_ext().execute_with(|| {
		// This call requires a signed origin, whitelisting dispatches calls with root.
		let call = RuntimeCall::System(frame_system::Call::remark_with_event { remark: vec![1] });
		let call_hash = <Test as frame_system::Config>::Hashing::hash_of(&call);
		let call_len = call.encoded_size() as u32;
		let encoded_call = call.encode();
		let call_weight = call.get_dispatch_info().call_weight;

		run_to_block(1);

		// Defer dispatch (no preimage yet)
		assert_ok!(Whitelist::dispatch_whitelisted_call(
			RuntimeOrigin::root(),
			call_hash,
			call_len,
			call_weight
		));

		assert!(events().iter().any(|event| {
			match event {
				Event::<Test>::DispatchDeferred { call_hash: hash } => hash == &call_hash,
				_ => false,
			}
		}));

		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));

		assert_ok!(Preimage::note(encoded_call.into()));

		assert!(Preimage::is_requested(&call_hash));

		// Try to dispatch with signed origin
		assert_ok!(Whitelist::dispatch_whitelisted_call(
			RuntimeOrigin::signed(1),
			call_hash,
			call_len,
			call_weight,
		));

		let emitted_events = events();

		assert!(emitted_events.iter().any(|event| {
			matches!(event, Event::<Test>::CallWhitelisted { call_hash: hash } if hash == &call_hash)
		}));

		// Dispatches with Root when executing whitelisted calls. However,
		// remark_with_event requires RawOrigin::Signed(_)
		assert!(emitted_events.iter().any(|event| {
			matches!(
				event,
				Event::<Test>::WhitelistedCallDispatched {
					call_hash: hash,
					result: Err(DispatchErrorWithPostInfo { error: DispatchError::BadOrigin, .. })
				} if hash == &call_hash
			)
		}));

		// Even though the inner call failed, the deferred entry was still consumed by the relayer,
		// so `DeferredDispatchExecuted` is emitted regardless of the inner call's outcome.
		assert!(emitted_events.iter().any(|event| {
			matches!(
				event,
				Event::<Test>::DeferredDispatchExecuted { call_hash: hash, who: 1 }
				if hash == &call_hash
			)
		}));
		assert!(!Preimage::is_requested(&call_hash));
	});
}

#[test]
fn test_deferred_dispatch_expires_after_block_delay() {
	new_test_ext().execute_with(|| {
		let call = RuntimeCall::System(frame_system::Call::remark { remark: vec![1] });
		let call_hash = <Test as frame_system::Config>::Hashing::hash_of(&call);
		let call_len = call.encoded_size() as u32;
		let encoded_call = call.encode();
		let call_weight = call.get_dispatch_info().call_weight;

		run_to_block(1);

		// Defer dispatch (no preimage yet)
		assert_ok!(Whitelist::dispatch_whitelisted_call(
			RuntimeOrigin::root(),
			call_hash,
			call_len,
			call_weight
		));

		assert!(events().iter().any(|event| {
			match event {
				Event::<Test>::DispatchDeferred { call_hash: hash } => hash == &call_hash,
				_ => false,
			}
		}));

		run_to_block(16);

		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));

		assert_ok!(Preimage::note(encoded_call.into()));

		assert!(Preimage::is_requested(&call_hash));

		// Signed origin fails with expired error, still will result in dispatch error because the
		// call can only be dispatched by root.
		assert_noop!(
			Whitelist::dispatch_whitelisted_call(
				RuntimeOrigin::signed(1),
				call_hash,
				call_len,
				call_weight
			),
			crate::Error::<Test>::DeferredDispatchExpired
		);

		// Same for preimage variant
		assert_noop!(
			Whitelist::dispatch_whitelisted_call_with_preimage(
				RuntimeOrigin::signed(1),
				Box::new(call.clone())
			),
			crate::Error::<Test>::DeferredDispatchExpired
		);

		// Root can still dispatch directly, provided call has been whitelisted
		assert_ok!(Whitelist::dispatch_whitelisted_call(
			RuntimeOrigin::root(),
			call_hash,
			call_len,
			call_weight
		));

		let emitted_events = events();

		assert!(emitted_events.iter().any(|event| {
			matches!(event, Event::<Test>::CallWhitelisted { call_hash: hash } if hash == &call_hash)
		}));

		assert!(emitted_events.iter().any(|event| {
			matches!(
				event,
				Event::<Test>::WhitelistedCallDispatched {
					call_hash: hash,
					result: Ok(PostDispatchInfo { actual_weight: None, pays_fee: Pays::Yes })
				} if hash == &call_hash
			)
		}));

		assert!(!Preimage::is_requested(&call_hash));
	});
}

#[test]
fn test_deferred_dispatch_with_signed_origin() {
	new_test_ext().execute_with(|| {
		// This is a call that both signed origin and root can call, provided all requirements are
		// met.
		let balance_call = RuntimeCall::Balances(pallet_balances::Call::force_transfer {
			source: 1,
			dest: 2,
			value: 100,
		});

		// Fund source account balance
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 1, 1000));

		let balance_call_hash = <Test as frame_system::Config>::Hashing::hash_of(&balance_call);
		let balance_call_len = balance_call.encoded_size() as u32;
		let balance_encoded_call = balance_call.encode();
		let balance_call_weight = balance_call.get_dispatch_info().call_weight;

		// Initial caller should be root or assigned Origin
		assert_noop!(
			Whitelist::dispatch_whitelisted_call(
				RuntimeOrigin::signed(1),
				balance_call_hash,
				balance_call_len,
				balance_call_weight,
			),
			crate::Error::<Test>::DeferredDispatchNotFound
		);

		assert_ok!(Whitelist::dispatch_whitelisted_call(
			RuntimeOrigin::root(),
			balance_call_hash,
			balance_call_len,
			balance_call_weight,
		));

		assert!(events().iter().any(|event| {
			match event {
				Event::<Test>::DispatchDeferred { call_hash: hash } => hash == &balance_call_hash,
				_ => false,
			}
		}));

		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), balance_call_hash));

		assert_ok!(Preimage::note(balance_encoded_call.into()));

		assert!(Preimage::is_requested(&balance_call_hash));

		// Subsequent call to the same hash can be from any signed origin before the dispatch expiry
		assert_ok!(Whitelist::dispatch_whitelisted_call(
			RuntimeOrigin::signed(4),
			balance_call_hash,
			balance_call_len,
			balance_call_weight,
		));

		let post_dispatch_events = events();

		assert!(post_dispatch_events.iter().any(|event| {
			matches!(event, Event::<Test>::CallWhitelisted { call_hash: hash } if hash == &balance_call_hash)
		}));

		assert!(post_dispatch_events.iter().any(|event| {
			matches!(
				event,
				Event::<Test>::WhitelistedCallDispatched { call_hash: hash, result: Ok(_) }
				if hash == &balance_call_hash
			)
		}));

		assert!(post_dispatch_events.iter().any(|event| {
			matches!(
				event,
				Event::<Test>::DeferredDispatchExecuted { call_hash: hash, who: 4 }
				if hash == &balance_call_hash
			)
		}));

		assert!(!Preimage::is_requested(&balance_call_hash));
	});
}

#[test]
fn remove_deferred_dispatch_works() {
	new_test_ext().execute_with(|| {
		let call =
			Box::new(RuntimeCall::System(frame_system::Call::remark { remark: vec![3u8; 24] }));
		let call_hash = <Test as frame_system::Config>::Hashing::hash_of(&call);

		// Root defers the call; it is never whitelisted.
		assert_ok!(Whitelist::dispatch_whitelisted_call_with_preimage(RuntimeOrigin::root(), call));

		// It cannot be removed until it has expired.
		assert_noop!(
			Whitelist::remove_deferred_dispatch(RuntimeOrigin::signed(1), call_hash),
			crate::Error::<Test>::DeferredDispatchNotExpired
		);

		// Past the expiration window, anyone can permissionlessly clean up the entry, paying no
		// fee.
		run_to_block(System::block_number() + 16);
		let post = Whitelist::remove_deferred_dispatch(RuntimeOrigin::signed(1), call_hash)
			.expect("removal of an expired entry succeeds");
		assert_eq!(post.pays_fee, Pays::No);
		assert!(events().iter().any(|event| matches!(
			event,
			Event::<Test>::DeferredDispatchRemoved { call_hash: hash } if hash == &call_hash
		)));

		// The entry is gone: a second removal finds nothing.
		assert_noop!(
			Whitelist::remove_deferred_dispatch(RuntimeOrigin::signed(1), call_hash),
			crate::Error::<Test>::DeferredDispatchNotFound
		);
	});
}

#[test]
fn relayer_cannot_bypass_unwhitelisting() {
	new_test_ext().execute_with(|| {
		let call =
			Box::new(RuntimeCall::System(frame_system::Call::remark { remark: vec![2u8; 16] }));
		let call_hash = <Test as frame_system::Config>::Hashing::hash_of(&call);

		assert_ok!(Whitelist::dispatch_whitelisted_call_with_preimage(
			RuntimeOrigin::root(),
			call.clone(),
		));
		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));
		assert_ok!(Whitelist::remove_whitelisted_call(RuntimeOrigin::root(), call_hash));

		// The whitelist was revoked, so a relayer can no longer execute the still-deferred call.
		assert_noop!(
			Whitelist::dispatch_whitelisted_call_with_preimage(RuntimeOrigin::signed(1), call),
			crate::Error::<Test>::CallIsNotWhitelisted,
		);
	});
}

#[test]
fn relay_cannot_be_replayed() {
	new_test_ext().execute_with(|| {
		let call =
			Box::new(RuntimeCall::System(frame_system::Call::remark { remark: vec![7u8; 8] }));
		let call_hash = <Test as frame_system::Config>::Hashing::hash_of(&call);

		// Defer, whitelist, then relay once — succeeds and consumes the deferred entry.
		assert_ok!(Whitelist::dispatch_whitelisted_call_with_preimage(
			RuntimeOrigin::root(),
			call.clone(),
		));
		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));
		let post = Whitelist::dispatch_whitelisted_call_with_preimage(
			RuntimeOrigin::signed(1),
			call.clone(),
		)
		.expect("relay succeeds");
		// The relayer is not charged.
		assert_eq!(post.pays_fee, Pays::No);

		// A second relay of the same hash must fail: the authorized dispatch can't be replayed.
		assert_noop!(
			Whitelist::dispatch_whitelisted_call_with_preimage(RuntimeOrigin::signed(1), call),
			crate::Error::<Test>::DeferredDispatchNotFound,
		);
	});
}

#[test]
fn relay_cannot_be_replayed_without_preimage() {
	new_test_ext().execute_with(|| {
		let call =
			Box::new(RuntimeCall::System(frame_system::Call::remark { remark: vec![6u8; 8] }));
		let call_weight = call.get_dispatch_info().call_weight;
		let encoded_call = call.encode();
		let call_encoded_len = encoded_call.len() as u32;
		let call_hash = <Test as frame_system::Config>::Hashing::hash(&encoded_call[..]);

		// Defer via the privileged origin, note the preimage, then whitelist.
		assert_ok!(Whitelist::dispatch_whitelisted_call(
			RuntimeOrigin::root(),
			call_hash,
			call_encoded_len,
			call_weight,
		));
		assert_ok!(Preimage::note(encoded_call.into()));
		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));

		// A signed relayer executes the deferred call once — succeeds and consumes the entry.
		let post = Whitelist::dispatch_whitelisted_call(
			RuntimeOrigin::signed(1),
			call_hash,
			call_encoded_len,
			call_weight,
		)
		.expect("relay succeeds");
		// The relayer is not charged.
		assert_eq!(post.pays_fee, Pays::No);

		// A second relay of the same hash must fail: the authorized dispatch can't be replayed.
		assert_noop!(
			Whitelist::dispatch_whitelisted_call(
				RuntimeOrigin::signed(1),
				call_hash,
				call_encoded_len,
				call_weight,
			),
			crate::Error::<Test>::DeferredDispatchNotFound,
		);
	});
}

#[test]
fn deferred_relay_nets_preimage_request_to_zero() {
	new_test_ext().execute_with(|| {
		let call =
			Box::new(RuntimeCall::System(frame_system::Call::remark { remark: vec![9u8; 12] }));
		let call_hash = <Test as frame_system::Config>::Hashing::hash_of(&call);

		// Deferring does not note or request the preimage.
		assert_ok!(Whitelist::dispatch_whitelisted_call_with_preimage(
			RuntimeOrigin::root(),
			call.clone(),
		));
		assert!(!Preimage::is_requested(&call_hash));

		// Whitelisting adds exactly one request.
		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));
		assert!(Preimage::is_requested(&call_hash));

		// Relaying the deferred call nets the request back to zero with nothing left behind.
		assert_ok!(Whitelist::dispatch_whitelisted_call_with_preimage(
			RuntimeOrigin::signed(1),
			call,
		));
		assert!(!Preimage::is_requested(&call_hash));
		assert_eq!(pallet_preimage::RequestStatusFor::<Test>::iter().count(), 0);
	});
}

#[test]
fn remove_deferred_dispatch_does_not_unwhitelist() {
	new_test_ext().execute_with(|| {
		let call =
			Box::new(RuntimeCall::System(frame_system::Call::remark { remark: vec![5u8; 20] }));
		let call_hash = <Test as frame_system::Config>::Hashing::hash_of(&call);

		// Defer and whitelist the call.
		assert_ok!(Whitelist::dispatch_whitelisted_call_with_preimage(RuntimeOrigin::root(), call));
		assert_ok!(Whitelist::whitelist_call(RuntimeOrigin::root(), call_hash));
		assert!(Preimage::is_requested(&call_hash));

		// Clean up the deferred entry once it has expired.
		run_to_block(System::block_number() + 16);
		assert_ok!(Whitelist::remove_deferred_dispatch(RuntimeOrigin::signed(1), call_hash));

		// Removing a deferred entry only drops the deferral — the whitelist and its preimage
		// request are left intact.
		assert!(!crate::DeferredDispatch::<Test>::contains_key(call_hash));
		assert!(crate::WhitelistedCall::<Test>::contains_key(call_hash));
		assert!(Preimage::is_requested(&call_hash));
	});
}
