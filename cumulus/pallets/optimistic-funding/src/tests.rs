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
use crate::{
    constants::EXISTENTIAL_DEPOSIT,
    mock::{
        new_test_ext, run_to_block, Balances, OptimisticFunding, RuntimeEvent, RuntimeOrigin, System,
        Test, treasury_account, *
    },
    Error, Event, FundingRequests, Config, Votes, types::{FundingRequest as Request, Vote}
};
use frame_support::{
    assert_noop, assert_ok,
    traits::{Currency, Hooks, ConstU32},
    BoundedVec,
};
use sp_runtime::{bounded_vec, traits::{BlakeTwo256, Hash}};
use frame_system::{EventRecord, Phase};

fn events() -> Vec<RuntimeEvent> {
    System::events()
        .into_iter()
        .map(|r| r.event)
        .collect::<Vec<_>>()
}

#[test]
fn submit_request_works() {
    new_test_ext().execute_with(|| {
        // Set up initial state
        run_to_block(1);

        // Set the period end
        assert_ok!(OptimisticFunding::set_period_end(
            RuntimeOrigin::signed(treasury_account()),
            100
        ));

        // Submit a funding request
        let description: BoundedVec<u8, ConstU32<100>> = bounded_vec![b'a'; 50];
        assert_ok!(OptimisticFunding::submit_request(
            RuntimeOrigin::signed(1),
            100u64,
            description.clone()
        ));

        // Get the events to find the request hash
        let event_list = events();
        let request_hash = event_list.iter().find_map(|event| {
            if let RuntimeEvent::OptimisticFunding(crate::Event::RequestSubmitted { request_hash, .. }) = event {
                Some(request_hash)
            } else {
                None
            }
        }).expect("Expected RequestSubmitted event");

        // Check that the request was stored
        assert!(FundingRequests::<Test>::contains_key(request_hash));

        // Check that the active request count was incremented
        assert_eq!(OptimisticFunding::active_request_count(), 1);

        // Check that the deposit was reserved
        assert_eq!(
            Balances::reserved_balance(1),
            <Test as Config>::RequestDeposit::get()
        );

        // Check for the RequestSubmitted event
        assert!(event_list.iter().any(|event| matches!(event,
            RuntimeEvent::OptimisticFunding(crate::Event::RequestSubmitted {
                request_hash: req_hash,
                proposer: 1,
                amount: 100,
            }) if req_hash == request_hash
        )));
    });
}

#[test]
fn submit_request_fails_with_too_small_amount() {
    new_test_ext().execute_with(|| {
        // Set up initial state
        run_to_block(1);

        // Set the period end
        assert_ok!(OptimisticFunding::set_period_end(
            RuntimeOrigin::signed(treasury_account()),
            100
        ));

        // Submit a funding request with too small amount
        let description: BoundedVec<u8, ConstU32<100>> = bounded_vec![b'a'; 50];
        assert_noop!(
            OptimisticFunding::submit_request(
                RuntimeOrigin::signed(1),
                <Test as Config>::MinimumRequestAmount::get() - 1u64,
                description.clone()
            ),
            Error::<Test>::RequestAmountTooSmall
        );
    });
}

#[test]
fn submit_request_fails_with_too_large_amount() {
    new_test_ext().execute_with(|| {
        // Set up initial state
        run_to_block(1);

        // Set the period end
        assert_ok!(OptimisticFunding::set_period_end(
            RuntimeOrigin::signed(treasury_account()),
            100
        ));

        // Submit a funding request with too large amount
        let description: BoundedVec<u8, ConstU32<100>> = bounded_vec![b'a'; 50];
        assert_noop!(
            OptimisticFunding::submit_request(
                RuntimeOrigin::signed(1),
                <Test as Config>::MaximumRequestAmount::get() + 1u64,
                description.clone()
            ),
            Error::<Test>::RequestAmountTooLarge
        );
    });
}

