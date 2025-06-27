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

//! bounties pallet tests.

#![cfg(test)]

use super::{Event as BountiesEvent, *};
use crate as pallet_bounties;
use crate::mock::{Bounties, *};

use frame_support::{assert_err_ignore_postinfo, assert_noop, assert_ok, traits::Currency};
use sp_runtime::traits::Dispatchable;

type UtilityCall = pallet_utility::Call<Test>;
type BountiesCall = crate::Call<Test>;

#[test]
fn fund_bounty_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let asset_kind = 1;
		let value = 50;
		let curator = 4;
		let fee = 10;

		// When
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::root(),
			Box::new(asset_kind),
			value,
			curator,
			fee,
			b"1234567890".to_vec()
		));

		// Then
		let parent_bounty_id = 0;
		let payment_id = get_payment_id(parent_bounty_id, None, None).expect("no payment attempt");
		expect_events(vec![
			BountiesEvent::Paid { index: parent_bounty_id, child_index: None, payment_id },
			BountiesEvent::BountyFunded { index: parent_bounty_id },
		]);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(parent_bounty_id).unwrap(),
			Bounty {
				asset_kind,
				value,
				fee,
				curator_deposit: 0,
				status: BountyStatus::FundingAttempted {
					curator,
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);
		assert_eq!(pallet_bounties::BountyCount::<Test>::get(), 1);
		assert_eq!(
			pallet_bounties::BountyDescriptions::<Test>::get(parent_bounty_id).unwrap(),
			b"1234567890".to_vec()
		);
	});
}

#[test]
fn fund_bounty_in_batch_respects_max_total() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let asset_kind = 1;
		let spend_origin = 10; // max spending of 10
		let value = 2; // `native_amount` is 2
		let curator = 4;
		let fee = 0;

		// When/Then
		// Respect the `max_total` for the given origin.
		assert_ok!(RuntimeCall::from(UtilityCall::batch_all {
			calls: vec![
				RuntimeCall::from(BountiesCall::fund_bounty {
					asset_kind: Box::new(asset_kind),
					value,
					curator,
					fee,
					description: b"1234567890".to_vec()
				}),
				RuntimeCall::from(BountiesCall::fund_bounty {
					asset_kind: Box::new(asset_kind),
					value,
					curator,
					fee,
					description: b"1234567890".to_vec()
				})
			]
		})
		.dispatch(RuntimeOrigin::signed(spend_origin)));

		// Given
		let value = 5; // `native_amount` is 5

		// When/Then
		// `spend` of 10 surpasses `max_total` for the given origin.
		assert_err_ignore_postinfo!(
			RuntimeCall::from(UtilityCall::batch_all {
				calls: vec![
					RuntimeCall::from(BountiesCall::fund_bounty {
						asset_kind: Box::new(asset_kind),
						value,
						curator,
						fee,
						description: b"1234567890".to_vec()
					}),
					RuntimeCall::from(BountiesCall::fund_bounty {
						asset_kind: Box::new(asset_kind),
						value,
						curator,
						fee,
						description: b"1234567890".to_vec()
					})
				]
			})
			.dispatch(RuntimeOrigin::signed(spend_origin)),
			Error::<Test>::InsufficientPermission
		);
	});
}

#[test]
fn fund_bounty_second_instance_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let asset_kind = 1;
		let value = 10;
		let curator = 4;
		let fee = 1;
		let parent_bounty_id = 0;
		let parent_bounty_account_1 =
			Bounties::bounty_account(parent_bounty_id, asset_kind).expect("conversion failed");
		let parent_bounty_account_2 =
			Bounties1::bounty_account(parent_bounty_id, asset_kind).expect("conversion failed");

		// When
		assert_ok!(Bounties1::fund_bounty(
			RuntimeOrigin::root(),
			Box::new(asset_kind),
			value,
			curator,
			fee,
			b"1234567890".to_vec()
		));

		// Then
		assert_eq!(paid(parent_bounty_account_2, asset_kind), value); // Bounties 2 is funded
		assert_eq!(paid(parent_bounty_account_1, asset_kind), 0); // Bounties 1 is unchanged
	});
}

