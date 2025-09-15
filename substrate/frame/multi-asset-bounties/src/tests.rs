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

//! Bounties pallet tests.

#![cfg(test)]

use super::{Event as BountiesEvent, *};
use crate as pallet_bounties;
use crate::mock::{Bounties, *};

use frame_support::{
	assert_err_ignore_postinfo, assert_noop, assert_ok,
	traits::{fungible::Mutate, Currency},
};
use sp_runtime::{traits::Dispatchable, TokenError};

type UtilityCall = pallet_utility::Call<Test>;
type BountiesCall = crate::Call<Test>;

#[docify::export]
#[test]
fn fund_bounty_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let asset_kind = 1;
		let value = 50;
		let curator = 4;
		let metadata = note_preimage(1);
		let _ = Balances::mint_into(&curator, Balances::minimum_balance());

		// When
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::root(),
			Box::new(asset_kind),
			value,
			curator,
			metadata
		));

		// Then
		let parent_bounty_id = 0;
		let bounty_account =
			Bounties::bounty_account(parent_bounty_id, asset_kind).expect("conversion failed");
		let payment_id = get_payment_id(parent_bounty_id, None).expect("no payment attempt");
		assert_eq!(paid(bounty_account, asset_kind), value);
		expect_events(vec![
			BountiesEvent::Paid { index: parent_bounty_id, child_index: None, payment_id },
			BountiesEvent::BountyCreated { index: parent_bounty_id },
		]);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(parent_bounty_id).unwrap(),
			Bounty {
				asset_kind,
				value,
				metadata,
				status: BountyStatus::FundingAttempted {
					curator,
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);
		assert_eq!(pallet_bounties::BountyCount::<Test>::get(), 1);
		assert!(Preimage::is_requested(&metadata));
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(parent_bounty_id, None::<BountyIndex>),
			None
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
		let metadata = note_preimage(1);
		let _ = Balances::mint_into(&curator, Balances::minimum_balance());

		// When/Then
		// Respect the `max_total` for the given origin.
		assert_ok!(RuntimeCall::from(UtilityCall::batch_all {
			calls: vec![
				RuntimeCall::from(BountiesCall::fund_bounty {
					asset_kind: Box::new(asset_kind),
					value,
					curator,
					metadata
				}),
				RuntimeCall::from(BountiesCall::fund_bounty {
					asset_kind: Box::new(asset_kind),
					value,
					curator,
					metadata
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
						metadata
					}),
					RuntimeCall::from(BountiesCall::fund_bounty {
						asset_kind: Box::new(asset_kind),
						value,
						curator,
						metadata
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
		let parent_bounty_id = 0;
		let parent_bounty_account_1 =
			Bounties::bounty_account(parent_bounty_id, asset_kind).expect("conversion failed");
		let parent_bounty_account_2 =
			Bounties1::bounty_account(parent_bounty_id, asset_kind).expect("conversion failed");
		let metadata = note_preimage(1);
		let _ = Balances::mint_into(&curator, Balances::minimum_balance());

		// When
		assert_ok!(Bounties1::fund_bounty(
			RuntimeOrigin::root(),
			Box::new(asset_kind),
			value,
			curator,
			metadata
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
		let value = 50;
		let curator = 4;
		let metadata = note_preimage(1);
		let _ = Balances::mint_into(&curator, Balances::minimum_balance());

		// When/Then
		let invalid_origin = RuntimeOrigin::none();
		assert_noop!(
			Bounties::fund_bounty(invalid_origin, Box::new(asset_kind), value, curator, metadata),
			BadOrigin
		);

		// When/Then
		let invalid_origin = RuntimeOrigin::signed(0);
		assert_noop!(
			Bounties::fund_bounty(invalid_origin, Box::new(asset_kind), value, curator, metadata),
			BadOrigin
		);

		// When/Then
		let invalid_value = 1;
		assert_noop!(
			Bounties::fund_bounty(
				RuntimeOrigin::root(),
				Box::new(asset_kind),
				invalid_value,
				curator,
				metadata
			),
			Error::<Test>::InvalidValue
		);

		// When/Then
		let invalid_value = 11;
		assert_noop!(
			Bounties::fund_bounty(
				RuntimeOrigin::signed(10), // max spending of 10
				Box::new(asset_kind),
				invalid_value,
				curator,
				metadata
			),
			Error::<Test>::InsufficientPermission
		);

		// When/Then
		let invalid_metadata: <Test as frame_system::Config>::Hash = [1u8; 32].into();
		assert_noop!(
			Bounties::fund_bounty(
				RuntimeOrigin::root(),
				Box::new(asset_kind),
				value,
				curator,
				invalid_metadata
			),
			Error::<Test>::PreimageNotExist
		);

		// When/Then
		SpendLimit::set(50);
		let invalid_value = 51;
		assert_noop!(
			Bounties::fund_bounty(
				RuntimeOrigin::root(),
				Box::new(asset_kind),
				invalid_value,
				curator,
				metadata
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
			s.metadata
		));
		s.child_bounty_id =
			pallet_bounties::TotalChildBountiesPerParent::<Test>::get(s.parent_bounty_id) - 1;

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id))
			.expect("no payment attempt");
		let child_bounty_account =
			Bounties::child_bounty_account(s.parent_bounty_id, s.child_bounty_id, s.asset_kind)
				.expect("conversion failed");
		assert_eq!(paid(child_bounty_account, s.asset_kind), s.child_value);
		expect_events(vec![
			BountiesEvent::Paid {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id),
				payment_id,
			},
			BountiesEvent::ChildBountyCreated {
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
				metadata: s.metadata,
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
			pallet_bounties::ChildBountiesValuePerParent::<Test>::get(s.parent_bounty_id),
			s.child_value
		);
		assert!(Preimage::is_requested(&s.metadata));
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(
				s.parent_bounty_id,
				Some(s.child_bounty_id)
			),
			None
		);

		// When
		assert_ok!(Bounties::fund_child_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			s.child_value,
			None,
			s.metadata
		));
		s.child_bounty_id =
			pallet_bounties::TotalChildBountiesPerParent::<Test>::get(s.parent_bounty_id) - 1;

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id))
			.expect("no payment attempt");
		expect_events(vec![
			BountiesEvent::Paid {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id),
				payment_id,
			},
			BountiesEvent::ChildBountyCreated {
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
				metadata: s.metadata,
				status: BountyStatus::FundingAttempted {
					curator: s.curator,
					payment_status: PaymentState::Attempted { id: payment_id }
				}
			}
		);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(s.parent_bounty_id, None::<BountyIndex>)
				.unwrap(),
			consideration(s.value)
		);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(
				s.parent_bounty_id,
				Some(s.child_bounty_id)
			),
			None
		);
	})
}