#[test]
fn vote_works() {
    new_test_ext().execute_with(|| {
        // Set up initial state
        run_to_block(1);

        // Set the period end
        assert_ok!(OptimisticFunding::set_period_end(
            RuntimeOrigin::signed(treasury_account()),
            100
        ));

        // Submit a funding request
        let description: BoundedVec<u8, ConstU32<100>> = bounded_vec![b'a'; 50];
        assert_ok!(OptimisticFunding::submit_request(
            RuntimeOrigin::signed(1),
            100u64,
            description.clone()
        ));

        // Get the events to find the request hash
        let event_list = events();
        let request_hash = event_list.iter().find_map(|event| {
            if let RuntimeEvent::OptimisticFunding(crate::Event::RequestSubmitted { request_hash, .. }) = event {
                Some(request_hash)
            } else {
                None
            }
        }).expect("Expected RequestSubmitted event");

        // Vote on the request
        System::reset_events();
        assert_ok!(OptimisticFunding::vote(
            RuntimeOrigin::signed(2),
            *request_hash,
            50u64
        ));

        // Check that the vote was stored
        assert!(Votes::<Test>::contains_key(request_hash, 2));

        // Check that the request was updated
        let request = FundingRequests::<Test>::get(request_hash).unwrap();
        assert_eq!(request.votes_count, 1);
        // Account 2 has rank 1, so its vote weight is amount * (rank + 1) = 50 * 2 = 100
        assert_eq!(request.votes_amount, 100);

        // Check that the event was emitted
        let new_events = events();
        assert!(new_events.iter().any(|event| matches!(event,
            RuntimeEvent::OptimisticFunding(crate::Event::VoteCast {
                request_hash: req_hash,
                voter: 2,
                amount: 50,
            }) if req_hash == request_hash
        )));
    });
}

#[test]
fn vote_fails_for_nonexistent_request() {
    new_test_ext().execute_with(|| {
        // Set up initial state
        run_to_block(1);

        // Try to vote on a nonexistent request
        let request_hash = <Test as frame_system::Config>::Hashing::hash_of(&1u32);
        assert_noop!(
            OptimisticFunding::vote(RuntimeOrigin::signed(1), request_hash, 50u64),
            Error::<Test>::RequestDoesNotExist
        );
    });
}

#[test]
fn vote_fails_when_already_voted() {
    new_test_ext().execute_with(|| {
        // Set up initial state
        run_to_block(1);

        // Set the period end
        assert_ok!(OptimisticFunding::set_period_end(
            RuntimeOrigin::signed(treasury_account()),
            100
        ));

        // Submit a funding request
        let description: BoundedVec<u8, ConstU32<100>> = bounded_vec![b'a'; 50];
        assert_ok!(OptimisticFunding::submit_request(
            RuntimeOrigin::signed(1),
            100u64,
            description.clone()
        ));

        // Get the events to find the request hash
        let event_list = events();
        let request_hash = event_list.iter().find_map(|event| {
            if let RuntimeEvent::OptimisticFunding(crate::Event::RequestSubmitted { request_hash, .. }) = event {
                Some(request_hash)
            } else {
                None
            }
        }).expect("Expected RequestSubmitted event");

        // Vote on the request
        assert_ok!(OptimisticFunding::vote(
            RuntimeOrigin::signed(2),
            *request_hash,
            50u64
        ));

        // Try to vote again
        assert_noop!(
            OptimisticFunding::vote(RuntimeOrigin::signed(2), *request_hash, 50u64),
            Error::<Test>::AlreadyVoted
        );
    });
}

#[test]
fn cancel_vote_works() {
    new_test_ext().execute_with(|| {
        // Set up initial state
        run_to_block(1);

        // Set the period end
        assert_ok!(OptimisticFunding::set_period_end(
            RuntimeOrigin::signed(treasury_account()),
            100
        ));

        // Submit a funding request
        let description: BoundedVec<u8, ConstU32<100>> = bounded_vec![b'a'; 50];
        assert_ok!(OptimisticFunding::submit_request(
            RuntimeOrigin::signed(1),
            100u64,
            description.clone()
        ));

        // Get the events to find the request hash
        let event_list = events();
        let request_hash = event_list.iter().find_map(|event| {
            if let RuntimeEvent::OptimisticFunding(crate::Event::RequestSubmitted { request_hash, .. }) = event {
                Some(request_hash)
            } else {
                None
            }
        }).expect("Expected RequestSubmitted event");

        // Vote on the request
        System::reset_events();
        assert_ok!(OptimisticFunding::vote(
            RuntimeOrigin::signed(2),
            *request_hash,
            50u64
        ));

        // Cancel the vote
        System::reset_events();
        assert_ok!(OptimisticFunding::cancel_vote(
            RuntimeOrigin::signed(2),
            *request_hash
        ));

        // Check that the vote status was updated
        let vote = Votes::<Test>::get(request_hash, 2).unwrap();
        assert_eq!(vote.status, VoteStatus::Cancelled);

        // Check that the request's vote count was updated
        let request = FundingRequests::<Test>::get(request_hash).unwrap();
        assert_eq!(request.votes_count, 0);
        // Account 2 has rank 1, so its vote weight is amount * (rank + 1) = 50 * 2 = 100
        // After cancellation, votes_amount should be 0
        assert_eq!(request.votes_amount, 0);

        // Check that the event was emitted
        let new_events = events();
        assert!(new_events.iter().any(|event| matches!(event,
            RuntimeEvent::OptimisticFunding(crate::Event::VoteCancelled {
                request_hash: req_hash,
                voter: 2,
            }) if req_hash == request_hash
        )));
    });
}