#[test]
fn fund_bounty_fails() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let asset_kind = 1;
		let curator = 4;
		let fee = 10;

		// When/Then
		assert_noop!(
			Bounties::fund_bounty(
				RuntimeOrigin::none(),
				Box::new(asset_kind),
				50,
				curator,
				fee,
				b"1234567890".to_vec()
			),
			BadOrigin
		);

		// When/Then
		assert_noop!(
			Bounties::fund_bounty(
				RuntimeOrigin::signed(0),
				Box::new(asset_kind),
				50,
				curator,
				fee,
				b"1234567890".to_vec()
			),
			BadOrigin
		);

		// When/Then
		assert_noop!(
			Bounties::fund_bounty(
				RuntimeOrigin::root(),
				Box::new(asset_kind),
				1,
				curator,
				fee,
				b"1234567890".to_vec()
			),
			Error::<Test>::InvalidFee
		);

		// When/Then
		assert_noop!(
			Bounties::fund_bounty(
				RuntimeOrigin::root(),
				Box::new(asset_kind),
				1,
				curator,
				0,
				b"1234567890".to_vec()
			),
			Error::<Test>::InvalidValue
		);

		// When/Then
		assert_noop!(
			Bounties::fund_bounty(
				RuntimeOrigin::signed(10), // max spending of 10
				Box::new(asset_kind),
				11,
				curator,
				fee,
				b"1234567890".to_vec()
			),
			Error::<Test>::InsufficientPermission
		);

		// When/Then
		SpendLimit::set(50);
		assert_noop!(
			Bounties::fund_bounty(
				RuntimeOrigin::root(),
				Box::new(asset_kind),
				51,
				curator,
				fee,
				b"1234567890".to_vec()
			),
			Error::<Test>::InsufficientPermission
		);
	});
}

#[test]
fn check_and_retry_funding_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let asset_kind = 1;
		let value = 50;
		let curator = 4;
		let fee = 10;
		let parent_bounty_id = 0;
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::root(),
			Box::new(asset_kind),
			value,
			curator,
			fee,
			b"1234567890".to_vec()
		));

		// When
		let payment_id = get_payment_id(parent_bounty_id, None, None).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Failure);
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), parent_bounty_id, None));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::PaymentFailed { index: parent_bounty_id, child_index: None, payment_id }
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(parent_bounty_id).unwrap(),
			Bounty {
				asset_kind,
				value,
				fee,
				curator_deposit: 0,
				status: BountyStatus::FundingAttempted {
					curator,
					payment_status: PaymentState::Failed
				},
			}
		);

		// When
		assert_ok!(Bounties::retry_payment(RuntimeOrigin::signed(1), parent_bounty_id, None));

		// Then
		let payment_id = get_payment_id(parent_bounty_id, None, None).expect("no payment attempt");
		assert_eq!(
			last_event(),
			BountiesEvent::Paid { index: parent_bounty_id, child_index: None, payment_id }
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(parent_bounty_id).unwrap(),
			Bounty {
				asset_kind,
				value,
				fee,
				curator_deposit: 0,
				status: BountyStatus::FundingAttempted {
					curator,
					payment_status: PaymentState::Attempted { id: 1 }
				},
			}
		);

		// When
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), parent_bounty_id, None));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::BountyFundingProcessed { index: parent_bounty_id, child_index: None }
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(parent_bounty_id).unwrap(),
			Bounty {
				asset_kind,
				value,
				fee,
				curator_deposit: 0,
				status: BountyStatus::Funded { curator },
			}
		);
	});
}

