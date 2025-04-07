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

//! Child-bounties pallet tests.

#![cfg(test)]

use super::{Event as ChildBountiesEvent, *};
use crate as pallet_child_bounties;
use crate::mock::{ChildBounties, *};

use frame_support::{
	assert_noop, assert_ok,
	dispatch::PostDispatchInfo,
	traits::{Currency, Hooks},
};
use sp_runtime::traits::BadOrigin;

#[test]
fn add_child_bounty() {
	new_test_ext().execute_with(|| {
		// TestProcedure.
		// 1, Create bounty & move to active state with enough bounty fund & parent curator.
		// 2, Parent curator adds child-bounty child-bounty-1, test for error like RequireCurator,
		//    InsufficientProposersBalance, InsufficientBountyBalance with invalid arguments.
		// 3, Parent curator adds child-bounty child-bounty-1, moves to "Approved" state &
		//    test for the event Added.
		// 4, Test for DB state of `Bounties` & `ChildBounties`.
		// 5, Observe fund transaction moment between Bounty, Child-bounty, Curator, child-bounty
		//    curator & beneficiary.

		// Given (make the parent bounty)
		// proposer = 0;
		// parent_curator = 4;
		// asset_kind = 1;
		// value = 50;
		// bounty_id = 0;
		// stash = 10;
		go_to_block(1);
		Balances::make_free_balance_be(&Treasury::account_id(), 101);
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(account_id(0)),
			Box::new(1),
			50,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), 0));
		approve_bounty_payment(
			Bounties::bounty_account_id(0, 1).expect("conversion failed"),
			0,
			1,
			50,
		);
		assert_eq!(Balances::free_balance(Bounties::account_id()), 101 - 50);
		assert_eq!(
			Balances::free_balance(Bounties::bounty_account_id(0, 1).expect("conversion failed")),
			50
		);
		let fee = 8;
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), 0, account_id(4), fee));
		Balances::make_free_balance_be(&account_id(4), 10);
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			account_id(10)
		));
		// This verifies that the accept curator logic took a deposit.
		let expected_deposit = CuratorDepositMultiplier::get() * fee;
		assert_eq!(Balances::reserved_balance(&account_id(4)), expected_deposit);
		assert_eq!(Balances::free_balance(&account_id(4)), 10 - expected_deposit);

		// When/Then (add child-bounty).
		// Acc-4 is the parent curator.
		// Call from invalid origin & check for error "RequireCurator".
		assert_noop!(
			ChildBounties::add_child_bounty(
				RuntimeOrigin::signed(account_id(0)),
				0,
				10,
				b"12345-p1".to_vec()
			),
			BountiesError::<Test>::RequireCurator,
		);

		// When/Then
		// Tiago: I don't think I need this verification anymore
		// Update the parent curator balance.
		Balances::make_free_balance_be(&account_id(4), 101);
		// parent curator fee is reserved on parent bounty account.
		assert_eq!(
			Balances::free_balance(Bounties::bounty_account_id(0, 1).expect("conversion failed")),
			50
		);
		assert_eq!(
			Balances::reserved_balance(
				Bounties::bounty_account_id(0, 1).expect("conversion failed")
			),
			0
		);
		assert_noop!(
			ChildBounties::add_child_bounty(
				RuntimeOrigin::signed(account_id(4)),
				0,
				50,
				b"12345-p1".to_vec()
			),
			Error::<Test>::InsufficientBountyBalance,
		);

		// When/Then
		assert_noop!(
			ChildBounties::add_child_bounty(
				RuntimeOrigin::signed(account_id(4)),
				0,
				100,
				b"12345-p1".to_vec()
			),
			Error::<Test>::InsufficientBountyBalance,
		);

		// When
		// Add child-bounty with valid value, which can be funded by parent bounty.
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(account_id(4)),
			0,
			10,
			b"12345-p1".to_vec()
		));

		// Then
		// Check for the event child-bounty added.
		assert_eq!(last_event(), ChildBountiesEvent::Added { index: 0, child_index: 0 });
		assert_eq!(Balances::free_balance(account_id(4)), 101);
		assert_eq!(Balances::reserved_balance(account_id(4)), expected_deposit);
		// DB check.
		// Check the child-bounty status.
		let payment_id = get_child_bounty_payment_id(0, 0, None).expect("no payment attempt");
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee: 0,
				curator_deposit: 0,
				status: ChildBountyStatus::Approved {
					payment_status: PaymentState::Attempted { id: payment_id }
				}
			}
		);
		// Check the child-bounty count.
		assert_eq!(pallet_child_bounties::ParentChildBounties::<Test>::get(0), 1);
		// Check the child-bounty description status.
		assert_eq!(
			pallet_child_bounties::ChildBountyDescriptionsV1::<Test>::get(0, 0).unwrap(),
			b"12345-p1".to_vec(),
		);

		// When (PaymentState::Success)
		approve_child_bounty_payment(ChildBounties::child_bounty_account_id(0, 0), 0, 0, 1, 10);

		// Then (funding returned)
		assert_eq!(
			Balances::free_balance(Bounties::bounty_account_id(0, 1).expect("conversion failed")),
			40
		);
		assert_eq!(Balances::free_balance(ChildBounties::child_bounty_account_id(0, 0)), 10);
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee: 0,
				curator_deposit: 0,
				status: ChildBountyStatus::Funded
			}
		);
	});
}

