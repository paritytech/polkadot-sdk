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
fn fund_child_bounty_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let mut s = create_active_parent_bounty();

		// When
		assert_ok!(Bounties::fund_child_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			s.child_value,
			Some(s.child_curator),
			Some(s.child_fee),
			b"1234567890".to_vec()
		));
		s.child_bounty_id =
			pallet_bounties::TotalChildBountiesPerParent::<Test>::get(s.parent_bounty_id) - 1;

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), None)
			.expect("no payment attempt");
		expect_events(vec![
			BountiesEvent::Paid {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id),
				payment_id,
			},
			BountiesEvent::ChildBountyFunded {
				index: s.parent_bounty_id,
				child_index: s.child_bounty_id,
			},
		]);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap(),
			ChildBounty {
				parent_bounty: s.parent_bounty_id,
				value: s.child_value,
				fee: s.child_fee,
				curator_deposit: 0,
				status: BountyStatus::FundingAttempted {
					curator: s.child_curator,
					payment_status: PaymentState::Attempted { id: payment_id }
				}
			}
		);
		assert_eq!(
			pallet_bounties::TotalChildBountiesPerParent::<Test>::get(s.parent_bounty_id),
			1
		);
		assert_eq!(pallet_bounties::ChildBountiesPerParent::<Test>::get(s.parent_bounty_id), 1);
		assert_eq!(
			pallet_bounties::ChildBountyDescriptions::<Test>::get(
				s.parent_bounty_id,
				s.child_bounty_id
			)
			.unwrap(),
			b"1234567890".to_vec()
		);
		assert_eq!(
			pallet_bounties::ChildBountiesValuePerParent::<Test>::get(s.parent_bounty_id),
			s.child_value
		);
		assert_eq!(
			pallet_bounties::ChildBountiesCuratorFeesPerParent::<Test>::get(s.parent_bounty_id),
			s.child_fee
		);

		// When
		assert_ok!(Bounties::fund_child_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			s.child_value,
			None,
			None,
			b"1234567890".to_vec()
		));
		s.child_bounty_id =
			pallet_bounties::TotalChildBountiesPerParent::<Test>::get(s.parent_bounty_id) - 1;

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), None)
			.expect("no payment attempt");
		expect_events(vec![
			BountiesEvent::Paid {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id),
				payment_id,
			},
			BountiesEvent::ChildBountyFunded {
				index: s.parent_bounty_id,
				child_index: s.child_bounty_id,
			},
		]);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap(),
			ChildBounty {
				parent_bounty: s.parent_bounty_id,
				value: s.child_value,
				fee: 0,
				curator_deposit: 0,
				status: BountyStatus::FundingAttempted {
					curator: s.curator,
					payment_status: PaymentState::Attempted { id: payment_id }
				}
			}
		);
	})
}

#[test]
fn fund_child_bounty_fails() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let mut s = create_active_parent_bounty();

		// When/Then
		assert_noop!(
			Bounties::fund_child_bounty(
				RuntimeOrigin::none(),
				s.parent_bounty_id,
				s.child_value,
				Some(s.child_curator),
				Some(s.child_fee),
				b"1234567890".to_vec()
			),
			BadOrigin
		);

		// When/Then
		assert_noop!(
			Bounties::fund_child_bounty(
				RuntimeOrigin::signed(s.curator),
				2,
				s.child_value,
				None,
				None,
				b"1234567890".to_vec()
			),
			Error::<Test>::InvalidIndex
		);

		// When/Then
		assert_noop!(
			Bounties::fund_child_bounty(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				0,
				None,
				None,
				b"1234567890".to_vec()
			),
			Error::<Test>::InvalidValue
		);

		// When/Then
		assert_noop!(
			Bounties::fund_child_bounty(
				RuntimeOrigin::signed(1),
				s.parent_bounty_id,
				s.child_value,
				None,
				None,
				b"1234567890".to_vec()
			),
			Error::<Test>::RequireCurator
		);

		// When/Then
		assert_noop!(
			Bounties::fund_child_bounty(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				51,
				None,
				None,
				b"1234567890".to_vec()
			),
			Error::<Test>::InsufficientBountyValue
		);

		// When/Then
		assert_noop!(
			Bounties::fund_child_bounty(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				s.child_value,
				None,
				Some(11),
				b"1234567890".to_vec()
			),
			Error::<Test>::InvalidFee
		);

		// Given
		MaxActiveChildBountyCount::set(1);
		assert_ok!(Bounties::fund_child_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			s.child_value,
			None,
			None,
			b"1234567890".to_vec()
		));

		// When/Then
		assert_noop!(
			Bounties::fund_child_bounty(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				s.child_value,
				None,
				None,
				b"1234567890".to_vec()
			),
			Error::<Test>::TooManyChildBounties
		);

		// Given
		let s = create_awarded_parent_bounty();

		// When/Then
		assert_noop!(
			Bounties::fund_child_bounty(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				s.child_value,
				None,
				None,
				b"1234567890".to_vec()
			),
			Error::<Test>::UnexpectedStatus
		);
	})
}

