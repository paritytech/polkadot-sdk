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

//! Tests for `pallet-registrar-para`.

use crate::{mock::*, Error, Event, HoldReason, Paras, RegistrationState};
use frame_support::{assert_noop, assert_ok, traits::fungible::InspectHold};
use registrar_primitives::{
	FailureReason, MessageToPara, MessageToParaV1, MessageToRelay, MessageToRelayV1, Outcome,
};
use sp_runtime::{
	traits::{BlakeTwo256, Hash},
	DispatchError, TokenError,
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

/// Total on hold for `who` across both of this pallet's reasons.
fn held(who: AccountId) -> Balance {
	Balances::balance_on_hold(&HoldReason::ParaIdReservation.into(), &who) +
		Balances::balance_on_hold(&HoldReason::Registration.into(), &who)
}

/// Reserve a para id for `who` and return it.
fn reserve_for(who: AccountId) -> u32 {
	assert_ok!(Registrar::reserve(RuntimeOrigin::signed(who)));
	let para_id = crate::NextFreeParaId::<Test>::get() - 1;
	let _ = registrar_events();
	para_id
}

/// Put `para_id` into `Pending` with a code of `code_len` bytes, and return that code.
fn request_registration(who: AccountId, para_id: u32, head_len: usize, code_len: usize) -> Vec<u8> {
	let blob = code(code_len);
	assert_ok!(Registrar::register(
		RuntimeOrigin::signed(who),
		para_id,
		head(head_len),
		code_len as u32,
		hash_of(&blob),
	));
	blob
}

/// Reserve, register and confirm a para for `who` (message id 0), leaving the logs clean.
///
/// Head is 20 bytes; the code side is always priced at `MAX_CODE_SIZE`, so the registration
/// deposit is `PER_BYTE * (20 + MAX_CODE_SIZE)` whatever code length is declared.
fn registered_para(who: AccountId) -> u32 {
	let para_id = reserve_for(who);
	request_registration(who, para_id, 20, 300);
	assert_ok!(Registrar::receive(
		RuntimeOrigin::root(),
		MessageToPara::V1(MessageToParaV1::RegisterResponse {
			para_id,
			message_id: 0,
			outcome: Ok(()),
		}),
	));
	let _ = registrar_events();
	let _ = take_sent();
	para_id
}

/// Put `who`'s para into `Deregistering` (message id 1), leaving the logs clean.
fn deregistering_para(who: AccountId) -> u32 {
	let para_id = registered_para(who);
	assert_ok!(Registrar::deregister(RuntimeOrigin::signed(who), para_id));
	let _ = registrar_events();
	let _ = take_sent();
	para_id
}

mod reserve {
	use super::*;

	#[test]
	fn allocates_ids_from_the_first_public_id_upwards() {
		new_test_ext().execute_with(|| {
			assert_ok!(Registrar::reserve(RuntimeOrigin::signed(ALICE)));
			assert_ok!(Registrar::reserve(RuntimeOrigin::signed(BOB)));

			assert_eq!(
				registrar_events(),
				vec![
					Event::Reserved { para_id: FIRST_PARA_ID, who: ALICE },
					Event::Reserved { para_id: FIRST_PARA_ID + 1, who: BOB },
				]
			);
			assert_eq!(crate::NextFreeParaId::<Test>::get(), FIRST_PARA_ID + 2);
		});
	}

	#[test]
	fn holds_the_para_deposit_and_records_the_manager() {
		new_test_ext().execute_with(|| {
			let before = Balances::free_balance(ALICE);
			let para_id = reserve_for(ALICE);

			let info = Paras::<Test>::get(para_id).unwrap();
			assert_eq!(info.manager, ALICE);
			assert_eq!(info.state, RegistrationState::Reserved);
			assert_eq!(held(ALICE), PARA_DEPOSIT);
			assert_eq!(Balances::free_balance(ALICE), before - PARA_DEPOSIT);
		});
	}

	#[test]
	fn steps_over_an_id_the_counter_and_the_map_disagree_about() {
		new_test_ext().execute_with(|| {
			// GIVEN the counter points at an id this pallet already knows. The two can drift:
			// ids may arrive from elsewhere, or the counter may be restored from another chain.
			let alice_id = reserve_for(ALICE); // manager of the id in the way
			crate::NextFreeParaId::<Test>::put(alice_id);

			// WHEN Bob reserves.
			let bob_id = reserve_for(BOB); // next manager in line

			// THEN he is handed the next id up, and the counter is past both. Failing instead
			// would brick `reserve` for every account, permanently.
			assert_eq!(bob_id, alice_id + 1);
			assert_eq!(crate::NextFreeParaId::<Test>::get(), alice_id + 2);
		});
	}

	#[test]
	fn fails_without_the_deposit() {
		new_test_ext().execute_with(|| {
			// Alice keeps only enough to stay alive, well short of the deposit.
			let keep = 10;
			assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), ALICE, keep));

			assert_noop!(
				Registrar::reserve(RuntimeOrigin::signed(ALICE)),
				DispatchError::Token(TokenError::FundsUnavailable)
			);
			// Nothing was handed out.
			assert_eq!(crate::NextFreeParaId::<Test>::get(), 0);
			assert!(Paras::<Test>::iter().next().is_none());
		});
	}
}

mod register {
	use super::*;

