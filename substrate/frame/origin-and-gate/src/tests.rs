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
fn proposal_exists(proposal_hash: H256, origin_id: OriginId) -> bool {
	Proposals::<Test>::get(proposal_hash, origin_id).is_some()
}

/// Helper function to check if proposal call exists in storage
fn proposal_call_exists(proposal_hash: H256) -> bool {
	ProposalCalls::<Test>::get(proposal_hash).is_some()
}

/// Helper function to check if any approvals exist for a proposal
fn proposal_has_approvals(proposal_hash: H256, origin_id: OriginId) -> bool {
	// Cannot directly check if any entries exist with a specific prefix in a double map
	// Testing purposes only simplification since in a real scenario we might need
	// a more sophisticated approach
	let proposal = Proposals::<Test>::get(proposal_hash, origin_id);
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
				let call_hash =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

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
				));

				// Verify proposal stored
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);
				assert_eq!(proposal.approvals.len(), 1);
				assert_eq!(proposal.approvals[0], (ALICE, ALICE_ORIGIN_ID));

				// Verify proposal creation event with timepoint
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCreated {
					proposal_hash: call_hash,
					origin_id: ALICE_ORIGIN_ID,
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
		fn propose_after_expiry_fails() {
			new_test_ext().execute_with(|| {
				let starting_block = 1;
				// Override the `ProposalExpiry` value of the runtime in mock
				System::set_block_number(starting_block);

				let call = make_remark_call("1000").unwrap();
				let call_hash = <Test as Config>::Hashing::hash_of(&call);
				// Expire after 10 blocks
				let expiry = Some(starting_block + <Test as Config>::ProposalExpiry::get());

				System::set_block_number(expiry.unwrap() + 1);

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
	}

	mod cancel_proposal {
		use super::*;

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
					timepoint: current_timepoint(),
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
				let mut approvals = BoundedVec::<(AccountId, OriginId), MaxApprovals>::default();
				approvals.try_push((ALICE, ALICE_ORIGIN_ID)).unwrap();
				approvals.try_push((BOB, BOB_ORIGIN_ID)).unwrap(); // Already have 2 approvals
				let executed_at = Some(System::block_number());

				let proposal_info = ProposalInfo {
					call_hash: call_hash2,
					expiry: None,
					approvals,
					status: ProposalStatus::Executed,
					proposer: ALICE,
					executed_at,
					submitted_at: System::block_number(),
				};

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
				assert!(
					Approvals::<Test>::get((call_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID).is_some()
				);

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
					timepoint: current_timepoint(),
				}));

				// Verify proposal no longer exists
				assert!(!Proposals::<Test>::contains_key(call_hash, ALICE_ORIGIN_ID));

				// Verify proposal calls no longer exists
				assert!(!ProposalCalls::<Test>::contains_key(call_hash));

				// Verify approvals storage is also cleaned up
				assert!(
					Approvals::<Test>::get((call_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID).is_none()
				);

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
				assert!(
					Approvals::<Test>::get((call_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID).is_some()
				);

				// Cancel proposal
				assert_ok!(OriginAndGate::cancel_proposal(
					RuntimeOrigin::signed(ALICE),
					call_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify all storage is cleaned up after cancellation
				assert!(!Proposals::<Test>::contains_key(call_hash, ALICE_ORIGIN_ID));
				assert!(!ProposalCalls::<Test>::contains_key(call_hash));
				assert!(
					Approvals::<Test>::get((call_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID).is_none()
				);

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
				assert!(
					Approvals::<Test>::get((call_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID).is_some()
				);
			});
		}

		#[test]
		fn proposal_cancellation_not_allowed_for_executed_status() {
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
				let mut approvals = BoundedVec::<(AccountId, OriginId), MaxApprovals>::default();
				approvals.try_push((ALICE, ALICE_ORIGIN_ID)).unwrap();
				approvals.try_push((BOB, BOB_ORIGIN_ID)).unwrap(); // Already have 2 approvals
				let executed_at = Some(System::block_number());

				let proposal_info = ProposalInfo {
					call_hash,
					expiry: None,
					approvals,
					status: ProposalStatus::Executed,
					proposer: ALICE,
					executed_at,
					submitted_at: System::block_number(),
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
	}

	mod add_approval {
		use super::*;

		#[test]
		fn approve_of_proposal_adds_approval() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create call
				let call = create_dummy_call(1000);
				let call_hash =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

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
					true, // Auto-execute
				));

				// Verify approval added
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.approvals.len(), MaxApprovals::get() as usize);
				assert!(proposal.approvals.contains(&(BOB, BOB_ORIGIN_ID)));

				// Verify event emitted
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::OriginApprovalAdded {
					proposal_hash: call_hash,
					origin_id: ALICE_ORIGIN_ID,
					approving_origin_id: BOB_ORIGIN_ID,
					timepoint: current_timepoint(),
				}));
			});
		}

		#[test]
		fn cannot_approve_own_proposal_with_different_origin_id() {
			new_test_ext().execute_with(|| {
				let call = make_remark_call("1000").unwrap();
				let call_hash = <Test as Config>::Hashing::hash_of(&call);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
				));

				// Alice tries to approve with different origin ID should fail
				assert_noop!(
					OriginAndGate::add_approval(
						RuntimeOrigin::signed(ALICE),
						call_hash,
						ALICE_ORIGIN_ID,
						BOB_ORIGIN_ID,
						true, // Auto-execute
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
				let call_hash =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

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
						true, // Auto-execute
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
						true, // Auto-execute
					),
					Error::<Test>::ProposalNotFound
				);
			});
		}

		#[test]
		fn approve_with_wrong_call_hash_fails_to_find_proposal() {
			new_test_ext().execute_with(|| {
				let call = make_remark_call("1000").unwrap();
				let call_hash = <Test as Config>::Hashing::hash_of(&call);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
				));

				// Use different call hash
				let wrong_call = make_remark_call("2000").unwrap();
				let wrong_call_hash = <Test as Config>::Hashing::hash_of(&wrong_call);

				// Bob tries approve but should fail
				assert_noop!(
					OriginAndGate::add_approval(
						RuntimeOrigin::signed(BOB),
						wrong_call_hash,
						ALICE_ORIGIN_ID,
						BOB_ORIGIN_ID,
						true, // Auto-execute
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
				let call_hash = <Test as Config>::Hashing::hash_of(&call);
				// Expire after 10 blocks when `ProposalExpiry` is set to 10
				let expiry = Some(starting_block + <Test as Config>::ProposalExpiry::get());

				// Manually create and insert proposal but with empty `approvals`
				// without the proposer automatically approving that normally occurs.
				// Instead delay that to occur later
				let approvals = BoundedVec::default();

				let proposal_info = ProposalInfo {
					call_hash,
					expiry,
					approvals,
					status: ProposalStatus::Pending, /* Force pending even enough approvals to
					                                  * execute */
					proposer: ALICE,
					executed_at: None,
					submitted_at: System::block_number(),
				};

				// Insert custom proposal directly into storage
				Proposals::<Test>::insert(call_hash, ALICE_ORIGIN_ID, proposal_info);
				ProposalCalls::<Test>::insert(call_hash, call);

				// Advance past expiry
				System::set_block_number(expiry.unwrap() + 1);

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
					true, // Auto-execute
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
		fn approval_after_cancelled_fails() {
			new_test_ext().execute_with(|| {
				let call = make_remark_call("1000").unwrap();
				let call_hash = <Test as Config>::Hashing::hash_of(&call);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
				));

				// Alice cancels proposal
				assert_ok!(OriginAndGate::cancel_proposal(
					RuntimeOrigin::signed(ALICE),
					call_hash,
					ALICE_ORIGIN_ID,
				));

				// Bob tries to approve cancelled proposal
				assert_noop!(
					OriginAndGate::add_approval(
						RuntimeOrigin::signed(BOB),
						call_hash,
						ALICE_ORIGIN_ID,
						BOB_ORIGIN_ID,
						true, // Auto-execute
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
				let call_hash = <Test as Config>::Hashing::hash_of(&call);

				// Create proposal
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
				));

				// Proposal that includes approval
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.approvals.len(), 1);

				// Approval from Bob
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					call_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					true, // Auto-execute
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
						true, // Auto-execute
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
				let mut approvals = BoundedVec::<(AccountId, OriginId), MaxApprovals>::default();
				approvals.try_push((ALICE, ALICE_ORIGIN_ID)).unwrap();
				approvals.try_push((BOB, BOB_ORIGIN_ID)).unwrap();
				let executed_at = Some(System::block_number());

				let proposal_info = ProposalInfo {
					call_hash,
					expiry: None,
					approvals,
					status: ProposalStatus::Pending, /* Force pending even enough approvals to
					                                  * execute */
					proposer: ALICE,
					executed_at,
					submitted_at: System::block_number(),
				};

				// Insert custom proposal directly into storage
				Proposals::<Test>::insert(call_hash, ALICE_ORIGIN_ID, proposal_info);
				ProposalCalls::<Test>::insert(call_hash, call);

				// Add approval records manually
				Approvals::<Test>::insert((call_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID, ALICE);
				Approvals::<Test>::insert((call_hash, ALICE_ORIGIN_ID), BOB_ORIGIN_ID, BOB);

				// Verify test setup correct with `MaxApprovals::get()` and proposal is still
				// 'Pending' status
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
						true, // Auto-execute
					),
					Error::<Test>::TooManyApprovals
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
					true, // Auto-execute
				));

				let execution_timepoint = current_timepoint();

				assert_eq!(MaxApprovals::get() as usize, 2);

				// Verify proposal now Executed
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);
				assert_eq!(proposal.approvals.len(), MaxApprovals::get() as usize); // Alice + Bob

				// Verify both `OriginApprovalAdded` and `ProposalExecuted` events were emitted
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::OriginApprovalAdded {
					proposal_hash: call_hash,
					origin_id: ALICE_ORIGIN_ID,
					approving_origin_id: BOB_ORIGIN_ID,
					timepoint: execution_timepoint,
				}));

				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
					proposal_hash: call_hash,
					origin_id: ALICE_ORIGIN_ID,
					result: Ok(()),
					timepoint: execution_timepoint,
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
				assert!(proposal.approvals.contains(&(ALICE, ALICE_ORIGIN_ID)));

				// Alice withdraws approval
				assert_ok!(OriginAndGate::withdraw_approval(
					RuntimeOrigin::signed(ALICE),
					call_hash,
					ALICE_ORIGIN_ID,
					ALICE_ORIGIN_ID,
				));

				// Verify approval removed
				let updated_proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert!(!updated_proposal.approvals.contains(&(ALICE, ALICE_ORIGIN_ID)));

				// Verify updated proposal status reflects that an approval
				// was cancelled by Alice and has status `Pending`
				assert_eq!(updated_proposal.status, ProposalStatus::Pending);

				// Verify event
				System::assert_has_event(RuntimeEvent::OriginAndGate(
					Event::OriginApprovalWithdrawn {
						proposal_hash: call_hash,
						origin_id: ALICE_ORIGIN_ID,
						withdrawing_origin_id: ALICE_ORIGIN_ID,
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
				assert!(proposal.approvals.contains(&(ALICE, ALICE_ORIGIN_ID)));

				// Verify approval stored in Approvals storage
				assert!(
					Approvals::<Test>::get((call_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID).is_some()
				);

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
				assert!(!proposal.approvals.contains(&(ALICE, ALICE_ORIGIN_ID)));

				// Verify approval no longer in Approvals storage
				assert!(
					Approvals::<Test>::get((call_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID).is_none()
				);

				// Verify event
				System::assert_has_event(RuntimeEvent::OriginAndGate(
					Event::OriginApprovalWithdrawn {
						proposal_hash: call_hash,
						origin_id: ALICE_ORIGIN_ID,
						withdrawing_origin_id: ALICE_ORIGIN_ID,
						timepoint: current_timepoint(),
					},
				));

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
	}

	mod race_condition {
		use super::*;

		#[test]
		fn test_race_condition_approval_after_execution() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create proposal
				let call = make_remark_call("1000").unwrap();
				let call_hash =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
				));

				let alice_submission_timepoint = current_timepoint();

				// Verify proposal created with Alice's approval and remains `Pending`
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);

				// Bob approves should trigger execution
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					call_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					true, // Auto-execute
				));

				let execution_timepoint = current_timepoint();

				// Verify proposal status changed to `Executed`
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);

				// Charlie tries approve after execution
				assert_noop!(
					OriginAndGate::add_approval(
						RuntimeOrigin::signed(CHARLIE),
						call_hash,
						ALICE_ORIGIN_ID,
						CHARLIE_ORIGIN_ID,
						true, // Auto-execute
					),
					Error::<Test>::ProposalAlreadyExecuted
				);

				// Verify approval events with timepoint
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::OriginApprovalAdded {
					proposal_hash: call_hash,
					origin_id: ALICE_ORIGIN_ID,
					approving_origin_id: BOB_ORIGIN_ID,
					timepoint: execution_timepoint,
				}));

				// Verify execution event with timepoint
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
					proposal_hash: call_hash,
					origin_id: ALICE_ORIGIN_ID,
					result: Ok(()),
					timepoint: execution_timepoint,
				}));
			});
		}

		#[test]
		fn test_race_condition_withdrawal_after_execution() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create proposal
				let call = make_remark_call("1000").unwrap();
				let call_hash =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
				));

				let alice_submission_timepoint = current_timepoint();

				// Bob approves that should trigger execution
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					call_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					true, // Auto-execute
				));

				let execution_timepoint = current_timepoint();

				// Verify proposal status changed to `Executed`
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);

				// Alice tries to withdraw her approval after execution
				assert_noop!(
					OriginAndGate::withdraw_approval(
						RuntimeOrigin::signed(ALICE),
						call_hash,
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
				let call_hash =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					Some(10), // Expires at block 10
				));

				let alice_submission_timepoint = Timepoint {
					height: System::block_number(),
					index: System::extrinsic_index().unwrap_or_default(),
				};
				assert_eq!(alice_submission_timepoint.height, 1);
				assert_eq!(alice_submission_timepoint.index, 0);

				// Skip to block 11 that expired
				System::set_block_number(11);

				// Check proposal status before attempting approval
				let proposal_before = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal_before.status, ProposalStatus::Pending);

				// Bob tries approve after expiry
				let result = OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					call_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					true, // Auto-execute
				);

				// Verify call fails with ProposalExpired error
				assert_err!(result, Error::<Test>::ProposalExpired);

				// Since transaction failed the proposal status should still be Pending
				// because status update is rolled back when transaction fails
				let proposal_after = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal_after.status, ProposalStatus::Pending);

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
				let call_hash =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
				));

				// Bob approves and triggers execution
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					call_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					true, // Auto-execute
				));

				// Verify proposal exists in storage before execution
				assert!(proposal_exists(call_hash, ALICE_ORIGIN_ID));
				assert!(proposal_call_exists(call_hash));
				assert!(proposal_has_approvals(call_hash, ALICE_ORIGIN_ID));

				// Verify execution event emitted
				let execution_timepoint = current_timepoint();
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
					proposal_hash: call_hash,
					origin_id: ALICE_ORIGIN_ID,
					result: Ok(()),
					timepoint: execution_timepoint,
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
				let call_hash =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
				));

				// Verify proposal exists in storage before cancellation
				assert!(proposal_exists(call_hash, ALICE_ORIGIN_ID));
				assert!(proposal_call_exists(call_hash));
				assert!(proposal_has_approvals(call_hash, ALICE_ORIGIN_ID));

				// Alice cancels proposal
				assert_ok!(OriginAndGate::cancel_proposal(
					RuntimeOrigin::signed(ALICE),
					call_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify proposal and all related storage cleaned up
				assert!(!proposal_exists(call_hash, ALICE_ORIGIN_ID));
				assert!(!proposal_call_exists(call_hash));
				assert!(!proposal_has_approvals(call_hash, ALICE_ORIGIN_ID));

				// Verify cancellation event emitted
				let cancellation_timepoint = current_timepoint();
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCancelled {
					proposal_hash: call_hash,
					origin_id: ALICE_ORIGIN_ID,
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
				NonCancelledProposalRetentionPeriod::set(50);

				System::set_block_number(1);

				// Create proposal with expiry at block 10
				let call = make_remark_call("1000").unwrap();
				let call_hash =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					Some(10), // Expires at block 10
				));

				// Verify proposal is in pending state before expiry
				assert!(proposal_exists(call_hash, ALICE_ORIGIN_ID));
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
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
					call_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					true, // Auto-execute
				);
				assert_err!(result, Error::<Test>::ProposalExpired);

				// Verify proposal still exists in storage but is marked as expired
				assert!(proposal_exists(call_hash, ALICE_ORIGIN_ID));
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Expired);

				// Verify that the ProposalExpired event was emitted
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExpired {
					proposal_hash: call_hash,
					origin_id: ALICE_ORIGIN_ID,
					timepoint: Timepoint { height: 10, index: 0 },
				}));

				// Advance to block 60 to satisfy the retention period
				// expiry at block 10 + retention period 50 = block 60)
				System::set_block_number(60);

				// Clean up expired proposal that may be initiated via governance proposal
				assert_ok!(OriginAndGate::clean(
					RuntimeOrigin::signed(ALICE),
					call_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify proposal and all related storage has been cleaned up
				assert!(!proposal_exists(call_hash, ALICE_ORIGIN_ID));
				assert!(!proposal_call_exists(call_hash));
				assert!(!proposal_has_approvals(call_hash, ALICE_ORIGIN_ID));

				// Verify clean event emitted
				let cleanup_timepoint = current_timepoint();
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCleaned {
					proposal_hash: call_hash,
					origin_id: ALICE_ORIGIN_ID,
					timepoint: cleanup_timepoint,
				}));
			});
		}

		#[test]
		fn clean_fails_for_pending_proposals() {
			new_test_ext().execute_with(|| {
				System::set_block_number(1);

				// Create proposal
				let call = make_remark_call("1000").unwrap();
				let call_hash =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
				));

				// Try clean pending proposal should fail
				assert_noop!(
					OriginAndGate::clean(RuntimeOrigin::signed(ALICE), call_hash, ALICE_ORIGIN_ID,),
					Error::<Test>::ProposalNotInExpiredOrExecutedState
				);

				// Verify proposal still exists
				assert!(proposal_exists(call_hash, ALICE_ORIGIN_ID));
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
				NonCancelledProposalRetentionPeriod::set(50);

				System::set_block_number(1);

				// Create proposal
				let call = make_remark_call("1000").unwrap();
				let call_hash = <<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
				));

				// Bob approves and triggers execution
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					call_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					true, // Auto-execute
				));

				// Verify proposal is marked as executed but still exists in storage
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);
				assert!(proposal_exists(call_hash, ALICE_ORIGIN_ID));
				assert!(proposal_call_exists(call_hash));
				assert!(proposal_has_approvals(call_hash, ALICE_ORIGIN_ID));

				println!("clean_fails_before_retention_period_elapsed - Proposal executed_at: {:?}, Current block: {}", proposal.executed_at, System::block_number());

				// Try to clean immediately after execution should fail
				// where retention period starts from execution block + 50 blocks
				assert_noop!(
					OriginAndGate::clean(RuntimeOrigin::signed(ALICE), call_hash, ALICE_ORIGIN_ID),
					Error::<Test>::ProposalRetentionPeriodNotElapsed
				);

				// Advance to block 50 since no expiry set so we use proposal expiry of 10 blocks instead
				// and retention period of 50 blocks
				System::set_block_number(50);

				// Clean should still fail
				assert_noop!(
					OriginAndGate::clean(RuntimeOrigin::signed(ALICE), call_hash, ALICE_ORIGIN_ID),
					Error::<Test>::ProposalRetentionPeriodNotElapsed
				);

				// Advance to block 51 that is past retention period
				System::set_block_number(51);

				// Clean should now succeed
				assert_ok!(OriginAndGate::clean(
					RuntimeOrigin::signed(ALICE),
					call_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify proposal and all related storage cleaned up
				assert!(!proposal_exists(call_hash, ALICE_ORIGIN_ID));
				assert!(!proposal_call_exists(call_hash));
				assert!(!proposal_has_approvals(call_hash, ALICE_ORIGIN_ID));

				// Verify clean event emitted
				let cleanup_timepoint = current_timepoint();
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCleaned {
					proposal_hash: call_hash,
					origin_id: ALICE_ORIGIN_ID,
					timepoint: cleanup_timepoint,
				}));

				// Reset retention period to default
				NonCancelledProposalRetentionPeriod::set(0);
			});
		}

		#[test]
		fn clean_after_expiry_respects_retention_period() {
			new_test_ext().execute_with(|| {
				// Set retention period to 50 blocks for this test
				NonCancelledProposalRetentionPeriod::set(50);

				System::set_block_number(1);

				// Create proposal with expiry at block 10
				let call = make_remark_call("1000").unwrap();
				let call_hash =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					Some(10), // Expires at block 10
				));

				// Manually run the on_initialize hook to process expiring proposals
				// marking them from pending to expired
				OriginAndGate::on_initialize(10);

				// Advance to block 11 (after expiry)
				System::set_block_number(11);

				// Try to add approval which should fail due to expiry
				let result = OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					call_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					true, // Auto-execute
				);
				assert_err!(result, Error::<Test>::ProposalExpired);

				// Verify proposal is marked as expired
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Expired);

				// Try to clean immediately after expiry should fail
				// where retention period starts from expiry block 10 + 50 blocks = block 60
				assert_noop!(
					OriginAndGate::clean(RuntimeOrigin::signed(ALICE), call_hash, ALICE_ORIGIN_ID),
					Error::<Test>::ProposalRetentionPeriodNotElapsed
				);

				// Advance to block 59 just before retention period ends
				System::set_block_number(59);

				// Clean should still fail
				assert_noop!(
					OriginAndGate::clean(RuntimeOrigin::signed(ALICE), call_hash, ALICE_ORIGIN_ID),
					Error::<Test>::ProposalRetentionPeriodNotElapsed
				);

				// Advance to block 60 (exactly at retention period end)
				System::set_block_number(60);

				// Clean should now succeed
				assert_ok!(OriginAndGate::clean(
					RuntimeOrigin::signed(ALICE),
					call_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify proposal and all related storage cleaned up
				assert!(!proposal_exists(call_hash, ALICE_ORIGIN_ID));
				assert!(!proposal_call_exists(call_hash));
				assert!(!proposal_has_approvals(call_hash, ALICE_ORIGIN_ID));

				// Verify clean event emitted
				let cleanup_timepoint = current_timepoint();
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCleaned {
					proposal_hash: call_hash,
					origin_id: ALICE_ORIGIN_ID,
					timepoint: cleanup_timepoint,
				}));

				// Reset retention period to default
				NonCancelledProposalRetentionPeriod::set(0);
			});
		}

		#[test]
		fn clean_after_cancellation_does_not_respect_retention_period() {
			new_test_ext().execute_with(|| {
				// Set retention period to 50 blocks for this test
				NonCancelledProposalRetentionPeriod::set(50);

				System::set_block_number(1);

				// Create proposal with expiry at block 100
				let call = make_remark_call("1000").unwrap();
				let call_hash =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					Some(100), // Expires at block 100
				));

				// Cancel proposal at block 5
				System::set_block_number(5);
				assert_ok!(OriginAndGate::cancel_proposal(
					RuntimeOrigin::signed(ALICE),
					call_hash,
					ALICE_ORIGIN_ID,
				));

				assert!(!proposal_exists(call_hash, ALICE_ORIGIN_ID));
				assert!(!proposal_call_exists(call_hash));
				assert!(!proposal_has_approvals(call_hash, ALICE_ORIGIN_ID));
			});
		}

		#[test]
		fn executed_proposal_without_expiry_uses_executed_block() {
			new_test_ext().execute_with(|| {
				ProposalExpiry::set(10);
				// Set retention period to 50 blocks for this test
				NonCancelledProposalRetentionPeriod::set(50);

				System::set_block_number(1);

				// Create proposal without expiry
				let call = make_remark_call("1000").unwrap();
				let call_hash =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None, // No expiry
				));

				// Change to block 5
				System::set_block_number(5);

				// Bob approves and triggers execution
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					call_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					true, // Auto-execute
				));

				// Verify proposal marked as executed
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);
				println!(
					"Proposal executed_at: {:?}, Current block: {}",
					proposal.executed_at,
					System::block_number()
				);

				// Try clean immediately after execution should fail
				assert_noop!(
					OriginAndGate::clean(RuntimeOrigin::signed(ALICE), call_hash, ALICE_ORIGIN_ID),
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
					call_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify proposal and all related storage cleaned up
				assert!(!proposal_exists(call_hash, ALICE_ORIGIN_ID));
				assert!(!proposal_call_exists(call_hash));
				assert!(!proposal_has_approvals(call_hash, ALICE_ORIGIN_ID));

				// Reset retention period to default
				NonCancelledProposalRetentionPeriod::set(0);
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
				let call_hash = <Test as Config>::Hashing::hash_of(&call);

				// Create proposal by Alice with no expiry
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None, // No expiry
				));

				// Verify proposal has Pending status
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);

				// Add Bob's approval without auto-execute
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					call_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					false, // Do not auto-execute
				));

				// Verify proposal still has Pending status with approvals from Alice + Bob
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);
				assert_eq!(proposal.approvals.len(), MaxApprovals::get() as usize);

				// Advance block number staying within expiry period
				System::set_block_number(100); // At the end of proposal expiry

				// Execute the proposal manually
				assert_ok!(OriginAndGate::execute_proposal(
					RuntimeOrigin::signed(ALICE),
					call_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify proposal has Executed status
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);
				assert_eq!(proposal.executed_at, Some(100));

				// Verify dummy value was set
				let expected_bytes = b"1000".to_vec();
				let expected_bounded: DummyValueOf = BoundedVec::try_from(expected_bytes).unwrap();
				assert_eq!(Dummy::<Test>::get(), Some(expected_bounded));

				// Verify execution event was emitted
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
					proposal_hash: call_hash,
					origin_id: ALICE_ORIGIN_ID,
					result: Ok(()),
					timepoint: Timepoint { height: 100, index: 0 },
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
				let call_hash1 =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call1);

				// Proposal 2: Expires at block 10 as well
				let call2 = make_remark_call("1002").unwrap();
				let call_hash2 =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call2);

				// Proposal 3: Expires at block 15
				let call3 = make_remark_call("1003").unwrap();
				let call_hash3 =
					<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call3);

				// Create proposals
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call1.clone(),
					ALICE_ORIGIN_ID,
					Some(10), // Expires at block 10
				));
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(BOB),
					call2.clone(),
					BOB_ORIGIN_ID,
					Some(10), // Expires at block 10
				));
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call3.clone(),
					ALICE_ORIGIN_ID,
					Some(15), // Expires at block 15
				));

				// Verify all proposals are in Pending status
				assert_eq!(
					Proposals::<Test>::get(call_hash1, ALICE_ORIGIN_ID).unwrap().status,
					ProposalStatus::Pending
				);
				assert_eq!(
					Proposals::<Test>::get(call_hash2, BOB_ORIGIN_ID).unwrap().status,
					ProposalStatus::Pending
				);
				assert_eq!(
					Proposals::<Test>::get(call_hash3, ALICE_ORIGIN_ID).unwrap().status,
					ProposalStatus::Pending
				);

				// Verify proposals are tracked in ExpiringProposals
				assert!(ExpiringProposals::<Test>::get(10).contains(&(call_hash1, ALICE_ORIGIN_ID)));
				assert!(ExpiringProposals::<Test>::get(10).contains(&(call_hash2, BOB_ORIGIN_ID)));
				assert!(ExpiringProposals::<Test>::get(15).contains(&(call_hash3, ALICE_ORIGIN_ID)));

				// Advance to block 10 that should trigger on_initialize and expire the first two
				// proposals
				System::set_block_number(10);
				OriginAndGate::on_initialize(10);

				// Verify first two proposals are now expired
				assert_eq!(
					Proposals::<Test>::get(call_hash1, ALICE_ORIGIN_ID).unwrap().status,
					ProposalStatus::Expired
				);
				assert_eq!(
					Proposals::<Test>::get(call_hash2, BOB_ORIGIN_ID).unwrap().status,
					ProposalStatus::Expired
				);
				// Third proposal should still be pending
				assert_eq!(
					Proposals::<Test>::get(call_hash3, ALICE_ORIGIN_ID).unwrap().status,
					ProposalStatus::Pending
				);

				// Verify expiry events were emitted
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExpired {
					proposal_hash: call_hash1,
					origin_id: ALICE_ORIGIN_ID,
					timepoint: Timepoint { height: 10, index: 0 },
				}));
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExpired {
					proposal_hash: call_hash2,
					origin_id: BOB_ORIGIN_ID,
					timepoint: Timepoint { height: 10, index: 0 },
				}));

				// Verify ExpiringProposals for block 10 is now empty by being consumed
				assert!(ExpiringProposals::<Test>::get(10).is_empty());

				// Advance to block 15 that should expire the third proposal
				System::set_block_number(15);
				OriginAndGate::on_initialize(15);

				// Verify third proposal is now expired
				assert_eq!(
					Proposals::<Test>::get(call_hash3, ALICE_ORIGIN_ID).unwrap().status,
					ProposalStatus::Expired
				);

				// Verify expiry event was emitted for third proposal
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExpired {
					proposal_hash: call_hash3,
					origin_id: ALICE_ORIGIN_ID,
					timepoint: Timepoint { height: 15, index: 0 },
				}));

				// Verify ExpiringProposals for block 15 is now empty
				assert!(ExpiringProposals::<Test>::get(15).is_empty());
			});
		}

		#[test]
		fn max_proposals_to_expire_per_block_is_enforced() {
			new_test_ext().execute_with(|| {
				// Set max proposals to expire per block to 2 for this test
				MaxProposalsToExpirePerBlock::set(2);

				// Start at block 1
				System::set_block_number(1);

				// Create 5 proposals all expiring at block 10
				let mut call_hashes = Vec::new();

				for i in 0..5 {
					let call = make_remark_call(&format!("100{}", i)).unwrap();
					let call_hash =
						<<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);
					call_hashes.push((call_hash, ALICE_ORIGIN_ID));

					assert_ok!(OriginAndGate::propose(
						RuntimeOrigin::signed(ALICE),
						call.clone(),
						ALICE_ORIGIN_ID,
						Some(10), // All expire at block 10
					));
				}

				// Verify all proposals are in Pending status
				for (hash, origin_id) in &call_hashes {
					assert_eq!(
						Proposals::<Test>::get(hash, origin_id).unwrap().status,
						ProposalStatus::Pending
					);
				}

				// Verify ExpiringProposals for block 10 contains all 5 proposals
				assert_eq!(ExpiringProposals::<Test>::get(10).len(), 5);

				// Advance to block 10 that should trigger on_initialize and expire only the first 2
				// proposals due to MaxProposalsToExpirePerBlock limit
				System::set_block_number(10);
				OriginAndGate::on_initialize(10);

				// Count how many proposals were expired
				let expired_count = call_hashes
					.iter()
					.filter(|(hash, origin_id)| {
						Proposals::<Test>::get(hash, origin_id)
							.map(|p| p.status == ProposalStatus::Expired)
							.unwrap_or(false)
					})
					.count();

				// Verify only 2 proposals were expired due to the limit
				assert_eq!(expired_count, 2);

				// Verify we have exactly 2 ProposalExpired events
				let events = System::events();
				let expiry_events = events
					.iter()
					.filter(|event| {
						matches!(
							event.event,
							RuntimeEvent::OriginAndGate(Event::ProposalExpired { .. })
						)
					})
					.count();
				assert_eq!(expiry_events, 2);

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
				let call_hash = <Test as Config>::Hashing::hash_of(&call);
				let expiry_block = starting_block + 99; // Proposal expires at block 100
				let expiry = Some(expiry_block);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					expiry,
				));

				// Verify proposal in ExpiringProposals storage
				let expiring_at_block_100 = OriginAndGate::expiring_proposals(expiry_block);
				assert!(
					expiring_at_block_100
						.iter()
						.any(|(hash, id)| *hash == call_hash && *id == ALICE_ORIGIN_ID),
					"Proposal should be in ExpiringProposals before execution"
				);

				// Bob approves with auto_execute
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					call_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					true,
				));

				// Verify proposal executed
				let proposal = OriginAndGate::proposals(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);

				// Verify proposal removed from ExpiringProposals storage
				let expiring_at_block_100_after_execution =
					OriginAndGate::expiring_proposals(expiry_block);
				assert!(
					!expiring_at_block_100_after_execution
						.iter()
						.any(|(hash, id)| *hash == call_hash && *id == ALICE_ORIGIN_ID),
					"Proposal should be removed from ExpiringProposals after execution"
				);

				// Advance to block 5 (execution block + retention period - 1)
				let execution_block = 1; // Current block when executed
				let retention_period = <Test as Config>::NonCancelledProposalRetentionPeriod::get();
				System::set_block_number(execution_block + retention_period - 1);

				// Clean up should fail since retention period hasn't yet elapsed
				assert_noop!(
					OriginAndGate::clean(RuntimeOrigin::signed(ALICE), call_hash, ALICE_ORIGIN_ID,),
					Error::<Test>::ProposalRetentionPeriodNotElapsed
				);

				// Advance to cleanup eligible block
				System::set_block_number(execution_block + retention_period);

				// Clean up should succeed
				assert_ok!(OriginAndGate::clean(
					RuntimeOrigin::signed(ALICE),
					call_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify proposal removed
				assert!(!OriginAndGate::proposals(call_hash, ALICE_ORIGIN_ID).is_some());
			});
		}

		#[test]
		fn cancelled_proposals_can_be_cleaned_up_immediately() {
			new_test_ext().execute_with(|| {
				let starting_block = 1;
				System::set_block_number(starting_block);

				// Create proposal with expiry at block 100
				let call = make_remark_call("2000").unwrap();
				let call_hash = <Test as Config>::Hashing::hash_of(&call);
				let expiry_block = starting_block + 99; // Proposal expires at block 100
				let expiry = Some(expiry_block);

				// Alice proposes
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					expiry,
				));

				// Verify proposal is in ExpiringProposals storage
				let expiring_proposals = OriginAndGate::expiring_proposals(expiry_block);
				assert!(
					expiring_proposals
						.iter()
						.any(|(hash, id)| *hash == call_hash && *id == ALICE_ORIGIN_ID),
					"Proposal should be in ExpiringProposals"
				);

				// Alice cancels proposal
				assert_ok!(OriginAndGate::cancel_proposal(
					RuntimeOrigin::signed(ALICE),
					call_hash,
					ALICE_ORIGIN_ID,
				));

				// Verify proposal is removed from storage after cancellation
				assert!(OriginAndGate::proposals(call_hash, ALICE_ORIGIN_ID).is_none());
			});
		}
	}

	mod max_approvals_scaling {
		use super::*;

		// Helper function to test a specific MaxApprovals value
		fn test_with_max_approvals(max_approvals: u32) {
			new_test_ext().execute_with(|| {
				// Set MaxApprovals for this test
				MaxApprovals::set(max_approvals);

				System::set_block_number(1);

				// Create a call for our test
				let call = make_remark_call("1000").unwrap();
				let call_hash = <Test as Config>::Hashing::hash_of(&call);

				// Create proposal by Alice
				assert_ok!(OriginAndGate::propose(
					RuntimeOrigin::signed(ALICE),
					call.clone(),
					ALICE_ORIGIN_ID,
					None,
				));

				// Verify proposal was created with Pending status
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);

				// Proposer counts as 1 approval already
				let required_additional_approvals = max_approvals - 1;

				// Add approvals from other accounts
				let approvers = [BOB, CHARLIE];
				let origin_ids = [BOB_ORIGIN_ID, CHARLIE_ORIGIN_ID];

				// Add all but last required approval without auto-execute
				for i in 0..required_additional_approvals - 1 {
					assert_ok!(OriginAndGate::add_approval(
						RuntimeOrigin::signed(approvers[i as usize]),
						call_hash,
						ALICE_ORIGIN_ID,
						origin_ids[i as usize],
						false, // Don't auto-execute
					));

					// Verify proposal still in Pending state
					let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
					assert_eq!(proposal.status, ProposalStatus::Pending);
				}

				// Add final approval with auto-execute
				let final_approver_index = required_additional_approvals - 1;
				assert_ok!(OriginAndGate::add_approval(
					RuntimeOrigin::signed(approvers[final_approver_index as usize]),
					call_hash,
					ALICE_ORIGIN_ID,
					origin_ids[final_approver_index as usize],
					true, // Auto-execute
				));

				// Verify proposal was executed
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);

				// Verify dummy value was set
				let expected_bytes = b"1000".to_vec();
				let expected_bounded: DummyValueOf = BoundedVec::try_from(expected_bytes).unwrap();
				assert_eq!(Dummy::<Test>::get(), Some(expected_bounded));
			});
		}

		#[test]
		fn proposal_execution_requires_max_approvals_minus_one_additional_approvals() {
			// Verifies that proposals require (MaxApprovals - 1) additional approvals
			// after the proposer in order to auto-execute or manually execute

			// Test with MaxApprovals = 2
			test_with_max_approvals(2);

			// Test with MaxApprovals = 3
			test_with_max_approvals(3);
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
				true, // Auto-execute
			));

			// Verify proposal exists and has both approvals
			let proposal = Proposals::<Test>::get(call_hash, alice_origin_id).unwrap();
			assert_eq!(proposal.approvals.len(), MaxApprovals::get() as usize);

			// Verify both origin IDs are in approvals
			assert!(proposal.approvals.contains(&(alice_origin_id.into(), ALICE_ORIGIN_ID)));
			assert!(proposal.approvals.contains(&(bob_origin_id.into(), BOB_ORIGIN_ID)));

			// Verify approval event emitted with correct origin IDs
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::OriginApprovalAdded {
				proposal_hash: call_hash,
				origin_id: ALICE_ORIGIN_ID,
				approving_origin_id: BOB_ORIGIN_ID,
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

				let proposal_timepoint = current_timepoint();

				// Verify proposal created with Alice's approval and remains `Pending`
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Pending);
				assert_eq!(Dummy::<Test>::get(), None); // Call not executed yet

				// Verify proposal creation event with timepoint
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCreated {
					proposal_hash: call_hash,
					origin_id: ALICE_ORIGIN_ID,
					timepoint: proposal_timepoint,
				}));

				// Verify proposal pending and not executed yet since only Alice approved
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
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
					call_hash,
					ALICE_ORIGIN_ID,
					BOB_ORIGIN_ID,
					true, // Auto-execute
				));

				// Verify execution successful after both approvals
				let expected_bytes = b"1000".to_vec();
				let expected_bounded: DummyValueOf = BoundedVec::try_from(expected_bytes).unwrap();
				assert_eq!(Dummy::<Test>::get(), Some(expected_bounded));

				// Verify ExecutedCalls storage updated with call hash at execution timepoint
				assert_eq!(ExecutedCalls::<Test>::get(execution_timepoint), Some(call_hash));

				// Verify proposal marked executed
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);

				// Verify execution event with execution timepoint
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
					proposal_hash: call_hash,
					origin_id: ALICE_ORIGIN_ID,
					result: Ok(()),
					timepoint: execution_timepoint,
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
					true, // Auto-execute
				);
				assert!(result.is_err());

				// Try to approve with same origin ID and Bob approving should fail
				let result = OriginAndGate::add_approval(
					RuntimeOrigin::signed(BOB),
					call_hash,
					ALICE_ORIGIN_ID,
					ALICE_ORIGIN_ID,
					true, // Auto-execute
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

				// Verify `ProposalCreated` event was emitted
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCreated {
					proposal_hash: call_hash,
					origin_id: ALICE_ORIGIN_ID,
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
					true, // Auto-execute
				));

				let execution_timepoint = current_timepoint();

				// Verify proposal status changed to `Executed`
				let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
				assert_eq!(proposal.status, ProposalStatus::Executed);
				assert_eq!(proposal.approvals.len(), MaxApprovals::get() as usize); // Alice + Bob

				// Verify both `OriginApprovalAdded` and `ProposalExecuted` events were emitted
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::OriginApprovalAdded {
					proposal_hash: call_hash,
					origin_id: ALICE_ORIGIN_ID,
					approving_origin_id: BOB_ORIGIN_ID,
					timepoint: execution_timepoint,
				}));

				// Verify proposal creation event with timepoint
				System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
					proposal_hash: call_hash,
					origin_id: ALICE_ORIGIN_ID,
					result: Ok(()),
					timepoint: execution_timepoint,
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
				let call_hash = H256::repeat_byte(0xab);

				// Create proposal info with sufficient approvals
				let mut approvals = BoundedVec::<(AccountId, OriginId), MaxApprovals>::default();
				approvals.try_push((ALICE, ALICE_ORIGIN_ID)).unwrap();
				approvals.try_push((BOB, BOB_ORIGIN_ID)).unwrap(); // Already have 2 approvals
				let executed_at = Some(System::block_number());

				let proposal_info = ProposalInfo {
					call_hash,
					expiry: None,
					approvals,
					status: ProposalStatus::Pending,
					proposer: ALICE,
					executed_at,
					submitted_at: System::block_number(),
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
					true, // Auto-execute
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
			let call_hash = <Test as Config>::Hashing::hash_of(&call);

			// Alice proposes and approves
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call.clone(),
				ALICE_ORIGIN_ID,
				None,
			));

			// Verify proposal creation event with timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCreated {
				proposal_hash: call_hash,
				origin_id: ALICE_ORIGIN_ID,
				timepoint: alice_submission_timepoint,
			}));

			// Verify proposal pending and not executed yet since only Alice approved
			let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
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
				call_hash,
				ALICE_ORIGIN_ID,
				BOB_ORIGIN_ID,
				true, // Auto-execute
			));

			// Verify execution successful after both approvals
			let expected_bytes = b"1000".to_vec();
			let expected_bounded: DummyValueOf = BoundedVec::try_from(expected_bytes).unwrap();
			assert_eq!(Dummy::<Test>::get(), Some(expected_bounded));

			// Verify ExecutedCalls storage updated with call hash at execution timepoint
			assert_eq!(ExecutedCalls::<Test>::get(execution_timepoint), Some(call_hash));

			// Verify proposal marked executed
			let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.status, ProposalStatus::Executed);

			// Verify execution event with execution timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
				proposal_hash: call_hash,
				origin_id: ALICE_ORIGIN_ID,
				result: Ok(()),
				timepoint: execution_timepoint,
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
			let call_hash = <Test as Config>::Hashing::hash_of(&call);

			// Alice proposes
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call.clone(),
				ALICE_ORIGIN_ID,
				None,
			));

			// Verify proposal creation event with timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCreated {
				proposal_hash: call_hash,
				origin_id: ALICE_ORIGIN_ID,
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
			let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.status, ProposalStatus::Pending);
			assert_eq!(proposal.approvals.len(), 1);
			assert!(proposal.approvals.contains(&(ALICE, ALICE_ORIGIN_ID)));

			// No ExecutedCalls entries should exist at either timepoint
			assert!(ExecutedCalls::<Test>::get(alice_submission_timepoint).is_none());
			assert!(ExecutedCalls::<Test>::get(execution_timepoint).is_none());

			// Bob approves proposal later
			assert_ok!(OriginAndGate::add_approval(
				RuntimeOrigin::signed(BOB),
				call_hash,
				ALICE_ORIGIN_ID,
				BOB_ORIGIN_ID,
				true, // Auto-execute
			));

			// Verify execution occurred
			let expected_bytes = b"1000".to_vec();
			let expected_bounded: DummyValueOf = BoundedVec::try_from(expected_bytes).unwrap();
			assert_eq!(Dummy::<Test>::get(), Some(expected_bounded));

			// Verify ExecutedCalls storage updated with call hash at execution timepoint
			assert_eq!(ExecutedCalls::<Test>::get(execution_timepoint), Some(call_hash));

			// Verify proposal marked executed
			let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.status, ProposalStatus::Executed);

			// Verify execution event with execution timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
				proposal_hash: call_hash,
				origin_id: ALICE_ORIGIN_ID,
				result: Ok(()),
				timepoint: execution_timepoint,
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
			let call_hash = <Test as Config>::Hashing::hash_of(&call);

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
			assert!(Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).is_none());
			assert!(Proposals::<Test>::get(call_hash, BOB_ORIGIN_ID).is_none());
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
			let call_hash = <Test as Config>::Hashing::hash_of(&call);

			// Submit with first origin
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call.clone(),
				ALICE_ORIGIN_ID,
				None,
			));

			// Verify proposal creation event with timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCreated {
				proposal_hash: call_hash,
				origin_id: ALICE_ORIGIN_ID,
				timepoint: alice_submission_timepoint,
			}));

			// Try execute with one origin should fail
			let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
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
				call_hash,
				ALICE_ORIGIN_ID,
				BOB_ORIGIN_ID,
				true, // Auto-execute
			));

			// Verify execution successful after both origins approved
			let expected_bytes = b"1000".to_vec();
			let expected_bounded: DummyValueOf = BoundedVec::try_from(expected_bytes).unwrap();
			assert_eq!(Dummy::<Test>::get(), Some(expected_bounded));

			// Verify ExecutedCalls storage updated with call hash at execution timepoint
			assert_eq!(ExecutedCalls::<Test>::get(execution_timepoint), Some(call_hash));

			// Verify proposal marked as executed
			let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
			assert_eq!(proposal.status, ProposalStatus::Executed);

			// Verify execution event with execution timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
				proposal_hash: call_hash,
				origin_id: ALICE_ORIGIN_ID,
				result: Ok(()),
				timepoint: execution_timepoint,
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
			let call_hash1 = <Test as Config>::Hashing::hash_of(&call1);
			let call_hash2 = <Test as Config>::Hashing::hash_of(&call2);

			// Requirement 1 & 2: Two independent origins must approve asynchronously
			// Alice proposes first call
			assert_ok!(OriginAndGate::propose(
				RuntimeOrigin::signed(ALICE),
				call1.clone(),
				ALICE_ORIGIN_ID,
				None,
			));

			// Verify proposal creation event with timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCreated {
				proposal_hash: call_hash1,
				origin_id: ALICE_ORIGIN_ID,
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
			));

			// Verify proposal creation event with timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalCreated {
				proposal_hash: call_hash2,
				origin_id: BOB_ORIGIN_ID,
				timepoint: bob_submission_timepoint,
			}));

			// Requirement 3: Module retains state over proposal hashes
			// Verify both proposals stored correctly
			let proposal1 = Proposals::<Test>::get(call_hash1, ALICE_ORIGIN_ID).unwrap();
			let proposal2 = Proposals::<Test>::get(call_hash2, BOB_ORIGIN_ID).unwrap();
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
				call_hash2,
				BOB_ORIGIN_ID,
				ALICE_ORIGIN_ID,
				true, // Auto-execute
			));

			// Verify first call execution and ExecutedCalls entry
			assert_eq!(ExecutedCalls::<Test>::get(first_execution_timepoint), Some(call_hash2));

			// Verify execution event with timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
				proposal_hash: call_hash2,
				origin_id: BOB_ORIGIN_ID,
				result: Ok(()),
				timepoint: first_execution_timepoint,
			}));

			// Skip to new block for second approval
			System::set_block_number(11);
			let second_execution_timepoint = current_timepoint();

			// Ensure second execution timepoint differs
			assert_ne!(first_execution_timepoint, second_execution_timepoint);

			// Bob approves Alice's proposal
			assert_ok!(OriginAndGate::add_approval(
				RuntimeOrigin::signed(BOB),
				call_hash1,
				ALICE_ORIGIN_ID,
				BOB_ORIGIN_ID,
				true, // Auto-execute
			));

			// Verify second call execution and ExecutedCalls entry
			assert_eq!(ExecutedCalls::<Test>::get(second_execution_timepoint), Some(call_hash1));

			// Verify both calls executed
			// Last executed call wins which appears to conflict
			// with comment below so need to verify
			let expected_bytes = b"1000".to_vec();
			let expected_bounded: DummyValueOf = BoundedVec::try_from(expected_bytes).unwrap();
			assert_eq!(Dummy::<Test>::get(), Some(expected_bounded));

			// Verify execution event with timepoint
			System::assert_has_event(RuntimeEvent::OriginAndGate(Event::ProposalExecuted {
				proposal_hash: call_hash1,
				origin_id: ALICE_ORIGIN_ID,
				result: Ok(()),
				timepoint: second_execution_timepoint,
			}));

			// Verify both proposals marked as executed
			let proposal1 = Proposals::<Test>::get(call_hash1, ALICE_ORIGIN_ID).unwrap();
			let proposal2 = Proposals::<Test>::get(call_hash2, BOB_ORIGIN_ID).unwrap();
			assert_eq!(proposal1.status, ProposalStatus::Executed);
			assert_eq!(proposal2.status, ProposalStatus::Executed);
		});
	}
}