#[test]
fn check_status_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: parent bounty status is `FundingAttempted` and payment fails
		let s = create_parent_bounty();
		let payment_id =
			get_payment_id(s.parent_bounty_id, None, None).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Failure);

		// When
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::PaymentFailed {
				index: s.parent_bounty_id,
				child_index: None,
				payment_id
			}
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap(),
			Bounty {
				asset_kind: s.asset_kind,
				value: s.value,
				fee: s.fee,
				curator_deposit: 0,
				status: BountyStatus::FundingAttempted {
					curator: s.curator,
					payment_status: PaymentState::Failed
				},
			}
		);

		// Given: parent bounty status is `FundingAttempted` and payment succeeds
		let s = create_parent_bounty();
		let payment_id =
			get_payment_id(s.parent_bounty_id, None, None).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);

		// When
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::BountyFundingProcessed { index: s.parent_bounty_id, child_index: None }
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap().status,
			BountyStatus::Funded { curator: s.curator }
		);

		// Given: parent bounty status is `RefundAttempted` and payment fails
		let s = create_canceled_parent_bounty();
		let payment_id =
			get_payment_id(s.parent_bounty_id, None, None).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Failure);

		// When
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::PaymentFailed {
				index: s.parent_bounty_id,
				child_index: None,
				payment_id
			}
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap().status,
			BountyStatus::RefundAttempted {
				curator: Some(s.curator),
				payment_status: PaymentState::Failed
			}
		);

		// Given: parent bounty status is `RefundAttempted` and payment succeeds
		let s = create_canceled_parent_bounty();
		let payment_id =
			get_payment_id(s.parent_bounty_id, None, None).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);

		// When
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::BountyRefundProcessed { index: s.parent_bounty_id, child_index: None }
		);
		assert_eq!(Balances::free_balance(s.curator), 100); // initial 100
		assert_eq!(pallet_bounties::Bounties::<Test>::iter().count(), 4 - 1);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id), None);
		assert_eq!(pallet_bounties::BountyDescriptions::<Test>::get(s.parent_bounty_id), None);

		// Given: parent bounty status is `PayoutAttempted` with 2 payments failed
		let s = create_awarded_parent_bounty();
		let beneficiary_payment_id = get_payment_id(s.parent_bounty_id, None, Some(s.beneficiary))
			.expect("no payment attempt");
		let curator_payment_id = get_payment_id(s.parent_bounty_id, None, Some(s.curator_stash))
			.expect("no payment attempt");
		set_status(beneficiary_payment_id, PaymentStatus::Failure);
		set_status(curator_payment_id, PaymentStatus::Failure);

		// When
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		expect_events(vec![
			BountiesEvent::PaymentFailed {
				index: s.parent_bounty_id,
				child_index: None,
				payment_id: beneficiary_payment_id,
			},
			BountiesEvent::PaymentFailed {
				index: s.parent_bounty_id,
				child_index: None,
				payment_id: curator_payment_id,
			},
		]);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap().status,
			BountyStatus::PayoutAttempted {
				curator: s.curator,
				beneficiary: (s.beneficiary, PaymentState::Failed),
				curator_stash: (s.curator_stash, PaymentState::Failed)
			}
		);

		// Given: parent bounty status is `PayoutAttempted` with 1 payment failed and 1 succeeded
		let s = create_awarded_parent_bounty();
		let beneficiary_payment_id = get_payment_id(s.parent_bounty_id, None, Some(s.beneficiary))
			.expect("no payment attempt");
		let curator_payment_id = get_payment_id(s.parent_bounty_id, None, Some(s.curator_stash))
			.expect("no payment attempt");
		set_status(beneficiary_payment_id, PaymentStatus::Success);
		set_status(curator_payment_id, PaymentStatus::Failure);

		// When
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		expect_events(vec![BountiesEvent::PaymentFailed {
			index: s.parent_bounty_id,
			child_index: None,
			payment_id: curator_payment_id,
		}]);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap().status,
			BountyStatus::PayoutAttempted {
				curator: s.curator,
				beneficiary: (s.beneficiary, PaymentState::Succeeded),
				curator_stash: (s.curator_stash, PaymentState::Failed)
			}
		);

		// Given: parent bounty status is `PayoutAttempted` with 2 payments succeeded
		let s = create_awarded_parent_bounty();
		let beneficiary_payment_id = get_payment_id(s.parent_bounty_id, None, Some(s.beneficiary))
			.expect("no payment attempt");
		let curator_payment_id = get_payment_id(s.parent_bounty_id, None, Some(s.curator_stash))
			.expect("no payment attempt");
		set_status(beneficiary_payment_id, PaymentStatus::Success);
		set_status(curator_payment_id, PaymentStatus::Success);

		// When
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		expect_events(vec![BountiesEvent::BountyPayoutProcessed {
			index: s.parent_bounty_id,
			child_index: None,
			asset_kind: s.asset_kind,
			value: s.value - s.fee,
			beneficiary: s.beneficiary,
		}]);
		assert_eq!(Balances::free_balance(s.curator), 100); // initial 100
		assert_eq!(pallet_bounties::Bounties::<Test>::iter().count(), 7 - 1 - 1);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id), None);
		assert_eq!(pallet_bounties::BountyDescriptions::<Test>::get(s.parent_bounty_id), None);

		// Given: child-bounty status is `FundingAttempted` and payment fails
		let s = create_child_bounty_with_curator();
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), None)
			.expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Failure);

		// When
		assert_ok!(Bounties::check_status(
			RuntimeOrigin::signed(1),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::PaymentFailed {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id),
				payment_id
			}
		);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap(),
			ChildBounty {
				parent_bounty: s.parent_bounty_id,
				value: s.child_value,
				fee: s.child_fee,
				curator_deposit: 0,
				status: BountyStatus::FundingAttempted {
					curator: s.child_curator,
					payment_status: PaymentState::Failed
				},
			}
		);

		// Given: child-bounty with curator status is `FundingAttempted` and payment succeeds
		let s = create_child_bounty_with_curator();
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), None)
			.expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);

		// When
		assert_ok!(Bounties::check_status(
			RuntimeOrigin::signed(1),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::BountyFundingProcessed {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id),
			}
		);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap()
				.status,
			BountyStatus::Funded { curator: s.child_curator },
		);

		// Given: child-bounty without curator and status `FundingAttempted` and payment fails
		let s = create_child_bounty_without_curator();
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), None)
			.expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Failure);

		// When
		assert_ok!(Bounties::check_status(
			RuntimeOrigin::signed(1),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::PaymentFailed {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id),
				payment_id
			}
		);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap(),
			ChildBounty {
				parent_bounty: s.parent_bounty_id,
				value: s.child_value,
				fee: 0,
				curator_deposit: 0,
				status: BountyStatus::FundingAttempted {
					curator: s.curator,
					payment_status: PaymentState::Failed
				},
			}
		);

		// Given: child-bounty without curator and status `FundingAttempted` and payment succeeds
		let s = create_child_bounty_without_curator();
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), None)
			.expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);

		// When
		assert_ok!(Bounties::check_status(
			RuntimeOrigin::signed(1),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));

		// Then bounty becomes active
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap()
				.status,
			BountyStatus::Active { curator: s.curator, curator_stash: s.curator_stash },
		);
	});
}

