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
	transport::{MessageSendError, OnIncomingTransfers, OnJamOutcome},
	AllocationMode, AllocationState, Error, Event, FailureReason, OperationState, ReturnClass,
};
use frame_support::{assert_noop, assert_ok, traits::fungibles::Inspect, weights::WeightMeter};
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

/// The ids of the operations listed for `on_poll`, in order.
fn listed() -> Vec<u64> {
	crate::Resolved::<Test>::get().into_iter().map(|(id, _)| id).collect()
}

fn failed(error: TransferError) -> OperationState<Balance> {
	OperationState::Failed(FailureReason::TransferFailed(error))
}

// ── Setup and control ──────────────────────────────────────────────────────

#[test]
fn initialize_creates_the_asset_and_mints_nothing() {
	build_and_execute(|| {
		assert_noop!(
			Jamkb::initialize(RuntimeOrigin::signed(ALICE)),
			sp_runtime::DispatchError::BadOrigin
		);
		assert!(!Assets::asset_exists(JAMKB));

		assert_ok!(Jamkb::initialize(RuntimeOrigin::root()));

		assert!(Assets::asset_exists(JAMKB));
		assert_eq!(supply(), 0);
		assert_eq!(Assets::minimum_balance(JAMKB), 1);
		// The asset is not sufficient: custody exists through its own provider.
		assert_eq!(System::providers(&custody()), 1);
		assert_eq!(Mapped::get(), vec![custody()]);

		assert_noop!(Jamkb::initialize(RuntimeOrigin::root()), pallet_assets::Error::<Test>::InUse);
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
		assert_noop!(Jamkb::claim(RuntimeOrigin::signed(ALICE), ALICE), Error::<Test>::NotAttested);
		assert_noop!(
			Jamkb::dispose_excess(RuntimeOrigin::root(), SERVICE, 10, false, MEMO),
			Error::<Test>::NotAttested
		);
		assert_noop!(
			Jamkb::force_mint(RuntimeOrigin::root(), ANCHOR, ALICE, 10),
			Error::<Test>::NotAttested
		);
		assert_eq!(<Jamkb as OnIncomingTransfers<Balance>>::capacity(), 0);

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
fn attest_mints_the_units_on_the_parachain_service_into_custody() {
	build_and_execute(|| {
		// At genesis another service holds 500 of the supply.
		Jam::set_balance(PARACHAIN_SERVICE, CAP - 500);
		Jam::set_balance(SERVICE, 500);
		assert_ok!(Jamkb::initialize(RuntimeOrigin::root()));

		assert_ok!(Jamkb::attest(RuntimeOrigin::root(), ANCHOR, 500));

		assert_eq!(free(custody()), CAP - 500);
		assert_eq!(supply(), CAP - 500);
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
fn approve_release_records_the_allocation_and_burns_nothing() {
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

		assert_ok!(Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 100, MEMO, Some(5), 10));

		let allocation = Jamkb::allocation(1).unwrap();
		assert_eq!(allocation.approver, None);
		assert_eq!(allocation.state, AllocationState::Approved);
		assert_eq!(allocation.mode, AllocationMode::Permanent { memo: MEMO });
		assert_eq!(allocation.target, SERVICE);
		assert_eq!(allocation.amount, 100);
		assert_eq!(allocation.valid_from, 5);
		assert_eq!(allocation.expires_at, 15);
		assert_eq!(free(custody()), CAP);
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::ReleaseApproved { id: 1, target: SERVICE, amount: 100 })
		);
	});
}

#[test]
fn valid_from_none_means_the_current_timeslot() {
	ready(|| {
		run_to_timeslot(7);
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

		run_to_timeslot(15);
		assert_noop!(Jamkb::execute_release(RuntimeOrigin::signed(BOB), 1), Error::<Test>::Expired);

		run_to_timeslot(14);
		assert_ok!(Jamkb::execute_release(RuntimeOrigin::signed(BOB), 1));
		assert_eq!(Jamkb::allocation(1).unwrap().state, AllocationState::Delivering);
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::OperationRequested { id: op(1), allocation: Some(1) })
		);
		assert_noop!(
			Jamkb::execute_release(RuntimeOrigin::signed(BOB), 1),
			Error::<Test>::WrongState
		);
	});
}

#[test]
fn execute_release_burns_the_amount_from_custody() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, CAP - 150));
		assert_ok!(Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 100, MEMO, None, 100));
		assert_ok!(Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 100, MEMO, None, 100));

		assert_ok!(Jamkb::execute_release(RuntimeOrigin::signed(BOB), 1));
		assert_eq!(free(custody()), 50);
		assert_eq!(supply(), CAP - 100);

		// Custody cannot cover the second release.
		assert_noop!(
			Jamkb::execute_release(RuntimeOrigin::signed(BOB), 2),
			Error::<Test>::InsufficientBalance
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
		assert_eq!(operation.accumulated_at, None);
	});
}

#[test]
fn a_release_burns_on_asset_hub_and_moves_the_units_on_jam() {
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
		assert_eq!(operation.accumulated_at, Some(3));
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::OperationConfirmed { id: op, entry: None })
		);
		// The units were burned when the release was executed.
		assert_eq!(free(custody()), CAP - 100);

		poll();

		let allocation = Jamkb::allocation(id).unwrap();
		assert_eq!(allocation.state, AllocationState::Delivered);
		assert_eq!(allocation.delivered_at, Some(3));
		assert_eq!(free(custody()), CAP - 100);
		assert_eq!(supply(), CAP - 100);
		assert!(Jamkb::operation(op).is_none());
		assert_books_match_jam();
	});
}