#[test]
fn child_bounty_assign_curator() {
	new_test_ext().execute_with(|| {
		// TestProcedure
		// 1, Create bounty & move to active state with enough bounty fund & parent curator.
		// 2, Parent curator adds child-bounty child-bounty-1, moves to "Active" state.
		// 3, Test for DB state of `ChildBounties`.

		// Given (make the parent bounty)
		// proposer = 0;
		// parent_curator = 4;
		// child_curator = 8;
		// asset_kind = 1;
		// value = 50;
		// bounty_id = 0;
		// parent/child stash = 10;
		go_to_block(1);
		Balances::make_free_balance_be(&Treasury::account_id(), 101);
		Balances::make_free_balance_be(&account_id(4), 101);
		Balances::make_free_balance_be(&account_id(8), 101);
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(account_id(0)),
			Box::new(1),
			50,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), 0));
		go_to_block(2);
		approve_bounty_payment(
			Bounties::bounty_account_id(0, 1).expect("conversion failed"),
			0,
			1,
			50,
		);
		let fee = 4;
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), 0, account_id(4), fee));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			account_id(10)
		));
		// Bounty account status before adding child-bounty.
		assert_eq!(
			Balances::free_balance(Bounties::bounty_account_id(0, 1).expect("conversion failed")),
			50
		);
		assert_eq!(
			Balances::reserved_balance(
				Bounties::bounty_account_id(0, 1).expect("conversion failed")
			),
			0
		);
		// Check the balance of parent curator.
		// Curator deposit is reserved for parent curator on parent bounty.
		let expected_deposit = Bounties::calculate_curator_deposit(&fee, 1).unwrap();
		assert_eq!(Balances::free_balance(account_id(4)), 101 - expected_deposit);
		assert_eq!(Balances::reserved_balance(account_id(4)), expected_deposit);
		// Add child-bounty.
		// Acc-4 is the parent curator & make sure enough deposit.
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(account_id(4)),
			0,
			10,
			b"12345-p1".to_vec()
		));
		approve_child_bounty_payment(ChildBounties::child_bounty_account_id(0, 0), 0, 0, 1, 10);
		assert_eq!(last_event(), ChildBountiesEvent::Added { index: 0, child_index: 0 });
		// Bounty account status after adding child-bounty.
		assert_eq!(
			Balances::free_balance(Bounties::bounty_account_id(0, 1).expect("conversion failed")),
			40
		);
		assert_eq!(
			Balances::reserved_balance(
				Bounties::bounty_account_id(0, 1).expect("conversion failed")
			),
			0
		);
		// Child-bounty account status.
		assert_eq!(Balances::free_balance(ChildBounties::child_bounty_account_id(0, 0)), 10);
		assert_eq!(Balances::reserved_balance(ChildBounties::child_bounty_account_id(0, 0)), 0);

		// When
		let fee = 6u64;
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			0,
			account_id(8),
			fee
		));

		// Then
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee,
				curator_deposit: 0,
				status: ChildBountyStatus::CuratorProposed { curator: account_id(8) },
			}
		);
		// Check the balance of parent curator.
		assert_eq!(Balances::free_balance(account_id(4)), 101 - expected_deposit);
		assert_eq!(Balances::reserved_balance(account_id(4)), expected_deposit);

		// When/Then
		assert_noop!(
			ChildBounties::accept_curator(
				RuntimeOrigin::signed(account_id(3)),
				0,
				0,
				account_id(10)
			),
			BountiesError::<Test>::RequireCurator,
		);

		// When
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(account_id(8)),
			0,
			0,
			account_id(10)
		));

		// Then
		let expected_child_deposit = CuratorDepositMultiplier::get() * fee;
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee,
				curator_deposit: expected_child_deposit,
				status: ChildBountyStatus::Active {
					curator: account_id(8),
					curator_stash: account_id(10),
					update_due: 12
				},
			}
		);
		// Deposit for child-bounty curator deposit is reserved.
		assert_eq!(Balances::free_balance(account_id(8)), 101 - expected_child_deposit);
		assert_eq!(Balances::reserved_balance(account_id(8)), expected_child_deposit);
		// Bounty account status at exit.
		assert_eq!(
			Balances::free_balance(Bounties::bounty_account_id(0, 1).expect("conversion failed")),
			40
		);
		assert_eq!(
			Balances::reserved_balance(
				Bounties::bounty_account_id(0, 1).expect("conversion failed")
			),
			0
		);
		// Child-bounty account status at exit.
		assert_eq!(Balances::free_balance(ChildBounties::child_bounty_account_id(0, 0)), 10);
		assert_eq!(Balances::reserved_balance(ChildBounties::child_bounty_account_id(0, 0)), 0);
		// Treasury account status at exit.
		assert_eq!(Balances::free_balance(Treasury::account_id()), 26);
		assert_eq!(Balances::reserved_balance(Treasury::account_id()), 0);
	});
}

#[test]
fn award_claim_child_bounty() {
	new_test_ext().execute_with(|| {
		// Given (make the parent bounty)
		// proposer = 0;
		// parent_curator = 4;
		// child_curator = 8;
		// asset_kind = 1;
		// value = 50;
		// bounty_id = 0;
		// parent/child stash = 10;
		go_to_block(1);
		Balances::make_free_balance_be(&Treasury::account_id(), 101);
		assert_eq!(Balances::free_balance(Treasury::account_id()), 101);
		assert_eq!(Balances::reserved_balance(Treasury::account_id()), 0);
		// Bounty curator initial balance.
		Balances::make_free_balance_be(&account_id(4), 101); // Parent-bounty curator.
		Balances::make_free_balance_be(&account_id(8), 101); // Child-bounty curator.
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(account_id(0)),
			Box::new(1),
			50,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), 0));
		approve_bounty_payment(
			Bounties::bounty_account_id(0, 1).expect("conversion failed"),
			0,
			1,
			50,
		);
		go_to_block(2);
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), 0, account_id(4), 6));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			account_id(10)
		));
		// Child-bounty.
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(account_id(4)),
			0,
			10,
			b"12345-p1".to_vec()
		));
		assert_eq!(last_event(), ChildBountiesEvent::Added { index: 0, child_index: 0 });
		approve_child_bounty_payment(ChildBounties::child_bounty_account_id(0, 0), 0, 0, 1, 10);
		// Propose and accept curator for child-bounty.
		let fee = 8;
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			0,
			account_id(8),
			fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(account_id(8)),
			0,
			0,
			account_id(10)
		));

		// When/Then (Award child-bount)
		// Test for non child-bounty curator.
		assert_noop!(
			ChildBounties::award_child_bounty(
				RuntimeOrigin::signed(account_id(3)),
				0,
				0,
				account_id(7)
			),
			BountiesError::<Test>::RequireCurator,
		);

		// When
		assert_ok!(ChildBounties::award_child_bounty(
			RuntimeOrigin::signed(account_id(8)),
			0,
			0,
			account_id(7)
		));

		// Then
		let expected_deposit = CuratorDepositMultiplier::get() * fee;
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee,
				curator_deposit: expected_deposit,
				status: ChildBountyStatus::PendingPayout {
					curator: account_id(8),
					beneficiary: account_id(7),
					unlock_at: 5,
					curator_stash: account_id(10)
				},
			}
		);

		// When/Then (Claim child-bounty)
		// Test for Premature condition.
		assert_noop!(
			ChildBounties::claim_child_bounty(RuntimeOrigin::signed(account_id(7)), 0, 0),
			BountiesError::<Test>::Premature
		);

		// When
		go_to_block(9); // block_number >= unlock_at
		assert_ok!(ChildBounties::claim_child_bounty(RuntimeOrigin::signed(account_id(7)), 0, 0));

		// Then (PaymentState::Attempted)
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee,
				curator_deposit: expected_deposit,
				status: ChildBountyStatus::PayoutAttempted {
					curator: account_id(8),
					beneficiary: (account_id(7), PaymentState::Attempted { id: 3 }),
					curator_stash: (account_id(10), PaymentState::Attempted { id: 2 })
				},
			}
		);

		// When
		approve_child_bounty_payment(account_id(7), 0, 0, 1, 2); // pay child-bounty beneficiary
		approve_child_bounty_payment(account_id(10), 0, 0, 1, 8); // pay child-bounty curator_stash

		// Then (PaymentState::Success)
		// Ensure child-bounty curator is paid deposit refund.
		assert_eq!(Balances::free_balance(account_id(8)), 101);
		assert_eq!(Balances::reserved_balance(account_id(8)), 0);
		// Ensure child-bounty curator stash is paid with curator fee.
		assert_eq!(Balances::free_balance(account_id(10)), fee);
		// Ensure executor is paid with beneficiary amount.
		assert_eq!(Balances::free_balance(account_id(7)), 10 - fee);
		assert_eq!(Balances::reserved_balance(account_id(7)), 0);
		// Child-bounty account status.
		assert_eq!(Balances::free_balance(ChildBounties::child_bounty_account_id(0, 0)), 0);
		assert_eq!(Balances::reserved_balance(ChildBounties::child_bounty_account_id(0, 0)), 0);
		// Check the child-bounty count.
		assert_eq!(pallet_child_bounties::ParentChildBounties::<Test>::get(0), 0);
	});
}