#[test]
fn check_status_fails() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let s = create_parent_bounty();

		// When/Then
		assert_noop!(Bounties::check_status(RuntimeOrigin::none(), 2, None), BadOrigin);

		// When/Then
		assert_noop!(
			Bounties::check_status(RuntimeOrigin::signed(1), 2, None),
			Error::<Test>::InvalidIndex
		);

		// When/Then
		assert_noop!(
			Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None),
			Error::<Test>::FundingInconclusive
		);

		// Given
		let payment_id =
			get_payment_id(s.parent_bounty_id, None, None).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Failure);
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		assert_noop!(
			Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None),
			Error::<Test>::UnexpectedStatus
		);

		// Given
		let s = create_canceled_parent_bounty();

		// Then
		assert_noop!(
			Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None),
			Error::<Test>::RefundInconclusive
		);

		// Given
		let s = create_awarded_parent_bounty();

		// Then
		assert_noop!(
			Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None),
			Error::<Test>::PayoutInconclusive
		);
	});
}

#[test]
fn retry_payment_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: parent bounty status `FundingAttempted`
		let s = create_parent_bounty();
		let payment_id =
			get_payment_id(s.parent_bounty_id, None, None).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Failure);
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// When
		assert_ok!(Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		let payment_id =
			get_payment_id(s.parent_bounty_id, None, None).expect("no payment attempt");
		assert_eq!(
			last_event(),
			BountiesEvent::Paid { index: s.parent_bounty_id, child_index: None, payment_id }
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap(),
			Bounty {
				asset_kind: s.asset_kind,
				value: s.value,
				fee: s.fee,
				curator_deposit: 0,
				status: BountyStatus::FundingAttempted {
					curator: s.curator,
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);

		// Given: parent bounty status `RefundAttempted`
		let s = create_canceled_parent_bounty();
		let payment_id =
			get_payment_id(s.parent_bounty_id, None, None).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Failure);
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// When
		assert_ok!(Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		let payment_id =
			get_payment_id(s.parent_bounty_id, None, None).expect("no payment attempt");
		assert_eq!(
			last_event(),
			BountiesEvent::Paid { index: s.parent_bounty_id, child_index: None, payment_id }
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap().status,
			BountyStatus::RefundAttempted {
				curator: Some(s.curator),
				payment_status: PaymentState::Attempted { id: payment_id }
			},
		);

		// Given: parent bounty status `PayoutAttempted` with 1 payment failed (beneficiary)
		let s = create_awarded_parent_bounty();
		let beneficiary_payment_id = get_payment_id(s.parent_bounty_id, None, Some(s.beneficiary))
			.expect("no payment attempt");
		set_status(beneficiary_payment_id, PaymentStatus::Failure);
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// When
		assert_ok!(Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		let beneficiary_payment_id = get_payment_id(s.parent_bounty_id, None, Some(s.beneficiary))
			.expect("no payment attempt");
		let curator_stash_payment_id =
			get_payment_id(s.parent_bounty_id, None, Some(s.curator_stash))
				.expect("no payment attempt");
		assert_eq!(
			last_event(),
			BountiesEvent::Paid {
				index: s.parent_bounty_id,
				child_index: None,
				payment_id: beneficiary_payment_id
			}
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap().status,
			BountyStatus::PayoutAttempted {
				curator: s.curator,
				beneficiary: (
					s.beneficiary,
					PaymentState::Attempted { id: beneficiary_payment_id }
				),
				curator_stash: (
					s.curator_stash,
					PaymentState::Attempted { id: curator_stash_payment_id }
				)
			}
		);

		// Given: parent bounty status `PayoutAttempted` with 2 payments failed
		let s = create_awarded_parent_bounty();
		let beneficiary_payment_id = get_payment_id(s.parent_bounty_id, None, Some(s.beneficiary))
			.expect("no payment attempt");
		let curator_stash_payment_id =
			get_payment_id(s.parent_bounty_id, None, Some(s.curator_stash))
				.expect("no payment attempt");
		set_status(beneficiary_payment_id, PaymentStatus::Failure);
		set_status(curator_stash_payment_id, PaymentStatus::Failure);
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// When
		assert_ok!(Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		let beneficiary_payment_id = get_payment_id(s.parent_bounty_id, None, Some(s.beneficiary))
			.expect("no payment attempt");
		let curator_stash_payment_id =
			get_payment_id(s.parent_bounty_id, None, Some(s.curator_stash))
				.expect("no payment attempt");
		expect_events(vec![
			BountiesEvent::Paid {
				index: s.parent_bounty_id,
				child_index: None,
				payment_id: beneficiary_payment_id,
			},
			BountiesEvent::Paid {
				index: s.parent_bounty_id,
				child_index: None,
				payment_id: curator_stash_payment_id,
			},
		]);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap().status,
			BountyStatus::PayoutAttempted {
				curator: s.curator,
				beneficiary: (
					s.beneficiary,
					PaymentState::Attempted { id: beneficiary_payment_id }
				),
				curator_stash: (
					s.curator_stash,
					PaymentState::Attempted { id: curator_stash_payment_id }
				)
			}
		);
	});
}

#[test]
fn retry_payment_fails() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let s = create_parent_bounty();

		// When/Then
		assert_noop!(
			Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, None),
			Error::<Test>::UnexpectedStatus
		);

		// When/Then
		assert_noop!(
			Bounties::retry_payment(RuntimeOrigin::none(), s.parent_bounty_id, None),
			BadOrigin
		);

		// When/Then
		assert_noop!(
			Bounties::retry_payment(RuntimeOrigin::signed(1), 2, None),
			Error::<Test>::InvalidIndex
		);

		// Given
		let s = create_parent_bounty();
		let payment_id =
			get_payment_id(s.parent_bounty_id, None, None).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);

		// When/Then
		assert_noop!(
			Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, None),
			Error::<Test>::UnexpectedStatus
		);

		// Given
		let s = create_canceled_parent_bounty();
		let payment_id =
			get_payment_id(s.parent_bounty_id, None, None).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);

		// When/Then
		assert_noop!(
			Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, None),
			Error::<Test>::UnexpectedStatus
		);

		// Given
		let s = create_awarded_parent_bounty();
		let beneficiary_payment_id = get_payment_id(s.parent_bounty_id, None, Some(s.beneficiary))
			.expect("no payment attempt");
		let curator_stash_payment_id =
			get_payment_id(s.parent_bounty_id, None, Some(s.curator_stash))
				.expect("no payment attempt");
		set_status(beneficiary_payment_id, PaymentStatus::Success);
		set_status(curator_stash_payment_id, PaymentStatus::Success);

		// When/Then
		assert_noop!(
			Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, None),
			Error::<Test>::UnexpectedStatus
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