#[test]
fn fund_child_bounty_fails() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let s = create_active_parent_bounty();

		// When/Then
		let invalid_origin = RuntimeOrigin::none();
		assert_noop!(
			Bounties::fund_child_bounty(
				invalid_origin,
				s.parent_bounty_id,
				s.child_value,
				Some(s.child_curator),
				s.metadata
			),
			BadOrigin
		);

		// When/Then
		let invalid_parent_index = 2;
		assert_noop!(
			Bounties::fund_child_bounty(
				RuntimeOrigin::signed(s.curator),
				invalid_parent_index,
				s.child_value,
				None,
				s.metadata
			),
			Error::<Test>::InvalidIndex
		);

		// When/Then
		let invalid_value = 0;
		assert_noop!(
			Bounties::fund_child_bounty(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				invalid_value,
				None,
				s.metadata
			),
			Error::<Test>::InvalidValue
		);

		// When/Then
		let invalid_metadata: <Test as frame_system::Config>::Hash = [1u8; 32].into();
		assert_noop!(
			Bounties::fund_child_bounty(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				s.child_value,
				None,
				invalid_metadata
			),
			Error::<Test>::PreimageNotExist
		);

		// When/Then
		let invalid_origin = RuntimeOrigin::signed(1);
		assert_noop!(
			Bounties::fund_child_bounty(
				invalid_origin,
				s.parent_bounty_id,
				s.child_value,
				None,
				s.metadata
			),
			Error::<Test>::RequireCurator
		);

		// When/Then
		let invalid_value = s.value + 1;
		assert_noop!(
			Bounties::fund_child_bounty(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				invalid_value,
				None,
				s.metadata
			),
			Error::<Test>::InsufficientBountyValue
		);

		// Given
		MaxActiveChildBountyCount::set(1);
		assert_ok!(Bounties::fund_child_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			s.child_value,
			None,
			s.metadata
		));

		// When/Then
		assert_noop!(
			Bounties::fund_child_bounty(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				s.child_value,
				None,
				s.metadata
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
				s.metadata
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
		let payment_id = get_payment_id(s.parent_bounty_id, None).expect("no payment attempt");
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
				metadata: s.metadata,
				status: BountyStatus::FundingAttempted {
					curator: s.curator,
					payment_status: PaymentState::Failed
				},
			}
		);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(s.parent_bounty_id, None::<BountyIndex>),
			None
		);

		// Given: parent bounty status is `FundingAttempted` and payment succeeds
		let s = create_parent_bounty();
		let payment_id = get_payment_id(s.parent_bounty_id, None).expect("no payment attempt");
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
		let payment_id = get_payment_id(s.parent_bounty_id, None).expect("no payment attempt");
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
		assert!(Preimage::is_requested(&s.metadata));
		assert_eq!(Balances::free_balance(s.curator), Balances::minimum_balance());
		assert_eq!(Balances::reserved_balance(s.curator), s.curator_deposit);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(s.parent_bounty_id, None::<BountyIndex>)
				.unwrap(),
			consideration(s.value)
		);

		// Given: parent bounty status is `RefundAttempted` and payment succeeds
		let s = create_canceled_parent_bounty();
		let payment_id = get_payment_id(s.parent_bounty_id, None).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);

		// When
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::BountyRefundProcessed { index: s.parent_bounty_id, child_index: None }
		);
		assert_eq!(pallet_bounties::Bounties::<Test>::iter().count(), 4 - 1);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id), None);
		assert_eq!(pallet_bounties::ChildBountiesPerParent::<Test>::get(s.parent_bounty_id), 0);
		assert_eq!(
			pallet_bounties::TotalChildBountiesPerParent::<Test>::get(s.parent_bounty_id),
			0
		);
		assert!(!Preimage::is_requested(&s.metadata));
		assert_eq!(
			Balances::free_balance(s.curator),
			Balances::minimum_balance() + s.curator_deposit
		); // initial
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(s.parent_bounty_id, None::<BountyIndex>),
			None
		);

		// Given: parent bounty status is `PayoutAttempted` and payment fails
		let s = create_awarded_parent_bounty();
		let payment_id = get_payment_id(s.parent_bounty_id, None).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Failure);

		// When
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		expect_events(vec![BountiesEvent::PaymentFailed {
			index: s.parent_bounty_id,
			child_index: None,
			payment_id,
		}]);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap().status,
			BountyStatus::PayoutAttempted {
				curator: s.curator,
				beneficiary: s.beneficiary,
				payment_status: PaymentState::Failed
			}
		);
		assert!(Preimage::is_requested(&s.metadata));
		assert_eq!(Balances::free_balance(s.curator), Balances::minimum_balance());
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(s.parent_bounty_id, None::<BountyIndex>)
				.unwrap(),
			consideration(s.value)
		);

		// Given: parent bounty status is `PayoutAttempted` and payment succeeds
		let s = create_awarded_parent_bounty();
		let payment_id = get_payment_id(s.parent_bounty_id, None).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);

		// When
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		expect_events(vec![BountiesEvent::BountyPayoutProcessed {
			index: s.parent_bounty_id,
			child_index: None,
			asset_kind: s.asset_kind,
			value: s.value,
			beneficiary: s.beneficiary,
		}]);
		assert_eq!(pallet_bounties::Bounties::<Test>::iter().count(), 6 - 1 - 1);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id), None);
		assert_eq!(pallet_bounties::ChildBountiesPerParent::<Test>::get(s.parent_bounty_id), 0);
		assert_eq!(
			pallet_bounties::TotalChildBountiesPerParent::<Test>::get(s.parent_bounty_id),
			0
		);
		assert!(!Preimage::is_requested(&s.metadata));
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(s.parent_bounty_id, None::<BountyIndex>),
			None
		);
		assert_eq!(
			Balances::free_balance(s.curator),
			Balances::minimum_balance() + s.curator_deposit
		); // initial

		// Given: child-bounty status is `FundingAttempted` and payment fails
		let s = create_child_bounty_with_curator();
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id))
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
				metadata: s.metadata,
				status: BountyStatus::FundingAttempted {
					curator: s.child_curator,
					payment_status: PaymentState::Failed
				},
			}
		);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(
				s.parent_bounty_id,
				Some(s.child_bounty_id)
			),
			None
		);

		// Given: child-bounty with curator status is `FundingAttempted` and payment succeeds
		let s = create_child_bounty_with_curator();
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id))
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
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::iter_prefix(s.parent_bounty_id).count(),
			1
		);

		// Given: child-bounty with curator and status `RefundAttempted` and payment fails
		let s = create_canceled_child_bounty();
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id))
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
				.unwrap()
				.status,
			BountyStatus::RefundAttempted {
				curator: Some(s.child_curator),
				payment_status: PaymentState::Failed
			},
		);
		assert_eq!(
			pallet_bounties::ChildBountiesValuePerParent::<Test>::get(s.parent_bounty_id),
			s.child_value
		);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::iter_prefix(s.parent_bounty_id).count(),
			1
		);
		assert!(Preimage::is_requested(&s.metadata));
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(
				s.parent_bounty_id,
				Some(s.child_bounty_id)
			)
			.unwrap(),
			consideration(s.child_value)
		);
		assert_eq!(Balances::free_balance(s.child_curator), Balances::minimum_balance());

		// Given: child-bounty with curator and status `RefundAttempted` and payment succeeds
		let s = create_canceled_child_bounty();
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id))
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
			BountiesEvent::BountyRefundProcessed {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id)
			}
		);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id),
			None
		);
		assert_eq!(pallet_bounties::ChildBountiesPerParent::<Test>::get(s.parent_bounty_id), 0);
		assert_eq!(
			pallet_bounties::TotalChildBountiesPerParent::<Test>::get(s.parent_bounty_id),
			1
		);
		assert_eq!(
			pallet_bounties::ChildBountiesValuePerParent::<Test>::get(s.parent_bounty_id),
			0
		);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::iter_prefix(s.parent_bounty_id).count(),
			0
		);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(
				s.parent_bounty_id,
				Some(s.child_bounty_id)
			),
			None
		);
		assert!(Preimage::is_requested(&s.metadata)); // still requested by parent bounty
		assert_eq!(
			Balances::free_balance(s.child_curator),
			Balances::minimum_balance() + s.child_curator_deposit
		); // initial

		// Given: child-bounty without curator and status `RefundAttempted` and payment succeeds
		let s = create_active_parent_bounty();
		assert_ok!(Bounties::fund_child_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			s.child_value,
			None,
			s.metadata
		));
		let child_bounty_account =
			Bounties::child_bounty_account(s.parent_bounty_id, s.child_bounty_id, s.asset_kind)
				.expect("conversion failed");
		approve_payment(
			child_bounty_account,
			s.parent_bounty_id,
			Some(s.child_bounty_id),
			s.asset_kind,
			s.child_value,
		);
		assert_ok!(Bounties::close_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			Some(s.child_bounty_id),
		));
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id))
			.expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);
		assert_eq!(Balances::free_balance(s.curator), Balances::minimum_balance()); // reserved

		// When
		assert_ok!(Bounties::check_status(
			RuntimeOrigin::signed(1),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));

		// Then
		assert!(Preimage::is_requested(&s.metadata)); // still requested by parent bounty
		assert_eq!(Balances::free_balance(s.curator), Balances::minimum_balance()); // still reserved

		// Given: child-bounty status is `PayoutAttempted` and payment succeeds
		let s = create_awarded_child_bounty();
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id))
			.expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);

		// When
		assert_ok!(Bounties::check_status(
			RuntimeOrigin::signed(1),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));

		// Then
		expect_events(vec![BountiesEvent::BountyPayoutProcessed {
			index: s.parent_bounty_id,
			child_index: Some(s.child_bounty_id),
			asset_kind: s.asset_kind,
			value: s.child_value,
			beneficiary: s.child_beneficiary,
		}]);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::iter_prefix(s.parent_bounty_id).count(),
			0
		);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id),
			None
		);
		assert!(Preimage::is_requested(&s.metadata)); // still requested by parent bounty
		assert_eq!(pallet_bounties::ChildBountiesPerParent::<Test>::get(s.parent_bounty_id), 0);
		assert_eq!(
			pallet_bounties::TotalChildBountiesPerParent::<Test>::get(s.parent_bounty_id),
			1
		);
		assert_eq!(
			pallet_bounties::ChildBountiesValuePerParent::<Test>::get(s.parent_bounty_id),
			s.child_value
		);
		assert_eq!(
			Balances::free_balance(s.child_curator),
			Balances::minimum_balance() + s.child_curator_deposit
		); // initial

		// Given: award same parent bounty as previous `Given` setup
		assert_ok!(Bounties::award_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			None,
			s.beneficiary
		));

		// When
		let payment_id = get_payment_id(s.parent_bounty_id, None).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		assert_eq!(
			pallet_bounties::ChildBountiesValuePerParent::<Test>::get(s.parent_bounty_id),
			0
		);
		assert!(!Preimage::is_requested(&s.metadata)); // no longer requested
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
			Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, Some(2)),
			Error::<Test>::InvalidIndex
		);

		// When/Then
		assert_noop!(
			Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None),
			Error::<Test>::FundingInconclusive
		);

		// Given
		let payment_id = get_payment_id(s.parent_bounty_id, None).expect("no payment attempt");
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
		let parent_bounty_account =
			Bounties::bounty_account(s.parent_bounty_id, s.asset_kind).expect("conversion failed");
		reject_payment(parent_bounty_account, s.parent_bounty_id, None, s.asset_kind, s.value);

		// When
		assert_ok!(Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, None).expect("no payment attempt");
		assert_eq!(paid(parent_bounty_account, s.asset_kind), s.value);
		assert_eq!(
			last_event(),
			BountiesEvent::Paid { index: s.parent_bounty_id, child_index: None, payment_id }
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap(),
			Bounty {
				asset_kind: s.asset_kind,
				value: s.value,
				metadata: s.metadata,
				status: BountyStatus::FundingAttempted {
					curator: s.curator,
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(s.parent_bounty_id, None::<BountyIndex>),
			None
		);

		// Given: parent bounty status `RefundAttempted`
		let s = create_canceled_parent_bounty();
		let funding_source_account =
			Bounties::funding_source_account(s.asset_kind).expect("conversion failed");
		reject_payment(funding_source_account, s.parent_bounty_id, None, s.asset_kind, s.value);

		// When
		assert_ok!(Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, None).expect("no payment attempt");
		assert_eq!(paid(funding_source_account, s.asset_kind), s.value);
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

		// Given: parent bounty status `PayoutAttempted`
		let s = create_awarded_parent_bounty();
		reject_payment(s.beneficiary, s.parent_bounty_id, None, s.asset_kind, s.value);

		// When
		assert_ok!(Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, None).expect("no payment attempt");
		assert_eq!(paid(s.beneficiary, s.asset_kind), s.value);
		assert_eq!(
			last_event(),
			BountiesEvent::Paid { index: s.parent_bounty_id, child_index: None, payment_id }
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap().status,
			BountyStatus::PayoutAttempted {
				curator: s.curator,
				beneficiary: s.beneficiary,
				payment_status: PaymentState::Attempted { id: payment_id }
			}
		);

		// Given: child-bounty status `FundingAttempted`
		let s = create_child_bounty_with_curator();
		let child_bounty_account =
			Bounties::child_bounty_account(s.parent_bounty_id, s.child_bounty_id, s.asset_kind)
				.expect("conversion failed");
		reject_payment(
			child_bounty_account,
			s.parent_bounty_id,
			Some(s.child_bounty_id),
			s.asset_kind,
			s.child_value,
		);

		// When
		assert_ok!(Bounties::retry_payment(
			RuntimeOrigin::signed(1),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id))
			.expect("no payment attempt");
		assert_eq!(paid(child_bounty_account, s.asset_kind), s.child_value);
		assert_eq!(
			last_event(),
			BountiesEvent::Paid {
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
				metadata: s.metadata,
				status: BountyStatus::FundingAttempted {
					curator: s.child_curator,
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(
				s.parent_bounty_id,
				Some(s.child_bounty_id)
			),
			None
		);

		// Given: child-bounty status `RefundAttempted`
		let s = create_canceled_child_bounty();
		let child_bounty_account =
			Bounties::child_bounty_account(s.parent_bounty_id, s.child_bounty_id, s.asset_kind)
				.expect("conversion failed");
		let parent_bounty_account =
			Bounties::bounty_account(s.parent_bounty_id, s.asset_kind).expect("conversion failed");
		reject_payment(
			parent_bounty_account,
			s.parent_bounty_id,
			Some(s.child_bounty_id),
			s.asset_kind,
			s.child_value,
		);

		// When
		assert_ok!(Bounties::retry_payment(
			RuntimeOrigin::signed(1),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id))
			.expect("no payment attempt");
		assert_eq!(paid(child_bounty_account, s.asset_kind), s.child_value);
		assert_eq!(
			last_event(),
			BountiesEvent::Paid {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id),
				payment_id
			}
		);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap()
				.status,
			BountyStatus::RefundAttempted {
				curator: Some(s.child_curator),
				payment_status: PaymentState::Attempted { id: payment_id }
			},
		);

		// Given: child-bounty status `PayoutAttempted`
		let s = create_awarded_child_bounty();
		reject_payment(
			s.child_beneficiary,
			s.parent_bounty_id,
			Some(s.child_bounty_id),
			s.asset_kind,
			s.child_value,
		);

		// When
		assert_ok!(Bounties::retry_payment(
			RuntimeOrigin::signed(1),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id))
			.expect("no payment attempt");
		assert_eq!(paid(s.child_beneficiary, s.asset_kind), s.child_value);
		expect_events(vec![BountiesEvent::Paid {
			index: s.parent_bounty_id,
			child_index: Some(s.child_bounty_id),
			payment_id,
		}]);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap()
				.status,
			BountyStatus::PayoutAttempted {
				curator: s.child_curator,
				beneficiary: s.child_beneficiary,
				payment_status: PaymentState::Attempted { id: payment_id }
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

		// When/Then
		assert_noop!(
			Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, Some(1)),
			Error::<Test>::InvalidIndex
		);

		// Given: parent bounty status is `FundingAttempted` and payment succeeds
		let s = create_parent_bounty();
		let payment_id = get_payment_id(s.parent_bounty_id, None).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);

		// When/Then
		assert_noop!(
			Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, None),
			Error::<Test>::UnexpectedStatus
		);

		// Given: parent bounty status is `RefundAttempted` and payment succeeds
		let s = create_canceled_parent_bounty();
		let payment_id = get_payment_id(s.parent_bounty_id, None).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);

		// When/Then
		assert_noop!(
			Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, None),
			Error::<Test>::UnexpectedStatus
		);

		// Given: parent bounty status is `PayoutAttempted` and payments succeed
		let s = create_awarded_parent_bounty();
		let payment_id = get_payment_id(s.parent_bounty_id, None).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);

		// When/Then
		assert_noop!(
			Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, None),
			Error::<Test>::UnexpectedStatus
		);

		// Given: child-bounty status is `FundingAttempted` and payment succeeds
		let s = create_child_bounty_with_curator();
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id))
			.expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);

		// When/Then
		assert_noop!(
			Bounties::retry_payment(
				RuntimeOrigin::signed(1),
				s.parent_bounty_id,
				Some(s.child_bounty_id)
			),
			Error::<Test>::UnexpectedStatus
		);
	});
}

