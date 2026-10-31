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

//! Tests for `pallet-jamkb`, run against the mocked parachain-system and JAM of [`crate::mock`].

use crate::{
	messages::{AccumulateLog, TransferError, UpwardMessage},
	mock::*,
	transport::{MessageSendError, OnJamOutcome},
	AllocationMode, AllocationState, Error, Event, FailureReason, OperationState,
};
use frame_support::{
	assert_noop, assert_ok,
	traits::{
		fungibles::{Inspect, MutateHold},
		tokens::Precision,
	},
	weights::WeightMeter,
};
use sp_core::H256;
use sp_runtime::{ArithmeticError, DispatchError, DispatchResult, TokenError};

const ANCHOR: H256 = H256::repeat_byte(0xAA);

/// Runs `f` on a fresh, initialized and attested pallet and checks the invariants afterwards.
fn ready(f: impl FnOnce()) {
	new_test_ext().execute_with(|| {
		assert_ok!(Jamkb::initialize(RuntimeOrigin::root()));
		assert_ok!(Jamkb::attest(RuntimeOrigin::root(), ANCHOR, 0));
		f();
		assert_ok!(Jamkb::do_try_state());
	});
}

/// Approves a release of `amount` to `target` and executes it. Returns the allocation and
/// operation ids.
fn release_to(target: ServiceId, amount: Balance) -> (u64, u64) {
	assert_ok!(Jamkb::approve_release(RuntimeOrigin::root(), target, amount, MEMO, None, 100));
	let id = crate::LastAllocationId::<Test>::get();
	assert_ok!(Jamkb::execute_release(RuntimeOrigin::signed(BOB), id));
	let op = sent().last().expect("the release is queued").id;
	(id, op)
}

/// The id the mock transport gives the `n`th operation sent, counting from 1.
fn op(n: u64) -> u64 {
	OPERATION_ID_BASE + n
}

fn failed(error: TransferError) -> OperationState<Balance> {
	OperationState::Failed(FailureReason::Transfer(error))
}

// ── Setup and control ──────────────────────────────────────────────────────

#[test]
fn initialize_creates_the_asset_and_mints_the_cap_into_custody() {
	build_and_execute(|| {
		assert_noop!(
			Jamkb::initialize(RuntimeOrigin::signed(ALICE)),
			sp_runtime::DispatchError::BadOrigin
		);
		assert!(!Assets::asset_exists(JAMKB));

		assert_ok!(Jamkb::initialize(RuntimeOrigin::root()));

		assert!(Assets::asset_exists(JAMKB));
		assert_eq!(Assets::total_issuance(JAMKB), CAP);
		assert_eq!(Assets::minimum_balance(JAMKB), 1);
		assert_eq!(free(custody()), CAP);
		// The asset is not sufficient: the pallet accounts exist through their own provider.
		for account in Jamkb::pallet_accounts() {
			assert_eq!(System::providers(&account), 1);
		}
		assert_eq!(Mapped::get(), vec![custody()]);
		assert_eq!(last_event(), RuntimeEvent::Jamkb(Event::Initialized));

		assert_noop!(Jamkb::initialize(RuntimeOrigin::root()), Error::<Test>::AlreadyInitialized);
	});
}

#[test]
fn every_call_except_initialize_and_attest_waits_for_the_attestation() {
	build_and_execute(|| {
		assert_noop!(
			Jamkb::attest(RuntimeOrigin::root(), ANCHOR, 0),
			Error::<Test>::NotInitialized
		);
		assert_ok!(Jamkb::initialize(RuntimeOrigin::root()));

		assert_noop!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 10), Error::<Test>::NotAttested);
		assert_noop!(
			Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 10, MEMO, None, 10),
			Error::<Test>::NotAttested
		);
		assert_noop!(
			Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO),
			Error::<Test>::NotAttested
		);
		assert_noop!(Jamkb::pause(RuntimeOrigin::root()), Error::<Test>::NotAttested);
		assert_noop!(Jamkb::resume(RuntimeOrigin::root()), Error::<Test>::NotAttested);
		assert_noop!(
			Jamkb::cancel_allocation(RuntimeOrigin::root(), 1),
			Error::<Test>::NotAttested
		);
		assert_noop!(Jamkb::settle(RuntimeOrigin::signed(ALICE), 1), Error::<Test>::NotAttested);

		assert_ok!(Jamkb::attest(RuntimeOrigin::root(), ANCHOR, 0));
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::Attested { anchor: ANCHOR, released: 0 })
		);
		assert_noop!(
			Jamkb::attest(RuntimeOrigin::root(), ANCHOR, 0),
			Error::<Test>::AlreadyAttested
		);

		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 10));
	});
}

#[test]
fn attest_records_the_units_held_outside_the_parachain_service_as_released() {
	build_and_execute(|| {
		// At genesis another service holds 500 of the supply.
		Jam::set_balance(PARACHAIN_SERVICE, CAP - 500);
		Jam::set_balance(SERVICE, 500);
		assert_ok!(Jamkb::initialize(RuntimeOrigin::root()));

		assert_ok!(Jamkb::attest(RuntimeOrigin::root(), ANCHOR, 500));

		assert_eq!(free(custody()), CAP - 500);
		assert_eq!(free(released()), 500);
		assert_eq!(Assets::total_issuance(JAMKB), CAP);
		assert_books_match_jam();
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::Attested { anchor: ANCHOR, released: 500 })
		);
	});
}