#[test]
fn cancel_vote_fails_for_nonexistent_vote() {
    new_test_ext().execute_with(|| {
        // Set up initial state
        run_to_block(1);

        // Set the period end
        assert_ok!(OptimisticFunding::set_period_end(
            RuntimeOrigin::signed(treasury_account()),
            100
        ));

        // Submit a funding request
        let description: BoundedVec<u8, ConstU32<100>> = bounded_vec![b'a'; 50];
        assert_ok!(OptimisticFunding::submit_request(
            RuntimeOrigin::signed(1),
            100u64,
            description.clone()
        ));

        // Get the events to find the request hash
        let event_list = events();
        let request_hash = event_list.iter().find_map(|event| {
            if let RuntimeEvent::OptimisticFunding(crate::Event::RequestSubmitted { request_hash, .. }) = event {
                Some(request_hash)
            } else {
                None
            }
        }).expect("Expected RequestSubmitted event");

        // Try to cancel a nonexistent vote
        assert_noop!(
            OptimisticFunding::cancel_vote(RuntimeOrigin::signed(2), *request_hash),
            Error::<Test>::VoteDoesNotExist
        );
    });
}

#[test]
fn top_up_treasury_works() {
    new_test_ext().execute_with(|| {
        // Get the treasury account
        let treasury_account = OptimisticFunding::treasury_account();

        // Set up the funding amount
        let funding_amount: u64 = 1000;

        // Set balances
        Balances::make_free_balance_be(&treasury_account, funding_amount);

        // Top up treasury - this only updates the storage value, not actual balances
        assert_ok!(OptimisticFunding::top_up_treasury(
            RuntimeOrigin::signed(treasury_account),
            500u64
        ));

        // Check the treasury balance storage value has increased
        let treasury_balance = OptimisticFunding::treasury_balance();
        assert_eq!(treasury_balance, 500);

        // The account balance remains unchanged because the pallet account and treasury account
        // are the same, so the transfer is from an account to itself
        let treasury_account_balance = Balances::free_balance(&treasury_account);
        assert_eq!(treasury_account_balance, funding_amount);
    });
}

#[test]
fn top_up_treasury_fails_with_wrong_origin() {
    new_test_ext().execute_with(|| {
        // Set up initial state
        run_to_block(1);

        // Try to top up treasury with a non-treasury origin
        assert_noop!(
            OptimisticFunding::top_up_treasury(RuntimeOrigin::signed(1), 500),
            DispatchError::BadOrigin
        );
    });
}

#[test]
fn reject_request_works() {
    new_test_ext().execute_with(|| {
        // Set funding period end
        let period_end = 101;
        assert_ok!(OptimisticFunding::set_period_end(
            RuntimeOrigin::signed(treasury_account()),
            period_end
        ));

        // Setup funding amount
        let funding_amount: u64 = 1000;
        let treasury_account = treasury_account();

        // Set balances
        Balances::make_free_balance_be(&treasury_account, funding_amount);

        // Create funding request
        let amount = 500;
        let description: BoundedVec<u8, ConstU32<100>> = bounded_vec![1, 2, 3];

        // Reset events before submitting request
        System::reset_events();

        assert_ok!(OptimisticFunding::submit_request(
            RuntimeOrigin::signed(1),
            amount,
            description.clone()
        ));

        // Get request hash from emitted event
        let event_list = events();
        let request_hash = event_list.iter().find_map(|event| {
            if let RuntimeEvent::OptimisticFunding(crate::Event::RequestSubmitted { request_hash, .. }) = event {
                Some(request_hash)
            } else {
                None
            }
        }).expect("Expected RequestSubmitted event");

        // Reject request
        assert_ok!(OptimisticFunding::reject_request(
            RuntimeOrigin::signed(treasury_account),
            *request_hash
        ));

        // Check the request was removed
        assert!(!<FundingRequests<Test>>::contains_key(request_hash));

        // Check the deposit was unreserved
        assert_eq!(Balances::reserved_balance(&1), 0);

        // Check the event was emitted
        assert!(events().iter().any(|event| matches!(event,
            RuntimeEvent::OptimisticFunding(crate::Event::RequestRejected { request_hash })
        )));
    });
}