#[test]
fn accept_curator_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: parent bounty status is `Funded`
		let s = create_funded_parent_bounty();

		// When
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			None,
		));

		// Then
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap(),
			Bounty {
				asset_kind: s.asset_kind,
				value: s.value,
				metadata: s.metadata,
				status: BountyStatus::Active { curator: s.curator },
			}
		);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(s.parent_bounty_id, None::<BountyIndex>)
				.unwrap(),
			consideration(s.value)
		);
		assert_eq!(Balances::reserved_balance(&s.curator), s.curator_deposit);

		// Given: child-bounty status is `Funded`
		let s = create_funded_child_bounty();

		// When
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(s.child_curator),
			s.parent_bounty_id,
			Some(s.child_bounty_id),
		));

		// Then
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap(),
			ChildBounty {
				parent_bounty: s.parent_bounty_id,
				value: s.child_value,
				metadata: s.metadata,
				status: BountyStatus::Active { curator: s.child_curator },
			}
		);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(
				s.parent_bounty_id,
				Some(s.child_bounty_id)
			)
			.unwrap(),
			consideration(s.child_value)
		);
		assert_eq!(Balances::reserved_balance(&s.child_curator), s.child_curator_deposit);

		// Given: 2nd child-bounty with same curator
		let _ = Balances::mint_into(&s.child_curator, s.child_curator_deposit);
		assert_ok!(Bounties::fund_child_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			s.child_value,
			Some(s.child_curator),
			s.metadata
		));
		let child_bounty_id =
			pallet_bounties::TotalChildBountiesPerParent::<Test>::get(s.parent_bounty_id) - 1;
		let child_bounty_account =
			Bounties::child_bounty_account(s.parent_bounty_id, child_bounty_id, s.asset_kind)
				.expect("conversion failed");
		approve_payment(
			child_bounty_account,
			s.parent_bounty_id,
			Some(child_bounty_id),
			s.asset_kind,
			s.child_value * 2,
		);

		// When
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(s.child_curator),
			s.parent_bounty_id,
			Some(child_bounty_id),
		));
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(
				s.parent_bounty_id,
				Some(s.child_bounty_id)
			)
			.unwrap(),
			consideration(s.child_value)
		);
		assert_eq!(Balances::reserved_balance(&s.child_curator), s.child_curator_deposit * 2);
	});
}

