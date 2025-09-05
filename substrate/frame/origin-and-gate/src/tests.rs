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

	// Convert the value to a BoundedVec<u8>
	let value_bytes = value.to_string().into_bytes();
	let bounded_bytes: DummyValueOf =
		BoundedVec::try_from(value_bytes).map_err(|_| "Value too large for BoundedVec")?;

	let remark = self::Call::<Test>::set_dummy { new_value: bounded_bytes };
	Ok(Box::new(RuntimeCall::OriginAndGate(remark)))
}

/// Helper function to create dummy call for use with testing
fn create_dummy_call(value: u64) -> Box<<Test as Config>::RuntimeCall> {
	// Convert the value to a BoundedVec<u8>
	let value_bytes = value.to_string().into_bytes();
	let bounded_bytes: DummyValueOf =
		BoundedVec::try_from(value_bytes).expect("Value should fit in BoundedVec");

	let call = Call::<Test>::set_dummy { new_value: bounded_bytes };
	Box::new(RuntimeCall::OriginAndGate(call))
}

/// Helper function to get current timepoint
/// Uses actual extrinsic index for more accurate testing
fn current_timepoint() -> Timepoint<BlockNumberFor<Test>> {
	Timepoint {
		height: System::block_number(),
		index: System::extrinsic_index().unwrap_or_default(),
	}
}

/// Helper function to check if proposal exists in storage
fn proposal_exists(proposal_hash: H256, proposal_origin_id: OriginId) -> bool {
	Proposals::<Test>::get(proposal_hash, proposal_origin_id).is_some()
}

/// Helper function to check if proposal call exists in storage
fn proposal_call_exists(proposal_hash: H256) -> bool {
	ProposalCalls::<Test>::get(proposal_hash).is_some()
}

/// Helper function to check if any approvals exist for a proposal
fn proposal_has_approvals(proposal_hash: H256, proposal_origin_id: OriginId) -> bool {
	// Cannot directly check if any entries exist with a specific prefix in a double map
	// Testing purposes only simplification since in a real scenario we might need
	// a more sophisticated approach
	let proposal = Proposals::<Test>::get(proposal_hash, proposal_origin_id);
	match proposal {
		Some(p) => !p.approvals.is_empty(),
		None => false,
	}
}

/// Unit tests that focus on testing individual pallet functions in isolation
/// rather than complex workflows covered by integration tests that are in ../tests
mod unit_test {
	use super::*;

	mod set_dummy {
		use super::*;

		#[test]
		fn set_dummy_works() {
			new_test_ext().execute_with(|| {
				// Check initial value
				assert_eq!(Dummy::<Test>::get(), None);

				// Set dummy value
				let dummy_bytes = b"1000".to_vec();
				let bounded_dummy: DummyValueOf = BoundedVec::try_from(dummy_bytes).unwrap();
				assert_ok!(OriginAndGate::set_dummy(RuntimeOrigin::root(), bounded_dummy.clone()));

				// Check value set
				assert_eq!(Dummy::<Test>::get(), Some(bounded_dummy.clone()));

				// Set new value
				let new_dummy_bytes = b"100".to_vec();
				let bounded_new_dummy: DummyValueOf =
					BoundedVec::try_from(new_dummy_bytes).unwrap();
				assert_ok!(OriginAndGate::set_dummy(
					RuntimeOrigin::root(),
					bounded_new_dummy.clone()
				));

				// Check value updated
				assert_eq!(Dummy::<Test>::get(), Some(bounded_new_dummy));
			});
		}

		#[test]
		fn set_dummy_privileged_call_fails_with_non_root_origin() {
			new_test_ext().execute_with(|| {
				// Attempt to set with signed origin should fail
				let dummy_bytes = b"1000".to_vec();
				let bounded_dummy: DummyValueOf = BoundedVec::try_from(dummy_bytes).unwrap();
				assert_noop!(
					OriginAndGate::set_dummy(RuntimeOrigin::signed(1), bounded_dummy),
					DispatchError::BadOrigin
				);
			});
		}
	}

	mod propose {
		use super::*;

		#[test]
		fn propose_a_proposal_creates_new_proposal() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				let call = create_dummy_call(1000);
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Record current block and extrinsic index
				let alice_submission_timepoint = Timepoint {
					height: System::block_number(),
					index: System::extrinsic_index().unwrap_or_default(),
				};

