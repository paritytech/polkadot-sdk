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

//! Tests for `pallet-registrar-relay`.

use crate::{mock::*, AwaitingFirstHead, Error, Event, PendingCodeUpgrades, PendingRegistrations};
use frame_support::{assert_noop, assert_ok};
use registrar_primitives::{
	FailureReason, MessageToPara, MessageToParaV1, MessageToRelay, MessageToRelayV1, ParaId,
	ParachainRegistrar,
};
use sp_runtime::{
	traits::{BlakeTwo256, Hash},
	transaction_validity::{InvalidTransaction, TransactionSource},
	DispatchError,
};

fn head(len: usize) -> Vec<u8> {
	vec![7u8; len]
}

fn code(len: usize) -> Vec<u8> {
	vec![3u8; len]
}

fn hash_of(code: &[u8]) -> sp_core::H256 {
	BlakeTwo256::hash(code)
}

/// The message id every test registration request carries. An arbitrary value: this side only
/// echoes what the parachain sent.
const MSG_ID: u64 = 5;

/// The message id every test cancellation carries. Distinct from [`MSG_ID`] so the tests notice
/// if a cancellation were answered with the registration's id.
const CANCEL_ID: u64 = 6;

fn register_msg(
	para_id: ParaId,
	head_len: usize,
	code_len: usize,
) -> (MessageToRelay<AccountId>, Vec<u8>) {
	let blob = code(code_len);
	let msg = MessageToRelay::V1(MessageToRelayV1::Register {
		para_id,
		message_id: MSG_ID,
		manager: ALICE,
		genesis_head: head(head_len),
		code_hash: hash_of(&blob),
		code_len: code_len as u32,
	});
	(msg, blob)
}

/// Push a valid registration request through and return the code that will satisfy it.
fn request(para_id: ParaId, head_len: usize, code_len: usize) -> Vec<u8> {
	let (msg, blob) = register_msg(para_id, head_len, code_len);
	assert_ok!(Registrar::receive(RuntimeOrigin::root(), msg));
	blob
}

fn failure_report(para_id: ParaId, reason: FailureReason) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::RegisterResponse {
		para_id,
		message_id: MSG_ID,
		outcome: Err(reason),
	})
}

fn success_report(para_id: ParaId) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::RegisterResponse {
		para_id,
		message_id: MSG_ID,
		outcome: Ok(()),
	})
}

fn cancel_msg(para_id: ParaId) -> MessageToRelay<AccountId> {
	MessageToRelay::V1(MessageToRelayV1::CancelRegistration { para_id, message_id: CANCEL_ID })
}

fn cancel_report(para_id: ParaId, outcome: registrar_primitives::Outcome) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::CancelResponse { para_id, message_id: CANCEL_ID, outcome })
}

/// The message id every test deregistration carries, distinct from the ids above.
const DEREGISTER_ID: u64 = 7;

fn deregister_msg(para_id: ParaId) -> MessageToRelay<AccountId> {
	MessageToRelay::V1(MessageToRelayV1::Deregister { para_id, message_id: DEREGISTER_ID })
}

fn deregister_report(para_id: ParaId, outcome: registrar_primitives::Outcome) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::DeregisterResponse {
		para_id,
		message_id: DEREGISTER_ID,
		outcome,
	})
}

/// Onboard `para_id` for real, so the registry knows it.
fn onboard(para_id: ParaId) {
	let blob = request(para_id, 20, 300);
	assert_ok!(Registrar::apply_authorized_code(
		frame_system::RawOrigin::Authorized.into(),
		para_id,
		blob
	));
	let _ = registrar_events();
	let _ = take_sent();
}

/// The message id every test head update carries.
const HEAD_ID: u64 = 8;

fn set_head_msg(para_id: ParaId, head_len: usize) -> MessageToRelay<AccountId> {
	MessageToRelay::V1(MessageToRelayV1::SetCurrentHead {
		para_id,
		message_id: HEAD_ID,
		head: head(head_len),
	})
}

fn set_head_report(para_id: ParaId, outcome: registrar_primitives::Outcome) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::SetHeadResponse { para_id, message_id: HEAD_ID, outcome })
}

/// Run `authorize_apply_authorized_code` and the dispatch together, the way the node does.
///
/// Returns both verdicts so a test can assert the pool and the block agree.
fn authorize_and_dispatch(
	para_id: ParaId,
	validation_code: Vec<u8>,
) -> (Result<(), InvalidTransaction>, Result<(), DispatchError>) {
	let authorized = Registrar::authorize_apply_authorized_code(
		TransactionSource::External,
		&para_id,
		&validation_code,
	)
	.map(|_| ())
	.map_err(|e| match e {
		sp_runtime::transaction_validity::TransactionValidityError::Invalid(i) => i,
		other => panic!("unexpected validity error: {other:?}"),
	});

	let dispatched = Registrar::apply_authorized_code(
		frame_system::RawOrigin::Authorized.into(),
		para_id,
		validation_code,
	)
	.map(|_| ())
	.map_err(|e| e.error);

	(authorized, dispatched)
}

mod receive_register {
	use super::*;

	#[test]
	fn parks_a_valid_request_and_says_nothing_to_the_parachain_yet() {
		new_test_ext().execute_with(|| {
			let blob = request(PARA_A, 20, 300);

			let pending = PendingRegistrations::<Test>::get(PARA_A).unwrap();
			assert_eq!(pending.manager, ALICE);
			assert_eq!(pending.message_id, MSG_ID);
			assert_eq!(pending.genesis_head.into_inner(), head(20));
			assert_eq!(pending.code_hash, hash_of(&blob));
			assert_eq!(pending.code_len, 300);
			assert_eq!(PendingRegistrations::<Test>::count(), 1);

			assert_eq!(
				registrar_events(),
				vec![Event::RegistrationPending {
					para_id: PARA_A,
					message_id: MSG_ID,
					code_hash: hash_of(&blob),
				}]
			);
			// Nothing is reported until the code actually lands.
			assert!(take_sent().is_empty());
		});
	}

	#[test]
	fn only_the_parachain_may_send_requests() {
		new_test_ext().execute_with(|| {
			let (msg, _) = register_msg(PARA_A, 20, 300);
			assert_noop!(
				Registrar::receive(RuntimeOrigin::signed(ALICE), msg),
				DispatchError::BadOrigin
			);
		});
	}

	#[test]
	fn a_para_the_relay_chain_already_knows_is_rejected_and_reported() {
		new_test_ext().execute_with(|| {
			AlreadyKnown::set(vec![PARA_A]);
			let (msg, _) = register_msg(PARA_A, 20, 300);

			// A business rejection is not an extrinsic failure: erroring would roll back the
			// report and strand the parachain's deposit.
			assert_ok!(Registrar::receive(RuntimeOrigin::root(), msg));

			assert!(PendingRegistrations::<Test>::get(PARA_A).is_none());
			assert_eq!(take_sent(), vec![failure_report(PARA_A, FailureReason::AlreadyRegistered)]);
			assert_eq!(
				registrar_events(),
				vec![Event::RegistrationRejected {
					para_id: PARA_A,
					message_id: MSG_ID,
					reason: FailureReason::AlreadyRegistered,
				}]
			);
		});
	}

