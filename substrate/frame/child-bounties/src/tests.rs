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
	traits::{Currency, Hooks},
};
use pallet_bounties::BountyOf;
use sp_runtime::traits::BadOrigin;

fn create_bounty() -> BountyOf<Test, ()> {
	let proposer = 0;
	let asset_kind = 1;
	let value = 50;
	let curator = 4;
	let fee = 8;
	let curator_stash = 10;
	let bounty_id = 0;

	assert_ok!(Bounties::propose_bounty(
		RuntimeOrigin::signed(account_id(proposer)),
		Box::new(asset_kind),
		value,
		b"12345".to_vec()
	));
	assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));
	let bounty_account =
		Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
	approve_bounty_payment(bounty_account, bounty_id, asset_kind, value);
	assert_ok!(Bounties::propose_curator(
		RuntimeOrigin::root(),
		bounty_id,
		account_id(curator),
		fee
	));
	Balances::make_free_balance_be(&account_id(curator), 101);
	assert_ok!(Bounties::accept_curator(
		RuntimeOrigin::signed(account_id(curator)),
		bounty_id,
		account_id(curator_stash)
	));
	let expected_deposit = Bounties::calculate_curator_deposit(&fee, asset_kind).unwrap();
	assert_eq!(Balances::reserved_balance(&account_id(curator)), expected_deposit);
	assert_eq!(Balances::free_balance(&account_id(curator)), 101 - expected_deposit);

	let bounty = pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap();
	bounty
}

#[test]
fn add_child_bounty() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let bounty = create_bounty();
		let asset_kind = bounty.asset_kind;
		let value = 10;
		let curator = 4;
		let bounty_id = 0;
		let child_bounty_id = 0;

		// When/Then
		assert_noop!(
			ChildBounties::add_child_bounty(
				RuntimeOrigin::signed(bounty.proposer),
				bounty_id,
				value,
				b"12345-p1".to_vec()
			),
			BountiesError::<Test>::RequireCurator,
		);

		// When/Then
		assert_noop!(
			ChildBounties::add_child_bounty(
				RuntimeOrigin::signed(account_id(curator)),
				bounty_id,
				0,
				b"12345-p1".to_vec()
			),
			BountiesError::<Test>::InvalidValue,
		);

		// When/Then
		assert_noop!(
			ChildBounties::add_child_bounty(
				RuntimeOrigin::signed(account_id(curator)),
				bounty_id,
				51,
				b"12345-p1".to_vec()
			),
			Error::<Test>::InsufficientBountyBalance,
		);

		// When
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(account_id(curator)),
			bounty_id,
			value,
			b"12345-p1".to_vec()
		));

		// Then
		let payment_id = get_child_bounty_payment_id(bounty_id, child_bounty_id, None)
			.expect("no payment attempt");
		expect_events(vec![
			ChildBountiesEvent::Paid { index: bounty_id, child_index: child_bounty_id, payment_id },
			ChildBountiesEvent::Added { index: bounty_id, child_index: child_bounty_id },
		]);
		assert_eq!(Balances::reserved_balance(account_id(curator)), bounty.curator_deposit);
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee: 0,
				curator_deposit: 0,
				status: ChildBountyStatus::Approved {
					payment_status: PaymentState::Attempted { id: payment_id }
				}
			}
		);
		assert_eq!(pallet_child_bounties::ParentTotalChildBounties::<Test>::get(bounty_id), 1);
		assert_eq!(
			pallet_child_bounties::ChildBountyDescriptionsV1::<Test>::get(
				bounty_id,
				child_bounty_id
			)
			.unwrap(),
			b"12345-p1".to_vec(),
		);
		assert_eq!(pallet_child_bounties::ChildrenValue::<Test>::get(bounty_id), value);

		// When (PaymentState::Success)
		let child_bounty_account =
			ChildBounties::child_bounty_account_id(bounty_id, child_bounty_id);
		approve_child_bounty_payment(
			child_bounty_account,
			bounty_id,
			child_bounty_id,
			asset_kind,
			value,
		);

		// Then
		assert_eq!(
			last_event(),
			ChildBountiesEvent::BecameActive { index: bounty_id, child_index: child_bounty_id }
		);
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee: 0,
				curator_deposit: 0,
				status: ChildBountyStatus::Funded
			}
		);
		assert_eq!(pallet_child_bounties::ParentChildBounties::<Test>::get(bounty_id), 1);
		assert_eq!(pallet_child_bounties::ParentTotalChildBounties::<Test>::get(bounty_id), 1);
		assert_eq!(pallet_child_bounties::ChildBounties::<Test>::iter().count(), 1);
		assert_eq!(pallet_child_bounties::ChildBountyDescriptionsV1::<Test>::iter().count(), 1);

		// Given
		MaxActiveChildBountyCount::set(1);

		// When/Then
		assert_noop!(
			ChildBounties::add_child_bounty(
				RuntimeOrigin::signed(account_id(curator)),
				bounty_id,
				value,
				b"12345-p1".to_vec()
			),
			Error::<Test>::TooManyChildBounties,
		);
	});
}

#[test]
fn child_bounty_assign_curator() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let bounty = create_bounty();
		let asset_kind = bounty.asset_kind;
		let value = 10;
		let parent_curator = account_id(4);
		let child_curator = account_id(8);
		let child_curator_stash = account_id(10);
		let fee = 6u64;
		let bounty_id = 0;
		let child_bounty_id = 0;

		Balances::make_free_balance_be(&child_curator, 101);
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			value,
			b"12345-p1".to_vec()
		));
		let child_bounties_account =
			ChildBounties::child_bounty_account_id(bounty_id, child_bounty_id);
		approve_child_bounty_payment(
			child_bounties_account,
			bounty_id,
			child_bounty_id,
			asset_kind,
			value,
		);

		// When/Then
		assert_noop!(
			ChildBounties::propose_curator(
				RuntimeOrigin::signed(child_curator),
				bounty_id,
				child_bounty_id,
				child_curator,
				fee
			),
			BountiesError::<Test>::RequireCurator
		);

		// When/Then
		assert_noop!(
			ChildBounties::propose_curator(
				RuntimeOrigin::signed(parent_curator),
				1,
				child_bounty_id,
				child_curator,
				fee
			),
			BountiesError::<Test>::InvalidIndex
		);

		// When/Then
		assert_noop!(
			ChildBounties::propose_curator(
				RuntimeOrigin::signed(parent_curator),
				bounty_id,
				child_bounty_id,
				child_curator,
				11
			),
			BountiesError::<Test>::InvalidFee
		);

		// When
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_bounty_id,
			child_curator,
			fee
		));

		// Then
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
				curator_deposit: 0,
				status: ChildBountyStatus::CuratorProposed { curator: child_curator },
			}
		);
		assert_eq!(pallet_child_bounties::ChildrenCuratorFees::<Test>::get(bounty_id), fee);

		// When/Then
		assert_noop!(
			ChildBounties::propose_curator(
				RuntimeOrigin::signed(parent_curator),
				bounty_id,
				child_bounty_id,
				child_curator,
				fee
			),
			BountiesError::<Test>::UnexpectedStatus,
		);

		// When/Then
		assert_noop!(
			ChildBounties::accept_curator(
				RuntimeOrigin::signed(child_curator),
				bounty_id,
				1,
				child_curator_stash
			),
			BountiesError::<Test>::InvalidIndex,
		);

		// When/Then
		assert_noop!(
			ChildBounties::accept_curator(
				RuntimeOrigin::signed(account_id(0)),
				bounty_id,
				child_bounty_id,
				child_curator_stash
			),
			BountiesError::<Test>::RequireCurator,
		);

		// When
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator),
			bounty_id,
			child_bounty_id,
			child_curator_stash
		));

		// Then
		let expected_child_deposit = CuratorDepositMultiplier::get() * fee;
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
				curator_deposit: expected_child_deposit,
				status: ChildBountyStatus::Active {
					curator: child_curator,
					curator_stash: child_curator_stash,
					update_due: 11
				},
			}
		);
		assert_eq!(pallet_child_bounties::ChildrenCuratorFees::<Test>::get(bounty_id), fee);
		assert_eq!(Balances::free_balance(child_curator), 101 - expected_child_deposit);
		assert_eq!(Balances::reserved_balance(child_curator), expected_child_deposit);

		// When/Then
		assert_noop!(
			ChildBounties::accept_curator(
				RuntimeOrigin::signed(child_curator),
				bounty_id,
				child_bounty_id,
				child_curator_stash
			),
			BountiesError::<Test>::UnexpectedStatus,
		);
	});
}

