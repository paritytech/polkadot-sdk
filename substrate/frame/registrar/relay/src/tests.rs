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

use crate::{mock::*, Error, Event, PendingCodeUpgrades, PendingRegistrations};
use frame_support::{assert_noop, assert_ok};
use registrar_primitives::{
	FailureReason, MessageToPara, MessageToParaV1, MessageToRelay, MessageToRelayV1, ParaId,
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

/// The message id every test deregistration carries. See [`MSG_ID`].
const DEREGISTER_ID: u64 = 7;

/// The message id every test deregistration chase-up carries. See [`MSG_ID`].
const CHASE_UP_ID: u64 = 8;

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

fn chase_up_msg(para_id: ParaId) -> MessageToRelay<AccountId> {
	MessageToRelay::V1(MessageToRelayV1::CancelDeregistration { para_id, message_id: CHASE_UP_ID })
}

fn chase_up_report(para_id: ParaId, outcome: registrar_primitives::Outcome) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::CancelDeregistrationResponse {
		para_id,
		message_id: CHASE_UP_ID,
		outcome,
	})
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

mod authorize_code {
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
			// Shorter than declared, so the manager underpaid on the parachain.
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

mod cancel_authorization {
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

mod deregister {
	use super::*;


	#[test]
	fn deregisters_a_known_para_and_reports_success() {
		new_test_ext().execute_with(|| {
			Managers::set(vec![(PARA_A, ALICE)]);

			assert_ok!(Registrar::receive(RuntimeOrigin::root(), deregister_msg(PARA_A)));

			assert_eq!(DeregisteredParas::get(), vec![PARA_A]);
			assert!(Managers::get().is_empty());
			assert_eq!(take_sent(), vec![deregister_report(PARA_A, Ok(()))]);
			assert_eq!(
				registrar_events(),
				vec![Event::Deregistered { para_id: PARA_A, message_id: DEREGISTER_ID }]
			);
		});
	}

	#[test]
	fn an_id_the_relay_chain_never_knew_is_confirmed_gone() {
		new_test_ext().execute_with(|| {
			// There is nothing to remove and the parachain is waiting to release deposits, so
			// this is a confirmation, not a refusal.
			assert_ok!(Registrar::receive(RuntimeOrigin::root(), deregister_msg(PARA_A)));

			assert!(DeregisteredParas::get().is_empty());
			assert_eq!(take_sent(), vec![deregister_report(PARA_A, Ok(()))]);
			assert_eq!(
				registrar_events(),
				vec![Event::Deregistered { para_id: PARA_A, message_id: DEREGISTER_ID }]
			);
		});
	}

	#[test]
	fn a_para_registered_outside_the_registry_is_refused() {
		new_test_ext().execute_with(|| {
			// The relay chain knows the para, but not through the registry (e.g. a system para).
			// This pallet does not test for that itself: the registry refuses, and the refusal is
			// reported. The protection lives where the knowledge does.
			AlreadyKnown::set(vec![PARA_A]);

			assert_ok!(Registrar::receive(RuntimeOrigin::root(), deregister_msg(PARA_A)));

			assert!(DeregisteredParas::get().is_empty());
			assert_eq!(
				take_sent(),
				vec![deregister_report(PARA_A, Err(FailureReason::CannotDeregister))]
			);
			assert_eq!(
				registrar_events(),
				vec![Event::DeregistrationRejected {
					para_id: PARA_A,
					message_id: DEREGISTER_ID,
					reason: FailureReason::CannotDeregister,
				}]
			);
		});
	}

	#[test]
	fn the_manager_is_not_re_checked_here() {
		new_test_ext().execute_with(|| {
			// The registry's idea of the manager is deliberately ignored: the parachain holds the
			// deposits, so it is the authority on who may ask, and it has already checked. Once
			// the registry is drained in favour of the parachain's, `manager_of` answers `None`
			// for every para that predates the move -- re-checking here would refuse them all,
			// permanently.
			Managers::set(vec![(PARA_A, ALICE)]);

			assert_ok!(Registrar::receive(RuntimeOrigin::root(), deregister_msg(PARA_A)));

			assert_eq!(DeregisteredParas::get(), vec![PARA_A]);
			assert_eq!(Managers::get(), vec![]);
			assert_eq!(take_sent(), vec![deregister_report(PARA_A, Ok(()))]);
			assert_eq!(
				registrar_events(),
				vec![Event::Deregistered { para_id: PARA_A, message_id: DEREGISTER_ID }]
			);
		});
	}