	#[test]
	fn a_duplicate_request_is_rejected_and_reported() {
		new_test_ext().execute_with(|| {
			request(PARA_A, 20, 300);
			let _ = registrar_events();
			let _ = take_sent();

			let (msg, _) = register_msg(PARA_A, 20, 300);
			assert_ok!(Registrar::receive(RuntimeOrigin::root(), msg));

			assert_eq!(PendingRegistrations::<Test>::count(), 1);
			assert_eq!(take_sent(), vec![failure_report(PARA_A, FailureReason::AlreadyRegistered)]);
		});
	}

	#[test]
	fn onboarding_bounds_are_enforced_before_any_code_is_uploaded() {
		new_test_ext().execute_with(|| {
			for (head_len, code_len) in
				[(MAX_HEAD_SIZE as usize + 1, 300), (20, MAX_CODE_SIZE as usize + 1), (20, 1)]
			{
				let (msg, _) = register_msg(PARA_A, head_len, code_len);
				assert_ok!(Registrar::receive(RuntimeOrigin::root(), msg));

				assert!(PendingRegistrations::<Test>::get(PARA_A).is_none());
				assert_eq!(
					take_sent(),
					vec![failure_report(PARA_A, FailureReason::InvalidOnboardingData)]
				);
				let _ = registrar_events();
			}
		});
	}

	#[test]
	fn requests_beyond_the_pending_cap_are_rejected_and_reported() {
		new_test_ext().execute_with(|| {
			for para_id in 0..MAX_PENDING {
				request(PARA_A + para_id, 20, 300);
			}
			let _ = registrar_events();
			let _ = take_sent();

			let overflow = PARA_A + MAX_PENDING;
			let (msg, _) = register_msg(overflow, 20, 300);
			assert_ok!(Registrar::receive(RuntimeOrigin::root(), msg));

			assert!(PendingRegistrations::<Test>::get(overflow).is_none());
			assert_eq!(PendingRegistrations::<Test>::count(), MAX_PENDING);
			assert_eq!(take_sent(), vec![failure_report(overflow, FailureReason::TooManyPending)]);
		});
	}
}

mod apply_authorized_code {
	use super::*;

	#[test]
	fn the_right_blob_onboards_the_para_and_reports_success() {
		new_test_ext().execute_with(|| {
			let blob = request(PARA_A, 20, 300);
			let _ = registrar_events();

			let (authorized, dispatched) = authorize_and_dispatch(PARA_A, blob.clone());
			assert_eq!(authorized, Ok(()));
			assert_eq!(dispatched, Ok(()));

			assert_eq!(Onboarded::get(), vec![(PARA_A, ALICE, head(20), blob)]);
			assert!(PendingRegistrations::<Test>::get(PARA_A).is_none());
			assert_eq!(PendingRegistrations::<Test>::count(), 0);
			assert_eq!(take_sent(), vec![success_report(PARA_A)]);
			assert_eq!(
				registrar_events(),
				vec![Event::Registered { para_id: PARA_A, message_id: MSG_ID, manager: ALICE }]
			);
		});
	}

	#[test]
	fn nothing_pending_is_refused_by_both_the_pool_and_the_block() {
		new_test_ext().execute_with(|| {
			let (authorized, dispatched) = authorize_and_dispatch(PARA_A, code(300));

			assert_eq!(
				authorized,
				Err(InvalidTransaction::Custom(Registrar::err_to_code(Error::NothingPending)))
			);
			assert_eq!(dispatched, Err(Error::<Test>::NothingPending.into()));
			assert!(Onboarded::get().is_empty());
		});
	}

	#[test]
	fn a_blob_with_the_wrong_hash_is_refused_by_both() {
		new_test_ext().execute_with(|| {
			request(PARA_A, 20, 300);
			// Same length, different bytes: only the hash check can catch this.
			let impostor = vec![9u8; 300];

			let (authorized, dispatched) = authorize_and_dispatch(PARA_A, impostor);

			assert_eq!(
				authorized,
				Err(InvalidTransaction::Custom(Registrar::err_to_code(Error::CodeHashMismatch)))
			);
			assert_eq!(dispatched, Err(Error::<Test>::CodeHashMismatch.into()));
			assert!(Onboarded::get().is_empty());
			// Still waiting for the real thing.
			assert!(PendingRegistrations::<Test>::get(PARA_A).is_some());
		});
	}

	#[test]
	fn a_blob_of_the_wrong_length_is_refused_by_both() {
		new_test_ext().execute_with(|| {
			request(PARA_A, 20, 300);
			// Shorter than declared, which the length check catches before the hash.
			let short = code(200);

			let (authorized, dispatched) = authorize_and_dispatch(PARA_A, short);

			assert_eq!(
				authorized,
				Err(InvalidTransaction::Custom(Registrar::err_to_code(Error::CodeLenMismatch)))
			);
			assert_eq!(dispatched, Err(Error::<Test>::CodeLenMismatch.into()));
			assert!(Onboarded::get().is_empty());
		});
	}

	#[test]
	fn an_oversized_blob_is_refused_before_it_is_hashed() {
		new_test_ext().execute_with(|| {
			request(PARA_A, 20, 300);

			let (authorized, dispatched) =
				authorize_and_dispatch(PARA_A, code(MAX_CODE_SIZE as usize + 1));

			assert_eq!(
				authorized,
				Err(InvalidTransaction::Custom(Registrar::err_to_code(Error::CodeTooLarge)))
			);
			assert_eq!(dispatched, Err(Error::<Test>::CodeTooLarge.into()));
		});
	}

	#[test]
	fn an_authorization_does_not_go_stale_with_time() {
		new_test_ext().execute_with(|| {
			let blob = request(PARA_A, 20, 300);

			// Nothing expires here, so however long the code takes, it is still the code that was
			// paid for.
			System::set_block_number(System::block_number() + 100_000);

			let (authorized, dispatched) = authorize_and_dispatch(PARA_A, blob);
			assert_eq!(authorized, Ok(()));
			assert_eq!(dispatched, Ok(()));
		});
	}

	#[test]
	fn it_cannot_be_replayed() {
		new_test_ext().execute_with(|| {
			let blob = request(PARA_A, 20, 300);
			assert_eq!(authorize_and_dispatch(PARA_A, blob.clone()).1, Ok(()));

			let (authorized, dispatched) = authorize_and_dispatch(PARA_A, blob);
			assert_eq!(
				authorized,
				Err(InvalidTransaction::Custom(Registrar::err_to_code(Error::NothingPending)))
			);
			assert_eq!(dispatched, Err(Error::<Test>::NothingPending.into()));
			// Onboarded exactly once.
			assert_eq!(Onboarded::get().len(), 1);
		});
	}

	#[test]
	fn a_registrar_failure_leaves_the_request_pending() {
		new_test_ext().execute_with(|| {
			let blob = request(PARA_A, 20, 300);
			RegisterFails::set(true);
			let _ = take_sent();

			assert!(Registrar::apply_authorized_code(
				frame_system::RawOrigin::Authorized.into(),
				PARA_A,
				blob.clone()
			)
			.is_err());

			// Nothing was consumed and nothing was reported, so the user can retry.
			assert!(PendingRegistrations::<Test>::get(PARA_A).is_some());
			assert_eq!(PendingRegistrations::<Test>::count(), 1);
			assert!(take_sent().is_empty());

			RegisterFails::set(false);
			assert_ok!(Registrar::apply_authorized_code(
				frame_system::RawOrigin::Authorized.into(),
				PARA_A,
				blob
			));
		});
	}