#[test]
fn allocate_funds_works() {
    new_test_ext().execute_with(|| {
        // Set the funding period end
        let period_end = 101;
        assert_ok!(OptimisticFunding::set_period_end(
            RuntimeOrigin::signed(treasury_account()),
            period_end
        ));

        // Set up the funding amount
        let funding_amount: u64 = 1000;

        // Get the treasury account
        let treasury_account = treasury_account();

        // Set the balances
        Balances::make_free_balance_be(&treasury_account, funding_amount);
        Balances::make_free_balance_be(&treasury_account, funding_amount);

        // Top up the treasury
        assert_ok!(OptimisticFunding::top_up_treasury(
            RuntimeOrigin::signed(treasury_account),
            funding_amount
        ));

        // Create a funding request
        let amount = 500;
        let description: BoundedVec<u8, ConstU32<100>> = bounded_vec![1, 2, 3];

        // Reset events before submitting the request
        System::reset_events();

        assert_ok!(OptimisticFunding::submit_request(
            RuntimeOrigin::signed(1),
            amount,
            description.clone()
        ));

        // Get the request hash from the emitted event
        let event_list = events();
        let request_hash = event_list.iter().find_map(|event| {
            if let RuntimeEvent::OptimisticFunding(crate::Event::RequestSubmitted { request_hash, .. }) = event {
                Some(request_hash)
            } else {
                None
            }
        }).expect("Expected RequestSubmitted event");

        // Vote on the request
        assert_ok!(OptimisticFunding::vote(RuntimeOrigin::signed(2), *request_hash, 100));
        assert_ok!(OptimisticFunding::vote(RuntimeOrigin::signed(3), *request_hash, 100));

        // Run to the end of the funding period
        run_to_block(102);

        // Allocate funds
        assert_ok!(OptimisticFunding::allocate_funds(
            RuntimeOrigin::signed(treasury_account),
            *request_hash
        ));

        // Check that the request was removed
        assert!(!<FundingRequests<Test>>::contains_key(request_hash));

        // Check that the treasury balance was updated
        assert_eq!(OptimisticFunding::treasury_balance(), funding_amount - amount);

        // Check that the proposer's balance was updated
        assert_eq!(Balances::free_balance(&1), 10000 + amount);

        // Check that the event was emitted
        assert!(events().iter().any(|event| matches!(event,
            RuntimeEvent::OptimisticFunding(crate::Event::FundsAllocated { request_hash, amount: 500, recipient })
        )));
    });
}

#[test]
fn allocate_funds_fails_with_insufficient_treasury_balance() {
    new_test_ext().execute_with(|| {
        // Set up initial state
        run_to_block(1);

        // Set the period end
        assert_ok!(OptimisticFunding::set_period_end(
            RuntimeOrigin::signed(treasury_account()),
            100
        ));

        // Submit a funding request
        let description: BoundedVec<u8, ConstU32<100>> = bounded_vec![b'a'; 50];
        assert_ok!(OptimisticFunding::submit_request(
            RuntimeOrigin::signed(1),
            100u64,
            description.clone()
        ));

        // Get the events to find the request hash
        let event_list = events();
        let request_hash = event_list.iter().find_map(|event| {
            if let RuntimeEvent::OptimisticFunding(crate::Event::RequestSubmitted { request_hash, .. }) = event {
                Some(request_hash)
            } else {
                None
            }
        }).expect("Expected RequestSubmitted event");

        // Try to allocate funds without sufficient treasury balance
        assert_noop!(
            OptimisticFunding::allocate_funds(
                RuntimeOrigin::signed(treasury_account()),
                *request_hash
            ),
            Error::<Test>::InsufficientTreasuryFunds
        );
    });
}