#[test]
fn award_claim_child_bounty() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let bounty = create_bounty();
		let asset_kind = bounty.asset_kind;
		let value = 10;
		let parent_curator = account_id(4);
		let child_curator = account_id(8);
		let child_curator_stash = account_id(10);
		let fee = 8u64;
		let beneficiary = account_id(7);
		let bounty_id = 0;
		let child_bounty_id = 0;

		Balances::make_free_balance_be(&child_curator, 101);
		go_to_block(2);
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			value,
			b"12345-p1".to_vec()
		));
		let child_bounties_account =
			ChildBounties::child_bounty_account_id(bounty_id, child_bounty_id);
		approve_child_bounty_payment(
			child_bounties_account,
			bounty_id,
			child_bounty_id,
			asset_kind,
			value,
		);
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_bounty_id,
			child_curator,
			fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator),
			bounty_id,
			child_bounty_id,
			child_curator_stash
		));

		// When/Then
		assert_noop!(
			ChildBounties::award_child_bounty(
				RuntimeOrigin::signed(child_curator),
				bounty_id,
				1,
				beneficiary
			),
			BountiesError::<Test>::InvalidIndex,
		);

		// When/Then
		assert_noop!(
			ChildBounties::award_child_bounty(
				RuntimeOrigin::signed(account_id(3)),
				bounty_id,
				child_bounty_id,
				beneficiary
			),
			BountiesError::<Test>::RequireCurator,
		);

		// When
		assert_ok!(ChildBounties::award_child_bounty(
			RuntimeOrigin::signed(child_curator),
			bounty_id,
			child_bounty_id,
			beneficiary
		));

		// Then
		let expected_deposit = CuratorDepositMultiplier::get() * fee;
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
				curator_deposit: expected_deposit,
				status: ChildBountyStatus::PendingPayout {
					curator: child_curator,
					beneficiary,
					unlock_at: 5,
					curator_stash: child_curator_stash
				},
			}
		);

		// When/Then
		assert_noop!(
			ChildBounties::award_child_bounty(
				RuntimeOrigin::signed(child_curator),
				bounty_id,
				child_bounty_id,
				beneficiary
			),
			BountiesError::<Test>::UnexpectedStatus,
		);

		// When/Then
		assert_noop!(
			ChildBounties::claim_child_bounty(RuntimeOrigin::signed(beneficiary), bounty_id, 1),
			BountiesError::<Test>::InvalidIndex
		);

		// When/Then
		assert_noop!(
			ChildBounties::claim_child_bounty(
				RuntimeOrigin::signed(beneficiary),
				bounty_id,
				child_bounty_id
			),
			BountiesError::<Test>::Premature
		);

		// When
		go_to_block(9); // block_number >= unlock_at
		assert_ok!(ChildBounties::claim_child_bounty(
			RuntimeOrigin::signed(beneficiary),
			bounty_id,
			child_bounty_id
		));

		// Then (PaymentState::Attempted)
		let beneficiary_payment_id =
			get_child_bounty_payment_id(bounty_id, child_bounty_id, Some(beneficiary))
				.expect("no payment attempt");
		let curator_payment_id =
			get_child_bounty_payment_id(bounty_id, child_bounty_id, Some(child_curator_stash))
				.expect("no payment attempt");
		expect_events(vec![
			ChildBountiesEvent::Paid {
				index: bounty_id,
				child_index: child_bounty_id,
				payment_id: curator_payment_id,
			},
			ChildBountiesEvent::Paid {
				index: bounty_id,
				child_index: child_bounty_id,
				payment_id: beneficiary_payment_id,
			},
		]);
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
				curator_deposit: expected_deposit,
				status: ChildBountyStatus::PayoutAttempted {
					curator: child_curator,
					beneficiary: (
						beneficiary,
						PaymentState::Attempted { id: beneficiary_payment_id }
					),
					curator_stash: (
						child_curator_stash,
						PaymentState::Attempted { id: curator_payment_id }
					)
				},
			}
		);

		// When (1x PaymentState::Success)
		approve_child_bounty_payment(
			beneficiary,
			bounty_id,
			child_bounty_id,
			asset_kind,
			value - fee,
		); // pay 10 - 8 child-bounty beneficiary

		// Then
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
				curator_deposit: expected_deposit,
				status: ChildBountyStatus::PayoutAttempted {
					curator: child_curator,
					beneficiary: (beneficiary, PaymentState::Succeeded),
					curator_stash: (
						child_curator_stash,
						PaymentState::Attempted { id: curator_payment_id }
					)
				},
			}
		);
		assert!(
			get_child_bounty_payment_id(bounty_id, child_bounty_id, Some(beneficiary)).is_none()
		);

		// When (2x PaymentState::Success)
		approve_child_bounty_payment(
			child_curator_stash,
			bounty_id,
			child_bounty_id,
			asset_kind,
			fee,
		); // pay 2 child-bounty curator_stash

		// Then
		assert_eq!(
			last_event(),
			ChildBountiesEvent::PayoutProcessed {
				index: bounty_id,
				child_index: child_bounty_id,
				asset_kind,
				value: value - fee,
				beneficiary
			}
		);
		assert_eq!(pallet_child_bounties::ChildrenCuratorFees::<Test>::get(bounty_id), fee);
		assert_eq!(pallet_child_bounties::ChildrenValue::<Test>::get(bounty_id), value);
		// Ensure child-bounty curator is paid deposit refund.
		assert_eq!(Balances::free_balance(child_curator), 101);
		assert_eq!(Balances::reserved_balance(child_curator), 0);
		// Check the child-bounty count.
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id),
			None
		);
		assert_eq!(pallet_child_bounties::ParentChildBounties::<Test>::get(bounty_id), 0);
		assert_eq!(pallet_child_bounties::ParentTotalChildBounties::<Test>::get(bounty_id), 1);
		assert_eq!(pallet_child_bounties::ChildBounties::<Test>::iter().count(), 0);
		assert_eq!(pallet_child_bounties::ChildBountyDescriptionsV1::<Test>::iter().count(), 0);

		// When/Then
		assert_noop!(
			ChildBounties::close_child_bounty(
				RuntimeOrigin::signed(parent_curator),
				bounty_id,
				child_bounty_id
			),
			BountiesError::<Test>::InvalidIndex,
		);
	});
}