	#[test]
	fn a_lock_on_the_relay_chain_does_not_block_the_control_plane() {
		new_test_ext().execute_with(|| {
			// Same reasoning as the manager: the lock is the parachain's state now, and it
			// enforces it before sending. A stale lock here must not strand a para.
			Managers::set(vec![(PARA_A, ALICE)]);
			LockedParas::set(vec![PARA_A]);

			assert_ok!(Registrar::receive(RuntimeOrigin::root(), deregister_msg(PARA_A)));

			assert_eq!(DeregisteredParas::get(), vec![PARA_A]);
			assert_eq!(take_sent(), vec![deregister_report(PARA_A, Ok(()))]);
			assert_eq!(
				registrar_events(),
				vec![Event::Deregistered { para_id: PARA_A, message_id: DEREGISTER_ID }]
			);
		});
	}

	#[test]
	fn a_registry_failure_is_rolled_back_and_reported() {
		new_test_ext().execute_with(|| {
			Managers::set(vec![(PARA_A, ALICE)]);
			DeregisterFails::set(true);

			// A refusal is reported, not an extrinsic failure.
			assert_ok!(Registrar::receive(RuntimeOrigin::root(), deregister_msg(PARA_A)));

			assert_eq!(Managers::get(), vec![(PARA_A, ALICE)]);
			assert_eq!(
				take_sent(),
				vec![deregister_report(PARA_A, Err(FailureReason::CannotDeregister))]
			);
			assert_eq!(
				registrar_events(),
				vec![Event::DeregistrationRejected {
					para_id: PARA_A,
					message_id: DEREGISTER_ID,
					reason: FailureReason::CannotDeregister,
				}]
			);
			// The registry wrote before it failed; the storage layer unwound that write even
			// though the extrinsic as a whole succeeded.
			assert_eq!(frame_support::storage::unhashed::get::<ParaId>(PARTIAL_WRITE_KEY), None);
		});
	}

	#[test]
	fn only_the_parachain_may_deregister() {
		new_test_ext().execute_with(|| {
			Managers::set(vec![(PARA_A, ALICE)]);

			assert_noop!(
				Registrar::receive(RuntimeOrigin::signed(ALICE), deregister_msg(PARA_A)),
				DispatchError::BadOrigin
			);
			assert!(DeregisteredParas::get().is_empty());
		});
	}
}

mod cancel_deregistration {
	use super::*;

	#[test]
	fn a_chase_up_while_the_para_is_still_registered_is_confirmed() {
		new_test_ext().execute_with(|| {
			// The deregistration request never arrived, so the para is still in the registry and
			// the parachain may safely go back to registered.
			Managers::set(vec![(PARA_A, ALICE)]);

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				chase_up_msg(PARA_A)
			));

			assert_eq!(Managers::get(), vec![(PARA_A, ALICE)]);
			assert_eq!(take_sent(), vec![chase_up_report(PARA_A, Ok(()))]);
			assert_eq!(
				registrar_events(),
				vec![Event::DeregistrationCancelled { para_id: PARA_A, message_id: CHASE_UP_ID }]
			);
		});
	}