	#[test]
	fn a_signed_origin_is_not_an_authorized_one() {
		new_test_ext().execute_with(|| {
			let blob = request(PARA_A, 20, 300);
			assert_noop!(
				Registrar::apply_authorized_code(RuntimeOrigin::signed(ALICE), PARA_A, blob),
				DispatchError::BadOrigin
			);
		});
	}
}

mod receive_cancel_registration {
	use super::*;

	#[test]
	fn drops_the_authorization_and_confirms_it() {
		new_test_ext().execute_with(|| {
			let blob = request(PARA_A, 20, 300);
			let _ = registrar_events();
			let _ = take_sent();

			assert_ok!(Registrar::receive(RuntimeOrigin::root(), cancel_msg(PARA_A)));

			assert!(PendingRegistrations::<Test>::get(PARA_A).is_none());
			assert_eq!(PendingRegistrations::<Test>::count(), 0);
			assert_eq!(take_sent(), vec![cancel_report(PARA_A, Ok(()))]);
			assert_eq!(
				registrar_events(),
				vec![Event::AuthorizationCancelled { para_id: PARA_A, message_id: CANCEL_ID }]
			);

			// And the code can no longer be pushed through.
			let (authorized, dispatched) = authorize_and_dispatch(PARA_A, blob);
			assert_eq!(
				authorized,
				Err(InvalidTransaction::Custom(Registrar::err_to_code(Error::NothingPending)))
			);
			assert_eq!(dispatched, Err(Error::<Test>::NothingPending.into()));
		});
	}

	#[test]
	fn cancelling_one_leaves_the_others_alone_and_frees_capacity() {
		new_test_ext().execute_with(|| {
			for i in 0..MAX_PENDING {
				request(PARA_A + i, 20, 300);
			}
			let _ = take_sent();

			assert_ok!(Registrar::receive(RuntimeOrigin::root(), cancel_msg(PARA_A)));

			assert!(PendingRegistrations::<Test>::get(PARA_A).is_none());
			assert!(PendingRegistrations::<Test>::get(PARA_B).is_some());
			assert_eq!(PendingRegistrations::<Test>::count(), MAX_PENDING - 1);

			// The freed slot is usable again.
			request(PARA_A + MAX_PENDING, 20, 300);
			assert!(PendingRegistrations::<Test>::get(PARA_A + MAX_PENDING).is_some());
		});
	}

	#[test]
	fn a_cancellation_that_lost_the_race_to_the_code_is_refused() {
		new_test_ext().execute_with(|| {
			let blob = request(PARA_A, 20, 300);
			assert_eq!(authorize_and_dispatch(PARA_A, blob).1, Ok(()));
			let _ = registrar_events();
			let _ = take_sent();

			assert_ok!(Registrar::receive(RuntimeOrigin::root(), cancel_msg(PARA_A)));

			// The para is registered, so the deposit is owed and the parachain is told so.
			assert_eq!(
				take_sent(),
				vec![cancel_report(PARA_A, Err(FailureReason::AlreadyRegistered))]
			);
			assert_eq!(
				registrar_events(),
				vec![Event::CancellationRefused { para_id: PARA_A, message_id: CANCEL_ID }]
			);
		});
	}

	#[test]
	fn a_refused_cancellation_still_clears_the_dead_authorization() {
		new_test_ext().execute_with(|| {
			request(PARA_A, 20, 300);
			// The id was taken on the relay chain some other way, so this authorization can never
			// be applied and there is no reason to keep holding a slot for it.
			AlreadyKnown::set(vec![PARA_A]);
			let _ = registrar_events();
			let _ = take_sent();

			assert_ok!(Registrar::receive(RuntimeOrigin::root(), cancel_msg(PARA_A)));

			assert_eq!(PendingRegistrations::<Test>::count(), 0);
			assert_eq!(
				take_sent(),
				vec![cancel_report(PARA_A, Err(FailureReason::AlreadyRegistered))]
			);
			assert_eq!(
				registrar_events(),
				vec![Event::CancellationRefused { para_id: PARA_A, message_id: CANCEL_ID }]
			);
		});
	}

	#[test]
	fn cancelling_nothing_still_gets_an_answer() {
		new_test_ext().execute_with(|| {
			// The request may have been rejected here and the report lost; the parachain still has
			// a deposit to release, so silence would strand it.
			assert_ok!(Registrar::receive(RuntimeOrigin::root(), cancel_msg(PARA_A)));

			assert_eq!(take_sent(), vec![cancel_report(PARA_A, Ok(()))]);
			assert_eq!(
				registrar_events(),
				vec![Event::AuthorizationCancelled { para_id: PARA_A, message_id: CANCEL_ID }]
			);
		});
	}

	#[test]
	fn only_the_parachain_may_cancel() {
		new_test_ext().execute_with(|| {
			request(PARA_A, 20, 300);

			assert_noop!(
				Registrar::receive(RuntimeOrigin::signed(ALICE), cancel_msg(PARA_A)),
				DispatchError::BadOrigin
			);
			assert!(PendingRegistrations::<Test>::get(PARA_A).is_some());
		});
	}
}

mod receive_deregister {
	use super::*;

	#[test]
	fn drops_the_para_and_confirms_it() {
		new_test_ext().execute_with(|| {
			onboard(PARA_A);
			assert!(MockRegistrar::is_registered(PARA_A));
			assert!(AwaitingFirstHead::<Test>::contains_key(PARA_A));

			assert_ok!(Registrar::receive(RuntimeOrigin::root(), deregister_msg(PARA_A)));

			assert!(!MockRegistrar::is_registered(PARA_A));
			// Dropped before its first head, so nothing is left waiting on one.
			assert!(!AwaitingFirstHead::<Test>::contains_key(PARA_A));
			assert_eq!(take_sent(), vec![deregister_report(PARA_A, Ok(()))]);
			assert_eq!(
				registrar_events(),
				vec![Event::Deregistered { para_id: PARA_A, message_id: DEREGISTER_ID }]
			);
		});
	}

	#[test]
	fn a_retry_after_a_lost_answer_is_confirmed_again() {
		new_test_ext().execute_with(|| {
			onboard(PARA_A);
			assert_ok!(Registrar::receive(RuntimeOrigin::root(), deregister_msg(PARA_A)));
			let _ = take_sent();
			let _ = registrar_events();

			assert_ok!(Registrar::receive(RuntimeOrigin::root(), deregister_msg(PARA_A)));

			assert_eq!(take_sent(), vec![deregister_report(PARA_A, Ok(()))]);
			assert_eq!(
				registrar_events(),
				vec![Event::Deregistered { para_id: PARA_A, message_id: DEREGISTER_ID }]
			);
		});
	}

	#[test]
	fn a_registry_that_will_not_let_the_para_go_is_reported_not_dispatch_failed() {
		new_test_ext().execute_with(|| {
			onboard(PARA_A);
			DeregisterFails::set(true);

			// `Ok`, not `Err`: failing would roll the refusal report back with it.
			assert_ok!(Registrar::receive(RuntimeOrigin::root(), deregister_msg(PARA_A)));

			assert!(MockRegistrar::is_registered(PARA_A));
			assert!(AwaitingFirstHead::<Test>::contains_key(PARA_A));
			assert_eq!(
				take_sent(),
				vec![deregister_report(PARA_A, Err(FailureReason::NotDeregisterable))]
			);
			assert_eq!(
				registrar_events(),
				vec![Event::DeregistrationRejected { para_id: PARA_A, message_id: DEREGISTER_ID }]
			);
		});
	}