#[test]
fn check_and_retry_funding_fails() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let asset_kind = 1;
		let value = 50;
		let curator = 4;
		let fee = 10;
		let parent_bounty_id = 0;
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::root(),
			Box::new(asset_kind),
			value,
			curator,
			fee,
			b"1234567890".to_vec()
		));

		// When/Then
		assert_noop!(Bounties::check_status(RuntimeOrigin::none(), 2, None), BadOrigin);

		// When/Then
		assert_noop!(
			Bounties::check_status(RuntimeOrigin::signed(1), 2, None),
			Error::<Test>::InvalidIndex
		);

		// When/Then
		assert_noop!(
			Bounties::check_status(RuntimeOrigin::signed(1), parent_bounty_id, None),
			Error::<Test>::FundingInconclusive
		);

		// When/Then
		assert_noop!(
			Bounties::retry_payment(RuntimeOrigin::signed(1), parent_bounty_id, None),
			Error::<Test>::UnexpectedStatus
		);

		// When
		let payment_id = get_payment_id(parent_bounty_id, None, None).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Failure);
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), parent_bounty_id, None));

		// Then
		assert_noop!(
			Bounties::check_status(RuntimeOrigin::signed(1), parent_bounty_id, None),
			Error::<Test>::UnexpectedStatus
		);

		// When/Then
		assert_noop!(
			Bounties::retry_payment(RuntimeOrigin::none(), parent_bounty_id, None),
			BadOrigin
		);

		// When/Then
		assert_noop!(
			Bounties::retry_payment(RuntimeOrigin::signed(1), 2, None),
			Error::<Test>::InvalidIndex
		);
	});
}

#[test]
fn accept_curator_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let asset_kind = 1;
		let value = 50;
		let curator = 4;
		let curator_stash = 7;
		let fee = 10;
		let parent_bounty_id = 0;
		let parent_bounty_account =
			Bounties::bounty_account(parent_bounty_id, asset_kind).expect("conversion failed");
		Balances::make_free_balance_be(&curator, 6);
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::root(),
			Box::new(asset_kind),
			value,
			curator,
			fee,
			b"1234567890".to_vec()
		));
		approve_payment(parent_bounty_account, parent_bounty_id, None, asset_kind, value);

		// When
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(curator),
			parent_bounty_id,
			None,
			curator_stash
		));

		// Then
		let expected_deposit = Bounties::calculate_curator_deposit(&fee, asset_kind).unwrap();
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(parent_bounty_id).unwrap(),
			Bounty {
				asset_kind,
				value,
				fee,
				curator_deposit: expected_deposit,
				status: BountyStatus::Active { curator, curator_stash },
			}
		);
		assert_eq!(pallet_bounties::BountyCount::<Test>::get(), 1);
		assert_eq!(
			pallet_bounties::BountyDescriptions::<Test>::get(parent_bounty_id).unwrap(),
			b"1234567890".to_vec()
		);
	});
}