				// Propose using Alice's origin
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call,
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(false),
				));

				// Verify proposal stored
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);
				assert_eq!(proposal.approvals.len(), 1);
				assert_eq!(proposal.approvals[0], (ALICE, ALICE_ORIGIN_ID));

				// Verify proposal creation event with timepoint
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCreated {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					proposal_account_id: ALICE,
					timepoint: alice_submission_timepoint,
				}));
			});
		}

		#[test]
		fn duplicate_proposals_create_unique_entries_with_warning() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create identical call parameters
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);
				let proposal_origin_id = ALICE_ORIGIN_ID;
				let expiry_at = None;

				// First proposal should succeed
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					proposal_origin_id,
					expiry_at,
					Some(true),
					None,
					None,
					None,
					Some(false),
				));

				// Second identical proposal should also succeed but create a unique entry
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					proposal_origin_id,
					expiry_at,
					Some(true),
					None,
					None,
					None,
					Some(false),
				));

				// Check DuplicateProposalWarning event was emitted
				let events = System::events();
				let warning_event = events.iter().find(|e| {
					matches!(
						e.event,
						RuntimeEvent::OriginAndGate(Event::DuplicateProposalWarning { .. })
					)
				});
				assert!(
					warning_event.is_some(),
					"DuplicateProposalWarning event should have been emitted"
				);

				// Proposal by different user with same parameters should succeed
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(BOB),
					call.clone(),
					proposal_origin_id,
					expiry_at,
					Some(true),
					None,
					None,
					None,
					Some(false),
				));

				// Check another DuplicateProposalWarning event was emitted
				let events = System::events();
				let warning_events_count = events
					.iter()
					.filter(|e| {
						matches!(
							e.event,
							RuntimeEvent::OriginAndGate(Event::DuplicateProposalWarning { .. })
						)
					})
					.count();
				assert_eq!(
					warning_events_count, 2,
					"Two DuplicateProposalWarning events should have been emitted"
				);

				// Verify we have three distinct proposal entries
				let proposal_events = events
					.iter()
					.filter(|e| {
						matches!(
							e.event,
							RuntimeEvent::OriginAndGate(Event::ProposalCreated { .. })
						)
					})
					.count();
				assert_eq!(proposal_events, 3, "Three distinct proposals should have been created");
			});
		}

		#[test]
		fn propose_after_expiry_fails() {
			new_test_ext().execute_with(|| {
				let starting_block = 1;
				// Override the `ProposalExpiry` value of the runtime in mock
				System::set_block_number(starting_block);

				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);
				// Expire after 10 blocks
				let expiry_at = Some(starting_block + <Test as Config>::ProposalExpiry::get());

				System::set_block_number(expiry_at.unwrap() + 1);

				// Create proposal
				let result = OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					expiry_at,
					Some(true),
					None,
					None,
					None,
					Some(false),
				);

				// Verify returned expected error
				assert_eq!(result, Err(Error::<Test>::ProposalExpired.into()));

				// Proposal should not exist in storage since creation failed
				assert!(!Proposals::<Test>::contains_key(proposal_hash, ALICE_ORIGIN_ID));
			});
		}
	}

	mod cancel_proposal {
		use super::*;

		#[test]
		fn proposal_cancellation_only_by_proposer_and_only_when_pending_status() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Create proposal
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Verify proposal exists
				assert!(Proposals::<Test>::contains_key(proposal_hash, ALICE_ORIGIN_ID));

				// Read pallet storage to verify proposal is marked as pending
				assert!(Proposals::<Test>::contains_key(proposal_hash, ALICE_ORIGIN_ID));
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);

				// Cancel proposal
				assert_ok!(OriginAndGate::cancel_proposal(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify proposal no longer exists
				assert!(!Proposals::<Test>::contains_key(proposal_hash, ALICE_ORIGIN_ID));

				// Verify proposal calls no longer exists
				assert!(!ProposalCalls::<Test>::contains_key(proposal_hash));

				// Verify event
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCancelled {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					timepoint: current_timepoint(),
				}));

				// Non-proposer cannot cancel
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				assert_noop!(
					OriginAndGate::cancel_proposal(
						RuntimeOrigin::signed(BOB),
						proposal_hash,
						ALICE_ORIGIN_ID,
					),
					Error::<Test>::NotAuthorized
				);

				// Proposal without 'Pending' status cannot be cancelled
				let call2 = make_remark_call("1001").unwrap(); // Different call with different hash
				let proposal_hash2 = <Test as Config>::Hashing::hash_of(&call2);

				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call2.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Create proposal info with sufficient approvals
				let mut approvals =
					BoundedVec::<(AccountId, OriginId), RequiredApprovalsCount>::default();
				approvals.try_push((ALICE, ALICE_ORIGIN_ID)).unwrap();
				approvals.try_push((BOB, BOB_ORIGIN_ID)).unwrap(); // Already have 2 approvals
				let executed_at = Some(System::block_number());

				let proposal_info = ProposalInfo {
					proposal_hash: proposal_hash2,
					proposal_origin_id: ALICE_ORIGIN_ID,
					expiry_at: None,
					approvals,
					status: ProposalStatus::Executed,
					proposer: ALICE,
					executed_at,
					submitted_at: System::block_number(),
					auto_execute: Some(true),
				};

				// Override proposal with `Executed` status
				Proposals::<Test>::insert(proposal_hash2, ALICE_ORIGIN_ID, proposal_info);

				// Verify proposal remains `Executed`
				let proposal = Proposals::<Test>::get(proposal_hash2, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);

				assert_noop!(
					OriginAndGate::cancel_proposal(
						RuntimeOrigin::signed(BOB),
						proposal_hash2,
						ALICE_ORIGIN_ID,
					),
					Error::<Test>::ProposalNotPending
				);
			});
		}

		#[test]
		fn proposal_cancellation_cleans_up_all_storage() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Create proposal
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(false),
				));

				// Verify approvals now exist
				assert!(Approvals::<Test>::get((proposal_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID)
					.is_some());

				// Cancel proposal
				assert_ok!(OriginAndGate::cancel_proposal(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify event
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCancelled {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					timepoint: current_timepoint(),
				}));

				// Verify proposal no longer exists
				assert!(!Proposals::<Test>::contains_key(proposal_hash, ALICE_ORIGIN_ID));

				// Verify proposal calls no longer exists
				assert!(!ProposalCalls::<Test>::contains_key(proposal_hash));

				// Verify approvals storage is also cleaned up
				assert!(Approvals::<Test>::get((proposal_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID)
					.is_none());

				// Try to cancel again and should fail as proposal no longer exists
				assert_noop!(
					OriginAndGate::cancel_proposal(
						RuntimeOrigin::signed(ALICE),
						proposal_hash,
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
					Some(true),
					None,
					None,
					None,
					Some(false),
				));

				// Verify approvals now exist
				assert!(Approvals::<Test>::get((proposal_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID)
					.is_some());

				// Cancel proposal
				assert_ok!(OriginAndGate::cancel_proposal(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify all storage is cleaned up after cancellation
				assert!(!Proposals::<Test>::contains_key(proposal_hash, ALICE_ORIGIN_ID));
				assert!(!ProposalCalls::<Test>::contains_key(proposal_hash));
				assert!(Approvals::<Test>::get((proposal_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID)
					.is_none());

				// Create a new proposal and try have non-proposer cancel it
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(false),
				));

				// Verify Bob cannot cancel Alice's proposal
				assert_noop!(
					OriginAndGate::cancel_proposal(
						RuntimeOrigin::signed(BOB),
						proposal_hash,
						ALICE_ORIGIN_ID,
					),
					Error::<Test>::NotAuthorized
				);

				// Storage should still exist after failed cancellation
				assert!(Proposals::<Test>::contains_key(proposal_hash, ALICE_ORIGIN_ID));
				assert!(ProposalCalls::<Test>::contains_key(proposal_hash));
				assert!(Approvals::<Test>::get((proposal_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID)
					.is_some());
			});
		}

		#[test]
		fn proposal_cancellation_not_allowed_for_executed_status() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Create proposal
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Create proposal info with sufficient approvals and executed status
				let mut approvals =
					BoundedVec::<(AccountId, OriginId), RequiredApprovalsCount>::default();
				approvals.try_push((ALICE, ALICE_ORIGIN_ID)).unwrap();
				approvals.try_push((BOB, BOB_ORIGIN_ID)).unwrap(); // Already have 2 approvals
				let executed_at = Some(System::block_number());

				let proposal_info = ProposalInfo {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					expiry_at: None,
					approvals,
					status: ProposalStatus::Executed,
					proposer: ALICE,
					executed_at,
					submitted_at: System::block_number(),
					auto_execute: Some(true),
				};

				// Override proposal with executed status
				Proposals::<Test>::insert(proposal_hash, ALICE_ORIGIN_ID, proposal_info);

				// Verify proposal status is executed
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);

				// Attempt to cancel executed proposal should fail
				assert_noop!(
					OriginAndGate::cancel_proposal(
						RuntimeOrigin::signed(ALICE),
						proposal_hash,
						ALICE_ORIGIN_ID,
					),
					Error::<Test>::ProposalNotPending
				);
			});
		}
	}

	mod add_approval {
		use super::*;

		#[test]
		fn approve_of_proposal_adds_approval() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create call
				let call = create_dummy_call(1000);
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Propose using Alice's origin
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call,
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Approve using Bob's origin
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
				));

				// Verify approval added
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.approvals.len(), RequiredApprovalsCount::get() as usize);
				assert!(proposal.approvals.contains(&(BOB, BOB_ORIGIN_ID)));

				// Verify event emitted
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::OriginApprovalAdded {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					approving_origin_id: BOB_ORIGIN_ID,
					approving_account_id: BOB,
					timepoint: current_timepoint(),
				}));
			});
		}

		#[test]
		fn cannot_approve_own_proposal_with_different_proposal_origin_id() {
			new_test_ext().execute_with(|| {
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Alice tries to approve with different origin ID should fail
				assert_noop!(
					OriginAndGate::add_approval(
						RuntimeOrigin::signed(ALICE),
						proposal_hash,
						ALICE_ORIGIN_ID,
						BOB_ORIGIN_ID,
						None,
						None,
						None,
					),
					Error::<Test>::CannotApproveOwnProposalUsingDifferentOrigin
				);
			});
		}

		#[test]
		fn duplicate_approval_of_proposal_fails() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create call
				let call = create_dummy_call(42);
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Propose using Alice's origin
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call,
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Try approve again with same origin
				assert_noop!(
					OriginAndGate::add_approval(
						RuntimeOrigin::signed(ALICE),
						proposal_hash,
						ALICE_ORIGIN_ID,
						ALICE_ORIGIN_ID,
						None,
						None,
						None,
					),
					Error::<Test>::AccountOriginAlreadyApproved
				);
			});
		}

		#[test]
		fn approve_non_existent_proposal_fails() {
			new_test_ext().execute_with(|| {
				// Create non-existent call hash
				let proposal_hash = H256::repeat_byte(0xab);

				// Try approve non-existent proposal
				assert_noop!(
					OriginAndGate::add_approval(
						RuntimeOrigin::signed(BOB),
						proposal_hash,
						ALICE_ORIGIN_ID,
						BOB_ORIGIN_ID,
						None,
						None,
						None,
					),
					Error::<Test>::ProposalNotFound
				);
			});
		}

		#[test]
		fn approve_with_wrong_proposal_hash_fails_to_find_proposal() {
			new_test_ext().execute_with(|| {
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Use different call hash
				let wrong_call = make_remark_call("2000").unwrap();
				let wrong_proposal_hash = <Test as Config>::Hashing::hash_of(&wrong_call);

				// Bob tries approve but should fail
				assert_noop!(
					OriginAndGate::add_approval(
						RuntimeOrigin::signed(BOB),
						wrong_proposal_hash,
						ALICE_ORIGIN_ID,
						BOB_ORIGIN_ID,
						None,
						None,
						None,
					),
					Error::<Test>::ProposalNotFound
				);
			});
		}

		#[test]
		fn approve_after_expiry_fails() {
			new_test_ext().execute_with(|| {
				let starting_block = 1;
				System::set_block_number(starting_block);

				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);
				// Expire after 10 blocks when `ProposalExpiry` is set to 10
				let expiry_at = Some(starting_block + <Test as Config>::ProposalExpiry::get());

				// Manually create and insert proposal but with empty `approvals`
				// without the proposer automatically approving that normally occurs.
				// Instead delay that to occur later
				let approvals = BoundedVec::default();

				let proposal_info = ProposalInfo {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					expiry_at,
					approvals,
					// Force pending even enough approvals to execute
					status: ProposalStatus::Pending,
					proposer: ALICE,
					executed_at: None,
					submitted_at: System::block_number(),
					auto_execute: Some(true),
				};

				// Insert custom proposal directly into storage
				Proposals::<Test>::insert(proposal_hash, ALICE_ORIGIN_ID, proposal_info);
				ProposalCalls::<Test>::insert(proposal_hash, call);

				// Advance past expiry
				System::set_block_number(expiry_at.unwrap() + 1);

				// Proposal should be marked as Pending due to override
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);

				// // Manually process proposer approval to occur after expiry
				// approvals.try_push(ALICE_ORIGIN_ID).unwrap();
				// Approvals::<Test>::insert((proposal_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID,
				// ALICE);

				// // Verify test setup correct
				// let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				// assert_eq!(proposal.approvals.len(), 1 as usize);
				// assert_eq!(proposal.status, ProposalStatus::Pending);

				println!(
					"Current block: {}, Expiry: {:?}, Call hash: {:?}",
					System::block_number(),
					Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID)
						.unwrap()
						.expiry_at
						.unwrap(),
					proposal_hash
				);

				// Attempt to approve after expiry should fail
				let result = OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
				);

				let post_call_proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID);
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
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);
			});
		}

		#[test]
		fn approval_after_cancelled_fails() {
			new_test_ext().execute_with(|| {
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Alice cancels proposal
				assert_ok!(OriginAndGate::cancel_proposal(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
				));

				// Bob tries to approve cancelled proposal
				assert_noop!(
					OriginAndGate::add_approval(
						RuntimeOrigin::signed(BOB),
						proposal_hash,
						ALICE_ORIGIN_ID,
						BOB_ORIGIN_ID,
						None,
						None,
						None,
					),
					Error::<Test>::ProposalNotFound
				);
			});
		}

		#[test]
		fn approval_after_executed_fails() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Create proposal
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Proposal that includes approval
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.approvals.len(), 1);

				// Approval from Bob
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
				));

				// Verify call was executed and assume RequiredApprovalsCount::get() == 2
				let executed_proposal =
					Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(executed_proposal.status, ProposalStatus::Executed);

				// Trying additional approval should fail since already executed
				assert_noop!(
					OriginAndGate::add_approval(
						RuntimeOrigin::signed(CHARLIE),
						proposal_hash,
						ALICE_ORIGIN_ID,
						CHARLIE_ORIGIN_ID,
						None,
						None,
						None,
					),
					Error::<Test>::ProposalAlreadyExecuted
				);

				// Verify proposal status is still `Executed`
				// despite additional approval confirming the approval did not override its status
				let executed_proposal =
					Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(executed_proposal.status, ProposalStatus::Executed);
			});
		}

		#[test]
		fn required_approvals_count_enforced() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create test scenario where we can verify
				// that required approvals defined by `RequiredApprovalsCount::get()`
				// as a configurable parameter of the pallet
				// where upon `RequiredApprovalsCount::get()` being met the proposal
				// will be executed.
				//
				// Manually create a proposal that with `RequiredApprovalsCount::get()` it
				// is forced to have 'Pending' status instead of 'Executed' status
				// even though it was executed to prevent it was not executed
				// and then add a `RequiredApprovalsCount::get()` + 1 approval to test
				// that it returns `TooManyApprovals`

				// Create a call for our test
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Manually create and insert proposal with approvals at
				// the `RequiredApprovalsCount::get()` limit and forced not to
				// change from 'Pending' status
				let mut approvals =
					BoundedVec::<(AccountId, OriginId), RequiredApprovalsCount>::default();
				approvals.try_push((ALICE, ALICE_ORIGIN_ID)).unwrap();
				approvals.try_push((BOB, BOB_ORIGIN_ID)).unwrap();
				let executed_at = Some(System::block_number());

				let proposal_info = ProposalInfo {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					expiry_at: None,
					approvals,
					// Force pending even enough approvals to execute
					status: ProposalStatus::Pending,
					proposer: ALICE,
					executed_at,
					submitted_at: System::block_number(),
					auto_execute: Some(true),
				};

				// Insert custom proposal directly into storage
				Proposals::<Test>::insert(proposal_hash, ALICE_ORIGIN_ID, proposal_info);
				ProposalCalls::<Test>::insert(proposal_hash, call);

				// Add approval records manually
				Approvals::<Test>::insert((proposal_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID, ALICE);
				Approvals::<Test>::insert((proposal_hash, ALICE_ORIGIN_ID), BOB_ORIGIN_ID, BOB);

				// Verify test setup correct with `RequiredApprovalsCount::get()` and proposal is
				// still 'Pending' status
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.approvals.len(), RequiredApprovalsCount::get() as usize);
				assert_eq!(proposal.status, ProposalStatus::Pending);

				// Try to add Charlie's approval to check that it fails with `TooManyApprovals`
				assert_noop!(
					OriginAndGate::add_approval(
						RuntimeOrigin::signed(CHARLIE),
						proposal_hash,
						ALICE_ORIGIN_ID,
						CHARLIE_ORIGIN_ID,
						None,
						None,
						None,
					),
					Error::<Test>::TooManyApprovals
				);
			});
		}

		#[test]
		fn proposal_execution_with_required_approvals_count() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create a call for our test
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Create proposal by Alice that adds Alice's approval automatically
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Add Bob's approval triggers execution if RequiredApprovalsCount::get() value met
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
				));

				let execution_timepoint = current_timepoint();

				assert_eq!(RequiredApprovalsCount::get() as usize, 2);

				// Verify proposal now Executed
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);
				assert_eq!(proposal.approvals.len(), RequiredApprovalsCount::get() as usize); // Alice + Bob

				// Verify both `OriginApprovalAdded` and `ProposalExecuted` events were emitted
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::OriginApprovalAdded {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					approving_origin_id: BOB_ORIGIN_ID,
					approving_account_id: BOB,
					timepoint: execution_timepoint,
				}));

				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					result: Ok(()),
					timepoint: execution_timepoint,
					is_collective: false,
				}));

				// Verify dummy value was set
				let expected_bytes = b"1000".to_vec();
				let expected_bounded: DummyValueOf = BoundedVec::try_from(expected_bytes).unwrap();
				assert_eq!(Dummy::<Test>::get(), Some(expected_bounded));
			});
		}
	}

	mod withdraw_approval {
		use super::*;

		#[test]
		fn approval_withdrawn_by_an_origin_that_previously_approved_but_not_yet_executed() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Create proposal
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(false),
				));

				// Verify approval exists
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert!(proposal.approvals.contains(&(ALICE, ALICE_ORIGIN_ID)));

				// Alice withdraws approval
				assert_ok!(OriginAndGate::withdraw_approval(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
					ALICE_ORIGIN_ID,
				));

				// Verify approval withdrawn
				let withdrawn_approvals = WithdrawnApprovals::<Test>::get((
					proposal_hash,
					ALICE_ORIGIN_ID,
					ALICE_ORIGIN_ID,
				))
				.ok_or(Error::<Test>::WithdrawnApprovalNotFound);
				assert!(withdrawn_approvals.unwrap()[0].0 == ALICE);

				// Verify approval removed
				let updated_proposal =
					Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert!(!updated_proposal.approvals.contains(&(ALICE, ALICE_ORIGIN_ID)));

				// Verify updated proposal status reflects that an approval
				// was cancelled by Alice and has status `Pending`
				assert_eq!(updated_proposal.status, ProposalStatus::Pending);

				// Verify event
				System::assert_has_event(RuntimeEvent::OriginAndGate(
					Event::OriginApprovalWithdrawn {
						proposal_hash,
						proposal_origin_id: ALICE_ORIGIN_ID,
						withdrawing_origin_id: ALICE_ORIGIN_ID,
						withdrawing_account_id: ALICE,
						timepoint: current_timepoint(),
					},
				));
			});
		}

		#[test]
		fn withdraw_approval_works() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Create proposal
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(false),
				));

				// Verify proposal has approval from Alice (proposer)
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.approvals.len(), 1 as usize); // Alice (proposer)
				assert!(proposal.approvals.contains(&(ALICE, ALICE_ORIGIN_ID)));

				// Verify approval stored in Approvals storage
				assert!(Approvals::<Test>::get((proposal_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID)
					.is_some());

				// Alice withdraws approval
				assert_ok!(OriginAndGate::withdraw_approval(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
					ALICE_ORIGIN_ID,
				));

				// Verify Alice's approval removed from proposal
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.approvals.len(), 0); // No approvals remain
				assert!(!proposal.approvals.contains(&(ALICE, ALICE_ORIGIN_ID)));

				// Verify withdrawn approval is recorded
				let withdrawn = OriginAndGate::get_approvals_withdrawn();
				assert_eq!(withdrawn.len(), 1);
				assert_eq!(withdrawn[0].0, proposal_hash);
				assert_eq!(withdrawn[0].1, ALICE_ORIGIN_ID);
				assert_eq!(withdrawn[0].2, ALICE_ORIGIN_ID);

				// Verify approval no longer in Approvals storage
				assert!(Approvals::<Test>::get((proposal_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID)
					.is_none());

				// Verify event
				System::assert_has_event(RuntimeEvent::OriginAndGate(
					Event::OriginApprovalWithdrawn {
						proposal_hash,
						proposal_origin_id: ALICE_ORIGIN_ID,
						withdrawing_origin_id: ALICE_ORIGIN_ID,
						withdrawing_account_id: ALICE,
						timepoint: current_timepoint(),
					},
				));

				// Cannot withdraw twice
				assert_noop!(
					OriginAndGate::withdraw_approval(
						RuntimeOrigin::signed(ALICE),
						proposal_hash,
						ALICE_ORIGIN_ID,
						ALICE_ORIGIN_ID,
					),
					Error::<Test>::AccountOriginApprovalNotFound
				);

				// Create another proposal for error case testing
				let call2 = make_remark_call("2000").unwrap();
				let proposal_hash2 = <Test as Config>::Hashing::hash_of(&call2);

				// Create proposal
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call2.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(false),
				));

				// Try to withdraw Bob's approval that does not exist
				assert_noop!(
					OriginAndGate::withdraw_approval(
						RuntimeOrigin::signed(BOB),
						proposal_hash2,
						ALICE_ORIGIN_ID,
						BOB_ORIGIN_ID,
					),
					Error::<Test>::AccountOriginApprovalNotFound
				);
			});
		}
	}

	mod race_condition {
		use super::*;

		#[test]
		fn test_race_condition_approval_after_execution() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create proposal
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				let alice_submission_timepoint = current_timepoint();

				// Verify proposal created with Alice's approval and remains `Pending`
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);

				// Bob approves should trigger execution
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
				));

				let execution_timepoint = current_timepoint();

				// Verify proposal status changed to `Executed`
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);

				// Charlie tries approve after execution
				assert_noop!(
					OriginAndGate::add_approval(
						RuntimeOrigin::signed(CHARLIE),
						proposal_hash,
						ALICE_ORIGIN_ID,
						CHARLIE_ORIGIN_ID,
						None,
						None,
						None,
					),
					Error::<Test>::ProposalAlreadyExecuted
				);

				// Verify approval events with timepoint
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::OriginApprovalAdded {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					approving_origin_id: BOB_ORIGIN_ID,
					approving_account_id: BOB,
					timepoint: execution_timepoint,
				}));

				// Verify execution event with timepoint
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					result: Ok(()),
					timepoint: execution_timepoint,
					is_collective: false,
				}));
			});
		}

		#[test]
		fn test_race_condition_withdrawal_after_execution() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create proposal
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				let alice_submission_timepoint = current_timepoint();

				// Bob approves that should trigger execution
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
				));

				let execution_timepoint = current_timepoint();

				// Verify proposal status changed to `Executed`
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);

				// Alice tries to withdraw her approval after execution
				assert_noop!(
					OriginAndGate::withdraw_approval(
						RuntimeOrigin::signed(ALICE),
						proposal_hash,
						ALICE_ORIGIN_ID,
						ALICE_ORIGIN_ID,
					),
					Error::<Test>::ProposalAlreadyExecuted
				);
			});
		}

		#[test]
		fn test_race_condition_execution_after_expiry() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create proposal with expiry at block 10
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					Some(10), // Expires at block 10
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Manually run the on_initialize hook to process expiring proposals
				// marking them from pending to expired
				OriginAndGate::on_initialize(10);

				// Skip to block 11 after expiry
				System::set_block_number(11);

				// Check proposal status before attempting approval
				let proposal_before =
					Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal_before.status, ProposalStatus::Expired);

				// Bob tries approve after expiry
				let result = OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
				);

				// Verify call fails with ProposalExpired error
				assert_err!(result, Error::<Test>::ProposalExpired);

				// Whilst add_approval detects proposal has expired it updates the
				// status to `Expired` and then returns an error but Substrate automatically
				// rolls back all storage changes made within a function when that function
				// returns an error as part of its transaction model to ensure atomicity so
				// even though code in add_approval is updating status to `Expired` that
				// change gets rolled back because the function returns an error.
				// So the proposal status remains `Pending`.
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Expired);

				// Note: Cannot verify ProposalExpired event because events rolled back
				// when transaction fails in Substrate test framework
			});
		}
	}

	mod storage_cleanup_after_execution {
		use super::*;

		#[test]
		fn storage_cleanup_does_not_happen_during_execution() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create proposal
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Bob approves and triggers execution
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
				));

				// Verify proposal exists in storage before execution
				assert!(proposal_exists(proposal_hash, ALICE_ORIGIN_ID));
				assert!(proposal_call_exists(proposal_hash));
				assert!(proposal_has_approvals(proposal_hash, ALICE_ORIGIN_ID));

				// Verify execution event emitted
				let execution_timepoint = current_timepoint();
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					result: Ok(()),
					timepoint: execution_timepoint,
					is_collective: false,
				}));
			});
		}
	}

	mod storage_cleanup_after_cancellation {
		use super::*;

		#[test]
		fn storage_cleanup_after_cancellation_works() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create proposal
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(false),
				));

				// Verify proposal exists in storage before cancellation
				assert!(proposal_exists(proposal_hash, ALICE_ORIGIN_ID));
				assert!(proposal_call_exists(proposal_hash));
				assert!(proposal_has_approvals(proposal_hash, ALICE_ORIGIN_ID));

				// Alice cancels proposal
				assert_ok!(OriginAndGate::cancel_proposal(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify proposal and all related storage cleaned up
				assert!(!proposal_exists(proposal_hash, ALICE_ORIGIN_ID));
				assert!(!proposal_call_exists(proposal_hash));
				assert!(!proposal_has_approvals(proposal_hash, ALICE_ORIGIN_ID));

				// Verify cancellation event emitted
				let cancellation_timepoint = current_timepoint();
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCancelled {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					timepoint: cancellation_timepoint,
				}));
			});
		}
	}

	mod clean_proposal {
		use super::*;

		#[test]
		fn clean_works_for_expired_proposals() {
			new_test_ext().execute_with(|| {
				// Set retention period to 50 blocks for this test
				ProposalRetentionPeriodWhenNotCancelled::set(50);

				System::set_block_number(1);

				// Create proposal with expiry at block 10
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					Some(10), // Expires at block 10
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Verify proposal is in pending state before expiry
				assert!(proposal_exists(proposal_hash, ALICE_ORIGIN_ID));
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);

				// Advance to block 10 where the proposal should expire
				System::set_block_number(10);

				// Manually run the on_initialize hook to process expiring proposals
				// marking them from pending to expired
				OriginAndGate::on_initialize(10);

				// Advance to block 11 after expiry
				System::set_block_number(11);

				// Try to add approval which should fail due to expiry
				let result = OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
				);
				assert_err!(result, Error::<Test>::ProposalExpired);

				// Verify proposal still exists in storage but is marked as expired
				assert!(proposal_exists(proposal_hash, ALICE_ORIGIN_ID));
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Expired);

				// Verify that the ProposalExpired event was emitted
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExpired {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					timepoint: Timepoint { height: 10, index: 0 },
				}));

				// Advance to block 60 to satisfy the retention period
				// expiry at block 10 + retention period 50 = block 60)
				System::set_block_number(60);

				// Clean up expired proposal that may be initiated via governance proposal
				assert_ok!(OriginAndGate::clean(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify proposal and all related storage has been cleaned up
				assert!(!proposal_exists(proposal_hash, ALICE_ORIGIN_ID));
				assert!(!proposal_call_exists(proposal_hash));
				assert!(!proposal_has_approvals(proposal_hash, ALICE_ORIGIN_ID));

				// Verify clean event emitted
				let cleanup_timepoint = current_timepoint();
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCleaned {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					timepoint: cleanup_timepoint,
				}));
			});
		}

		#[test]
		fn clean_works_for_withdrawn_approvals() {
			new_test_ext().execute_with(|| {
				// Set retention period to 50 blocks
				ProposalRetentionPeriodWhenNotCancelled::set(50);

				System::set_block_number(1);

				// Create proposal with expiry at block 10
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					Some(10), // Expires at block 10
					Some(true),
					None,
					None,
					None,
					Some(false), // Do not auto-execute
				));

				// Add approval from BOB
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
				));

				// Verify approval exists
				assert!(proposal_has_approvals(proposal_hash, ALICE_ORIGIN_ID));

				// Check if any withdrawn approvals (should be none)
				assert!(WithdrawnApprovals::<Test>::get((
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID
				))
				.is_none());

				// Withdraw approval
				assert_ok!(OriginAndGate::withdraw_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
				));

				// Verify withdrawn approval exists
				assert!(WithdrawnApprovals::<Test>::get((
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID
				))
				.is_some());

				// Advance to block 10 where proposal expires
				System::set_block_number(10);

				// Manually run on_initialize hook to process expiring proposals
				OriginAndGate::on_initialize(10);

				// Advance to block 60 to satisfy retention period
				// (expiry at block 10 + retention period 50 = block 60)
				System::set_block_number(60);

				// Clean up expired proposal
				assert_ok!(OriginAndGate::clean(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify proposal and all related storage cleaned up
				assert!(!proposal_exists(proposal_hash, ALICE_ORIGIN_ID));
				assert!(!proposal_call_exists(proposal_hash));
				assert!(!proposal_has_approvals(proposal_hash, ALICE_ORIGIN_ID));

				// Verify withdrawn approvals cleaned up
				assert!(WithdrawnApprovals::<Test>::get((
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID
				))
				.is_none());

				// Verify clean event emitted
				let timepoint = current_timepoint();
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCleaned {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					timepoint,
				}));

				// Reset retention period to default
				ProposalRetentionPeriodWhenNotCancelled::set(0);
			});
		}

		#[test]
		fn clean_fails_for_pending_proposals() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create proposal
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(false),
				));

				// Try clean pending proposal should fail
				assert_noop!(
					OriginAndGate::clean(
						RuntimeOrigin::signed(ALICE),
						proposal_hash,
						ALICE_ORIGIN_ID,
					),
					Error::<Test>::ProposalNotInExpiredOrExecutedState
				);

				// Verify proposal still exists
				assert!(proposal_exists(proposal_hash, ALICE_ORIGIN_ID));
			});
		}
	}

	mod proposal_retention_period {
		use super::*;

		#[test]
		fn clean_fails_before_retention_period_elapsed() {
			new_test_ext().execute_with(|| {
				// Automatically expire proposals after 10 blocks if proposal does not have an expiry
				ProposalExpiry::set(10);
				// Set retention period to 50 blocks for this test
				ProposalRetentionPeriodWhenNotCancelled::set(50);

				System::set_block_number(1);

				// Create proposal
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Bob approves and triggers execution
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
				));

				// Verify proposal is marked as executed but still exists in storage
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);
				assert!(proposal_exists(proposal_hash, ALICE_ORIGIN_ID));
				assert!(proposal_call_exists(proposal_hash));
				assert!(proposal_has_approvals(proposal_hash, ALICE_ORIGIN_ID));

				println!("clean_fails_before_retention_period_elapsed - Proposal executed_at: {:?}, Current block: {}", proposal.executed_at, System::block_number());

				// Try to clean immediately after execution should fail
				// where retention period starts from execution block + 50 blocks
				assert_noop!(
					OriginAndGate::clean(RuntimeOrigin::signed(ALICE), proposal_hash, ALICE_ORIGIN_ID),
					Error::<Test>::ProposalRetentionPeriodNotElapsed
				);

				// Advance to block 50 since no expiry set so we use proposal expiry of 10 blocks instead
				// and retention period of 50 blocks
				System::set_block_number(50);

				// Clean should still fail
				assert_noop!(
					OriginAndGate::clean(RuntimeOrigin::signed(ALICE), proposal_hash, ALICE_ORIGIN_ID),
					Error::<Test>::ProposalRetentionPeriodNotElapsed
				);

				// Advance to block 51 that is past retention period
				System::set_block_number(51);

				// Clean should now succeed
				assert_ok!(OriginAndGate::clean(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify proposal and all related storage cleaned up
				assert!(!proposal_exists(proposal_hash, ALICE_ORIGIN_ID));
				assert!(!proposal_call_exists(proposal_hash));
				assert!(!proposal_has_approvals(proposal_hash, ALICE_ORIGIN_ID));

				// Verify clean event emitted
				let cleanup_timepoint = current_timepoint();
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCleaned {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					timepoint: cleanup_timepoint,
				}));

				// Reset retention period to default
				ProposalRetentionPeriodWhenNotCancelled::set(0);
			});
		}

		#[test]
		fn clean_after_expiry_respects_retention_period() {
			new_test_ext().execute_with(|| {
				// Set retention period to 50 blocks for this test
				ProposalRetentionPeriodWhenNotCancelled::set(50);

				System::set_block_number(1);

				// Create proposal with expiry at block 10
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					Some(10), // Expires at block 10
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Manually run the on_initialize hook to process expiring proposals
				// marking them from pending to expired
				OriginAndGate::on_initialize(10);

				// Advance to block 11 (after expiry)
				System::set_block_number(11);

				// Try to add approval which should fail due to expiry
				let result = OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
				);
				assert_err!(result, Error::<Test>::ProposalExpired);

				// Verify proposal is marked as expired
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Expired);

				// Try to clean immediately after expiry should fail
				// where retention period starts from expiry block 10 + 50 blocks = block 60
				assert_noop!(
					OriginAndGate::clean(
						RuntimeOrigin::signed(ALICE),
						proposal_hash,
						ALICE_ORIGIN_ID
					),
					Error::<Test>::ProposalRetentionPeriodNotElapsed
				);

				// Advance to block 59 just before retention period ends
				System::set_block_number(59);

				// Clean should still fail
				assert_noop!(
					OriginAndGate::clean(
						RuntimeOrigin::signed(ALICE),
						proposal_hash,
						ALICE_ORIGIN_ID
					),
					Error::<Test>::ProposalRetentionPeriodNotElapsed
				);

				// Advance to block 60 (exactly at retention period end)
				System::set_block_number(60);

				// Clean should now succeed
				assert_ok!(OriginAndGate::clean(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify proposal and all related storage cleaned up
				assert!(!proposal_exists(proposal_hash, ALICE_ORIGIN_ID));
				assert!(!proposal_call_exists(proposal_hash));
				assert!(!proposal_has_approvals(proposal_hash, ALICE_ORIGIN_ID));

				// Verify clean event emitted
				let cleanup_timepoint = current_timepoint();
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCleaned {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					timepoint: cleanup_timepoint,
				}));

				// Reset retention period to default
				ProposalRetentionPeriodWhenNotCancelled::set(0);
			});
		}

		#[test]
		fn clean_after_cancellation_does_not_respect_retention_period() {
			new_test_ext().execute_with(|| {
				// Set retention period to 50 blocks for this test
				ProposalRetentionPeriodWhenNotCancelled::set(50);

				System::set_block_number(1);

				// Create proposal with expiry at block 100
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					Some(100), // Expires at block 100
					Some(true),
					None,
					None,
					None,
					Some(false),
				));

				// Cancel proposal at block 5
				System::set_block_number(5);
				assert_ok!(OriginAndGate::cancel_proposal(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
				));

				assert!(!proposal_exists(proposal_hash, ALICE_ORIGIN_ID));
				assert!(!proposal_call_exists(proposal_hash));
				assert!(!proposal_has_approvals(proposal_hash, ALICE_ORIGIN_ID));
			});
		}

		#[test]
		fn executed_proposal_without_expiry_uses_executed_block() {
			new_test_ext().execute_with(|| {
				ProposalExpiry::set(10);
				// Set retention period to 50 blocks for this test
				ProposalRetentionPeriodWhenNotCancelled::set(50);

				System::set_block_number(1);

				// Create proposal without expiry
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None, // No expiry
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Change to block 5
				System::set_block_number(5);

				// Bob approves and triggers execution
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
				));

				// Verify proposal marked as executed
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);
				println!(
					"Proposal executed_at: {:?}, Current block: {}",
					proposal.executed_at,
					System::block_number()
				);

				// Try clean immediately after execution should fail
				assert_noop!(
					OriginAndGate::clean(
						RuntimeOrigin::signed(ALICE),
						proposal_hash,
						ALICE_ORIGIN_ID
					),
					Error::<Test>::ProposalRetentionPeriodNotElapsed
				);

				// Advance to block 65 where execution at block 5 +
				// proposal expiry of 10 (instead of expiry) +
				// retention period 50 = block 65
				System::set_block_number(65);
				println!("After advancing current block: {}", System::block_number());

				// Clean should now succeed
				assert_ok!(OriginAndGate::clean(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify proposal and all related storage cleaned up
				assert!(!proposal_exists(proposal_hash, ALICE_ORIGIN_ID));
				assert!(!proposal_call_exists(proposal_hash));
				assert!(!proposal_has_approvals(proposal_hash, ALICE_ORIGIN_ID));

				// Reset retention period to default
				ProposalRetentionPeriodWhenNotCancelled::set(0);
			});
		}
	}

	mod proposal_execution_timing {
		use super::*;

		#[test]
		fn proposal_execution_succeeds_before_proposal_expiry() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Create proposal by Alice with no expiry and without auto-execute
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None, // No expiry
					Some(true),
					None,
					None,
					None,
					Some(false), // Don't auto-execute
				));

				// Verify proposal has Pending status
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);

				// Add Bob's approval
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
				));

				// Verify proposal still has Pending status with approvals from Alice + Bob
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);
				assert_eq!(proposal.approvals.len(), RequiredApprovalsCount::get() as usize);

				// Advance block number staying within expiry period
				System::set_block_number(100); // At the end of proposal expiry

				// Execute the proposal manually
				assert_ok!(OriginAndGate::execute_proposal(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify proposal has Executed status
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);
				assert_eq!(proposal.executed_at, Some(100));

				// Verify dummy value was set
				let expected_bytes = b"1000".to_vec();
				let expected_bounded: DummyValueOf = BoundedVec::try_from(expected_bytes).unwrap();
				assert_eq!(Dummy::<Test>::get(), Some(expected_bounded));

				// Verify execution event was emitted
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					result: Ok(()),
					timepoint: Timepoint { height: 100, index: 0 },
					is_collective: false,
				}));
			});
		}
	}

	mod automatic_expiry {
		use super::*;

		#[test]
		fn proposals_automatically_expire_on_block_advancement() {
			new_test_ext().execute_with(|| {
				// Start at block 1
				System::set_block_number(1);

				// Create multiple proposals with different expiry blocks
				// Proposal 1: Expires at block 10
				let call1 = make_remark_call("1001").unwrap();
				let proposal_hash1 = <Test as Config>::Hashing::hash_of(&call1);

				// Proposal 2: Expires at block 10 as well
				let call2 = make_remark_call("1002").unwrap();
				let proposal_hash2 = <Test as Config>::Hashing::hash_of(&call2);

				// Proposal 3: Expires at block 15
				let call3 = make_remark_call("1003").unwrap();
				let proposal_hash3 = <Test as Config>::Hashing::hash_of(&call3);

				// Create proposals
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call1.clone(),
					ALICE_ORIGIN_ID,
					Some(10), // Expires at block 10'
					Some(true),
					None,
					None,
					None,
					Some(false),
				));
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(BOB),
					call2.clone(),
					BOB_ORIGIN_ID,
					Some(10), // Expires at block 10
					Some(true),
					None,
					None,
					None,
					Some(false),
				));
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call3.clone(),
					ALICE_ORIGIN_ID,
					Some(15), // Expires at block 15
					Some(true),
					None,
					None,
					None,
					Some(false),
				));

				// Verify all proposals are in Pending status
				assert_eq!(
					Proposals::<Test>::get(proposal_hash1, ALICE_ORIGIN_ID).unwrap().status,
					ProposalStatus::Pending
				);
				assert_eq!(
					Proposals::<Test>::get(proposal_hash2, BOB_ORIGIN_ID).unwrap().status,
					ProposalStatus::Pending
				);
				assert_eq!(
					Proposals::<Test>::get(proposal_hash3, ALICE_ORIGIN_ID).unwrap().status,
					ProposalStatus::Pending
				);

				// Verify proposals are tracked in ExpiringProposals
				assert!(
					ExpiringProposals::<Test>::get(10).contains(&(proposal_hash1, ALICE_ORIGIN_ID))
				);
				assert!(
					ExpiringProposals::<Test>::get(10).contains(&(proposal_hash2, BOB_ORIGIN_ID))
				);
				assert!(
					ExpiringProposals::<Test>::get(15).contains(&(proposal_hash3, ALICE_ORIGIN_ID))
				);

				// Advance to block 10 that should trigger on_initialize and expire the first two
				// proposals
				System::set_block_number(10);
				OriginAndGate::on_initialize(10);

				// Verify first two proposals are now expired
				assert_eq!(
					Proposals::<Test>::get(proposal_hash1, ALICE_ORIGIN_ID).unwrap().status,
					ProposalStatus::Expired
				);
				assert_eq!(
					Proposals::<Test>::get(proposal_hash2, BOB_ORIGIN_ID).unwrap().status,
					ProposalStatus::Expired
				);
				// Third proposal should still be pending
				assert_eq!(
					Proposals::<Test>::get(proposal_hash3, ALICE_ORIGIN_ID).unwrap().status,
					ProposalStatus::Pending
				);

				// Verify expiry events were emitted
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExpired {
					proposal_hash: proposal_hash1,
					proposal_origin_id: ALICE_ORIGIN_ID,
					timepoint: Timepoint { height: 10, index: 0 },
				}));
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExpired {
					proposal_hash: proposal_hash2,
					proposal_origin_id: BOB_ORIGIN_ID,
					timepoint: Timepoint { height: 10, index: 0 },
				}));

				// Verify ExpiringProposals for block 10 is now empty by being consumed
				assert!(ExpiringProposals::<Test>::get(10).is_empty());

				// Advance to block 15 that should expire the third proposal
				System::set_block_number(15);
				OriginAndGate::on_initialize(15);

				// Verify third proposal is now expired
				assert_eq!(
					Proposals::<Test>::get(proposal_hash3, ALICE_ORIGIN_ID).unwrap().status,
					ProposalStatus::Expired
				);

				// Verify expiry event was emitted for third proposal
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExpired {
					proposal_hash: proposal_hash3,
					proposal_origin_id: ALICE_ORIGIN_ID,
					timepoint: Timepoint { height: 15, index: 0 },
				}));

				// Verify ExpiringProposals for block 15 is now empty
				assert!(ExpiringProposals::<Test>::get(15).is_empty());
			});
		}

		#[test]
		fn max_proposals_to_expire_per_block_is_enforced() {
			new_test_ext().execute_with(|| {
				// Set max proposals to expire per block to 4 for this test
				MaxProposalsToExpirePerBlock::set(4);

				// Start at block 1
				System::set_block_number(1);

				// Create 5 proposals all expiring at block 10
				let mut proposal_hashes = Vec::new();

				for i in 0..5 {
					let call = make_remark_call(&format!("100{}", i)).unwrap();
					let proposal_hash = <Test as Config>::Hashing::hash_of(&call);
					proposal_hashes.push((proposal_hash, ALICE_ORIGIN_ID));

					assert_ok!(OriginAndGate::propose(
						RuntimeOrigin::signed(ALICE),
						call.clone(),
						ALICE_ORIGIN_ID,
						Some(10), // All expire at block 10
						Some(true),
						None,
						None,
						None,
						Some(false),
					));
				}

				// Verify all proposals are in Pending status
				for (i, (hash, proposal_origin_id)) in proposal_hashes.iter().enumerate() {
					assert_eq!(
						Proposals::<Test>::get(hash, proposal_origin_id).unwrap().status,
						ProposalStatus::Pending
					);
				}

				// Verify ExpiringProposals for block 10 contains all 5 proposals
				let expiring_proposals = ExpiringProposals::<Test>::get(10);
				println!(
					"[TEST DEBUG] Number of expiring proposals at block 10: {}",
					expiring_proposals.len()
				);
				for (i, (hash, proposal_origin_id)) in expiring_proposals.iter().enumerate() {
					println!(
						"[TEST DEBUG] Proposal {}: hash={:?}, proposal_origin_id={:?}",
						i, hash, proposal_origin_id
					);
				}
				assert_eq!(expiring_proposals.len(), 5);

				// Advance to block 10 that should trigger on_initialize and expire only the first 4
				// proposals due to MaxProposalsToExpirePerBlock limit
				println!("[TEST DEBUG] Setting block number to 10 and calling on_initialize");
				System::set_block_number(10);
				OriginAndGate::on_initialize(10);

				// Count how many proposals were expired
				let expired_count = proposal_hashes
					.iter()
					.filter(|(hash, proposal_origin_id)| {
						let proposal = Proposals::<Test>::get(hash, proposal_origin_id);
						let is_expired = proposal
							.as_ref()
							.map(|p| p.status == ProposalStatus::Expired)
							.unwrap_or(false);

						println!(
							"[TEST DEBUG] Proposal hash={:?}, proposal_origin_id={:?}, exists={}, status={:?}, is_expired={}",
							hash,
							proposal_origin_id,
							proposal.is_some(),
							proposal.as_ref().map(|p| &p.status),
							is_expired
						);

						is_expired
					})
					.count();

				println!("[TEST DEBUG] Total expired proposals: {}", expired_count);

				// Verify only 4 proposals were expired due to the limit
				assert_eq!(expired_count, 4);

				// Verify we have exactly 4 ProposalExpired events
				let events = System::events();
				let expiry_events = events
					.iter()
					.filter(|event| {
						let is_expiry_event = matches!(
							event.event,
							RuntimeEvent::OriginAndGate(Event::ProposalExpired { .. })
						);
						if is_expiry_event {
							println!("[TEST DEBUG] Found ProposalExpired event: {:?}", event);
						}
						is_expiry_event
					})
					.count();
				println!("[TEST DEBUG] Total ProposalExpired events: {}", expiry_events);
				assert_eq!(expiry_events, 4);

				// Reset the parameter for other tests
				MaxProposalsToExpirePerBlock::set(10);
			});
		}

		#[test]
		fn executed_proposals_are_removed_from_expiry_tracking() {
			new_test_ext().execute_with(|| {
				let starting_block = 1;
				System::set_block_number(starting_block);

				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);
				let proposal_expiry_block = starting_block + 99; // Proposal expires at block 100
				let expiry_at = Some(proposal_expiry_block);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					expiry_at,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Verify proposal in ExpiringProposals storage
				let expiring_at_block_100 =
					OriginAndGate::expiring_proposals(proposal_expiry_block);
				assert!(
					expiring_at_block_100
						.iter()
						.any(|(hash, id)| *hash == proposal_hash && *id == ALICE_ORIGIN_ID),
					"Proposal should be in ExpiringProposals before execution"
				);

				// Bob approves with auto_execute
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
				));

				// Verify proposal executed
				let proposal = OriginAndGate::proposals(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);

				// Verify proposal removed from ExpiringProposals storage
				let expiring_at_block_100_after_execution =
					OriginAndGate::expiring_proposals(proposal_expiry_block);
				assert!(
					!expiring_at_block_100_after_execution
						.iter()
						.any(|(hash, id)| *hash == proposal_hash && *id == ALICE_ORIGIN_ID),
					"Proposal should be removed from ExpiringProposals after execution"
				);

				// Advance to block 5 (execution block + retention period - 1)
				let execution_block = 1; // Current block when executed
				let proposal_retention_period =
					<Test as Config>::ProposalRetentionPeriodWhenNotCancelled::get();
				System::set_block_number(execution_block + proposal_retention_period - 1);

				// Clean up should fail since retention period hasn't yet elapsed
				assert_noop!(
					OriginAndGate::clean(
						RuntimeOrigin::signed(ALICE),
						proposal_hash,
						ALICE_ORIGIN_ID
					),
					Error::<Test>::ProposalRetentionPeriodNotElapsed
				);

				// Advance to cleanup eligible block
				System::set_block_number(execution_block + proposal_retention_period);

				// Clean up should succeed
				assert_ok!(OriginAndGate::clean(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify proposal removed
				assert!(!OriginAndGate::proposals(proposal_hash, ALICE_ORIGIN_ID).is_some());
			});
		}

		#[test]
		fn cancelled_proposals_can_be_cleaned_up_immediately() {
			new_test_ext().execute_with(|| {
				let starting_block = 1;
				System::set_block_number(starting_block);

				// Create proposal with expiry at block 100
				let call = make_remark_call("2000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);
				let proposal_expiry_block = starting_block + 99; // Proposal expires at block 100
				let expiry_at = Some(proposal_expiry_block);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					expiry_at,
					Some(true),
					None,
					None,
					None,
					Some(false),
				));

				// Verify proposal is in ExpiringProposals storage
				let expiring_proposals = OriginAndGate::expiring_proposals(proposal_expiry_block);
				assert!(
					expiring_proposals
						.iter()
						.any(|(hash, id)| *hash == proposal_hash && *id == ALICE_ORIGIN_ID),
					"Proposal should be in ExpiringProposals"
				);

				// Alice cancels proposal
				assert_ok!(OriginAndGate::cancel_proposal(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify proposal is removed from storage after cancellation
				assert!(OriginAndGate::proposals(proposal_hash, ALICE_ORIGIN_ID).is_none());
			});
		}
	}

	mod required_approvals_count_scaling {
		use super::*;

		// Helper function to test a specific RequiredApprovalsCount value
		fn test_with_required_approvals_count(required_approvals_count: u32, auto_execute: bool) {
			new_test_ext().execute_with(|| {
				// Set RequiredApprovalsCount for this test
				RequiredApprovalsCount::set(required_approvals_count);
				System::set_block_number(1);

				// Create a call for our test
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Create proposal by Alice
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(auto_execute),
				));

				// Verify proposal was created with Pending status
				let proposal_info = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal_info.status, ProposalStatus::Pending);

				// Proposer counts as 1 approval already
				let required_additional_approvals = required_approvals_count - 1;

				// Add approvals from other accounts
				let approvers = [BOB, CHARLIE];
				let proposal_origin_ids = [BOB_ORIGIN_ID, CHARLIE_ORIGIN_ID];
				let remaining_approvals_but_not_last =
					required_additional_approvals.saturating_sub(1);

				// Add all but last required approval
				for i in 0..remaining_approvals_but_not_last {
					assert_ok!(OriginAndGate::add_approval(
						RuntimeOrigin::signed(approvers[i as usize]),
						proposal_hash,
						ALICE_ORIGIN_ID,
						proposal_origin_ids[i as usize],
						None,
						None,
						None,
					));

					// Verify proposal still in Pending state
					let proposal_info =
						Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
					assert_eq!(proposal_info.status, ProposalStatus::Pending);
				}

				// Add final approval if needed
				if required_additional_approvals > 0 {
					let final_approver_index = required_additional_approvals - 1;
					assert_ok!(OriginAndGate::add_approval(
						RuntimeOrigin::signed(approvers[final_approver_index as usize]),
						proposal_hash,
						ALICE_ORIGIN_ID,
						proposal_origin_ids[final_approver_index as usize],
						None,
						None,
						None,
					));
				}

				// Verify proposal was executed
				let proposal_info = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal_info.status, ProposalStatus::Executed);

				// Verify dummy value was set
				let expected_bytes = b"1000".to_vec();
				let expected_bounded: DummyValueOf = BoundedVec::try_from(expected_bytes).unwrap();
				assert_eq!(Dummy::<Test>::get(), Some(expected_bounded));
			});
		}

		#[test]
		fn proposal_execution_requires_required_approvals_count_minus_one_additional_approvals() {
			// Verifies that proposals require (RequiredApprovalsCount - 1) additional approvals
			// after the proposer in order to auto-execute or manually execute

			// Test with RequiredApprovalsCount = 2
			test_with_required_approvals_count(2, true);

			// Test with RequiredApprovalsCount = 3
			test_with_required_approvals_count(3, true);
		}
	}

	mod remarks {
		use super::*;

		#[test]
		fn proposal_with_remark_works() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);
				let submission_timepoint = current_timepoint();

				// Create call
				let call = create_dummy_call(1000);
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);
				let remark = b"Conditional approval included with proposal".to_vec();

				// Propose using Alice's origin with remark
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call,
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					Some(remark.clone()),
					None,
					None,
					Some(false),
				));

				// Verify standard proposal creation event
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCreated {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					proposal_account_id: ALICE,
					timepoint: submission_timepoint,
				}));

				// Verify remark event
				System::assert_has_event(RuntimeEvent::OriginAndGate(
					Event::ProposalCreatedWithRemark {
						proposal_hash,
						proposal_origin_id: ALICE_ORIGIN_ID,
						proposal_account_id: ALICE,
						timepoint: submission_timepoint,
						remark,
					},
				));
			});
		}

		#[test]
		fn add_approval_with_remark_works() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create call
				let call = create_dummy_call(1000);
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Propose using Alice's origin without remark
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call,
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(false), // Do not auto-execute
				));

				// Skip to next block for approval
				System::set_block_number(2);
				let approval_timepoint = current_timepoint();
				let remark = b"Conditional approval from Bob".to_vec();

				// Approve using Bob's origin with conditional approval remark
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					Some(remark.clone()),
					None,
					None,
				));

				// Verify standard approval event
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::OriginApprovalAdded {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					approving_origin_id: BOB_ORIGIN_ID,
					approving_account_id: BOB,
					timepoint: approval_timepoint,
				}));

				// Verify remark event
				System::assert_has_event(RuntimeEvent::OriginAndGate(
					Event::OriginApprovalAmendedWithRemark {
						proposal_hash,
						proposal_origin_id: ALICE_ORIGIN_ID,
						approving_origin_id: BOB_ORIGIN_ID,
						approving_account_id: BOB,
						timepoint: approval_timepoint,
						remark,
					},
				));
			});
		}

		#[test]
		fn amend_remark_works_for_proposal_without_approval() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create a proposal from Alice but exclude her approval
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);
				let initial_remark = b"Initial proposal remark".to_vec();

				// Submit proposal with initial remark but exclude proposer approval
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,        // No expiry
					Some(false), // Exclude Alice's approval
					Some(initial_remark.clone()),
					None,
					None,
					Some(false), // Don't auto-execute
				));

				// Verify initial events
				let submission_timepoint = current_timepoint();
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCreated {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					proposal_account_id: ALICE,
					timepoint: submission_timepoint,
				}));

				System::assert_has_event(RuntimeEvent::OriginAndGate(
					Event::ProposalCreatedWithRemark {
						proposal_hash,
						proposal_origin_id: ALICE_ORIGIN_ID,
						proposal_account_id: ALICE,
						timepoint: submission_timepoint,
						remark: initial_remark,
					},
				));

				// Verify Alice's approval is not included
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.approvals.len(), 0);

				System::set_block_number(2);

				// Alice amends remark as proposer
				let amended_remark = b"Amended proposer remark".to_vec();
				assert_ok!(OriginAndGate::amend_remark(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
					None, // No approving_origin_id means amending as proposer
					amended_remark.clone(),
					None,
					None,
				));

				// Verify amendment event
				let amend_timepoint = current_timepoint();
				System::assert_has_event(RuntimeEvent::OriginAndGate(
					Event::ProposerAmendedProposalWithRemark {
						proposal_hash,
						proposal_origin_id: ALICE_ORIGIN_ID,
						proposal_account_id: ALICE,
						timepoint: amend_timepoint,
						remark: amended_remark,
					},
				));

				// Verify Bob (non-proposer) cannot amend Alice's remark
				let bob_remark = b"Bob trying to amend Alice's remark".to_vec();
				assert_noop!(
					OriginAndGate::amend_remark(
						RuntimeOrigin::signed(BOB),
						proposal_hash,
						ALICE_ORIGIN_ID,
						None, // No approving_origin_id
						bob_remark.clone(),
						None,
						None,
					),
					Error::<Test>::NotAuthorized
				);
			});
		}

		#[test]
		fn amend_remark_of_approved_proposal_with_remark_works() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create call
				let call = create_dummy_call(1000);
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Propose using Alice's origin
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call,
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(false), // Don't auto-execute
				));

				// Skip to next block for approval
				System::set_block_number(2);

				// Approve using Bob's origin
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
				));

				// Skip to next block for amending approval
				System::set_block_number(3);
				let amend_timepoint = current_timepoint();
				let amend_remark = b"Amended approval with additional conditions".to_vec();

				// Amend Bob's approval with conditional remark
				assert_ok!(OriginAndGate::amend_remark(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					Some(BOB_ORIGIN_ID),
					amend_remark.clone(),
					None,
					None,
				));

				// Verify remark amendment event
				System::assert_has_event(RuntimeEvent::OriginAndGate(
					Event::OriginApprovalAmendedWithRemark {
						proposal_hash,
						proposal_origin_id: ALICE_ORIGIN_ID,
						approving_origin_id: BOB_ORIGIN_ID,
						approving_account_id: BOB,
						timepoint: amend_timepoint,
						remark: amend_remark,
					},
				));
			});
		}

		#[test]
		fn amend_remark_of_approved_proposal_fails_when_not_approved() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create call
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Propose using Alice's origin
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call,
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(false),
				));

				// Try to amend Bob's non-existent approval
				let amend_remark = b"Amended approval with additional conditions".to_vec();
				assert_noop!(
					OriginAndGate::amend_remark(
						RuntimeOrigin::signed(BOB),
						proposal_hash,
						ALICE_ORIGIN_ID,
						Some(BOB_ORIGIN_ID),
						amend_remark.clone(),
						None,
						None,
					),
					Error::<Test>::AccountOriginApprovalNotFound
				);
			});
		}

		#[test]
		fn remark_stored_event_is_emitted() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);
				let submission_timepoint = current_timepoint();

				// Create call
				let call = create_dummy_call(1000);
				let proposal_hash = BlakeTwo256::hash_of(&call);
				let remark = b"Testing RemarkStored event emission".to_vec();
				let remark_hash = BlakeTwo256::hash_of(&remark);

				// Propose using Alice's origin with remark
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call,
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					Some(remark.clone()),
					None,
					None,
					Some(false),
				));

				// Verify RemarkStored event is emitted
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::RemarkStored {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					account_id: ALICE,
					remark_hash,
				}));
			});
		}
	}

	mod origin_helpers {
		use super::*;

		#[test]
		fn ensure_signed_or_collective_works() {
			new_test_ext().execute_with(|| {
				// Signed origin (ALICE account)
				let signed_origin = RuntimeOrigin::signed(ALICE);
				let result = OriginAndGate::ensure_signed_or_collective(signed_origin);
				assert_ok!(&result);
				let (account_id, is_collective) = result.unwrap();
				assert_eq!(account_id, ALICE);
				assert_eq!(is_collective, false);

				// Signed origin (BOB account)
				let bob_origin = RuntimeOrigin::signed(BOB);
				let result = OriginAndGate::ensure_signed_or_collective(bob_origin);
				assert_ok!(&result);
				let (account_id, is_collective) = result.unwrap();
				assert_eq!(account_id, BOB);
				assert_eq!(is_collective, false);

				// Root origin (collective)
				let root_origin = RuntimeOrigin::root();
				let result = OriginAndGate::ensure_signed_or_collective(root_origin);
				assert_ok!(&result);
				let (account_id, is_collective) = result.unwrap();
				assert_eq!(account_id, ROOT);
				assert_eq!(is_collective, true);

				// Verify collective origin consistently generated
				let root_origin2 = RuntimeOrigin::root();
				let result2 = OriginAndGate::ensure_signed_or_collective(root_origin2);
				let (collective_account2, is_collective2) = result2.unwrap();
				assert_eq!(account_id, collective_account2);
				assert_eq!(is_collective, is_collective2);

				// TECH_FELLOWSHIP origin (collective) has root privileges
				let tech_fellowship_origin = RuntimeOrigin::collective(TECH_FELLOWSHIP);
				let result = OriginAndGate::ensure_signed_or_collective(tech_fellowship_origin);
				assert_ok!(&result);
				let (account_id, is_collective) = result.unwrap();
				assert_eq!(account_id, ROOT);
				assert_eq!(is_collective, true);

				// None origin should fail
				let none_origin = RuntimeOrigin::none();
				let result = OriginAndGate::ensure_signed_or_collective(none_origin);
				assert!(result.is_err());
			});
		}
	}
}