	#[test]
	fn only_the_parachain_may_ask() {
		new_test_ext().execute_with(|| {
			onboard(PARA_A);

			assert_noop!(
				Registrar::receive(RuntimeOrigin::signed(ALICE), deregister_msg(PARA_A)),
				DispatchError::BadOrigin
			);
			assert!(MockRegistrar::is_registered(PARA_A));
		});
	}
}

mod receive_set_current_head {
	use super::*;

	#[test]
	fn sets_the_head_and_confirms_it() {
		new_test_ext().execute_with(|| {
			AlreadyKnown::set(vec![PARA_A]);

			assert_ok!(Registrar::receive(RuntimeOrigin::root(), set_head_msg(PARA_A, 20)));

			assert_eq!(Heads::get(), vec![(PARA_A, head(20))]);
			assert_eq!(take_sent(), vec![set_head_report(PARA_A, Ok(()))]);
			assert_eq!(
				registrar_events(),
				vec![Event::HeadUpdated { para_id: PARA_A, message_id: HEAD_ID }]
			);
		});
	}

	#[test]
	fn refuses_a_head_the_relay_chain_will_not_take() {
		new_test_ext().execute_with(|| {
			AlreadyKnown::set(vec![PARA_A]);

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				set_head_msg(PARA_A, MAX_HEAD_SIZE as usize + 1)
			));

			assert!(Heads::get().is_empty());
			assert_eq!(
				take_sent(),
				vec![set_head_report(PARA_A, Err(FailureReason::HeadDataTooLarge))]
			);
			assert_eq!(
				registrar_events(),
				vec![Event::HeadUpdateRejected {
					para_id: PARA_A,
					message_id: HEAD_ID,
					reason: FailureReason::HeadDataTooLarge,
				}]
			);

			// The bound itself is fine.
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				set_head_msg(PARA_A, MAX_HEAD_SIZE as usize)
			));
			assert_eq!(Heads::get(), vec![(PARA_A, head(MAX_HEAD_SIZE as usize))]);
		});
	}

	#[test]
	fn refuses_a_head_for_a_para_the_relay_chain_does_not_know() {
		new_test_ext().execute_with(|| {
			assert_ok!(Registrar::receive(RuntimeOrigin::root(), set_head_msg(PARA_A, 20)));

			assert!(Heads::get().is_empty());
			assert_eq!(
				take_sent(),
				vec![set_head_report(PARA_A, Err(FailureReason::NotRegistered))]
			);
			assert_eq!(
				registrar_events(),
				vec![Event::HeadUpdateRejected {
					para_id: PARA_A,
					message_id: HEAD_ID,
					reason: FailureReason::NotRegistered,
				}]
			);
		});
	}

	#[test]
	fn a_head_overtaken_by_the_deregistration_is_refused() {
		new_test_ext().execute_with(|| {
			onboard(PARA_A);
			assert_ok!(Registrar::receive(RuntimeOrigin::root(), deregister_msg(PARA_A)));
			let _ = take_sent();
			let _ = registrar_events();

			assert_ok!(Registrar::receive(RuntimeOrigin::root(), set_head_msg(PARA_A, 20)));

			assert!(Heads::get().is_empty());
			assert_eq!(
				take_sent(),
				vec![set_head_report(PARA_A, Err(FailureReason::NotRegistered))]
			);
			assert_eq!(
				registrar_events(),
				vec![Event::HeadUpdateRejected {
					para_id: PARA_A,
					message_id: HEAD_ID,
					reason: FailureReason::NotRegistered,
				}]
			);
		});
	}

	#[test]
	fn only_the_parachain_may_set_heads() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Registrar::receive(RuntimeOrigin::signed(ALICE), set_head_msg(PARA_A, 20)),
				DispatchError::BadOrigin
			);
			assert!(Heads::get().is_empty());
		});
	}
}

mod reporting {
	use super::*;

	#[test]
	fn a_bounced_report_does_not_undo_the_deregistration() {
		new_test_ext().execute_with(|| {
			onboard(PARA_A);
			SendFails::set(true);

			assert_ok!(Registrar::receive(RuntimeOrigin::root(), deregister_msg(PARA_A)));

			// The para is gone here regardless; the parachain retries and is confirmed then.
			assert!(!MockRegistrar::is_registered(PARA_A));
			assert_eq!(
				registrar_events(),
				vec![
					Event::ReportFailed { para_id: PARA_A, message_id: DEREGISTER_ID },
					Event::Deregistered { para_id: PARA_A, message_id: DEREGISTER_ID },
				]
			);
		});
	}

	#[test]
	fn a_bounced_report_does_not_undo_the_onboarding() {
		new_test_ext().execute_with(|| {
			let blob = request(PARA_A, 20, 300);
			let _ = registrar_events();
			SendFails::set(true);

			assert_ok!(Registrar::apply_authorized_code(
				frame_system::RawOrigin::Authorized.into(),
				PARA_A,
				blob
			));

			// The relay chain's own state is correct and stays that way; the parachain is simply
			// told nothing, and the failure is surfaced.
			assert_eq!(Onboarded::get().len(), 1);
			assert!(PendingRegistrations::<Test>::get(PARA_A).is_none());
			assert_eq!(
				registrar_events(),
				vec![
					Event::ReportFailed { para_id: PARA_A, message_id: MSG_ID },
					Event::Registered { para_id: PARA_A, message_id: MSG_ID, manager: ALICE },
				]
			);
		});
	}
}

mod receive_authorize_code_upgrade {
	use super::*;

	/// The message id every test upgrade request carries, distinct from the registration's.
	const UPGRADE_ID: u64 = 7;

	fn upgrade_msg(para_id: ParaId, code_len: usize) -> (MessageToRelay<AccountId>, Vec<u8>) {
		let blob = code(code_len);
		let msg = MessageToRelay::V1(MessageToRelayV1::AuthorizeCodeUpgrade {
			para_id,
			message_id: UPGRADE_ID,
			code_hash: hash_of(&blob),
			code_len: code_len as u32,
		});
		(msg, blob)
	}

	fn upgrade_report(para_id: ParaId, outcome: Result<u32, FailureReason>) -> MessageToPara {
		MessageToPara::V1(MessageToParaV1::CodeUpgradeResponse {
			para_id,
			message_id: UPGRADE_ID,
			outcome,
		})
	}

	/// Onboard `para_id` and authorize an upgrade for it, returning the code it now expects.
	fn authorize(para_id: ParaId, code_len: usize) -> Vec<u8> {
		AlreadyKnown::mutate(|known| known.push(para_id));
		let (msg, blob) = upgrade_msg(para_id, code_len);
		assert_ok!(Registrar::receive(RuntimeOrigin::root(), msg));
		let _ = take_sent();
		let _ = registrar_events();
		blob
	}