	#[test]
	fn a_chase_up_after_the_deregistration_went_through_is_refused() {
		new_test_ext().execute_with(|| {
			// The para is gone: the deregistration happened and its report was lost, so the
			// parachain is told to release the deposits after all.
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				chase_up_msg(PARA_A)
			));

			assert_eq!(
				take_sent(),
				vec![chase_up_report(PARA_A, Err(FailureReason::NotRegistered))]
			);
			assert_eq!(
				registrar_events(),
				vec![Event::DeregistrationCancellationRefused {
					para_id: PARA_A,
					message_id: CHASE_UP_ID,
				}]
			);
		});
	}

	#[test]
	fn only_the_parachain_may_chase_up() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Registrar::receive(
					RuntimeOrigin::signed(ALICE),
					chase_up_msg(PARA_A)
				),
				DispatchError::BadOrigin
			);
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

	#[test]
	fn a_bounced_deregistration_report_does_not_undo_the_deregistration() {
		new_test_ext().execute_with(|| {
			Managers::set(vec![(PARA_A, ALICE)]);
			SendFails::set(true);

			assert_ok!(Registrar::receive(RuntimeOrigin::root(), deregister_msg(PARA_A)));

			assert_eq!(DeregisteredParas::get(), vec![PARA_A]);
			assert!(Managers::get().is_empty());
			assert_eq!(
				registrar_events(),
				vec![
					Event::ReportFailed { para_id: PARA_A, message_id: DEREGISTER_ID },
					Event::Deregistered { para_id: PARA_A, message_id: DEREGISTER_ID },
				]
			);
		});
	}
}

/// The message id every test code upgrade carries. See [`MSG_ID`].
const UPGRADE_ID: u64 = 9;

fn upgrade_msg(para_id: ParaId, code: &[u8]) -> MessageToRelay<AccountId> {
	MessageToRelay::V1(MessageToRelayV1::AuthorizeCodeUpgrade {
		para_id,
		message_id: UPGRADE_ID,
		code_hash: hash_of(code),
		code_len: code.len() as u32,
	})
}

/// A para the registry knows, so upgrade and head requests have something to act on.
fn known_para(para_id: ParaId) {
	Managers::set(vec![(para_id, ALICE)]);
}

mod authorize_code_upgrade {
	use super::*;

	#[test]
	fn parks_the_authorization_for_a_known_para() {
		new_test_ext().execute_with(|| {
			known_para(PARA_A);
			let blob = code(MAX_CODE_SIZE as usize);

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				upgrade_msg(PARA_A, &blob)
			));

			let pending = PendingCodeUpgrades::<Test>::get(PARA_A).unwrap();
			assert_eq!(pending.message_id, UPGRADE_ID);
			assert_eq!(pending.code_hash, hash_of(&blob));
			assert_eq!(pending.code_len, MAX_CODE_SIZE);
			// Nothing goes back: an upgrade stakes nothing on the parachain, so a verdict would
			// be a round trip nobody is waiting on.
			assert_eq!(take_sent(), vec![]);
			assert_eq!(
				registrar_events(),
				vec![Event::CodeUpgradePending {
					para_id: PARA_A,
					message_id: UPGRADE_ID,
					code_hash: hash_of(&blob),
				}]
			);
		});
	}

	#[test]
	fn refuses_a_para_the_registry_does_not_know() {
		new_test_ext().execute_with(|| {
			let blob = code(MAX_CODE_SIZE as usize);

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				upgrade_msg(PARA_A, &blob)
			));

			assert!(PendingCodeUpgrades::<Test>::get(PARA_A).is_none());
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
	fn refuses_code_the_live_configuration_would_not_take() {
		new_test_ext().execute_with(|| {
			known_para(PARA_A);

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				upgrade_msg(PARA_A, &code(MAX_CODE_SIZE as usize + 1))
			));

			assert!(PendingCodeUpgrades::<Test>::get(PARA_A).is_none());
			assert_eq!(
				registrar_events(),
				vec![Event::CodeUpgradeRejected {
					para_id: PARA_A,
					message_id: UPGRADE_ID,
					reason: FailureReason::InvalidOnboardingData,
				}]
			);
		});
	}

	#[test]
	fn one_authorization_per_para_at_a_time() {
		new_test_ext().execute_with(|| {
			known_para(PARA_A);
			let first = code(MAX_CODE_SIZE as usize);
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				upgrade_msg(PARA_A, &first)
			));
			let _ = registrar_events();

			// A second attempt must not overwrite the first, or a para could grow this state
			// without bound and invalidate an upload already in flight.
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				upgrade_msg(PARA_A, &code(MIN_CODE_SIZE as usize))
			));

			assert_eq!(PendingCodeUpgrades::<Test>::get(PARA_A).unwrap().code_hash, hash_of(&first));
			assert_eq!(
				registrar_events(),
				vec![Event::CodeUpgradeRejected {
					para_id: PARA_A,
					message_id: UPGRADE_ID,
					reason: FailureReason::AlreadyRegistered,
				}]
			);
		});
	}

	#[test]
	fn refuses_users() {
		new_test_ext().execute_with(|| {
			known_para(PARA_A);

			assert_noop!(
				Registrar::receive(
					RuntimeOrigin::signed(ALICE),
					upgrade_msg(PARA_A, &code(MIN_CODE_SIZE as usize))
				),
				DispatchError::BadOrigin
			);
		});
	}
}