#[test]
fn close_child_bounty_added() {
	new_test_ext().execute_with(|| {
		// Given (make the parent bounty)
		go_to_block(1);
		Balances::make_free_balance_be(&Treasury::account_id(), 101);
		assert_eq!(Balances::free_balance(Treasury::account_id()), 101);
		assert_eq!(Balances::reserved_balance(Treasury::account_id()), 0);
		// Bounty curator initial balance.
		Balances::make_free_balance_be(&account_id(4), 101); // Parent-bounty curator.
		Balances::make_free_balance_be(&account_id(8), 101); // Child-bounty curator.
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(account_id(0)),
			Box::new(1),
			50,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), 0));
		approve_bounty_payment(
			Bounties::bounty_account_id(0, 1).expect("conversion failed"),
			0,
			1,
			50,
		);
		go_to_block(2);
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), 0, account_id(4), 6));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			account_id(10)
		));
		// Child-bounty.
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(account_id(4)),
			0,
			10,
			b"12345-p1".to_vec()
		));
		approve_child_bounty_payment(ChildBounties::child_bounty_account_id(0, 0), 0, 0, 1, 10);
		assert_eq!(last_event(), ChildBountiesEvent::Added { index: 0, child_index: 0 });
		go_to_block(4);

		// When/Then (Wrong origin)
		assert_noop!(
			ChildBounties::close_child_bounty(RuntimeOrigin::signed(account_id(7)), 0, 0),
			BadOrigin
		);

		// When/Then (Wrong origin)
		assert_noop!(
			ChildBounties::close_child_bounty(RuntimeOrigin::signed(account_id(8)), 0, 0),
			BadOrigin
		);

		// When/Then (Correct origin - parent curator)
		assert_ok!(ChildBounties::close_child_bounty(RuntimeOrigin::signed(account_id(4)), 0, 0));

		// Then (PaymentState::Attempted)
		let payment_id = get_child_bounty_payment_id(0, 0, None).expect("no payment attempt");
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee: 0,
				curator_deposit: 0,
				status: ChildBountyStatus::RefundAttempted {
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);

		// When
		approve_child_bounty_payment(ChildBounties::child_bounty_account_id(0, 0), 0, 0, 1, 10);

		// Then (PaymentState::Success)
		// Check the child-bounty count.
		assert_eq!(pallet_child_bounties::ParentChildBounties::<Test>::get(0), 0);
		// Parent-bounty account status.
		assert_eq!(
			Balances::free_balance(Bounties::bounty_account_id(0, 1).expect("conversion failed")),
			50
		);
		assert_eq!(
			Balances::reserved_balance(
				Bounties::bounty_account_id(0, 1).expect("conversion failed")
			),
			0
		);
		// Child-bounty account status.
		assert_eq!(Balances::free_balance(ChildBounties::child_bounty_account_id(0, 0)), 0);
		assert_eq!(Balances::reserved_balance(ChildBounties::child_bounty_account_id(0, 0)), 0);
	});
}

#[test]
fn close_child_bounty_active() {
	new_test_ext().execute_with(|| {
		// Given (make the parent bounty)
		go_to_block(1);
		Balances::make_free_balance_be(&Treasury::account_id(), 101);
		assert_eq!(Balances::free_balance(Treasury::account_id()), 101);
		assert_eq!(Balances::reserved_balance(Treasury::account_id()), 0);
		// Bounty curator initial balance.
		Balances::make_free_balance_be(&account_id(4), 101); // Parent-bounty curator.
		Balances::make_free_balance_be(&account_id(8), 101); // Child-bounty curator.
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(account_id(0)),
			Box::new(1),
			50,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), 0));
		approve_bounty_payment(
			Bounties::bounty_account_id(0, 1).expect("conversion failed"),
			0,
			1,
			50,
		);
		go_to_block(2);
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), 0, account_id(4), 6));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			account_id(10)
		));
		// Child-bounty.
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(account_id(4)),
			0,
			10,
			b"12345-p1".to_vec()
		));
		approve_child_bounty_payment(ChildBounties::child_bounty_account_id(0, 0), 0, 0, 1, 10);
		assert_eq!(last_event(), ChildBountiesEvent::Added { index: 0, child_index: 0 });
		// Propose and accept curator for child-bounty.
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			0,
			account_id(8),
			2
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(account_id(8)),
			0,
			0,
			account_id(10)
		));

		// When (Close child-bounty in active state).
		assert_ok!(ChildBounties::close_child_bounty(RuntimeOrigin::signed(account_id(4)), 0, 0));

		// Then (PaymentState::Attempted)
		let payment_id = get_child_bounty_payment_id(0, 0, None).expect("no payment attempt");
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee: 2,
				curator_deposit: 3,
				status: ChildBountyStatus::RefundAttempted {
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);
		// Ensure child-bounty curator balance is unreserved.
		assert_eq!(Balances::free_balance(account_id(8)), 101);
		assert_eq!(Balances::reserved_balance(account_id(8)), 0);

		// When
		approve_child_bounty_payment(ChildBounties::child_bounty_account_id(0, 0), 0, 0, 1, 10);

		// Then (PaymentState::Success)
		// Check the child-bounty count.
		assert_eq!(pallet_child_bounties::ParentChildBounties::<Test>::get(0), 0);
		// Parent-bounty account status.
		assert_eq!(
			Balances::free_balance(Bounties::bounty_account_id(0, 1).expect("conversion failed")),
			50
		);
		assert_eq!(
			Balances::reserved_balance(
				Bounties::bounty_account_id(0, 1).expect("conversion failed")
			),
			0
		);
		// Child-bounty account status.
		assert_eq!(Balances::free_balance(ChildBounties::child_bounty_account_id(0, 0)), 0);
		assert_eq!(Balances::reserved_balance(ChildBounties::child_bounty_account_id(0, 0)), 0);
	});
}

#[test]
fn close_child_bounty_pending() {
	new_test_ext().execute_with(|| {
		// Givem (Make the parent bounty)
		go_to_block(1);
		Balances::make_free_balance_be(&Treasury::account_id(), 101);
		assert_eq!(Balances::free_balance(Treasury::account_id()), 101);
		assert_eq!(Balances::reserved_balance(Treasury::account_id()), 0);
		// Bounty curator initial balance.
		Balances::make_free_balance_be(&account_id(4), 101); // Parent-bounty curator.
		Balances::make_free_balance_be(&account_id(8), 101); // Child-bounty curator.
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(account_id(0)),
			Box::new(1),
			50,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), 0));
		approve_bounty_payment(
			Bounties::bounty_account_id(0, 1).expect("conversion failed"),
			0,
			1,
			50,
		);
		go_to_block(2);
		let parent_fee = 6;
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), 0, account_id(4), parent_fee));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			account_id(10)
		));
		// Child-bounty.
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(account_id(4)),
			0,
			10,
			b"12345-p1".to_vec()
		));
		approve_child_bounty_payment(ChildBounties::child_bounty_account_id(0, 0), 0, 0, 1, 10);
		assert_eq!(last_event(), ChildBountiesEvent::Added { index: 0, child_index: 0 });
		// Propose and accept curator for child-bounty.
		let child_fee = 4;
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			0,
			account_id(8),
			child_fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(account_id(8)),
			0,
			0,
			account_id(10)
		));
		let expected_child_deposit = CuratorDepositMin::get();
		assert_ok!(ChildBounties::award_child_bounty(
			RuntimeOrigin::signed(account_id(8)),
			0,
			0,
			account_id(7)
		));

		// When (Close child-bounty in pending_payout state)
		assert_noop!(
			ChildBounties::close_child_bounty(RuntimeOrigin::signed(account_id(4)), 0, 0),
			BountiesError::<Test>::PendingPayout
		);

		// Then
		// Check the child-bounty count.
		assert_eq!(pallet_child_bounties::ParentChildBounties::<Test>::get(0), 1);
		// Ensure no changes in child-bounty curator balance.
		assert_eq!(Balances::reserved_balance(account_id(8)), expected_child_deposit);
		assert_eq!(Balances::free_balance(account_id(8)), 101 - expected_child_deposit);
		// Child-bounty account status.
		assert_eq!(Balances::free_balance(ChildBounties::child_bounty_account_id(0, 0)), 10);
		assert_eq!(Balances::reserved_balance(ChildBounties::child_bounty_account_id(0, 0)), 0);
	});
}