#[test]
fn close_child_bounty_added() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let bounty = create_bounty();
		let asset_kind = bounty.asset_kind;
		let value = 10;
		let parent_curator = account_id(4);
		let child_curator = account_id(8);
		let bounty_id = 0;
		let child_bounty_id = 0;
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		Balances::make_free_balance_be(&child_curator, 101);
		go_to_block(2);
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			value,
			b"12345-p1".to_vec()
		));
		unpay(bounty_account, asset_kind, value);

		// When/Then
		go_to_block(4);
		assert_noop!(
			ChildBounties::close_child_bounty(
				RuntimeOrigin::signed(parent_curator),
				bounty_id,
				child_bounty_id
			),
			BountiesError::<Test>::UnexpectedStatus,
		);

		// When/Then
		let child_bounties_account =
			ChildBounties::child_bounty_account_id(bounty_id, child_bounty_id);
		approve_child_bounty_payment(
			child_bounties_account,
			bounty_id,
			child_bounty_id,
			asset_kind,
			value,
		);

		// When/Then
		assert_noop!(
			ChildBounties::close_child_bounty(
				RuntimeOrigin::signed(account_id(7)),
				bounty_id,
				child_bounty_id
			),
			BadOrigin
		);

		// When/Then
		assert_noop!(
			ChildBounties::close_child_bounty(
				RuntimeOrigin::signed(child_curator),
				bounty_id,
				child_bounty_id
			),
			BadOrigin
		);

		// When
		assert_ok!(ChildBounties::close_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_bounty_id
		));

		// Then (PaymentState::Attempted)
		let payment_id = get_child_bounty_payment_id(bounty_id, child_bounty_id, None)
			.expect("no payment attempt");
		expect_events(vec![
			ChildBountiesEvent::Paid { index: bounty_id, child_index: child_bounty_id, payment_id },
			ChildBountiesEvent::Canceled { index: bounty_id, child_index: child_bounty_id },
		]);
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee: 0,
				curator_deposit: 0,
				status: ChildBountyStatus::RefundAttempted {
					curator: None,
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);

		// When/Then
		assert_noop!(
			ChildBounties::close_child_bounty(
				RuntimeOrigin::signed(parent_curator),
				bounty_id,
				child_bounty_id
			),
			BountiesError::<Test>::UnexpectedStatus,
		);

		// When
		approve_child_bounty_payment(
			bounty_account,
			bounty_id,
			child_bounty_id,
			asset_kind,
			bounty.value,
		);

		// Then (PaymentState::Success)
		assert_eq!(
			last_event(),
			ChildBountiesEvent::RefundProcessed { index: bounty_id, child_index: child_bounty_id }
		);
		assert_eq!(pallet_child_bounties::ChildrenCuratorFees::<Test>::get(bounty_id), 0);
		assert_eq!(pallet_child_bounties::ChildrenValue::<Test>::get(bounty_id), 0);
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id),
			None
		);
		assert_eq!(pallet_child_bounties::ParentChildBounties::<Test>::get(bounty_id), 0);
		assert_eq!(pallet_child_bounties::ParentTotalChildBounties::<Test>::get(bounty_id), 1);
		assert_eq!(pallet_child_bounties::ChildBounties::<Test>::iter().count(), 0);
		assert_eq!(pallet_child_bounties::ChildBountyDescriptionsV1::<Test>::iter().count(), 0);
	});
}

#[test]
fn close_child_bounty_active() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let bounty = create_bounty();
		let asset_kind = bounty.asset_kind;
		let value = 10;
		let parent_curator = account_id(4);
		let child_curator = account_id(8);
		let child_curator_stash = account_id(10);
		let fee = 2u64;
		let bounty_id = 0;
		let child_bounty_id = 0;
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		Balances::make_free_balance_be(&child_curator, 101);
		go_to_block(2);
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			value,
			b"12345-p1".to_vec()
		));
		unpay(bounty_account, asset_kind, value);
		let child_bounty_account =
			ChildBounties::child_bounty_account_id(bounty_id, child_bounty_id);
		approve_child_bounty_payment(
			child_bounty_account,
			bounty_id,
			child_bounty_id,
			asset_kind,
			value,
		);
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_bounty_id,
			child_curator,
			fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator),
			bounty_id,
			child_bounty_id,
			child_curator_stash
		));
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id,).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
				curator_deposit: 3,
				status: ChildBountyStatus::Active {
					curator: child_curator,
					curator_stash: child_curator_stash,
					update_due: 11,
				},
			}
		);

		// When
		assert_ok!(ChildBounties::close_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_bounty_id
		));

		// Then (PaymentState::Attempted)
		let payment_id = get_child_bounty_payment_id(bounty_id, child_bounty_id, None)
			.expect("no payment attempt");
		expect_events(vec![
			ChildBountiesEvent::Paid { index: bounty_id, child_index: child_bounty_id, payment_id },
			ChildBountiesEvent::Canceled { index: bounty_id, child_index: child_bounty_id },
		]);
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
				curator_deposit: 3,
				status: ChildBountyStatus::RefundAttempted {
					curator: Some(child_curator),
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);
		// Ensure child-bounty curator balance is still reserved.
		assert_eq!(Balances::free_balance(child_curator), 101 - 3);
		assert_eq!(Balances::reserved_balance(child_curator), 3);

		// When/Then
		assert_noop!(
			ChildBounties::close_child_bounty(
				RuntimeOrigin::signed(parent_curator),
				bounty_id,
				child_bounty_id
			),
			BountiesError::<Test>::UnexpectedStatus,
		);

		// When
		approve_child_bounty_payment(
			bounty_account,
			bounty_id,
			child_bounty_id,
			asset_kind,
			bounty.value,
		);

		// Then (PaymentState::Success)
		assert_eq!(
			last_event(),
			ChildBountiesEvent::RefundProcessed { index: bounty_id, child_index: child_bounty_id }
		);
		assert_eq!(Balances::free_balance(child_curator), 101);
		assert_eq!(Balances::reserved_balance(child_curator), 0);
		assert_eq!(pallet_child_bounties::ChildrenCuratorFees::<Test>::get(bounty_id), 0);
		assert_eq!(pallet_child_bounties::ChildrenValue::<Test>::get(bounty_id), 0);
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id),
			None
		);
		assert_eq!(pallet_child_bounties::ParentChildBounties::<Test>::get(bounty_id), 0);
		assert_eq!(pallet_child_bounties::ParentTotalChildBounties::<Test>::get(bounty_id), 1);
		assert_eq!(pallet_child_bounties::ChildBounties::<Test>::iter().count(), 0);
		assert_eq!(pallet_child_bounties::ChildBountyDescriptionsV1::<Test>::iter().count(), 0);
	});
}

#[test]
fn close_child_bounty_pending() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let bounty = create_bounty();
		let asset_kind = bounty.asset_kind;
		let value = 10;
		let parent_curator = account_id(4);
		let child_curator = account_id(8);
		let child_curator_stash = account_id(10);
		let fee = 4u64;
		let beneficiary = account_id(7);
		let bounty_id = 0;
		let child_bounty_id = 0;
		Balances::make_free_balance_be(&child_curator, 101);

		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			value,
			b"12345-p1".to_vec()
		));
		let child_bounties_account =
			ChildBounties::child_bounty_account_id(bounty_id, child_bounty_id);
		approve_child_bounty_payment(
			child_bounties_account,
			bounty_id,
			child_bounty_id,
			asset_kind,
			value,
		);
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_bounty_id,
			child_curator,
			fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator),
			bounty_id,
			child_bounty_id,
			child_curator_stash
		));
		assert_ok!(ChildBounties::award_child_bounty(
			RuntimeOrigin::signed(child_curator),
			bounty_id,
			child_bounty_id,
			beneficiary
		));

		// When/Then
		assert_noop!(
			ChildBounties::close_child_bounty(
				RuntimeOrigin::signed(parent_curator),
				bounty_id,
				child_bounty_id
			),
			BountiesError::<Test>::PendingPayout
		);

		// Then
		let expected_child_deposit = CuratorDepositMin::get();
		// Check the child-bounty count.
		assert_eq!(pallet_child_bounties::ParentChildBounties::<Test>::get(bounty_id), 1);
		// Ensure no changes in child-bounty curator balance.
		assert_eq!(Balances::reserved_balance(child_curator), expected_child_deposit);
		assert_eq!(Balances::free_balance(child_curator), 101 - expected_child_deposit);
		// Child-bounty account status.
		assert_eq!(pallet_child_bounties::ChildrenCuratorFees::<Test>::get(bounty_id), fee);
		assert_eq!(pallet_child_bounties::ChildrenValue::<Test>::get(bounty_id), value);
	});
}