#[test]
fn accept_curator_handles_different_deposit_calculations() {
	ExtBuilder::default().build_and_execute(|| {
		// Given case 1: Lower bound
		let curator = 2;
		let parent_bounty_id = 0;
		let asset_kind = 1;
		let value = 2;
		let metadata = note_preimage(1);
		let parent_bounty_account =
			Bounties::bounty_account(parent_bounty_id, asset_kind).expect("conversion failed");
		Balances::make_free_balance_be(&curator, 100);
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::root(),
			Box::new(asset_kind),
			value,
			curator,
			metadata
		));
		approve_payment(parent_bounty_account, parent_bounty_id, None, asset_kind, value);

		// When
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(curator),
			parent_bounty_id,
			None,
		));

		// Then
		let expected_deposit = CuratorDepositMin::get();
		assert_eq!(Balances::free_balance(&curator), 100 - expected_deposit);
		assert_eq!(Balances::reserved_balance(&curator), expected_deposit);

		// Given case 2: Upper bound
		let curator = 3;
		let parent_bounty_id = 1;
		let value = 1_000_000;
		let starting_balance = value * 2;
		let parent_bounty_account =
			Bounties::bounty_account(parent_bounty_id, asset_kind).expect("conversion failed");
		Balances::make_free_balance_be(&curator, starting_balance);
		SpendLimit::set(value); // Allow for a larger spend limit:
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::root(),
			Box::new(asset_kind),
			value,
			curator,
			metadata
		));
		approve_payment(parent_bounty_account, parent_bounty_id, None, asset_kind, value);

		// When
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(curator),
			parent_bounty_id,
			None,
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
		let s = create_parent_bounty();

		// When/Then
		assert_noop!(
			Bounties::accept_curator(RuntimeOrigin::signed(s.curator), s.parent_bounty_id, None,),
			Error::<Test>::UnexpectedStatus
		);

		// When/Then
		assert_noop!(
			Bounties::accept_curator(RuntimeOrigin::none(), s.parent_bounty_id, None,),
			BadOrigin
		);

		// When/Then
		assert_noop!(
			Bounties::accept_curator(RuntimeOrigin::signed(s.curator), 2, None),
			Error::<Test>::InvalidIndex
		);

		// When/Then
		assert_noop!(
			Bounties::accept_curator(
				RuntimeOrigin::signed(s.child_curator),
				s.parent_bounty_id,
				Some(2),
			),
			Error::<Test>::InvalidIndex
		);

		// Given: parent bounty status is `Funded`
		Balances::make_free_balance_be(&s.curator, s.curator_deposit - 1);
		let parent_bounty_account =
			Bounties::bounty_account(s.parent_bounty_id, s.asset_kind).expect("conversion failed");
		approve_payment(parent_bounty_account, s.parent_bounty_id, None, s.asset_kind, s.value);

		// When/Then
		assert_noop!(
			Bounties::accept_curator(RuntimeOrigin::signed(1), s.parent_bounty_id, None,),
			Error::<Test>::RequireCurator
		);

		// When/Then
		assert_noop!(
			Bounties::accept_curator(RuntimeOrigin::signed(s.curator), s.parent_bounty_id, None,),
			TokenError::FundsUnavailable
		);

		// Given: child-bounty status is `Funded`
		let s = create_child_bounty_with_curator();
		Balances::make_free_balance_be(&s.child_curator, s.child_curator_deposit - 1);
		let child_bounty_account =
			Bounties::child_bounty_account(s.parent_bounty_id, s.child_bounty_id, s.asset_kind)
				.expect("conversion failed");
		approve_payment(
			child_bounty_account,
			s.parent_bounty_id,
			Some(s.child_bounty_id),
			s.asset_kind,
			s.child_value,
		);

		// When/Then
		assert_noop!(
			Bounties::accept_curator(
				RuntimeOrigin::signed(1),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			),
			Error::<Test>::RequireCurator
		);

		// When/Then
		assert_noop!(
			Bounties::accept_curator(
				RuntimeOrigin::signed(s.child_curator),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			),
			TokenError::FundsUnavailable
		);
	});
}