#[test]
fn a_failed_release_mints_the_units_back_to_custody() {
	ready(|| {
		let (id, op) = release_to(UNKNOWN_SERVICE, 100);

		accumulate_all(1);

		assert_eq!(Jamkb::operation(op).unwrap().state, failed(TransferError::UnknownDestination));
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::OperationFailed {
				id: op,
				error: FailureReason::TransferFailed(TransferError::UnknownDestination),
			})
		);
		// The allocation is locked until the failure is settled.
		assert_noop!(
			Jamkb::execute_release(RuntimeOrigin::signed(BOB), id),
			Error::<Test>::WrongState
		);

		poll();

		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Approved);
		assert_eq!(free(custody()), CAP);
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
		Jam::set_service(SERVICE, JamService { min_memo_gas: MEMO_GAS + 1, ..Default::default() });
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
		<Jamkb as OnJamOutcome>::on_outcome(JAMKB_SENDER + 1, op, 1, &failure, &mut meter);
		assert_eq!(Jamkb::operation(op).unwrap().state, OperationState::Requested);

		accumulate_all(1);
		assert_eq!(Jamkb::operation(op).unwrap().state, OperationState::Confirmed(None));

		// A second report for the same operation, and one for an unknown id, change nothing.
		<Jamkb as OnJamOutcome>::on_outcome(JAMKB_SENDER, op, 2, &failure, &mut meter);
		<Jamkb as OnJamOutcome>::on_outcome(JAMKB_SENDER, 99, 2, &failure, &mut meter);
		let operation = Jamkb::operation(op).unwrap();
		assert_eq!(operation.state, OperationState::Confirmed(None));
		assert_eq!(operation.accumulated_at, Some(1));
		assert_eq!(listed(), vec![op]);
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
		let count = SETTLEMENTS_PER_BLOCK + 1;
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 400));
		for _ in 0..DEFAULT_TRANSPORT_CAPACITY {
			assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		}
		accumulate_all(2);
		for _ in DEFAULT_TRANSPORT_CAPACITY as u64..count {
			assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		}
		accumulate_all(2);
		for id in (1..=count).map(op) {
			assert_eq!(Jamkb::operation(id).unwrap().state, OperationState::Confirmed(None));
		}

		poll();
		for id in (1..count).map(op) {
			assert!(Jamkb::operation(id).is_none());
		}
		assert_eq!(Jamkb::operation(op(count)).unwrap().state, OperationState::Confirmed(None));
		assert_eq!(listed(), vec![op(count)]);

		poll();
		assert!(Jamkb::operation(op(count)).is_none());
		assert_books_match_jam();
	});
}

#[test]
fn a_failed_settlement_blocks_its_target_but_not_the_others() {
	ready(|| {
		const OTHER: ServiceId = 7;
		Jam::set_service(OTHER, JamService::default());
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		let (id, release) = release_to(SERVICE, 100);
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 50, SERVICE, MEMO));
		let same_target = sent().last().unwrap().id;
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 20, OTHER, MEMO));
		let other_target = sent().last().unwrap().id;
		fail_next(1, TransferError::GasBelowDestinationMinimum);
		accumulate_all(1);

		// The supply has no room for the failed release's units, so its settlement fails. The
		// redemption to the same target waits behind it; the one to another target settles.
		let filled = fill_supply();
		poll();

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Jamkb(Event::SettlementFailed { id, error })
				if id == release && error == ArithmeticError::Overflow.into()
		)));
		assert_eq!(
			Jamkb::operation(release).unwrap().state,
			failed(TransferError::GasBelowDestinationMinimum)
		);
		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Delivering);
		assert_eq!(crate::Blocked::<Test>::get(SERVICE), Some(release));
		assert_eq!(Jamkb::operation(same_target).unwrap().state, OperationState::Confirmed(None));
		assert!(Jamkb::operation(other_target).is_none());
		assert!(listed().is_empty());

		// The waiting operation cannot be settled ahead of the failed one.
		assert_noop!(
			Jamkb::settle(RuntimeOrigin::signed(BOB), same_target),
			Error::<Test>::OutOfOrder
		);

		// Once the cause is gone, `on_poll` does not pick it up again; `settle` does, and the
		// target's later operation is next in line for `on_poll`.
		drain_supply(filled);
		poll();
		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Delivering);
		let info = Jamkb::settle(RuntimeOrigin::signed(BOB), release).expect("the cause is gone");
		// Unblocking reads the target's queue and its one waiting operation, and writes
		// `Blocked` and `Resolved`.
		assert_eq!(
			info.actual_weight,
			Some(
				<() as WeightInfo>::settle().saturating_add(MockDbWeight::get().reads_writes(2, 2))
			)
		);
		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Approved);
		assert_eq!(free(custody()), CAP - 100);
		assert!(Jamkb::operation(release).is_none());
		assert!(crate::Blocked::<Test>::get(SERVICE).is_none());
		assert_eq!(listed(), vec![same_target]);
		poll();
		assert!(Jamkb::operation(same_target).is_none());
		assert_books_match_jam();
	});
}

#[test]
fn settle_takes_any_resolved_head() {
	ready(|| {
		let (id, op) = release_to(SERVICE, 100);
		assert_noop!(Jamkb::settle(RuntimeOrigin::signed(ALICE), op), Error::<Test>::NotResolved);

		accumulate_all(1);
		assert_eq!(listed(), vec![op]);
		let info = Jamkb::settle(RuntimeOrigin::signed(ALICE), op).expect("the head settles");
		// Charged for what it settled, not for the declared full `Move`.
		assert_eq!(info.actual_weight, Some(<() as WeightInfo>::settle()));
		assert!(listed().is_empty());
		assert_ok!(Jamkb::do_try_state());
		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Delivered);

		assert_noop!(
			Jamkb::settle(RuntimeOrigin::signed(ALICE), op),
			Error::<Test>::UnknownOperation
		);
		assert_eq!(supply(), CAP - 100);
		assert_books_match_jam();
	});
}

#[test]
fn settle_rescues_an_operation_that_was_never_listed() {
	ready(|| {
		const LOST: u64 = 500;
		crate::Operations::<Test>::insert(
			LOST,
			crate::Operation {
				id: LOST,
				payload: crate::OperationPayload::Refund {
					source: SERVICE,
					amount: 10,
					memo: MEMO,
				},
				state: OperationState::Confirmed(None),
				accumulated_at: Some(1),
				target: SERVICE,
				message_id: 1,
			},
		);
		crate::TargetOperations::<Test>::mutate(SERVICE, |queue| queue.try_push(LOST).unwrap());
		assert!(listed().is_empty());

		assert_ok!(Jamkb::settle(RuntimeOrigin::signed(BOB), LOST));
		assert!(Jamkb::operation(LOST).is_none());
		assert!(crate::TargetOperations::<Test>::get(SERVICE).is_empty());
	});
}

// ── Redemption ─────────────────────────────────────────────────────────────