#[test]
fn child_bounty_added_unassign_curator() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let bounty = create_bounty();
		let asset_kind = bounty.asset_kind;
		let value = 50;
		let parent_curator = account_id(4);
		let bounty_id = 0;
		let child_bounty_id = 0;

		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			value,
			b"12345-p1".to_vec()
		));
		let child_bounties_account =
			ChildBounties::child_bounty_account_id(bounty_id, child_bounty_id);
		approve_child_bounty_payment(
			child_bounties_account,
			bounty_id,
			child_bounty_id,
			asset_kind,
			value,
		);

		// When/Then (Unassign curator in added state)
		assert_noop!(
			ChildBounties::unassign_curator(
				RuntimeOrigin::signed(parent_curator),
				bounty_id,
				child_bounty_id
			),
			BountiesError::<Test>::UnexpectedStatus
		);
	});
}

#[test]
fn child_bounty_curator_proposed_unassign_curator() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let bounty = create_bounty();
		let asset_kind = bounty.asset_kind;
		let value = 10;
		let parent_curator = account_id(4);
		let child_curator = account_id(8);
		let fee = 2u64;
		let bounty_id = 0;
		let child_bounty_id = 0;

		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			value,
			b"12345-p1".to_vec()
		));
		let child_bounties_account =
			ChildBounties::child_bounty_account_id(bounty_id, child_bounty_id);
		approve_child_bounty_payment(
			child_bounties_account,
			bounty_id,
			child_bounty_id,
			asset_kind,
			value,
		);
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_bounty_id,
			child_curator,
			fee
		));

		// When/Then
		assert_noop!(
			ChildBounties::unassign_curator(
				RuntimeOrigin::signed(account_id(99)),
				bounty_id,
				child_bounty_id
			),
			BadOrigin
		);

		// When (Unassign curator)
		assert_ok!(ChildBounties::unassign_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_bounty_id
		));

		// Then
		// Verify updated child-bounty status.
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
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
	// Should slash.
	// Step 4: Assign, accept another curator for child bounty. Unassign from
	// child-bounty curator. Should NOT slash.
	// Step 5: Assign, accept another curator for child bounty. Unassign from random account. Should
	// slash.
	ExtBuilder::default().build_and_execute(|| {
		// Given
		go_to_block(2);
		let bounty = create_bounty();
		let asset_kind = bounty.asset_kind;
		let value = 10;
		let parent_curator = account_id(4);
		let child_curator_1 = account_id(6);
		let child_curator_2 = account_id(7);
		let child_curator_3 = account_id(8);
		let child_curator_stash = account_id(10);
		let fee = 6u64;
		let bounty_id = 0;
		let child_bounty_id = 0;
		Balances::make_free_balance_be(&child_curator_1, 101); // Child-bounty curator 1.
		Balances::make_free_balance_be(&child_curator_2, 101); // Child-bounty curator 2.
		Balances::make_free_balance_be(&child_curator_3, 101); // Child-bounty curator 3.

		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			value,
			b"12345-p1".to_vec()
		));
		let child_bounties_account =
			ChildBounties::child_bounty_account_id(bounty_id, child_bounty_id);
		approve_child_bounty_payment(
			child_bounties_account,
			bounty_id,
			child_bounty_id,
			asset_kind,
			value,
		);
		go_to_block(3);
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_bounty_id,
			child_curator_3,
			fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator_3),
			bounty_id,
			child_bounty_id,
			child_curator_stash
		));
		let expected_child_deposit = CuratorDepositMultiplier::get() * fee;
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
				curator_deposit: expected_child_deposit,
				status: ChildBountyStatus::Active {
					curator: child_curator_3,
					curator_stash: child_curator_stash,
					update_due: 12
				},
			}
		);

		// When
		go_to_block(4);
		assert_ok!(ChildBounties::unassign_curator(
			RuntimeOrigin::root(),
			bounty_id,
			child_bounty_id
		));

		// Then
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
				curator_deposit: 0,
				status: ChildBountyStatus::Funded,
			}
		);
		// Ensure child-bounty curator was slashed.
		assert_eq!(Balances::free_balance(child_curator_3), 101 - expected_child_deposit);
		assert_eq!(Balances::reserved_balance(child_curator_3), 0); // slashed

		// Given (Propose and accept curator for child-bounty again)
		let fee = 2;
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_bounty_id,
			child_curator_2,
			fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator_2),
			bounty_id,
			child_bounty_id,
			child_curator_stash
		));
		let expected_child_deposit = CuratorDepositMin::get();
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
				curator_deposit: expected_child_deposit,
				status: ChildBountyStatus::Active {
					curator: child_curator_2,
					curator_stash: child_curator_stash,
					update_due: 12
				},
			}
		);
		go_to_block(5);

		// When (Unassign curator again - from parent curator)
		assert_ok!(ChildBounties::unassign_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_bounty_id
		));

		// Then
		// Verify updated child-bounty status.
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
				curator_deposit: 0,
				status: ChildBountyStatus::Funded,
			}
		);
		// Ensure child-bounty curator was slashed.
		assert_eq!(Balances::free_balance(child_curator_2), 101 - expected_child_deposit);
		assert_eq!(Balances::reserved_balance(child_curator_2), 0); // slashed

		// Given (Propose and accept curator for child-bounty again)
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_bounty_id,
			child_curator_1,
			fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator_1),
			bounty_id,
			child_bounty_id,
			child_curator_stash
		));
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
				curator_deposit: expected_child_deposit,
				status: ChildBountyStatus::Active {
					curator: child_curator_1,
					curator_stash: child_curator_stash,
					update_due: 12
				},
			}
		);

		// When (Unassign curator again - from child-bounty curator)
		go_to_block(6);
		assert_ok!(ChildBounties::unassign_curator(
			RuntimeOrigin::signed(child_curator_1),
			bounty_id,
			child_bounty_id
		));

		// Then
		// Verify updated child-bounty status.
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
				curator_deposit: 0,
				status: ChildBountyStatus::Funded,
			}
		);
		// Ensure child-bounty curator was **not** slashed.
		assert_eq!(Balances::free_balance(child_curator_1), 101); // not slashed
		assert_eq!(Balances::reserved_balance(child_curator_1), 0);

		// Given (Propose and accept curator for child-bounty one last time)
		let fee = 2;
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_bounty_id,
			child_curator_1,
			fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator_1),
			bounty_id,
			child_bounty_id,
			child_curator_stash
		));
		let expected_child_deposit = CuratorDepositMin::get();
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
				curator_deposit: expected_child_deposit,
				status: ChildBountyStatus::Active {
					curator: child_curator_1,
					curator_stash: child_curator_stash,
					update_due: 12
				},
			}
		);

		// When/ Then (Unassign curator again - from non curator; non reject origin; some random
		// guy) Bounty update period is not yet complete.
		go_to_block(7);
		assert_noop!(
			ChildBounties::unassign_curator(
				RuntimeOrigin::signed(account_id(3)),
				bounty_id,
				child_bounty_id
			),
			BountiesError::<Test>::Premature
		);

		// When (Unassign child curator from random account after inactivity)
		go_to_block(20);
		assert_ok!(ChildBounties::unassign_curator(
			RuntimeOrigin::signed(account_id(3)),
			bounty_id,
			child_bounty_id
		));

		// Then
		// Verify updated child-bounty status.
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
				curator_deposit: 0,
				status: ChildBountyStatus::Funded,
			}
		);
		// Ensure child-bounty curator was slashed.
		assert_eq!(Balances::free_balance(child_curator_1), 101 - expected_child_deposit); // slashed
		assert_eq!(Balances::reserved_balance(child_curator_1), 0);
	});
}