#[test]
fn unassign_curator_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: parent bounty status is `Funded`
		let s = create_funded_parent_bounty();

		// When: sender is the curator
		assert_ok!(Bounties::unassign_curator(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			None
		));

		// Then
		expect_events(vec![BountiesEvent::CuratorUnassigned {
			index: s.parent_bounty_id,
			child_index: None,
		}]);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap(),
			Bounty {
				asset_kind: s.asset_kind,
				value: s.value,
				metadata: s.metadata,
				status: BountyStatus::CuratorUnassigned,
			}
		);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(s.parent_bounty_id, None::<BountyIndex>),
			None
		);
		assert_eq!(
			Balances::free_balance(&s.curator),
			Balances::minimum_balance() + s.curator_deposit
		); // not burned

		// Given: parent bounty status is `Funded`
		let s = create_funded_parent_bounty();

		// When: sender is `RejectOrigin`
		assert_ok!(Bounties::unassign_curator(RuntimeOrigin::root(), s.parent_bounty_id, None));

		// Then
		assert_eq!(
			Balances::free_balance(&s.curator),
			Balances::minimum_balance() + s.curator_deposit
		); // not burned

		// Given: parent bounty status is `Active` and sender is the curator
		let s = create_active_parent_bounty();

		// When: sender is the curator
		assert_ok!(Bounties::unassign_curator(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			None
		));

		// Then
		assert_eq!(
			Balances::free_balance(&s.curator),
			Balances::minimum_balance() + s.curator_deposit
		); // not burned

		// Given: parent bounty status is `Active` and sender is `RejectOrigin`
		let s = create_active_parent_bounty();

		// When: sender is `RejectOrigin`
		assert_ok!(Bounties::unassign_curator(RuntimeOrigin::root(), s.parent_bounty_id, None));

		// Then
		assert_eq!(Balances::free_balance(&s.curator), Balances::minimum_balance()); // burned

		// Given: child-bounty status is `Funded`
		let s = create_funded_child_bounty();

		// When: sender is the child curator
		assert_ok!(Bounties::unassign_curator(
			RuntimeOrigin::signed(s.child_curator),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));

		// Then
		expect_events(vec![BountiesEvent::CuratorUnassigned {
			index: s.parent_bounty_id,
			child_index: Some(s.child_bounty_id),
		}]);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap(),
			ChildBounty {
				parent_bounty: s.parent_bounty_id,
				value: s.child_value,
				metadata: s.metadata,
				status: BountyStatus::CuratorUnassigned,
			}
		);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(
				s.parent_bounty_id,
				Some(s.child_bounty_id)
			),
			None
		);
		assert_eq!(
			Balances::free_balance(&s.child_curator),
			Balances::minimum_balance() + s.child_curator_deposit
		); // not burned

		// Given: child-bounty status is `Funded`
		let s = create_funded_child_bounty();

		// When: sender is `RejectOrigin`
		assert_ok!(Bounties::unassign_curator(
			RuntimeOrigin::root(),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));

		// Then
		assert_eq!(
			Balances::free_balance(&s.child_curator),
			Balances::minimum_balance() + s.child_curator_deposit
		); // not burned

		// Given: child-bounty status is `Active`
		let s = create_active_child_bounty();

		// When: sender is child curator
		assert_ok!(Bounties::unassign_curator(
			RuntimeOrigin::signed(s.child_curator),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));

		// Then
		assert_eq!(
			Balances::free_balance(&s.child_curator),
			Balances::minimum_balance() + s.child_curator_deposit
		); // not burned

		// Given: child-bounty status is `Active`
		let s = create_active_child_bounty();

		// When: sender is parent curator
		assert_ok!(Bounties::unassign_curator(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));

		// Then
		assert_eq!(Balances::free_balance(&s.child_curator), Balances::minimum_balance()); // burned
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
			s.metadata
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

		// When/Then
		assert_noop!(
			Bounties::unassign_curator(RuntimeOrigin::root(), s.parent_bounty_id, Some(1)),
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
		// Given: parent bounty status `UnassignedCurator`
		let s = create_parent_bounty_with_unassigned_curator();

		// When
		assert_ok!(Bounties::propose_curator(
			RuntimeOrigin::root(),
			s.parent_bounty_id,
			None,
			s.curator,
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
				metadata: s.metadata,
				status: BountyStatus::Funded { curator: s.curator },
			}
		);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(s.parent_bounty_id, None::<BountyIndex>),
			None
		);

		// Given: child-bounty status `UnassignedCurator`
		let s = create_child_bounty_with_unassigned_curator();

		// When
		assert_ok!(Bounties::propose_curator(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			Some(s.child_bounty_id),
			s.child_curator,
		));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::CuratorProposed {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id),
				curator: s.child_curator
			}
		);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap(),
			ChildBounty {
				parent_bounty: s.parent_bounty_id,
				value: s.child_value,
				metadata: s.metadata,
				status: BountyStatus::Funded { curator: s.child_curator },
			}
		);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(
				s.parent_bounty_id,
				Some(s.child_bounty_id)
			),
			None
		);
	});
}