#[test]
fn redeem_burns_from_the_caller() {
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
			Error::<Test>::InsufficientBalance
		);

		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 200, SERVICE, MEMO));
		assert_eq!(free(ALICE), 100);
		assert_eq!(supply(), CAP - 200);
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::OperationRequested { id: op(1), allocation: None })
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

		assert_eq!(free(ALICE), 100);
		assert_eq!(Jam::balance(SERVICE), 200);
		assert_eq!(supply(), CAP - 200);
		assert_books_match_jam();
	});
}

#[test]
fn a_failed_redemption_mints_the_units_back() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 300));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 200, UNKNOWN_SERVICE, MEMO));

		accumulate_all(1);
		assert_eq!(
			Jamkb::operation(op(1)).unwrap().state,
			failed(TransferError::UnknownDestination)
		);
		poll();

		assert_eq!(free(ALICE), 300);
		assert_eq!(supply(), CAP);
		assert_books_match_jam();
	});
}

#[test]
fn a_failed_redemption_to_a_holder_that_cannot_receive_it_is_booked_unclaimed() {
	ready(|| {
		use frame_support::traits::{
			fungible::Mutate as _,
			tokens::{Fortitude, Precision, Preservation},
		};
		const CAROL: AccountId = 3;
		assert_ok!(Balances::mint_into(&CAROL, NATIVE));
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), CAROL, 50));
		// The whole balance is burned, and with it the holder's JAMKB account.
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(CAROL), 50, UNKNOWN_SERVICE, MEMO));
		assert!(!pallet_assets::Account::<Test>::contains_key(JAMKB, CAROL));
		// The holder leaves, so a new JAMKB account cannot be created for it.
		assert_ok!(Balances::burn_from(
			&CAROL,
			native(CAROL),
			Preservation::Expendable,
			Precision::Exact,
			Fortitude::Polite,
		));

		accumulate_all(1);
		poll();

		assert_eq!(free(CAROL), 0);
		assert_eq!(unclaimed_of(CAROL), 50);
		assert!(jamkb_events().contains(&Event::UnclaimedBooked { beneficiary: CAROL, amount: 50 }));
		assert_books_match_jam();
	});
}

#[test]
fn redemptions_are_independent_of_each_other() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 300));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 100, SERVICE, MEMO));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 150, UNKNOWN_SERVICE, MEMO));
		assert_eq!(free(ALICE), 50);
		assert_eq!(sent().len(), 2);

		// The second fails, the first confirms.
		accumulate_all(1);
		poll();

		assert_eq!(free(ALICE), 200);
		assert_eq!(supply(), CAP - 100);
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
			sp_runtime::DispatchError::BadOrigin
		);
		run_to_timeslot(10);
		assert_noop!(
			Jamkb::cancel_allocation(RuntimeOrigin::signed(BOB), 1),
			sp_runtime::DispatchError::BadOrigin
		);
		run_to_timeslot(11);
		// The origin is checked before anything else.
		assert_noop!(
			Jamkb::cancel_allocation(RuntimeOrigin::none(), 99),
			sp_runtime::DispatchError::BadOrigin
		);
		assert_ok!(Jamkb::cancel_allocation(RuntimeOrigin::signed(BOB), 1));

		assert_eq!(Jamkb::allocation(1).unwrap().state, AllocationState::Closed);
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
			Error::<Test>::TransportQueueFull
		);
		assert_eq!(free(ALICE), 60);

		// Once the transport has sent some, there is room again.
		accumulate(2, 1);
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
	});
}

#[test]
fn the_unsettled_operations_are_bounded() {
	ready(|| {
		released_to_service(100);
		assert_eq!(deliver(vec![return_from(SERVICE, 50, [0; 128], false)]), Ok(()));
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		for _ in 0..DEFAULT_TRANSPORT_CAPACITY {
			assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		}
		accumulate_all(2);
		for _ in DEFAULT_TRANSPORT_CAPACITY..MAX_USER_OPERATIONS as usize {
			assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		}
		// The user bound is reached.
		assert_noop!(
			Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO),
			Error::<Test>::TooManyUserOperations
		);
		assert_eq!(crate::UserOperations::<Test>::get(), MAX_USER_OPERATIONS);

		// Refunds fill the rest of the total bound.
		for _ in MAX_USER_OPERATIONS..MAX_OPERATIONS {
			assert_ok!(Jamkb::dispose_excess(RuntimeOrigin::root(), SERVICE, 10, true, MEMO));
		}
		assert_noop!(
			Jamkb::dispose_excess(RuntimeOrigin::root(), SERVICE, 10, true, MEMO),
			Error::<Test>::TooManyOperations
		);

		accumulate_all(3);
		poll();
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
	});
}

#[test]
fn cheap_redemptions_cannot_starve_system_operations() {
	ready(|| {
		released_to_service(100);
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		for _ in 0..DEFAULT_TRANSPORT_CAPACITY {
			assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		}
		accumulate_all(2);
		for _ in DEFAULT_TRANSPORT_CAPACITY..MAX_USER_OPERATIONS as usize {
			assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		}
		assert_noop!(
			Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO),
			Error::<Test>::TooManyUserOperations
		);

		// A supervisor bucket's `Move` still has room.
		assert_eq!(deliver(vec![return_from(SERVICE, 30, memo_for(BOB), true)]), Ok(()));
		let mv = sent().last().expect("the move is queued").id;
		assert!(matches!(
			Jamkb::operation(mv).unwrap().payload,
			crate::OperationPayload::Move { .. }
		));
		assert_eq!(crate::UserOperations::<Test>::get(), MAX_USER_OPERATIONS);
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
				Event::OperationRequested { id: redemption, allocation: None },
				Event::ReleaseApproved { id, target: SERVICE, amount: 100 },
				Event::OperationRequested { id: release, allocation: Some(id) },
				Event::OperationFailed {
					id: redemption,
					error: FailureReason::TransferFailed(TransferError::UnknownDestination),
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
		Jam::set_service(OTHER, JamService { min_memo_gas: MEMO_GAS + 1, ..Default::default() });
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 500));

		// Block 1: two releases and a redemption; the one to `OTHER` is refused for its gas.
		let (to_service, _) = release_to(SERVICE, 100);
		let (to_other, _) = release_to(OTHER, 70);
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 40, SERVICE, MEMO));
		accumulate_all(1);
		poll();

		// Block 2: one redemption to a service that does not exist, one that lands.
		run_to_timeslot(2);
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 30, UNKNOWN_SERVICE, MEMO));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 20, SERVICE, MEMO));
		accumulate_all(2);
		poll();

		assert_eq!(Jamkb::allocation(to_service).unwrap().state, AllocationState::Delivered);
		assert_eq!(Jamkb::allocation(to_other).unwrap().state, AllocationState::Approved);
		assert_eq!(Jam::balance(SERVICE), 100 + 40 + 20);
		assert_eq!(Jam::balance(OTHER), 0);
		assert_eq!(free(custody()), CAP - 500 - 100);
		assert_eq!(free(ALICE), 500 - 40 - 20);
		assert_eq!(supply(), CAP - 160);
		assert!(listed().is_empty());
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
		<Jamkb as OnJamOutcome>::on_outcome(
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
			settlement_overhead()
				.saturating_add(settling(<() as WeightInfo>::settle()).saturating_mul(2))
		);
		assert_eq!(poll(), reads);
	});
}