#[test]
fn pause_blocks_new_work_but_not_settling() {
	ready(|| {
		let (id, op) = release_to(SERVICE, 100);
		assert_ok!(Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 50, MEMO, None, 100));

		assert_ok!(Jamkb::pause(RuntimeOrigin::root()));
		assert_noop!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 10), Error::<Test>::Paused);
		assert_noop!(
			Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 10, MEMO, None, 10),
			Error::<Test>::Paused
		);
		assert_noop!(Jamkb::execute_release(RuntimeOrigin::signed(BOB), 2), Error::<Test>::Paused);
		assert_noop!(
			Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO),
			Error::<Test>::Paused
		);

		// Already sent work still completes.
		accumulate_all(1);
		poll();
		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Delivered);
		assert!(Jamkb::operation(op).is_none());

		assert_ok!(Jamkb::resume(RuntimeOrigin::root()));
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 10));
	});
}

#[test]
fn grant_moves_units_from_custody() {
	ready(|| {
		assert_noop!(
			Jamkb::grant(RuntimeOrigin::signed(ALICE), ALICE, 10),
			sp_runtime::DispatchError::BadOrigin
		);
		assert_noop!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 0), Error::<Test>::ZeroAmount);

		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 250));

		assert_eq!(free(ALICE), 250);
		assert_eq!(free(custody()), CAP - 250);
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::BudgetGranted { beneficiary: ALICE, amount: 250 })
		);
		// Nothing left the Parachain Service on JAM.
		assert_books_match_jam();
	});
}

// ── Permanent release ──────────────────────────────────────────────────────

#[test]
fn approve_release_holds_the_amount_on_custody() {
	ready(|| {
		assert_noop!(
			Jamkb::approve_release(RuntimeOrigin::root(), PARACHAIN_SERVICE, 10, MEMO, None, 10),
			Error::<Test>::TargetIsParachainService
		);
		assert_noop!(
			Jamkb::approve_release(
				RuntimeOrigin::root(),
				SERVICE,
				MIN_TRANSFER - 1,
				MEMO,
				None,
				10
			),
			Error::<Test>::BelowMinTransfer
		);

		assert_noop!(
			Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 100, MEMO, None, 0),
			Error::<Test>::EmptyValidity
		);

		assert_ok!(Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 100, MEMO, Some(5), 10));

		let allocation = Jamkb::allocation(1).unwrap();
		assert_eq!(allocation.approver, None);
		assert_eq!(allocation.state, AllocationState::Approved);
		assert_eq!(allocation.mode, AllocationMode::Permanent { memo: MEMO });
		assert_eq!(allocation.target, SERVICE);
		assert_eq!(allocation.amount, 100);
		assert_eq!(allocation.valid_from, 5);
		assert_eq!(allocation.expires_at, 15);
		assert_eq!(held(custody()), 100);
		assert_eq!(free(custody()), CAP - 100);
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::ReleaseApproved { id: 1, target: SERVICE, amount: 100 })
		);
	});
}

#[test]
fn valid_from_none_means_the_current_block() {
	ready(|| {
		run_to_block(7);
		assert_ok!(Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 100, MEMO, None, 3));
		let allocation = Jamkb::allocation(1).unwrap();
		assert_eq!(allocation.valid_from, 7);
		assert_eq!(allocation.expires_at, 10);
	});
}

#[test]
fn execute_release_respects_the_validity_window() {
	ready(|| {
		assert_ok!(Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 100, MEMO, Some(5), 10));

		assert_noop!(
			Jamkb::execute_release(RuntimeOrigin::signed(BOB), 9),
			Error::<Test>::UnknownAllocation
		);
		assert_noop!(
			Jamkb::execute_release(RuntimeOrigin::signed(BOB), 1),
			Error::<Test>::NotYetValid
		);

		run_to_block(15);
		assert_noop!(Jamkb::execute_release(RuntimeOrigin::signed(BOB), 1), Error::<Test>::Expired);

		run_to_block(14);
		assert_ok!(Jamkb::execute_release(RuntimeOrigin::signed(BOB), 1));
		assert_eq!(Jamkb::allocation(1).unwrap().state, AllocationState::Delivering);
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::OperationSubmitted { id: op(1), allocation: Some(1) })
		);
		assert_noop!(
			Jamkb::execute_release(RuntimeOrigin::signed(BOB), 1),
			Error::<Test>::WrongState
		);
	});
}

#[test]
fn execute_release_queues_one_deferred_transfer_out_with_the_memo_and_gas() {
	ready(|| {
		let (_, op) = release_to(SERVICE, 100);

		assert_eq!(
			sent(),
			vec![Queued {
				sender: JAMKB_SENDER,
				id: op,
				messages: vec![UpwardMessage::TransferOut {
					source: None,
					dest: SERVICE,
					amount: 100,
					id: last_message_id(),
					source_supervisor_balance: false,
					dest_supervisor_balance: false,
					deferred: Some((MEMO, MEMO_GAS)),
				}],
			}]
		);
		let operation = Jamkb::operation(op).unwrap();
		assert_eq!(operation.state, OperationState::Requested);
		assert_eq!(operation.submitted_at, None);
	});
}