#[test]
fn propose_curator_fails() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: parent bounty status `Funded`
		let s = create_funded_parent_bounty();

		// When/Then
		assert_noop!(
			Bounties::propose_curator(RuntimeOrigin::root(), s.parent_bounty_id, None, s.curator,),
			Error::<Test>::UnexpectedStatus
		);

		// Given: parent bounty status `UnassignedCurator`
		let s = create_parent_bounty_with_unassigned_curator();

		// When/Then
		assert_noop!(
			Bounties::propose_curator(
				RuntimeOrigin::signed(1),
				s.parent_bounty_id,
				None,
				s.curator,
			),
			BadOrigin
		);

		// When/Then
		assert_noop!(
			Bounties::propose_curator(RuntimeOrigin::root(), 3, None, s.curator),
			Error::<Test>::InvalidIndex
		);

		// When/Then
		SpendLimit::set(s.value - 1);
		assert_noop!(
			Bounties::propose_curator(RuntimeOrigin::root(), s.parent_bounty_id, None, s.curator,),
			Error::<Test>::InsufficientPermission
		);

		// Given: child-bounty status `Funded`
		SpendLimit::set(s.value);
		let s = create_active_parent_bounty();
		assert_ok!(Bounties::fund_child_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			s.child_value,
			Some(s.child_curator),
			s.metadata
		));

		// When/Then
		assert_noop!(
			Bounties::propose_curator(
				RuntimeOrigin::signed(1),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
				s.child_curator,
			),
			Error::<Test>::UnexpectedStatus
		);

		// Given: child-bounty status `UnassignedCurator`
		let s = create_child_bounty_with_unassigned_curator();

		// When/Then
		assert_noop!(
			Bounties::propose_curator(
				RuntimeOrigin::signed(1),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
				s.child_curator,
			),
			BadOrigin
		);

		// When/Then
		assert_noop!(
			Bounties::propose_curator(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				Some(3),
				s.child_curator,
			),
			Error::<Test>::InvalidIndex
		);

		// When/Then
		assert_noop!(
			Bounties::propose_curator(
				RuntimeOrigin::root(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
				s.child_curator,
			),
			BadOrigin
		);

		// When/Then
		assert_noop!(
			Bounties::propose_curator(
				RuntimeOrigin::signed(1),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
				s.child_curator,
			),
			BadOrigin
		);
	});
}