mod apply_authorized_code_upgrade {
	use super::*;

	fn authorize(para_id: ParaId, blob: &[u8]) {
		known_para(para_id);
		assert_ok!(Registrar::receive(
			RuntimeOrigin::root(),
			upgrade_msg(para_id, blob)
		));
		let _ = registrar_events();
	}

	#[test]
	fn anybody_may_upload_matching_bytes_and_the_upgrade_lands() {
		new_test_ext().execute_with(|| {
			let blob = code(MAX_CODE_SIZE as usize);
			authorize(PARA_A, &blob);

			assert_ok!(Registrar::apply_authorized_code_upgrade(
				frame_system::RawOrigin::Authorized.into(),
				PARA_A,
				blob.clone()
			));

			assert_eq!(Upgraded::get(), vec![(PARA_A, blob)]);
			assert!(PendingCodeUpgrades::<Test>::get(PARA_A).is_none());
			assert_eq!(
				registrar_events(),
				vec![Event::CodeUpgraded { para_id: PARA_A, message_id: UPGRADE_ID }]
			);
		});
	}

	#[test]
	fn refuses_bytes_that_do_not_match_the_commitment() {
		new_test_ext().execute_with(|| {
			let blob = code(MAX_CODE_SIZE as usize);
			authorize(PARA_A, &blob);

			// Right length, wrong bytes.
			let mut wrong = blob.clone();
			wrong[0] = wrong[0].wrapping_add(1);
			assert_noop!(
				Registrar::apply_authorized_code_upgrade(
					frame_system::RawOrigin::Authorized.into(),
					PARA_A,
					wrong
				),
				Error::<Test>::CodeHashMismatch
			);

			// Wrong length.
			assert_noop!(
				Registrar::apply_authorized_code_upgrade(
					frame_system::RawOrigin::Authorized.into(),
					PARA_A,
					code(MAX_CODE_SIZE as usize - 1)
				),
				Error::<Test>::CodeLenMismatch
			);

			assert!(Upgraded::get().is_empty());
			assert!(PendingCodeUpgrades::<Test>::get(PARA_A).is_some());
		});
	}