#[test]
fn parent_bounty_inactive_unassign_curator_child_bounty() {
	// Unassign curator when parent bounty in not in active state.
	// This can happen when the curator of parent bounty has been unassigned.
	ExtBuilder::default().build_and_execute(|| {
		// Given
		go_to_block(2);
		let bounty = create_bounty();
		let asset_kind = bounty.asset_kind;
		let value = 10;
		let parent_curator = account_id(4); // free balance 101
		let parent_curator_2 = account_id(5);
		let child_curator_1 = account_id(6);
		let child_curator_2 = account_id(7);
		let child_curator_3 = account_id(8);
		let child_curator_stash = account_id(10);
		let fee = 8u64;
		let bounty_id = 0;
		let child_bounty_id = 0;
		Balances::make_free_balance_be(&parent_curator_2, 101); // Parent-bounty curator 2.
		Balances::make_free_balance_be(&child_curator_1, 101); // Child-bounty curator 1.
		Balances::make_free_balance_be(&child_curator_2, 101); // Child-bounty curator 2.
		Balances::make_free_balance_be(&child_curator_3, 101); // Child-bounty curator 3.

		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			value,
			b"12345-p1".to_vec()
		));
		let child_bounties_account =
			ChildBounties::child_bounty_account_id(bounty_id, child_bounty_id);
		approve_child_bounty_payment(
			child_bounties_account,
			bounty_id,
			child_bounty_id,
			asset_kind,
			value,
		);
		go_to_block(3);
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_bounty_id,
			child_curator_3,
			fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator_3),
			bounty_id,
			child_bounty_id,
			child_curator_stash
		));
		let expected_child_deposit = CuratorDepositMultiplier::get() * fee;
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
				curator_deposit: expected_child_deposit,
				status: ChildBountyStatus::Active {
					curator: child_curator_3,
					curator_stash: child_curator_stash,
					update_due: 12
				},
			}
		);

		// When/Then (Unassign parent bounty curator)
		go_to_block(4);
		assert_ok!(Bounties::unassign_curator(RuntimeOrigin::root(), bounty_id));

		// When/ Then (Try unassign child-bounty curator - from non curator; non reject
		// origin; some random guy. Bounty update period is not yet complete)
		go_to_block(5);
		assert_noop!(
			ChildBounties::unassign_curator(
				RuntimeOrigin::signed(account_id(3)),
				bounty_id,
				child_bounty_id
			),
			Error::<Test>::ParentBountyNotActive
		);

		// When (Unassign curator - from reject origin)
		assert_ok!(ChildBounties::unassign_curator(
			RuntimeOrigin::root(),
			bounty_id,
			child_bounty_id
		));

		// Then
		// Verify updated child-bounty status.
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
				curator_deposit: 0,
				status: ChildBountyStatus::Funded,
			}
		);
		// Ensure child-bounty curator was slashed.
		assert_eq!(Balances::free_balance(child_curator_3), 101 - expected_child_deposit);
		assert_eq!(Balances::reserved_balance(child_curator_3), 0); // slashed

		// Given
		// Propose and accept curator for parent-bounty again.
		go_to_block(6);
		let fee = 6u64;
		assert_ok!(Bounties::propose_curator(
			RuntimeOrigin::root(),
			bounty_id,
			parent_curator_2,
			fee
		));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(parent_curator_2),
			bounty_id,
			child_curator_stash
		));
		go_to_block(7);
		// Propose and accept curator for child-bounty again.
		let fee = 2;
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator_2),
			bounty_id,
			child_bounty_id,
			child_curator_2,
			fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator_2),
			bounty_id,
			child_bounty_id,
			child_curator_stash
		));
		let expected_deposit = CuratorDepositMin::get();
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
				curator_deposit: expected_deposit,
				status: ChildBountyStatus::Active {
					curator: child_curator_2,
					curator_stash: child_curator_stash,
					update_due: 16
				},
			}
		);

		// When/Then
		go_to_block(8);
		assert_noop!(
			ChildBounties::unassign_curator(
				RuntimeOrigin::signed(account_id(3)),
				bounty_id,
				child_bounty_id
			),
			BountiesError::<Test>::Premature
		);

		// When/Then (Unassign parent bounty curator again)
		assert_ok!(Bounties::unassign_curator(RuntimeOrigin::signed(parent_curator_2), bounty_id));

		// When (Unassign curator again - from parent curator)
		go_to_block(9);
		assert_ok!(ChildBounties::unassign_curator(
			RuntimeOrigin::signed(child_curator_2),
			bounty_id,
			child_bounty_id
		));

		// Then
		// Verify updated child-bounty status.
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value,
				fee,
				curator_deposit: 0,
				status: ChildBountyStatus::Funded,
			}
		);
		// Ensure child-bounty curator was not slashed.
		assert_eq!(Balances::free_balance(child_curator_2), 101);
		assert_eq!(Balances::reserved_balance(child_curator_2), 0); // slashed
	});
}

#[test]
fn close_parent_with_child_bounty() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let parent_proposer = 0;
		let asset_kind = 1;
		let parent_value = 50;
		let parent_fee = 8;
		let child_value = 10;
		let parent_curator = account_id(4);
		let child_curator = account_id(8);
		let curator_stash = account_id(10);
		let bounty_id = 0;
		let child_bounty_id = 0;
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		let child_bounty_account =
			ChildBounties::child_bounty_account_id(bounty_id, child_bounty_id);
		let treasury_account = Bounties::account_id();
		Balances::make_free_balance_be(&parent_curator, 101);
		Balances::make_free_balance_be(&child_curator, 101);

		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(account_id(parent_proposer)),
			Box::new(asset_kind),
			parent_value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));
		approve_bounty_payment(bounty_account, bounty_id, asset_kind, parent_value);

		// When/Then (Try add child-bounty)
		// Should fail, parent bounty not active yet.
		assert_noop!(
			ChildBounties::add_child_bounty(
				RuntimeOrigin::signed(parent_curator),
				bounty_id,
				child_value,
				b"12345-p1".to_vec()
			),
			Error::<Test>::ParentBountyNotActive
		);

		// Given
		go_to_block(2);
		assert_ok!(Bounties::propose_curator(
			RuntimeOrigin::root(),
			bounty_id,
			parent_curator,
			parent_fee
		));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			curator_stash
		));

		// When
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_value,
			b"12345-p1".to_vec()
		));
		unpay(bounty_account, asset_kind, child_value);
		approve_child_bounty_payment(
			child_bounty_account,
			bounty_id,
			child_bounty_id,
			asset_kind,
			child_value,
		);

		// When/Then (Try close parent-bounty)
		// Child bounty active, can't close parent.
		go_to_block(4);
		assert_noop!(
			Bounties::close_bounty(RuntimeOrigin::root(), bounty_id),
			BountiesError::<Test>::HasActiveChildBounty
		);

		// Given (Close child-bounty)
		assert_ok!(ChildBounties::close_child_bounty(
			RuntimeOrigin::root(),
			bounty_id,
			child_bounty_id
		));
		approve_child_bounty_payment(
			bounty_account,
			bounty_id,
			child_bounty_id,
			asset_kind,
			parent_value,
		);
		// Check the child-bounty count.
		assert_eq!(pallet_child_bounties::ParentChildBounties::<Test>::get(bounty_id), 0);
		assert_eq!(pallet_child_bounties::ParentTotalChildBounties::<Test>::get(bounty_id), 1);

		// When (Try close parent-bounty again)
		// Should pass this time.
		assert_ok!(Bounties::close_bounty(RuntimeOrigin::root(), bounty_id));
		approve_bounty_payment(treasury_account, bounty_id, asset_kind, parent_value);

		// Then
		// Check the total count is removed after the parent bounty removal.
		assert_eq!(pallet_child_bounties::ParentTotalChildBounties::<Test>::get(bounty_id), 0);
	});
}

