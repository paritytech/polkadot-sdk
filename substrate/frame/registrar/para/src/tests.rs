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

use crate::{mock::*, Error, Event, HoldReason, Paras, RegistrationState, UnexpectedKind};
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
/// The id the most recent message to the relay chain went out with.
fn last_message_id() -> u64 {
	crate::NextMessageId::<Test>::get() - 1
}

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

/// Reserve, register and confirm a para for `who`, leaving the logs clean.
///
/// The head data is 20 bytes, so the registration deposit is `PER_BYTE * (20 + MAX_CODE_SIZE)`.
fn registered_para(who: AccountId) -> u32 {
	let para_id = reserve_for(who);
	request_registration(who, para_id, 20, 300);
	assert_ok!(Registrar::receive(
		RuntimeOrigin::root(),
		MessageToPara::V1(MessageToParaV1::RegisterResponse {
			para_id,
			message_id: last_message_id(),
			outcome: Ok(()),
		}),
	));
	let _ = registrar_events();
	let _ = take_sent();
	para_id
}

/// Lock `who`'s registered para, leaving the logs clean.
fn locked_para(who: AccountId) -> u32 {
	let para_id = registered_para(who);
	assert_ok!(Registrar::add_lock(RuntimeOrigin::signed(who), para_id));
	let _ = registrar_events();
	para_id
}

/// Reserve, register and ask to deregister `who`'s para (message id 1), leaving the logs clean.
fn deregistering_para(who: AccountId) -> u32 {
	let para_id = registered_para(who);
	assert_ok!(Registrar::deregister(RuntimeOrigin::signed(who), para_id));
	let _ = registrar_events();
	let _ = take_sent();
	para_id
}

fn deregister_request(para_id: u32, message_id: u64) -> MessageToRelay<AccountId> {
	MessageToRelay::V1(MessageToRelayV1::Deregister { para_id, message_id })
}

fn deregister_answer(para_id: u32, message_id: u64, outcome: Outcome) -> MessageToPara {
	MessageToPara::V1(MessageToParaV1::DeregisterResponse { para_id, message_id, outcome })
}

/// A system para id, below `FIRST_PARA_ID` and so out of reach of `reserve`.
const SYSTEM_PARA_ID: u32 = 100;

mod reserve {
	use super::*;