	#[test]
	fn holds_the_data_deposit_and_asks_the_relay_chain() {
		new_test_ext().execute_with(|| {
			let para_id = reserve_for(ALICE);
			let head_len = 20;
			let code_len = 300;

			let blob = request_registration(ALICE, para_id, head_len, code_len);

			// Deposit covers head data plus the largest code the relay chain would accept, not
			// the code actually declared. That is what makes an upgrade need no top-up.
			let expected = PER_BYTE * (head_len as Balance + MAX_CODE_SIZE as Balance);
			assert_eq!(held(ALICE), PARA_DEPOSIT + expected);

			let expected_at = System::block_number() + PENDING_DEADLINE;
			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Pending { cancellable_at, .. } if cancellable_at == expected_at
			));
			assert_eq!(
				registrar_events(),
				vec![Event::RegisterRequested { para_id, message_id: 0, manager: ALICE }]
			);
			assert_eq!(
				take_sent(),
				vec![MessageToRelay::V1(MessageToRelayV1::Register {
					para_id,
					message_id: 0,
					manager: ALICE,
					genesis_head: head(head_len),
					code_hash: hash_of(&blob),
					code_len: code_len as u32,
				})]
			);
			assert_eq!(crate::NextMessageId::<Test>::get(), 1);
		});
	}

	#[test]
	fn rejects_an_unreserved_id() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Registrar::register(
					RuntimeOrigin::signed(ALICE),
					FIRST_PARA_ID,
					head(10),
					100,
					hash_of(&code(100)),
				),
				Error::<Test>::NotReserved
			);
		});
	}

	#[test]
	fn rejects_someone_elses_id() {
		new_test_ext().execute_with(|| {
			let para_id = reserve_for(ALICE);
			assert_noop!(
				Registrar::register(
					RuntimeOrigin::signed(BOB),
					para_id,
					head(10),
					100,
					hash_of(&code(100)),
				),
				Error::<Test>::NotOwner
			);
		});
	}

	#[test]
	fn rejects_a_second_request_while_one_is_in_flight() {
		new_test_ext().execute_with(|| {
			let para_id = reserve_for(ALICE);
			request_registration(ALICE, para_id, 10, 100);

			assert_noop!(
				Registrar::register(
					RuntimeOrigin::signed(ALICE),
					para_id,
					head(10),
					100,
					hash_of(&code(100)),
				),
				Error::<Test>::AlreadyRegistered
			);
		});
	}

	#[test]
	fn enforces_the_relay_chains_size_bounds() {
		new_test_ext().execute_with(|| {
			let para_id = reserve_for(ALICE);
			assert_noop!(
				Registrar::register(
					RuntimeOrigin::signed(ALICE),
					para_id,
					head(MAX_HEAD_SIZE as usize + 1),
					100,
					hash_of(&code(100)),
				),
				Error::<Test>::HeadDataTooLarge
			);
			assert_noop!(
				Registrar::register(
					RuntimeOrigin::signed(ALICE),
					para_id,
					head(10),
					MAX_CODE_SIZE + 1,
					hash_of(&code(10)),
				),
				Error::<Test>::CodeTooLarge
			);
			assert_noop!(
				Registrar::register(
					RuntimeOrigin::signed(ALICE),
					para_id,
					head(10),
					MIN_CODE_SIZE - 1,
					hash_of(&code(10)),
				),
				Error::<Test>::CodeTooSmall
			);
		});
	}

	#[test]
	fn a_transport_failure_rolls_the_whole_call_back() {
		new_test_ext().execute_with(|| {
			let para_id = reserve_for(ALICE);
			SendFails::set(true);

			assert_noop!(
				Registrar::register(
					RuntimeOrigin::signed(ALICE),
					para_id,
					head(10),
					100,
					hash_of(&code(100)),
				),
				Error::<Test>::SendFailed
			);

			// The hold, the state change and the message id went with it.
			assert_eq!(held(ALICE), PARA_DEPOSIT);
			assert_eq!(Paras::<Test>::get(para_id).unwrap().state, RegistrationState::Reserved);
			assert_eq!(crate::NextMessageId::<Test>::get(), 0);
		});
	}
}

mod receive {
	use super::*;

	fn result_message(para_id: u32, message_id: u64, outcome: Outcome) -> MessageToPara {
		MessageToPara::V1(MessageToParaV1::RegisterResponse { para_id, message_id, outcome })
	}

	fn cancel_message(para_id: u32, message_id: u64, outcome: Outcome) -> MessageToPara {
		MessageToPara::V1(MessageToParaV1::CancelResponse { para_id, message_id, outcome })
	}