	#[test]
	fn parks_the_authorization_and_tells_the_parachain_when_it_lapses() {
		new_test_ext().execute_with(|| {
			AlreadyKnown::mutate(|known| known.push(PARA_A));
			let (msg, blob) = upgrade_msg(PARA_A, 300);

			assert_ok!(Registrar::receive(RuntimeOrigin::root(), msg));

			let expire_at = 1 + CODE_UPGRADE_VALID_PERIOD;
			let pending = PendingCodeUpgrades::<Test>::get(PARA_A).unwrap();
			assert_eq!(pending.message_id, UPGRADE_ID);
			assert_eq!(pending.code_hash, hash_of(&blob));
			assert_eq!(pending.code_len, 300);
			assert_eq!(pending.expire_at, expire_at);

			assert_eq!(take_sent(), vec![upgrade_report(PARA_A, Ok(expire_at))]);
			assert_eq!(
				registrar_events(),
				vec![Event::CodeUpgradePending {
					para_id: PARA_A,
					message_id: UPGRADE_ID,
					code_hash: hash_of(&blob),
					expire_at,
				}]
			);
		});
	}

	#[test]
	fn a_para_this_chain_does_not_know_is_refused_and_reported() {
		new_test_ext().execute_with(|| {
			let (msg, _) = upgrade_msg(PARA_A, 300);

			// A refusal is not an extrinsic failure.
			assert_ok!(Registrar::receive(RuntimeOrigin::root(), msg));

			assert!(!PendingCodeUpgrades::<Test>::contains_key(PARA_A));
			assert_eq!(
				take_sent(),
				vec![upgrade_report(PARA_A, Err(FailureReason::NotRegistered))]
			);
			assert_eq!(
				registrar_events(),
				vec![Event::CodeUpgradeRejected {
					para_id: PARA_A,
					message_id: UPGRADE_ID,
					reason: FailureReason::NotRegistered,
				}]
			);
		});
	}

	#[test]
	fn the_registrys_refusal_is_reported() {
		new_test_ext().execute_with(|| {
			AlreadyKnown::mutate(|known| known.push(PARA_A));
			UpgradeCheckFails::set(true);
			let (msg, _) = upgrade_msg(PARA_A, 300);

			assert_ok!(Registrar::receive(RuntimeOrigin::root(), msg));

			assert!(!PendingCodeUpgrades::<Test>::contains_key(PARA_A));
			assert_eq!(
				take_sent(),
				vec![upgrade_report(PARA_A, Err(FailureReason::CannotUpgradeCode))]
			);
		});
	}

	#[test]
	fn the_code_size_is_bounded_by_this_pallet_and_by_the_registry() {
		new_test_ext().execute_with(|| {
			AlreadyKnown::mutate(|known| known.push(PARA_A));

			for (code_len, reason) in [
				(MAX_CODE_SIZE as usize + 1, FailureReason::InvalidCodeSize),
				(MIN_CODE_SIZE as usize - 1, FailureReason::CannotUpgradeCode),
			] {
				let (msg, _) = upgrade_msg(PARA_A, code_len);
				assert_ok!(Registrar::receive(RuntimeOrigin::root(), msg));
				assert!(!PendingCodeUpgrades::<Test>::contains_key(PARA_A));
				assert_eq!(take_sent(), vec![upgrade_report(PARA_A, Err(reason))]);
				let _ = registrar_events();
			}
		});
	}

