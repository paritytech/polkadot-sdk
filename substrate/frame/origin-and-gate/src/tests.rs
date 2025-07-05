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

use super::*;
use crate::{self as pallet_origin_and_gate, mock::OriginId};
use assert_matches::assert_matches;
use frame_support::{assert_err, assert_noop, assert_ok};
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, Hash, IdentityLookup},
	BuildStorage, DispatchError,
};

// Import mock directly instead of through module import
#[path = "./mock.rs"]
mod mock;
pub use mock::*;

/// Helper function to create remark call for use with testing
fn make_remark_call(text: &str) -> Result<Box<<Test as Config>::RuntimeCall>, &'static str> {
	// Try parse text as u64
	let value = match text.parse::<u64>() {
		Ok(v) => v,
		Err(_) => return Err("Failed to parse input as u64"),
	};

	let remark = self::Call::<Test>::set_dummy { new_value: value };
	Ok(Box::new(RuntimeCall::OriginAndGate(remark)))
}

/// Unit tests that focus on testing individual pallet functions in isolation
/// rather than complex workflows covered by integration tests that are in ../tests
mod unit_test {
	use super::*;

	/// Helper function to create dummy call for use with testing
	fn create_dummy_call(value: u64) -> Box<<Test as Config>::RuntimeCall> {
		let call = Call::<Test>::set_dummy { new_value: value };
		Box::new(RuntimeCall::OriginAndGate(call))
	}

	// TODO: Organise tests into further submodules for each pallet function
	// (e.g. propose, add_approval, cancel_proposal, withdraw_approval, set_dummy)

	#[test]
	fn set_dummy_works() {
		new_test_ext().execute_with(|| {
			// Check initial value is None
			assert_eq!(Dummy::<Test>::get(), None);

			// Set dummy value
			assert_ok!(OriginAndGate::set_dummy(RuntimeOrigin::root(), 1000));

			// Check value set
			assert_eq!(Dummy::<Test>::get(), Some(1000));

			// Set new value
			assert_ok!(OriginAndGate::set_dummy(RuntimeOrigin::root(), 100));

			// Check value updated
			assert_eq!(Dummy::<Test>::get(), Some(100));
		});
	}

	#[test]
	fn set_dummy_privileged_call_fails_with_non_root_origin() {
		new_test_ext().execute_with(|| {
			// Attempt to set with signed origin should fail
			assert_noop!(
				OriginAndGate::set_dummy(RuntimeOrigin::signed(1), 1000),
				DispatchError::BadOrigin
			);
		});
	}