#[test]
fn a_release_moves_the_units_on_jam_and_settles_into_released() {
	ready(|| {
		let (id, op) = release_to(SERVICE, 100);

		// Nothing accumulated yet: nothing to settle, and JAM is untouched.
		poll();
		assert_eq!(Jamkb::operation(op).unwrap().state, OperationState::Requested);
		assert_eq!(Jam::balance(SERVICE), 0);

		accumulate_all(3);
		assert_eq!(Jam::balance(SERVICE), 100);
		assert_eq!(Jam::balance(PARACHAIN_SERVICE), CAP - 100);
		let operation = Jamkb::operation(op).unwrap();
		assert_eq!(operation.state, OperationState::Confirmed(None));
		assert_eq!(operation.submitted_at, Some(3));
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::OperationConfirmed { id: op, entry: None })
		);
		// Resolved, not yet settled: the units are still held.
		assert_eq!(held(custody()), 100);

		poll();

		let allocation = Jamkb::allocation(id).unwrap();
		assert_eq!(allocation.state, AllocationState::Delivered);
		assert_eq!(allocation.delivered_at, Some(3));
		assert_eq!(held(custody()), 0);
		assert_eq!(free(custody()), CAP - 100);
		assert_eq!(free(released()), 100);
		assert_eq!(Assets::total_issuance(JAMKB), CAP);
		assert!(Jamkb::operation(op).is_none());
		assert_books_match_jam();
	});
}

#[test]
fn a_release_to_an_unknown_service_fails_and_returns_the_allocation_to_approved() {
	ready(|| {
		let (id, op) = release_to(UNKNOWN_SERVICE, 100);

		accumulate_all(1);

		assert_eq!(Jamkb::operation(op).unwrap().state, failed(TransferError::UnknownDestination));
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::OperationFailed {
				id: op,
				error: FailureReason::Transfer(TransferError::UnknownDestination),
			})
		);
		// The allocation is locked until the failure is settled.
		assert_noop!(
			Jamkb::execute_release(RuntimeOrigin::signed(BOB), id),
			Error::<Test>::WrongState
		);

		poll();

		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Approved);
		assert_eq!(held(custody()), 100);
		assert_eq!(free(released()), 0);
		assert!(Jamkb::operation(op).is_none());
		assert_books_match_jam();

		// Once the service exists, a retry is a new operation and succeeds.
		Jam::set_service(UNKNOWN_SERVICE, JamService::default());
		assert_ok!(Jamkb::execute_release(RuntimeOrigin::signed(BOB), id));
		let retry = sent().last().unwrap().id;
		assert_ne!(retry, op);
		accumulate_all(2);
		poll();
		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Delivered);
		assert_eq!(Jam::balance(UNKNOWN_SERVICE), 100);
		assert_books_match_jam();
	});
}

#[test]
fn a_target_that_asks_more_memo_gas_than_memo_gas_refuses_the_transfer() {
	ready(|| {
		Jam::set_service(
			SERVICE,
			JamService { balance: 0, threshold: 0, min_memo_gas: MEMO_GAS + 1 },
		);
		let (id, op) = release_to(SERVICE, 100);

		accumulate_all(1);
		assert_eq!(
			Jamkb::operation(op).unwrap().state,
			failed(TransferError::GasBelowDestinationMinimum)
		);
		poll();

		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Approved);
		assert_eq!(Jam::balance(SERVICE), 0);
		assert_books_match_jam();
	});
}

#[test]
fn a_parachain_service_short_of_balance_refuses_the_transfer() {
	ready(|| {
		Jam::set_balance(PARACHAIN_SERVICE, 50);
		let (id, op) = release_to(SERVICE, 100);

		accumulate_all(1);
		assert_eq!(
			Jamkb::operation(op).unwrap().state,
			failed(TransferError::InsufficientServiceBalance)
		);
		poll();

		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Approved);
		assert_eq!(Jam::balance(PARACHAIN_SERVICE), 50);
	});
}

// ── Outcomes and settlement ────────────────────────────────────────────────

#[test]
fn outcomes_for_another_sender_unknown_or_resolved_operations_are_ignored() {
	ready(|| {
		let (_, op) = release_to(SERVICE, 100);
		let failure = [AccumulateLog::TransferFailed {
			id: last_message_id(),
			error: TransferError::UnknownDestination,
		}];
		let mut meter = WeightMeter::new();

		// Another sender's outcome under the same id leaves the operation alone.
		<Jamkb as OnJamOutcome<u64>>::on_outcome(JAMKB_SENDER + 1, op, 1, &failure, &mut meter);
		assert_eq!(Jamkb::operation(op).unwrap().state, OperationState::Requested);

		accumulate_all(1);
		assert_eq!(Jamkb::operation(op).unwrap().state, OperationState::Confirmed(None));

		// A second report for the same operation, and one for an unknown id, change nothing.
		<Jamkb as OnJamOutcome<u64>>::on_outcome(JAMKB_SENDER, op, 2, &failure, &mut meter);
		<Jamkb as OnJamOutcome<u64>>::on_outcome(JAMKB_SENDER, 99, 2, &failure, &mut meter);
		let operation = Jamkb::operation(op).unwrap();
		assert_eq!(operation.state, OperationState::Confirmed(None));
		assert_eq!(operation.submitted_at, Some(1));
		assert_eq!(crate::Resolved::<Test>::get().into_inner(), vec![op]);
		assert!(Jamkb::operation(99).is_none());
	});
}