#[test]
fn on_poll_charges_a_move_by_its_bucket_size() {
	ready(|| {
		released_to_service(100);
		let transfers = (0..TRANSFERS_PER_BUCKET)
			.map(|_| return_from(SERVICE, 10, memo_for(BOB), true))
			.collect();
		assert_eq!(deliver(transfers), Ok(()));
		accumulate_all(2);

		assert_eq!(
			poll(),
			settlement_overhead()
				.saturating_add(settling(<() as WeightInfo>::settle_move(TRANSFERS_PER_BUCKET)))
		);
		assert_eq!(free(BOB), 10 * TRANSFERS_PER_BUCKET as Balance);
		assert_books_match_jam();
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
		let one = settlement_overhead().saturating_add(settling(<() as WeightInfo>::settle()));
		assert_eq!(poll_within(one), one);
		assert!(Jamkb::operation(op(1)).is_none());
		assert_eq!(listed(), vec![op(2), op(3)]);

		// Not even the reads fit: nothing is read, nothing settles.
		let reads = MockDbWeight::get().reads(1);
		assert_eq!(poll_within(reads - Weight::from_parts(1, 0)), Weight::zero());
		// The reads fit, the writes do not: the lists are read and left as they are.
		assert_eq!(poll_within(settlement_overhead() - Weight::from_parts(1, 0)), reads);
		// Only the overhead fits: the lists are read, nothing settles.
		assert_eq!(poll_within(settlement_overhead()), settlement_overhead());
		assert_eq!(listed(), vec![op(2), op(3)]);
	});
}

#[test]
fn a_blocked_target_does_not_use_the_settlement_budget() {
	ready(|| {
		const OTHER: ServiceId = 7;
		Jam::set_service(OTHER, JamService::default());
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		let releases: Vec<u64> =
			(0..SETTLEMENTS_PER_BLOCK).map(|_| release_to(SERVICE, 10).1).collect();
		fail_next(1, TransferError::GasBelowDestinationMinimum);
		accumulate_all(1);

		// The first release's settlement fails: the supply has no room for its units. The other
		// releases to the same target wait and are not tried.
		let filled = fill_supply();
		poll();
		assert!(listed().is_empty());
		let failures = System::events()
			.iter()
			.filter(|r| matches!(r.event, RuntimeEvent::Jamkb(Event::SettlementFailed { .. })))
			.count();
		assert_eq!(failures, 1);
		assert_eq!(crate::Blocked::<Test>::get(SERVICE), Some(releases[0]));

		// A redemption to the blocked target resolved later waits too; one to another target
		// settles.
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		let waiting = sent().last().unwrap().id;
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, OTHER, MEMO));
		let other = sent().last().unwrap().id;
		accumulate_all(2);
		assert_eq!(listed(), vec![other]);
		poll();
		assert!(Jamkb::operation(other).is_none());
		assert!(Jamkb::operation(waiting).is_some());

		// Once the cause is gone, `settle` settles the failed one and `on_poll` the rest.
		drain_supply(filled);
		assert_ok!(Jamkb::settle(RuntimeOrigin::signed(BOB), releases[0]));
		poll();
		assert!(Jamkb::operation(waiting).is_none());
		assert!(listed().is_empty());
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

// ── Voluntary returns ──────────────────────────────────────────────────────

/// Releases `amount` to `SERVICE` and settles it, so that `SERVICE` holds units to return.
fn released_to_service(amount: Balance) {
	release_to(SERVICE, amount);
	accumulate_all(1);
	poll();
}

fn unclaimed_of(who: AccountId) -> Balance {
	crate::Unclaimed::<Test>::get(who)
}

fn excess_of(source: ServiceId) -> Balance {
	crate::Excess::<Test>::get(source)
}

#[test]
fn buckets_are_taken_only_while_attested_and_not_paused() {
	ready(|| {
		assert_eq!(<Jamkb as OnIncomingTransfers<Balance>>::capacity(), BUCKETS_PER_BLOCK);
		assert_ok!(Jamkb::pause(RuntimeOrigin::root()));
		assert_eq!(<Jamkb as OnIncomingTransfers<Balance>>::capacity(), 0);
		assert_eq!(
			<Jamkb as OnIncomingTransfers<Balance>>::on_incoming_transfers(
				bucket(vec![]),
				&mut WeightMeter::new()
			),
			Err(())
		);
		assert_ok!(Jamkb::resume(RuntimeOrigin::root()));
		assert_eq!(<Jamkb as OnIncomingTransfers<Balance>>::capacity(), BUCKETS_PER_BLOCK);
	});
}

#[test]
fn a_return_naming_a_beneficiary_is_minted_to_it() {
	ready(|| {
		released_to_service(100);
		let transfer = return_from(SERVICE, 60, memo_for(ALICE), false);
		assert_eq!(deliver(vec![transfer]), Ok(()));

		assert_eq!(free(ALICE), 60);
		assert_eq!(supply(), CAP - 40);
		assert_eq!(unclaimed_of(ALICE), 0);
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::ReturnBooked {
				source: SERVICE,
				amount: 60,
				class: ReturnClass::Credited(ALICE),
				memo: memo_for(ALICE),
			})
		);
		assert_noop!(
			Jamkb::claim(RuntimeOrigin::signed(BOB), ALICE),
			Error::<Test>::NothingToClaim
		);
		assert_books_match_jam();
	});
}