	#[test]
	fn propose_a_proposal_creates_new_proposal() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);

			// Create call
			let call = create_dummy_call(1000);
			let call_hash = <<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

			// Propose using Alice's origin
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call,
				ALICE_ORIGIN_ID,
				None,
			));

			// Verify proposal stored
			let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.status, ProposalStatus::Pending);
			assert_eq!(proposal.approvals.len(), 1);
			assert_eq!(proposal.approvals[0], ALICE_ORIGIN_ID);

			// Verify event emitted
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCreated {
				proposal_hash: call_hash,
				origin_id: ALICE_ORIGIN_ID,
			}));
		});
	}

	#[test]
	fn duplicate_proposals_create_unique_entries_with_warning() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);

			// Create identical call parameters
			let call = make_remark_call("1000").unwrap();
			let call_hash = <Test as Config>::Hashing::hash_of(&call);
			let origin_id = ALICE_ORIGIN_ID;
			let expiry = None;

			// First proposal should succeed
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call.clone(),
				origin_id,
				expiry,
			));

			// Second identical proposal should fail with ProposalAlreadyExists
			// assert_noop!(
			// 	OriginAndGate::propose(
			// 		RuntimeOrigin::signed(ALICE),
			// 		call.clone(),
			// 		origin_id,
			// 		expiry,
			// 	),
			// 	Error::<Test>::ProposalAlreadyExists
			// );

			// Even from a different user with same parameters
			// assert_noop!(
			// 	OriginAndGate::propose(
			// 		RuntimeOrigin::signed(BOB),
			// 		call.clone(),
			// 		origin_id,
			// 		expiry,
			// 	),
			// 	Error::<Test>::ProposalAlreadyExists
			// );

			// TODO - we don't want it to fail, we want it to
			// provide the user with a warning of what duplicate was
			// detected, but still create a new unique proposal entry
			// and emit associated events.
			// because if there's a chance of a hash collision we
			// must handle that since proposals stored as hashes with
			// proposal_hash
		});
	}

	#[test]
	fn approve_of_proposal_adds_approval() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);

			// Create call
			let call = create_dummy_call(1000);
			let call_hash = <<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

			// Propose using Alice's origin
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call,
				ALICE_ORIGIN_ID,
				None,
			));

			// Approve using Bob's origin
			assert_ok!(OriginAndGate::add_approval(
				RuntimeOrigin::signed(BOB),
				call_hash,
				ALICE_ORIGIN_ID,
				BOB_ORIGIN_ID,
			));

			// Verify approval added
			let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.approvals.len(), MaxApprovals::get() as usize);
			assert!(proposal.approvals.contains(&BOB_ORIGIN_ID));

			// Verify event emitted
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::OriginApprovalAdded {
				proposal_hash: call_hash,
				origin_id: ALICE_ORIGIN_ID,
				approving_origin_id: BOB_ORIGIN_ID,
			}));
		});
	}

	#[test]
	fn duplicate_approval_of_proposal_fails() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);

			// Create call
			let call = create_dummy_call(42);
			let call_hash = <<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

			// Propose using Alice's origin
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call,
				ALICE_ORIGIN_ID,
				None,
			));

			// Try approve again with same origin
			assert_noop!(
				OriginAndGate::add_approval(
					RuntimeOrigin::signed(ALICE),
					call_hash,
					ALICE_ORIGIN_ID,
					ALICE_ORIGIN_ID,
				),
				Error::<Test>::OriginAlreadyApproved
			);
		});
	}

	#[test]
	fn approve_non_existent_proposal_fails() {
		new_test_ext().execute_with(|| {
			// Create non-existent call hash
			let call_hash = H256::repeat_byte(0xab);

			// Try approve non-existent proposal
			assert_noop!(
				OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					call_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
				),
				Error::<Test>::ProposalNotFound
			);
		});
	}

	#[test]
	fn and_gate_requires_two_origins_not_individual() {
		new_test_ext().execute_with(|| {
			// Direct use of AndGate should fail for any single origin
			// to represent real-world scenario where single account
			// cannot simultaneously satisfy multiple origin requirements
			assert!(AliceAndBob::ensure_origin(RuntimeOrigin::signed(ALICE)).is_err());
			assert!(AliceAndBob::ensure_origin(RuntimeOrigin::signed(BOB)).is_err());
			assert!(AliceAndBob::ensure_origin(RuntimeOrigin::root()).is_err());

			// Rely on integration tests to verify full approval workflow
			// Verifies trait implementation works as expected
		});
	}

	#[test]
	fn proposal_cancellation_only_by_proposer_and_only_when_pending_status() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);

			let call = make_remark_call("1000").unwrap();
			let call_hash = <Test as Config>::Hashing::hash_of(&call);

			// Create proposal
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call.clone(),
				ALICE_ORIGIN_ID,
				None,
			));

			// Verify proposal exists
			assert!(Proposals::<Test>::contains_key(call_hash, ALICE_ORIGIN_ID));

			// Read pallet storage to verify proposal is marked as pending
			assert!(Proposals::<Test>::contains_key(call_hash, ALICE_ORIGIN_ID));
			let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.status, ProposalStatus::Pending);

			// Cancel proposal
			assert_ok!(OriginAndGate::cancel_proposal(
				RuntimeOrigin::signed(ALICE),
				call_hash,
				ALICE_ORIGIN_ID,
			));

			// Verify proposal no longer exists
			assert!(!Proposals::<Test>::contains_key(call_hash, ALICE_ORIGIN_ID));

			// Verify proposal calls no longer exists
			assert!(!ProposalCalls::<Test>::contains_key(call_hash));

			// Verify event
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCancelled {
				proposal_hash: call_hash,
				origin_id: ALICE_ORIGIN_ID,
			}));

			// Non-proposer cannot cancel
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call.clone(),
				ALICE_ORIGIN_ID,
				None,
			));

			assert_noop!(
				OriginAndGate::cancel_proposal(
					RuntimeOrigin::signed(BOB),
					call_hash,
					ALICE_ORIGIN_ID,
				),
				Error::<Test>::NotAuthorized
			);

			// Proposal without 'Pending' status cannot be cancelled
			let call2 = make_remark_call("1001").unwrap(); // Different call with different hash
			let call_hash2 = <Test as Config>::Hashing::hash_of(&call2);

			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call2.clone(),
				ALICE_ORIGIN_ID,
				None,
			));

			// Create proposal info with sufficient approvals
			let mut approvals = BoundedVec::default();
			approvals.try_push(ALICE_ORIGIN_ID.into()).unwrap();
			approvals.try_push(BOB_ORIGIN_ID.into()).unwrap(); // Already have 2 approvals

			let proposal_info = ProposalInfo {
				call_hash: call_hash2,
				expiry: None,
				approvals,
				status: ProposalStatus::Executed,
				proposer: ALICE,
			};

			// Skip calling `propose` and instead store proposal directly in storage
			// but not the `call` to execute since here that does not matter
			// Override proposal with `Executed` status
			Proposals::<Test>::insert(call_hash2, ALICE_ORIGIN_ID, proposal_info);

			// Verify proposal remains `Executed`
			let proposal = Proposals::<Test>::get(call_hash2, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.status, ProposalStatus::Executed);

			assert_noop!(
				OriginAndGate::cancel_proposal(
					RuntimeOrigin::signed(BOB),
					call_hash2,
					ALICE_ORIGIN_ID,
				),
				Error::<Test>::ProposalNotPending
			);
		});
	}

	#[test]
	fn approval_withdrawn_by_an_origin_that_previously_approved_but_not_yet_executed() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);

			let call = make_remark_call("1000").unwrap();
			let call_hash = <Test as Config>::Hashing::hash_of(&call);

			// Create proposal
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call.clone(),
				ALICE_ORIGIN_ID,
				None,
			));

			// Verify approval exists
			let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			assert!(proposal.approvals.contains(&ALICE_ORIGIN_ID));

			// Alice withdraws approval
			assert_ok!(OriginAndGate::withdraw_approval(
				RuntimeOrigin::signed(ALICE),
				call_hash,
				ALICE_ORIGIN_ID,
				ALICE_ORIGIN_ID,
			));

			// Verify approval removed
			let updated_proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			assert!(!updated_proposal.approvals.contains(&ALICE_ORIGIN_ID));

			// Verify updated proposal status reflects that an approval
			// was cancelled by Alice and has status `Pending`
			assert_eq!(updated_proposal.status, ProposalStatus::Pending);

			// Verify event
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::OriginApprovalWithdrawn {
				proposal_hash: call_hash,
				origin_id: ALICE_ORIGIN_ID,
				withdrawing_origin_id: ALICE_ORIGIN_ID,
			}));
		});
	}

	#[test]
	fn propose_after_expiry_fails() {
		new_test_ext().execute_with(|| {
			let starting_block = 1;
			// Override the `ProposalLifetime` value of the runtime in mock
			System::set_block_number(starting_block);

			let call = make_remark_call("1000").unwrap();
			let call_hash = <Test as Config>::Hashing::hash_of(&call);
			let expiry = Some(starting_block + 10); // Expire after 10 blocks

			System::set_block_number(starting_block + 11);

			// Create proposal
			let result = OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call.clone(),
				ALICE_ORIGIN_ID,
				expiry,
			);

			// Verify returned expected error
			assert_eq!(result, Err(Error::<Test>::ProposalExpired.into()));

			// Proposal should not exist in storage since creation failed
			assert!(!Proposals::<Test>::contains_key(call_hash, ALICE_ORIGIN_ID));
		});
	}

	#[test]
	fn approve_after_expiry_fails() {
		new_test_ext().execute_with(|| {
			let starting_block = 1;
			// Override the `ProposalLifetime` value of the runtime in mock
			System::set_block_number(starting_block);

			let call = make_remark_call("1000").unwrap();
			let call_hash = <Test as Config>::Hashing::hash_of(&call);
			let expiry = Some(starting_block + 10); // Expire after 10 blocks

			// Manually create and insert proposal but with empty `approvals`
			// without the proposer automatically approving that normally occurs.
			// Instead delay that to occur later
			let mut approvals = BoundedVec::default();

			let proposal_info = ProposalInfo {
				call_hash,
				expiry,
				approvals,
				status: ProposalStatus::Pending, // Force pending even enough approvals to execute
				proposer: ALICE,
			};

			// Insert custom proposal directly into storage
			Proposals::<Test>::insert(call_hash, ALICE_ORIGIN_ID, proposal_info);
			ProposalCalls::<Test>::insert(call_hash, call);

			// Advance past expiry
			System::set_block_number(starting_block + 11);

			// Proposal should be marked as Pending due to override
			let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.status, ProposalStatus::Pending);

			// // Manually process proposer approval to occur after expiry
			// approvals.try_push(ALICE_ORIGIN_ID).unwrap();
			// Approvals::<Test>::insert((call_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID, ALICE);

			// // Verify test setup correct
			// let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			// assert_eq!(proposal.approvals.len(), 1 as usize);
			// assert_eq!(proposal.status, ProposalStatus::Pending);

			println!(
				"Current block: {}, Expiry: {:?}, Call hash: {:?}",
				System::block_number(),
				Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap().expiry.unwrap(),
				call_hash
			);

			// Attempt to approve after expiry should fail
			let result = OriginAndGate::add_approval(
				RuntimeOrigin::signed(BOB),
				call_hash,
				ALICE_ORIGIN_ID,
				BOB_ORIGIN_ID,
			);

			let post_call_proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID);
			println!(
				"Proposal exists: {}, Status: {:?}",
				post_call_proposal.is_some(),
				post_call_proposal.map(|p| p.status)
			);

			// Verify returned expected error
			assert_eq!(result, Err(Error::<Test>::ProposalExpired.into()));

			// Whilst add_approval detects proposal has expired it updates the
			// status to `Expired` and then returns an error but Substrate automatically
			// rolls back all storage changes made within a function when that function
			// returns an error as part of its transaction model to ensure atomicity so
			// even though code in add_approval is updating status to `Expired` that
			// change gets rolled back because the function returns an error.
			// So the proposal status remains `Pending`.
			let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.status, ProposalStatus::Pending);
		});
	}

	#[test]
	fn additional_approvals_after_required_approvals_count_are_rejected() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);

			let call = make_remark_call("1000").unwrap();
			let call_hash = <Test as Config>::Hashing::hash_of(&call);

			// Create proposal
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call.clone(),
				ALICE_ORIGIN_ID,
				None,
			));

			// First approval (already counts as one from proposal)
			let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.approvals.len(), 1); // Proposer counts as first approval

			// Second approval from Bob
			assert_ok!(OriginAndGate::add_approval(
				RuntimeOrigin::signed(BOB),
				call_hash,
				ALICE_ORIGIN_ID,
				BOB_ORIGIN_ID,
			));

			// Verify call was executed and assume MaxApprovals::get() == 2
			let executed_proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(executed_proposal.status, ProposalStatus::Executed);

			// Trying additional approval should fail since already executed
			assert_noop!(
				OriginAndGate::add_approval(
					RuntimeOrigin::signed(CHARLIE),
					call_hash,
					ALICE_ORIGIN_ID,
					CHARLIE_ORIGIN_ID,
				),
				Error::<Test>::ProposalAlreadyExecuted
			);

			// Verify proposal status is still `Executed`
			// despite additional approval confirming the approval did not override its status
			let executed_proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(executed_proposal.status, ProposalStatus::Executed);
		});
	}

	#[test]
	fn proposal_cancellation_cleans_up_all_storage() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);

			let call = make_remark_call("1000").unwrap();
			let call_hash = <Test as Config>::Hashing::hash_of(&call);

			// Create proposal
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call.clone(),
				ALICE_ORIGIN_ID,
				None,
			));

			// Verify approvals now exist
			assert!(Approvals::<Test>::get((call_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID).is_some());

			// Skip adding approval from Bob to avoid auto-executio

			// Cancel proposal
			assert_ok!(OriginAndGate::cancel_proposal(
				RuntimeOrigin::signed(ALICE),
				call_hash,
				ALICE_ORIGIN_ID,
			));

			// Verify event
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCancelled {
				proposal_hash: call_hash,
				origin_id: ALICE_ORIGIN_ID,
			}));

			// Verify proposal no longer exists
			assert!(!Proposals::<Test>::contains_key(call_hash, ALICE_ORIGIN_ID));

			// Verify proposal calls no longer exists
			assert!(!ProposalCalls::<Test>::contains_key(call_hash));

			// Verify approvals storage is also cleaned up
			assert!(Approvals::<Test>::get((call_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID).is_none());

			// Try to cancel again and should fail as proposal no longer exists
			assert_noop!(
				OriginAndGate::cancel_proposal(
					RuntimeOrigin::signed(ALICE),
					call_hash,
					ALICE_ORIGIN_ID,
				),
				Error::<Test>::ProposalNotFound
			);

			// Create another proposal and cancel it
			// Use the same call hash again to verify the complete removal of
			// all storage items related to the proposal
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call.clone(),
				ALICE_ORIGIN_ID,
				None,
			));

			// Verify approvals now exist
			assert!(Approvals::<Test>::get((call_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID).is_some());

			// Cancel proposal
			assert_ok!(OriginAndGate::cancel_proposal(
				RuntimeOrigin::signed(ALICE),
				call_hash,
				ALICE_ORIGIN_ID,
			));

			// Verify all storage is cleaned up after cancellation
			assert!(!Proposals::<Test>::contains_key(call_hash, ALICE_ORIGIN_ID));
			assert!(!ProposalCalls::<Test>::contains_key(call_hash));
			assert!(Approvals::<Test>::get((call_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID).is_none());

			// Create a new proposal and try have non-proposer cancel it
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call.clone(),
				ALICE_ORIGIN_ID,
				None,
			));

			// Verify Bob cannot cancel Alice's proposal
			assert_noop!(
				OriginAndGate::cancel_proposal(
					RuntimeOrigin::signed(BOB),
					call_hash,
					ALICE_ORIGIN_ID,
				),
				Error::<Test>::NotAuthorized
			);

			// Storage should still exist after failed cancellation
			assert!(Proposals::<Test>::contains_key(call_hash, ALICE_ORIGIN_ID));
			assert!(ProposalCalls::<Test>::contains_key(call_hash));
			assert!(Approvals::<Test>::get((call_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID).is_some());
		});
	}

	#[test]
	fn max_approvals_enforced() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);

			// Create test scenario where we can verify
			// that max approvals defined by `MaxApprovals::get()`
			// as a configurable parameter of the pallet
			// where upon `MaxApprovals::get()` being met the proposal
			// will be executed.
			//
			// Manually create a proposal that with `MaxApprovals::get()` it
			// is forced to have 'Pending' status instead of 'Executed' status
			// even though it was executed to prevent it was not executed
			// and then add a `MaxApprovals::get()` + 1 approval to test
			// that it returns `TooManyApprovals`

			// Create a call for our test
			let call = make_remark_call("1000").unwrap();
			let call_hash = <Test as Config>::Hashing::hash_of(&call);

			// Manually create and insert proposal with approvals at
			// the `MaxApprovals::get()` limit and forced not to
			// change from 'Pending' status
			let mut approvals = BoundedVec::default();
			approvals.try_push(ALICE_ORIGIN_ID).unwrap();
			approvals.try_push(BOB_ORIGIN_ID).unwrap();

			let proposal_info = ProposalInfo {
				call_hash,
				expiry: None,
				approvals,
				status: ProposalStatus::Pending, // Force pending even enough approvals to execute
				proposer: ALICE,
			};

			// Insert custom proposal directly into storage
			Proposals::<Test>::insert(call_hash, ALICE_ORIGIN_ID, proposal_info);
			ProposalCalls::<Test>::insert(call_hash, call);

			// Add approval records manually
			Approvals::<Test>::insert((call_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID, ALICE);
			Approvals::<Test>::insert((call_hash, ALICE_ORIGIN_ID), BOB_ORIGIN_ID, BOB);

			// Verify test setup correct with `MaxApprovals::get()` and proposal is still 'Pending'
			// status
			let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.approvals.len(), MaxApprovals::get() as usize);
			assert_eq!(proposal.status, ProposalStatus::Pending);

			// Try to add Charlie's approval to check that it fails with `TooManyApprovals`
			assert_noop!(
				OriginAndGate::add_approval(
					RuntimeOrigin::signed(CHARLIE),
					call_hash,
					ALICE_ORIGIN_ID,
					CHARLIE_ORIGIN_ID,
				),
				Error::<Test>::TooManyApprovals
			);
		});
	}

	#[test]
	fn withdraw_approval_works() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);

			let call = make_remark_call("1000").unwrap();
			let call_hash = <Test as Config>::Hashing::hash_of(&call);

			// Create proposal
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call.clone(),
				ALICE_ORIGIN_ID,
				None,
			));

			// Verify proposal has approval from Alice (proposer)
			let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.approvals.len(), 1 as usize); // Alice (proposer)
			assert!(proposal.approvals.contains(&ALICE_ORIGIN_ID));

			// Verify approval stored in Approvals storage map
			assert!(Approvals::<Test>::get((call_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID).is_some());

			// Alice withdraws approval
			assert_ok!(OriginAndGate::withdraw_approval(
				RuntimeOrigin::signed(ALICE),
				call_hash,
				ALICE_ORIGIN_ID,
				ALICE_ORIGIN_ID,
			));

			// Verify Alice's approval removed from proposal
			let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.approvals.len(), 0); // No approvals remain
			assert!(!proposal.approvals.contains(&ALICE_ORIGIN_ID));

			// Verify approval no longer in Approvals storage
			assert!(Approvals::<Test>::get((call_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID).is_none());

			// Verify event
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::OriginApprovalWithdrawn {
				proposal_hash: call_hash,
				origin_id: ALICE_ORIGIN_ID,
				withdrawing_origin_id: ALICE_ORIGIN_ID,
			}));

			// Cannot withdraw twice
			assert_noop!(
				OriginAndGate::withdraw_approval(
					RuntimeOrigin::signed(ALICE),
					call_hash,
					ALICE_ORIGIN_ID,
					ALICE_ORIGIN_ID,
				),
				Error::<Test>::OriginApprovalNotFound
			);

			// Create another proposal for error case testing
			let call2 = make_remark_call("2000").unwrap();
			let call_hash2 = <Test as Config>::Hashing::hash_of(&call2);

			// Create proposal
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call2.clone(),
				ALICE_ORIGIN_ID,
				None,
			));

			// Try to withdraw Bob's approval that does not exist
			assert_noop!(
				OriginAndGate::withdraw_approval(
					RuntimeOrigin::signed(BOB),
					call_hash2,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
				),
				Error::<Test>::OriginApprovalNotFound
			);
		});
	}

	#[test]
	fn proposal_cancellation_not_allowed_for_non_pending_status() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);

			let call = make_remark_call("1000").unwrap();
			let call_hash = <Test as Config>::Hashing::hash_of(&call);

			// Create proposal
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call.clone(),
				ALICE_ORIGIN_ID,
				None,
			));

			// Create proposal info with sufficient approvals and executed status
			let mut approvals = BoundedVec::<u8, MaxApprovals>::default();
			approvals.try_push(ALICE_ORIGIN_ID.into()).unwrap();
			approvals.try_push(BOB_ORIGIN_ID.into()).unwrap(); // Already have 2 approvals

			let proposal_info = ProposalInfo {
				call_hash,
				expiry: None,
				approvals,
				status: ProposalStatus::Executed,
				proposer: ALICE,
			};

			// Override proposal with executed status
			Proposals::<Test>::insert(call_hash, ALICE_ORIGIN_ID, proposal_info);

			// Verify proposal status is executed
			let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.status, ProposalStatus::Executed);

			// Attempt to cancel executed proposal should fail
			assert_noop!(
				OriginAndGate::cancel_proposal(
					RuntimeOrigin::signed(ALICE),
					call_hash,
					ALICE_ORIGIN_ID,
				),
				Error::<Test>::ProposalNotPending
			);
		});
	}

	#[test]
	fn proposal_execution_with_max_approvals() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);

			// Create a call for our test
			let call = make_remark_call("1000").unwrap();
			let call_hash = <Test as Config>::Hashing::hash_of(&call);

			// Create proposal by Alice that adds Alice's approval automatically
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call.clone(),
				ALICE_ORIGIN_ID,
				None,
			));

			// Add Bob's approval triggers execution if MaxApprovals::get() value met
			assert_ok!(OriginAndGate::add_approval(
				RuntimeOrigin::signed(BOB),
				call_hash,
				ALICE_ORIGIN_ID,
				BOB_ORIGIN_ID,
			));

			assert_eq!(MaxApprovals::get() as usize, 2);

			// Verify proposal now Executed
			let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.status, ProposalStatus::Executed);
			assert_eq!(proposal.approvals.len(), MaxApprovals::get() as usize);

			// Try add Charlie's approval but should fail with ProposalAlreadyExecuted
			assert_noop!(
				OriginAndGate::add_approval(
					RuntimeOrigin::signed(CHARLIE),
					call_hash,
					ALICE_ORIGIN_ID,
					CHARLIE_ORIGIN_ID,
				),
				Error::<Test>::ProposalAlreadyExecuted
			);

			// Verify event was emitted for proposal execution
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
				proposal_hash: call_hash,
				origin_id: ALICE_ORIGIN_ID,
				result: Ok(()),
			}));
		});
	}
}