#[docify::export]
#[test]
fn award_bounty_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: parent bounty status `Active`
		let s = create_active_parent_bounty();

		// When
		assert_ok!(Bounties::award_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			None,
			s.beneficiary
		));

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, None).expect("no payment attempt");
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap(),
			Bounty {
				asset_kind: s.asset_kind,
				value: s.value,
				metadata: s.metadata,
				status: BountyStatus::PayoutAttempted {
					curator: s.curator,
					beneficiary: s.beneficiary,
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(s.parent_bounty_id, None::<BountyIndex>)
				.unwrap(),
			consideration(s.value)
		);

		expect_events(vec![
			BountiesEvent::Paid { index: s.parent_bounty_id, child_index: None, payment_id },
			BountiesEvent::BountyAwarded {
				index: s.parent_bounty_id,
				child_index: None,
				beneficiary: s.beneficiary,
			},
		]);

		// Given: child-bounty status `Active`
		let s = create_active_child_bounty();

		// When
		assert_ok!(Bounties::award_bounty(
			RuntimeOrigin::signed(s.child_curator),
			s.parent_bounty_id,
			Some(s.child_bounty_id),
			s.child_beneficiary
		));

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id))
			.expect("no payment attempt");
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap(),
			ChildBounty {
				parent_bounty: s.parent_bounty_id,
				value: s.child_value,
				metadata: s.metadata,
				status: BountyStatus::PayoutAttempted {
					curator: s.child_curator,
					beneficiary: s.child_beneficiary,
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(
				s.parent_bounty_id,
				Some(s.child_bounty_id)
			)
			.unwrap(),
			consideration(s.child_value)
		);
		expect_events(vec![
			BountiesEvent::Paid {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id),
				payment_id,
			},
			BountiesEvent::BountyAwarded {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id),
				beneficiary: s.child_beneficiary,
			},
		]);
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
			Bounties::award_bounty(RuntimeOrigin::signed(s.curator), 3, None, s.beneficiary),
			Error::<Test>::InvalidIndex
		);

		// Given
		let s = create_active_child_bounty();

		// When/Then
		assert_noop!(
			Bounties::award_bounty(
				RuntimeOrigin::signed(s.child_curator),
				s.parent_bounty_id,
				Some(3),
				s.child_beneficiary
			),
			Error::<Test>::InvalidIndex
		);
	})
}

#[test]
fn close_bounty_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: parent bounty status is `Funded`
		let s = create_funded_parent_bounty();

		// When: sender is `RejectOrigin`
		assert_ok!(Bounties::close_bounty(RuntimeOrigin::root(), s.parent_bounty_id, None,));

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, None).expect("no payment attempt");
		assert_eq!(
			last_event(),
			BountiesEvent::BountyCanceled { index: s.parent_bounty_id, child_index: None }
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap(),
			Bounty {
				asset_kind: s.asset_kind,
				value: s.value,
				metadata: s.metadata,
				status: BountyStatus::RefundAttempted {
					curator: Some(s.curator),
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(s.parent_bounty_id, None::<BountyIndex>),
			None
		);

		// Given: parent bounty status is `Funded`
		let s = create_funded_parent_bounty();

		// When: sender is parent curator
		assert_ok!(Bounties::close_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			None,
		));

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, None).expect("no payment attempt");
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap().status,
			BountyStatus::RefundAttempted {
				curator: Some(s.curator),
				payment_status: PaymentState::Attempted { id: payment_id }
			}
		);

		// Given: parent bounty status is `UnassignedCurator`
		let s = create_parent_bounty_with_unassigned_curator();

		// When: sender is `RejectOrigin`
		assert_ok!(Bounties::close_bounty(RuntimeOrigin::root(), s.parent_bounty_id, None,));

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, None).expect("no payment attempt");
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap().status,
			BountyStatus::RefundAttempted {
				curator: None,
				payment_status: PaymentState::Attempted { id: payment_id }
			}
		);

		// Given: child-bounty status is `Funded`
		let s = create_funded_child_bounty();

		// When: sender is `RejectOrigin`
		assert_ok!(Bounties::close_bounty(
			RuntimeOrigin::root(),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id))
			.expect("no payment attempt");
		assert_eq!(
			last_event(),
			BountiesEvent::BountyCanceled {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id)
			}
		);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap(),
			ChildBounty {
				parent_bounty: s.parent_bounty_id,
				value: s.child_value,
				metadata: s.metadata,
				status: BountyStatus::RefundAttempted {
					curator: Some(s.child_curator),
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(
				s.parent_bounty_id,
				Some(s.child_bounty_id)
			),
			None
		);

		// Given: child-bounty status is `Funded`
		let s = create_funded_child_bounty();

		// When: sender is curator
		assert_ok!(Bounties::close_bounty(
			RuntimeOrigin::signed(s.child_curator),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id))
			.expect("no payment attempt");
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap()
				.status,
			BountyStatus::RefundAttempted {
				curator: Some(s.child_curator),
				payment_status: PaymentState::Attempted { id: payment_id }
			}
		);

		// Given: child-bounty status is `Funded`
		let s = create_funded_child_bounty();

		// When: sender is parent curator
		assert_ok!(Bounties::close_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id))
			.expect("no payment attempt");
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap()
				.status,
			BountyStatus::RefundAttempted {
				curator: Some(s.child_curator),
				payment_status: PaymentState::Attempted { id: payment_id }
			}
		);

		// Given: child-bounty status is `UnassignedCurator`
		let s = create_child_bounty_with_unassigned_curator();

		// When: sender is `RejectOrigin`
		assert_ok!(Bounties::close_bounty(
			RuntimeOrigin::root(),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id))
			.expect("no payment attempt");
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap()
				.status,
			BountyStatus::RefundAttempted {
				curator: None,
				payment_status: PaymentState::Attempted { id: payment_id }
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
			Bounties::close_bounty(RuntimeOrigin::root(), s.parent_bounty_id, None),
			Error::<Test>::UnexpectedStatus
		);

		// Given
		let s = create_parent_bounty_with_unassigned_curator();

		// When/Then
		assert_noop!(
			Bounties::close_bounty(RuntimeOrigin::signed(s.curator), s.parent_bounty_id, None),
			BadOrigin
		);

		// Given
		let s = create_active_parent_bounty();

		// When/Then
		assert_noop!(
			Bounties::close_bounty(RuntimeOrigin::none(), s.parent_bounty_id, None),
			BadOrigin
		);

		// When/Then
		assert_noop!(
			Bounties::close_bounty(RuntimeOrigin::signed(1), s.parent_bounty_id, None),
			BadOrigin
		);

		// When/Then
		assert_noop!(
			Bounties::close_bounty(RuntimeOrigin::root(), 3, None),
			Error::<Test>::InvalidIndex
		);

		// Given
		assert_ok!(Bounties::fund_child_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			s.child_value,
			Some(s.child_curator),
			s.metadata
		));

		// When/Then
		assert_noop!(
			Bounties::close_bounty(RuntimeOrigin::root(), s.parent_bounty_id, None),
			Error::<Test>::HasActiveChildBounty
		);

		// Given
		let s = create_active_child_bounty();

		// When/Then
		assert_noop!(
			Bounties::close_bounty(RuntimeOrigin::root(), s.parent_bounty_id, Some(2)),
			Error::<Test>::InvalidIndex
		);

		// When/Then
		assert_noop!(
			Bounties::close_bounty(
				RuntimeOrigin::signed(1),
				s.parent_bounty_id,
				Some(s.child_bounty_id)
			),
			BadOrigin
		);

		// When/Then
		assert_noop!(
			Bounties::close_bounty(
				RuntimeOrigin::none(),
				s.parent_bounty_id,
				Some(s.child_bounty_id)
			),
			BadOrigin
		);

		// Given
		let s = create_child_bounty_with_unassigned_curator();

		// When/Then
		assert_noop!(
			Bounties::close_bounty(
				RuntimeOrigin::signed(s.child_curator),
				s.parent_bounty_id,
				Some(s.child_bounty_id)
			),
			BadOrigin
		);
	})
}