#[test]
fn children_curator_fee_calculation_test() {
	// Tests the calculation of subtracting child-bounty curator fee
	// from parent bounty fee when claiming bounties.
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let bounty = create_bounty();
		let asset_kind = bounty.asset_kind;
		let parent_value = 50;
		let child_value = 10;
		let parent_curator = account_id(4);
		let child_curator = account_id(8);
		let curator_stash = account_id(10);
		let parent_fee = 8u64;
		let child_fee = 6u64;
		let parent_beneficiary = account_id(9);
		let child_beneficiary = account_id(7);
		let bounty_id = 0;
		let child_bounty_id = 0;

		Balances::make_free_balance_be(&child_curator, 101);
		go_to_block(2);
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_value,
			b"12345-p1".to_vec()
		));
		let child_bounties_account =
			ChildBounties::child_bounty_account_id(bounty_id, child_bounty_id);
		approve_child_bounty_payment(
			child_bounties_account,
			bounty_id,
			child_bounty_id,
			asset_kind,
			child_value,
		);
		go_to_block(4);
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_bounty_id,
			child_curator,
			child_fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator),
			bounty_id,
			child_bounty_id,
			curator_stash
		));
		assert_ok!(ChildBounties::award_child_bounty(
			RuntimeOrigin::signed(child_curator),
			bounty_id,
			child_bounty_id,
			child_beneficiary
		));
		let expected_child_deposit = CuratorDepositMultiplier::get() * child_fee;
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value: child_value,
				fee: child_fee,
				curator_deposit: expected_child_deposit,
				status: ChildBountyStatus::PendingPayout {
					curator: child_curator,
					beneficiary: child_beneficiary,
					unlock_at: 7,
					curator_stash,
				},
			}
		);

		// When
		go_to_block(9);
		assert_ok!(ChildBounties::claim_child_bounty(
			RuntimeOrigin::signed(child_beneficiary),
			bounty_id,
			child_bounty_id
		));
		approve_child_bounty_payment(
			curator_stash,
			bounty_id,
			child_bounty_id,
			asset_kind,
			child_fee,
		); // pay child-bounty curator_stash
		approve_child_bounty_payment(
			child_beneficiary,
			bounty_id,
			child_bounty_id,
			asset_kind,
			child_value - child_fee,
		); // pay child-bounty beneficiary

		// Then
		assert_eq!(pallet_child_bounties::ParentChildBounties::<Test>::get(bounty_id), 0);
		assert_eq!(pallet_child_bounties::ParentTotalChildBounties::<Test>::get(bounty_id), 1);
		assert_eq!(pallet_child_bounties::ChildrenValue::<Test>::get(bounty_id), child_value);
		assert_eq!(pallet_child_bounties::ChildrenCuratorFees::<Test>::get(bounty_id), child_fee);
		assert_eq!(
			last_event(),
			Event::PayoutProcessed {
				index: bounty_id,
				child_index: child_bounty_id,
				asset_kind,
				value: child_value - child_fee,
				beneficiary: child_beneficiary,
			}
		);

		// Given
		assert_ok!(Bounties::award_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			parent_beneficiary
		));
		go_to_block(15);

		// When (Claim the parent bounty)
		assert_ok!(Bounties::claim_bounty(RuntimeOrigin::signed(parent_beneficiary), bounty_id));
		approve_bounty_payment(curator_stash, bounty_id, asset_kind, parent_fee); // pay parent-bounty curator_stash
		approve_bounty_payment(
			parent_beneficiary,
			bounty_id,
			asset_kind,
			parent_value - child_value - (parent_fee - child_fee),
		); // pay parent-bounty beneficiary

		// Then
		// Check the total count after the parent bounty removal.
		assert_eq!(pallet_child_bounties::ParentTotalChildBounties::<Test>::get(bounty_id), 0);
		assert_eq!(Balances::free_balance(parent_curator), 101);
		assert_eq!(Balances::reserved_balance(parent_curator), 0);
		assert_eq!(Balances::free_balance(child_curator), 101);
		assert_eq!(Balances::reserved_balance(child_curator), 0);
		assert_eq!(pallet_child_bounties::ChildrenValue::<Test>::get(bounty_id), 0); // returns default value
		assert_eq!(pallet_child_bounties::ChildrenCuratorFees::<Test>::get(bounty_id), 0); // returns default value
	});
}