#[test]
fn a_return_without_a_valid_memo_is_booked_as_excess() {
	ready(|| {
		released_to_service(100);
		let mut malformed = memo_for(ALICE);
		malformed[100] = 1;
		let bucket = vec![
			return_from(SERVICE, 10, [0; 128], false),
			return_from(SERVICE, 20, malformed, false),
			return_from(SERVICE, 30, MEMO, false),
		];
		assert_eq!(deliver(bucket), Ok(()));

		assert_eq!(excess_of(SERVICE), 60);
		assert_eq!(supply(), CAP - 100);
		assert_eq!(free(ALICE), 0);
		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Jamkb(Event::ReturnBooked { amount: 20, class: ReturnClass::Excess, .. })
		)));
		assert_books_match_jam();
	});
}

#[test]
fn a_bucket_whose_mint_fails_is_refused_whole() {
	ready(|| {
		released_to_service(100);
		let bucket = vec![
			return_from(SERVICE, 10, [0; 128], false),
			return_from(SERVICE, 20, memo_for(ALICE), false),
		];
		let filled = fill_supply();
		assert_eq!(deliver(bucket.clone()), Err(()));
		assert_eq!(excess_of(SERVICE), 0);

		// Delivered again once the mint fits.
		drain_supply(filled);
		assert_eq!(deliver(bucket), Ok(()));
		assert_eq!(excess_of(SERVICE), 10);
		assert_eq!(free(ALICE), 20);
		assert_books_match_jam();
	});
}

#[test]
fn transfers_to_the_supervisor_balance_are_moved_then_booked() {
	ready(|| {
		released_to_service(100);
		let bucket = vec![
			return_from(SERVICE, 30, memo_for(BOB), true),
			return_from(SERVICE, 10, memo_for(ALICE), false),
			return_from(SERVICE, 20, [0; 128], true),
		];
		assert_eq!(Jam::supervisor_balance(PARACHAIN_SERVICE), 50);
		let fees_before = native(ALICE) + native(BOB);
		assert_eq!(deliver(bucket), Ok(()));

		// The transfer to the regular balance is booked at once; the others wait for the move.
		assert_eq!(free(ALICE), 10);
		assert_eq!(free(BOB), 0);
		assert_eq!(excess_of(SERVICE), 0);
		let queued = sent();
		assert_eq!(queued.len(), 1);
		assert_eq!(
			queued[0].messages,
			vec![UpwardMessage::TransferOut {
				source: None,
				dest: PARACHAIN_SERVICE,
				amount: 50,
				id: last_message_id(),
				source_supervisor_balance: true,
				dest_supervisor_balance: false,
				deferred: None,
			}]
		);
		let mv = queued[0].id;
		assert_eq!(Jamkb::operation(mv).unwrap().target, PARACHAIN_SERVICE);
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::OperationRequested { id: mv, allocation: None })
		);
		// A move pays no fee.
		assert_eq!(native(ALICE) + native(BOB), fees_before);

		accumulate_all(2);
		assert_eq!(Jam::supervisor_balance(PARACHAIN_SERVICE), 0);
		poll();

		assert_eq!(free(BOB), 30);
		assert_eq!(excess_of(SERVICE), 20);
		assert_books_match_jam();
	});
}

/// Delivers one transfer of 30 to the supervisor balance naming `BOB`, and fails its `Move`.
/// Returns the transfer and the id of the failed `Move`.
fn failed_move() -> (crate::transport::IncomingTransfer<Balance>, u64) {
	released_to_service(100);
	let transfer = return_from(SERVICE, 30, memo_for(BOB), true);
	assert_eq!(deliver(vec![transfer.clone()]), Ok(()));
	let mv = fail_next(2, TransferError::InsufficientServiceBalance);
	poll();
	(transfer, mv)
}

#[test]
fn a_failed_move_keeps_its_bucket_for_retry() {
	ready(|| {
		let (transfer, mv) = failed_move();

		assert_eq!(crate::Unmoved::<Test>::get(mv).unwrap().into_inner(), vec![transfer]);
		assert!(jamkb_events().contains(&Event::MoveFailed { id: mv, amount: 30 }));
		assert_eq!(free(BOB), 0);
		// The units stay on the supervisor balance, outside the supply.
		assert_eq!(Jam::supervisor_balance(PARACHAIN_SERVICE), 30);
		assert_books_match_jam();
	});
}

#[test]
fn retry_move_resends_the_bucket_and_books_it_on_confirmation() {
	ready(|| {
		let (_, mv) = failed_move();

		assert_ok!(Jamkb::retry_move(RuntimeOrigin::root(), mv));
		let retry = sent().last().expect("the retry is queued").id;
		assert_ne!(retry, mv);
		assert!(matches!(
			sent()[0].messages[0],
			UpwardMessage::TransferOut {
				dest: PARACHAIN_SERVICE,
				amount: 30,
				source_supervisor_balance: true,
				..
			}
		));
		assert_eq!(crate::Unmoved::<Test>::iter().count(), 0);

		accumulate_all(3);
		poll();
		assert_eq!(free(BOB), 30);
		assert_eq!(Jam::supervisor_balance(PARACHAIN_SERVICE), 0);
		assert_books_match_jam();
	});
}

#[test]
fn retry_move_rejects_an_unknown_id_and_pauses() {
	ready(|| {
		assert_noop!(Jamkb::retry_move(RuntimeOrigin::root(), 999), Error::<Test>::UnknownMove);
		let (_, mv) = failed_move();
		assert_ok!(Jamkb::pause(RuntimeOrigin::root()));
		assert_noop!(Jamkb::retry_move(RuntimeOrigin::root(), mv), Error::<Test>::Paused);
		assert!(crate::Unmoved::<Test>::contains_key(mv));
	});
}