	#[test]
	fn allocates_ids_from_the_first_public_id_upwards() {
		build_and_execute(|| {
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
		build_and_execute(|| {
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
	fn fails_without_the_deposit() {
		build_and_execute(|| {
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
		build_and_execute(|| {
			let para_id = reserve_for(ALICE);
			let head_len = 20;
			let code_len = 300;

			let blob = request_registration(ALICE, para_id, head_len, code_len);

			// Deposit covers head data plus a full-sized code, not the declared 300 bytes.
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
		build_and_execute(|| {
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
		build_and_execute(|| {
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
		build_and_execute(|| {
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
		build_and_execute(|| {
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
	fn follows_the_host_configuration() {
		build_and_execute(|| {
			let para_id = reserve_for(ALICE);
			let max_code_size = MAX_CODE_SIZE * 2;
			HostConfig::set(Some(host_config(max_code_size, MAX_HEAD_SIZE)));

			// A code past the old limit now fits, and the deposit is sized by the new one.
			request_registration(ALICE, para_id, 20, MAX_CODE_SIZE as usize + 1);
			assert_eq!(held(ALICE), PARA_DEPOSIT + PER_BYTE * (20 + max_code_size as Balance));
		});
	}

	#[test]
	fn needs_the_host_configuration() {
		build_and_execute(|| {
			let para_id = reserve_for(ALICE);
			HostConfig::set(None);

			assert_noop!(
				Registrar::register(
					RuntimeOrigin::signed(ALICE),
					para_id,
					head(10),
					100,
					hash_of(&code(100)),
				),
				Error::<Test>::NoHostConfiguration
			);
		});
	}

	#[test]
	fn a_transport_failure_rolls_the_whole_call_back() {
		build_and_execute(|| {
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
		build_and_execute(|| {
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
		build_and_execute(|| {
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
		build_and_execute(|| {
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
	fn a_report_for_an_unknown_para_is_reported() {
		build_and_execute(|| {
			assert_ok!(Registrar::receive(RuntimeOrigin::root(), result_message(4242, 7, Ok(()))));

			assert_eq!(
				registrar_events(),
				vec![Event::Unexpected(UnexpectedKind::RegisterResponseForUnknownPara {
					para_id: 4242,
					message_id: 7,
				})]
			);
		});
	}

	#[test]
	fn a_report_for_a_non_pending_para_is_reported() {
		build_and_execute(|| {
			let para_id = reserve_for(ALICE);

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				result_message(para_id, 7, Ok(()))
			));

			assert_eq!(
				registrar_events(),
				vec![Event::Unexpected(UnexpectedKind::RegisterResponseNotPending {
					para_id,
					message_id: 7,
				})]
			);
		});
	}

	#[test]
	fn a_refused_cancellation_records_the_registration_and_keeps_the_deposit() {
		build_and_execute(|| {
			let para_id = reserve_for(ALICE);
			request_registration(ALICE, para_id, 20, 300);
			run_to_block(System::block_number() + PENDING_DEADLINE);
			assert_ok!(Registrar::cancel_registration(RuntimeOrigin::signed(ALICE), para_id));
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
		build_and_execute(|| {
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
	fn a_cancel_response_for_an_unknown_para_is_reported() {
		build_and_execute(|| {
			assert_ok!(Registrar::receive(RuntimeOrigin::root(), cancel_message(4242, 7, Ok(()))));

			assert_eq!(
				registrar_events(),
				vec![Event::Unexpected(UnexpectedKind::CancelResponseForUnknownPara {
					para_id: 4242,
					message_id: 7,
				})]
			);
		});
	}

	#[test]
	fn a_cancel_refusal_we_cannot_settle_is_reported_and_leaves_it_pending() {
		build_and_execute(|| {
			let para_id = reserve_for(ALICE);
			request_registration(ALICE, para_id, 20, 300);
			run_to_block(System::block_number() + PENDING_DEADLINE);
			assert_ok!(Registrar::cancel_registration(RuntimeOrigin::signed(ALICE), para_id));
			let _ = registrar_events();

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				cancel_message(para_id, 1, Err(FailureReason::TooManyPending))
			));

			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Pending { .. }
			));
			assert_eq!(
				registrar_events(),
				vec![Event::Unexpected(UnexpectedKind::CancelRefused {
					para_id,
					message_id: 1,
					reason: FailureReason::TooManyPending,
				})]
			);
		});
	}

	#[test]
	fn a_register_response_for_another_request_leaves_it_pending() {
		build_and_execute(|| {
			let para_id = reserve_for(ALICE);
			request_registration(ALICE, para_id, 20, 300);
			let _ = registrar_events();

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				result_message(para_id, 9, Ok(())),
			));

			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Pending { .. }
			));
			assert_eq!(
				registrar_events(),
				vec![Event::Unexpected(UnexpectedKind::StaleResponse {
					para_id,
					message_id: 9,
					expected: 0,
				})]
			);
		});
	}

	#[test]
	fn a_cancellation_makes_the_registrations_verdict_stale() {
		build_and_execute(|| {
			let para_id = reserve_for(ALICE);
			request_registration(ALICE, para_id, 20, 300);
			run_to_block(System::block_number() + PENDING_DEADLINE);
			assert_ok!(Registrar::cancel_registration(RuntimeOrigin::signed(ALICE), para_id));
			let _ = registrar_events();

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				result_message(para_id, 0, Ok(())),
			));

			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Pending { .. }
			));
			assert_eq!(
				registrar_events(),
				vec![Event::Unexpected(UnexpectedKind::StaleResponse {
					para_id,
					message_id: 0,
					expected: 1,
				})]
			);
		});
	}

	#[test]
	fn a_cancel_response_for_another_request_leaves_it_pending() {
		build_and_execute(|| {
			let para_id = reserve_for(ALICE);
			request_registration(ALICE, para_id, 20, 300);
			run_to_block(System::block_number() + PENDING_DEADLINE);
			assert_ok!(Registrar::cancel_registration(RuntimeOrigin::signed(ALICE), para_id));
			let _ = registrar_events();

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				cancel_message(para_id, 0, Ok(())),
			));

			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Pending { .. }
			));
			assert_eq!(
				registrar_events(),
				vec![Event::Unexpected(UnexpectedKind::StaleResponse {
					para_id,
					message_id: 0,
					expected: 1,
				})]
			);
		});
	}

	#[test]
	fn a_stale_verdict_cannot_settle_a_later_attempt() {
		build_and_execute(|| {
			let deposit = PER_BYTE * (20 + MAX_CODE_SIZE as Balance);
			let para_id = reserve_for(ALICE);
			request_registration(ALICE, para_id, 20, 300);
			run_to_block(System::block_number() + PENDING_DEADLINE);
			assert_ok!(Registrar::cancel_registration(RuntimeOrigin::signed(ALICE), para_id));
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				cancel_message(para_id, 1, Ok(())),
			));
			assert_eq!(Paras::<Test>::get(para_id).unwrap().state, RegistrationState::Reserved);

			request_registration(ALICE, para_id, 20, 300);
			let _ = registrar_events();

			// The first attempt's verdict turns up late and must not drop the second's ticket.
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				result_message(para_id, 0, Err(FailureReason::TooManyPending)),
			));

			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Pending { .. }
			));
			assert_eq!(held(ALICE), PARA_DEPOSIT + deposit);
			assert_eq!(
				registrar_events(),
				vec![Event::Unexpected(UnexpectedKind::StaleResponse {
					para_id,
					message_id: 0,
					expected: 2,
				})]
			);
		});
	}

	#[test]
	fn a_set_head_response_becomes_an_event() {
		new_test_ext().execute_with(|| {
			let para_id = registered_para(ALICE);
			let response = |outcome| {
				MessageToPara::V1(MessageToParaV1::SetHeadResponse {
					para_id,
					message_id: 1,
					outcome,
				})
			};

			assert_ok!(Registrar::receive(RuntimeOrigin::root(), response(Ok(()))));
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				response(Err(FailureReason::HeadDataTooLarge))
			));

			assert_eq!(
				registrar_events(),
				vec![
					Event::HeadUpdated { para_id, message_id: 1 },
					Event::HeadUpdateFailed {
						para_id,
						message_id: 1,
						reason: FailureReason::HeadDataTooLarge,
					},
				]
			);
		});
	}
}

mod set_current_head {
	use super::*;

	#[test]
	fn the_manager_asks_the_relay_chain() {
		new_test_ext().execute_with(|| {
			let para_id = registered_para(ALICE);
			let info = Paras::<Test>::get(para_id).unwrap();
			let deposit = held(ALICE);

			assert_ok!(Registrar::set_current_head(
				RuntimeOrigin::signed(ALICE),
				para_id,
				head(20)
			));

			// Registration took message id 0, so this is message 1. Nothing is recorded here.
			assert_eq!(
				take_sent(),
				vec![MessageToRelay::V1(MessageToRelayV1::SetCurrentHead {
					para_id,
					message_id: 1,
					head: head(20),
				})]
			);
			assert_eq!(
				registrar_events(),
				vec![Event::HeadUpdateRequested { para_id, message_id: 1 }]
			);
			assert_eq!(crate::NextMessageId::<Test>::get(), 2);
			assert_eq!(Paras::<Test>::get(para_id).unwrap(), info);
			assert_eq!(held(ALICE), deposit);
		});
	}

	#[test]
	fn the_para_itself_and_root_may_ask_even_while_locked() {
		new_test_ext().execute_with(|| {
			// The lock only shuts out the manager.
			let para_id = locked_para(ALICE);

			// Another para cannot pose as this one.
			assert_noop!(
				Registrar::set_current_head(para_origin(para_id + 1), para_id, head(20)),
				Error::<Test>::NotOwner
			);

			assert_ok!(Registrar::set_current_head(para_origin(para_id), para_id, head(20)));
			assert_ok!(Registrar::set_current_head(RuntimeOrigin::root(), para_id, head(20)));

			let request = |message_id| {
				MessageToRelay::V1(MessageToRelayV1::SetCurrentHead {
					para_id,
					message_id,
					head: head(20),
				})
			};
			assert_eq!(take_sent(), vec![request(1), request(2)]);
			assert_eq!(
				registrar_events(),
				vec![
					Event::HeadUpdateRequested { para_id, message_id: 1 },
					Event::HeadUpdateRequested { para_id, message_id: 2 },
				]
			);
		});
	}

	#[test]
	fn rejects_an_unknown_id_a_non_manager_an_unregistered_para_a_locked_one_and_a_large_head() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Registrar::set_current_head(RuntimeOrigin::signed(ALICE), 4242, head(20)),
				Error::<Test>::NotReserved
			);

			let reserved = reserve_for(ALICE);
			assert_noop!(
				Registrar::set_current_head(RuntimeOrigin::signed(BOB), reserved, head(20)),
				Error::<Test>::NotOwner
			);
			assert_noop!(
				Registrar::set_current_head(RuntimeOrigin::signed(ALICE), reserved, head(20)),
				Error::<Test>::NotRegistered
			);
			// A registration in flight is not registered either.
			request_registration(ALICE, reserved, 20, 300);
			let _ = take_sent();
			assert_noop!(
				Registrar::set_current_head(RuntimeOrigin::signed(ALICE), reserved, head(20)),
				Error::<Test>::NotRegistered
			);

			let locked = locked_para(ALICE);
			assert_noop!(
				Registrar::set_current_head(RuntimeOrigin::signed(ALICE), locked, head(20)),
				Error::<Test>::ParaLocked
			);

			let para_id = registered_para(ALICE);
			assert_noop!(
				Registrar::set_current_head(
					RuntimeOrigin::signed(ALICE),
					para_id,
					head(MAX_HEAD_SIZE as usize + 1)
				),
				Error::<Test>::HeadDataTooLarge
			);

			assert!(take_sent().is_empty());
		});
	}

	#[test]
	fn a_transport_failure_rolls_the_whole_call_back() {
		new_test_ext().execute_with(|| {
			let para_id = registered_para(ALICE);
			SendFails::set(true);

			assert_noop!(
				Registrar::set_current_head(RuntimeOrigin::signed(ALICE), para_id, head(20)),
				Error::<Test>::SendFailed
			);
			assert_eq!(crate::NextMessageId::<Test>::get(), 1);
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
		build_and_execute(|| {
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
		build_and_execute(|| {
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
			// Giving up means the verdict on message 0 never turned up, which is worth saying out
			// loud.
			assert_eq!(
				registrar_events(),
				vec![
					Event::Unexpected(UnexpectedKind::ResponseNeverArrived {
						para_id,
						message_id: 0
					}),
					Event::CancelRequested { para_id, message_id: 1, manager: ALICE },
				]
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
		build_and_execute(|| {
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
			let _ = registrar_events();
			assert_ok!(Registrar::cancel_registration(RuntimeOrigin::signed(ALICE), para_id));
			// Register was 0, the first cancellation 1, so the retry carries 2.
			assert_eq!(take_sent(), vec![cancel_request(para_id, 2)]);
			// And the retry says that the answer to the first cancellation never came.
			assert_eq!(
				registrar_events(),
				vec![
					Event::Unexpected(UnexpectedKind::ResponseNeverArrived {
						para_id,
						message_id: 1
					}),
					Event::CancelRequested { para_id, message_id: 2, manager: ALICE },
				]
			);
		});
	}

	#[test]
	fn a_transport_failure_rolls_the_whole_call_back() {
		build_and_execute(|| {
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
		build_and_execute(|| {
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
	fn drops_a_reserved_id_on_the_spot_and_asks_nobody() {
		build_and_execute(|| {
			let para_id = reserve_for(ALICE);
			assert_eq!(held(ALICE), PARA_DEPOSIT);

			assert_ok!(Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id));

			assert!(Paras::<Test>::get(para_id).is_none());
			assert_eq!(held(ALICE), 0);
			assert!(take_sent().is_empty());
			assert_eq!(
				registrar_events(),
				vec![Event::ReservationDropped { para_id, manager: ALICE }]
			);
		});
	}

	#[test]
	fn a_dropped_id_is_not_handed_out_again() {
		build_and_execute(|| {
			let para_id = reserve_for(ALICE);
			assert_ok!(Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id));

			assert_eq!(reserve_for(BOB), para_id + 1);
		});
	}

	#[test]
	fn asks_the_relay_chain_and_holds_on_to_both_deposits() {
		build_and_execute(|| {
			let para_id = registered_para(ALICE);
			let deposit = PER_BYTE * (20 + MAX_CODE_SIZE as Balance);

			assert_ok!(Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id));

			let expected_at = System::block_number() + PENDING_DEADLINE;
			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Deregistering { can_retry_after, message_id: 1, .. }
					if can_retry_after == expected_at
			));
			assert_eq!(held(ALICE), PARA_DEPOSIT + deposit);
			// The registration took message id 0, so this is message 1.
			assert_eq!(take_sent(), vec![deregister_request(para_id, 1)]);
			assert_eq!(
				registrar_events(),
				vec![Event::DeregisterRequested { para_id, message_id: 1, manager: ALICE }]
			);
		});
	}

	#[test]
	fn a_confirmed_deregistration_releases_every_deposit_and_forgets_the_id() {
		build_and_execute(|| {
			let para_id = deregistering_para(ALICE);

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				deregister_answer(para_id, 1, Ok(()))
			));

			assert!(Paras::<Test>::get(para_id).is_none());
			assert_eq!(held(ALICE), 0);
			assert_eq!(
				registrar_events(),
				vec![Event::Deregistered { para_id, message_id: 1, manager: ALICE }]
			);
		});
	}

	#[test]
	fn a_refused_deregistration_leaves_the_para_registered_and_paid_for() {
		build_and_execute(|| {
			let para_id = deregistering_para(ALICE);
			let deposit = PER_BYTE * (20 + MAX_CODE_SIZE as Balance);

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				deregister_answer(para_id, 1, Err(FailureReason::NotDeregisterable)),
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
					reason: FailureReason::NotDeregisterable,
				}]
			);

			// And the manager can ask again.
			assert_ok!(Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id));
		});
	}

	#[test]
	fn a_retry_is_refused_before_the_deadline() {
		build_and_execute(|| {
			let para_id = deregistering_para(ALICE);

			run_to_block(System::block_number() + PENDING_DEADLINE - 1);
			assert_noop!(
				Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::CannotRetryYet
			);
		});
	}

	#[test]
	fn a_retry_reports_the_lost_answer_and_asks_again() {
		build_and_execute(|| {
			let para_id = deregistering_para(ALICE);
			let deposit = PER_BYTE * (20 + MAX_CODE_SIZE as Balance);

			run_to_block(System::block_number() + PENDING_DEADLINE);
			assert_ok!(Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id));

			let expected_at = System::block_number() + PENDING_DEADLINE;
			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Deregistering { can_retry_after, message_id: 2, .. }
					if can_retry_after == expected_at
			));
			assert_eq!(held(ALICE), PARA_DEPOSIT + deposit);
			assert_eq!(take_sent(), vec![deregister_request(para_id, 2)]);
			assert_eq!(
				registrar_events(),
				vec![
					Event::Unexpected(UnexpectedKind::ResponseNeverArrived {
						para_id,
						message_id: 1
					}),
					Event::DeregisterRequested { para_id, message_id: 2, manager: ALICE },
				]
			);

			// Only the answer to the retry settles it.
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				deregister_answer(para_id, 2, Ok(()))
			));
			assert!(Paras::<Test>::get(para_id).is_none());
			assert_eq!(held(ALICE), 0);
		});
	}

	#[test]
	fn the_answer_to_a_superseded_request_is_stale() {
		build_and_execute(|| {
			let para_id = deregistering_para(ALICE);
			run_to_block(System::block_number() + PENDING_DEADLINE);
			assert_ok!(Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id));
			let _ = registrar_events();

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				deregister_answer(para_id, 1, Ok(()))
			));

			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Deregistering { message_id: 2, .. }
			));
			assert_eq!(
				registrar_events(),
				vec![Event::Unexpected(UnexpectedKind::StaleResponse {
					para_id,
					message_id: 1,
					expected: 2,
				})]
			);
		});
	}

	#[test]
	fn an_answer_for_an_unknown_para_is_reported() {
		build_and_execute(|| {
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				deregister_answer(4242, 0, Ok(()))
			));

			assert_eq!(
				registrar_events(),
				vec![Event::Unexpected(UnexpectedKind::DeregisterResponseForUnknownPara {
					para_id: 4242,
					message_id: 0,
				})]
			);
		});
	}

	#[test]
	fn an_answer_for_a_para_not_deregistering_is_reported() {
		build_and_execute(|| {
			let para_id = registered_para(ALICE);

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				deregister_answer(para_id, 0, Ok(()))
			));

			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Registered { .. }
			));
			assert_eq!(
				registrar_events(),
				vec![Event::Unexpected(UnexpectedKind::DeregisterResponseNotDeregistering {
					para_id,
					message_id: 0,
				})]
			);
		});
	}

	#[test]
	fn the_para_itself_may_ask_but_a_stranger_may_not() {
		build_and_execute(|| {
			let para_id = registered_para(ALICE);

			assert_noop!(
				Registrar::deregister(RuntimeOrigin::signed(BOB), para_id),
				Error::<Test>::NotOwner
			);
			assert_noop!(
				Registrar::deregister(para_origin(para_id + 1), para_id),
				Error::<Test>::NotOwner
			);
			assert_ok!(Registrar::deregister(para_origin(para_id), para_id));
		});
	}

	#[test]
	fn a_locked_para_is_out_of_the_managers_hands_but_not_roots() {
		build_and_execute(|| {
			let para_id = locked_para(ALICE);

			assert_noop!(
				Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::ParaLocked
			);
			assert_ok!(Registrar::deregister(RuntimeOrigin::root(), para_id));
		});
	}

	#[test]
	fn a_registration_in_flight_has_to_be_settled_first() {
		build_and_execute(|| {
			let para_id = reserve_for(ALICE);
			request_registration(ALICE, para_id, 20, 300);

			assert_noop!(
				Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::NotRegistered
			);
		});
	}

	#[test]
	fn an_id_nobody_reserved_is_refused() {
		build_and_execute(|| {
			assert_noop!(
				Registrar::deregister(RuntimeOrigin::signed(ALICE), 4242),
				Error::<Test>::NotReserved
			);
		});
	}

	#[test]
	fn a_transport_failure_rolls_the_whole_call_back() {
		build_and_execute(|| {
			let para_id = registered_para(ALICE);
			SendFails::set(true);

			assert_noop!(
				Registrar::deregister(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::SendFailed
			);

			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Registered { .. }
			));
			assert_eq!(crate::NextMessageId::<Test>::get(), 1);
		});
	}
}