#[test]
fn child_bounty_added_unassign_curator() {
	new_test_ext().execute_with(|| {
		// Given (Make the parent bounty)
		go_to_block(1);
		Balances::make_free_balance_be(&Treasury::account_id(), 101);
		assert_eq!(Balances::free_balance(Treasury::account_id()), 101);
		assert_eq!(Balances::reserved_balance(Treasury::account_id()), 0);
		// Bounty curator initial balance.
		Balances::make_free_balance_be(&account_id(4), 101); // Parent-bounty curator.
		Balances::make_free_balance_be(&account_id(8), 101); // Child-bounty curator.
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(account_id(0)),
			Box::new(1),
			50,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), 0));
		approve_bounty_payment(
			Bounties::bounty_account_id(0, 1).expect("conversion failed"),
			0,
			1,
			50,
		);
		go_to_block(2);
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), 0, account_id(4), 6));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			account_id(10)
		));
		// Child-bounty.
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(account_id(4)),
			0,
			10,
			b"12345-p1".to_vec()
		));
		assert_eq!(last_event(), ChildBountiesEvent::Added { index: 0, child_index: 0 });
		approve_child_bounty_payment(ChildBounties::child_bounty_account_id(0, 0), 0, 0, 1, 10);

		// When/Then (Unassign curator in added state)
		assert_noop!(
			ChildBounties::unassign_curator(RuntimeOrigin::signed(account_id(4)), 0, 0),
			BountiesError::<Test>::UnexpectedStatus
		);
	});
}

#[test]
fn child_bounty_curator_proposed_unassign_curator() {
	new_test_ext().execute_with(|| {
		// Given (Make the parent bounty)
		go_to_block(1);
		Balances::make_free_balance_be(&Treasury::account_id(), 101);
		assert_eq!(Balances::free_balance(Treasury::account_id()), 101);
		assert_eq!(Balances::reserved_balance(Treasury::account_id()), 0);
		// Bounty curator initial balance.
		Balances::make_free_balance_be(&account_id(4), 101); // Parent-bounty curator.
		Balances::make_free_balance_be(&account_id(8), 101); // Child-bounty curator.
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(account_id(0)),
			Box::new(1),
			50,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), 0));
		approve_bounty_payment(
			Bounties::bounty_account_id(0, 1).expect("conversion failed"),
			0,
			1,
			50,
		);
		go_to_block(2);
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), 0, account_id(4), 6));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			account_id(10)
		));
		// Child-bounty.
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(account_id(4)),
			0,
			10,
			b"12345-p1".to_vec()
		));
		assert_eq!(last_event(), ChildBountiesEvent::Added { index: 0, child_index: 0 });
		approve_child_bounty_payment(ChildBounties::child_bounty_account_id(0, 0), 0, 0, 1, 10);
		// Propose curator for child-bounty.
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			0,
			account_id(8),
			2
		));
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee: 2,
				curator_deposit: 0,
				status: ChildBountyStatus::CuratorProposed { curator: account_id(8) },
			}
		);

		// When/Then (Random account cannot unassign the curator when in proposed state)
		assert_noop!(
			ChildBounties::unassign_curator(RuntimeOrigin::signed(account_id(99)), 0, 0),
			BadOrigin
		);

		// When (Unassign curator)
		assert_ok!(ChildBounties::unassign_curator(RuntimeOrigin::signed(account_id(4)), 0, 0));

		// Then
		// Verify updated child-bounty status.
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee: 2,
				curator_deposit: 0,
				status: ChildBountyStatus::Funded,
			}
		);
	});
}