#[test]
fn retry_move_requires_governance() {
	ready(|| {
		let (_, mv) = failed_move();
		assert_noop!(
			Jamkb::retry_move(RuntimeOrigin::signed(ALICE), mv),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}

#[test]
fn a_bucket_whose_move_cannot_be_recorded_is_refused_whole() {
	ready(|| {
		released_to_service(100);
		let bucket = vec![
			crate::transport::IncomingTransfer {
				source: SERVICE,
				amount: 10,
				to_supervisor_balance: false,
				memo: memo_for(ALICE),
			},
			crate::transport::IncomingTransfer {
				source: SERVICE,
				amount: 20,
				to_supervisor_balance: true,
				memo: memo_for(ALICE),
			},
		];
		SendFailure::set(Some(MessageSendError::QueueFull));
		assert_eq!(deliver(bucket.clone()), Err(()));
		assert_eq!(free(ALICE), 0);

		assert!(sent().is_empty());

		SendFailure::set(None);
		assert_eq!(deliver(bucket), Ok(()));
		assert_eq!(free(ALICE), 10);
	});
}

#[test]
fn a_full_bucket_is_booked_and_weighed_by_its_size() {
	ready(|| {
		released_to_service(100);
		let full: Vec<_> = (0..TRANSFERS_PER_BUCKET)
			.map(|_| return_from(SERVICE, 10, memo_for(ALICE), false))
			.collect();
		let mut meter = WeightMeter::new();
		assert_eq!(
			<Jamkb as OnIncomingTransfers<Balance>>::on_incoming_transfers(
				bucket(full),
				&mut meter
			),
			Ok(())
		);
		assert_eq!(
			meter.consumed(),
			<() as WeightInfo>::on_incoming_transfers(TRANSFERS_PER_BUCKET)
		);
		assert_eq!(free(ALICE), 10 * TRANSFERS_PER_BUCKET as Balance);
		assert_books_match_jam();
	});
}

#[test]
fn a_return_to_an_account_that_cannot_hold_jamkb_is_booked_until_claimed() {
	ready(|| {
		use frame_support::traits::fungible::Mutate as _;
		released_to_service(100);
		assert_eq!(deliver(vec![return_from(SERVICE, 25, memo_for(NOBODY), false)]), Ok(()));
		assert_eq!(free(NOBODY), 0);
		assert_eq!(unclaimed_of(NOBODY), 25);
		assert!(
			jamkb_events().contains(&Event::UnclaimedBooked { beneficiary: NOBODY, amount: 25 })
		);
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::ReturnBooked {
				source: SERVICE,
				amount: 25,
				class: ReturnClass::Unclaimed(NOBODY),
				memo: memo_for(NOBODY),
			})
		);
		assert_books_match_jam();

		assert_noop!(Jamkb::claim(RuntimeOrigin::signed(BOB), NOBODY), Error::<Test>::CannotCreate);
		assert_eq!(unclaimed_of(NOBODY), 25);

		assert_ok!(Balances::mint_into(&NOBODY, 10));
		// Anyone may claim it for the beneficiary.
		assert_ok!(Jamkb::claim(RuntimeOrigin::signed(BOB), NOBODY));
		assert_eq!(free(NOBODY), 25);
		assert_eq!(unclaimed_of(NOBODY), 0);
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::Claimed { beneficiary: NOBODY, amount: 25 })
		);
		assert_noop!(
			Jamkb::claim(RuntimeOrigin::signed(BOB), NOBODY),
			Error::<Test>::NothingToClaim
		);
		assert_books_match_jam();
	});
}

#[test]
fn claim_rejects_a_mint_that_would_overflow_the_supply() {
	ready(|| {
		use frame_support::traits::fungible::Mutate as _;
		released_to_service(100);
		assert_eq!(deliver(vec![return_from(SERVICE, 25, memo_for(NOBODY), false)]), Ok(()));
		assert_ok!(Balances::mint_into(&NOBODY, 10));

		let filled = fill_supply();
		assert_noop!(Jamkb::claim(RuntimeOrigin::signed(BOB), NOBODY), ArithmeticError::Overflow);
		assert_eq!(unclaimed_of(NOBODY), 25);

		drain_supply(filled);
		assert_books_match_jam();
	});
}

#[test]
fn a_return_to_a_blocked_account_is_booked_unclaimed() {
	ready(|| {
		released_to_service(100);
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 10));
		// Custody holds the freezer role of the asset.
		assert_ok!(Assets::block(RuntimeOrigin::signed(custody()), JAMKB, ALICE));

		assert_eq!(deliver(vec![return_from(SERVICE, 25, memo_for(ALICE), false)]), Ok(()));

		assert_eq!(free(ALICE), 10);
		assert_eq!(unclaimed_of(ALICE), 25);
		let booked = Event::UnclaimedBooked { beneficiary: ALICE, amount: 25 };
		assert!(jamkb_events().contains(&booked));
		assert_books_match_jam();
	});
}

#[test]
fn a_return_that_would_overflow_a_book_is_refused() {
	ready(|| {
		released_to_service(100);
		crate::Excess::<Test>::insert(SERVICE, Balance::MAX);
		let supply_before = supply();

		assert_eq!(deliver(vec![return_from(SERVICE, 25, [0; 128], false)]), Err(()));

		assert_eq!(excess_of(SERVICE), Balance::MAX);
		assert_eq!(supply(), supply_before);
		assert!(!jamkb_events().iter().any(|e| matches!(e, Event::ReturnBooked { .. })));
	});
}

#[test]
fn dispose_excess_mints_to_custody_or_refunds_the_source() {
	ready(|| {
		released_to_service(100);
		assert_eq!(deliver(vec![return_from(SERVICE, 50, [0; 128], false)]), Ok(()));
		assert_eq!(excess_of(SERVICE), 50);

		assert_noop!(
			Jamkb::dispose_excess(RuntimeOrigin::signed(ALICE), SERVICE, 10, false, MEMO),
			sp_runtime::DispatchError::BadOrigin
		);
		assert_noop!(
			Jamkb::dispose_excess(RuntimeOrigin::root(), SERVICE, 51, false, MEMO),
			Error::<Test>::InsufficientExcess
		);
		assert_noop!(
			Jamkb::dispose_excess(RuntimeOrigin::root(), SERVICE, MIN_TRANSFER - 1, true, MEMO),
			Error::<Test>::BelowMinTransfer
		);

		// Back to custody.
		assert_ok!(Jamkb::dispose_excess(RuntimeOrigin::root(), SERVICE, 20, false, MEMO));
		assert_eq!(excess_of(SERVICE), 30);
		assert_eq!(free(custody()), CAP - 100 + 20);
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::ExcessDisposed { source: SERVICE, amount: 20 })
		);
		assert_books_match_jam();

		// Refunded to the source, without a fee.
		let fees_before = native(ALICE) + native(BOB);
		assert_ok!(Jamkb::dispose_excess(RuntimeOrigin::root(), SERVICE, 20, true, MEMO));
		assert_eq!(excess_of(SERVICE), 10);
		assert!(matches!(
			sent()[0].messages[0],
			UpwardMessage::TransferOut {
				dest: SERVICE,
				amount: 20,
				deferred: Some((MEMO, MEMO_GAS)),
				..
			}
		));
		assert_eq!(native(ALICE) + native(BOB), fees_before);
		accumulate_all(2);
		poll();
		assert_eq!(excess_of(SERVICE), 10);
		assert_eq!(Jam::balance(SERVICE), 50 + 20);
		assert_books_match_jam();

		// The last of it can be refunded too.
		assert_ok!(Jamkb::dispose_excess(RuntimeOrigin::root(), SERVICE, 10, true, MEMO));
		assert_eq!(excess_of(SERVICE), 0);
		accumulate_all(3);
		poll();
		assert_books_match_jam();
	});
}