#[test]
fn recording_an_outcome_is_charged_to_the_meter() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		for _ in 0..3 {
			assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		}
		let charged = accumulate_all(1);
		assert_eq!(charged, <() as WeightInfo>::on_outcome().saturating_mul(3));
	});
}

#[test]
fn on_poll_settles_within_the_settlement_weight_and_keeps_the_rest_in_order() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 400));
		for _ in 0..4 {
			assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		}
		accumulate_all(2);
		for id in (1..=4).map(op) {
			assert_eq!(Jamkb::operation(id).unwrap().state, OperationState::Confirmed(None));
		}

		poll();
		assert!(Jamkb::operation(op(1)).is_none());
		assert!(Jamkb::operation(op(2)).is_none());
		assert!(Jamkb::operation(op(3)).is_none());
		assert_eq!(Jamkb::operation(op(4)).unwrap().state, OperationState::Confirmed(None));
		assert_eq!(crate::Resolved::<Test>::get().into_inner(), vec![op(4)]);
		assert_eq!(free(released()), 30);

		poll();
		assert!(Jamkb::operation(op(4)).is_none());
		assert_eq!(free(released()), 40);
		assert_books_match_jam();
	});
}

#[test]
fn a_failed_settlement_does_not_hold_up_the_rest_and_only_settle_retries_it() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		let (id, release) = release_to(SERVICE, 100);
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 50, SERVICE, MEMO));
		let redemption = sent().last().unwrap().id;
		accumulate_all(1);

		// The release's hold is gone from custody, so its settlement fails; the redemption,
		// sent after it, still settles.
		let releasing = RuntimeHoldReason::Jamkb(HoldReason::Releasing);
		assert_ok!(<AssetsHolder as MutateHold<AccountId>>::release(
			JAMKB,
			&releasing,
			&custody(),
			100,
			Precision::Exact,
		));
		poll_unchecked();

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Jamkb(Event::SettlementFailed { id, error })
				if id == release && error == TokenError::Frozen.into()
		)));
		assert_eq!(Jamkb::operation(release).unwrap().state, OperationState::Confirmed(None));
		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Delivering);
		assert!(Jamkb::operation(redemption).is_none());
		assert!(crate::Resolved::<Test>::get().is_empty());

		// Once the cause is gone, `on_poll` does not pick it up again; `settle` does.
		assert_ok!(<AssetsHolder as MutateHold<AccountId>>::hold(
			JAMKB,
			&releasing,
			&custody(),
			100
		));
		poll();
		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Delivering);
		assert_ok!(Jamkb::settle(RuntimeOrigin::signed(BOB), release));
		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Delivered);
		assert!(Jamkb::operation(release).is_none());
		assert_books_match_jam();
	});
}

#[test]
fn settle_settles_a_resolved_operation_at_once() {
	ready(|| {
		let (id, op) = release_to(SERVICE, 100);
		assert_noop!(Jamkb::settle(RuntimeOrigin::signed(ALICE), op), Error::<Test>::NotResolved);

		accumulate_all(1);
		assert_ok!(Jamkb::settle(RuntimeOrigin::signed(ALICE), op));

		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Delivered);
		assert!(crate::Resolved::<Test>::get().is_empty());
		assert_noop!(
			Jamkb::settle(RuntimeOrigin::signed(ALICE), op),
			Error::<Test>::UnknownOperation
		);
		// `on_poll` finds nothing left.
		poll();
		assert_eq!(free(released()), 100);
	});
}

// ── Redemption ─────────────────────────────────────────────────────────────

#[test]
fn redeem_holds_on_the_caller_and_confirms_into_released() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 300));
		assert_noop!(
			Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, PARACHAIN_SERVICE, MEMO),
			Error::<Test>::TargetIsParachainService
		);
		assert_noop!(
			Jamkb::redeem(RuntimeOrigin::signed(ALICE), MIN_TRANSFER - 1, SERVICE, MEMO),
			Error::<Test>::BelowMinTransfer
		);
		assert_noop!(
			Jamkb::redeem(RuntimeOrigin::signed(ALICE), 301, SERVICE, MEMO),
			TokenError::FundsUnavailable
		);

		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 200, SERVICE, MEMO));
		assert_eq!(held(ALICE), 200);
		assert_eq!(free(ALICE), 100);
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::OperationSubmitted { id: op(1), allocation: None })
		);
		assert!(matches!(
			sent()[0].messages[0],
			UpwardMessage::TransferOut {
				dest: SERVICE,
				amount: 200,
				deferred: Some((MEMO, MEMO_GAS)),
				..
			}
		));

		accumulate_all(1);
		poll();

		assert_eq!(held(ALICE), 0);
		assert_eq!(free(ALICE), 100);
		assert_eq!(free(released()), 200);
		assert_eq!(Jam::balance(SERVICE), 200);
		assert_eq!(Assets::total_issuance(JAMKB), CAP);
		assert_books_match_jam();
	});
}