#[test]
fn set_period_end_works() {
    new_test_ext().execute_with(|| {
        // Set up initial state
        run_to_block(1);

        // Set the period end
        assert_ok!(OptimisticFunding::set_period_end(
            RuntimeOrigin::signed(treasury_account()),
            100
        ));

        // Check that the period end was updated
        assert_eq!(OptimisticFunding::current_period_end(), 100);
    });
}

#[test]
fn process_period_end_works() {
    new_test_ext().execute_with(|| {
        // Set up initial state
        run_to_block(1);

        // Set the period end
        assert_ok!(OptimisticFunding::set_period_end(
            RuntimeOrigin::signed(treasury_account()),
            10
        ));

        // Submit funding requests
        let description1: BoundedVec<u8, ConstU32<100>> = bounded_vec![b'a'; 50];
        assert_ok!(OptimisticFunding::submit_request(
            RuntimeOrigin::signed(1),
            100u64,
            description1.clone()
        ));

        let event_list1 = events();
        let request_hash1 = event_list1.iter().find_map(|event| {
            if let RuntimeEvent::OptimisticFunding(crate::Event::RequestSubmitted { request_hash, .. }) = event {
                Some(request_hash)
            } else {
                None
            }
        }).expect("Expected RequestSubmitted event");

        System::reset_events();
        let description2: BoundedVec<u8, ConstU32<100>> = bounded_vec![b'b'; 50];
        assert_ok!(OptimisticFunding::submit_request(
            RuntimeOrigin::signed(2),
            200u64,
            description2.clone()
        ));

        let event_list2 = events();
        let request_hash2 = event_list2.iter().find_map(|event| {
            if let RuntimeEvent::OptimisticFunding(crate::Event::RequestSubmitted { request_hash, .. }) = event {
                Some(request_hash)
            } else {
                None
            }
        }).expect("Expected RequestSubmitted event");

        // Vote on the requests
        System::reset_events();
        assert_ok!(OptimisticFunding::vote(
            RuntimeOrigin::signed(3),
            *request_hash1,
            30u64
        ));
        assert_ok!(OptimisticFunding::vote(
            RuntimeOrigin::signed(3),
            *request_hash2,
            50u64
        ));

        // Fund the accounts needed for transfers
        let optimistic_funding_account = <Test as Config>::PalletId::get().into_account_truncating();

        // Ensure both accounts have enough funds (ED + sufficient amount for transfers)
        let funding_amount = EXISTENTIAL_DEPOSIT as u64 + 1_000_000;
        Balances::make_free_balance_be(&optimistic_funding_account, funding_amount);
        Balances::make_free_balance_be(&treasury_account(), funding_amount);

        // Top up the treasury
        System::reset_events();
        assert_ok!(OptimisticFunding::top_up_treasury(
            RuntimeOrigin::signed(treasury_account()),
            500u64
        ));

        // Run to the period end
        System::reset_events();
        run_to_block(10);

        // Calculate the expected new period end
        let expected_new_period_end = 10 + <Test as Config>::FundingPeriod::get() as u64;

        // Check that the period end event was emitted with the NEW period end
        let final_events = events();
        assert!(final_events.iter().any(|event| matches!(event,
            RuntimeEvent::OptimisticFunding(crate::Event::PeriodEnded { period_end }) if *period_end == expected_new_period_end
        )));

        // Check that the next period end was set
        assert_eq!(
            OptimisticFunding::current_period_end(),
            expected_new_period_end
        );

        // Manually allocate funds to the requests since the pallet doesn't do this automatically
        System::reset_events();
        assert_ok!(OptimisticFunding::allocate_funds(
            RuntimeOrigin::signed(treasury_account()),
            *request_hash1
        ));

        assert_ok!(OptimisticFunding::allocate_funds(
            RuntimeOrigin::signed(treasury_account()),
            *request_hash2
        ));

        let allocation_events = events();

        // Check that the funds were allocated to the requests
        assert!(!FundingRequests::<Test>::contains_key(request_hash1));
        assert!(!FundingRequests::<Test>::contains_key(request_hash2));

        // Check that the treasury balance was updated
        assert_eq!(OptimisticFunding::treasury_balance(), 200);

        // Check that the funds allocation events were emitted
        assert!(allocation_events.iter().any(|event| matches!(event,
            RuntimeEvent::OptimisticFunding(crate::Event::FundsAllocated {
                request_hash,
                recipient: 2,
                amount: 200
            }) if request_hash == request_hash2
        )));

        assert!(allocation_events.iter().any(|event| matches!(event,
            RuntimeEvent::OptimisticFunding(crate::Event::FundsAllocated {
                request_hash,
                recipient: 1,
                amount: 100
            }) if request_hash == request_hash1
        )));
    });
}