#[test]
fn a_failed_refund_restores_the_excess() {
	ready(|| {
		released_to_service(100);
		assert_eq!(deliver(vec![return_from(SERVICE, 50, [0; 128], false)]), Ok(()));
		Jam::set_service(
			SERVICE,
			JamService { min_memo_gas: MEMO_GAS + 1, ..Jam::service(SERVICE).unwrap() },
		);
		assert_ok!(Jamkb::dispose_excess(RuntimeOrigin::root(), SERVICE, 20, true, MEMO));
		accumulate_all(2);
		poll();

		assert_eq!(excess_of(SERVICE), 50);
		assert_books_match_jam();
	});
}

// ── Corrections ────────────────────────────────────────────────────────────

#[test]
fn force_mint_corrects_the_supply_after_an_unrecorded_return() {
	ready(|| {
		released_to_service(100);
		// `SERVICE` returned 40 on JAM that the Parachain Service did not record.
		Jam::set_balance(SERVICE, 60);
		Jam::set_balance(PARACHAIN_SERVICE, Jam::balance(PARACHAIN_SERVICE) + 40);

		assert_noop!(
			Jamkb::force_mint(RuntimeOrigin::signed(ALICE), ANCHOR, ALICE, 40),
			sp_runtime::DispatchError::BadOrigin
		);
		assert_ok!(Jamkb::force_mint(RuntimeOrigin::root(), ANCHOR, ALICE, 30));
		assert_eq!(free(ALICE), 30);
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::ForceMinted {
				anchor: ANCHOR,
				beneficiary: ALICE,
				amount: 30
			})
		);

		// A beneficiary that cannot receive the mint gets it booked.
		assert_ok!(Jamkb::force_mint(RuntimeOrigin::root(), ANCHOR, NOBODY, 10));
		assert_eq!(unclaimed_of(NOBODY), 10);
		assert_books_match_jam();
	});
}

#[test]
fn force_burn_corrects_the_supply_after_an_unrecorded_outflow() {
	ready(|| {
		// 40 left the Parachain Service on JAM without a record on Asset Hub.
		Jam::set_balance(PARACHAIN_SERVICE, CAP - 40);

		assert_noop!(
			Jamkb::force_burn(RuntimeOrigin::signed(ALICE), ANCHOR, 40),
			sp_runtime::DispatchError::BadOrigin
		);
		assert_noop!(
			Jamkb::force_burn(RuntimeOrigin::root(), ANCHOR, CAP + 1),
			TokenError::FundsUnavailable
		);

		assert_ok!(Jamkb::force_burn(RuntimeOrigin::root(), ANCHOR, 40));
		assert_eq!(free(custody()), CAP - 40);
		assert_eq!(supply(), CAP - 40);
		assert_eq!(
			last_event(),
			RuntimeEvent::Jamkb(Event::ForceBurned { anchor: ANCHOR, amount: 40 })
		);
		assert_books_match_jam();
	});
}

#[test]
fn governance_resolves_an_operation_the_transport_never_reported() {
	ready(|| {
		run_to_timeslot(4);
		let (id, op) = release_to(SERVICE, 100);

		assert_ok!(Jamkb::force_resolve(RuntimeOrigin::root(), op, None));
		let operation = Jamkb::operation(op).unwrap();
		assert_eq!(operation.state, OperationState::Confirmed(None));
		assert_eq!(operation.accumulated_at, Some(4));
		assert_eq!(listed(), vec![op]);
		let events = jamkb_events();
		assert_eq!(
			events[events.len() - 2..],
			[Event::OperationConfirmed { id: op, entry: None }, Event::ForceResolved { id: op }]
		);

		poll();
		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Delivered);
		assert!(Jamkb::operation(op).is_none());

		// The transport reports it late: the report is ignored.
		accumulate_all(5);
		assert!(Jamkb::operation(op).is_none());
		assert_eq!(Jamkb::allocation(id).unwrap().delivered_at, Some(4));
		assert_books_match_jam();
	});
}

#[test]
fn force_resolve_only_takes_the_earliest_requested_operation() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 20, SERVICE, MEMO));
		let failure = FailureReason::TransferFailed(TransferError::UnknownDestination);

		assert_noop!(
			Jamkb::force_resolve(RuntimeOrigin::root(), op(2), None),
			Error::<Test>::OutOfOrder
		);
		assert_noop!(
			Jamkb::force_resolve(RuntimeOrigin::root(), 999, None),
			Error::<Test>::UnknownOperation
		);
		assert_ok!(Jamkb::force_resolve(RuntimeOrigin::root(), op(1), Some(failure)));
		assert_eq!(Jamkb::operation(op(1)).unwrap().state, OperationState::Failed(failure));
		// A resolved operation is not taken again.
		assert_noop!(
			Jamkb::force_resolve(RuntimeOrigin::root(), op(1), None),
			Error::<Test>::OutOfOrder
		);
		assert_ok!(Jamkb::force_resolve(RuntimeOrigin::root(), op(2), None));
		assert_eq!(listed(), vec![op(1), op(2)]);
	});
}

