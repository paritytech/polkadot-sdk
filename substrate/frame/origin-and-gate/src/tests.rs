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
use assert_matches::assert_matches;
use crate::{
	self as pallet_origin_and_gate,
};
use frame_support::{
	assert_ok,
};
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
	DispatchError,
};

// Import mock directly instead of through module import
#[path = "./mock.rs"]
mod mock;
pub use mock::*;

/// Helper function to create a remark call that can be used for testing
fn make_remark_call(text: &str) -> Result<Box<<Test as Config>::RuntimeCall>, &'static str> {
    // Try to parse the text as a u64
    let value = match text.parse::<u64>() {
        Ok(v) => v,
        Err(_) => return Err("Failed to parse input as u64"),
    };

    let remark = self::Call::<Test>::set_dummy {
        new_value: value,
    };
    Ok(Box::new(RuntimeCall::OriginAndGate(remark)))
}

#[test]
fn ensure_origin_works_with_and_gate() {
	new_test_ext().execute_with(|| {
		// Proceed past genesis block so events get deposited
		System::set_block_number(1);

		// Generate call hash
		let call = make_remark_call("1000").unwrap();
		let call_hash = <<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

		// let call = Box::new(mock::RuntimeCall::System(frame_system::Call::remark {
		// 	remark: vec![1, 2, 3, 4],
		// }));
		// let call_hash = <<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

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
		assert_ok!(OriginAndGate::approve(
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
				mock::RuntimeEvent::OriginAndGate(crate::Event::ProposalExecuted { proposal_hash: call_hash, origin_id, result })
			)
		}))
	});
}

#[test]
fn test_direct_and_gate_impossible_with_signed_origins() {
	new_test_ext().execute_with(|| {
		// Test that signed origins cannot satisfy AndGate directly
		// to represents the real-world scenario where a single account
		// cannot simultaneously satisfy multiple origin requirements
		assert!(AliceAndBob::ensure_origin(RuntimeOrigin::signed(ALICE)).is_err());
		assert!(AliceAndBob::ensure_origin(RuntimeOrigin::signed(BOB)).is_err());
	});
}

#[test]
fn test_direct_and_gate_impossible_with_root_origin() {
	new_test_ext().execute_with(|| {
		// Test that even root origin cannot bypass AndGate requirements
		assert!(AliceAndBob::ensure_origin(RuntimeOrigin::root()).is_err());
	});
}


#[test]
fn proposal_is_approved_but_does_not_execute_and_status_remains_pending_when_only_proposed() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		// Create a dummy proposal
		let call = make_remark_call("1000").unwrap();
		let call_hash = <<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

		// Alice proposes through `propose` pallet call that automatically adds Alice as first approval
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
		assert!(Approvals::<Test>::contains_key((call_hash, ALICE_ORIGIN_ID), ALICE_ORIGIN_ID));

		// At this point the proposal should have `Pending` status sinc only have Alice's approval
		// and it is less than `REQUIRED_APPROVALS`

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
		let call_hash = <<Test as Config>::Hashing as sp_runtime::traits::Hash>::hash_of(&call);

		// Alice proposes through `propose` pallet call that automatically adds Alice as first approval
		assert_ok!(OriginAndGate::propose(
			RuntimeOrigin::signed(ALICE),
			call,
			ALICE_ORIGIN_ID,
			None,
		));

		// Verify proposal created with Alice's approval and remains `Pending`
		let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
		assert_eq!(proposal.status, ProposalStatus::Pending);

		// Adding Bob's approval should trigger execution since now have `REQUIRED_APPROVALS` approvals
		assert_ok!(OriginAndGate::approve(
			RuntimeOrigin::signed(BOB),
			call_hash,
			ALICE_ORIGIN_ID,
			BOB_ORIGIN_ID,
		));

		// Verify proposal status changed to `Executed`
		let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
		assert_eq!(proposal.status, ProposalStatus::Executed);
		assert_eq!(proposal.approvals.len(), REQUIRED_APPROVALS as usize); // Alice + Bob

		// Verify both `ProposalApproved` and `ProposalExecuted` events were emitted
		let events = System::events();
		assert!(events.iter().any(|record| matches!(
			record.event,
			mock::RuntimeEvent::OriginAndGate(Event::ProposalApproved { proposal_hash, .. })
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
		let mut approvals = BoundedVec::default();
		approvals.try_push(ALICE_ORIGIN_ID).unwrap();
		approvals.try_push(BOB_ORIGIN_ID).unwrap(); // We already have 2 approvals

		let proposal_info = ProposalInfo {
			call_hash,
			expiry: None,
			approvals,
			status: ProposalStatus::Pending,
		};

		// Skip calling `propose` and instead store proposal directly in storage
		// but not the `call` to execute
		Proposals::<Test>::insert(call_hash, ALICE_ORIGIN_ID, proposal_info);

		// Verify proposal created with Alice's approval and remains `Pending`
		let proposal = Proposals::<Test>::get(call_hash, ALICE_ORIGIN_ID).unwrap();
		assert_eq!(proposal.status, ProposalStatus::Pending);

		// Approval of proposal by Bob means we have enough approvals to try execution but
		// should fail with `ProposalNotFound` because we did not store the `call` to execute
		let result = OriginAndGate::approve(
			RuntimeOrigin::signed(BOB),
			call_hash,
			ALICE_ORIGIN_ID,
			BOB_ORIGIN_ID,
		);

		// Verify error is fully propagated and is not `InsufficientApprovals` error since we
		// silently ignore that error
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
						module_error.error[0],
						PROPOSAL_NOT_FOUND_INDEX,
						"Expected `ProposalNotFound` error (index {}) but got error index: {}",
						PROPOSAL_NOT_FOUND_INDEX,
						module_error.error[0]
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