#[test]
fn child_bounty_active_unassign_curator() {
	// Covers all scenarios with all origin types.
	// Step 1: Setup bounty, child bounty.
	// Step 2: Assign, accept curator for child bounty. Unassign from reject origin. Should slash.
	// Step 3: Assign, accept another curator for child bounty. Unassign from parent-bounty curator.
	// Should slash. Step 4: Assign, accept another curator for child bounty. Unassign from
	// child-bounty curator. Should NOT slash. Step 5: Assign, accept another curator for child
	// bounty. Unassign from random account. Should slash.
	new_test_ext().execute_with(|| {
		// Given (Make the parent bounty)
		go_to_block(1);
		Balances::make_free_balance_be(&Treasury::account_id(), 101);
		assert_eq!(Balances::free_balance(Treasury::account_id()), 101);
		assert_eq!(Balances::reserved_balance(Treasury::account_id()), 0);
		// Bounty curator initial balance.
		Balances::make_free_balance_be(&account_id(4), 101); // Parent-bounty curator.
		Balances::make_free_balance_be(&account_id(6), 101); // Child-bounty curator 1.
		Balances::make_free_balance_be(&account_id(7), 101); // Child-bounty curator 2.
		Balances::make_free_balance_be(&account_id(8), 101); // Child-bounty curator 3.
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(account_id(0)),
			Box::new(1),
			50,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), 0));
		approve_bounty_payment(
			Bounties::bounty_account_id(0, 1).expect("conversion failed"),
			0,
			1,
			50,
		);
		go_to_block(2);
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), 0, account_id(4), 6));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			account_id(10)
		));
		// Create Child-bounty.
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(account_id(4)),
			0,
			10,
			b"12345-p1".to_vec()
		));
		assert_eq!(last_event(), ChildBountiesEvent::Added { index: 0, child_index: 0 });
		approve_child_bounty_payment(ChildBounties::child_bounty_account_id(0, 0), 0, 0, 1, 10);
		go_to_block(3);
		// Propose and accept curator for child-bounty.
		let fee = 6;
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			0,
			account_id(8),
			fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(account_id(8)),
			0,
			0,
			account_id(10)
		));
		let expected_child_deposit = CuratorDepositMultiplier::get() * fee;
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee,
				curator_deposit: expected_child_deposit,
				status: ChildBountyStatus::Active {
					curator: account_id(8),
					curator_stash: account_id(10),
					update_due: 12
				},
			}
		);

		// When (Unassign curator - from reject origin)
		go_to_block(4);
		assert_ok!(ChildBounties::unassign_curator(RuntimeOrigin::root(), 0, 0));

		// Then
		// Verify updated child-bounty status.
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee,
				curator_deposit: 0,
				status: ChildBountyStatus::Funded,
			}
		);
		// Ensure child-bounty curator was slashed.
		assert_eq!(Balances::free_balance(account_id(8)), 101 - expected_child_deposit);
		assert_eq!(Balances::reserved_balance(account_id(8)), 0); // slashed

		// Given (Propose and accept curator for child-bounty again)
		let fee = 2;
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			0,
			account_id(7),
			fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(account_id(7)),
			0,
			0,
			account_id(10)
		));
		let expected_child_deposit = CuratorDepositMin::get();
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee,
				curator_deposit: expected_child_deposit,
				status: ChildBountyStatus::Active {
					curator: account_id(7),
					curator_stash: account_id(10),
					update_due: 12
				},
			}
		);
		go_to_block(5);

		// When (Unassign curator again - from parent curator)
		assert_ok!(ChildBounties::unassign_curator(RuntimeOrigin::signed(account_id(4)), 0, 0));

		// Then
		// Verify updated child-bounty status.
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee: 2,
				curator_deposit: 0,
				status: ChildBountyStatus::Funded,
			}
		);
		// Ensure child-bounty curator was slashed.
		assert_eq!(Balances::free_balance(account_id(7)), 101 - expected_child_deposit);
		assert_eq!(Balances::reserved_balance(account_id(7)), 0); // slashed
															// Propose and accept curator for child-bounty again.
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			0,
			account_id(6),
			2
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(account_id(6)),
			0,
			0,
			account_id(10)
		));
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee,
				curator_deposit: expected_child_deposit,
				status: ChildBountyStatus::Active {
					curator: account_id(6),
					curator_stash: account_id(10),
					update_due: 12
				},
			}
		);

		// When (Unassign curator again - from child-bounty curator)
		go_to_block(6);
		assert_ok!(ChildBounties::unassign_curator(RuntimeOrigin::signed(account_id(6)), 0, 0));

		// Then
		// Verify updated child-bounty status.
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee: 2,
				curator_deposit: 0,
				status: ChildBountyStatus::Funded,
			}
		);
		// Ensure child-bounty curator was **not** slashed.
		assert_eq!(Balances::free_balance(account_id(6)), 101); // not slashed
		assert_eq!(Balances::reserved_balance(account_id(6)), 0);

		// Given (Propose and accept curator for child-bounty one last time)
		let fee = 2;
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			0,
			account_id(6),
			fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(account_id(6)),
			0,
			0,
			account_id(10)
		));
		let expected_child_deposit = CuratorDepositMin::get();
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee,
				curator_deposit: expected_child_deposit,
				status: ChildBountyStatus::Active {
					curator: account_id(6),
					curator_stash: account_id(10),
					update_due: 12
				},
			}
		);

		// When/ Then (Unassign curator again - from non curator; non reject origin; some random
		// guy) Bounty update period is not yet complete.
		go_to_block(7);
		assert_noop!(
			ChildBounties::unassign_curator(RuntimeOrigin::signed(account_id(3)), 0, 0),
			BountiesError::<Test>::Premature
		);

		// When (Unassign child curator from random account after inactivity)
		go_to_block(20);
		assert_ok!(ChildBounties::unassign_curator(RuntimeOrigin::signed(account_id(3)), 0, 0));

		// Then
		// Verify updated child-bounty status.
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee: 2,
				curator_deposit: 0,
				status: ChildBountyStatus::Funded,
			}
		);
		// Ensure child-bounty curator was slashed.
		assert_eq!(Balances::free_balance(account_id(6)), 101 - expected_child_deposit); // slashed
		assert_eq!(Balances::reserved_balance(account_id(6)), 0);
	});
}

#[test]
fn parent_bounty_inactive_unassign_curator_child_bounty() {
	// Unassign curator when parent bounty in not in active state.
	// This can happen when the curator of parent bounty has been unassigned.
	new_test_ext().execute_with(|| {
		// Given (Make the parent bounty)
		go_to_block(1);
		Balances::make_free_balance_be(&Treasury::account_id(), 101);
		assert_eq!(Balances::free_balance(Treasury::account_id()), 101);
		assert_eq!(Balances::reserved_balance(Treasury::account_id()), 0);
		// Bounty curator initial balance.
		Balances::make_free_balance_be(&account_id(4), 101); // Parent-bounty curator 1.
		Balances::make_free_balance_be(&account_id(5), 101); // Parent-bounty curator 2.
		Balances::make_free_balance_be(&account_id(6), 101); // Child-bounty curator 1.
		Balances::make_free_balance_be(&account_id(7), 101); // Child-bounty curator 2.
		Balances::make_free_balance_be(&account_id(8), 101); // Child-bounty curator 3.
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(account_id(0)),
			Box::new(1),
			50,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), 0));
		approve_bounty_payment(
			Bounties::bounty_account_id(0, 1).expect("conversion failed"),
			0,
			1,
			50,
		);
		go_to_block(2);
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), 0, account_id(4), 6));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			account_id(10)
		));
		// Create Child-bounty.
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(account_id(4)),
			0,
			10,
			b"12345-p1".to_vec()
		));
		assert_eq!(last_event(), ChildBountiesEvent::Added { index: 0, child_index: 0 });
		approve_child_bounty_payment(ChildBounties::child_bounty_account_id(0, 0), 0, 0, 1, 10);
		go_to_block(3);
		// Propose and accept curator for child-bounty.
		let fee = 8;
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			0,
			account_id(8),
			fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(account_id(8)),
			0,
			0,
			account_id(10)
		));
		let expected_child_deposit = CuratorDepositMultiplier::get() * fee;
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee,
				curator_deposit: expected_child_deposit,
				status: ChildBountyStatus::Active {
					curator: account_id(8),
					curator_stash: account_id(10),
					update_due: 12
				},
			}
		);

		// When/Then (Unassign parent bounty curator)
		go_to_block(4);
		assert_ok!(Bounties::unassign_curator(RuntimeOrigin::root(), 0));

		// When/ Then (Try unassign child-bounty curator - from non curator; non reject
		// origin; some random guy. Bounty update period is not yet complete)
		go_to_block(5);
		assert_noop!(
			ChildBounties::unassign_curator(RuntimeOrigin::signed(account_id(3)), 0, 0),
			Error::<Test>::ParentBountyNotActive
		);

		// When (Unassign curator - from reject origin)
		assert_ok!(ChildBounties::unassign_curator(RuntimeOrigin::root(), 0, 0));

		// Then
		// Verify updated child-bounty status.
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee,
				curator_deposit: 0,
				status: ChildBountyStatus::Funded,
			}
		);
		// Ensure child-bounty curator was slashed.
		assert_eq!(Balances::free_balance(account_id(8)), 101 - expected_child_deposit);
		assert_eq!(Balances::reserved_balance(account_id(8)), 0); // slashed

		// Given
		go_to_block(6);
		// Propose and accept curator for parent-bounty again.
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), 0, account_id(5), 6));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(account_id(5)),
			0,
			account_id(10)
		));
		go_to_block(7);
		// Propose and accept curator for child-bounty again.
		let fee = 2;
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(account_id(5)),
			0,
			0,
			account_id(7),
			fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(account_id(7)),
			0,
			0,
			account_id(10)
		));
		let expected_deposit = CuratorDepositMin::get();
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee,
				curator_deposit: expected_deposit,
				status: ChildBountyStatus::Active {
					curator: account_id(7),
					curator_stash: account_id(10),
					update_due: 16
				},
			}
		);
		go_to_block(8);

		// When/Then
		assert_noop!(
			ChildBounties::unassign_curator(RuntimeOrigin::signed(account_id(3)), 0, 0),
			BountiesError::<Test>::Premature
		);

		// When/Then (Unassign parent bounty curator again)
		assert_ok!(Bounties::unassign_curator(RuntimeOrigin::signed(account_id(5)), 0));

		// When (Unassign curator again - from parent curator)
		go_to_block(9);
		assert_ok!(ChildBounties::unassign_curator(RuntimeOrigin::signed(account_id(7)), 0, 0));

		// Then
		// Verify updated child-bounty status.
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee: 2,
				curator_deposit: 0,
				status: ChildBountyStatus::Funded,
			}
		);
		// Ensure child-bounty curator was not slashed.
		assert_eq!(Balances::free_balance(account_id(7)), 101);
		assert_eq!(Balances::reserved_balance(account_id(7)), 0); // slashed
	});
}