#[test]
fn unassign_curator_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let s = create_funded_parent_bounty();

		// When case 1: Bounty status is `Funded` and sender is the curator
		assert_ok!(Bounties::unassign_curator(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			None
		));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::CuratorUnassigned { index: s.parent_bounty_id, child_index: None }
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap(),
			Bounty {
				asset_kind: s.asset_kind,
				value: s.value,
				fee: s.fee,
				curator_deposit: 0,
				status: BountyStatus::CuratorUnassigned,
			}
		);
		assert_eq!(Balances::free_balance(&s.curator), 100);
		assert_eq!(Balances::reserved_balance(&s.curator), 0); // not slashed

		// Given
		let s = create_funded_parent_bounty();

		// When case 2: Bounty status is `Funded` and sender is `RejectOrigin`
		assert_ok!(Bounties::unassign_curator(RuntimeOrigin::root(), s.parent_bounty_id, None));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::CuratorUnassigned { index: s.parent_bounty_id, child_index: None }
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap(),
			Bounty {
				asset_kind: s.asset_kind,
				value: s.value,
				fee: s.fee,
				curator_deposit: 0,
				status: BountyStatus::CuratorUnassigned,
			}
		);
		assert_eq!(Balances::free_balance(&s.curator), 100);
		assert_eq!(Balances::reserved_balance(&s.curator), 0); // not slashed

		// Given
		let s = create_active_parent_bounty();

		// When case 4: Bounty status is `Active` and sender is the curator
		assert_ok!(Bounties::unassign_curator(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			None
		));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::CuratorUnassigned { index: s.parent_bounty_id, child_index: None }
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap(),
			Bounty {
				asset_kind: s.asset_kind,
				value: s.value,
				fee: s.fee,
				curator_deposit: 0,
				status: BountyStatus::CuratorUnassigned,
			}
		);
		assert_eq!(Balances::free_balance(&s.curator), 100);
		assert_eq!(Balances::reserved_balance(&s.curator), 0); // not slashed

		// Given
		let s = create_active_parent_bounty();

		// When case 5: Bounty status is `Active` and sender is `RejectOrigin`
		assert_ok!(Bounties::unassign_curator(RuntimeOrigin::root(), s.parent_bounty_id, None));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::CuratorUnassigned { index: s.parent_bounty_id, child_index: None }
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap(),
			Bounty {
				asset_kind: s.asset_kind,
				value: s.value,
				fee: s.fee,
				curator_deposit: 0,
				status: BountyStatus::CuratorUnassigned,
			}
		);
		assert_eq!(Balances::free_balance(&s.curator), 95); // slashed
		assert_eq!(Balances::reserved_balance(&s.curator), 0);
	});
}