	#[test]
	fn success_finalises_the_registration_and_keeps_the_deposit() {
		new_test_ext().execute_with(|| {
			let para_id = reserve_for(ALICE);
			request_registration(ALICE, para_id, 20, 300);
			let _ = registrar_events();
			let deposit = PER_BYTE * (20 + MAX_CODE_SIZE as Balance);

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				result_message(para_id, 0, Ok(())),
			));

			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Registered { .. }
			));
			assert_eq!(held(ALICE), PARA_DEPOSIT + deposit);
			assert_eq!(
				registrar_events(),
				vec![Event::Registered { para_id, message_id: 0, manager: ALICE }]
			);
		});
	}

	#[test]
	fn failure_releases_the_deposit_and_keeps_the_id_reserved() {
		new_test_ext().execute_with(|| {
			let para_id = reserve_for(ALICE);
			request_registration(ALICE, para_id, 20, 300);
			let _ = registrar_events();

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				result_message(para_id, 0, Err(FailureReason::AlreadyRegistered)),
			));

			// Back to square one, para id still ours, only the para deposit still held.
			assert_eq!(Paras::<Test>::get(para_id).unwrap().state, RegistrationState::Reserved);
			assert_eq!(held(ALICE), PARA_DEPOSIT);
			assert_eq!(
				registrar_events(),
				vec![Event::RegistrationFailed {
					para_id,
					message_id: 0,
					manager: ALICE,
					reason: FailureReason::AlreadyRegistered,
				}]
			);
		});
	}

	#[test]
	fn a_user_cannot_forge_a_relay_chain_report() {
		new_test_ext().execute_with(|| {
			let para_id = reserve_for(ALICE);
			request_registration(ALICE, para_id, 20, 300);

			assert_noop!(
				Registrar::receive(
					RuntimeOrigin::signed(ALICE),
					result_message(para_id, 0, Ok(())),
				),
				DispatchError::BadOrigin
			);
		});
	}

	#[test]
	#[cfg(debug_assertions)]
	#[should_panic(expected = "register response for unknown para, dropping")]
	fn a_report_for_an_unknown_para_is_defensive() {
		new_test_ext().execute_with(|| {
			let _ = Registrar::receive(RuntimeOrigin::root(), result_message(4242, 0, Ok(())));
		});
	}

	#[test]
	#[cfg(debug_assertions)]
	#[should_panic(expected = "register response for para which is not pending, dropping")]
	fn a_report_for_a_non_pending_para_is_defensive() {
		new_test_ext().execute_with(|| {
			let para_id = reserve_for(ALICE);
			let _ = Registrar::receive(RuntimeOrigin::root(), result_message(para_id, 0, Ok(())));
		});
	}

	#[test]
	fn a_refused_cancellation_records_the_registration_and_keeps_the_deposit() {
		new_test_ext().execute_with(|| {
			let para_id = reserve_for(ALICE);
			request_registration(ALICE, para_id, 20, 300);
			let _ = registrar_events();
			let deposit = PER_BYTE * (20 + MAX_CODE_SIZE as Balance);

			// The code landed on the relay chain after all and the success report was lost, so the
			// cancellation comes back refused and the deposit is owed. The event echoes the
			// cancellation's message id, not the registration's.
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				cancel_message(para_id, 1, Err(FailureReason::AlreadyRegistered)),
			));

			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Registered { .. }
			));
			assert_eq!(held(ALICE), PARA_DEPOSIT + deposit);
			assert_eq!(
				registrar_events(),
				vec![Event::Registered { para_id, message_id: 1, manager: ALICE }]
			);
		});
	}

	#[test]
	fn a_cancel_response_that_lost_the_race_to_a_report_is_dropped_quietly() {
		new_test_ext().execute_with(|| {
			let para_id = reserve_for(ALICE);
			request_registration(ALICE, para_id, 20, 300);
			let deposit = PER_BYTE * (20 + MAX_CODE_SIZE as Balance);

			// A verdict already in flight settles the registration first.
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				result_message(para_id, 0, Ok(()))
			));
			let _ = registrar_events();

			// The answer to the cancellation then has nothing left to do. Unlike a stray register
			// response this is expected, so it is not defensive.
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				cancel_message(para_id, 1, Ok(()))
			));

			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Registered { .. }
			));
			assert_eq!(held(ALICE), PARA_DEPOSIT + deposit);
			assert!(registrar_events().is_empty());
		});
	}

	#[test]
	#[cfg(debug_assertions)]
	#[should_panic(expected = "cancel response for unknown para, dropping")]
	fn a_cancel_response_for_an_unknown_para_is_defensive() {
		new_test_ext().execute_with(|| {
			let _ = Registrar::receive(RuntimeOrigin::root(), cancel_message(4242, 0, Ok(())));
		});
	}

	fn deregister_answer(para_id: u32, message_id: u64, outcome: Outcome) -> MessageToPara {
		MessageToPara::V1(MessageToParaV1::DeregisterResponse { para_id, message_id, outcome })
	}

	fn chase_up_answer(para_id: u32, message_id: u64, outcome: Outcome) -> MessageToPara {
		MessageToPara::V1(MessageToParaV1::CancelDeregistrationResponse {
			para_id,
			message_id,
			outcome,
		})
	}

	#[test]
	fn a_confirmed_deregistration_releases_both_deposits_and_frees_the_id() {
		new_test_ext().execute_with(|| {
			let para_id = deregistering_para(ALICE);

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				deregister_answer(para_id, 1, Ok(())),
			));

			assert!(Paras::<Test>::get(para_id).is_none());
			assert_eq!(held(ALICE), 0);
			assert_eq!(registrar_events(), vec![Event::Deregistered { para_id, manager: ALICE }]);
		});
	}

	#[test]
	fn a_refused_deregistration_flips_back_to_registered_and_keeps_the_deposits() {
		new_test_ext().execute_with(|| {
			let para_id = deregistering_para(ALICE);
			let deposit = PER_BYTE * (20 + MAX_CODE_SIZE as Balance);

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				deregister_answer(para_id, 1, Err(FailureReason::Locked)),
			));

			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Registered { .. }
			));
			assert_eq!(held(ALICE), PARA_DEPOSIT + deposit);
			assert_eq!(
				registrar_events(),
				vec![Event::DeregistrationFailed {
					para_id,
					message_id: 1,
					manager: ALICE,
					reason: FailureReason::Locked,
				}]
			);
		});
	}

	#[test]
	#[cfg(debug_assertions)]
	#[should_panic(expected = "deregister response for unknown para, dropping")]
	fn a_deregister_response_for_an_unknown_para_is_defensive() {
		new_test_ext().execute_with(|| {
			let _ = Registrar::receive(RuntimeOrigin::root(), deregister_answer(4242, 0, Ok(())));
		});
	}

	#[test]
	#[cfg(debug_assertions)]
	#[should_panic(expected = "deregister response for para which is not deregistering, dropping")]
	fn a_deregister_response_for_a_para_that_is_not_deregistering_is_defensive() {
		new_test_ext().execute_with(|| {
			let para_id = registered_para(ALICE);
			let _ =
				Registrar::receive(RuntimeOrigin::root(), deregister_answer(para_id, 1, Ok(())));
		});
	}

	#[test]
	fn a_refused_chase_up_completes_the_deregistration() {
		new_test_ext().execute_with(|| {
			let para_id = deregistering_para(ALICE);

			// The deregistration went through and its report was lost, so the chase-up comes
			// back refused and the deposits are owed.
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				chase_up_answer(para_id, 2, Err(FailureReason::NotRegistered)),
			));

			assert!(Paras::<Test>::get(para_id).is_none());
			assert_eq!(held(ALICE), 0);
			assert_eq!(registrar_events(), vec![Event::Deregistered { para_id, manager: ALICE }]);
		});
	}

	#[test]
	fn a_chase_up_answer_that_lost_the_race_to_the_verdict_is_dropped_quietly() {
		new_test_ext().execute_with(|| {
			let para_id = deregistering_para(ALICE);

			// The verdict settles the deregistration first and removes the entry.
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				deregister_answer(para_id, 1, Ok(())),
			));
			let _ = registrar_events();

			// The chase-up's answer then finds no para at all. Unlike a stray deregister
			// response this is expected, so it is not defensive.
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				chase_up_answer(para_id, 2, Err(FailureReason::NotRegistered)),
			));

			assert!(Paras::<Test>::get(para_id).is_none());
			assert_eq!(held(ALICE), 0);
			assert!(registrar_events().is_empty());
		});
	}

	#[test]
	fn a_chase_up_answer_for_a_registered_para_is_dropped_quietly() {
		new_test_ext().execute_with(|| {
			let para_id = deregistering_para(ALICE);
			let deposit = PER_BYTE * (20 + MAX_CODE_SIZE as Balance);

			// The verdict was a refusal, so the para is registered again by the time the
			// chase-up's answer arrives.
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				deregister_answer(para_id, 1, Err(FailureReason::Locked)),
			));
			let _ = registrar_events();

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				chase_up_answer(para_id, 2, Ok(())),
			));

			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Registered { .. }
			));
			assert_eq!(held(ALICE), PARA_DEPOSIT + deposit);
			assert!(registrar_events().is_empty());
		});
	}

	#[test]
	#[cfg(debug_assertions)]
	#[should_panic(expected = "unexpected cancel deregistration refusal, leaving deregistering")]
	fn an_unexpected_chase_up_refusal_is_defensive_and_leaves_state() {
		new_test_ext().execute_with(|| {
			let para_id = deregistering_para(ALICE);
			let _ = Registrar::receive(
				RuntimeOrigin::root(),
				chase_up_answer(para_id, 2, Err(FailureReason::AlreadyRegistered)),
			);
		});
	}

	#[test]
	fn a_user_cannot_forge_a_deregister_report() {
		new_test_ext().execute_with(|| {
			let para_id = deregistering_para(ALICE);

			assert_noop!(
				Registrar::receive(
					RuntimeOrigin::signed(ALICE),
					deregister_answer(para_id, 1, Ok(())),
				),
				DispatchError::BadOrigin
			);
			assert_noop!(
				Registrar::receive(
					RuntimeOrigin::signed(ALICE),
					chase_up_answer(para_id, 2, Ok(())),
				),
				DispatchError::BadOrigin
			);
		});
	}
}