#[test]
fn a_failed_redemption_releases_the_hold() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 300));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 200, UNKNOWN_SERVICE, MEMO));

		accumulate_all(1);
		assert_eq!(
			Jamkb::operation(op(1)).unwrap().state,
			failed(TransferError::UnknownDestination)
		);
		poll();

		assert_eq!(held(ALICE), 0);
		assert_eq!(free(ALICE), 300);
		assert_eq!(free(released()), 0);
		assert_books_match_jam();
	});
}

#[test]
fn redemptions_are_independent_of_each_other() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 300));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 100, SERVICE, MEMO));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 150, UNKNOWN_SERVICE, MEMO));
		assert_eq!(held(ALICE), 250);
		assert_eq!(sent().len(), 2);

		// The second fails, the first confirms.
		accumulate_all(1);
		poll();

		assert_eq!(held(ALICE), 0);
		assert_eq!(free(ALICE), 200);
		assert_eq!(free(released()), 100);
		assert_books_match_jam();
	});
}

// ── Cancellation ───────────────────────────────────────────────────────────

#[test]
fn governance_cancels_an_approved_allocation_at_any_time() {
	ready(|| {
		assert_ok!(Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 100, MEMO, None, 100));
		assert_ok!(Jamkb::cancel_allocation(RuntimeOrigin::root(), 1));

		assert_eq!(Jamkb::allocation(1).unwrap().state, AllocationState::Closed);
		assert_eq!(held(custody()), 0);
		assert_eq!(free(custody()), CAP);
		assert_eq!(last_event(), RuntimeEvent::Jamkb(Event::AllocationCancelled { id: 1 }));
		assert_noop!(Jamkb::cancel_allocation(RuntimeOrigin::root(), 1), Error::<Test>::WrongState);
		assert_noop!(
			Jamkb::execute_release(RuntimeOrigin::signed(BOB), 1),
			Error::<Test>::WrongState
		);
	});
}

#[test]
fn anyone_cancels_an_approved_allocation_from_its_expiry_on() {
	ready(|| {
		assert_ok!(Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 100, MEMO, None, 10));

		assert_noop!(
			Jamkb::cancel_allocation(RuntimeOrigin::signed(BOB), 1),
			Error::<Test>::NotExpired
		);
		run_to_block(10);
		assert_noop!(
			Jamkb::cancel_allocation(RuntimeOrigin::signed(BOB), 1),
			Error::<Test>::NotExpired
		);
		run_to_block(11);
		// The origin is checked before anything else.
		assert_noop!(
			Jamkb::cancel_allocation(RuntimeOrigin::none(), 99),
			sp_runtime::DispatchError::BadOrigin
		);
		assert_ok!(Jamkb::cancel_allocation(RuntimeOrigin::signed(BOB), 1));

		assert_eq!(Jamkb::allocation(1).unwrap().state, AllocationState::Closed);
		assert_eq!(held(custody()), 0);
	});
}

#[test]
fn a_delivering_allocation_cannot_be_cancelled() {
	ready(|| {
		let (id, _) = release_to(UNKNOWN_SERVICE, 100);
		assert_noop!(
			Jamkb::cancel_allocation(RuntimeOrigin::root(), id),
			Error::<Test>::WrongState
		);

		// Nor after its delivery failed but before the failure is settled.
		accumulate_all(1);
		assert_noop!(
			Jamkb::cancel_allocation(RuntimeOrigin::root(), id),
			Error::<Test>::WrongState
		);

		poll();
		assert_ok!(Jamkb::cancel_allocation(RuntimeOrigin::root(), id));
	});
}

// ── Transport limits ───────────────────────────────────────────────────────

#[test]
fn a_full_transport_queue_rejects_the_call_and_leaves_nothing_behind() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		for _ in 0..DEFAULT_TRANSPORT_CAPACITY {
			assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		}
		assert_noop!(
			Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO),
			Error::<Test>::Transport
		);
		assert_eq!(held(ALICE), 40);

		// Once the transport has sent some, there is room again.
		accumulate(2, 1);
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
	});
}

#[test]
fn the_unsettled_operations_are_bounded() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		for _ in 0..DEFAULT_TRANSPORT_CAPACITY {
			assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		}
		accumulate_all(1);
		for _ in 0..2 {
			assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		}
		// Six unsettled: four resolved, two queued.
		assert_noop!(
			Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO),
			Error::<Test>::TooManyOperations
		);

		poll();
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
	});
}

#[test]
fn allocation_ids_start_at_one_and_are_never_reused() {
	ready(|| {
		assert_ok!(Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 10, MEMO, None, 100));
		assert_ok!(Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 10, MEMO, None, 100));
		assert_ok!(Jamkb::cancel_allocation(RuntimeOrigin::root(), 1));
		assert_ok!(Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 10, MEMO, None, 100));
		assert!(Jamkb::allocation(1).is_some());
		assert!(Jamkb::allocation(3).is_some());
	});
}