#[test]
fn unassign_curator_fails() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let s = setup_bounty();
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::root(),
			Box::new(s.asset_kind),
			s.value,
			s.curator,
			s.fee,
			b"1234567890".to_vec()
		));

		// When/Then
		assert_noop!(
			Bounties::unassign_curator(RuntimeOrigin::root(), s.parent_bounty_id, None),
			Error::<Test>::UnexpectedStatus
		);

		// Given
		let s = create_funded_parent_bounty();

		// When/Then
		assert_noop!(
			Bounties::unassign_curator(RuntimeOrigin::none(), s.parent_bounty_id, None),
			BadOrigin
		);

		// When/Then
		assert_noop!(
			Bounties::unassign_curator(RuntimeOrigin::signed(1), s.parent_bounty_id, None),
			BadOrigin
		);

		// When/Then
		assert_noop!(
			Bounties::unassign_curator(RuntimeOrigin::root(), 2, None),
			Error::<Test>::InvalidIndex
		);

		// Given
		let s = create_active_parent_bounty();

		// When/Then
		assert_noop!(
			Bounties::unassign_curator(RuntimeOrigin::signed(1), s.parent_bounty_id, None),
			BadOrigin
		);
	});
}

#[test]
fn propose_curator_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let s = create_parent_bounty_with_unassigned_curator();

		// When
		assert_ok!(Bounties::propose_curator(
			RuntimeOrigin::root(),
			s.parent_bounty_id,
			None,
			s.curator,
			s.fee
		));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::CuratorProposed {
				index: s.parent_bounty_id,
				child_index: None,
				curator: s.curator
			}
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap(),
			Bounty {
				asset_kind: s.asset_kind,
				value: s.value,
				fee: s.fee,
				curator_deposit: 0,
				status: BountyStatus::Funded { curator: s.curator },
			}
		);
	});
}