mod cancel_registration {
	use super::*;

	fn cancel_request(para_id: u32, message_id: u64) -> MessageToRelay<AccountId> {
		MessageToRelay::V1(MessageToRelayV1::CancelRegistration { para_id, message_id })
	}

	fn cancel_confirmation(para_id: u32, message_id: u64) -> MessageToPara {
		MessageToPara::V1(MessageToParaV1::CancelResponse { para_id, message_id, outcome: Ok(()) })
	}

	#[test]
	fn is_refused_before_the_deadline() {
		new_test_ext().execute_with(|| {
			let para_id = reserve_for(ALICE);
			request_registration(ALICE, para_id, 20, 300);

			run_to_block(System::block_number() + PENDING_DEADLINE - 1);
			assert_noop!(
				Registrar::cancel_registration(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::CannotCancelYet
			);
		});
	}

	#[test]
	fn asks_the_relay_chain_and_only_the_answer_releases_the_deposit() {
		new_test_ext().execute_with(|| {
			let para_id = reserve_for(ALICE);
			request_registration(ALICE, para_id, 20, 300);
			let _ = registrar_events();
			let _ = take_sent();
			let deposit = PER_BYTE * (20 + MAX_CODE_SIZE as Balance);

			run_to_block(System::block_number() + PENDING_DEADLINE);
			assert_ok!(Registrar::cancel_registration(RuntimeOrigin::signed(ALICE), para_id));

			// The relay chain has been asked, and until it answers nothing is given back: it is the
			// only side that knows whether the code landed.
			let expected_at = System::block_number() + PENDING_DEADLINE;
			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Pending { cancellable_at, .. } if cancellable_at == expected_at
			));
			assert_eq!(held(ALICE), PARA_DEPOSIT + deposit);
			// The registration took message id 0, so the cancellation is message 1.
			assert_eq!(take_sent(), vec![cancel_request(para_id, 1)]);
			assert_eq!(
				registrar_events(),
				vec![Event::CancelRequested { para_id, message_id: 1, manager: ALICE }]
			);

			assert_ok!(Registrar::receive(RuntimeOrigin::root(), cancel_confirmation(para_id, 1)));

			assert_eq!(Paras::<Test>::get(para_id).unwrap().state, RegistrationState::Reserved);
			assert_eq!(held(ALICE), PARA_DEPOSIT);
			assert_eq!(
				registrar_events(),
				vec![Event::RegistrationCancelled { para_id, message_id: 1, manager: ALICE }]
			);

			// And the manager can simply try again on the same id.
			request_registration(ALICE, para_id, 20, 300);
		});
	}

	#[test]
	fn cannot_be_asked_again_until_another_deadline_passes() {
		new_test_ext().execute_with(|| {
			let para_id = reserve_for(ALICE);
			request_registration(ALICE, para_id, 20, 300);
			run_to_block(System::block_number() + PENDING_DEADLINE);
			assert_ok!(Registrar::cancel_registration(RuntimeOrigin::signed(ALICE), para_id));
			let _ = take_sent();

			// One request per deadline, so a cancellation that goes missing can be retried without
			// the relay chain being asked once per block.
			run_to_block(System::block_number() + PENDING_DEADLINE - 1);
			assert_noop!(
				Registrar::cancel_registration(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::CannotCancelYet
			);

			run_to_block(System::block_number() + 1);
			assert_ok!(Registrar::cancel_registration(RuntimeOrigin::signed(ALICE), para_id));
			// Register was 0, the first cancellation 1, so the retry carries 2.
			assert_eq!(take_sent(), vec![cancel_request(para_id, 2)]);
		});
	}

	#[test]
	fn a_transport_failure_rolls_the_whole_call_back() {
		new_test_ext().execute_with(|| {
			let para_id = reserve_for(ALICE);
			request_registration(ALICE, para_id, 20, 300);
			let cancellable_at = System::block_number() + PENDING_DEADLINE;
			run_to_block(cancellable_at);
			SendFails::set(true);

			assert_noop!(
				Registrar::cancel_registration(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::SendFailed
			);

			// The pushed-out deadline went with it, so the manager can retry immediately.
			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Pending { cancellable_at: at, .. } if at == cancellable_at
			));
			assert_eq!(held(ALICE), PARA_DEPOSIT + PER_BYTE * (20 + MAX_CODE_SIZE as Balance));
			SendFails::set(false);
			assert_ok!(Registrar::cancel_registration(RuntimeOrigin::signed(ALICE), para_id));
		});
	}

	#[test]
	fn rejects_a_non_manager_and_a_para_that_is_not_pending() {
		new_test_ext().execute_with(|| {
			let para_id = reserve_for(ALICE);

			// Reserved, not pending.
			assert_noop!(
				Registrar::cancel_registration(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::NotPending
			);

			request_registration(ALICE, para_id, 20, 300);
			run_to_block(System::block_number() + PENDING_DEADLINE);
			assert_noop!(
				Registrar::cancel_registration(RuntimeOrigin::signed(BOB), para_id),
				Error::<Test>::NotOwner
			);
		});
	}
}

mod deregister {
	use super::*;