mod add_lock {
	use super::*;

	#[test]
	fn the_manager_locks_a_registered_para() {
		build_and_execute(|| {
			let para_id = registered_para(ALICE);

			assert_ok!(Registrar::add_lock(RuntimeOrigin::signed(ALICE), para_id));

			assert!(Paras::<Test>::get(para_id).unwrap().is_locked());
			assert!(take_sent().is_empty());
			assert_eq!(registrar_events(), vec![Event::ParaLocked { para_id }]);
		});
	}

	#[test]
	fn the_para_itself_and_root_may_lock() {
		build_and_execute(|| {
			// Another para cannot pose as this one.
			let para_id = registered_para(ALICE);
			assert_noop!(
				Registrar::add_lock(para_origin(para_id + 1), para_id),
				Error::<Test>::NotOwner
			);

			assert_ok!(Registrar::add_lock(para_origin(para_id), para_id));
			assert!(Paras::<Test>::get(para_id).unwrap().is_locked());
			assert_eq!(registrar_events(), vec![Event::ParaLocked { para_id }]);

			let other_id = registered_para(BOB);
			assert_ok!(Registrar::add_lock(RuntimeOrigin::root(), other_id));
			assert!(Paras::<Test>::get(other_id).unwrap().is_locked());
			assert_eq!(registrar_events(), vec![Event::ParaLocked { para_id: other_id }]);
		});
	}