#[test]
fn process_period_end_with_insufficient_funds() {
    new_test_ext().execute_with(|| {
        // Set up initial state
        run_to_block(1);

        // Set the period end
        assert_ok!(OptimisticFunding::set_period_end(
            RuntimeOrigin::signed(treasury_account()),
            10
        ));

        // Submit funding requests
        let description1: BoundedVec<u8, ConstU32<100>> = bounded_vec![b'a'; 50];
        assert_ok!(OptimisticFunding::submit_request(
            RuntimeOrigin::signed(1),
            300u64,
            description1.clone()
        ));

        let event_list1 = events();
        let request_hash1 = event_list1.iter().find_map(|event| {
            if let RuntimeEvent::OptimisticFunding(crate::Event::RequestSubmitted { request_hash, .. }) = event {
                Some(request_hash)
            } else {
                None
            }
        }).expect("Expected RequestSubmitted event");

        System::reset_events();
        let description2: BoundedVec<u8, ConstU32<100>> = bounded_vec![b'b'; 50];
        assert_ok!(OptimisticFunding::submit_request(
            RuntimeOrigin::signed(2),
            400u64,
            description2.clone()
        ));

        let event_list2 = events();
        let request_hash2 = event_list2.iter().find_map(|event| {
            if let RuntimeEvent::OptimisticFunding(crate::Event::RequestSubmitted { request_hash, .. }) = event {
                Some(request_hash)
            } else {
                None
            }
        }).expect("Expected RequestSubmitted event");

        // Vote on the requests
        System::reset_events();
        assert_ok!(OptimisticFunding::vote(
            RuntimeOrigin::signed(3),
            *request_hash1,
            30u64
        ));
        assert_ok!(OptimisticFunding::vote(
            RuntimeOrigin::signed(3),
            *request_hash2,
            50u64
        ));

        // Fund the accounts needed for transfers
        let optimistic_funding_account = <Test as Config>::PalletId::get().into_account_truncating();

        // Ensure both accounts have enough funds (ED + sufficient amount for transfers)
        let funding_amount = EXISTENTIAL_DEPOSIT as u64 + 1_000_000;
        Balances::make_free_balance_be(&optimistic_funding_account, funding_amount);
        Balances::make_free_balance_be(&treasury_account(), funding_amount);

        // Top up the treasury with insufficient funds for both requests
        System::reset_events();
        assert_ok!(OptimisticFunding::top_up_treasury(
            RuntimeOrigin::signed(treasury_account()),
            500u64
        ));

        // Run to the period end
        System::reset_events();
        run_to_block(10);

        // Calculate the expected new period end
        let expected_new_period_end = 10 + <Test as Config>::FundingPeriod::get() as u64;

        // Check that the period end event was emitted with the NEW period end
        let final_events = events();
        assert!(final_events.iter().any(|event| matches!(event,
            RuntimeEvent::OptimisticFunding(crate::Event::PeriodEnded { period_end }) if *period_end == expected_new_period_end
        )));

        // Check that the next period end was set
        assert_eq!(
            OptimisticFunding::current_period_end(),
            expected_new_period_end
        );

        // Manually allocate funds to the request with more votes first
        System::reset_events();
        assert_ok!(OptimisticFunding::allocate_funds(
            RuntimeOrigin::signed(treasury_account()),
            *request_hash2
        ));

        let allocation_events = events();

        // Check that the request with more votes was funded
        assert!(!FundingRequests::<Test>::contains_key(request_hash2));

        // Try to allocate funds to the second request, but it should fail due to insufficient funds
        assert_noop!(
            OptimisticFunding::allocate_funds(
                RuntimeOrigin::signed(treasury_account()),
                *request_hash1
            ),
            Error::<Test>::InsufficientTreasuryFunds
        );

        // Check that the request with fewer votes is still in storage
        assert!(FundingRequests::<Test>::contains_key(request_hash1));

        // Check that the treasury balance was updated
        assert_eq!(OptimisticFunding::treasury_balance(), 100);

        // Check that the funds allocation event was emitted for the funded request
        assert!(allocation_events.iter().any(|event| matches!(event,
            RuntimeEvent::OptimisticFunding(crate::Event::FundsAllocated {
                request_hash,
                recipient: 2,
                amount: 400
            }) if request_hash == request_hash2
        )));
    });
}

