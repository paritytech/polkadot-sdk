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

use crate::{
	mock::*, Error, Event, PendingCount, PendingRegistrations, INVALID_TX_BAD_CODE,
	INVALID_TX_EXPIRED, INVALID_TX_NOTHING_PENDING,
};
use frame_support::{assert_noop, assert_ok};
use registrar_primitives::{
	FailureReason, MessageToPara, MessageToParaV1, MessageToRelay, MessageToRelayV1, ParaId,
	RegistrationOutcome,
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

fn register_msg(
	para_id: ParaId,
	head_len: usize,
	code_len: usize,
) -> (MessageToRelay<AccountId>, Vec<u8>) {
	let blob = code(code_len);
	let msg = MessageToRelay::V1(MessageToRelayV1::Register {
		para_id,
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
	MessageToPara::V1(MessageToParaV1::RegistrationResult {
		para_id,
		outcome: RegistrationOutcome::Failed(reason),
	})
}

fn success_report(para_id: ParaId) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::RegistrationResult {
		para_id,
		outcome: RegistrationOutcome::Registered,
	})
}

/// Run `authorize_register_code` and the dispatch together, the way the node does.
///
/// Returns both verdicts so a test can assert the pool and the block agree.
fn authorize_and_dispatch(
	para_id: ParaId,
	validation_code: Vec<u8>,
) -> (Result<(), InvalidTransaction>, Result<(), DispatchError>) {
	let authorized =
		Registrar::authorize_register_code(TransactionSource::External, &para_id, &validation_code)
			.map(|_| ())
			.map_err(|e| match e {
				sp_runtime::transaction_validity::TransactionValidityError::Invalid(i) => i,
				other => panic!("unexpected validity error: {other:?}"),
			});

	let dispatched = Registrar::register_code(
		frame_system::RawOrigin::Authorized.into(),
		para_id,
		validation_code,
	)
	.map(|_| ())
	.map_err(|e| e.error);

	(authorized, dispatched)
}

mod receive {
	use super::*;

	#[test]
	fn parks_a_valid_request_and_says_nothing_to_the_parachain_yet() {
		new_test_ext().execute_with(|| {
			let blob = request(PARA_A, 20, 300);

			let pending = PendingRegistrations::<Test>::get(PARA_A).unwrap();
			assert_eq!(pending.manager, ALICE);
			assert_eq!(pending.genesis_head.into_inner(), head(20));
			assert_eq!(pending.code_hash, hash_of(&blob));
			assert_eq!(pending.code_len, 300);
			assert_eq!(pending.expire_at, System::block_number() + PENDING_TIMEOUT);
			assert_eq!(PendingCount::<Test>::get(), 1);

			assert_eq!(
				registrar_events(),
				vec![Event::RegistrationPending {
					para_id: PARA_A,
					code_hash: hash_of(&blob),
					expire_at: System::block_number() + PENDING_TIMEOUT,
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

			assert_eq!(PendingCount::<Test>::get(), 1);
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
			assert_eq!(PendingCount::<Test>::get(), MAX_PENDING);
			assert_eq!(take_sent(), vec![failure_report(overflow, FailureReason::TooManyPending)]);
		});
	}
}

mod register_code {
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
			assert_eq!(PendingCount::<Test>::get(), 0);
			assert_eq!(take_sent(), vec![success_report(PARA_A)]);
			assert_eq!(
				registrar_events(),
				vec![Event::Registered { para_id: PARA_A, manager: ALICE }]
			);
		});
	}

	#[test]
	fn nothing_pending_is_refused_by_both_the_pool_and_the_block() {
		new_test_ext().execute_with(|| {
			let (authorized, dispatched) = authorize_and_dispatch(PARA_A, code(300));

			assert_eq!(authorized, Err(InvalidTransaction::Custom(INVALID_TX_NOTHING_PENDING)));
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

			assert_eq!(authorized, Err(InvalidTransaction::Custom(INVALID_TX_BAD_CODE)));
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
			// Shorter than declared, so the manager underpaid on the parachain.
			let short = code(200);

			let (authorized, dispatched) = authorize_and_dispatch(PARA_A, short);

			assert_eq!(authorized, Err(InvalidTransaction::Custom(INVALID_TX_BAD_CODE)));
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

			assert_eq!(authorized, Err(InvalidTransaction::Custom(INVALID_TX_BAD_CODE)));
			assert_eq!(dispatched, Err(Error::<Test>::CodeTooLarge.into()));
		});
	}

	#[test]
	fn an_expired_request_is_refused_by_both() {
		new_test_ext().execute_with(|| {
			let blob = request(PARA_A, 20, 300);
			let expire_at = PendingRegistrations::<Test>::get(PARA_A).unwrap().expire_at;

			// Sit exactly on the expiry block without letting `on_initialize` clean up, so the
			// expiry branch itself is what rejects.
			System::set_block_number(expire_at);

			let (authorized, dispatched) = authorize_and_dispatch(PARA_A, blob);

			assert_eq!(authorized, Err(InvalidTransaction::Custom(INVALID_TX_EXPIRED)));
			assert_eq!(dispatched, Err(Error::<Test>::Expired.into()));
		});
	}

	#[test]
	fn it_cannot_be_replayed() {
		new_test_ext().execute_with(|| {
			let blob = request(PARA_A, 20, 300);
			assert_eq!(authorize_and_dispatch(PARA_A, blob.clone()).1, Ok(()));

			let (authorized, dispatched) = authorize_and_dispatch(PARA_A, blob);
			assert_eq!(authorized, Err(InvalidTransaction::Custom(INVALID_TX_NOTHING_PENDING)));
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

			assert!(Registrar::register_code(
				frame_system::RawOrigin::Authorized.into(),
				PARA_A,
				blob.clone()
			)
			.is_err());

			// Nothing was consumed and nothing was reported, so the user can retry.
			assert!(PendingRegistrations::<Test>::get(PARA_A).is_some());
			assert_eq!(PendingCount::<Test>::get(), 1);
			assert!(take_sent().is_empty());

			RegisterFails::set(false);
			assert_ok!(Registrar::register_code(
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
				Registrar::register_code(RuntimeOrigin::signed(ALICE), PARA_A, blob),
				DispatchError::BadOrigin
			);
		});
	}
}

mod expiry {
	use super::*;

	#[test]
	fn a_request_whose_code_never_arrives_is_abandoned_and_reported() {
		new_test_ext().execute_with(|| {
			request(PARA_A, 20, 300);
			let expire_at = PendingRegistrations::<Test>::get(PARA_A).unwrap().expire_at;
			let _ = registrar_events();
			let _ = take_sent();

			// One block short: still waiting.
			run_to_block(expire_at - 1);
			assert!(PendingRegistrations::<Test>::get(PARA_A).is_some());
			assert!(take_sent().is_empty());

			run_to_block(expire_at);
			assert!(PendingRegistrations::<Test>::get(PARA_A).is_none());
			assert_eq!(PendingCount::<Test>::get(), 0);
			assert_eq!(take_sent(), vec![failure_report(PARA_A, FailureReason::Expired)]);
			assert_eq!(registrar_events(), vec![Event::RegistrationExpired { para_id: PARA_A }]);
		});
	}

	#[test]
	fn expiring_one_request_leaves_a_younger_one_alone() {
		new_test_ext().execute_with(|| {
			request(PARA_A, 20, 300);
			let first_expiry = PendingRegistrations::<Test>::get(PARA_A).unwrap().expire_at;

			run_to_block(System::block_number() + 3);
			request(PARA_B, 20, 300);
			let _ = take_sent();

			run_to_block(first_expiry);
			assert!(PendingRegistrations::<Test>::get(PARA_A).is_none());
			assert!(PendingRegistrations::<Test>::get(PARA_B).is_some());
			assert_eq!(PendingCount::<Test>::get(), 1);
			assert_eq!(take_sent(), vec![failure_report(PARA_A, FailureReason::Expired)]);
		});
	}

	#[test]
	fn freed_capacity_lets_a_new_request_in() {
		new_test_ext().execute_with(|| {
			for i in 0..MAX_PENDING {
				request(PARA_A + i, 20, 300);
			}
			let expiry = PendingRegistrations::<Test>::get(PARA_A).unwrap().expire_at;

			run_to_block(expiry);
			assert_eq!(PendingCount::<Test>::get(), 0);
			let _ = take_sent();

			request(PARA_A + MAX_PENDING, 20, 300);
			assert!(PendingRegistrations::<Test>::get(PARA_A + MAX_PENDING).is_some());
		});
	}
}

mod reporting {
	use super::*;

	#[test]
	fn a_bounced_report_does_not_undo_the_onboarding() {
		new_test_ext().execute_with(|| {
			let blob = request(PARA_A, 20, 300);
			let _ = registrar_events();
			SendFails::set(true);

			assert_ok!(Registrar::register_code(
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
					Event::ReportFailed { para_id: PARA_A },
					Event::Registered { para_id: PARA_A, manager: ALICE },
				]
			);
		});
	}
}