#[test]
fn close_parent_with_child_bounty() {
	new_test_ext().execute_with(|| {
		// Given (Make the parent bounty)
		go_to_block(1);
		Balances::make_free_balance_be(&Treasury::account_id(), 101);
		assert_eq!(Balances::free_balance(Treasury::account_id()), 101);
		assert_eq!(Balances::reserved_balance(Treasury::account_id()), 0);
		// Bounty curator initial balance.
		Balances::make_free_balance_be(&account_id(4), 101); // Parent-bounty curator.
		Balances::make_free_balance_be(&account_id(8), 101); // Child-bounty curator.
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(account_id(0)),
			Box::new(1),
			50,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), 0));
		approve_bounty_payment(
			Bounties::bounty_account_id(0, 1).expect("conversion failed"),
			0,
			1,
			50,
		);

		// When/Then (Try add child-bounty)
		// Should fail, parent bounty not active yet.
		assert_noop!(
			ChildBounties::add_child_bounty(
				RuntimeOrigin::signed(account_id(4)),
				0,
				10,
				b"12345-p1".to_vec()
			),
			Error::<Test>::ParentBountyNotActive
		);

		// Given
		go_to_block(2);
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), 0, account_id(4), 6));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			account_id(10)
		));
		// Child-bounty.
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(account_id(4)),
			0,
			10,
			b"12345-p1".to_vec()
		));
		assert_eq!(last_event(), ChildBountiesEvent::Added { index: 0, child_index: 0 });
		approve_child_bounty_payment(ChildBounties::child_bounty_account_id(0, 0), 0, 0, 1, 10);

		// When/Then (Try close parent-bounty)
		// Child bounty active, can't close parent.
		go_to_block(4);
		assert_noop!(
			Bounties::close_bounty(RuntimeOrigin::root(), 0),
			BountiesError::<Test>::HasActiveChildBounty
		);

		// Given (Close child-bounty)
		assert_ok!(ChildBounties::close_child_bounty(RuntimeOrigin::root(), 0, 0));
		approve_child_bounty_payment(ChildBounties::child_bounty_account_id(0, 0), 0, 0, 1, 10);
		// Check the child-bounty count.
		assert_eq!(pallet_child_bounties::ParentChildBounties::<Test>::get(0), 0);
		assert_eq!(pallet_child_bounties::ParentTotalChildBounties::<Test>::get(0), 1);

		// When (Try close parent-bounty again)
		// Should pass this time.
		assert_ok!(Bounties::close_bounty(RuntimeOrigin::root(), 0));
		approve_bounty_payment(Bounties::account_id(), 0, 1, 50);

		// Then
		// Check the total count is removed after the parent bounty removal.
		assert_eq!(pallet_child_bounties::ParentTotalChildBounties::<Test>::get(0), 0);
	});
}

#[test]
fn children_curator_fee_calculation_test() {
	// Tests the calculation of subtracting child-bounty curator fee
	// from parent bounty fee when claiming bounties.
	new_test_ext().execute_with(|| {
		// Given (Make the parent bounty)
		// parent-bounty curator = 4
		// parent-bounty beneficiary = 9
		// parent-bounty curator_stash = 10
		// parent-bounty value = 50
		// parent-bounty fee = 6
		// child-bounty curator = 8
		// child-bounty beneficiary = 7
		// child-bounty curator_stash = 10
		// child-bounty value = 10
		// child-bounty fee = 6
		go_to_block(1);
		Balances::make_free_balance_be(&Treasury::account_id(), 101);
		assert_eq!(Balances::free_balance(Treasury::account_id()), 101);
		assert_eq!(Balances::reserved_balance(Treasury::account_id()), 0);
		// Bounty curator initial balance.
		Balances::make_free_balance_be(&account_id(4), 101); // Parent-bounty curator.
		Balances::make_free_balance_be(&account_id(8), 101); // Child-bounty curator.
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(account_id(0)),
			Box::new(1),
			50,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), 0));
		approve_bounty_payment(
			Bounties::bounty_account_id(0, 1).expect("conversion failed"),
			0,
			1,
			50,
		);
		go_to_block(2);
		assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), 0, account_id(4), 6));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			account_id(10)
		));
		// Child-bounty.
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(account_id(4)),
			0,
			10,
			b"12345-p1".to_vec()
		));
		approve_child_bounty_payment(ChildBounties::child_bounty_account_id(0, 0), 0, 0, 1, 10);
		assert_eq!(last_event(), ChildBountiesEvent::Added { index: 0, child_index: 0 });
		go_to_block(4);
		let fee = 6;
		// Propose curator for child-bounty.
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(account_id(4)),
			0,
			0,
			account_id(8),
			fee
		));
		// Check curator fee added to the sum.
		assert_eq!(pallet_child_bounties::ChildrenCuratorFees::<Test>::get(0), fee);
		// Accept curator for child-bounty.
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(account_id(8)),
			0,
			0,
			account_id(10)
		));
		// Award child-bounty.
		assert_ok!(ChildBounties::award_child_bounty(
			RuntimeOrigin::signed(account_id(8)),
			0,
			0,
			account_id(7)
		));
		let expected_child_deposit = CuratorDepositMultiplier::get() * fee;
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(0, 0).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: 10,
				fee,
				curator_deposit: expected_child_deposit,
				status: ChildBountyStatus::PendingPayout {
					curator: account_id(8),
					beneficiary: account_id(7),
					unlock_at: 7,
					curator_stash: account_id(10),
				},
			}
		);
		go_to_block(9);

		// When
		// Claim child-bounty.
		assert_ok!(ChildBounties::claim_child_bounty(RuntimeOrigin::signed(account_id(7)), 0, 0));
		approve_child_bounty_payment(account_id(10), 0, 0, 1, 6); // pay child-bounty curator_stash
		approve_child_bounty_payment(account_id(7), 0, 0, 1, 4); // pay child-bounty beneficiary

		// Then
		// Check the child-bounty count.
		assert_eq!(pallet_child_bounties::ParentChildBounties::<Test>::get(0), 0);

		// Given
		// Award the parent bounty.
		assert_ok!(Bounties::award_bounty(RuntimeOrigin::signed(account_id(4)), 0, account_id(9)));
		go_to_block(15);
		// Check the total count.
		assert_eq!(pallet_child_bounties::ParentTotalChildBounties::<Test>::get(0), 1);

		// When (Claim the parent bounty)
		assert_ok!(Bounties::claim_bounty(RuntimeOrigin::signed(account_id(9)), 0));
		approve_bounty_payment(account_id(10), 0, 1, 6); // pay parent-bounty curator_stash
		approve_bounty_payment(account_id(9), 0, 1, 50 - 10 - 6); // pay parent-bounty beneficiary

		// Then
		// Check the total count after the parent bounty removal.
		assert_eq!(pallet_child_bounties::ParentTotalChildBounties::<Test>::get(0), 0);
		// Ensure parent-bounty curator received correctly reduced fee.
		assert_eq!(Balances::free_balance(account_id(4)), 101 + 6 - fee); // 101 + 6 - 2
		assert_eq!(Balances::reserved_balance(account_id(4)), 0);
		// Verify parent-bounty beneficiary balance.
		assert_eq!(Balances::free_balance(account_id(9)), 34);
		assert_eq!(Balances::reserved_balance(account_id(9)), 0);
	});
}