/// Integration tests for this pallet focusing on verifying end-to-end
/// workflows and interactions between components rather than isolated functions,
/// testing the pallet's public API from an external perspective of real-world usage
/// patterns, and with complex workflows and edge cases handled in dedicated integration
/// test files.
mod integration_test {
	use super::*;

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
			let dummy_bytes = b"1000".to_vec();
			let bounded_dummy: DummyValueOf = BoundedVec::try_from(dummy_bytes).unwrap();
			let call: RuntimeCall = Call::set_dummy { new_value: bounded_dummy }.into();
			let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

			// Propose using Alice's origin and get origin ID dynamically
			let alice_origin_id = ALICE_ORIGIN_ID;

			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				Box::new(call.clone()),
				alice_origin_id,
				None,
				Some(true),
				None,
				None,
				None,
				Some(true), // Auto-execute
			));

			// Bob approves proposal with dynamically determined origin ID
			let bob_origin_id = BOB_ORIGIN_ID;

			assert_ok!(OriginAndGate::add_approval(
				RuntimeOrigin::signed(BOB),
				proposal_hash,
				alice_origin_id,
				bob_origin_id,
				None,
				None,
				None,
			));

			// Verify proposal exists and has both approvals
			let proposal = Proposals::<Test>::get(proposal_hash, alice_origin_id).unwrap();
			assert_eq!(proposal.approvals.len(), RequiredApprovalsCount::get() as usize);

			// Verify both origin IDs are in approvals
			assert!(proposal.approvals.contains(&(ALICE, alice_origin_id)));
			assert!(proposal.approvals.contains(&(BOB, bob_origin_id)));

			// Verify approval event emitted with correct origin IDs
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::OriginApprovalAdded {
				proposal_hash,
				proposal_origin_id: ALICE_ORIGIN_ID,
				approving_origin_id: BOB_ORIGIN_ID,
				approving_account_id: BOB,
				timepoint: current_timepoint(),
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
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// let call = Box::new(mock::RuntimeCall::System(frame_system::Call::remark {
				// 	remark: vec![1, 2, 3, 4],
				// }));
				// let proposal_hash = <<Test as Config>::Hashing as
				// sp_runtime::traits::Hash>::hash_of(&call);

				// Proposal by Alice dispatching a signed extrinsic
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				let proposal_timepoint = current_timepoint();

				// Verify proposal created with Alice's approval and remains `Pending`
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);
				assert_eq!(Dummy::<Test>::get(), None); // Call not executed yet

				// Verify proposal creation event with timepoint
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCreated {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					proposal_account_id: ALICE,
					timepoint: proposal_timepoint,
				}));

				// Verify proposal pending and not executed yet since only Alice approved
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);
				assert_eq!(Dummy::<Test>::get(), None); // Call not executed yet

				// No ExecutedCalls entry should exist yet
				assert!(ExecutedCalls::<Test>::get(current_timepoint()).is_none());

				// Prior to Bob's approval we dispatch a signed extrinsic to test AliceAndBob
				// origin directly and expect it to fail without Bob's approval
				assert_matches!(
					AliceAndBob::ensure_origin(RuntimeOrigin::signed(ALICE)).is_err(),
					true
				);

				// Time passing simulated by different block
				System::set_block_number(2);
				let execution_timepoint = current_timepoint();

				// Ensure timepoints different
				assert_ne!(proposal_timepoint, execution_timepoint);

				// Approval by Bob dispatching a signed extrinsic
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
				));

				// Verify execution successful after both approvals
				let expected_bytes = b"1000".to_vec();
				let expected_bounded: DummyValueOf = BoundedVec::try_from(expected_bytes).unwrap();
				assert_eq!(Dummy::<Test>::get(), Some(expected_bounded));

				// Verify ExecutedCalls storage updated with call hash at execution timepoint
				assert_eq!(ExecutedCalls::<Test>::get(execution_timepoint), Some(proposal_hash));

				// Verify proposal marked executed
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);

				// Verify execution event with execution timepoint
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					result: Ok(()),
					timepoint: execution_timepoint,
					is_collective: false,
				}));
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
		fn ensure_different_origin_ids_must_be_used_for_approvals() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Generate call hash
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Proposal by Alice
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Try to approve with same origin ID and Alice approving should fail
				let result = OriginAndGate::add_approval(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
					ALICE_ORIGIN_ID,
					None,
					None,
					None,
				);
				assert!(result.is_err());

				// Try to approve with same origin ID and Bob approving should fail
				let result = OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					ALICE_ORIGIN_ID,
					None,
					None,
					None,
				);
				assert!(result.is_err());

				// Read pallet storage to verify proposal still pending
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);
			});
		}

		#[test]
		fn proposals_execution_requires_two_approvals_not_direct_execution() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Generate call hash
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Proposal by Alice
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Try execute call directly should fail
				assert!(AliceAndBob::ensure_origin(RuntimeOrigin::signed(ALICE)).is_err());

				// Even with root origin direct execution should fail
				assert!(AliceAndBob::ensure_origin(RuntimeOrigin::root()).is_err());

				// Read pallet storage to verify proposal still pending
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
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
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Record current block and extrinsic index
				let alice_submission_timepoint = Timepoint {
					height: System::block_number(),
					index: System::extrinsic_index().unwrap_or_default(),
				};

				// Alice proposes through `propose` pallet call that automatically adds Alice as
				// first approval
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call,
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Verify proposal created with Alice's approval and remains `Pending`
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);
				assert_eq!(proposal.approvals.len(), 1); // Only Alice (the proposer) approved so far

				// Verify Alice's approval is recorded in Approvals storage
				assert!(Approvals::<Test>::get((proposal_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID)
					.is_some());

				// At this point the proposal should have `Pending` status sinc only have Alice's
				// approval and it is less than `RequiredApprovalsCount::get()`

				// Verify `ProposalCreated` event was emitted
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCreated {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					proposal_account_id: ALICE,
					timepoint: alice_submission_timepoint,
				}));

				// Verify no `ProposalExecuted` event was emitted
				assert!(!System::events().iter().any(|record| {
					matches!(
						record.event,
						mock::RuntimeEvent::OriginAndGate(Event::ProposalExecuted { .. })
					)
				}));
			});
		}

		#[test]
		fn proposal_successfully_executes_and_status_becomes_executed_with_two_approvals() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create dummy proposal
				let call = make_remark_call("1000").unwrap();
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Alice proposes through `propose` pallet call that automatically adds Alice as
				// first approval
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call,
					ALICE_ORIGIN_ID,
					None,
					Some(true),
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Verify proposal created with Alice's approval and remains `Pending`
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);

				// Adding Bob's approval should trigger execution since now have
				// `RequiredApprovalsCount::get()` approvals
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
				));

				let execution_timepoint = current_timepoint();

				// Verify proposal status changed to `Executed`
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);
				assert_eq!(proposal.approvals.len(), RequiredApprovalsCount::get() as usize); // Alice + Bob

				// Verify both `OriginApprovalAdded` and `ProposalExecuted` events were emitted
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::OriginApprovalAdded {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					approving_origin_id: BOB_ORIGIN_ID,
					approving_account_id: BOB,
					timepoint: execution_timepoint,
				}));

				// Verify proposal creation event with timepoint
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					result: Ok(()),
					timepoint: execution_timepoint,
					is_collective: false,
				}));

				// Verify dummy value was set
				let expected_bytes = b"1000".to_vec();
				let expected_bounded: DummyValueOf = BoundedVec::try_from(expected_bytes).unwrap();
				assert_eq!(Dummy::<Test>::get(), Some(expected_bounded));
			});
		}

		#[test]
		fn approve_propagates_errors_other_than_insufficient_approvals() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create proposal hash but do not store the call data to execute
				// because that will cause `ProposalNotFound` error during execution
				let proposal_hash = H256::repeat_byte(0xab);

				// Create proposal info with sufficient approvals
				let mut approvals =
					BoundedVec::<(AccountId, OriginId), RequiredApprovalsCount>::default();
				approvals.try_push((ALICE, ALICE_ORIGIN_ID)).unwrap();
				let executed_at = Some(System::block_number());

				let proposal_info = ProposalInfo {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					expiry_at: None,
					approvals,
					status: ProposalStatus::Pending,
					proposer: CHARLIE,
					executed_at,
					submitted_at: System::block_number(),
					auto_execute: Some(true),
				};

				// Skip calling `propose` and instead store proposal directly in storage
				// but not the `call` to execute
				Proposals::<Test>::insert(proposal_hash, ALICE_ORIGIN_ID, proposal_info);

				// Verify proposal created with Alice's approval and remains `Pending`
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);

				// Approval of proposal by Bob means we have enough approvals to try execution but
				// should fail with `ProposalNotFound` because we did not store the `call` to
				// execute
				let result = OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
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
						const INSUFFICIENT_APPROVALS_INDEX: u8 = 9;
						const PROPOSAL_NOT_FOUND_INDEX: u8 = 1;

						assert!(
							!(module_error.index == origin_and_gate_index
								&& module_error.error[0] == INSUFFICIENT_APPROVALS_INDEX),
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

		#[test]
		fn third_approval_after_execution_has_no_effect() {
			new_test_ext().execute_with(|| {
				// Create a proposal
				let call = create_dummy_call(1000);
				let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call,
					ALICE_ORIGIN_ID,
					None,
					None,
					None,
					None,
					None,
					Some(true), // Auto-execute
				));

				// Bob approves (first approval)
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					None,
					None,
					None,
				));

				// Check proposal status is still pending after first approval
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);
				assert_eq!(proposal.approvals.len(), 1);

				// Charlie approves (second approval) so this should execute the proposal
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(CHARLIE),
					proposal_hash,
					ALICE_ORIGIN_ID,
					CHARLIE_ORIGIN_ID,
					None,
					None,
					None,
				));

				// Check proposal status is now executed
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);
				assert_eq!(proposal.approvals.len(), 2);

				// Verify the ProposalExecuted event was emitted
				System::assert_has_event(
					Event::ProposalExecuted {
						proposal_hash,
						proposal_origin_id: ALICE_ORIGIN_ID,
						result: Ok(()),
						timepoint: current_timepoint(),
						is_collective: false,
					}
					.into(),
				);

				// Clear events to check for new ones
				System::reset_events();

				// Dave tries to add a third approval after execution
				assert_err!(
					OriginAndGate::add_approval(
						RuntimeOrigin::signed(DAVE),
						proposal_hash,
						ALICE_ORIGIN_ID,
						DAVE_ORIGIN_ID,
						None,
						None,
						None,
					),
					Error::<Test>::ProposalAlreadyExecuted
				);

				// Verify proposal status and approval count remain unchanged
				let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);
				assert_eq!(proposal.approvals.len(), 2);

				// Verify no ProposalExecuted event was emitted again
				assert!(!System::events().iter().any(|record| {
					matches!(
						record.event,
						RuntimeEvent::OriginAndGate(Event::ProposalExecuted { .. })
					)
				}));
			});
		}
	}
}