#[test]
fn close_parent_with_child_bounty() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let s = create_active_child_bounty();

		// When/Then: close parent bounty fails
		assert_noop!(
			Bounties::close_bounty(RuntimeOrigin::root(), s.parent_bounty_id, None),
			Error::<Test>::HasActiveChildBounty
		);

		// Given: close child bounty
		assert_ok!(Bounties::close_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));
		let parent_bounty_account =
			Bounties::bounty_account(s.parent_bounty_id, s.asset_kind).expect("conversion failed");
		approve_payment(
			parent_bounty_account,
			s.parent_bounty_id,
			Some(s.child_bounty_id),
			s.asset_kind,
			s.value + s.child_value, // parent bounty value + child bounty value
		);
		assert_eq!(pallet_bounties::ChildBountiesPerParent::<Test>::get(s.parent_bounty_id), 0);
		assert_eq!(
			pallet_bounties::TotalChildBountiesPerParent::<Test>::get(s.parent_bounty_id),
			1
		);

		// When
		assert_ok!(Bounties::close_bounty(RuntimeOrigin::root(), s.parent_bounty_id, None));
		let funding_source_account =
			Bounties::funding_source_account(s.asset_kind).expect("conversion failed");
		approve_payment(funding_source_account, s.parent_bounty_id, None, s.asset_kind, s.value);

		// Then
		assert_eq!(
			pallet_bounties::TotalChildBountiesPerParent::<Test>::get(s.parent_bounty_id),
			0
		);
	});
}

#[test]
fn fund_and_award_child_bounty_without_curator_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let s = create_active_parent_bounty();

		// When
		assert_ok!(Bounties::fund_child_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			s.child_value,
			None,
			s.metadata
		));
		let child_bounty_id = 0;
		let child_bounty_account =
			Bounties::child_bounty_account(s.parent_bounty_id, child_bounty_id, s.asset_kind)
				.expect("conversion failed");
		approve_payment(
			child_bounty_account,
			s.parent_bounty_id,
			Some(child_bounty_id),
			s.asset_kind,
			s.child_value,
		);
		assert_ok!(Bounties::award_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			Some(child_bounty_id),
			s.child_beneficiary
		));
		approve_payment(
			s.child_beneficiary,
			s.parent_bounty_id,
			Some(child_bounty_id),
			s.asset_kind,
			s.child_value,
		);

		// Then
		expect_events(vec![BountiesEvent::BountyPayoutProcessed {
			index: s.parent_bounty_id,
			child_index: Some(s.child_bounty_id),
			asset_kind: s.asset_kind,
			value: s.child_value,
			beneficiary: s.child_beneficiary,
		}]);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::iter_prefix(s.parent_bounty_id).count(),
			0
		);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id),
			None
		);
		assert_eq!(pallet_bounties::ChildBountiesPerParent::<Test>::get(s.parent_bounty_id), 0);
		assert_eq!(
			pallet_bounties::TotalChildBountiesPerParent::<Test>::get(s.parent_bounty_id),
			1
		);
		assert_eq!(
			pallet_bounties::ChildBountiesValuePerParent::<Test>::get(s.parent_bounty_id),
			s.child_value
		);
		assert_eq!(
			Balances::free_balance(s.child_curator),
			Balances::minimum_balance() + s.child_curator_deposit
		); // initial
	})
}

#[test]
fn integrity_test() {
	ExtBuilder::default().build_and_execute(|| {
		Bounties::integrity_test();
	});
}