	#[test]
	fn a_second_request_replaces_the_first() {
		new_test_ext().execute_with(|| {
			let _ = authorize(PARA_A, 300);

			System::set_block_number(50);
			let blob = code(400);
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				MessageToRelay::V1(MessageToRelayV1::AuthorizeCodeUpgrade {
					para_id: PARA_A,
					message_id: UPGRADE_ID + 1,
					code_hash: hash_of(&blob),
					code_len: 400,
				}),
			));

			let pending = PendingCodeUpgrades::<Test>::get(PARA_A).unwrap();
			assert_eq!(pending.message_id, UPGRADE_ID + 1);
			assert_eq!(pending.code_hash, hash_of(&blob));
			assert_eq!(pending.code_len, 400);
			assert_eq!(pending.expire_at, 50 + CODE_UPGRADE_VALID_PERIOD);
		});
	}

	#[test]
	fn a_bounced_report_does_not_undo_the_authorization() {
		new_test_ext().execute_with(|| {
			AlreadyKnown::mutate(|known| known.push(PARA_A));
			let (msg, _) = upgrade_msg(PARA_A, 300);
			SendFails::set(true);

			assert_ok!(Registrar::receive(RuntimeOrigin::root(), msg));

			assert!(PendingCodeUpgrades::<Test>::contains_key(PARA_A));
			assert!(registrar_events().iter().any(
				|e| matches!(e, Event::ReportFailed { message_id, .. } if *message_id == UPGRADE_ID)
			));
		});
	}

	#[test]
	fn only_the_parachain_may_authorize() {
		new_test_ext().execute_with(|| {
			let (msg, _) = upgrade_msg(PARA_A, 300);
			assert_noop!(
				Registrar::receive(RuntimeOrigin::signed(ALICE), msg),
				DispatchError::BadOrigin
			);
		});
	}

	/// Run the pool check and the dispatch together, the way the node does.
	fn upgrade_and_dispatch(
		para_id: ParaId,
		validation_code: Vec<u8>,
	) -> (Result<(), InvalidTransaction>, Result<(), DispatchError>) {
		let authorized = Registrar::authorize_apply_authorized_code_upgrade(
			TransactionSource::External,
			&para_id,
			&validation_code,
		)
		.map(|_| ())
		.map_err(|e| match e {
			sp_runtime::transaction_validity::TransactionValidityError::Invalid(i) => i,
			other => panic!("unexpected validity error: {other:?}"),
		});

		let dispatched = Registrar::apply_authorized_code_upgrade(
			frame_system::RawOrigin::Authorized.into(),
			para_id,
			validation_code,
		)
		.map(|_| ())
		.map_err(|e| e.error);

		(authorized, dispatched)
	}

	#[test]
	fn the_right_blob_schedules_the_upgrade_and_reports_it() {
		new_test_ext().execute_with(|| {
			let blob = authorize(PARA_A, 300);

			let (authorized, dispatched) = upgrade_and_dispatch(PARA_A, blob.clone());
			assert_eq!(authorized, Ok(()));
			assert_eq!(dispatched, Ok(()));

			assert_eq!(ScheduledUpgrades::get(), vec![(PARA_A, blob)]);
			assert!(!PendingCodeUpgrades::<Test>::contains_key(PARA_A));
			assert_eq!(
				take_sent(),
				vec![MessageToPara::V1(MessageToParaV1::CodeUpgradeScheduled {
					para_id: PARA_A,
					message_id: UPGRADE_ID,
				})]
			);
			assert_eq!(
				registrar_events(),
				vec![Event::CodeUpgradeApplied { para_id: PARA_A, message_id: UPGRADE_ID }]
			);
		});
	}

	#[test]
	fn a_blob_that_is_not_the_authorized_one_is_refused_by_both() {
		new_test_ext().execute_with(|| {
			let blob = authorize(PARA_A, 300);

			// Nothing authorized for this para at all.
			assert_eq!(
				upgrade_and_dispatch(PARA_B, blob.clone()),
				(
					Err(InvalidTransaction::Custom(Registrar::err_to_code(
						Error::<Test>::NothingPending
					))),
					Err(Error::<Test>::NothingPending.into())
				)
			);

			// Right length, wrong bytes.
			let mut impostor = blob.clone();
			*impostor.last_mut().unwrap() ^= 0xff;
			assert_eq!(
				upgrade_and_dispatch(PARA_A, impostor),
				(
					Err(InvalidTransaction::Custom(Registrar::err_to_code(
						Error::<Test>::CodeHashMismatch
					))),
					Err(Error::<Test>::CodeHashMismatch.into())
				)
			);

			// Right bytes, wrong length.
			assert_eq!(
				upgrade_and_dispatch(PARA_A, blob[..blob.len() - 1].to_vec()),
				(
					Err(InvalidTransaction::Custom(Registrar::err_to_code(
						Error::<Test>::CodeLenMismatch
					))),
					Err(Error::<Test>::CodeLenMismatch.into())
				)
			);

			// Too large to be worth hashing.
			assert_eq!(
				upgrade_and_dispatch(PARA_A, code(MAX_CODE_SIZE as usize + 1)),
				(
					Err(InvalidTransaction::Custom(Registrar::err_to_code(
						Error::<Test>::CodeTooLarge
					))),
					Err(Error::<Test>::CodeTooLarge.into())
				)
			);

			assert!(ScheduledUpgrades::get().is_empty());
			assert!(PendingCodeUpgrades::<Test>::contains_key(PARA_A));
		});
	}

	#[test]
	fn an_authorization_stops_being_applicable_once_it_lapses() {
		new_test_ext().execute_with(|| {
			let blob = authorize(PARA_A, 300);
			let expire_at = PendingCodeUpgrades::<Test>::get(PARA_A).unwrap().expire_at;

			// Still good on the last block.
			System::set_block_number(expire_at);
			assert_eq!(
				Registrar::authorize_apply_authorized_code_upgrade(
					TransactionSource::External,
					&PARA_A,
					&blob,
				)
				.map(|_| ()),
				Ok(())
			);

			System::set_block_number(expire_at + 1);
			assert_eq!(
				upgrade_and_dispatch(PARA_A, blob),
				(
					Err(InvalidTransaction::Custom(Registrar::err_to_code(
						Error::<Test>::AuthorizationExpired
					))),
					Err(Error::<Test>::AuthorizationExpired.into())
				)
			);
			assert!(ScheduledUpgrades::get().is_empty());
		});
	}

	#[test]
	fn a_registry_failure_leaves_the_authorization_in_place() {
		new_test_ext().execute_with(|| {
			let blob = authorize(PARA_A, 300);
			ScheduleFails::set(true);

			assert!(upgrade_and_dispatch(PARA_A, blob.clone()).1.is_err());

			assert!(PendingCodeUpgrades::<Test>::contains_key(PARA_A));
			assert!(take_sent().is_empty());

			// And the upload works once whatever blocked it clears.
			ScheduleFails::set(false);
			assert_eq!(upgrade_and_dispatch(PARA_A, blob).1, Ok(()));
		});
	}

	#[test]
	fn an_upgrade_the_registry_would_refuse_stays_out_of_the_pool() {
		new_test_ext().execute_with(|| {
			let blob = authorize(PARA_A, 300);
			UpgradeCheckFails::set(true);

			assert_eq!(
				upgrade_and_dispatch(PARA_A, blob.clone()),
				(
					Err(InvalidTransaction::Custom(Registrar::err_to_code(
						Error::<Test>::CannotUpgradeCode
					))),
					Err(Error::<Test>::CannotUpgradeCode.into())
				)
			);
			assert!(ScheduledUpgrades::get().is_empty());
			assert!(PendingCodeUpgrades::<Test>::contains_key(PARA_A));

			UpgradeCheckFails::set(false);
			assert_eq!(upgrade_and_dispatch(PARA_A, blob), (Ok(()), Ok(())));
		});
	}

	#[test]
	fn a_deregistration_drops_the_authorization() {
		new_test_ext().execute_with(|| {
			let blob = authorize(PARA_A, 300);

			DeregisterFails::set(true);
			assert_ok!(Registrar::receive(RuntimeOrigin::root(), deregister_msg(PARA_A)));
			assert!(PendingCodeUpgrades::<Test>::contains_key(PARA_A));

			DeregisterFails::set(false);
			assert_ok!(Registrar::receive(RuntimeOrigin::root(), deregister_msg(PARA_A)));
			assert!(!PendingCodeUpgrades::<Test>::contains_key(PARA_A));

			assert_eq!(
				upgrade_and_dispatch(PARA_A, blob).1,
				Err(Error::<Test>::NothingPending.into())
			);
			assert!(ScheduledUpgrades::get().is_empty());
		});
	}

	#[test]
	fn it_cannot_be_replayed() {
		new_test_ext().execute_with(|| {
			let blob = authorize(PARA_A, 300);
			assert_eq!(upgrade_and_dispatch(PARA_A, blob.clone()).1, Ok(()));

			assert_eq!(
				upgrade_and_dispatch(PARA_A, blob).1,
				Err(Error::<Test>::NothingPending.into())
			);
			assert_eq!(ScheduledUpgrades::get().len(), 1);
		});
	}

	#[test]
	fn a_signed_origin_is_not_an_authorized_one() {
		new_test_ext().execute_with(|| {
			let blob = authorize(PARA_A, 300);
			assert_noop!(
				Registrar::apply_authorized_code_upgrade(
					RuntimeOrigin::signed(ALICE),
					PARA_A,
					blob
				),
				DispatchError::BadOrigin
			);
		});
	}
}

mod head_noted {
	use super::*;
	use polkadot_runtime_parachains::paras::OnNewHead;

	/// Leave `para_id` in the state a registration leaves behind: waiting on its first head.
	fn awaiting(para_id: ParaId) {
		AwaitingFirstHead::<Test>::insert(para_id, ());
	}

	#[test]
	fn a_registration_leaves_the_para_awaiting_its_first_head() {
		new_test_ext().execute_with(|| {
			request(PARA_A, 20, 300);

			assert!(AwaitingFirstHead::<Test>::contains_key(PARA_A));
		});
	}

	#[test]
	fn tells_the_parachain_about_a_new_head() {
		new_test_ext().execute_with(|| {
			request(PARA_A, 20, 300);
			let _ = registrar_events();

			Registrar::on_new_head(PARA_A.into(), &Default::default());

			assert_eq!(
				take_sent(),
				vec![MessageToPara::V1(MessageToParaV1::HeadNoted { para_id: PARA_A })]
			);
			assert_eq!(registrar_events(), vec![Event::HeadNoted { para_id: PARA_A }]);
		});
	}

	#[test]
	fn a_para_this_pallet_never_registered_is_skipped() {
		new_test_ext().execute_with(|| {
			// Only paras registered through here are waiting on a notification; every other para
			// on this chain locks itself and must not be told anything.
			assert!(!AwaitingFirstHead::<Test>::contains_key(PARA_A));

			Registrar::on_new_head(PARA_A.into(), &Default::default());

			assert!(take_sent().is_empty());
			assert!(registrar_events().is_empty());
		});
	}