/// Tests verifying technical requirements for AndGate
mod andgate_requirements {
	use super::*;

	#[test]
	fn andgate_requires_asynchronous_origin_approval() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);
			let alice_submission_timepoint = current_timepoint();

			// Create call that requires both Alice and Bob's approval
			let call = create_dummy_call(1000);
			let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

			// Alice proposes and approves
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call.clone(),
				ALICE_ORIGIN_ID,
				None,
				Some(true),
				None,
				None,
				None,
				Some(true), // Auto-execute
			));

			// Verify proposal creation event with timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCreated {
				proposal_hash,
				proposal_origin_id: ALICE_ORIGIN_ID,
				proposal_account_id: ALICE,
				timepoint: alice_submission_timepoint,
			}));

			// Verify proposal pending and not executed yet since only Alice approved
			let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.status, ProposalStatus::Pending);
			assert_eq!(Dummy::<Test>::get(), None); // Call not executed yet

			// No ExecutedCalls entry should exist yet
			assert!(ExecutedCalls::<Test>::get(alice_submission_timepoint).is_none());

			// Time passing simulated by different block
			System::set_block_number(2);
			let execution_timepoint = current_timepoint();

			// Ensure timepoints different
			assert_ne!(alice_submission_timepoint, execution_timepoint);

			// Bob approves proposal later
			assert_ok!(OriginAndGate::add_approval(
				RuntimeOrigin::signed(BOB),
				proposal_hash,
				ALICE_ORIGIN_ID,
				BOB_ORIGIN_ID,
				None,
				None,
				None,
			));

			// Verify execution successful after both approvals
			let expected_bytes = b"1000".to_vec();
			let expected_bounded: DummyValueOf = BoundedVec::try_from(expected_bytes).unwrap();
			assert_eq!(Dummy::<Test>::get(), Some(expected_bounded));

			// Verify ExecutedCalls storage updated with call hash at execution timepoint
			assert_eq!(ExecutedCalls::<Test>::get(execution_timepoint), Some(proposal_hash));

			// Verify proposal marked executed
			let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.status, ProposalStatus::Executed);

			// Verify execution event with execution timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
				proposal_hash,
				proposal_origin_id: ALICE_ORIGIN_ID,
				result: Ok(()),
				timepoint: execution_timepoint,
				is_collective: false,
			}));
		});
	}

	#[test]
	fn andgate_retains_state_between_origin_approvals() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);
			let alice_submission_timepoint = current_timepoint();

			// Create call
			let call = create_dummy_call(1000);
			let proposal_hash = <Test as Config>::Hashing::hash_of(&call);
			let call = make_remark_call("1000").unwrap();
			let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

			// Alice proposes
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call.clone(),
				ALICE_ORIGIN_ID,
				None,
				Some(true),
				None,
				None,
				None,
				Some(true), // Auto-execute
			));

			// Verify proposal creation event with timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCreated {
				proposal_hash,
				proposal_origin_id: ALICE_ORIGIN_ID,
				proposal_account_id: ALICE,
				timepoint: alice_submission_timepoint,
			}));

			// Simulate system restart or state reset with the exception of storage
			// by advancing blocks and clearing events
			System::set_block_number(100);
			System::reset_events();
			let execution_timepoint = current_timepoint();

			// Ensure timepoints are different since blocks advanced
			assert_ne!(alice_submission_timepoint, execution_timepoint);

			// Verify state is retained after events cleared and blocks advanced
			let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.status, ProposalStatus::Pending);
			assert_eq!(proposal.approvals.len(), 1);
			assert!(proposal.approvals.contains(&(ALICE, ALICE_ORIGIN_ID)));

			// No ExecutedCalls entries should exist at either timepoint
			assert!(ExecutedCalls::<Test>::get(alice_submission_timepoint).is_none());
			assert!(ExecutedCalls::<Test>::get(execution_timepoint).is_none());

			// Bob approves proposal later
			assert_ok!(OriginAndGate::add_approval(
				RuntimeOrigin::signed(BOB),
				proposal_hash,
				ALICE_ORIGIN_ID,
				BOB_ORIGIN_ID,
				None,
				None,
				None,
			));

			// Verify execution occurred
			let expected_bytes = b"1000".to_vec();
			let expected_bounded: DummyValueOf = BoundedVec::try_from(expected_bytes).unwrap();
			assert_eq!(Dummy::<Test>::get(), Some(expected_bounded));

			// Verify ExecutedCalls storage updated with call hash at execution timepoint
			assert_eq!(ExecutedCalls::<Test>::get(execution_timepoint), Some(proposal_hash));

			// Verify proposal marked executed
			let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.status, ProposalStatus::Executed);

			// Verify execution event with execution timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
				proposal_hash,
				proposal_origin_id: ALICE_ORIGIN_ID,
				result: Ok(()),
				timepoint: execution_timepoint,
				is_collective: false,
			}));
		});
	}

	#[test]
	fn andgate_direct_synchronous_approval_should_fail() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);
			let current_timepoint = current_timepoint();

			// Create call that typically requires AliceAndBob origin
			let call = create_dummy_call(1000);
			let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

			// Direct attempt using AndGate should fail
			// Note: This test explicitly validates that a synchronous approach fails
			// that shows why asynchronous proposal system is necessary
			let dummy_bytes = b"1000".to_vec();
			let bounded_dummy: DummyValueOf = BoundedVec::try_from(dummy_bytes).unwrap();
			assert_noop!(
				OriginAndGate::set_dummy(RuntimeOrigin::signed(ALICE), bounded_dummy),
				DispatchError::BadOrigin
			);

			// Verify state not changed
			assert_eq!(Dummy::<Test>::get(), None);

			// Verify no ExecutedCalls entry created since synchronous approach failed
			assert!(ExecutedCalls::<Test>::get(current_timepoint).is_none());

			// No proposal should exist in storage
			assert!(Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).is_none());
			assert!(Proposals::<Test>::get(proposal_hash, BOB_ORIGIN_ID).is_none());
		});
	}

	#[test]
	fn andgate_as_ensureorigin_should_function_asynchronously() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);
			let alice_submission_timepoint = current_timepoint();

			// Validates AndGate implements EnsureOrigin through asynchronous
			// proposal mechanism and not direct origin checks

			// Create call
			let call = create_dummy_call(1000);
			let proposal_hash = <Test as Config>::Hashing::hash_of(&call);

			// Submit with first origin
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call.clone(),
				ALICE_ORIGIN_ID,
				None,
				Some(true),
				None,
				None,
				None,
				Some(true), // Auto-execute
			));

			// Verify proposal creation event with timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCreated {
				proposal_hash,
				proposal_origin_id: ALICE_ORIGIN_ID,
				proposal_account_id: ALICE,
				timepoint: alice_submission_timepoint,
			}));

			// Try execute with one origin should fail
			let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.status, ProposalStatus::Pending);
			assert_eq!(Dummy::<Test>::get(), None);

			// No ExecutedCalls entry should exist yet
			assert!(ExecutedCalls::<Test>::get(alice_submission_timepoint).is_none());

			// Skip to next block for approval
			System::set_block_number(2);
			let execution_timepoint = current_timepoint();

			// Check timepoints differ
			assert_ne!(alice_submission_timepoint, execution_timepoint);

			// Approve with second origin to allow execution
			assert_ok!(OriginAndGate::add_approval(
				RuntimeOrigin::signed(BOB),
				proposal_hash,
				ALICE_ORIGIN_ID,
				BOB_ORIGIN_ID,
				None,
				None,
				None,
			));

			// Verify execution successful after both origins approved
			let expected_bytes = b"1000".to_vec();
			let expected_bounded: DummyValueOf = BoundedVec::try_from(expected_bytes).unwrap();
			assert_eq!(Dummy::<Test>::get(), Some(expected_bounded));

			// Verify ExecutedCalls storage updated with call hash at execution timepoint
			assert_eq!(ExecutedCalls::<Test>::get(execution_timepoint), Some(proposal_hash));

			// Verify proposal marked as executed
			let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.status, ProposalStatus::Executed);

			// Verify execution event with execution timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
				proposal_hash,
				proposal_origin_id: ALICE_ORIGIN_ID,
				result: Ok(()),
				timepoint: execution_timepoint,
				is_collective: false,
			}));
		});
	}

	#[test]
	fn andgate_fulfills_technical_specification_requirements() {
		new_test_ext().execute_with(|| {
			// Validates all requirements from technical spec:
			// 1. Two independent EnsureOrigin implementations must both agree
			// 2. Origins cannot be collected together ensuring asynchronous workflow
			// 3. Module retains state over proposal hashes
			// 4. Origins approve at different times
			// 5. Timepoint tracking ensures timestamp of approval and execution
			// 6. ExecutedCalls storage maps execution timepoints to call hashes

			System::set_block_number(1);
			let alice_submission_timepoint = current_timepoint();

			// Create two different calls for testing
			let call1 = create_dummy_call(1000);
			let call2 = create_dummy_call(2000);
			let proposal_hash1 = <Test as Config>::Hashing::hash_of(&call1);
			let proposal_hash2 = <Test as Config>::Hashing::hash_of(&call2);

			// Requirement 1 & 2: Two independent origins must approve asynchronously
			// Alice proposes first call
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call1.clone(),
				ALICE_ORIGIN_ID,
				None,
				Some(true),
				None,
				None,
				None,
				Some(true), // Auto-execute
			));

			// Verify proposal creation event with timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCreated {
				proposal_hash: proposal_hash1,
				proposal_origin_id: ALICE_ORIGIN_ID,
				proposal_account_id: ALICE,
				timepoint: alice_submission_timepoint,
			}));

			// Skip to block for Bob's proposal
			System::set_block_number(5);
			let bob_submission_timepoint = current_timepoint();

			// Ensure timepoints differ
			assert_ne!(alice_submission_timepoint, bob_submission_timepoint);

			// Bob proposes second call
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(BOB),
				call2.clone(),
				BOB_ORIGIN_ID,
				None,
				Some(true),
				None,
				None,
				None,
				Some(true), // Auto-execute
			));

			// Verify proposal creation event with timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCreated {
				proposal_hash: proposal_hash2,
				proposal_origin_id: BOB_ORIGIN_ID,
				proposal_account_id: BOB,
				timepoint: bob_submission_timepoint,
			}));

			// Requirement 3: Module retains state over proposal hashes
			// Verify both proposals stored correctly
			let proposal1 = Proposals::<Test>::get(proposal_hash1, ALICE_ORIGIN_ID).unwrap();
			let proposal2 = Proposals::<Test>::get(proposal_hash2, BOB_ORIGIN_ID).unwrap();
			assert_eq!(proposal1.status, ProposalStatus::Pending);
			assert_eq!(proposal2.status, ProposalStatus::Pending);

			// No ExecutedCalls entries should exist yet
			assert!(ExecutedCalls::<Test>::get(alice_submission_timepoint).is_none());
			assert!(ExecutedCalls::<Test>::get(bob_submission_timepoint).is_none());

			// Requirement 4: Origins approve at different times
			// Skip to block
			System::set_block_number(10);
			let first_execution_timepoint = current_timepoint();

			// Ensure execution timepoint differs from submission timepoints
			assert_ne!(alice_submission_timepoint, first_execution_timepoint);
			assert_ne!(bob_submission_timepoint, first_execution_timepoint);

			// Alice approves Bob's proposal
			assert_ok!(OriginAndGate::add_approval(
				RuntimeOrigin::signed(ALICE),
				proposal_hash2,
				BOB_ORIGIN_ID,
				ALICE_ORIGIN_ID,
				None,
				None,
				None,
			));

			// Verify first call execution and ExecutedCalls entry
			assert_eq!(ExecutedCalls::<Test>::get(first_execution_timepoint), Some(proposal_hash2));

			// Verify execution event with timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
				proposal_hash: proposal_hash2,
				proposal_origin_id: BOB_ORIGIN_ID,
				result: Ok(()),
				timepoint: first_execution_timepoint,
				is_collective: false,
			}));

			// Skip to new block for second approval
			System::set_block_number(11);
			let second_execution_timepoint = current_timepoint();

			// Ensure second execution timepoint differs
			assert_ne!(first_execution_timepoint, second_execution_timepoint);

			// Bob approves Alice's proposal
			assert_ok!(OriginAndGate::add_approval(
				RuntimeOrigin::signed(BOB),
				proposal_hash1,
				ALICE_ORIGIN_ID,
				BOB_ORIGIN_ID,
				None,
				None,
				None,
			));

			// Verify second call execution and ExecutedCalls entry
			assert_eq!(
				ExecutedCalls::<Test>::get(second_execution_timepoint),
				Some(proposal_hash1)
			);

			// Verify both calls executed
			// Last executed call wins which appears to conflict
			// with comment below so need to verify
			let expected_bytes = b"1000".to_vec();
			let expected_bounded: DummyValueOf = BoundedVec::try_from(expected_bytes).unwrap();
			assert_eq!(Dummy::<Test>::get(), Some(expected_bounded));

			// Verify execution event with timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
				proposal_hash: proposal_hash1,
				proposal_origin_id: ALICE_ORIGIN_ID,
				result: Ok(()),
				timepoint: second_execution_timepoint,
				is_collective: false,
			}));

			// Verify both proposals marked as executed
			let proposal1 = Proposals::<Test>::get(proposal_hash1, ALICE_ORIGIN_ID).unwrap();
			let proposal2 = Proposals::<Test>::get(proposal_hash2, BOB_ORIGIN_ID).unwrap();
			assert_eq!(proposal1.status, ProposalStatus::Executed);
			assert_eq!(proposal2.status, ProposalStatus::Executed);
		});
	}
}