	#[test]
	fn a_reserved_id_is_dropped_locally_and_the_deposit_comes_back() {
		new_test_ext().execute_with(|| {
			let para_id = reserve_for(ALICE);
			assert_eq!(held(ALICE), PARA_DEPOSIT);

			assert_ok!(Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id));

			assert!(Paras::<Test>::get(para_id).is_none());
			assert_eq!(held(ALICE), 0);
			assert_eq!(registrar_events(), vec![Event::Deregistered { para_id, manager: ALICE }]);
			// The relay chain never heard of this id, so there is no round trip.
			assert!(take_sent().is_empty());
			assert_eq!(crate::NextMessageId::<Test>::get(), 0);
		});
	}

	#[test]
	fn a_registered_para_asks_the_relay_chain_and_releases_nothing() {
		new_test_ext().execute_with(|| {
			let para_id = registered_para(ALICE);
			let deposit = PER_BYTE * (20 + MAX_CODE_SIZE as Balance);

			assert_ok!(Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id));

			// The relay chain has been asked, and until it answers nothing is given back.
			let expected_at = System::block_number() + PENDING_DEADLINE;
			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Deregistering { cancellable_at, .. }
					if cancellable_at == expected_at
			));
			assert_eq!(held(ALICE), PARA_DEPOSIT + deposit);
			// The registration took message id 0, so this is message 1.
			assert_eq!(
				take_sent(),
				vec![MessageToRelay::V1(MessageToRelayV1::Deregister {
					para_id,
					message_id: 1,
				})]
			);
			assert_eq!(
				registrar_events(),
				vec![Event::DeregisterRequested { para_id, message_id: 1, manager: ALICE }]
			);
		});
	}

	#[test]
	fn a_transport_failure_rolls_the_whole_call_back() {
		new_test_ext().execute_with(|| {
			let para_id = registered_para(ALICE);
			SendFails::set(true);

			assert_noop!(
				Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::SendFailed
			);

			// The state change and the message id went with it.
			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Registered { .. }
			));
			assert_eq!(crate::NextMessageId::<Test>::get(), 1);
		});
	}

	#[test]
	fn rejects_an_unknown_id_a_non_manager_and_in_flight_states() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Registrar::deregister(RuntimeOrigin::signed(ALICE), 4242),
				Error::<Test>::NotReserved
			);

			let para_id = reserve_for(ALICE);
			assert_noop!(
				Registrar::deregister(RuntimeOrigin::signed(BOB), para_id),
				Error::<Test>::NotOwner
			);

			// A registration in flight has to be settled or cancelled first.
			request_registration(ALICE, para_id, 20, 300);
			assert_noop!(
				Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::RequestInFlight
			);

			// And so does a deregistration already in flight.
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				MessageToPara::V1(MessageToParaV1::RegisterResponse {
					para_id,
					message_id: 0,
					outcome: Ok(()),
				}),
			));
			assert_ok!(Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id));
			assert_noop!(
				Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::RequestInFlight
			);
		});
	}

	#[test]
	fn the_para_itself_and_root_may_deregister() {
		new_test_ext().execute_with(|| {
			// Another para cannot pose as this one.
			let para_id = reserve_for(ALICE);
			assert_noop!(
				Registrar::deregister(para_origin(para_id + 1), para_id),
				Error::<Test>::NotOwner
			);

			// The para itself drops its reserved id; the deposit goes back to the manager.
			assert_ok!(Registrar::deregister(para_origin(para_id), para_id));
			assert!(Paras::<Test>::get(para_id).is_none());
			assert_eq!(held(ALICE), 0);
			assert_eq!(registrar_events(), vec![Event::Deregistered { para_id, manager: ALICE }]);

			// Root drives a registered para's deregistration; the request still names the
			// manager, whose deposits stay held until the relay chain answers.
			let para_id = registered_para(ALICE);
			assert_ok!(Registrar::deregister(RuntimeOrigin::root(), para_id));
			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Deregistering { .. }
			));
			assert_eq!(
				take_sent(),
				vec![MessageToRelay::V1(MessageToRelayV1::Deregister {
					para_id,
					message_id: 1,
				})]
			);
		});
	}

	#[test]
	fn a_para_still_holding_an_assignment_is_refused() {
		new_test_ext().execute_with(|| {
			let para_id = registered_para(ALICE);
			AssignedParas::set(vec![para_id]);

			// The manager is locked out before the state is even looked at: holding a core is
			// what locks a para here.
			assert_noop!(
				Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::ParaLocked
			);
			// Root is not locked out, so it reaches the assignment check itself — a para that can
			// still be scheduled must not be removed out from under itself, whoever asks.
			assert_noop!(
				Registrar::deregister(RuntimeOrigin::root(), para_id),
				Error::<Test>::StillAssigned
			);
			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Registered { .. }
			));

			// A merely reserved id is never locked by an assignment and is always droppable by
			// its manager. It cannot hold a core, so treating a stray entry as a lock would
			// strand the reservation deposit behind a governance call for nothing.
			let reserved = reserve_for(ALICE);
			AssignedParas::mutate(|assigned| assigned.push(reserved));
			assert_ok!(Registrar::deregister(RuntimeOrigin::signed(ALICE), reserved));
		});
	}
}

mod cancel_deregistration {
	use super::*;

	fn chase_up_request(para_id: u32, message_id: u64) -> MessageToRelay<AccountId> {
		MessageToRelay::V1(MessageToRelayV1::CancelDeregistration { para_id, message_id })
	}

	fn chase_up_confirmation(para_id: u32, message_id: u64) -> MessageToPara {
		MessageToPara::V1(MessageToParaV1::CancelDeregistrationResponse {
			para_id,
			message_id,
			outcome: Ok(()),
		})
	}