	#[test]
	fn a_later_head_is_not_reported_again() {
		new_test_ext().execute_with(|| {
			awaiting(PARA_A);
			Registrar::on_new_head(PARA_A.into(), &Default::default());
			assert_eq!(take_sent().len(), 1);
			assert_eq!(registrar_events(), vec![Event::HeadNoted { para_id: PARA_A }]);

			Registrar::on_new_head(PARA_A.into(), &Default::default());

			assert!(take_sent().is_empty());
			assert!(registrar_events().is_empty());
		});
	}

	#[test]
	fn each_para_is_reported_once() {
		new_test_ext().execute_with(|| {
			awaiting(PARA_A);
			awaiting(PARA_B);

			Registrar::on_new_head(PARA_A.into(), &Default::default());
			Registrar::on_new_head(PARA_B.into(), &Default::default());

			assert_eq!(
				take_sent(),
				vec![
					MessageToPara::V1(MessageToParaV1::HeadNoted { para_id: PARA_A }),
					MessageToPara::V1(MessageToParaV1::HeadNoted { para_id: PARA_B }),
				]
			);
		});
	}

	#[test]
	fn a_bounced_notification_is_only_surfaced() {
		new_test_ext().execute_with(|| {
			awaiting(PARA_A);
			SendFails::set(true);

			Registrar::on_new_head(PARA_A.into(), &Default::default());

			assert!(take_sent().is_empty());
			assert_eq!(registrar_events(), vec![Event::HeadNoteFailed { para_id: PARA_A }]);
		});
	}

	#[test]
	fn a_bounced_notification_is_retried_on_the_next_head() {
		new_test_ext().execute_with(|| {
			awaiting(PARA_A);
			SendFails::set(true);
			Registrar::on_new_head(PARA_A.into(), &Default::default());

			// A bounced notification keeps the para waiting, otherwise it would never learn about
			// its first head and could stay unlocked forever.
			assert!(AwaitingFirstHead::<Test>::contains_key(PARA_A));
			assert!(take_sent().is_empty());
			assert_eq!(registrar_events(), vec![Event::HeadNoteFailed { para_id: PARA_A }]);

			SendFails::set(false);
			Registrar::on_new_head(PARA_A.into(), &Default::default());

			assert_eq!(
				take_sent(),
				vec![MessageToPara::V1(MessageToParaV1::HeadNoted { para_id: PARA_A })]
			);
			assert_eq!(registrar_events(), vec![Event::HeadNoted { para_id: PARA_A }]);
			assert!(!AwaitingFirstHead::<Test>::contains_key(PARA_A));

			// And once it lands it is not sent again.
			Registrar::on_new_head(PARA_A.into(), &Default::default());

			assert!(take_sent().is_empty());
			assert!(registrar_events().is_empty());
		});
	}
}

mod relay {
	use super::*;
	use crate::{weights::WeightInfo, QueuedPerPara, RelayQueue, RelayQueueBounds};
	use frame_support::{dispatch::GetDispatchInfo, traits::OnPoll, weights::WeightMeter};
	use registrar_primitives::{ParaRequest, ParaRequestV1};

	const PARA_C: ParaId = 2002;

	fn para(para_id: ParaId) -> RuntimeOrigin {
		polkadot_runtime_parachains::Origin::Parachain(para_id.into()).into()
	}

	fn relayed(para_id: ParaId, request: ParaRequestV1) -> MessageToPara {
		MessageToPara::V1(MessageToParaV1::Relayed { para_id, request })
	}

	fn relay(para_id: ParaId, request: ParaRequestV1) -> sp_runtime::DispatchResult {
		Registrar::relay(para(para_id), ParaRequest::V1(request))
	}

	fn poll_with(meter: &mut WeightMeter) {
		Registrar::on_poll(System::block_number(), meter);
		Registrar::do_try_state().unwrap();
	}

	fn poll() {
		poll_with(&mut WeightMeter::new());
	}

	fn queue_len() -> u32 {
		let (oldest, next) = RelayQueueBounds::<Test>::get();
		next.wrapping_sub(oldest)
	}

	#[test]
	fn queues_the_request_and_forwards_it_on_poll() {
		new_test_ext().execute_with(|| {
			let request =
				ParaRequestV1::ScheduleCodeUpgrade { code_hash: hash_of(&code(20)), code_len: 20 };

			assert_ok!(relay(PARA_A, request.clone()));

			assert!(take_sent().is_empty());
			assert_eq!(registrar_events(), vec![Event::RelayQueued { para_id: PARA_A }]);
			assert_eq!(QueuedPerPara::<Test>::get(PARA_A), 1);

			poll();

			assert_eq!(take_sent(), vec![relayed(PARA_A, request)]);
			assert_eq!(registrar_events(), vec![Event::Relayed { para_id: PARA_A }]);
			assert_eq!(queue_len(), 0);
			assert!(!QueuedPerPara::<Test>::contains_key(PARA_A));
		});
	}

	#[test]
	fn nothing_is_checked_or_applied_here() {
		new_test_ext().execute_with(|| {
			// Unknown to the registry: forwarded all the same.
			let request = ParaRequestV1::SetCurrentHead { head: head(MAX_HEAD_SIZE as usize) };

			assert_ok!(relay(PARA_B, request.clone()));
			poll();

			assert!(Heads::get().is_empty());
			assert_eq!(take_sent(), vec![relayed(PARA_B, request)]);
		});
	}

	#[test]
	fn a_head_over_the_bound_is_refused() {
		new_test_ext().execute_with(|| {
			let request = ParaRequestV1::SetCurrentHead { head: head(MAX_HEAD_SIZE as usize + 1) };

			assert_noop!(relay(PARA_A, request), Error::<Test>::HeadDataTooLarge);
		});
	}

	#[test]
	fn only_a_para_may_relay() {
		new_test_ext().execute_with(|| {
			let request = ParaRequest::V1(ParaRequestV1::RemoveLock);

			assert_noop!(
				Registrar::relay(RuntimeOrigin::signed(ALICE), request.clone()),
				DispatchError::BadOrigin
			);
			// Root has no para id to speak for.
			assert_noop!(
				Registrar::relay(RuntimeOrigin::root(), request),
				DispatchError::BadOrigin
			);
			assert_eq!(queue_len(), 0);
		});
	}

	#[test]
	fn one_para_cannot_take_more_than_its_share() {
		new_test_ext().execute_with(|| {
			for _ in 0..MAX_QUEUED_PER_PARA {
				assert_ok!(relay(PARA_A, ParaRequestV1::AddLock));
			}

			assert_noop!(relay(PARA_A, ParaRequestV1::AddLock), Error::<Test>::TooManyQueued);
			// Others still get in.
			assert_ok!(relay(PARA_B, ParaRequestV1::AddLock));

			// Its share frees up as its requests go out.
			poll();
			assert_ok!(relay(PARA_A, ParaRequestV1::AddLock));
			Registrar::do_try_state().unwrap();
		});
	}

	#[test]
	fn the_queue_is_bounded_in_total() {
		new_test_ext().execute_with(|| {
			for para_id in [PARA_A, PARA_B] {
				for _ in 0..MAX_QUEUED_PER_PARA {
					assert_ok!(relay(para_id, ParaRequestV1::AddLock));
				}
			}
			assert_eq!(queue_len(), MAX_QUEUED);

			assert_noop!(relay(PARA_C, ParaRequestV1::AddLock), Error::<Test>::QueueFull);

			poll();
			assert_ok!(relay(PARA_C, ParaRequestV1::AddLock));
		});
	}