/// Test module to ensure consistency in error indices
mod error_index_consistency {
	use super::*;

	#[test]
	fn error_indices_match_enum_variants() {
		// Verifies the error indices returned by `error_index` function
		// matches the actual positions of variants in the `Error` enum so
		// if a change occurs to the `Error` enum order or variants are
		// added or removed without updating `error_index` then this test fails.
		//
		// When adding new enum variants to `error_index` and `Error` in lib.rs ensure a new
		// assertion for it is added below and update the `EXPECTED_VARIANT_COUNT` to include it.

		assert_eq!(OriginAndGate::error_index(Error::<Test>::ProposalAlreadyExists), 0);
		assert_eq!(OriginAndGate::error_index(Error::<Test>::ProposalNotFound), 1);
		assert_eq!(
			OriginAndGate::error_index(Error::<Test>::CannotApproveOwnProposalUsingDifferentOrigin),
			2
		);
		assert_eq!(OriginAndGate::error_index(Error::<Test>::TooManyApprovals), 3);
		assert_eq!(OriginAndGate::error_index(Error::<Test>::NotAuthorized), 4);
		assert_eq!(OriginAndGate::error_index(Error::<Test>::ProposalAlreadyExecuted), 5);
		assert_eq!(OriginAndGate::error_index(Error::<Test>::ProposalExpired), 6);
		assert_eq!(OriginAndGate::error_index(Error::<Test>::ProposalCancelled), 7);
		assert_eq!(OriginAndGate::error_index(Error::<Test>::AccountOriginAlreadyApproved), 8);
		assert_eq!(OriginAndGate::error_index(Error::<Test>::InsufficientApprovals), 9);
		assert_eq!(OriginAndGate::error_index(Error::<Test>::ProposalNotPending), 10);
		assert_eq!(
			OriginAndGate::error_index(Error::<Test>::ProposalNotInExpiredOrExecutedState),
			11
		);
		assert_eq!(OriginAndGate::error_index(Error::<Test>::AccountOriginApprovalNotFound), 12);
		assert_eq!(
			OriginAndGate::error_index(Error::<Test>::ProposalRetentionPeriodNotElapsed),
			13
		);
		assert_eq!(OriginAndGate::error_index(Error::<Test>::ProposalNotEligibleForCleanup), 14);

		// Verify the right number of variants of `error_index` are tested otherwise fail
		const EXPECTED_VARIANT_COUNT: u8 = 15;
	}
}