#[test]
fn accept_curator_handles_different_deposit_calculations() {
	// This test will verify that a bounty with and without a fee results
	// in a different curator deposit, and if the child curator matches the parent curator.
	new_test_ext().execute_with(|| {
		// Given (Make the parent bounty)
		let parent_curator = account_id(0);
		let parent_index = 0;
		let parent_value = 1_000_000;
		let parent_fee = 10_000;
		go_to_block(1);
		Balances::make_free_balance_be(&Treasury::account_id(), parent_value * 3);
		Balances::make_free_balance_be(&parent_curator, parent_fee * 100);
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(parent_curator),
			Box::new(1),
			parent_value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), parent_index));
		approve_bounty_payment(
			Bounties::bounty_account_id(parent_index, 1).expect("conversion failed"),
			parent_index,
			1,
			parent_value,
		);
		go_to_block(2);
		assert_ok!(Bounties::propose_curator(
			RuntimeOrigin::root(),
			parent_index,
			parent_curator,
			parent_fee
		));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(parent_curator),
			parent_index,
			account_id(10)
		));

		// When
		// Now we can start creating some child bounties.
		// Case 1: Parent and child curator are not the same.
		let child_index = 0;
		let child_curator = account_id(1);
		let child_value = 1_000;
		let child_fee = 100;
		let starting_balance = 100 * child_fee + child_value;
		Balances::make_free_balance_be(&child_curator, starting_balance);
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			parent_index,
			child_value,
			b"12345-p1".to_vec()
		));
		approve_child_bounty_payment(
			ChildBounties::child_bounty_account_id(parent_index, child_index),
			parent_index,
			child_index,
			1,
			child_value,
		);
		go_to_block(3);
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			parent_index,
			child_index,
			child_curator,
			child_fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator),
			parent_index,
			child_index,
			account_id(10),
		));

		// Then
		let expected_deposit = CuratorDepositMultiplier::get() * child_fee;
		assert_eq!(Balances::free_balance(child_curator), starting_balance - expected_deposit);
		assert_eq!(Balances::reserved_balance(child_curator), expected_deposit);

		// Given
		// Case 2: Parent and child curator are the same.
		let child_index = 1;
		let child_curator = parent_curator; // The same as parent bounty curator
		let child_value = 1_000;
		let child_fee = 10;
		let free_before = Balances::free_balance(&parent_curator);
		let reserved_before = Balances::reserved_balance(&parent_curator);

		// When
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			parent_index,
			child_value,
			b"12345-p1".to_vec()
		));
		approve_child_bounty_payment(
			ChildBounties::child_bounty_account_id(parent_index, child_index),
			parent_index,
			child_index,
			1,
			child_value,
		);
		go_to_block(4);
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			parent_index,
			child_index,
			child_curator,
			child_fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator),
			parent_index,
			child_index,
			account_id(10),
		));

		// Then
		// No expected deposit
		assert_eq!(Balances::free_balance(child_curator), free_before);
		assert_eq!(Balances::reserved_balance(child_curator), reserved_before);

		// Given
		// Case 3: Upper Limit
		let child_index = 2;
		let child_curator = account_id(2);
		let child_value = 10_000;
		let child_fee = 5_000;
		Balances::make_free_balance_be(&child_curator, starting_balance);

		// When
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			parent_index,
			child_value,
			b"12345-p1".to_vec()
		));
		go_to_block(5);
		approve_child_bounty_payment(
			ChildBounties::child_bounty_account_id(parent_index, child_index),
			parent_index,
			child_index,
			1,
			child_value,
		);
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			parent_index,
			child_index,
			child_curator,
			child_fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator),
			parent_index,
			child_index,
			account_id(10),
		));

		// Then
		let expected_deposit = CuratorDepositMax::get();
		assert_eq!(Balances::free_balance(child_curator), starting_balance - expected_deposit);
		assert_eq!(Balances::reserved_balance(child_curator), expected_deposit);
		// There is a max number of child bounties at a time.
		assert_ok!(ChildBounties::impl_close_child_bounty(parent_index, child_index));
		approve_child_bounty_payment(
			ChildBounties::child_bounty_account_id(parent_index, child_index),
			parent_index,
			child_index,
			1,
			child_value,
		);

		// Given
		// Case 4: Lower Limit
		let child_index = 3;
		let child_curator = account_id(3);
		let child_value = 10_000;
		let child_fee = 0;
		Balances::make_free_balance_be(&child_curator, starting_balance);

		// When
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			parent_index,
			child_value,
			b"12345-p1".to_vec()
		));
		approve_child_bounty_payment(
			ChildBounties::child_bounty_account_id(parent_index, child_index),
			parent_index,
			child_index,
			1,
			child_value,
		);
		go_to_block(5);
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			parent_index,
			child_index,
			child_curator,
			child_fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator),
			parent_index,
			child_index,
			account_id(10),
		));

		// Then
		let expected_deposit = CuratorDepositMin::get();
		assert_eq!(Balances::free_balance(child_curator), starting_balance - expected_deposit);
		assert_eq!(Balances::reserved_balance(child_curator), expected_deposit);
	});
}