#[test]
fn accept_curator_handles_different_deposit_calculations() {
	// This test will verify that a bounty with and without a fee results
	// in a different curator deposit, and if the child curator matches the parent curator.
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let parent_curator = account_id(0);
		let asset_kind = 1;
		let parent_value = 1_000_000;
		let parent_fee = 10_000;
		let curator_stash = account_id(10);
		let bounty_id = 0;
		let child_bounty_id = 0;
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		go_to_block(1);
		Balances::make_free_balance_be(&parent_curator, 100 * parent_fee);
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(parent_curator),
			Box::new(asset_kind),
			parent_value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));
		approve_bounty_payment(bounty_account, bounty_id, asset_kind, parent_value);
		go_to_block(2);
		assert_ok!(Bounties::propose_curator(
			RuntimeOrigin::root(),
			bounty_id,
			parent_curator,
			parent_fee
		));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			curator_stash
		));

		// When
		// Case 1: Parent and child curator are not the same.
		let child_curator = account_id(1);
		let child_value = 1_000;
		let child_fee = 100;
		let starting_balance = 100 * child_fee;
		let child_bounty_account =
			ChildBounties::child_bounty_account_id(bounty_id, child_bounty_id);
		Balances::make_free_balance_be(&child_curator, starting_balance);
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_value,
			b"12345-p1".to_vec()
		));
		approve_child_bounty_payment(
			child_bounty_account,
			bounty_id,
			child_bounty_id,
			asset_kind,
			child_value,
		);
		go_to_block(3);
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_bounty_id,
			child_curator,
			child_fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator),
			bounty_id,
			child_bounty_id,
			curator_stash,
		));

		// Then
		let expected_deposit = CuratorDepositMultiplier::get() * child_fee;
		assert_eq!(Balances::free_balance(child_curator), starting_balance - expected_deposit);
		assert_eq!(Balances::reserved_balance(child_curator), expected_deposit);

		// Given
		// Case 2: Parent and child curator are the same.
		let child_bounty_id = 1;
		let child_curator = parent_curator; // The same as parent bounty curator
		let child_value = 1_000;
		let child_fee = 10;
		let child_bounty_account =
			ChildBounties::child_bounty_account_id(bounty_id, child_bounty_id);
		let free_before = Balances::free_balance(&parent_curator);
		let reserved_before = Balances::reserved_balance(&parent_curator);

		// When
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_value,
			b"12345-p1".to_vec()
		));
		approve_child_bounty_payment(
			child_bounty_account,
			bounty_id,
			child_bounty_id,
			asset_kind,
			child_value,
		);
		go_to_block(4);
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_bounty_id,
			child_curator,
			child_fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator),
			bounty_id,
			child_bounty_id,
			account_id(10),
		));

		// Then
		// No expected deposit
		assert_eq!(Balances::free_balance(child_curator), free_before);
		assert_eq!(Balances::reserved_balance(child_curator), reserved_before);

		// Given
		// Case 3: Upper Limit
		let child_bounty_id = 2;
		let child_curator = account_id(2);
		let child_value = 10_000;
		let child_fee = 5_000;
		let child_bounty_account =
			ChildBounties::child_bounty_account_id(bounty_id, child_bounty_id);
		Balances::make_free_balance_be(&child_curator, starting_balance);

		// When
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_value,
			b"12345-p1".to_vec()
		));
		go_to_block(5);
		approve_child_bounty_payment(
			child_bounty_account,
			bounty_id,
			child_bounty_id,
			asset_kind,
			child_value,
		);
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_bounty_id,
			child_curator,
			child_fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator),
			bounty_id,
			child_bounty_id,
			account_id(10),
		));

		// Then
		let expected_deposit = CuratorDepositMax::get();
		assert_eq!(Balances::free_balance(child_curator), starting_balance - expected_deposit);
		assert_eq!(Balances::reserved_balance(child_curator), expected_deposit);
		// There is a max number of child bounties at a time.
		assert_ok!(ChildBounties::impl_close_child_bounty(bounty_id, child_bounty_id));
		approve_child_bounty_payment(
			ChildBounties::child_bounty_account_id(bounty_id, child_bounty_id),
			bounty_id,
			child_bounty_id,
			asset_kind,
			child_value,
		);

		// Given
		// Case 4: Lower Limit
		let child_bounty_id = 3;
		let child_curator = account_id(3);
		let child_value = 10_000;
		let child_fee = 0;
		let child_bounty_account =
			ChildBounties::child_bounty_account_id(bounty_id, child_bounty_id);
		Balances::make_free_balance_be(&child_curator, starting_balance);

		// When
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_value,
			b"12345-p1".to_vec()
		));
		approve_child_bounty_payment(
			child_bounty_account,
			bounty_id,
			child_bounty_id,
			asset_kind,
			child_value,
		);
		go_to_block(5);
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_bounty_id,
			child_curator,
			child_fee
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator),
			bounty_id,
			child_bounty_id,
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
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let parent_curator = account_id(0);
		let bounty_id = 0;
		let asset_kind = 1;
		let child_bounty_id = 0;
		let parent_value = 1_000_000;
		let parent_fee = 10_000;
		let parent_curator_stash = account_id(10);
		let user = account_id(1);
		let child_value = 10_000;
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");
		Balances::make_free_balance_be(&parent_curator, parent_fee * 100);
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(parent_curator),
			Box::new(asset_kind),
			parent_value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));
		approve_bounty_payment(bounty_account, bounty_id, asset_kind, parent_value);
		assert_ok!(Bounties::propose_curator(
			RuntimeOrigin::root(),
			bounty_id,
			parent_curator,
			parent_fee
		));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			parent_curator_stash
		));
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_value,
			b"12345-p1".to_vec()
		));
		let payment_id = get_child_bounty_payment_id(bounty_id, child_bounty_id, None)
			.expect("no payment attempt");
		expect_events(vec![
			ChildBountiesEvent::Paid { index: bounty_id, child_index: child_bounty_id, payment_id },
			ChildBountiesEvent::Added { index: bounty_id, child_index: child_bounty_id },
		]);

		// When/Then
		assert_noop!(
			ChildBounties::check_payment_status(RuntimeOrigin::signed(user), bounty_id, 2),
			BountiesError::<Test>::InvalidIndex
		);

		// When/Then (check ChildBountyStatus::Approved - PaymentState::Attempted -
		// PaymentStatus::InProgress)
		let payment_id = get_child_bounty_payment_id(bounty_id, child_bounty_id, None)
			.expect("no payment attempt");
		set_status(payment_id, PaymentStatus::InProgress);
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value: child_value,
				fee: 0,
				curator_deposit: 0,
				status: ChildBountyStatus::Approved {
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);

		// When/Then
		assert_noop!(
			Bounties::close_bounty(RuntimeOrigin::root(), bounty_id),
			BountiesError::<Test>::HasActiveChildBounty
		);

		// When/Then
		assert_noop!(
			ChildBounties::check_payment_status(
				RuntimeOrigin::signed(user),
				bounty_id,
				child_bounty_id
			),
			BountiesError::<Test>::FundingInconclusive
		);

		// When/Then (check ChildBountyStatus::Approved - PaymentState::Attempted -
		// PaymentStatus::Failure)
		set_status(payment_id, PaymentStatus::Failure);
		let res = ChildBounties::check_payment_status(
			RuntimeOrigin::signed(user),
			bounty_id,
			child_bounty_id,
		);
		assert_eq!(res.unwrap().pays_fee, Pays::Yes);
		assert_eq!(
			last_event(),
			ChildBountiesEvent::PaymentFailed {
				index: bounty_id,
				child_index: child_bounty_id,
				payment_id
			}
		);
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value: child_value,
				fee: 0,
				curator_deposit: 0,
				status: ChildBountyStatus::Approved { payment_status: PaymentState::Failed },
			}
		);

		// When/Then (check BountyStatus::Approved - PaymentState::Failed)
		assert_noop!(
			ChildBounties::check_payment_status(
				RuntimeOrigin::signed(user),
				bounty_id,
				child_bounty_id
			),
			BountiesError::<Test>::UnexpectedStatus
		);

		// When/Then
		assert_noop!(
			ChildBounties::process_payment(RuntimeOrigin::signed(user), bounty_id, 2),
			BountiesError::<Test>::InvalidIndex
		);

		// When (process BountyStatus::Approved and check PaymentState::Success)
		assert_ok!(ChildBounties::process_payment(
			RuntimeOrigin::signed(user),
			bounty_id,
			child_bounty_id
		));

		// Then
		let payment_id = get_child_bounty_payment_id(bounty_id, child_bounty_id, None)
			.expect("no payment attempt");
		assert_eq!(
			last_event(),
			ChildBountiesEvent::Paid { index: bounty_id, child_index: child_bounty_id, payment_id }
		);
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value: child_value,
				fee: 0,
				curator_deposit: 0,
				status: ChildBountyStatus::Approved {
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);

		// When/Then
		assert_noop!(
			ChildBounties::process_payment(RuntimeOrigin::signed(user), bounty_id, child_bounty_id),
			BountiesError::<Test>::UnexpectedStatus
		);

		// When (check PaymentState::Success)
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(ChildBounties::check_payment_status(
			RuntimeOrigin::signed(user),
			bounty_id,
			child_bounty_id
		));

		// Then
		assert_eq!(
			last_event(),
			ChildBountiesEvent::BecameActive { index: bounty_id, child_index: child_bounty_id }
		);
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
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
			bounty_id,
			child_bounty_id,
			child_curator,
			child_fee
		));
		Balances::make_free_balance_be(&child_curator, 6);
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator),
			bounty_id,
			child_bounty_id,
			child_curator_stash
		));
		assert_ok!(ChildBounties::award_child_bounty(
			RuntimeOrigin::signed(child_curator),
			bounty_id,
			child_bounty_id,
			beneficiary
		));
		go_to_block(5);
		assert_ok!(ChildBounties::claim_child_bounty(
			RuntimeOrigin::signed(beneficiary),
			bounty_id,
			child_bounty_id
		));
		let beneficiary_payment_id =
			get_child_bounty_payment_id(bounty_id, child_bounty_id, Some(beneficiary))
				.expect("no payment attempt");
		set_status(beneficiary_payment_id, PaymentStatus::InProgress);
		let curator_payment_id =
			get_child_bounty_payment_id(bounty_id, child_bounty_id, Some(child_curator_stash))
				.expect("no payment attempt");
		set_status(curator_payment_id, PaymentStatus::InProgress);

		// When/Then (check ChildBountyStatus::PayoutAttempted - PaymentState::Attempted - 2x
		// PaymentStatus::InProgress)
		assert_noop!(
			ChildBounties::check_payment_status(
				RuntimeOrigin::signed(user),
				bounty_id,
				child_bounty_id
			),
			BountiesError::<Test>::PayoutInconclusive
		);

		// When (check ChildBountyStatus::PayoutAttempted - PaymentState::PayoutAttempted - 1x
		// PaymentStatus::Failure)
		set_status(beneficiary_payment_id, PaymentStatus::Failure);
		let res = ChildBounties::check_payment_status(
			RuntimeOrigin::signed(user),
			bounty_id,
			child_bounty_id,
		);

		// Then
		assert_eq!(res.unwrap().pays_fee, Pays::Yes);
		assert_eq!(
			last_event(),
			ChildBountiesEvent::PaymentFailed {
				index: bounty_id,
				child_index: child_bounty_id,
				payment_id: beneficiary_payment_id
			}
		);
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value: child_value,
				fee: 1,
				curator_deposit: 3,
				status: ChildBountyStatus::PayoutAttempted {
					curator: child_curator,
					curator_stash: (
						child_curator_stash,
						PaymentState::Attempted { id: curator_payment_id }
					),
					beneficiary: (beneficiary, PaymentState::Failed),
				},
			}
		);

		// When (check ChildBountyStatus::PayoutAttempted - PaymentState::PayoutAttempted - 2x
		// PaymentStatus::Failure)
		set_status(curator_payment_id, PaymentStatus::Failure);
		let res = ChildBounties::check_payment_status(
			RuntimeOrigin::signed(user),
			bounty_id,
			child_bounty_id,
		);

		// Then
		assert_eq!(res.unwrap().pays_fee, Pays::Yes);
		assert_eq!(
			last_event(),
			ChildBountiesEvent::PaymentFailed {
				index: bounty_id,
				child_index: child_bounty_id,
				payment_id: curator_payment_id
			}
		);
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value: child_value,
				fee: 1,
				curator_deposit: 3,
				status: ChildBountyStatus::PayoutAttempted {
					curator: child_curator,
					curator_stash: (child_curator_stash, PaymentState::Failed),
					beneficiary: (beneficiary, PaymentState::Failed),
				},
			}
		);

		// When/Then (check ChildBountyStatus::PayoutAttempted - 2x PaymentState::Failed)
		assert_noop!(
			ChildBounties::check_payment_status(
				RuntimeOrigin::signed(user),
				bounty_id,
				child_bounty_id
			),
			BountiesError::<Test>::PayoutInconclusive
		);

		// When
		assert_ok!(ChildBounties::process_payment(
			RuntimeOrigin::signed(user),
			bounty_id,
			child_bounty_id
		));

		// Then
		let curator_payment_id =
			get_child_bounty_payment_id(bounty_id, child_bounty_id, Some(child_curator_stash))
				.expect("no payment attempt");
		let beneficiary_payment_id =
			get_child_bounty_payment_id(bounty_id, child_bounty_id, Some(beneficiary))
				.expect("no payment attempt");
		expect_events(vec![
			ChildBountiesEvent::Paid {
				index: bounty_id,
				child_index: child_bounty_id,
				payment_id: curator_payment_id,
			},
			ChildBountiesEvent::Paid {
				index: bounty_id,
				child_index: child_bounty_id,
				payment_id: beneficiary_payment_id,
			},
		]);
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value: child_value,
				fee: 1,
				curator_deposit: 3,
				status: ChildBountyStatus::PayoutAttempted {
					curator: child_curator,
					curator_stash: (
						child_curator_stash,
						PaymentState::Attempted { id: curator_payment_id }
					),
					beneficiary: (
						beneficiary,
						PaymentState::Attempted { id: beneficiary_payment_id }
					),
				},
			}
		);

		// When
		set_status(beneficiary_payment_id, PaymentStatus::Success);
		let res = ChildBounties::check_payment_status(
			RuntimeOrigin::signed(user),
			bounty_id,
			child_bounty_id,
		);

		// Then
		assert_eq!(res.unwrap().pays_fee, Pays::Yes);
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id).unwrap(),
			ChildBounty {
				parent_bounty: bounty_id,
				asset_kind,
				value: child_value,
				fee: 1,
				curator_deposit: 3,
				status: ChildBountyStatus::PayoutAttempted {
					curator: child_curator,
					curator_stash: (
						child_curator_stash,
						PaymentState::Attempted { id: curator_payment_id }
					),
					beneficiary: (beneficiary, PaymentState::Succeeded),
				},
			}
		);

		// When
		set_status(curator_payment_id, PaymentStatus::Success);
		let res = ChildBounties::check_payment_status(
			RuntimeOrigin::signed(user),
			bounty_id,
			child_bounty_id,
		);

		// Then
		assert_eq!(res.unwrap().pays_fee, Pays::No);
		assert_eq!(
			last_event(),
			Event::PayoutProcessed {
				index: bounty_id,
				child_index: child_bounty_id,
				asset_kind,
				value: child_value - child_fee,
				beneficiary,
			}
		);
		assert_eq!(Balances::free_balance(child_curator), 6);
		assert_eq!(Balances::reserved_balance(child_curator), 0);
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id),
			None
		);
		assert_eq!(pallet_child_bounties::ParentChildBounties::<Test>::get(bounty_id), 0);
		assert_eq!(pallet_child_bounties::ParentTotalChildBounties::<Test>::get(bounty_id), 1);
		assert_eq!(pallet_child_bounties::ChildBounties::<Test>::iter().count(), 0);
		assert_eq!(pallet_child_bounties::ChildBountyDescriptionsV1::<Test>::iter().count(), 0);
	});
}