	#[test]
	fn forwards_oldest_first_up_to_the_per_block_limit() {
		new_test_ext().execute_with(|| {
			let queued = [
				(PARA_A, ParaRequestV1::AddLock),
				(PARA_B, ParaRequestV1::RemoveLock),
				(PARA_A, ParaRequestV1::Deregister),
				(PARA_C, ParaRequestV1::AddLock),
			];
			for (para_id, request) in queued.clone() {
				assert_ok!(relay(para_id, request));
			}
			let expected: Vec<_> = queued.into_iter().map(|(p, r)| relayed(p, r)).collect();

			poll();
			assert_eq!(take_sent(), expected[..MAX_FORWARDED_PER_BLOCK as usize]);

			poll();
			assert_eq!(take_sent(), expected[MAX_FORWARDED_PER_BLOCK as usize..]);
			assert_eq!(queue_len(), 0);
		});
	}

	#[test]
	fn a_bounced_request_stays_first_in_line() {
		new_test_ext().execute_with(|| {
			assert_ok!(relay(PARA_A, ParaRequestV1::AddLock));
			assert_ok!(relay(PARA_B, ParaRequestV1::RemoveLock));
			registrar_events();

			SendFails::set(true);
			poll();
			assert!(take_sent().is_empty());
			assert!(registrar_events().is_empty());
			assert_eq!(queue_len(), 2);

			SendFails::set(false);
			poll();
			assert_eq!(
				take_sent(),
				vec![
					relayed(PARA_A, ParaRequestV1::AddLock),
					relayed(PARA_B, ParaRequestV1::RemoveLock)
				]
			);
		});
	}

	#[test]
	fn a_request_the_transport_cannot_carry_does_not_stall_other_paras() {
		new_test_ext().execute_with(|| {
			MaxSendSize::set(50);
			let request = ParaRequestV1::SetCurrentHead { head: head(MAX_HEAD_SIZE as usize) };
			assert_noop!(relay(PARA_A, request), Error::<Test>::HeadDataTooLarge);
			assert_ok!(relay(PARA_B, ParaRequestV1::AddLock));

			for n in 2..12 {
				System::set_block_number(n);
				poll();
			}

			assert_eq!(take_sent(), vec![relayed(PARA_B, ParaRequestV1::AddLock)]);
		});
	}

	#[test]
	fn a_stuck_entry_does_not_lock_everyone_out() {
		new_test_ext().execute_with(|| {
			MaxSendSize::set(50);
			let request = ParaRequestV1::SetCurrentHead { head: head(MAX_HEAD_SIZE as usize) };
			assert_noop!(relay(PARA_A, request), Error::<Test>::HeadDataTooLarge);
			assert_ok!(relay(PARA_B, ParaRequestV1::AddLock));
			assert_ok!(relay(PARA_B, ParaRequestV1::AddLock));
			assert_ok!(relay(PARA_C, ParaRequestV1::AddLock));
			for n in 2..12 {
				System::set_block_number(n);
				poll();
			}

			assert_ok!(relay(2003, ParaRequestV1::AddLock));
		});
	}

	#[test]
	fn nothing_overtakes_a_bounced_request() {
		new_test_ext().execute_with(|| {
			assert_ok!(relay(PARA_A, ParaRequestV1::SetCurrentHead { head: head(10) }));
			assert_ok!(relay(PARA_B, ParaRequestV1::RemoveLock));

			// Only PARA_A's message is refused this block.
			MaxSendSize::set(10);
			poll();
			assert!(take_sent().is_empty());

			MaxSendSize::set(u32::MAX);
			poll();
			assert_eq!(
				take_sent(),
				vec![
					relayed(PARA_A, ParaRequestV1::SetCurrentHead { head: head(10) }),
					relayed(PARA_B, ParaRequestV1::RemoveLock)
				]
			);
		});
	}

	#[test]
	fn forwarding_stops_when_the_meter_runs_out() {
		new_test_ext().execute_with(|| {
			assert_ok!(relay(PARA_A, ParaRequestV1::AddLock));
			let poll_weight = <() as WeightInfo>::poll_relay_queue();
			let worst = <() as WeightInfo>::forward_relayed(MAX_HEAD_SIZE) - poll_weight;

			poll_with(&mut WeightMeter::with_limit(poll_weight + worst - 1.into()));
			assert!(take_sent().is_empty());

			poll_with(&mut WeightMeter::with_limit(poll_weight + worst));
			assert_eq!(take_sent(), vec![relayed(PARA_A, ParaRequestV1::AddLock)]);
		});
	}

	#[test]
	fn queue_positions_wrap_around() {
		new_test_ext().execute_with(|| {
			RelayQueueBounds::<Test>::put((u32::MAX, u32::MAX));

			assert_ok!(relay(PARA_A, ParaRequestV1::AddLock));
			assert_ok!(relay(PARA_B, ParaRequestV1::RemoveLock));
			assert_eq!(RelayQueueBounds::<Test>::get(), (u32::MAX, 1));
			assert!(RelayQueue::<Test>::contains_key(0));

			poll();
			assert_eq!(take_sent().len(), 2);
			assert_eq!(RelayQueueBounds::<Test>::get(), (1, 1));
		});
	}

	#[test]
	fn on_poll_charges_for_the_head_it_forwards() {
		new_test_ext().execute_with(|| {
			let mut meter = WeightMeter::new();
			poll_with(&mut meter);
			assert_eq!(meter.consumed(), <() as WeightInfo>::poll_relay_queue());

			assert_ok!(relay(PARA_A, ParaRequestV1::SetCurrentHead { head: head(10) }));
			let charged = <() as WeightInfo>::forward_relayed(10);

			// A refused send was still read and attempted.
			SendFails::set(true);
			let mut meter = WeightMeter::new();
			poll_with(&mut meter);
			assert_eq!(meter.consumed(), charged);

			SendFails::set(false);
			let mut meter = WeightMeter::new();
			poll_with(&mut meter);
			assert_eq!(take_sent().len(), 1);
			assert_eq!(meter.consumed(), charged);
		});
	}

	#[test]
	fn the_call_is_weighed_by_its_head() {
		let weight = |request| {
			crate::Call::<Test>::relay { request: ParaRequest::V1(request) }
				.get_dispatch_info()
				.call_weight
		};

		assert_eq!(weight(ParaRequestV1::AddLock), <() as WeightInfo>::relay(0));
		assert_eq!(
			weight(ParaRequestV1::SetCurrentHead { head: head(64) }),
			<() as WeightInfo>::relay(64)
		);
	}

	#[test]
	fn try_state_catches_a_queue_out_of_step() {
		new_test_ext().execute_with(|| {
			assert_ok!(relay(PARA_A, ParaRequestV1::AddLock));
			assert_ok!(relay(PARA_B, ParaRequestV1::AddLock));
			assert_ok!(Registrar::do_try_state());

			QueuedPerPara::<Test>::insert(PARA_A, 2);
			assert!(Registrar::do_try_state().is_err());
			QueuedPerPara::<Test>::insert(PARA_A, 1);

			RelayQueue::<Test>::remove(1);
			assert!(Registrar::do_try_state().is_err());
		});
	}
}