#[test]
fn check_and_process_funding_and_payout_payment_works() {
	new_test_ext().execute_with(|| {
		// Given (Make the parent bounty)
		let parent_curator = account_id(0);
		let parent_index = 0;
		let child_index = 0;
		let parent_value = 1_000_000;
		let parent_fee = 10_000;
		let parent_curator_stash = account_id(10);
		let user = account_id(1);
		let child_value = 10_000;
		Balances::make_free_balance_be(&Treasury::account_id(), parent_value * 3);
		Balances::make_free_balance_be(&parent_curator, parent_fee * 100);
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(parent_curator),
			Box::new(1),
			parent_value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), parent_index));
		approve_bounty_payment(
			Bounties::bounty_account_id(parent_index, 1).expect("conversion failed"),
			parent_index,
			1,
			parent_value,
		);
		assert_ok!(Bounties::propose_curator(
			RuntimeOrigin::root(),
			parent_index,
			parent_curator,
			parent_fee
		));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(parent_curator),
			parent_index,
			parent_curator_stash
		));
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			parent_index,
			child_value,
			b"12345-p1".to_vec()
		));

		// When/Then (check ChildBountyStatus::Approved - PaymentState::Attempted -
		// PaymentStatus::InProgress)
		let payment_id = get_child_bounty_payment_id(parent_index, child_index, None)
			.expect("no payment attempt");
		set_status(payment_id, PaymentStatus::InProgress);
		assert_noop!(
			ChildBounties::check_payment_status(
				RuntimeOrigin::signed(user),
				parent_index,
				child_index
			),
			BountiesError::<Test>::FundingInconclusive
		);

		// When/Then (check ChildBountyStatus::Approved - PaymentState::Attempted -
		// PaymentStatus::Failure)
		set_status(payment_id, PaymentStatus::Failure);
		assert_eq!(
			ChildBounties::check_payment_status(
				RuntimeOrigin::signed(user),
				parent_index,
				child_index
			),
			Ok(PostDispatchInfo { actual_weight: None, pays_fee: Pays::No })
		);

		// When/Then (check BountyStatus::Approved - PaymentState::Failed)
		assert_noop!(
			ChildBounties::check_payment_status(
				RuntimeOrigin::signed(user),
				parent_index,
				child_index
			),
			BountiesError::<Test>::UnexpectedStatus
		);

		// When (process BountyStatus::Approved and check PaymentState::Success)
		assert_ok!(ChildBounties::process_payment(
			RuntimeOrigin::signed(user),
			parent_index,
			child_index
		));
		let payment_id = get_child_bounty_payment_id(parent_index, child_index, None)
			.expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(ChildBounties::check_payment_status(
			RuntimeOrigin::signed(user),
			parent_index,
			child_index
		));

		// Then
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(parent_index, child_index).unwrap(),
			ChildBounty {
				parent_bounty: 0,
				asset_kind: 1,
				value: child_value,
				fee: 0,
				curator_deposit: 0,
				status: ChildBountyStatus::Funded,
			}
		);

		// Given (claim child-bounty)
		let child_curator = account_id(4);
		let child_fee = 1;
		let child_curator_stash = account_id(7);
		let beneficiary = account_id(3);
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			parent_index,
			child_index,
			child_curator,
			child_fee
		));
		Balances::make_free_balance_be(&child_curator, 6);
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator),
			parent_index,
			child_index,
			child_curator_stash
		));
		assert_ok!(ChildBounties::award_child_bounty(
			RuntimeOrigin::signed(child_curator),
			parent_index,
			child_index,
			beneficiary
		));
		go_to_block(5);
		assert_ok!(ChildBounties::claim_child_bounty(
			RuntimeOrigin::signed(beneficiary),
			parent_index,
			child_index
		));

		// When (check ChildBountyStatus::PayoutAttempted - PaymentState::Attempted - 2x
		// PaymentStatus::InProgress)
		let beneficiary_payment_id =
			get_child_bounty_payment_id(parent_index, child_index, Some(beneficiary))
				.expect("no payment attempt");
		set_status(beneficiary_payment_id, PaymentStatus::InProgress);
		let curator_payment_id =
			get_child_bounty_payment_id(parent_index, child_index, Some(child_curator_stash))
				.expect("no payment attempt");
		set_status(curator_payment_id, PaymentStatus::InProgress);
		assert_noop!(
			ChildBounties::check_payment_status(
				RuntimeOrigin::signed(user),
				parent_index,
				child_index
			),
			BountiesError::<Test>::PayoutInconclusive
		);

		// When/Then (check ChildBountyStatus::PayoutAttempted - PaymentState::PayoutAttempted - 1x
		// PaymentStatus::Failure)
		set_status(beneficiary_payment_id, PaymentStatus::Failure);
		assert_eq!(
			ChildBounties::check_payment_status(
				RuntimeOrigin::signed(user),
				parent_index,
				child_index
			),
			Ok(PostDispatchInfo { actual_weight: None, pays_fee: Pays::Yes })
		);

		// When/Then (check ChildBountyStatus::PayoutAttempted - PaymentState::Failed)
		assert_noop!(
			ChildBounties::check_payment_status(
				RuntimeOrigin::signed(user),
				parent_index,
				child_index
			),
			BountiesError::<Test>::UnexpectedStatus
		);

		// When
		// TODO: continue
		// Tiago: process_payment does not change child_bounty.status state. Should it change?
	});
}

#[test]
fn check_and_process_refund_payment_works() {
	new_test_ext().execute_with(|| {
		// Given (Make the parent bounty)
		let parent_curator = account_id(0);
		let parent_index = 0;
		let child_index = 0;
		let parent_value = 1_000_000;
		let parent_fee = 10_000;
		let parent_curator_stash = account_id(10);
		let user = account_id(1);
		let child_value = 10_000;
		Balances::make_free_balance_be(&Treasury::account_id(), parent_value * 3);
		Balances::make_free_balance_be(&parent_curator, parent_fee * 100);
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(parent_curator),
			Box::new(1),
			parent_value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), parent_index));
		approve_bounty_payment(
			Bounties::bounty_account_id(parent_index, 1).expect("conversion failed"),
			parent_index,
			1,
			parent_value,
		);
		assert_ok!(Bounties::propose_curator(
			RuntimeOrigin::root(),
			parent_index,
			parent_curator,
			parent_fee
		));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(parent_curator),
			parent_index,
			parent_curator_stash
		));
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			parent_index,
			child_value,
			b"12345-p1".to_vec()
		));
		let payment_id = get_child_bounty_payment_id(parent_index, child_index, None)
			.expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(ChildBounties::check_payment_status(
			RuntimeOrigin::signed(user),
			parent_index,
			child_index
		));
		assert_ok!(ChildBounties::close_child_bounty(
			RuntimeOrigin::root(),
			parent_index,
			child_index
		));
		go_to_block(1);

		// When/Then (check ChildBountyStatus::RefundAttempted - PaymentState::Attempted -
		// PaymentStatus::InProgress)
		let payment_id = get_child_bounty_payment_id(parent_index, child_index, None)
			.expect("no payment attempt");
		set_status(payment_id, PaymentStatus::InProgress);
		assert_noop!(
			ChildBounties::check_payment_status(
				RuntimeOrigin::signed(user),
				parent_index,
				child_index
			),
			BountiesError::<Test>::RefundInconclusive
		);

		// When/Then (check ChildBountyStatus::RefundAttempted - PaymentState::Attempted -
		// PaymentStatus::Failure)
		set_status(payment_id, PaymentStatus::Failure);
		assert_eq!(
			ChildBounties::check_payment_status(
				RuntimeOrigin::signed(user),
				parent_index,
				child_index
			),
			Ok(PostDispatchInfo { actual_weight: None, pays_fee: Pays::Yes })
		);

		// When/Then (check ChildBountyStatus::RefundAttempted - PaymentState::Failed)
		assert_noop!(
			ChildBounties::check_payment_status(
				RuntimeOrigin::signed(user),
				parent_index,
				child_index
			),
			BountiesError::<Test>::UnexpectedStatus
		);

		// When (process ChildBountyStatus::RefundAttempted and check PaymentState::Success)
		assert_ok!(ChildBounties::process_payment(
			RuntimeOrigin::signed(user),
			parent_index,
			child_index
		));
		let payment_id = get_child_bounty_payment_id(parent_index, child_index, None)
			.expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(ChildBounties::check_payment_status(
			RuntimeOrigin::signed(user),
			parent_index,
			child_index
		));

		// Then
		assert_eq!(last_event(), ChildBountiesEvent::Canceled { index: parent_index, child_index });
		assert_eq!(
			Balances::free_balance(ChildBounties::child_bounty_account_id(
				parent_index,
				child_index
			)),
			0
		);
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(parent_index, child_index),
			None
		);
		assert_eq!(
			pallet_child_bounties::ChildBountyDescriptionsV1::<Test>::get(
				parent_index,
				child_index
			),
			None
		);
	});
}

#[test]
fn integrity_test() {
	new_test_ext().execute_with(|| {
		ChildBounties::integrity_test();
	});
}