	#[test]
	fn refuses_an_upload_nobody_authorized() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Registrar::apply_authorized_code_upgrade(
					frame_system::RawOrigin::Authorized.into(),
					PARA_A,
					code(MIN_CODE_SIZE as usize)
				),
				Error::<Test>::NothingPendingUpgrade
			);
		});
	}

	#[test]
	fn a_registry_refusal_is_reported_and_clears_the_authorization() {
		new_test_ext().execute_with(|| {
			let blob = code(MAX_CODE_SIZE as usize);
			authorize(PARA_A, &blob);
			UpgradeFails::set(true);

			// Not an extrinsic failure: the upload was valid, the registry simply said no.
			assert_ok!(Registrar::apply_authorized_code_upgrade(
				frame_system::RawOrigin::Authorized.into(),
				PARA_A,
				blob
			));

			assert!(Upgraded::get().is_empty());
			// The entry goes either way, so a manager cannot keep trying different bytes.
			assert!(PendingCodeUpgrades::<Test>::get(PARA_A).is_none());
			assert_eq!(
				registrar_events(),
				vec![Event::CodeUpgradeRejected {
					para_id: PARA_A,
					message_id: UPGRADE_ID,
					reason: FailureReason::CannotUpgrade,
				}]
			);
		});
	}

	#[test]
	fn the_pool_check_agrees_with_the_dispatch() {
		new_test_ext().execute_with(|| {
			let blob = code(MAX_CODE_SIZE as usize);
			authorize(PARA_A, &blob);

			assert!(Registrar::authorize_apply_authorized_code_upgrade(
				TransactionSource::External,
				&PARA_A,
				&blob,
			)
			.is_ok());

			assert_eq!(
				Registrar::authorize_apply_authorized_code_upgrade(
					TransactionSource::External,
					&PARA_A,
					&code(MIN_CODE_SIZE as usize),
				),
				Err(InvalidTransaction::Custom(
					Registrar::err_to_code(Error::<Test>::CodeLenMismatch)
				)
				.into())
			);
		});
	}
}

mod set_current_head {
	use super::*;

	fn head_msg(para_id: ParaId, head: Vec<u8>) -> MessageToRelay<AccountId> {
		MessageToRelay::V1(MessageToRelayV1::SetCurrentHead {
			para_id,
			message_id: UPGRADE_ID,
			head,
		})
	}

	#[test]
	fn writes_the_head_for_a_known_para() {
		new_test_ext().execute_with(|| {
			known_para(PARA_A);
			let new_head = head(MAX_HEAD_SIZE as usize);

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				head_msg(PARA_A, new_head.clone())
			));

			assert_eq!(HeadsSet::get(), vec![(PARA_A, new_head)]);
			assert_eq!(take_sent(), vec![]);
			assert_eq!(
				registrar_events(),
				vec![Event::HeadSet { para_id: PARA_A, message_id: UPGRADE_ID }]
			);
		});
	}

	#[test]
	fn refuses_a_para_the_registry_does_not_know() {
		new_test_ext().execute_with(|| {
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				head_msg(PARA_A, head(4))
			));

			assert!(HeadsSet::get().is_empty());
			assert_eq!(
				registrar_events(),
				vec![Event::HeadRejected {
					para_id: PARA_A,
					message_id: UPGRADE_ID,
					reason: FailureReason::NotRegistered,
				}]
			);
		});
	}

	#[test]
	fn a_registry_refusal_is_reported() {
		new_test_ext().execute_with(|| {
			known_para(PARA_A);
			SetHeadFails::set(true);

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				head_msg(PARA_A, head(4))
			));

			assert!(HeadsSet::get().is_empty());
			assert_eq!(
				registrar_events(),
				vec![Event::HeadRejected {
					para_id: PARA_A,
					message_id: UPGRADE_ID,
					reason: FailureReason::InvalidOnboardingData,
				}]
			);
		});
	}
}

mod force_drop_pending {
	use super::*;

	#[test]
	fn root_clears_both_kinds_of_pending_entry() {
		new_test_ext().execute_with(|| {
			// GIVEN a para with a registration and an upgrade both waiting on code. Neither
			// expires on its own, so without this call they would sit here for good.
			let blob = code(MAX_CODE_SIZE as usize);
			let (register, _) = register_msg(PARA_A, 20, MAX_CODE_SIZE as usize);
			assert_ok!(Registrar::receive(RuntimeOrigin::root(), register));
			known_para(PARA_A);
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				upgrade_msg(PARA_A, &blob)
			));
			let _ = registrar_events();

			assert_noop!(
				Registrar::force_drop_pending(RuntimeOrigin::signed(ALICE), PARA_A),
				DispatchError::BadOrigin
			);

			assert_ok!(Registrar::force_drop_pending(RuntimeOrigin::root(), PARA_A));

			assert!(PendingRegistrations::<Test>::get(PARA_A).is_none());
			assert!(PendingCodeUpgrades::<Test>::get(PARA_A).is_none());
			// Nothing is sent: the parachain still holds its deposit and must cancel there.
			assert_eq!(take_sent(), vec![]);
			assert_eq!(registrar_events(), vec![Event::PendingDropped { para_id: PARA_A }]);
		});
	}
}