#[test]
fn check_and_process_refund_payment_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given (Make the parent bounty)
		let parent_curator = account_id(0);
		let bounty_id = 0;
		let child_bounty_id = 0;
		let asset_kind = 1;
		let parent_value = 1_000_000;
		let parent_fee = 10_000;
		let parent_curator_stash = account_id(10);
		let user = account_id(1);
		let child_value = 10_000;
		let bounty_account =
			Bounties::bounty_account_id(bounty_id, asset_kind).expect("conversion failed");

		Balances::make_free_balance_be(&parent_curator, parent_fee * 100);
		assert_ok!(Bounties::propose_bounty(
			RuntimeOrigin::signed(parent_curator),
			Box::new(1),
			parent_value,
			b"12345".to_vec()
		));
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), bounty_id));
		approve_bounty_payment(bounty_account, bounty_id, asset_kind, parent_value);
		assert_ok!(Bounties::propose_curator(
			RuntimeOrigin::root(),
			bounty_id,
			parent_curator,
			parent_fee
		));
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			parent_curator_stash
		));
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(parent_curator),
			bounty_id,
			child_value,
			b"12345-p1".to_vec()
		));
		let payment_id = get_child_bounty_payment_id(bounty_id, child_bounty_id, None)
			.expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(ChildBounties::check_payment_status(
			RuntimeOrigin::signed(user),
			bounty_id,
			child_bounty_id
		));
		assert_ok!(ChildBounties::close_child_bounty(
			RuntimeOrigin::root(),
			bounty_id,
			child_bounty_id
		));
		go_to_block(1);

		// When/Then (check ChildBountyStatus::RefundAttempted - PaymentState::Attempted -
		// PaymentStatus::InProgress)
		let payment_id = get_child_bounty_payment_id(bounty_id, child_bounty_id, None)
			.expect("no payment attempt");
		set_status(payment_id, PaymentStatus::InProgress);
		assert_noop!(
			ChildBounties::check_payment_status(
				RuntimeOrigin::signed(user),
				bounty_id,
				child_bounty_id
			),
			BountiesError::<Test>::RefundInconclusive
		);

		// When/Then (check ChildBountyStatus::RefundAttempted - PaymentState::Attempted -
		// PaymentStatus::Failure)
		set_status(payment_id, PaymentStatus::Failure);
		let res = ChildBounties::check_payment_status(
			RuntimeOrigin::signed(user),
			bounty_id,
			child_bounty_id,
		);
		assert_eq!(res.unwrap().pays_fee, Pays::Yes);

		// When/Then (check ChildBountyStatus::RefundAttempted - PaymentState::Failed)
		assert_noop!(
			ChildBounties::check_payment_status(
				RuntimeOrigin::signed(user),
				bounty_id,
				child_bounty_id
			),
			BountiesError::<Test>::UnexpectedStatus
		);

		// When (process ChildBountyStatus::RefundAttempted and check PaymentState::Success)
		assert_ok!(ChildBounties::process_payment(
			RuntimeOrigin::signed(user),
			bounty_id,
			child_bounty_id
		));
		let payment_id = get_child_bounty_payment_id(bounty_id, child_bounty_id, None)
			.expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(ChildBounties::check_payment_status(
			RuntimeOrigin::signed(user),
			bounty_id,
			child_bounty_id
		));

		// Then
		assert_eq!(
			last_event(),
			ChildBountiesEvent::RefundProcessed { index: bounty_id, child_index: child_bounty_id }
		);
		assert_eq!(
			Balances::free_balance(ChildBounties::child_bounty_account_id(
				bounty_id,
				child_bounty_id
			)),
			0
		);
		assert_eq!(
			pallet_child_bounties::ChildBounties::<Test>::get(bounty_id, child_bounty_id),
			None
		);
		assert_eq!(
			pallet_child_bounties::ChildBountyDescriptionsV1::<Test>::get(
				bounty_id,
				child_bounty_id
			),
			None
		);
	});
}

#[test]
fn integrity_test() {
	ExtBuilder::default().build_and_execute(|| {
		ChildBounties::integrity_test();
	});
}