/// Integration tests for origin-and-gate pallet focusing on verifying end-to-end
/// workflows and interactions between components rather than isolated functions,
/// testing the pallet's public API from an external perspective of real-world usage
/// patterns, and with complex workflows and edge cases handled in dedicated integration
/// test files.
mod integration_test {
	use super::*;

	#[test]
	fn and_gate_direct_impossible_with_signed_origins() {
		new_test_ext().execute_with(|| {
			// Test signed origins cannot satisfy AndGate directly
			// to represent real-world scenario where single account
			// cannot simultaneously satisfy multiple origin requirements
			assert!(AliceAndBob::ensure_origin(RuntimeOrigin::signed(ALICE)).is_err());
			assert!(AliceAndBob::ensure_origin(RuntimeOrigin::signed(BOB)).is_err());
		});
	}

	#[test]
	fn and_gate_direct_impossible_with_root_origin() {
		new_test_ext().execute_with(|| {
			// Test even root origin cannot bypass AndGate requirements
			assert!(AliceAndBob::ensure_origin(RuntimeOrigin::root()).is_err());
		});
	}

	#[test]
	fn origin_id_correctly_tracked_in_proposal_workflow() {
		new_test_ext().execute_with(|| {
			// Set block number for event verification
			System::set_block_number(1);

			// Create test call to be used in proposal
			let call: RuntimeCall = Call::set_dummy { new_value: 1000 }.into();
			let call_hash = <Test as Config>::Hashing::hash_of(&call);

			// Propose using Alice's origin and get origin ID dynamically
			let alice_origin_id = ALICE_ORIGIN_ID;

			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				Box::new(call.clone()),
				alice_origin_id,
				None
			));

			// Bob approves proposal with dynamically determined origin ID
			let bob_origin_id = BOB_ORIGIN_ID;

			assert_ok!(OriginAndGate::add_approval(
				RuntimeOrigin::signed(BOB),
				call_hash,
				alice_origin_id,
				bob_origin_id,
			));

			// Verify proposal exists and has both approvals
			let proposal = Proposals::<Test>::get(call_hash, alice_origin_id).unwrap();
			assert_eq!(proposal.approvals.len(), MaxApprovals::get() as usize);

			// Verify both origin IDs are in approvals
			assert!(proposal.approvals.contains(&alice_origin_id));
			assert!(proposal.approvals.contains(&bob_origin_id));

			// Verify approval event emitted with correct origin IDs
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::OriginApprovalAdded {
				proposal_hash: call_hash,
				origin_id: alice_origin_id,
				approving_origin_id: bob_origin_id,
			}));
		});
	}

	mod origin_enforcement {
		use super::*;

		#[test]
		fn and_gate_ensure_origin_properly_enforces_two_origins() {
			new_test_ext().execute_with(|| {
				// Proceed past genesis block so events get deposited
				System::set_block_number(1);

				// Generate call hash
				let call = make_remark_call("1000").unwrap();
				let call_hash =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

				// let call = Box::new(mock::RuntimeCall::System(frame_system::Call::remark {
				// 	remark: vec![1, 2, 3, 4],
				// }));
				// let call_hash = <<Test as Config>::Hashing as
				// sp_runtime::traits::Hash>::hash_of(&call);

				// Proposal by Alice dispatching a signed extrinsic
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
				));

				// Prior to Bob's approval we dispatching a signed extrinsic to test AliceAndBob
				// origin directly and expect it to fail without Bob's approval
				assert_matches!(
					AliceAndBob::ensure_origin(RuntimeOrigin::signed(ALICE)).is_err(),
					true
				);

				// Approval by Bob dispatching a signed extrinsic
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					call_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
				));

				// Read pallet storage to verify the proposal is marked as executed as the
				// AliceAndBob origin should now pass for this call.
				assert!(Proposals::<Test>::contains_key(call_hash, ALICE_ORIGIN_ID));
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);

				assert!(System::events().iter().any(|r| {
					matches!(
						r.event,
						mock::RuntimeEvent::OriginAndGate(crate::Event::ProposalExecuted {
							proposal_hash: call_hash,
							origin_id,
							result
						})
					)
				}))
			});
		}

		#[test]
		fn direct_and_gate_execution_impossible_with_signed_origins() {
			new_test_ext().execute_with(|| {
				// Test signed origins cannot satisfy AndGate directly
				// to represent real-world scenario where single account
				// cannot simultaneously satisfy multiple origin requirements
				assert!(AliceAndBob::ensure_origin(RuntimeOrigin::signed(ALICE)).is_err());
				assert!(AliceAndBob::ensure_origin(RuntimeOrigin::signed(BOB)).is_err());
			});
		}

		#[test]
		fn direct_and_gate_execution_impossible_with_root_origin() {
			new_test_ext().execute_with(|| {
				// Test even root origin cannot bypass AndGate requirements
				assert!(AliceAndBob::ensure_origin(RuntimeOrigin::root()).is_err());
			});
		}

		#[test]
		fn ensure_different_origin_ids_must_be_used() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Generate call hash
				let call = make_remark_call("1000").unwrap();
				let call_hash =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

				// Proposal by Alice
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
				));

				// Try to approve with same origin ID and Alice approving should fail
				let result = OriginAndGate::add_approval(
					RuntimeOrigin::signed(ALICE),
					call_hash,
					ALICE_ORIGIN_ID,
					ALICE_ORIGIN_ID,
				);
				assert!(result.is_err());

				// Try to approve with same origin ID and Bob approving should fail
				let result = OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					call_hash,
					ALICE_ORIGIN_ID,
					ALICE_ORIGIN_ID,
				);
				assert!(result.is_err());

				// Read pallet storage to verify proposal still pending
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);
			});
		}

		#[test]
		fn proposals_execution_requires_two_approvals_not_direct_execution() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Generate call hash
				let call = make_remark_call("1000").unwrap();
				let call_hash =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

				// Proposal by Alice
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
				));

				// Try execute call directly should fail
				assert!(AliceAndBob::ensure_origin(RuntimeOrigin::signed(ALICE)).is_err());

				// Even with root origin direct execution should fail
				assert!(AliceAndBob::ensure_origin(RuntimeOrigin::root()).is_err());

				// Read pallet storage to verify proposal still pending
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);
			});
		}
	}

	mod proposal_lifecycle {
		use super::*;

		#[test]
		fn proposal_approved_but_does_not_execute_and_status_remains_pending_when_only_proposed() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create a dummy proposal
				let call = make_remark_call("1000").unwrap();
				let call_hash =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

				// Alice proposes through `propose` pallet call that automatically adds Alice as
				// first approval
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call,
					ALICE_ORIGIN_ID,
					None,
				));

				// Verify proposal created with Alice's approval and remains `Pending`
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);
				assert_eq!(proposal.approvals.len(), 1); // Only Alice (the proposer) approved so far

				// Verify Alice's approval is recorded in Approvals storage
				assert!(
					Approvals::<Test>::get((call_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID).is_some()
				);

				// At this point the proposal should have `Pending` status sinc only have Alice's
				// approval and it is less than `MaxApprovals::get()`

				let events = System::events();
				// Verify `ProposalCreated` event was emitted
				assert!(events.iter().any(|record| matches!(
					record.event,
					mock::RuntimeEvent::OriginAndGate(Event::ProposalCreated { proposal_hash, .. })
					if proposal_hash == call_hash
				)));

				// Verify no `ProposalExecuted` event was emitted
				assert!(!events.iter().any(|record| matches!(
					record.event,
					mock::RuntimeEvent::OriginAndGate(Event::ProposalExecuted { .. })
				)));
			});
		}

		#[test]
		fn proposal_successfully_executes_and_status_becomes_executed_with_two_approvals() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create dummy proposal
				let call = make_remark_call("1000").unwrap();
				let call_hash =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

				// Alice proposes through `propose` pallet call that automatically adds Alice as
				// first approval
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call,
					ALICE_ORIGIN_ID,
					None,
				));

				// Verify proposal created with Alice's approval and remains `Pending`
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);

				// Adding Bob's approval should trigger execution since now have
				// `MaxApprovals::get()` approvals
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					call_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
				));

				// Verify proposal status changed to `Executed`
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);
				assert_eq!(proposal.approvals.len(), MaxApprovals::get() as usize); // Alice + Bob

				// Verify both `OriginApprovalAdded` and `ProposalExecuted` events were emitted
				let events = System::events();
				assert!(events.iter().any(|record| matches!(
					record.event,
					mock::RuntimeEvent::OriginAndGate(Event::OriginApprovalAdded { proposal_hash, .. })
					if proposal_hash == call_hash
				)));

				assert!(events.iter().any(|record| matches!(
					record.event,
					mock::RuntimeEvent::OriginAndGate(Event::ProposalExecuted { .. })
				)));
			});
		}

		#[test]
		fn approve_propagates_errors_other_than_insufficient_approvals() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create proposal hash but do not store the call data to execute
				// because that will cause `ProposalNotFound` error during execution
				let call_hash = H256::repeat_byte(0xab);

				// Create proposal info with sufficient approvals
				let mut approvals = BoundedVec::<OriginId, MaxApprovals>::default();
				approvals.try_push(ALICE_ORIGIN_ID.into()).unwrap();
				approvals.try_push(BOB_ORIGIN_ID.into()).unwrap(); // Already have 2 approvals

				let proposal_info = ProposalInfo {
					call_hash,
					expiry: None,
					approvals,
					status: ProposalStatus::Pending,
					proposer: ALICE,
				};

				// Skip calling `propose` and instead store proposal directly in storage
				// but not the `call` to execute
				Proposals::<Test>::insert(call_hash, ALICE_ORIGIN_ID, proposal_info);

				// Verify proposal created with Alice's approval and remains `Pending`
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);

				// Approval of proposal by Bob means we have enough approvals to try execution but
				// should fail with `ProposalNotFound` because we did not store the `call` to
				// execute
				let result = OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					call_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
				);

				// Verify error is fully propagated and is not `InsufficientApprovals` error since
				// we silently ignore that error
				assert!(result.is_err(), "Expected error but got success");

				// Ensure any error type other than `InsufficientApprovals` is propagated.
				// Extract inner error from DispatchErrorWithPostInfo
				if let Err(err) = result {
					let dispatch_error = &err.error;
					if let DispatchError::Module(module_error) = dispatch_error {
						// Get pallet index for `OriginAndGate` that is usually
						// Substrate default of 42 for test pallets
						let origin_and_gate_index = OriginAndGate::index() as u8;

						// Define error indices based on position in Error enum
						const INSUFFICIENT_APPROVALS_INDEX: u8 = 7;
						const PROPOSAL_NOT_FOUND_INDEX: u8 = 1;

						assert!(
							!(module_error.index == origin_and_gate_index &&
								module_error.error[0] == INSUFFICIENT_APPROVALS_INDEX),
							"Encountered InsufficientApprovals error that should have been ignored"
						);

						// Additional verification that we actually got `ProposalNotFound` error
						if module_error.index == origin_and_gate_index {
							assert_eq!(
								module_error.error[0], PROPOSAL_NOT_FOUND_INDEX,
								"Expected `ProposalNotFound` error (index {}) but got error index: {}",
								PROPOSAL_NOT_FOUND_INDEX, module_error.error[0]
							);
						} else {
							panic!("Error from unexpected pallet index: {}", module_error.index);
						}
					} else {
						// Check we actually got a module error for completeness
						panic!("Expected a module error but got: {:?}", dispatch_error);
					}
				}
			});
		}
	}
}