#[test]
fn rank_weighted_voting_works() {
    new_test_ext().execute_with(|| {
        // Set the funding period end
        let period_end = 101;
        assert_ok!(OptimisticFunding::set_period_end(
            RuntimeOrigin::signed(OptimisticFunding::treasury_account()),
            period_end
        ));

        // Create a funding request from account 1 (rank 0)
        let amount = 500;
        let description: BoundedVec<u8, ConstU32<100>> = bounded_vec![1, 2, 3];

        // Reset events before submitting the request
        System::reset_events();

        assert_ok!(OptimisticFunding::submit_request(
            RuntimeOrigin::signed(1),
            amount,
            description.clone()
        ));

        // Get the request hash from the emitted event
        let event_list = events();
        let request_hash = event_list.iter().find_map(|event| {
            if let RuntimeEvent::OptimisticFunding(crate::Event::RequestSubmitted { request_hash, .. }) = event {
                Some(request_hash)
            } else {
                None
            }
        }).expect("Expected RequestSubmitted event");

        // Account 2 (rank 1) votes with 100 tokens
        // Their vote should count as 100 * (1 + 1) = 200
        assert_ok!(OptimisticFunding::vote(RuntimeOrigin::signed(2), *request_hash, 100u64));

        // Check the request's votes_amount is updated correctly
        let request = FundingRequests::<Test>::get(request_hash).unwrap();
        assert_eq!(request.votes_count, 1);
        assert_eq!(request.votes_amount, 200); // 100 * (rank 1 + 1)

        // Account 3 (rank 2) votes with 100 tokens
        // Their vote should count as 100 * (2 + 1) = 300
        assert_ok!(OptimisticFunding::vote(RuntimeOrigin::signed(3), *request_hash, 100u64));

        // Check the request's votes_amount is updated correctly
        let request = FundingRequests::<Test>::get(request_hash).unwrap();
        assert_eq!(request.votes_count, 2);
        assert_eq!(request.votes_amount, 500); // 200 + 300 = 500

        // Account 4 (rank 3) votes with 100 tokens
        // Their vote should count as 100 * (3 + 1) = 400
        assert_ok!(OptimisticFunding::vote(RuntimeOrigin::signed(4), *request_hash, 100u64));

        // Check the request's votes_amount is updated correctly
        let request = FundingRequests::<Test>::get(request_hash).unwrap();
        assert_eq!(request.votes_count, 3);
        assert_eq!(request.votes_amount, 900); // 500 + 400 = 900

        // Account 5 (rank 4) cancels their vote
        // Their vote would have counted as 100 * (4 + 1) = 500
        assert_ok!(OptimisticFunding::vote(RuntimeOrigin::signed(5), *request_hash, 100u64));
        let request = FundingRequests::<Test>::get(request_hash).unwrap();
        assert_eq!(request.votes_amount, 1400); // 900 + 500 = 1400

        assert_ok!(OptimisticFunding::cancel_vote(RuntimeOrigin::signed(5), *request_hash));

        // Check the request's votes_amount is updated correctly
        let request = FundingRequests::<Test>::get(request_hash).unwrap();
        assert_eq!(request.votes_count, 3); // Still 3 because we don't decrement count on cancel
        assert_eq!(request.votes_amount, 900); // 1400 - 500 = 900

        // Account 8 (no rank) votes with 100 tokens
        // Their vote should count as 100 * (0 + 1) = 100
        assert_ok!(OptimisticFunding::vote(RuntimeOrigin::signed(8), *request_hash, 100u64));

        // Check the request's votes_amount is updated correctly
        let request = FundingRequests::<Test>::get(request_hash).unwrap();
        assert_eq!(request.votes_count, 4);
        assert_eq!(request.votes_amount, 1000); // 900 + 100 = 1000
    });
}