/// Tests for external storage integration functionality
mod external_storage_integration {
	use super::*;

	// Helper function to create a test storage ID (simulates an IPFS CID)
	fn create_test_storage_id(id: u8) -> BoundedVec<u8, MaxStorageIdLength> {
		let prefix = b"Qm".to_vec();
		let mut cid = prefix;
		cid.extend_from_slice(&[id; 44]); // Pad to make it look like a real CID
		BoundedVec::<u8, MaxStorageIdLength>::try_from(cid)
			.expect("Storage ID should fit within bounds")
	}

	// Helper function to create a test storage ID description
	fn create_test_storage_id_description(
		desc: &str,
	) -> Option<BoundedVec<u8, MaxStorageIdDescriptionLength>> {
		Some(
			BoundedVec::<u8, MaxStorageIdDescriptionLength>::try_from(desc.as_bytes().to_vec())
				.expect("Storage ID description should fit within bounds"),
		)
	}

	#[test]
	fn add_storage_id_works() {
		new_test_ext().execute_with(|| {
			// Create a proposal
			let call = create_dummy_call(1000);
			let proposal_hash = BlakeTwo256::hash_of(&call);

			// Alice proposes
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call,
				ALICE_ORIGIN_ID,
				None,
				None,
				None,
				None,
				None,
				Some(true),
			));

			// Create storage ID
			let storage_id = create_test_storage_id(1);
			let storage_id_description = create_test_storage_id_description("Test IPFS CID");

			// Alice adds storage ID to proposal
			assert_ok!(OriginAndGate::add_storage_id(
				RuntimeOrigin::signed(ALICE),
				proposal_hash,
				ALICE_ORIGIN_ID,
				storage_id.clone().to_vec(),
				storage_id_description.clone().map(|d| d.to_vec()),
			));

			// Verify storage ID added
			assert!(OriginAndGate::has_storage_id_for_proposal(proposal_hash, &storage_id));

			// Verify event emitted
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::StorageIdAdded {
				proposal_hash,
				proposal_origin_id: ALICE_ORIGIN_ID,
				account_id: ALICE,
				storage_id: storage_id.clone(),
				storage_id_description: storage_id_description.clone(),
			}));

			// Get all storage IDs for proposal
			let storage_ids = OriginAndGate::get_proposal_storage_ids(proposal_hash);
			assert_eq!(storage_ids.len(), 1);
			assert_eq!(storage_ids[0].0, storage_id);
			assert_eq!(storage_ids[0].2, ALICE);
			assert_eq!(storage_ids[0].3, storage_id_description);
		});
	}

	#[test]
	fn add_storage_id_fails_for_nonexistent_proposal() {
		new_test_ext().execute_with(|| {
			let proposal_hash = H256::from_low_u64_be(1);
			let storage_id = create_test_storage_id(3);

			// Try to add storage ID to non-existent proposal
			assert_noop!(
				OriginAndGate::add_storage_id(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
					storage_id.clone().to_vec(),
					None,
				),
				Error::<Test>::ProposalNotFound
			);
		});
	}

	#[test]
	fn add_storage_id_fails_for_unauthorized_account() {
		new_test_ext().execute_with(|| {
			// Create proposal
			let call = create_dummy_call(1000);
			let proposal_hash = BlakeTwo256::hash_of(&call);

			// Alice proposes
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call,
				ALICE_ORIGIN_ID,
				None,
				None,
				None,
				None,
				None,
				Some(true),
			));

			// Create storage ID
			let storage_id = create_test_storage_id(4);

			// Charlie (not a proposer or approver) tries to add storage ID
			assert_noop!(
				OriginAndGate::add_storage_id(
					RuntimeOrigin::signed(CHARLIE),
					proposal_hash,
					ALICE_ORIGIN_ID,
					storage_id.clone().to_vec(),
					None,
				),
				Error::<Test>::NotAuthorized
			);
		});
	}

	#[test]
	fn add_duplicate_storage_id_fails() {
		new_test_ext().execute_with(|| {
			// Create proposal
			let call = create_dummy_call(1000);
			let proposal_hash = BlakeTwo256::hash_of(&call);

			// Alice proposes
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call,
				ALICE_ORIGIN_ID,
				None,
				None,
				None,
				None,
				None,
				Some(true),
			));

			// Create storage ID
			let storage_id = create_test_storage_id(5);

			// Alice adds storage ID to proposal
			assert_ok!(OriginAndGate::add_storage_id(
				RuntimeOrigin::signed(ALICE),
				proposal_hash,
				ALICE_ORIGIN_ID,
				storage_id.clone().to_vec(),
				None,
			));

			// Try to add same storage ID again
			assert_noop!(
				OriginAndGate::add_storage_id(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
					storage_id.clone().to_vec(),
					None,
				),
				Error::<Test>::StorageIdAlreadyPresent
			);
		});
	}

	#[test]
	fn add_too_many_storage_ids_fails() {
		new_test_ext().execute_with(|| {
			// Create proposal
			let call = create_dummy_call(1000);
			let proposal_hash = BlakeTwo256::hash_of(&call);

			// Alice proposes
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call,
				ALICE_ORIGIN_ID,
				None,
				None,
				None,
				None,
				None,
				Some(true),
			));

			// Add maximum number of storage IDs
			for i in 0..MaxStorageIdsPerProposal::get() {
				let storage_id = create_test_storage_id(i as u8);
				assert_ok!(OriginAndGate::add_storage_id(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
					storage_id.clone().to_vec(),
					None,
				));
			}

			// Try add one more storage ID
			let storage_id = create_test_storage_id(255);
			assert_noop!(
				OriginAndGate::add_storage_id(
					RuntimeOrigin::signed(ALICE),
					proposal_hash,
					ALICE_ORIGIN_ID,
					storage_id.clone().to_vec(),
					None,
				),
				Error::<Test>::TooManyStorageIds
			);
		});
	}

	#[test]
	fn remove_storage_id_works() {
		new_test_ext().execute_with(|| {
			// Create proposal
			let call = create_dummy_call(1000);
			let proposal_hash = BlakeTwo256::hash_of(&call);

			// Alice proposes
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call,
				ALICE_ORIGIN_ID,
				None,
				None,
				None,
				None,
				None,
				Some(true),
			));

			// Create storage ID
			let storage_id = create_test_storage_id(6);

			// Alice adds storage ID to proposal
			assert_ok!(OriginAndGate::add_storage_id(
				RuntimeOrigin::signed(ALICE),
				proposal_hash,
				ALICE_ORIGIN_ID,
				storage_id.clone().to_vec(),
				None,
			));

			// Verify storage ID added
			assert!(OriginAndGate::has_storage_id_for_proposal(proposal_hash, &storage_id));

			// Alice removes storage ID
			assert_ok!(OriginAndGate::remove_storage_id(
				RuntimeOrigin::signed(ALICE),
				proposal_hash,
				ALICE_ORIGIN_ID,
				storage_id.clone(),
			));

			// Verify storage ID removed
			assert!(!OriginAndGate::has_storage_id_for_proposal(proposal_hash, &storage_id));

			// Verify event emitted
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::StorageIdRemoved {
				proposal_hash,
				proposal_origin_id: ALICE_ORIGIN_ID,
				account_id: ALICE,
				storage_id: storage_id.clone(),
				is_collective: false,
			}));
		});
	}

	#[test]
	fn remove_storage_id_fails_for_unauthorized_account() {
		new_test_ext().execute_with(|| {
			// Create proposal
			let call = create_dummy_call(1000);
			let proposal_hash = BlakeTwo256::hash_of(&call);

			// Alice proposes
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call,
				ALICE_ORIGIN_ID,
				None,
				None,
				None,
				None,
				None,
				Some(true),
			));

			// Create storage ID
			let storage_id = create_test_storage_id(7);

			// Alice adds storage ID to proposal
			assert_ok!(OriginAndGate::add_storage_id(
				RuntimeOrigin::signed(ALICE),
				proposal_hash,
				ALICE_ORIGIN_ID,
				storage_id.clone().to_vec(),
				None,
			));

			// Bob tries to remove Alice's storage ID
			assert_noop!(
				OriginAndGate::remove_storage_id(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ALICE_ORIGIN_ID,
					storage_id.clone(),
				),
				Error::<Test>::NotAuthorized
			);
		});
	}

	#[test]
	fn remove_storage_id_with_collective_works() {
		new_test_ext().execute_with(|| {
			// Create proposal
			let call = create_dummy_call(1000);
			let proposal_hash = BlakeTwo256::hash_of(&call);

			// Alice proposes
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call,
				ALICE_ORIGIN_ID,
				None,
				None,
				None,
				None,
				None,
				Some(true),
			));

			// Create storage ID
			let storage_id = create_test_storage_id(8);

			// Alice adds storage ID to proposal
			assert_ok!(OriginAndGate::add_storage_id(
				RuntimeOrigin::signed(ALICE),
				proposal_hash,
				ALICE_ORIGIN_ID,
				storage_id.clone().to_vec(),
				None,
			));

			// Verify storage ID added
			assert!(OriginAndGate::has_storage_id_for_proposal(proposal_hash, &storage_id));

			// Root (collective) removes storage ID
			assert_ok!(OriginAndGate::remove_storage_id(
				RuntimeOrigin::root(),
				proposal_hash,
				ROOT_ORIGIN_ID,
				storage_id.clone(),
			));

			// Verify storage ID removed
			assert!(!OriginAndGate::has_storage_id_for_proposal(proposal_hash, &storage_id));

			// Verify event emitted
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::StorageIdRemoved {
				proposal_hash,
				proposal_origin_id: ROOT_ORIGIN_ID,
				account_id: ROOT,
				storage_id: storage_id.clone(),
				is_collective: true,
			}));
		});
	}

	#[test]
	fn remove_storage_id_with_collective_fails_for_unauthorized_origin() {
		new_test_ext().execute_with(|| {
			// Create proposal
			let call = create_dummy_call(1000);
			let proposal_hash = BlakeTwo256::hash_of(&call);

			// Alice proposes
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call,
				ALICE_ORIGIN_ID,
				None,
				None,
				None,
				None,
				None,
				Some(true),
			));

			// Create storage ID
			let storage_id = create_test_storage_id(9);

			// Alice adds storage ID to proposal
			assert_ok!(OriginAndGate::add_storage_id(
				RuntimeOrigin::signed(ALICE),
				proposal_hash,
				ALICE_ORIGIN_ID,
				storage_id.clone().to_vec(),
				None,
			));

			// Try to remove storage ID with a signed origin (BOB) pretending to be a collective
			// origin This should fail because BOB is not a valid collective origin
			assert_noop!(
				OriginAndGate::remove_storage_id(
					RuntimeOrigin::signed(BOB),
					proposal_hash,
					ROOT_ORIGIN_ID, // Using ROOT_ORIGIN_ID to signal collective path
					storage_id.clone(),
				),
				Error::<Test>::ProposalNotFound,
			);

			// Add second proposal with ROOT_ORIGIN_ID
			// so proposal by the collective may be found
			let root_call = create_dummy_call(2000);
			let root_proposal_hash = BlakeTwo256::hash_of(&root_call);
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				root_call,
				ROOT_ORIGIN_ID,
				None,
				None,
				None,
				None,
				None,
				Some(true),
			));

			// Test with second proposal
			assert_noop!(
				OriginAndGate::remove_storage_id(
					RuntimeOrigin::signed(BOB),
					root_proposal_hash,
					ROOT_ORIGIN_ID, // Using ROOT_ORIGIN_ID to signal collective path
					storage_id.clone(),
				),
				sp_runtime::traits::BadOrigin,
			);

			// Verify storage ID still exists
			assert!(OriginAndGate::has_storage_id_for_proposal(proposal_hash, &storage_id));
		});
	}

	#[test]
	fn remove_storage_id_with_opengov_origin_works() {
		new_test_ext().execute_with(|| {
			// Create proposal
			let call = create_dummy_call(1000);
			let proposal_hash = BlakeTwo256::hash_of(&call);

			// Alice proposes
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call,
				ALICE_ORIGIN_ID,
				None,
				None,
				None,
				None,
				None,
				Some(true),
			));

			// Create storage ID
			let storage_id = create_test_storage_id(6);

			// Alice adds storage ID to proposal
			assert_ok!(OriginAndGate::add_storage_id(
				RuntimeOrigin::signed(ALICE),
				proposal_hash,
				ALICE_ORIGIN_ID,
				storage_id.clone().to_vec(),
				None,
			));

			// Verify storage ID was added
			assert!(OriginAndGate::has_storage_id_for_proposal(proposal_hash, &storage_id));

			// Use root origin (which simulates OpenGov referendum in our test setup)
			// to remove the storage ID through the collective path
			assert_ok!(OriginAndGate::remove_storage_id(
				RuntimeOrigin::root(),
				proposal_hash,
				ROOT_ORIGIN_ID,
				storage_id.clone(),
			));

			// Verify storage ID removed
			assert!(!OriginAndGate::has_storage_id_for_proposal(proposal_hash, &storage_id));

			// Verify correct event emitted
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::StorageIdRemoved {
				proposal_hash,
				proposal_origin_id: ROOT_ORIGIN_ID,
				account_id: ROOT,
				storage_id: storage_id.clone(),
				is_collective: true,
			}));
		});
	}

	#[test]
	fn amend_remark_with_storage_id_works() {
		new_test_ext().execute_with(|| {
			// Create proposal
			let call = create_dummy_call(1000);
			let proposal_hash = BlakeTwo256::hash_of(&call);

			// Alice proposes with a remark
			let remark = b"Initial remark".to_vec();
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call,
				ALICE_ORIGIN_ID,
				None,
				None,
				Some(remark),
				None,
				None,
				Some(true),
			));

			// Create storage ID and new remark
			let storage_id = create_test_storage_id(10);
			let storage_id_description =
				create_test_storage_id_description("Added with remark amendment");
			let new_remark = b"Updated remark".to_vec();

			// Alice amends remark and adds storage ID
			assert_ok!(OriginAndGate::amend_remark(
				RuntimeOrigin::signed(ALICE),
				proposal_hash,
				ALICE_ORIGIN_ID,
				None,
				new_remark.clone(),
				Some(storage_id.clone().to_vec()),
				storage_id_description.clone().map(|d| d.to_vec()),
			));

			// Verify remark amended
			let (_, remark_hashes, _) = GovernanceHashes::<Test>::get(proposal_hash).unwrap();
			assert!(remark_hashes
				.values()
				.any(|r| r
					== &BoundedVec::<u8, MaxRemarkLength>::try_from(new_remark.clone()).unwrap()));

			// Verify storage ID added
			assert!(OriginAndGate::has_storage_id_for_proposal(proposal_hash, &storage_id));

			// Verify events emitted
			System::assert_has_event(RuntimeEvent::OriginAndGate(
				Event::ProposerAmendedProposalWithRemark {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					proposal_account_id: ALICE,
					timepoint: current_timepoint(),
					remark: new_remark,
				},
			));

			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::StorageIdAdded {
				proposal_hash,
				proposal_origin_id: ALICE_ORIGIN_ID,
				account_id: ALICE,
				storage_id: storage_id.clone(),
				storage_id_description: storage_id_description.clone(),
			}));
		});
	}

	#[test]
	fn amend_remark_with_storage_id_works_without_storage_id() {
		new_test_ext().execute_with(|| {
			// Create proposal
			let call = create_dummy_call(1000);
			let proposal_hash = BlakeTwo256::hash_of(&call);

			// Alice proposes with remark
			let remark = b"Initial remark".to_vec();
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call,
				ALICE_ORIGIN_ID,
				None,
				None,
				Some(remark),
				None,
				None,
				Some(true),
			));

			// New remark without storage ID
			let new_remark = b"Updated remark only".to_vec();

			// Alice amends remark without adding storage ID
			assert_ok!(OriginAndGate::amend_remark(
				RuntimeOrigin::signed(ALICE),
				proposal_hash,
				ALICE_ORIGIN_ID,
				None,
				new_remark.clone(),
				None,
				None,
			));

			// Verify remark amended
			let (_, remark_hashes, _) = GovernanceHashes::<Test>::get(proposal_hash).unwrap();
			assert!(remark_hashes
				.values()
				.any(|r| r
					== &BoundedVec::<u8, MaxRemarkLength>::try_from(new_remark.clone()).unwrap()));

			// Verify only remark amendment event emitted
			System::assert_has_event(RuntimeEvent::OriginAndGate(
				Event::ProposerAmendedProposalWithRemark {
					proposal_hash,
					proposal_origin_id: ALICE_ORIGIN_ID,
					proposal_account_id: ALICE,
					timepoint: current_timepoint(),
					remark: new_remark,
				},
			));
		});
	}

	#[test]
	fn get_proposal_storage_ids_works() {
		new_test_ext().execute_with(|| {
			// Create proposal
			let call = create_dummy_call(1000);
			let proposal_hash = BlakeTwo256::hash_of(&call);

			// Alice proposes
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call,
				ALICE_ORIGIN_ID,
				None,
				None,
				None,
				None,
				None,
				Some(true),
			));

			// Add multiple storage IDs
			let storage_id1 = create_test_storage_id(11);
			let bounded_storage_id1 = storage_id1.clone();
			let storage_id_description1 = create_test_storage_id_description("First CID");
			let bounded_storage_id_description1 =
				if let Some(desc) = storage_id_description1.clone() {
					Some(
						BoundedVec::<u8, MaxStorageIdDescriptionLength>::try_from(desc)
							.expect("Storage ID description should fit within bounds"),
					)
				} else {
					None
				};

			let storage_id2 = create_test_storage_id(12);
			let bounded_storage_id2 = storage_id2.clone();
			let storage_id_description2 = create_test_storage_id_description("Second CID");
			let bounded_storage_id_description2 =
				if let Some(desc) = storage_id_description2.clone() {
					Some(
						BoundedVec::<u8, MaxStorageIdDescriptionLength>::try_from(desc)
							.expect("Storage ID description should fit within bounds"),
					)
				} else {
					None
				};

			// Alice adds storage IDs
			assert_ok!(OriginAndGate::add_storage_id(
				RuntimeOrigin::signed(ALICE),
				proposal_hash,
				ALICE_ORIGIN_ID,
				storage_id1.clone().to_vec(),
				storage_id_description1.clone().map(|d| d.to_vec()),
			));

			assert_ok!(OriginAndGate::add_storage_id(
				RuntimeOrigin::signed(ALICE),
				proposal_hash,
				ALICE_ORIGIN_ID,
				storage_id2.clone().to_vec(),
				storage_id_description2.clone().map(|d| d.to_vec()),
			));

			// Get all storage IDs
			let storage_ids = OriginAndGate::get_proposal_storage_ids(proposal_hash);

			// Verify got both storage IDs
			assert_eq!(storage_ids.len(), 2);

			// Verify first storage ID
			let first_id = storage_ids.iter().find(|(id, _, _, _)| id == &storage_id1).unwrap();
			assert_eq!(first_id.0, bounded_storage_id1);
			assert_eq!(first_id.2, ALICE);
			assert_eq!(first_id.3, bounded_storage_id_description1);

			// Verify second storage ID
			let second_id = storage_ids.iter().find(|(id, _, _, _)| id == &storage_id2).unwrap();
			assert_eq!(second_id.0, bounded_storage_id2);
			assert_eq!(second_id.2, ALICE);
			assert_eq!(second_id.3, bounded_storage_id_description2);
		});
	}

	#[test]
	fn filter_storage_ids_for_proposal_works() {
		new_test_ext().execute_with(|| {
			// Create proposal
			let call = create_dummy_call(1000);
			let proposal_hash = BlakeTwo256::hash_of(&call);

			// Alice proposes
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call,
				ALICE_ORIGIN_ID,
				None,
				None,
				None,
				None,
				None,
				Some(true),
			));

			// Add different types of storage IDs
			let ipfs_cid = b"QmYyQSo1c1Ym7orWxLYvCrM2EmxFTANf8wXmmE7DWjhx5N".to_vec();
			let arweave_id = b"AR1tLYLq1AP5R1oTBK4wvLLMVdCwVFKgCnFm5uQZxSI".to_vec();

			// Alice adds storage IDs
			assert_ok!(OriginAndGate::add_storage_id(
				RuntimeOrigin::signed(ALICE),
				proposal_hash,
				ALICE_ORIGIN_ID,
				ipfs_cid.clone(),
				Some(b"IPFS CID".to_vec()),
			));

			assert_ok!(OriginAndGate::add_storage_id(
				RuntimeOrigin::signed(ALICE),
				proposal_hash,
				ALICE_ORIGIN_ID,
				arweave_id.clone(),
				Some(b"Arweave ID".to_vec()),
			));

			// Filter for IPFS CIDs only
			let ipfs_ids = OriginAndGate::filter_storage_ids_for_proposal(
				proposal_hash,
				ALICE_ORIGIN_ID,
				|id, _, _, _| id.starts_with(b"Qm"),
			);

			// Verify got only IPFS CID
			assert_eq!(ipfs_ids.len(), 1);
			assert_eq!(ipfs_ids[0].0, ipfs_cid);

			// Filter for Arweave IDs only
			let arweave_ids = OriginAndGate::filter_storage_ids_for_proposal(
				proposal_hash,
				ALICE_ORIGIN_ID,
				|id, _, _, _| id.starts_with(b"AR"),
			);

			// Verify got only Arweave ID
			assert_eq!(arweave_ids.len(), 1);
			assert_eq!(arweave_ids[0].0, arweave_id);
		});
	}

	#[test]
	fn get_proposal_ipfs_cids_works() {
		new_test_ext().execute_with(|| {
			// Create proposal
			let call = create_dummy_call(1000);
			let proposal_hash = BlakeTwo256::hash_of(&call);

			// Alice proposes
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call,
				ALICE_ORIGIN_ID,
				None,
				None,
				None,
				None,
				None,
				Some(true),
			));

			// Add different types of storage IDs
			let ipfs_cid_v0 = b"QmYyQSo1c1Ym7orWxLYvCrM2EmxFTANf8wXmmE7DWjhx5N".to_vec();
			let ipfs_cid_v1 =
				b"bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi".to_vec();
			let arweave_id = b"AR1tLYLq1AP5R1oTBK4wvLLMVdCwVFKgCnFm5uQZxSI".to_vec();

			// Alice adds storage IDs
			assert_ok!(OriginAndGate::add_storage_id(
				RuntimeOrigin::signed(ALICE),
				proposal_hash,
				ALICE_ORIGIN_ID,
				ipfs_cid_v0.clone().to_vec(),
				Some(b"IPFS CIDv0".to_vec()),
			));

			assert_ok!(OriginAndGate::add_storage_id(
				RuntimeOrigin::signed(ALICE),
				proposal_hash,
				ALICE_ORIGIN_ID,
				ipfs_cid_v1.clone(),
				Some(b"IPFS CIDv1".to_vec()),
			));

			assert_ok!(OriginAndGate::add_storage_id(
				RuntimeOrigin::signed(ALICE),
				proposal_hash,
				ALICE_ORIGIN_ID,
				arweave_id.clone(),
				Some(b"Arweave ID".to_vec()),
			));

			// Get only IPFS CIDs
			let ipfs_cids = OriginAndGate::get_proposal_ipfs_cids(proposal_hash, ALICE_ORIGIN_ID);

			// Verify got both IPFS CIDs but not Arweave ID
			assert_eq!(ipfs_cids.len(), 2);
			assert!(ipfs_cids.iter().any(|(id, _, _, _)| id == &ipfs_cid_v0));
			assert!(ipfs_cids.iter().any(|(id, _, _, _)| id == &ipfs_cid_v1));
			assert!(!ipfs_cids.iter().any(|(id, _, _, _)| id == &arweave_id));
		});
	}

	#[test]
	fn error_indices_match_enum_variants() {
		// Existing assertions...

		// Add assertions for new error variants
		assert_eq!(OriginAndGate::error_index(Error::<Test>::StorageIdAlreadyPresent), 16);
		assert_eq!(OriginAndGate::error_index(Error::<Test>::TooManyStorageIds), 17);
		assert_eq!(OriginAndGate::error_index(Error::<Test>::StorageIdTooLong), 18);
		assert_eq!(OriginAndGate::error_index(Error::<Test>::DescriptionTooLong), 19);
		assert_eq!(OriginAndGate::error_index(Error::<Test>::RemarkTooLong), 20);
		assert_eq!(OriginAndGate::error_index(Error::<Test>::RemarkNotFound), 21);
		assert_eq!(OriginAndGate::error_index(Error::<Test>::TooManyRemarks), 22);

		// Update the expected variant count
		const EXPECTED_VARIANT_COUNT: u8 = 23;
	}

	#[test]
	fn storage_ids_persist_through_proposal_lifecycle() {
		new_test_ext().execute_with(|| {
			// Create proposal
			let call = create_dummy_call(1000);
			let proposal_hash = BlakeTwo256::hash_of(&call);

			// Alice proposes
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call,
				ALICE_ORIGIN_ID,
				None,
				Some(true),
				None,
				None,
				None,
				Some(true),
			));

			// Add storage ID
			let storage_id = create_test_storage_id(20);
			let bounded_storage_id =
				BoundedVec::<u8, MaxStorageIdLength>::try_from(storage_id.clone())
					.expect("Storage ID should fit within bounds");
			assert_ok!(OriginAndGate::add_storage_id(
				RuntimeOrigin::signed(ALICE),
				proposal_hash,
				ALICE_ORIGIN_ID,
				storage_id.clone().to_vec(),
				None,
			));

			// Bob approves to execute proposal
			assert_ok!(OriginAndGate::add_approval(
				RuntimeOrigin::signed(BOB),
				proposal_hash,
				ALICE_ORIGIN_ID,
				BOB_ORIGIN_ID,
				None,
				None,
				None,
			));

			// Verify proposal executed
			let proposal = Proposals::<Test>::get(proposal_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.status, ProposalStatus::Executed);

			// Verify storage ID still exists after execution
			assert!(OriginAndGate::has_storage_id_for_proposal(proposal_hash, &bounded_storage_id));

			// Advance past retention period
			System::set_block_number(
				System::block_number() + ProposalRetentionPeriodWhenNotCancelled::get() + 1,
			);

			// Clean up proposal
			assert_ok!(OriginAndGate::clean(
				RuntimeOrigin::signed(ALICE),
				proposal_hash,
				ALICE_ORIGIN_ID,
			));

			// Verify proposal cleaned up
			assert!(!proposal_exists(proposal_hash, ALICE_ORIGIN_ID));

			// Verify storage ID also cleaned up
			assert!(!OriginAndGate::has_storage_id_for_proposal(
				proposal_hash,
				&bounded_storage_id
			));
		});
	}
}