#[test]
fn accept_curator_handles_different_deposit_calculations() {
	// This test will verify that a bounty with and without a fee results
	// in a different curator deposit: one using the value, and one using the fee.
	ExtBuilder::default().build_and_execute(|| {
		// Given case 1: With a fee
		let curator = 1;
		let curator_stash = 0;
		let parent_bounty_id = 0;
		let asset_kind = 1;
		let value = 88;
		let fee = 42;
		let parent_bounty_account =
			Bounties::bounty_account(parent_bounty_id, asset_kind).expect("conversion failed");
		Balances::make_free_balance_be(&curator, 100);
		SpendLimit::set(value); // Allow for a larger spend limit
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::root(),
			Box::new(asset_kind),
			value,
			curator,
			fee,
			b"1234567890".to_vec()
		));
		approve_payment(parent_bounty_account, parent_bounty_id, None, asset_kind, value);

		// When
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(curator),
			parent_bounty_id,
			None,
			curator_stash
		));

		// Then
		let expected_deposit = CuratorDepositMultiplier::get() * fee;
		assert_eq!(Balances::free_balance(&curator), 100 - expected_deposit);
		assert_eq!(Balances::reserved_balance(&curator), expected_deposit);

		// Given case 2: Lower bound
		let curator = 2;
		let parent_bounty_id = 1;
		let value = 35;
		let fee = 0;
		let parent_bounty_account =
			Bounties::bounty_account(parent_bounty_id, asset_kind).expect("conversion failed");
		Balances::make_free_balance_be(&curator, 100);
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::root(),
			Box::new(asset_kind),
			value,
			curator,
			fee,
			b"1234567890".to_vec()
		));
		approve_payment(parent_bounty_account, parent_bounty_id, None, asset_kind, value);

		// When
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(curator),
			parent_bounty_id,
			None,
			curator_stash
		));

		// Then
		let expected_deposit = CuratorDepositMin::get();
		assert_eq!(Balances::free_balance(&curator), 100 - expected_deposit);
		assert_eq!(Balances::reserved_balance(&curator), expected_deposit);

		// Given case 3: Upper bound
		let curator = 3;
		let parent_bounty_id = 2;
		let value = 1_000_000;
		let fee = 50_000;
		let starting_balance = fee * 2;
		let parent_bounty_account =
			Bounties::bounty_account(parent_bounty_id, asset_kind).expect("conversion failed");
		Balances::make_free_balance_be(&curator, starting_balance);
		SpendLimit::set(value); // Allow for a larger spend limit:
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::root(),
			Box::new(asset_kind),
			value,
			curator,
			fee,
			b"1234567890".to_vec()
		));
		approve_payment(parent_bounty_account, parent_bounty_id, None, asset_kind, value);

		// When
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(curator),
			parent_bounty_id,
			None,
			curator_stash
		));

		// Then
		let expected_deposit = CuratorDepositMax::get();
		assert_eq!(Balances::free_balance(&curator), starting_balance - expected_deposit);
		assert_eq!(Balances::reserved_balance(&curator), expected_deposit);
	});
}

#[test]
fn accept_curator_fails() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let asset_kind = 1;
		let value = 50;
		let curator = 4;
		let curator_stash = 7;
		let fee = 10;
		let parent_bounty_id = 0;
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::root(),
			Box::new(asset_kind),
			value,
			curator,
			fee,
			b"1234567890".to_vec()
		));

		// When/Then
		assert_noop!(
			Bounties::accept_curator(
				RuntimeOrigin::signed(curator),
				parent_bounty_id,
				None,
				curator_stash
			),
			Error::<Test>::UnexpectedStatus
		);

		// When/Then
		assert_noop!(
			Bounties::accept_curator(RuntimeOrigin::none(), parent_bounty_id, None, curator_stash),
			BadOrigin
		);

		// When/Then
		assert_noop!(
			Bounties::accept_curator(RuntimeOrigin::signed(curator), 2, Some(2), curator_stash),
			Error::<Test>::InvalidIndex
		);

		// Given
		let expected_deposit = Bounties::calculate_curator_deposit(&fee, asset_kind).unwrap();
		Balances::make_free_balance_be(&curator, expected_deposit - 1);
		let parent_bounty_account =
			Bounties::bounty_account(parent_bounty_id, asset_kind).expect("conversion failed");
		approve_payment(parent_bounty_account, parent_bounty_id, None, asset_kind, value);

		// When/Then
		assert_noop!(
			Bounties::accept_curator(
				RuntimeOrigin::signed(1),
				parent_bounty_id,
				None,
				curator_stash
			),
			Error::<Test>::RequireCurator
		);

		// When/Then
		assert_noop!(
			Bounties::accept_curator(
				RuntimeOrigin::signed(curator),
				parent_bounty_id,
				None,
				curator_stash
			),
			pallet_balances::Error::<Test, _>::InsufficientBalance
		);
	});
}