#[test]
fn events_follow_the_operation_lifecycle() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		System::reset_events();
		// A failing redemption ahead of the release, so both outcomes appear in send order.
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, UNKNOWN_SERVICE, MEMO));
		let redemption = sent().last().unwrap().id;
		let (id, release) = release_to(SERVICE, 100);
		accumulate_all(1);
		poll();
		assert_eq!(
			jamkb_events(),
			vec![
				Event::OperationSubmitted { id: redemption, allocation: None },
				Event::ReleaseApproved { id, target: SERVICE, amount: 100 },
				Event::OperationSubmitted { id: release, allocation: Some(id) },
				Event::OperationFailed {
					id: redemption,
					error: FailureReason::Transfer(TransferError::UnknownDestination),
				},
				Event::OperationConfirmed { id: release, entry: None },
			]
		);
	});
}

// ── End to end ─────────────────────────────────────────────────────────────

#[test]
fn asset_hub_and_jam_agree_after_a_mix_of_releases_and_redemptions() {
	ready(|| {
		const OTHER: ServiceId = 7;
		Jam::set_service(
			OTHER,
			JamService { balance: 0, threshold: 0, min_memo_gas: MEMO_GAS + 1 },
		);
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 500));

		// Block 1: two releases and a redemption; the one to `OTHER` is refused for its gas.
		let (to_service, _) = release_to(SERVICE, 100);
		let (to_other, _) = release_to(OTHER, 70);
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 40, SERVICE, MEMO));
		accumulate_all(1);
		poll();

		// Block 2: one redemption to a service that does not exist, one that lands.
		run_to_block(2);
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 30, UNKNOWN_SERVICE, MEMO));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 20, SERVICE, MEMO));
		accumulate_all(2);
		poll();

		assert_eq!(Jamkb::allocation(to_service).unwrap().state, AllocationState::Delivered);
		assert_eq!(Jamkb::allocation(to_other).unwrap().state, AllocationState::Approved);
		assert_eq!(Jam::balance(SERVICE), 100 + 40 + 20);
		assert_eq!(Jam::balance(OTHER), 0);
		assert_eq!(free(released()), 160);
		assert_eq!(held(custody()), 70);
		assert_eq!(free(ALICE), 500 - 40 - 20);
		assert_eq!(held(ALICE), 0);
		assert_eq!(Assets::total_issuance(JAMKB), CAP);
		assert!(crate::Resolved::<Test>::get().is_empty());
		assert_books_match_jam();
	});
}

// ── Review follow-ups ──────────────────────────────────────────────────────

#[test]
fn a_transfer_failed_for_another_message_does_not_fail_the_operation() {
	ready(|| {
		let (_, op) = release_to(SERVICE, 100);
		let stranger = [AccumulateLog::TransferFailed {
			id: last_message_id() + 1,
			error: TransferError::UnknownDestination,
		}];
		<Jamkb as OnJamOutcome<u64>>::on_outcome(
			JAMKB_SENDER,
			op,
			1,
			&stranger,
			&mut WeightMeter::new(),
		);
		assert_eq!(Jamkb::operation(op).unwrap().state, OperationState::Confirmed(None));
	});
}

#[test]
fn the_parachain_service_keeps_its_threshold_balance() {
	ready(|| {
		// Releasing all of custody would take the Parachain Service below its threshold.
		let (all, op) = release_to(SERVICE, CAP - PS_THRESHOLD + 1);
		accumulate_all(1);
		assert_eq!(
			Jamkb::operation(op).unwrap().state,
			failed(TransferError::InsufficientServiceBalance)
		);
		poll();
		assert_ok!(Jamkb::cancel_allocation(RuntimeOrigin::root(), all));

		let (_, op) = release_to(SERVICE, CAP - PS_THRESHOLD);
		accumulate_all(2);
		assert_eq!(Jamkb::operation(op).unwrap().state, OperationState::Confirmed(None));
		poll();
		assert_eq!(Jam::balance(PARACHAIN_SERVICE), PS_THRESHOLD);
		assert_books_match_jam();
	});
}

#[test]
fn on_poll_charges_its_reads_when_idle_and_each_settlement_when_busy() {
	ready(|| {
		let reads = MockDbWeight::get().reads(1);
		assert_eq!(poll(), reads);

		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		for _ in 0..2 {
			assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		}
		accumulate_all(1);
		assert_eq!(
			poll(),
			settlement_overhead().saturating_add(<() as WeightInfo>::settle().saturating_mul(2))
		);
		assert_eq!(poll(), reads);
	});
}

#[test]
fn on_poll_settles_only_what_the_weight_left_in_the_block_allows() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		for _ in 0..3 {
			assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		}
		accumulate_all(1);

		// Less than `SettlementWeight` is left: one settlement fits.
		let one = settlement_overhead().saturating_add(<() as WeightInfo>::settle());
		assert_eq!(poll_within(one), one);
		assert!(Jamkb::operation(op(1)).is_none());
		assert_eq!(crate::Resolved::<Test>::get().into_inner(), vec![op(2), op(3)]);

		// Not even the reads fit: nothing is read, nothing settles.
		let reads = MockDbWeight::get().reads(1);
		assert_eq!(poll_within(reads - Weight::from_parts(1, 0)), Weight::zero());
		// The reads fit, the writes do not: the lists are read and left as they are.
		assert_eq!(poll_within(settlement_overhead() - Weight::from_parts(1, 0)), reads);
		// Only the overhead fits: the lists are read, nothing settles.
		assert_eq!(poll_within(settlement_overhead()), settlement_overhead());
		assert_eq!(crate::Resolved::<Test>::get().into_inner(), vec![op(2), op(3)]);
	});
}