	#[test]
	fn is_refused_before_the_deadline() {
		new_test_ext().execute_with(|| {
			let para_id = deregistering_para(ALICE);

			run_to_block(System::block_number() + PENDING_DEADLINE - 1);
			assert_noop!(
				Registrar::cancel_deregistration(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::CannotCancelYet
			);
		});
	}

	#[test]
	fn asks_the_relay_chain_and_only_the_answer_settles_it() {
		new_test_ext().execute_with(|| {
			let para_id = deregistering_para(ALICE);
			let deposit = PER_BYTE * (20 + MAX_CODE_SIZE as Balance);

			run_to_block(System::block_number() + PENDING_DEADLINE);
			assert_ok!(Registrar::cancel_deregistration(RuntimeOrigin::signed(ALICE), para_id));

			// Only the relay chain knows whether the deregistration went through, so nothing is
			// settled until it answers.
			let expected_at = System::block_number() + PENDING_DEADLINE;
			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Deregistering { cancellable_at, .. }
					if cancellable_at == expected_at
			));
			assert_eq!(held(ALICE), PARA_DEPOSIT + deposit);
			// Register was 0 and the deregistration 1, so the chase-up carries 2.
			assert_eq!(take_sent(), vec![chase_up_request(para_id, 2)]);
			assert_eq!(
				registrar_events(),
				vec![Event::CancelDeregistrationRequested {
					para_id,
					message_id: 2,
					manager: ALICE,
				}]
			);

			// The deregistration never happened on the relay chain, so the para is registered
			// again and the deposits stay held.
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				chase_up_confirmation(para_id, 2),
			));

			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Registered { .. }
			));
			assert_eq!(held(ALICE), PARA_DEPOSIT + deposit);
			assert_eq!(
				registrar_events(),
				vec![Event::DeregistrationCancelled { para_id, message_id: 2, manager: ALICE }]
			);

			// And the manager can simply try again.
			assert_ok!(Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id));
		});
	}

	#[test]
	fn cannot_be_asked_again_until_another_deadline_passes() {
		new_test_ext().execute_with(|| {
			let para_id = deregistering_para(ALICE);
			run_to_block(System::block_number() + PENDING_DEADLINE);
			assert_ok!(Registrar::cancel_deregistration(RuntimeOrigin::signed(ALICE), para_id));
			let _ = take_sent();

			run_to_block(System::block_number() + PENDING_DEADLINE - 1);
			assert_noop!(
				Registrar::cancel_deregistration(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::CannotCancelYet
			);

			run_to_block(System::block_number() + 1);
			assert_ok!(Registrar::cancel_deregistration(RuntimeOrigin::signed(ALICE), para_id));
			// Register was 0, the deregistration 1, the first chase-up 2, so the retry carries 3.
			assert_eq!(take_sent(), vec![chase_up_request(para_id, 3)]);
		});
	}

	#[test]
	fn a_transport_failure_rolls_the_whole_call_back() {
		new_test_ext().execute_with(|| {
			let para_id = deregistering_para(ALICE);
			let cancellable_at = System::block_number() + PENDING_DEADLINE;
			run_to_block(cancellable_at);
			SendFails::set(true);

			assert_noop!(
				Registrar::cancel_deregistration(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::SendFailed
			);

			// The pushed-out deadline went with it, so the manager can retry immediately.
			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Deregistering { cancellable_at: at, .. }
					if at == cancellable_at
			));
			SendFails::set(false);
			assert_ok!(Registrar::cancel_deregistration(RuntimeOrigin::signed(ALICE), para_id));
		});
	}

	#[test]
	fn the_para_itself_and_root_may_chase_up() {
		new_test_ext().execute_with(|| {
			let para_id = deregistering_para(ALICE);
			run_to_block(System::block_number() + PENDING_DEADLINE);

			// Another para cannot pose as this one.
			assert_noop!(
				Registrar::cancel_deregistration(para_origin(para_id + 1), para_id),
				Error::<Test>::NotOwner
			);

			// Register was 0 and the deregistration 1, so the para's chase-up carries 2.
			assert_ok!(Registrar::cancel_deregistration(para_origin(para_id), para_id));
			assert_eq!(take_sent(), vec![chase_up_request(para_id, 2)]);

			run_to_block(System::block_number() + PENDING_DEADLINE);
			assert_ok!(Registrar::cancel_deregistration(RuntimeOrigin::root(), para_id));
			assert_eq!(take_sent(), vec![chase_up_request(para_id, 3)]);

			// Events name the manager, whoever asked.
			assert_eq!(
				registrar_events(),
				vec![
					Event::CancelDeregistrationRequested { para_id, message_id: 2, manager: ALICE },
					Event::CancelDeregistrationRequested { para_id, message_id: 3, manager: ALICE },
				]
			);
		});
	}

	#[test]
	fn rejects_a_non_manager_and_a_para_that_is_not_deregistering() {
		new_test_ext().execute_with(|| {
			let para_id = reserve_for(ALICE);

			// Reserved, not deregistering.
			assert_noop!(
				Registrar::cancel_deregistration(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::NotDeregistering
			);

			// Pending, not deregistering.
			request_registration(ALICE, para_id, 20, 300);
			assert_noop!(
				Registrar::cancel_deregistration(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::NotDeregistering
			);

			// Registered, not deregistering.
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				MessageToPara::V1(MessageToParaV1::RegisterResponse {
					para_id,
					message_id: 0,
					outcome: Ok(()),
				}),
			));
			assert_noop!(
				Registrar::cancel_deregistration(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::NotDeregistering
			);

			assert_ok!(Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id));
			run_to_block(System::block_number() + PENDING_DEADLINE);
			assert_noop!(
				Registrar::cancel_deregistration(RuntimeOrigin::signed(BOB), para_id),
				Error::<Test>::NotOwner
			);
		});
	}
}

mod locking {
	use super::*;

	#[test]
	fn the_manager_the_para_and_root_may_all_lock() {
		new_test_ext().execute_with(|| {
			// GIVEN three registered paras under three managers.
			let alice_para = registered_para(ALICE); // locked by its manager
			let bob_para = registered_para(BOB); // locked by the para itself
			let root_para = registered_para(ALICE); // second para of Alice's, locked by root

			// WHEN each is locked through a different origin.
			assert_ok!(Registrar::add_lock(RuntimeOrigin::signed(ALICE), alice_para));
			assert_ok!(Registrar::add_lock(para_origin(bob_para), bob_para));
			assert_ok!(Registrar::add_lock(RuntimeOrigin::root(), root_para));

			// THEN all three are locked, and nothing was sent anywhere: the lock is local state.
			assert!(Paras::<Test>::get(alice_para).unwrap().locked);
			assert!(Paras::<Test>::get(bob_para).unwrap().locked);
			assert!(Paras::<Test>::get(root_para).unwrap().locked);
			assert_eq!(take_sent(), vec![]);
			assert_eq!(
				registrar_events(),
				vec![
					Event::Locked { para_id: alice_para },
					Event::Locked { para_id: bob_para },
					Event::Locked { para_id: root_para },
				]
			);
		});
	}

	#[test]
	fn locking_twice_is_not_an_error_and_says_nothing_the_second_time() {
		new_test_ext().execute_with(|| {
			let para_id = registered_para(ALICE); // manager races its own lock
			assert_ok!(Registrar::add_lock(RuntimeOrigin::signed(ALICE), para_id));
			let _ = registrar_events();

			// A manager who is already locked out may still lock: the call is about the state it
			// leaves behind, not about who is allowed to reach it.
			assert_ok!(Registrar::add_lock(RuntimeOrigin::signed(ALICE), para_id));

			assert!(Paras::<Test>::get(para_id).unwrap().locked);
			assert_eq!(registrar_events(), vec![]);
		});
	}