mod bounds {
	use super::*;

	#[test]
	fn the_two_pending_maps_are_bounded_separately_but_by_the_same_number() {
		new_test_ext().execute_with(|| {
			// Fill the registration map to its bound.
			for i in 0..MAX_PENDING {
				let para = PARA_A + 100 + i;
				let (msg, _) = register_msg(para, 20, 300);
				assert_ok!(Registrar::receive(RuntimeOrigin::root(), msg));
				assert!(PendingRegistrations::<Test>::get(para).is_some(), "para {para}");
			}
			let _ = take_sent();
			let _ = registrar_events();

			// One more is refused and reported, rather than silently growing relay-chain state
			// that no deposit here pays for.
			let overflow = PARA_A + 999;
			let (msg, _) = register_msg(overflow, 20, 300);
			assert_ok!(Registrar::receive(RuntimeOrigin::root(), msg));
			assert!(PendingRegistrations::<Test>::get(overflow).is_none());
			assert_eq!(
				take_sent(),
				vec![failure_report(overflow, FailureReason::TooManyPending)]
			);

			// The upgrade map has its own budget: a full registration map must not stop a
			// registered para from upgrading.
			known_para(PARA_A);
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				upgrade_msg(PARA_A, &code(MAX_CODE_SIZE as usize))
			));
			assert!(PendingCodeUpgrades::<Test>::get(PARA_A).is_some());
		});
	}

	#[test]
	fn every_validation_failure_reports_a_distinct_code() {
		// These codes are what a node sees when an unsigned upload is refused from the pool. Two
		// failures sharing one would make the refusal ambiguous to anybody debugging it.
		let errors = [
			Error::<Test>::NothingPending,
			Error::<Test>::CodeHashMismatch,
			Error::<Test>::CodeLenMismatch,
			Error::<Test>::CodeTooLarge,
			Error::<Test>::NothingPendingUpgrade,
		];
		let mut codes: Vec<u8> = errors.into_iter().map(Registrar::err_to_code).collect();
		let total = codes.len();
		codes.sort_unstable();
		codes.dedup();
		assert_eq!(codes.len(), total, "two errors share an InvalidTransaction::Custom code");
	}
}

mod remove_upgrade_cooldown {
	use super::*;

	fn cooldown_msg(para_id: ParaId) -> MessageToRelay<AccountId> {
		MessageToRelay::V1(MessageToRelayV1::RemoveUpgradeCooldown {
			para_id,
			message_id: MSG_ID,
		})
	}

	#[test]
	fn drops_the_cooldown_and_charges_nothing_here() {
		new_test_ext().execute_with(|| {
			InCooldown::set(vec![PARA_A]);

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				cooldown_msg(PARA_A)
			));

			assert!(InCooldown::get().is_empty());
			// The payer was charged on the parachain, so nothing goes back and nothing is taken
			// here.
			assert_eq!(take_sent(), vec![]);
			assert_eq!(
				registrar_events(),
				vec![Event::UpgradeCooldownRemoved { para_id: PARA_A, message_id: MSG_ID }]
			);
		});
	}

	#[test]
	fn a_cooldown_that_already_expired_is_said_so_rather_than_looking_like_success() {
		new_test_ext().execute_with(|| {
			// The cooldown may lapse between the parachain deciding to pay and this arriving.
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				cooldown_msg(PARA_A)
			));

			assert_eq!(
				registrar_events(),
				vec![Event::NoUpgradeCooldown { para_id: PARA_A, message_id: MSG_ID }]
			);
		});
	}

	#[test]
	fn users_cannot_reach_it() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Registrar::receive(RuntimeOrigin::signed(ALICE), cooldown_msg(PARA_A)),
				DispatchError::BadOrigin
			);
		});
	}
}