#[test]
fn propose_curator_fails() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let s = create_funded_parent_bounty();

		// When/Then
		assert_noop!(
			Bounties::propose_curator(
				RuntimeOrigin::root(),
				s.parent_bounty_id,
				None,
				s.curator,
				s.fee
			),
			Error::<Test>::UnexpectedStatus
		);

		// Given
		let s = create_parent_bounty_with_unassigned_curator();

		// When/Then
		assert_noop!(
			Bounties::propose_curator(
				RuntimeOrigin::signed(1),
				s.parent_bounty_id,
				None,
				s.curator,
				s.fee
			),
			BadOrigin
		);

		// When/Then
		assert_noop!(
			Bounties::propose_curator(RuntimeOrigin::root(), 3, None, s.curator, s.fee),
			Error::<Test>::InvalidIndex
		);

		// When/Then
		assert_noop!(
			Bounties::propose_curator(
				RuntimeOrigin::root(),
				s.parent_bounty_id,
				None,
				s.curator,
				s.value + 1
			),
			Error::<Test>::InvalidFee
		);

		// When/Then
		SpendLimit::set(s.value - 1);
		assert_noop!(
			Bounties::propose_curator(
				RuntimeOrigin::root(),
				s.parent_bounty_id,
				None,
				s.curator,
				s.fee
			),
			Error::<Test>::InsufficientPermission
		);
	});
}