	#[test]
	fn only_the_para_and_root_may_unlock() {
		new_test_ext().execute_with(|| {
			// GIVEN a locked para.
			let para_id = registered_para(ALICE); // manager, locked out below
			assert_ok!(Registrar::add_lock(RuntimeOrigin::signed(ALICE), para_id));

			// WHEN its manager tries to lift the lock. THEN they cannot: a lock exists to
			// protect the para from whoever manages it.
			assert_noop!(
				Registrar::remove_lock(RuntimeOrigin::signed(ALICE), para_id),
				DispatchError::BadOrigin
			);
			assert!(Paras::<Test>::get(para_id).unwrap().locked);

			// WHEN the para itself asks. THEN it is unlocked.
			assert_ok!(Registrar::remove_lock(para_origin(para_id), para_id));
			assert!(!Paras::<Test>::get(para_id).unwrap().locked);

			// And root may do the same.
			assert_ok!(Registrar::add_lock(RuntimeOrigin::signed(ALICE), para_id));
			assert_ok!(Registrar::remove_lock(RuntimeOrigin::root(), para_id));
			assert!(!Paras::<Test>::get(para_id).unwrap().locked);
		});
	}

	#[test]
	fn a_lock_shuts_the_manager_out_of_every_call_it_gates() {
		new_test_ext().execute_with(|| {
			// GIVEN a locked, registered para.
			let para_id = registered_para(ALICE); // manager, locked out
			assert_ok!(Registrar::add_lock(RuntimeOrigin::signed(ALICE), para_id));
			let _ = take_sent();

			// THEN the manager reaches none of the three calls the lock gates, and no message is
			// built for any of them.
			assert_noop!(
				Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::ParaLocked
			);
			assert_noop!(
				Registrar::schedule_code_upgrade(
					RuntimeOrigin::signed(ALICE),
					para_id,
					MIN_CODE_SIZE,
					hash_of(&code(MIN_CODE_SIZE as usize)),
				),
				Error::<Test>::ParaLocked
			);
			assert_noop!(
				Registrar::set_current_head(RuntimeOrigin::signed(ALICE), para_id, head(4)),
				Error::<Test>::ParaLocked
			);
			assert_eq!(take_sent(), vec![]);
		});
	}

	#[test]
	fn a_lock_does_not_shut_out_the_para_or_root() {
		new_test_ext().execute_with(|| {
			// GIVEN a locked para.
			let para_id = registered_para(ALICE);
			assert_ok!(Registrar::add_lock(RuntimeOrigin::signed(ALICE), para_id));
			let _ = take_sent();

			// THEN the para itself still gets through: the lock is aimed at the manager alone.
			assert_ok!(Registrar::set_current_head(para_origin(para_id), para_id, head(4)));
			assert_ok!(Registrar::deregister(RuntimeOrigin::root(), para_id));

			assert_eq!(take_sent().len(), 2);
		});
	}

	#[test]
	fn holding_a_core_locks_the_para_against_its_manager() {
		new_test_ext().execute_with(|| {
			// GIVEN a registered para that has since been assigned coretime. This chain hosts
			// coretime, so it can ask directly rather than waiting for a hook.
			let para_id = registered_para(ALICE); // manager
			AssignedParas::mutate(|v| v.push(para_id));
			let _ = take_sent();

			// THEN the manager is shut out of everything a lock gates, even though nobody set the
			// stored flag. This is what replaces the relay chain's lock-at-first-head.
			assert!(!Paras::<Test>::get(para_id).unwrap().locked);
			assert_noop!(
				Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::ParaLocked
			);
			assert_noop!(
				Registrar::schedule_code_upgrade(
					RuntimeOrigin::signed(ALICE),
					para_id,
					MIN_CODE_SIZE,
					hash_of(&code(MIN_CODE_SIZE as usize)),
				),
				Error::<Test>::ParaLocked
			);
			assert_noop!(
				Registrar::set_current_head(RuntimeOrigin::signed(ALICE), para_id, head(4)),
				Error::<Test>::ParaLocked
			);
			assert_eq!(take_sent(), vec![]);

			// The para itself and root are unaffected: a lock protects the para from its manager.
			assert_ok!(Registrar::set_current_head(para_origin(para_id), para_id, head(4)));

			// WHEN the core lapses. THEN the manager has control again, because nothing made the
			// lock stick. Where that is not wanted, add_lock is what makes it permanent.
			AssignedParas::set(Vec::new());
			assert_ok!(Registrar::set_current_head(RuntimeOrigin::signed(ALICE), para_id, head(4)));
		});
	}

	#[test]
	fn a_fresh_registration_starts_unlocked() {
		new_test_ext().execute_with(|| {
			// A manager who has just made a mistake can undo it. What protects a para that is
			// actually in use is the assignment check, not this flag.
			let para_id = registered_para(ALICE);

			assert!(!Paras::<Test>::get(para_id).unwrap().locked);
		});
	}
}

mod schedule_code_upgrade {
	use super::*;

	#[test]
	fn asks_the_relay_chain_and_takes_no_deposit() {
		new_test_ext().execute_with(|| {
			// GIVEN a registered para whose deposit already covers the largest allowed code.
			let para_id = registered_para(ALICE); // manager
			let held_before = held(ALICE);
			let blob = code(MAX_CODE_SIZE as usize);

			// WHEN the manager upgrades to the largest code the relay chain would accept.
			assert_ok!(Registrar::schedule_code_upgrade(
				RuntimeOrigin::signed(ALICE),
				para_id,
				MAX_CODE_SIZE,
				hash_of(&blob),
			));

			// THEN nothing more is held: pricing at MaxCodeSize is what makes upgrades free.
			assert_eq!(held(ALICE), held_before);
			assert_eq!(
				take_sent(),
				vec![MessageToRelay::V1(MessageToRelayV1::AuthorizeCodeUpgrade {
					para_id,
					message_id: 1,
					code_hash: hash_of(&blob),
					code_len: MAX_CODE_SIZE,
				})]
			);
			assert_eq!(
				registrar_events(),
				vec![Event::CodeUpgradeRequested {
					para_id,
					message_id: 1,
					code_hash: hash_of(&blob)
				}]
			);
		});
	}

	#[test]
	fn refuses_code_outside_the_relay_chains_bounds() {
		new_test_ext().execute_with(|| {
			let para_id = registered_para(ALICE); // manager

			assert_noop!(
				Registrar::schedule_code_upgrade(
					RuntimeOrigin::signed(ALICE),
					para_id,
					MIN_CODE_SIZE - 1,
					hash_of(&code(1)),
				),
				Error::<Test>::CodeTooSmall
			);
			assert_noop!(
				Registrar::schedule_code_upgrade(
					RuntimeOrigin::signed(ALICE),
					para_id,
					MAX_CODE_SIZE + 1,
					hash_of(&code(1)),
				),
				Error::<Test>::CodeTooLarge
			);
			assert_eq!(take_sent(), vec![]);
		});
	}