	#[test]
	fn rejects_an_unknown_id_a_non_manager_and_a_para_that_is_not_registered() {
		build_and_execute(|| {
			assert_noop!(
				Registrar::add_lock(RuntimeOrigin::signed(ALICE), 4242),
				Error::<Test>::NotReserved
			);

			// Reserved, not registered.
			let para_id = reserve_for(ALICE);
			assert_noop!(
				Registrar::add_lock(RuntimeOrigin::signed(BOB), para_id),
				Error::<Test>::NotOwner
			);
			assert_noop!(
				Registrar::add_lock(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::NotRegistered
			);

			// And a registration in flight is not registered either.
			request_registration(ALICE, para_id, 20, 300);
			assert_noop!(
				Registrar::add_lock(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::NotRegistered
			);
		});
	}

	#[test]
	fn a_locked_para_cannot_be_locked_again() {
		build_and_execute(|| {
			let para_id = locked_para(ALICE);

			// The manager is turned away by the lock itself, before the state is looked at.
			assert_noop!(
				Registrar::add_lock(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::ParaLocked
			);

			// Root and the para are not, so they get the state error.
			assert_noop!(
				Registrar::add_lock(RuntimeOrigin::root(), para_id),
				Error::<Test>::AlreadyLocked
			);
			assert_noop!(
				Registrar::add_lock(para_origin(para_id), para_id),
				Error::<Test>::AlreadyLocked
			);
		});
	}
}

mod head_noted {
	use super::*;

	/// Tell the pallet, as the relay chain, that `para_id` produced a head.
	fn note_head(para_id: u32) {
		assert_ok!(Registrar::receive(
			RuntimeOrigin::root(),
			MessageToPara::V1(MessageToParaV1::HeadNoted { para_id }),
		));
	}

	#[test]
	fn locks_a_registered_para() {
		build_and_execute(|| {
			let para_id = registered_para(ALICE);

			note_head(para_id);

			assert!(Paras::<Test>::get(para_id).unwrap().is_locked());
			assert_eq!(registrar_events(), vec![Event::ParaLocked { para_id }]);
		});
	}

	#[test]
	fn only_the_relay_chain_may_note_a_head() {
		build_and_execute(|| {
			let para_id = registered_para(ALICE);

			assert_noop!(
				Registrar::receive(
					RuntimeOrigin::signed(ALICE),
					MessageToPara::V1(MessageToParaV1::HeadNoted { para_id }),
				),
				sp_runtime::DispatchError::BadOrigin,
			);

			assert!(!Paras::<Test>::get(para_id).unwrap().is_locked());
		});
	}

	#[test]
	fn reports_an_unknown_para() {
		build_and_execute(|| {
			note_head(4242);

			assert_eq!(
				registrar_events(),
				vec![Event::Unexpected(UnexpectedKind::HeadNotedForUnknownPara { para_id: 4242 })]
			);
		});
	}

	#[test]
	fn reports_a_para_whose_registration_is_still_pending() {
		build_and_execute(|| {
			let para_id = reserve_for(ALICE);
			request_registration(ALICE, para_id, 20, 300);
			let _ = registrar_events();

			note_head(para_id);

			let info = Paras::<Test>::get(para_id).unwrap();
			assert!(info.is_locked());
			assert!(matches!(info.state, RegistrationState::Pending { .. }));
			assert_eq!(
				registrar_events(),
				vec![
					Event::Unexpected(UnexpectedKind::HeadNotedForUnregisteredPara { para_id }),
					Event::ParaLocked { para_id }
				]
			);
		});
	}

	#[test]
	fn a_second_head_changes_nothing() {
		build_and_execute(|| {
			let para_id = locked_para(ALICE);
			let _ = registrar_events();

			note_head(para_id);

			assert!(Paras::<Test>::get(para_id).unwrap().is_locked());
			assert!(registrar_events().is_empty());
		});
	}

	#[test]
	fn a_lifted_lock_is_not_reapplied() {
		build_and_execute(|| {
			let para_id = locked_para(ALICE);
			assert_ok!(Registrar::remove_lock(RuntimeOrigin::root(), para_id));
			let _ = registrar_events();

			note_head(para_id);

			assert!(!Paras::<Test>::get(para_id).unwrap().is_locked());
			assert!(registrar_events().is_empty());
		});
	}
}

mod remove_lock {
	use super::*;

	#[test]
	fn root_unlocks() {
		build_and_execute(|| {
			let para_id = locked_para(ALICE);

			assert_ok!(Registrar::remove_lock(RuntimeOrigin::root(), para_id));

			assert!(!Paras::<Test>::get(para_id).unwrap().is_locked());
			assert!(take_sent().is_empty());
			assert_eq!(registrar_events(), vec![Event::ParaUnlocked { para_id }]);
		});
	}

	#[test]
	fn the_para_itself_unlocks() {
		build_and_execute(|| {
			let para_id = locked_para(ALICE);

			// Another para cannot pose as this one.
			assert_noop!(
				Registrar::remove_lock(para_origin(para_id + 1), para_id),
				Error::<Test>::NotOwner
			);

			assert_ok!(Registrar::remove_lock(para_origin(para_id), para_id));
			assert!(!Paras::<Test>::get(para_id).unwrap().is_locked());
			assert_eq!(registrar_events(), vec![Event::ParaUnlocked { para_id }]);
		});
	}

	#[test]
	fn the_manager_may_not_unlock() {
		build_and_execute(|| {
			let para_id = locked_para(ALICE);

			assert_noop!(
				Registrar::remove_lock(RuntimeOrigin::signed(ALICE), para_id),
				DispatchError::BadOrigin
			);
			assert!(Paras::<Test>::get(para_id).unwrap().is_locked());
		});
	}

	#[test]
	fn rejects_an_unknown_id_and_a_para_that_is_not_locked() {
		build_and_execute(|| {
			assert_noop!(
				Registrar::remove_lock(RuntimeOrigin::root(), 4242),
				Error::<Test>::NotReserved
			);

			let para_id = registered_para(ALICE);
			assert_noop!(
				Registrar::remove_lock(RuntimeOrigin::root(), para_id),
				Error::<Test>::NotLocked
			);
		});
	}

	#[test]
	fn a_lock_shuts_the_manager_out_until_it_is_lifted() {
		build_and_execute(|| {
			let para_id = locked_para(ALICE);

			assert_noop!(
				Registrar::add_lock(RuntimeOrigin::signed(ALICE), para_id),
				Error::<Test>::ParaLocked
			);

			assert_ok!(Registrar::remove_lock(RuntimeOrigin::root(), para_id));
			let _ = registrar_events();

			assert_ok!(Registrar::add_lock(RuntimeOrigin::signed(ALICE), para_id));
			assert_eq!(registrar_events(), vec![Event::ParaLocked { para_id }]);
		});
	}
}

mod force_register {
	use super::*;

	fn force(para_id: u32, manager: AccountId, head_len: usize, code_len: usize) -> Vec<u8> {
		let blob = code(code_len);
		assert_ok!(Registrar::force_register(
			RuntimeOrigin::root(),
			para_id,
			manager,
			head(head_len),
			code_len as u32,
			hash_of(&blob),
			false,
		));
		blob
	}

	#[test]
	fn only_root_may_force_register() {
		build_and_execute(|| {
			for origin in
				[RuntimeOrigin::signed(ALICE), para_origin(SYSTEM_PARA_ID), RuntimeOrigin::none()]
			{
				assert_noop!(
					Registrar::force_register(
						origin,
						SYSTEM_PARA_ID,
						ALICE,
						head(10),
						100,
						hash_of(&code(100)),
						false,
					),
					DispatchError::BadOrigin
				);
			}
			assert!(Paras::<Test>::get(SYSTEM_PARA_ID).is_none());
		});
	}

	#[test]
	fn onboards_an_unreserved_id_and_charges_the_manager_for_both() {
		build_and_execute(|| {
			let head_len = 20;
			let code_len = 300;
			let blob = force(FIRST_PARA_ID, ALICE, head_len, code_len);

			// The same bill as `reserve` followed by `register`.
			let expected = PER_BYTE * (head_len as Balance + MAX_CODE_SIZE as Balance);
			assert_eq!(held(ALICE), PARA_DEPOSIT + expected);

			let info = Paras::<Test>::get(FIRST_PARA_ID).unwrap();
			assert_eq!(info.manager, ALICE);
			let expected_at = System::block_number() + PENDING_DEADLINE;
			assert!(matches!(
				info.state,
				RegistrationState::Pending { cancellable_at, .. } if cancellable_at == expected_at
			));

			assert_eq!(
				registrar_events(),
				vec![Event::RegisterRequested {
					para_id: FIRST_PARA_ID,
					message_id: 0,
					manager: ALICE
				}]
			);
			assert_eq!(
				take_sent(),
				vec![MessageToRelay::V1(MessageToRelayV1::Register {
					para_id: FIRST_PARA_ID,
					message_id: 0,
					manager: ALICE,
					genesis_head: head(head_len),
					code_hash: hash_of(&blob),
					code_len: code_len as u32,
				})]
			);
		});
	}

	#[test]
	fn an_already_reserved_id_is_not_charged_twice() {
		build_and_execute(|| {
			let para_id = reserve_for(ALICE);
			assert_eq!(held(ALICE), PARA_DEPOSIT);

			force(para_id, ALICE, 20, 300);

			// Only the registration is added; the reservation ticket is the one already held.
			assert_eq!(held(ALICE), PARA_DEPOSIT + PER_BYTE * (20 + MAX_CODE_SIZE as Balance));
		});
	}

	#[test]
	fn skip_deposit_onboards_and_drops_a_para_without_holding_anything() {
		build_and_execute(|| {
			let broke = 3;

			assert_ok!(Registrar::force_register(
				RuntimeOrigin::root(),
				FIRST_PARA_ID,
				broke,
				head(10),
				100,
				hash_of(&code(100)),
				true,
			));
			let info = Paras::<Test>::get(FIRST_PARA_ID).unwrap();
			assert!(info.reservation.is_none());
			assert!(matches!(info.state, RegistrationState::Pending { ticket: None, .. }));

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				MessageToPara::V1(MessageToParaV1::RegisterResponse {
					para_id: FIRST_PARA_ID,
					message_id: 0,
					outcome: Ok(()),
				}),
			));
			assert_ok!(Registrar::deregister(RuntimeOrigin::root(), FIRST_PARA_ID));
			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				deregister_answer(FIRST_PARA_ID, 1, Ok(())),
			));

			assert!(Paras::<Test>::get(FIRST_PARA_ID).is_none());
			assert_eq!(held(broke), 0);
			assert!(!registrar_events().iter().any(|e| matches!(
				e,
				Event::Unexpected(UnexpectedKind::DepositNotReleased { .. })
			)));
		});
	}

	#[test]
	fn skip_deposit_keeps_a_reservation_already_held() {
		build_and_execute(|| {
			let para_id = reserve_for(ALICE);

			assert_ok!(Registrar::force_register(
				RuntimeOrigin::root(),
				para_id,
				ALICE,
				head(20),
				300,
				hash_of(&code(300)),
				true,
			));

			assert_eq!(held(ALICE), PARA_DEPOSIT);
			assert!(matches!(
				Paras::<Test>::get(para_id).unwrap().state,
				RegistrationState::Pending { ticket: None, .. }
			));
		});
	}

	#[test]
	fn a_system_id_never_takes_a_deposit() {
		build_and_execute(|| {
			force(SYSTEM_PARA_ID, ALICE, 20, 300);

			assert_eq!(held(ALICE), 0);
			let info = Paras::<Test>::get(SYSTEM_PARA_ID).unwrap();
			assert!(info.reservation.is_none());
			assert!(matches!(info.state, RegistrationState::Pending { ticket: None, .. }));
		});
	}

	#[test]
	fn a_turned_down_system_id_is_dropped() {
		build_and_execute(|| {
			force(SYSTEM_PARA_ID, ALICE, 10, 100);
			let _ = registrar_events();

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				MessageToPara::V1(MessageToParaV1::RegisterResponse {
					para_id: SYSTEM_PARA_ID,
					message_id: 0,
					outcome: Err(FailureReason::AlreadyRegistered),
				}),
			));

			assert!(Paras::<Test>::get(SYSTEM_PARA_ID).is_none());
			assert_eq!(
				registrar_events(),
				vec![Event::RegistrationFailed {
					para_id: SYSTEM_PARA_ID,
					message_id: 0,
					manager: ALICE,
					reason: FailureReason::AlreadyRegistered,
				}]
			);
			assert_noop!(
				Registrar::register(
					RuntimeOrigin::signed(ALICE),
					SYSTEM_PARA_ID,
					head(10),
					100,
					hash_of(&code(100)),
				),
				Error::<Test>::SystemParaId
			);
		});
	}

	#[test]
	fn a_cancelled_system_id_is_dropped() {
		build_and_execute(|| {
			force(SYSTEM_PARA_ID, ALICE, 10, 100);
			run_to_block(System::block_number() + PENDING_DEADLINE);
			assert_ok!(Registrar::cancel_registration(
				RuntimeOrigin::signed(ALICE),
				SYSTEM_PARA_ID
			));
			let _ = registrar_events();

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				MessageToPara::V1(MessageToParaV1::CancelResponse {
					para_id: SYSTEM_PARA_ID,
					message_id: 1,
					outcome: Ok(()),
				}),
			));

			assert!(Paras::<Test>::get(SYSTEM_PARA_ID).is_none());
			assert_eq!(
				registrar_events(),
				vec![Event::RegistrationCancelled {
					para_id: SYSTEM_PARA_ID,
					message_id: 1,
					manager: ALICE
				}]
			);
		});
	}

	#[test]
	fn rejects_an_id_reserved_by_somebody_else() {
		build_and_execute(|| {
			let para_id = reserve_for(ALICE);

			assert_noop!(
				Registrar::force_register(
					RuntimeOrigin::root(),
					para_id,
					BOB,
					head(10),
					100,
					hash_of(&code(100)),
					false,
				),
				Error::<Test>::NotOwner
			);
			assert_eq!(held(ALICE), PARA_DEPOSIT);
		});
	}

	#[test]
	fn rejects_a_para_that_is_pending_or_registered() {
		build_and_execute(|| {
			let pending = reserve_for(ALICE);
			request_registration(ALICE, pending, 10, 100);
			let registered = registered_para(ALICE);

			for para_id in [pending, registered] {
				assert_noop!(
					Registrar::force_register(
						RuntimeOrigin::root(),
						para_id,
						ALICE,
						head(10),
						100,
						hash_of(&code(100)),
						false,
					),
					Error::<Test>::AlreadyRegistered
				);
			}
		});
	}

	#[test]
	fn a_system_para_id_stays_out_of_reserves_way() {
		build_and_execute(|| {
			force(SYSTEM_PARA_ID, ALICE, 10, 100);

			assert_eq!(reserve_for(BOB), FIRST_PARA_ID);
		});
	}

	#[test]
	fn keeps_reserve_from_colliding_with_a_forced_public_id() {
		build_and_execute(|| {
			force(FIRST_PARA_ID, ALICE, 10, 100);

			assert_eq!(reserve_for(BOB), FIRST_PARA_ID + 1);
		});
	}

	#[test]
	fn enforces_the_relay_chains_size_bounds() {
		build_and_execute(|| {
			let cases = [
				(head(MAX_HEAD_SIZE as usize + 1), MIN_CODE_SIZE, Error::<Test>::HeadDataTooLarge),
				(head(10), MIN_CODE_SIZE - 1, Error::<Test>::CodeTooSmall),
				(head(10), MAX_CODE_SIZE + 1, Error::<Test>::CodeTooLarge),
			];

			for (genesis_head, code_len, expected) in cases {
				assert_noop!(
					Registrar::force_register(
						RuntimeOrigin::root(),
						SYSTEM_PARA_ID,
						ALICE,
						genesis_head,
						code_len,
						hash_of(&code(100)),
						false,
					),
					expected
				);
			}
			assert!(Paras::<Test>::get(SYSTEM_PARA_ID).is_none());
		});
	}

	#[test]
	fn fails_when_the_manager_cannot_pay() {
		build_and_execute(|| {
			let broke = 3;

			assert_noop!(
				Registrar::force_register(
					RuntimeOrigin::root(),
					FIRST_PARA_ID,
					broke,
					head(10),
					100,
					hash_of(&code(100)),
					false,
				),
				TokenError::FundsUnavailable
			);
		});
	}

	#[test]
	fn a_transport_failure_rolls_the_whole_call_back() {
		build_and_execute(|| {
			SendFails::set(true);

			assert_noop!(
				Registrar::force_register(
					RuntimeOrigin::root(),
					FIRST_PARA_ID,
					ALICE,
					head(10),
					100,
					hash_of(&code(100)),
					false,
				),
				Error::<Test>::SendFailed
			);

			assert_eq!(held(ALICE), 0);
			assert!(Paras::<Test>::get(FIRST_PARA_ID).is_none());
			assert_eq!(crate::NextFreeParaId::<Test>::get(), 0);
			assert_eq!(crate::NextMessageId::<Test>::get(), 0);
		});
	}

	#[test]
	fn the_relay_chain_confirms_the_registration() {
		build_and_execute(|| {
			force(FIRST_PARA_ID, ALICE, 20, 300);
			let _ = registrar_events();

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				MessageToPara::V1(MessageToParaV1::RegisterResponse {
					para_id: FIRST_PARA_ID,
					message_id: 0,
					outcome: Ok(()),
				}),
			));

			assert!(matches!(
				Paras::<Test>::get(FIRST_PARA_ID).unwrap().state,
				RegistrationState::Registered { .. }
			));
			assert_eq!(held(ALICE), PARA_DEPOSIT + PER_BYTE * (20 + MAX_CODE_SIZE as Balance));
			assert_eq!(
				registrar_events(),
				vec![Event::Registered { para_id: FIRST_PARA_ID, message_id: 0, manager: ALICE }]
			);
		});
	}

	#[test]
	fn a_rejection_releases_the_registration_and_leaves_the_id_reserved() {
		build_and_execute(|| {
			force(FIRST_PARA_ID, ALICE, 20, 300);
			let _ = registrar_events();

			assert_ok!(Registrar::receive(
				RuntimeOrigin::root(),
				MessageToPara::V1(MessageToParaV1::RegisterResponse {
					para_id: FIRST_PARA_ID,
					message_id: 0,
					outcome: Err(FailureReason::AlreadyRegistered),
				}),
			));

			let info = Paras::<Test>::get(FIRST_PARA_ID).unwrap();
			assert_eq!(info.state, RegistrationState::Reserved);
			assert_eq!(info.manager, ALICE);
			// The reservation stays, so the id can be retried.
			assert_eq!(held(ALICE), PARA_DEPOSIT);
			assert_eq!(
				registrar_events(),
				vec![Event::RegistrationFailed {
					para_id: FIRST_PARA_ID,
					message_id: 0,
					manager: ALICE,
					reason: FailureReason::AlreadyRegistered,
				}]
			);
		});
	}
}

mod try_state {
	use super::*;
	use crate::{Config, ParaInfo};
	use frame_support::traits::Consideration;

	fn insert_system_para(
		state: RegistrationState<<Test as Config>::RegistrationConsideration, BlockNumber>,
	) {
		Paras::<Test>::insert(
			SYSTEM_PARA_ID,
			ParaInfo { manager: ALICE, reservation: None, state, locked: None },
		);
	}

	#[test]
	fn flags_a_reserved_system_id() {
		new_test_ext().execute_with(|| {
			insert_system_para(RegistrationState::Reserved);

			assert_eq!(Registrar::do_try_state(), Err("a system para id is left reserved".into()));
		});
	}

	#[test]
	fn flags_a_system_para_holding_a_deposit() {
		new_test_ext().execute_with(|| {
			let ticket = <Test as Config>::RegistrationConsideration::new(
				&ALICE,
				Registrar::registration_footprint(10, &host_config(MAX_CODE_SIZE, MAX_HEAD_SIZE)),
			)
			.unwrap();
			insert_system_para(RegistrationState::Registered { ticket: Some(ticket) });

			assert_eq!(Registrar::do_try_state(), Err("a system para holds a deposit".into()));
		});
	}
}