#[test]
fn award_bounty_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let s = create_active_parent_bounty();

		// When
		assert_ok!(Bounties::award_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			None,
			s.beneficiary
		));

		// Then
		let beneficiary_payment_id = get_payment_id(s.parent_bounty_id, None, Some(s.beneficiary))
			.expect("no payment attempt");
		let curator_payment_id = get_payment_id(s.parent_bounty_id, None, Some(s.curator_stash))
			.expect("no payment attempt");
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap(),
			Bounty {
				asset_kind: s.asset_kind,
				value: s.value,
				fee: s.fee,
				curator_deposit: s.curator_deposit,
				status: BountyStatus::PayoutAttempted {
					curator: s.curator,
					beneficiary: (
						s.beneficiary,
						PaymentState::Attempted { id: beneficiary_payment_id }
					),
					curator_stash: (
						s.curator_stash,
						PaymentState::Attempted { id: curator_payment_id }
					),
				},
			}
		);
		expect_events(vec![
			BountiesEvent::Paid {
				index: s.parent_bounty_id,
				child_index: None,
				payment_id: beneficiary_payment_id,
			},
			BountiesEvent::Paid {
				index: s.parent_bounty_id,
				child_index: None,
				payment_id: curator_payment_id,
			},
			BountiesEvent::BountyAwarded {
				index: s.parent_bounty_id,
				child_index: None,
				beneficiary: s.beneficiary,
				curator_stash: s.curator_stash,
			},
		]);

		// Given a funded bounty with high fee
		let mut s = setup_bounty();
		s.fee = s.value - 1;
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::root(),
			Box::new(s.asset_kind),
			s.value,
			s.curator,
			s.fee,
			b"1234567890".to_vec()
		));
		s.parent_bounty_id = pallet_bounties::BountyCount::<Test>::get() - 1;
		let parent_bounty_account =
			Bounties::bounty_account(s.parent_bounty_id, s.asset_kind.clone())
				.expect("conversion failed");
		approve_payment(
			parent_bounty_account,
			s.parent_bounty_id,
			None,
			s.asset_kind.clone(),
			s.value,
		);
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			None,
			s.curator_stash
		));

		// When
		assert_ok!(Bounties::award_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			None,
			s.beneficiary
		));

		// Then
		let beneficiary_payment_id = get_payment_id(s.parent_bounty_id, None, Some(s.beneficiary))
			.expect("no payment attempt");
		let curator_payment_id = get_payment_id(s.parent_bounty_id, None, Some(s.curator_stash))
			.expect("no payment attempt");
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap(),
			Bounty {
				asset_kind: s.asset_kind,
				value: s.value,
				fee: s.fee,
				curator_deposit: 24,
				status: BountyStatus::PayoutAttempted {
					curator: s.curator,
					beneficiary: (
						s.beneficiary,
						PaymentState::Attempted { id: beneficiary_payment_id }
					),
					curator_stash: (
						s.curator_stash,
						PaymentState::Attempted { id: curator_payment_id }
					),
				},
			}
		);
	});
}

#[test]
fn award_bounty_fails() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let s = create_funded_parent_bounty();

		// When/Then
		assert_noop!(
			Bounties::award_bounty(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				None,
				s.beneficiary
			),
			Error::<Test>::UnexpectedStatus
		);

		// Given
		let s = create_active_parent_bounty();

		// When/Then
		assert_noop!(
			Bounties::award_bounty(
				RuntimeOrigin::signed(1),
				s.parent_bounty_id,
				None,
				s.beneficiary
			),
			Error::<Test>::RequireCurator
		);

		// When/Then
		assert_noop!(
			Bounties::award_bounty(RuntimeOrigin::signed(1), 3, None, s.beneficiary),
			Error::<Test>::InvalidIndex
		);
	})
}

#[test]
fn close_bounty_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let s = create_funded_parent_bounty();

		// When
		assert_ok!(Bounties::close_bounty(RuntimeOrigin::root(), s.parent_bounty_id, None,));

		// Then
		let payment_id =
			get_payment_id(s.parent_bounty_id, None, None).expect("no payment attempt");
		assert_eq!(
			last_event(),
			BountiesEvent::BountyCanceled { index: s.parent_bounty_id, child_index: None }
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap(),
			Bounty {
				asset_kind: s.asset_kind,
				value: s.value,
				fee: s.fee,
				curator_deposit: 0,
				status: BountyStatus::RefundAttempted {
					curator: Some(s.curator),
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);

		// Given
		let s = create_parent_bounty_with_unassigned_curator();

		// When
		assert_ok!(Bounties::close_bounty(RuntimeOrigin::root(), s.parent_bounty_id, None,));

		// Then
		let payment_id =
			get_payment_id(s.parent_bounty_id, None, None).expect("no payment attempt");
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap(),
			Bounty {
				asset_kind: s.asset_kind,
				value: s.value,
				fee: s.fee,
				curator_deposit: 0,
				status: BountyStatus::RefundAttempted {
					curator: None,
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);
	})
}

#[test]
fn close_bounty_fails() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let s = create_parent_bounty();

		// When/Then
		assert_noop!(
			Bounties::close_bounty(RuntimeOrigin::root(), s.parent_bounty_id, None,),
			Error::<Test>::UnexpectedStatus
		);

		// Given
		let s = create_funded_parent_bounty();

		// When/Then
		assert_noop!(
			Bounties::close_bounty(RuntimeOrigin::none(), s.parent_bounty_id, None,),
			BadOrigin
		);

		// When/Then
		assert_noop!(
			Bounties::close_bounty(RuntimeOrigin::root(), 3, None,),
			Error::<Test>::InvalidIndex
		);
	})
}