#[test]
fn failed_settlements_leave_the_queue_and_wait_for_settle() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		let releases: Vec<u64> =
			(0..SETTLEMENTS_PER_BLOCK).map(|_| release_to(SERVICE, 10).1).collect();
		accumulate_all(1);

		// Every release's settlement fails: their holds are gone from custody.
		let releasing = RuntimeHoldReason::Jamkb(HoldReason::Releasing);
		let all = 10 * SETTLEMENTS_PER_BLOCK as Balance;
		assert_ok!(<AssetsHolder as MutateHold<AccountId>>::release(
			JAMKB,
			&releasing,
			&custody(),
			all,
			Precision::Exact,
		));
		poll_unchecked();
		assert!(crate::Resolved::<Test>::get().is_empty());
		for release in &releases {
			assert_eq!(Jamkb::operation(*release).unwrap().state, OperationState::Confirmed(None));
		}

		// A redemption resolved later gets the whole budget in the next block.
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		let redemption = sent().last().unwrap().id;
		accumulate_all(2);
		poll_unchecked();
		assert!(Jamkb::operation(redemption).is_none());
		assert_eq!(free(released()), 10);

		// Once the cause is gone, `settle` settles each failed one.
		assert_ok!(<AssetsHolder as MutateHold<AccountId>>::hold(
			JAMKB,
			&releasing,
			&custody(),
			all
		));
		for release in releases {
			assert_ok!(Jamkb::settle(RuntimeOrigin::signed(BOB), release));
		}
		assert_eq!(free(released()), 10 + all);
		assert_books_match_jam();
	});
}

#[test]
fn a_failed_call_rolls_back_what_it_queued_at_the_transport() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		let result = frame_support::storage::with_storage_layer(|| -> DispatchResult {
			assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
			Err(DispatchError::Other("later step failed"))
		});
		assert!(result.is_err());
		assert!(sent().is_empty());
		assert_eq!(last_message_id(), MESSAGE_ID_BASE);
	});
}

#[test]
fn incoming_transfers_are_not_taken_yet() {
	ready(|| {
		use crate::transport::OnIncomingTransfers;
		assert_eq!(<Jamkb as OnIncomingTransfers<Balance>>::capacity(), 0);
	});
}

// ── Ids, errors and edge cases ─────────────────────────────────────────────

#[test]
fn an_entry_carrying_the_operation_id_instead_of_the_message_id_is_not_a_failure() {
	ready(|| {
		let (_, op) = release_to(SERVICE, 100);
		assert_ne!(op, last_message_id());
		let wrong = [AccumulateLog::TransferFailed { id: op, error: TransferError::UnknownSource }];
		<Jamkb as OnJamOutcome<u64>>::on_outcome(
			JAMKB_SENDER,
			op,
			1,
			&wrong,
			&mut WeightMeter::new(),
		);
		assert_eq!(Jamkb::operation(op).unwrap().state, OperationState::Confirmed(None));
	});
}

#[test]
fn the_runtime_configuration_passes_the_integrity_test() {
	new_test_ext().execute_with(|| {
		<Jamkb as frame_support::traits::Hooks<u64>>::integrity_test();
	});
}

#[test]
fn transport_refusals_map_to_pallet_errors_and_leave_nothing_behind() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		assert_ok!(Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 10, MEMO, None, 9));
		for (refusal, error) in [
			(MessageSendError::TooBig, DispatchError::from(Error::<Test>::Transport)),
			(MessageSendError::Conflict, Error::<Test>::Transport.into()),
			(MessageSendError::QueueFull, Error::<Test>::Transport.into()),
		] {
			SendFailure::set(Some(refusal));
			assert_noop!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO), error);
			assert_noop!(Jamkb::execute_release(RuntimeOrigin::signed(BOB), 1), error);
		}
		assert_eq!(last_message_id(), MESSAGE_ID_BASE);
		assert!(sent().is_empty());
		assert_eq!(held(ALICE), 0);
		assert_eq!(Jamkb::allocation(1).unwrap().state, AllocationState::Approved);

		SendFailure::set(None);
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
	});
}

#[test]
fn settle_undoes_a_failed_redemption_and_a_failed_delivery() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 40, SERVICE, MEMO));
		let (id, _) = release_to(SERVICE, 100);
		let redemption = fail_next(1, TransferError::GasBelowDestinationMinimum);
		let release = fail_next(1, TransferError::InsufficientServiceBalance);

		assert_ok!(Jamkb::settle(RuntimeOrigin::signed(BOB), redemption));
		assert_eq!(held(ALICE), 0);
		assert_eq!(free(ALICE), 100);

		assert_ok!(Jamkb::settle(RuntimeOrigin::signed(BOB), release));
		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Approved);
		assert!(crate::PendingOperationOf::<Test>::get(id).is_none());
		assert_eq!(held(custody()), 100);

		assert_eq!(free(released()), 0);
		assert!(crate::Resolved::<Test>::get().is_empty());
		assert_books_match_jam();
	});
}