	#[test]
	fn refuses_a_para_that_is_not_registered_yet() {
		new_test_ext().execute_with(|| {
			// GIVEN an id that is only reserved: there is nothing on the relay chain to upgrade.
			let para_id = reserve_for(ALICE); // manager

			assert_noop!(
				Registrar::schedule_code_upgrade(
					RuntimeOrigin::signed(ALICE),
					para_id,
					MIN_CODE_SIZE,
					hash_of(&code(MIN_CODE_SIZE as usize)),
				),
				Error::<Test>::NotRegistered
			);
			assert_eq!(take_sent(), vec![]);
		});
	}

	#[test]
	fn refuses_anybody_but_the_manager_the_para_and_root() {
		new_test_ext().execute_with(|| {
			let para_id = registered_para(ALICE); // manager

			assert_noop!(
				Registrar::schedule_code_upgrade(
					RuntimeOrigin::signed(BOB), // not the manager
					para_id,
					MIN_CODE_SIZE,
					hash_of(&code(MIN_CODE_SIZE as usize)),
				),
				Error::<Test>::NotOwner
			);
			assert_eq!(take_sent(), vec![]);
		});
	}

	#[test]
	fn a_transport_failure_rolls_the_whole_call_back() {
		new_test_ext().execute_with(|| {
			let para_id = registered_para(ALICE); // manager
			SendFails::set(true);

			assert_noop!(
				Registrar::schedule_code_upgrade(
					RuntimeOrigin::signed(ALICE),
					para_id,
					MIN_CODE_SIZE,
					hash_of(&code(MIN_CODE_SIZE as usize)),
				),
				Error::<Test>::SendFailed
			);
			// The message id was consumed inside the rolled-back call, so it is not spent.
			assert_eq!(crate::NextMessageId::<Test>::get(), 1);
		});
	}
}

mod set_current_head {
	use super::*;

	#[test]
	fn sends_the_head_inline() {
		new_test_ext().execute_with(|| {
			let para_id = registered_para(ALICE); // manager
			let new_head = head(MAX_HEAD_SIZE as usize);

			assert_ok!(Registrar::set_current_head(
				RuntimeOrigin::signed(ALICE),
				para_id,
				new_head.clone()
			));

			// Head data is small enough to travel whole, so there is no upload step.
			assert_eq!(
				take_sent(),
				vec![MessageToRelay::V1(MessageToRelayV1::SetCurrentHead {
					para_id,
					message_id: 1,
					head: new_head,
				})]
			);
			assert_eq!(
				registrar_events(),
				vec![Event::HeadUpdateRequested { para_id, message_id: 1 }]
			);
		});
	}

	#[test]
	fn refuses_head_data_the_relay_chain_would_not_take() {
		new_test_ext().execute_with(|| {
			let para_id = registered_para(ALICE); // manager

			assert_noop!(
				Registrar::set_current_head(
					RuntimeOrigin::signed(ALICE),
					para_id,
					head(MAX_HEAD_SIZE as usize + 1)
				),
				Error::<Test>::HeadDataTooLarge
			);
			assert_eq!(take_sent(), vec![]);
		});
	}

	#[test]
	fn refuses_a_para_that_is_not_registered_yet() {
		new_test_ext().execute_with(|| {
			let para_id = reserve_for(ALICE); // manager

			assert_noop!(
				Registrar::set_current_head(RuntimeOrigin::signed(ALICE), para_id, head(4)),
				Error::<Test>::NotRegistered
			);
			assert_eq!(take_sent(), vec![]);
		});
	}
}

mod force_set_next_free_para_id {
	use super::*;

	#[test]
	fn root_moves_the_counter_and_nobody_else_can() {
		new_test_ext().execute_with(|| {
			let target = FIRST_PARA_ID + 400;

			assert_noop!(
				Registrar::force_set_next_free_para_id(RuntimeOrigin::signed(ALICE), target),
				DispatchError::BadOrigin
			);

			assert_ok!(Registrar::force_set_next_free_para_id(RuntimeOrigin::root(), target));

			assert_eq!(crate::NextFreeParaId::<Test>::get(), target);
			assert_eq!(registrar_events(), vec![Event::NextFreeParaIdSet { para_id: target }]);
			// And the next reservation picks up from there.
			assert_eq!(reserve_for(ALICE), target);
		});
	}
}

mod force_remove_para {
	use super::*;

	#[test]
	fn root_repairs_a_record_the_two_chains_can_no_longer_agree_on() {
		new_test_ext().execute_with(|| {
			// GIVEN a registered para. Suppose governance has since removed it on the relay chain
			// directly — this chain would never hear, and the deposits would sit here forever
			// against a para it can no longer act on.
			let para_id = registered_para(ALICE); // manager
			let before = Balances::free_balance(ALICE);
			assert_eq!(held(ALICE), PARA_DEPOSIT + PER_BYTE * (20 + MAX_CODE_SIZE as Balance));

			assert_noop!(
				Registrar::force_remove_para(RuntimeOrigin::signed(ALICE), para_id),
				DispatchError::BadOrigin
			);

			assert_ok!(Registrar::force_remove_para(RuntimeOrigin::root(), para_id));

			// Both deposits go back to the manager who paid them, and the record is gone.
			assert_eq!(held(ALICE), 0);
			assert_eq!(
				Balances::free_balance(ALICE),
				before + PARA_DEPOSIT + PER_BYTE * (20 + MAX_CODE_SIZE as Balance)
			);
			assert!(Paras::<Test>::get(para_id).is_none());
			assert_eq!(take_sent(), vec![]);
			assert_eq!(
				registrar_events(),
				vec![Event::ParaForceRemoved { para_id, manager: ALICE }]
			);
		});
	}

	#[test]
	fn a_verdict_that_is_merely_slow_cannot_be_torn_down() {
		new_test_ext().execute_with(|| {
			// GIVEN a registration still waiting on the relay chain.
			let para_id = reserve_for(ALICE);
			request_registration(ALICE, para_id, 20, 300);

			// THEN governance must wait: tearing this down early would release a deposit for a
			// para the relay chain may be about to confirm.
			assert_noop!(
				Registrar::force_remove_para(RuntimeOrigin::root(), para_id),
				Error::<Test>::CannotCancelYet
			);

			run_to_block(System::block_number() + PENDING_DEADLINE);
			assert_ok!(Registrar::force_remove_para(RuntimeOrigin::root(), para_id));
			assert_eq!(held(ALICE), 0);
		});
	}

	#[test]
	fn a_para_this_chain_never_knew_is_refused() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Registrar::force_remove_para(RuntimeOrigin::root(), FIRST_PARA_ID),
				Error::<Test>::NotReserved
			);
		});
	}
}