#[test]
fn force_resolve_requires_governance() {
	ready(|| {
		let (_, op) = release_to(SERVICE, 100);
		assert_noop!(
			Jamkb::force_resolve(RuntimeOrigin::signed(ALICE), op, None),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}

// ── Delivery fee ───────────────────────────────────────────────────────────

#[test]
fn signed_calls_that_record_an_operation_pay_the_delivery_fee() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		release_to(SERVICE, 100);
		assert_eq!(native(BOB), NATIVE - FEE_PER_MESSAGE);
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		assert_eq!(native(ALICE), NATIVE - FEE_PER_MESSAGE);

		// A caller that cannot pay the fee cannot record the operation.
		assert_ok!(Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 10, MEMO, None, 100));
		let id = crate::LastAllocationId::<Test>::get();
		assert_noop!(
			Jamkb::execute_release(RuntimeOrigin::signed(NOBODY), id),
			TokenError::FundsUnavailable
		);
	});
}

// ── Ids, errors and edge cases ─────────────────────────────────────────────

#[test]
fn an_entry_carrying_the_operation_id_instead_of_the_message_id_is_not_a_failure() {
	ready(|| {
		let (_, op) = release_to(SERVICE, 100);
		assert_ne!(op, last_message_id());
		let wrong = [AccumulateLog::TransferFailed { id: op, error: TransferError::UnknownSource }];
		<Jamkb as OnJamOutcome>::on_outcome(JAMKB_SENDER, op, 1, &wrong, &mut WeightMeter::new());
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
			(MessageSendError::TooBig, DispatchError::from(Error::<Test>::TransportTooBig)),
			(MessageSendError::Conflict, Error::<Test>::TransportConflict.into()),
			(MessageSendError::QueueFull, Error::<Test>::TransportQueueFull.into()),
		] {
			SendFailure::set(Some(refusal));
			assert_noop!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO), error);
			assert_noop!(Jamkb::execute_release(RuntimeOrigin::signed(BOB), 1), error);
		}
		assert_eq!(last_message_id(), MESSAGE_ID_BASE);
		assert!(sent().is_empty());
		assert_eq!(free(ALICE), 100);
		assert_eq!(free(custody()), CAP - 100);
		assert_eq!(Jamkb::allocation(1).unwrap().state, AllocationState::Approved);

		SendFailure::set(None);
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
	});
}

#[test]
fn failed_outcomes_are_undone_by_on_poll() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 40, SERVICE, MEMO));
		let (id, _) = release_to(SERVICE, 100);
		let redemption = fail_next(1, TransferError::GasBelowDestinationMinimum);
		let release = fail_next(1, TransferError::InsufficientServiceBalance);

		assert_eq!(listed(), vec![redemption, release]);
		poll();
		assert_eq!(free(ALICE), 100);
		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Approved);
		assert!(crate::PendingOperationOf::<Test>::get(id).is_none());
		assert_eq!(free(custody()), CAP - 100);
		assert_eq!(supply(), CAP);
		assert!(listed().is_empty());
		assert_books_match_jam();
	});
}

#[test]
fn settle_retries_a_failed_settlement_until_its_cause_is_gone() {
	ready(|| {
		let (id, op) = release_to(SERVICE, 100);
		fail_next(1, TransferError::GasBelowDestinationMinimum);
		let filled = fill_supply();
		poll();
		assert!(listed().is_empty());

		assert_noop!(Jamkb::settle(RuntimeOrigin::signed(BOB), op), ArithmeticError::Overflow);
		assert!(Jamkb::operation(op).is_some());

		drain_supply(filled);
		assert_ok!(Jamkb::settle(RuntimeOrigin::signed(BOB), op));
		assert!(Jamkb::operation(op).is_none());
		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Approved);
		assert_eq!(free(custody()), CAP);
	});
}

#[test]
fn attest_rejects_more_than_the_cap() {
	build_and_execute(|| {
		assert_ok!(Jamkb::initialize(RuntimeOrigin::root()));
		assert_noop!(
			Jamkb::attest(RuntimeOrigin::root(), ANCHOR, CAP + 1),
			ArithmeticError::Underflow
		);
		assert!(crate::Attestation::<Test>::get().is_none());

		// Everything outside the Parachain Service: nothing is minted, nothing can be released.
		Jam::set_balance(PARACHAIN_SERVICE, 0);
		Jam::set_balance(SERVICE, CAP);
		assert_ok!(Jamkb::attest(RuntimeOrigin::root(), ANCHOR, CAP));
		assert_eq!(supply(), 0);
		assert_ok!(Jamkb::approve_release(RuntimeOrigin::root(), SERVICE, 10, MEMO, None, 10));
		assert_noop!(
			Jamkb::execute_release(RuntimeOrigin::signed(BOB), 1),
			Error::<Test>::InsufficientBalance
		);
		assert_books_match_jam();
	});
}

#[test]
fn a_burn_takes_at_most_the_whole_balance() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 60, SERVICE, MEMO));
		assert_noop!(
			Jamkb::redeem(RuntimeOrigin::signed(ALICE), 41, SERVICE, MEMO),
			Error::<Test>::InsufficientBalance
		);
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 40, SERVICE, MEMO));
		assert_eq!(free(ALICE), 0);
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
			sp_runtime::DispatchError::BadOrigin
		);
		run_to_timeslot(101);
		assert_ok!(Jamkb::cancel_allocation(RuntimeOrigin::signed(BOB), id));

		assert_eq!(Jamkb::allocation(id).unwrap().state, AllocationState::Closed);
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
		assert_eq!(listed(), vec![first, second, third]);

		poll();
		assert_eq!(supply(), CAP - 40);
		assert_eq!(free(ALICE), 60);
		assert_eq!(Jam::balance(SERVICE), 40);
		assert_books_match_jam();
	});
}

#[test]
fn settle_rejects_an_operation_behind_an_earlier_one_of_its_target() {
	ready(|| {
		assert_ok!(Jamkb::grant(RuntimeOrigin::root(), ALICE, 100));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 10, SERVICE, MEMO));
		assert_ok!(Jamkb::redeem(RuntimeOrigin::signed(ALICE), 20, SERVICE, MEMO));
		accumulate_all(1);

		assert_noop!(Jamkb::settle(RuntimeOrigin::signed(BOB), op(2)), Error::<Test>::OutOfOrder);
		poll();
		assert_eq!(supply(), CAP - 30);
		assert_books_match_jam();
	});
}