#[test]
fn settle_retries_a_failed_settlement_until_its_cause_is_gone() {
	ready(|| {
		let (id, op) = release_to(SERVICE, 100);
		accumulate_all(1);
		let releasing = RuntimeHoldReason::Jamkb(HoldReason::Releasing);
		assert_ok!(<AssetsHolder as MutateHold<AccountId>>::release(
			JAMKB,
			&releasing,
			&custody(),
			100,
			Precision::Exact,
		));
		poll_unchecked();
		assert!(crate::Resolved::<Test>::get().is_empty());

		assert_noop!(Jamkb::settle(RuntimeOrigin::signed(BOB), op), TokenError::Frozen);
		assert!(Jamkb::operation(op).is_some());

		assert_ok!(<AssetsHolder as MutateHold<AccountId>>::hold(
			JAMKB,
			&releasing,
			&custody(),
			100
		));
		assert_ok!(Jamkb::settle(RuntimeOrigin::signed(BOB), op));
		assert!(Jamkb::operation(op).is_none());
		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Delivered);
	});
}

#[test]
fn attest_cannot_record_more_than_custody_holds() {
	build_and_execute(|| {
		assert_ok!(Jamkb::initialize(RuntimeOrigin::root()));
		assert_noop!(
			Jamkb::attest(RuntimeOrigin::root(), ANCHOR, CAP + 1),
			ArithmeticError::Underflow
		);
		assert!(crate::Attestation::<Test>::get().is_none());

		// Everything outside the Parachain Service: nothing is left to release.
		Jam::set_balance(PARACHAIN_SERVICE, 0);
		Jam::set_balance(SERVICE, CAP);
		assert_ok!(Jamkb::attest(RuntimeOrigin::root(), ANCHOR, CAP));
		assert_noop!(
			Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 10, MEMO, None, 10),
			TokenError::FundsUnavailable
		);
		assert_books_match_jam();
	});
}

#[test]
fn held_units_can_be_neither_granted_nor_redeemed_twice() {
	ready(|| {
		assert_ok!(Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 100, MEMO, None, 10));
		assert_noop!(
			Jamkb::grant(RuntimeOrigin::root(), ALICE, CAP - 99),
			TokenError::FundsUnavailable
		);

		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 60, SERVICE, MEMO));
		assert_noop!(
			Jamkb::redeem(RuntimeOrigin::signed(ALICE), 41, SERVICE, MEMO),
			TokenError::FundsUnavailable
		);
	});
}

#[test]
fn grant_rejects_the_pallet_accounts() {
	ready(|| {
		for account in Jamkb::pallet_accounts() {
			assert_noop!(
				Jamkb::grant(RuntimeOrigin::root(), account, 10),
				Error::<Test>::PalletAccount
			);
		}
	});
}

#[test]
fn an_allocation_whose_delivery_failed_can_be_cancelled_once_settled() {
	ready(|| {
		let (id, _) = release_to(UNKNOWN_SERVICE, 100);
		fail_next(1, TransferError::UnknownDestination);
		poll();

		// Before expiry only the approver may cancel; after it anyone may.
		assert_noop!(
			Jamkb::cancel_allocation(RuntimeOrigin::signed(BOB), id),
			Error::<Test>::NotExpired
		);
		run_to_block(101);
		assert_ok!(Jamkb::cancel_allocation(RuntimeOrigin::signed(BOB), id));

		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Closed);
		assert_eq!(held(custody()), 0);
		assert_eq!(free(custody()), CAP);
		assert_eq!(last_event(), RuntimeEvent::Jamkb(Event::AllocationCancelled { id }));
		assert_books_match_jam();
	});
}

#[test]
fn a_refusal_between_successes_in_one_block_keeps_send_order_and_the_books() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 20, SERVICE, MEMO));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 30, SERVICE, MEMO));
		let [first, second, third] = [op(1), op(2), op(3)];

		accumulate(1, 1);
		assert_eq!(fail_next(1, TransferError::GasBelowDestinationMinimum), second);
		accumulate(1, 1);
		assert_eq!(crate::Resolved::<Test>::get().into_inner(), vec![first, second, third]);

		poll();
		assert_eq!(free(released()), 40);
		assert_eq!(held(ALICE), 0);
		assert_eq!(free(ALICE), 60);
		assert_eq!(Jam::balance(SERVICE), 40);
		assert_books_match_jam();
	});
}

#[test]
fn settle_may_settle_an_operation_ahead_of_send_order() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 20, SERVICE, MEMO));
		accumulate_all(1);

		// The later operation is settled first, by hand; `on_poll` then settles the earlier one.
		assert_ok!(Jamkb::settle(RuntimeOrigin::signed(BOB), op(2)));
		assert_eq!(crate::Resolved::<Test>::get().into_inner(), vec![op(1)]);
		assert_eq!(free(released()), 20);
		poll();
		assert_eq!(free(released()), 30);
		assert_books_match_jam();
	});
}
