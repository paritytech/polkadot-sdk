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

//! Multi-asset bounties pallet tests.

#![cfg(test)]

use super::{Event as BountiesEvent, *};
use crate as pallet_bounties;
use crate::mock::{Bounties, *};

use frame_support::{
	assert_noop, assert_ok,
	traits::{fungible::Mutate, Currency},
};
use sp_runtime::{
	traits::{BadOrigin, Convert},
	TokenError,
};

/// Curator-deposit amount calculator used in the mock (50% of value, clamped to [3, 1000]).
type CuratorDepositCalc = crate::CuratorDepositAmount<
	mock::CuratorDepositMultiplier,
	mock::CuratorDepositMin,
	mock::CuratorDepositMax,
	u64,
>;

/// Convenience: build the basket `BoundedVec` from a list of `(asset_kind, value)` pairs.
fn basket(assets: Vec<(u32, u64)>) -> BoundedVec<(u32, u64), <Test as Config>::MaxAssetsPerBounty> {
	BoundedVec::try_from(assets).unwrap()
}

/// Read a single parent bounty asset row.
fn bounty_asset(index: BountyIndex, asset_kind: u32) -> Option<AssetEntryOf<Test, ()>> {
	pallet_bounties::BountyAssets::<Test>::get(index, asset_kind)
}

/// Read the lifecycle status of a parent bounty.
fn status(index: BountyIndex) -> BountyStatus<u128, u128> {
	pallet_bounties::Bounties::<Test>::get(index).unwrap().status
}

/// Read the lifecycle status of a child bounty.
fn child_status(parent: BountyIndex, child: BountyIndex) -> BountyStatus<u128, u128> {
	pallet_bounties::ChildBounties::<Test>::get(parent, child).unwrap().status
}

// -----------------------------------------------------------------------------
// fund_bounty
// -----------------------------------------------------------------------------

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
			BoundedVec::try_from(vec![(asset_kind, value)]).unwrap(),
			curator,
			metadata
		));

		// Then
		let index = 0;
		let bounty_account = Bounties::bounty_account(index, asset_kind).expect("conversion failed");
		let payment_id = get_payment_id(index, None, asset_kind).expect("no payment attempt");
		// The funding payment landed in the bounty account for this asset.
		assert_eq!(paid(bounty_account, asset_kind), value);
		// The per-asset row records the committed value and the in-flight payment.
		assert_eq!(
			bounty_asset(index, asset_kind).unwrap(),
			AssetEntry { value, payment_state: PaymentState::Attempted { id: payment_id } }
		);
		// The bounty is in `Funding` with the proposed curator.
		assert_eq!(status(index), BountyStatus::Funding { curator });
		assert_eq!(pallet_bounties::BountyCount::<Test>::get(), 1);
		assert!(Preimage::is_requested(&metadata));
		expect_events(vec![
			BountiesEvent::Paid { index, child_index: None, payment_id },
			BountiesEvent::BountyCreated { index },
		]);
	});
}

#[test]
fn fund_bounty_multi_asset_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: a 3-asset basket; total native = 100 (within Root spend limit).
		let assets = vec![(1u32, 50u64), (2, 30), (3, 20)];
		let curator = 4;
		let metadata = note_preimage(1);
		let _ = Balances::mint_into(&curator, Balances::minimum_balance());

		// When
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::root(),
			basket(assets.clone()),
			curator,
			metadata
		));

		// Then: every asset has its own row in `Attempted`, with its account funded.
		let index = 0;
		let mut payment_ids = Vec::new();
		for (asset_kind, value) in &assets {
			let account = Bounties::bounty_account(index, *asset_kind).expect("conversion failed");
			let id = get_payment_id(index, None, *asset_kind).expect("no payment attempt");
			assert_eq!(paid(account, *asset_kind), *value);
			assert_eq!(
				bounty_asset(index, *asset_kind).unwrap(),
				AssetEntry { value: *value, payment_state: PaymentState::Attempted { id } }
			);
			payment_ids.push((*asset_kind, id));
		}
		assert_eq!(status(index), BountyStatus::Funding { curator });

		// When: approve only 2 of 3 -> bounty stays in `Funding`.
		approve_all_payments(index, None, &payment_ids[..2]);
		assert_eq!(status(index), BountyStatus::Funding { curator });
		// The two approved rows succeeded, the third remains attempted.
		assert!(matches!(
			bounty_asset(index, 1).unwrap().payment_state,
			PaymentState::Succeeded
		));
		assert!(matches!(
			bounty_asset(index, 3).unwrap().payment_state,
			PaymentState::Attempted { .. }
		));

		// When: approve the last one -> all rows succeeded -> `Funded`.
		approve_all_payments(index, None, &payment_ids[2..]);
		assert_eq!(status(index), BountyStatus::Funded { curator });
	});
}

#[test]
fn fund_bounty_rejects_empty_basket() {
	ExtBuilder::default().build_and_execute(|| {
		let curator = 4;
		let metadata = note_preimage(1);
		assert_noop!(
			Bounties::fund_bounty(RuntimeOrigin::root(), basket(vec![]), curator, metadata),
			Error::<Test>::EmptyBasket
		);
	});
}

#[test]
fn fund_bounty_rejects_duplicate_asset() {
	ExtBuilder::default().build_and_execute(|| {
		let curator = 4;
		let metadata = note_preimage(1);
		assert_noop!(
			Bounties::fund_bounty(
				RuntimeOrigin::root(),
				basket(vec![(1, 10), (1, 5)]),
				curator,
				metadata
			),
			Error::<Test>::DuplicateAsset
		);
	});
}

#[test]
fn fund_bounty_respects_spend_origin() {
	ExtBuilder::default().build_and_execute(|| {
		let curator = 4;
		let metadata = note_preimage(1);
		// Signed origin 11 may spend at most 10 native; basket sums to 6 + 5 = 11 native.
		assert_noop!(
			Bounties::fund_bounty(
				RuntimeOrigin::signed(11),
				basket(vec![(1, 6), (2, 5)]),
				curator,
				metadata
			),
			Error::<Test>::InsufficientPermission
		);

		// A basket summing exactly to the limit (10) succeeds.
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::signed(11),
			basket(vec![(1, 6), (2, 4)]),
			curator,
			metadata
		));
		assert_eq!(status(0), BountyStatus::Funding { curator });
	});
}

#[test]
fn fund_bounty_partial_funding_failure_then_retry() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: a 2-asset bounty.
		let curator = 4;
		let metadata = note_preimage(1);
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::root(),
			basket(vec![(1, 30), (2, 20)]),
			curator,
			metadata
		));
		let index = 0;
		let acc_a = Bounties::bounty_account(index, 1).expect("conversion failed");
		let acc_b = Bounties::bounty_account(index, 2).expect("conversion failed");

		// When: asset A succeeds, asset B fails.
		let id_a = get_payment_id(index, None, 1).unwrap();
		let id_b = get_payment_id(index, None, 2).unwrap();
		set_status(id_a, PaymentStatus::Success);
		set_status(id_b, PaymentStatus::Failure);
		unpay(acc_b, 2, 20);
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), index, None));

		// Then: still `Funding`; A succeeded, B failed.
		assert_eq!(status(index), BountyStatus::Funding { curator });
		assert!(matches!(bounty_asset(index, 1).unwrap().payment_state, PaymentState::Succeeded));
		assert!(matches!(bounty_asset(index, 2).unwrap().payment_state, PaymentState::Failed));

		// When: retry re-attempts only the failed row B.
		assert_ok!(Bounties::retry_payment(RuntimeOrigin::signed(1), index, None));

		// Then: A untouched (still Succeeded), B re-attempted and re-paid.
		assert!(matches!(bounty_asset(index, 1).unwrap().payment_state, PaymentState::Succeeded));
		let id_b2 = get_payment_id(index, None, 2).expect("B re-attempted");
		assert_eq!(paid(acc_b, 2), 20);
		assert_eq!(paid(acc_a, 1), 30); // A not paid again

		// When: approve B -> all succeeded -> `Funded`.
		set_status(id_b2, PaymentStatus::Success);
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), index, None));
		assert_eq!(status(index), BountyStatus::Funded { curator });
	});
}

// -----------------------------------------------------------------------------
// fund_child_bounty
// -----------------------------------------------------------------------------

/// Create a funded+active parent bounty holding assets `[(1,50),(2,40)]` with curator 4.
fn active_multi_asset_parent() -> (BountyIndex, u128, <Test as frame_system::Config>::Hash) {
	let curator = 4;
	let metadata = note_preimage(1);
	// Curator deposit covers total native value (90 -> 50% = 45).
	let total = 50 + 40;
	let _ = Balances::mint_into(&curator, Balances::minimum_balance() + CuratorDepositCalc::convert(total));
	assert_ok!(Bounties::fund_bounty(
		RuntimeOrigin::root(),
		basket(vec![(1, 50), (2, 40)]),
		curator,
		metadata
	));
	let index = 0;
	let id1 = get_payment_id(index, None, 1).unwrap();
	let id2 = get_payment_id(index, None, 2).unwrap();
	approve_all_payments(index, None, &[(1, id1), (2, id2)]);
	assert_eq!(status(index), BountyStatus::Funded { curator });
	assert_ok!(Bounties::accept_curator(RuntimeOrigin::signed(curator), index, None));
	assert_eq!(status(index), BountyStatus::Active { curator });
	(index, curator, metadata)
}

#[test]
fn fund_child_bounty_subset_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: active parent with [(1,50),(2,40)].
		let (parent, curator, metadata) = active_multi_asset_parent();

		// When: fund a child with the subset [(1,20)].
		assert_ok!(Bounties::fund_child_bounty(
			RuntimeOrigin::signed(curator),
			parent,
			basket(vec![(1, 20)]),
			metadata,
			Some(8),
		));
		let child = pallet_bounties::TotalChildBountiesPerParent::<Test>::get(parent) - 1;

		// Then: the child has a row for asset 1 only, in `Attempted`, and `Funding`.
		let id = get_payment_id(parent, Some(child), 1).expect("no payment attempt");
		assert_eq!(
			pallet_bounties::ChildBountyAssets::<Test>::get((parent, child), 1).unwrap(),
			AssetEntry { value: 20, payment_state: PaymentState::Attempted { id } }
		);
		assert!(pallet_bounties::ChildBountyAssets::<Test>::get((parent, child), 2).is_none());
		// Allocation against the parent's asset 1 is recorded.
		assert_eq!(pallet_bounties::ChildValuePerParentAsset::<Test>::get(parent, 1), 20);
		assert_eq!(pallet_bounties::ChildValuePerParentAsset::<Test>::get(parent, 2), 0);
		assert_eq!(child_status(parent, child), BountyStatus::Funding { curator: 8 });
		assert_eq!(pallet_bounties::ChildBountiesPerParent::<Test>::get(parent), 1);
	});
}

#[test]
fn fund_child_bounty_rejects_over_allocation() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: active parent with [(1,50),(2,40)] and a child already allocating 40 of asset 1.
		let (parent, curator, metadata) = active_multi_asset_parent();
		assert_ok!(Bounties::fund_child_bounty(
			RuntimeOrigin::signed(curator),
			parent,
			basket(vec![(1, 40)]),
			metadata,
			Some(8),
		));
		assert_eq!(pallet_bounties::ChildValuePerParentAsset::<Test>::get(parent, 1), 40);

		// When/Then: requesting 20 more of asset 1 (only 10 remaining) -> InsufficientBountyValue.
		assert_noop!(
			Bounties::fund_child_bounty(
				RuntimeOrigin::signed(curator),
				parent,
				basket(vec![(1, 20)]),
				metadata,
				Some(8),
			),
			Error::<Test>::InsufficientBountyValue
		);
	});
}

#[test]
fn fund_child_bounty_rejects_unknown_asset() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: active parent holding only assets 1 and 2.
		let (parent, curator, metadata) = active_multi_asset_parent();

		// When/Then: a child naming asset 3 -> AssetNotInBounty.
		assert_noop!(
			Bounties::fund_child_bounty(
				RuntimeOrigin::signed(curator),
				parent,
				basket(vec![(3, 5)]),
				metadata,
				Some(8),
			),
			Error::<Test>::AssetNotInBounty
		);
	});
}

// -----------------------------------------------------------------------------
// award_bounty
// -----------------------------------------------------------------------------

#[docify::export]
#[test]
fn award_bounty_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: an active single-asset bounty.
		let s = create_active_parent_bounty();
		let beneficiary = s.beneficiary;
		// Curator's deposit is reserved while active.
		assert_eq!(Balances::reserved_balance(s.curator), s.curator_deposit);

		// When: award it.
		assert_ok!(Bounties::award_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			None,
			beneficiary,
		));

		// Then: a payout payment is in flight and the bounty is `Awarding`.
		assert_eq!(
			status(s.parent_bounty_id),
			BountyStatus::Awarding { curator: s.curator, beneficiary }
		);
		let payment_id =
			get_payment_id(s.parent_bounty_id, None, s.asset_kind).expect("no payment attempt");

		// When: approve the payout.
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then: beneficiary paid the full value, deposit returned, bounty removed.
		assert_eq!(paid(beneficiary, s.asset_kind), s.value);
		assert_eq!(Balances::reserved_balance(s.curator), 0);
		assert_eq!(
			Balances::free_balance(s.curator),
			Balances::minimum_balance() + s.curator_deposit
		);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id), None);
		assert!(bounty_asset(s.parent_bounty_id, s.asset_kind).is_none());
		expect_events(vec![BountiesEvent::BountyPayoutProcessed {
			index: s.parent_bounty_id,
			child_index: None,
			asset_kind: s.asset_kind,
			value: s.value,
			beneficiary,
		}]);
	});
}

#[test]
fn award_multi_asset_pays_every_asset() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: active 2-asset parent with no children.
		let (parent, curator, _metadata) = active_multi_asset_parent();
		let beneficiary = 5;

		// When: award.
		assert_ok!(Bounties::award_bounty(
			RuntimeOrigin::signed(curator),
			parent,
			None,
			beneficiary,
		));
		assert_eq!(status(parent), BountyStatus::Awarding { curator, beneficiary });

		// Approve both payouts in one check.
		let id1 = get_payment_id(parent, None, 1).unwrap();
		let id2 = get_payment_id(parent, None, 2).unwrap();
		approve_all_payments(parent, None, &[(1, id1), (2, id2)]);

		// Then: beneficiary paid for both assets, bounty removed.
		assert_eq!(paid(beneficiary, 1), 50);
		assert_eq!(paid(beneficiary, 2), 40);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(parent), None);
		assert!(bounty_asset(parent, 1).is_none());
		assert!(bounty_asset(parent, 2).is_none());
	});
}

#[test]
fn award_parent_subtracts_child_allocation() {
	ExtBuilder::default().build_and_execute(|| {
		// Scenario 1: a parent with an active child cannot be awarded.
		let (parent, curator, metadata) = active_multi_asset_parent();
		assert_ok!(Bounties::fund_child_bounty(
			RuntimeOrigin::signed(curator),
			parent,
			basket(vec![(1, 20)]),
			metadata,
			None, // parent curator, so it becomes Active after funding
		));
		let child = pallet_bounties::TotalChildBountiesPerParent::<Test>::get(parent) - 1;
		let child_id = get_payment_id(parent, Some(child), 1).unwrap();
		approve_all_payments(parent, Some(child), &[(1, child_id)]);
		assert_eq!(child_status(parent, child), BountyStatus::Active { curator });
		assert_eq!(pallet_bounties::ChildBountiesPerParent::<Test>::get(parent), 1);

		// While the child is active, awarding the parent fails.
		assert_noop!(
			Bounties::award_bounty(RuntimeOrigin::signed(curator), parent, None, 5),
			Error::<Test>::HasActiveChildBounty
		);

		// Scenario 2: close the child (refund its allocation back), then award the parent for the
		// full value. The over-allocation guard `ChildValuePerParentAsset` is reverted on close, so
		// the parent payout for asset 1 equals the full 50 again.
		assert_ok!(Bounties::close_bounty(
			RuntimeOrigin::signed(curator),
			parent,
			Some(child),
		));
		let refund_id = get_payment_id(parent, Some(child), 1).unwrap();
		approve_all_payments(parent, Some(child), &[(1, refund_id)]);
		assert_eq!(pallet_bounties::ChildBountiesPerParent::<Test>::get(parent), 0);
		assert_eq!(pallet_bounties::ChildValuePerParentAsset::<Test>::get(parent, 1), 0);

		// Now award the parent: full value paid for each asset.
		assert_ok!(Bounties::award_bounty(RuntimeOrigin::signed(curator), parent, None, 5));
		let id1 = get_payment_id(parent, None, 1).unwrap();
		let id2 = get_payment_id(parent, None, 2).unwrap();
		approve_all_payments(parent, None, &[(1, id1), (2, id2)]);
		// Both assets emit a payout event with the full (un-reduced) value; iteration order over
		// the asset rows is not guaranteed, so assert membership rather than order.
		let payout_events = last_events(2);
		assert!(payout_events.contains(&BountiesEvent::BountyPayoutProcessed {
			index: parent,
			child_index: None,
			asset_kind: 1,
			value: 50,
			beneficiary: 5,
		}));
		assert!(payout_events.contains(&BountiesEvent::BountyPayoutProcessed {
			index: parent,
			child_index: None,
			asset_kind: 2,
			value: 40,
			beneficiary: 5,
		}));
		// Beneficiary received the full parent value in asset 1 (the child's allocation was
		// reverted on close).
		assert_eq!(paid(5, 1), 50);
		assert_eq!(paid(5, 2), 40);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(parent), None);
	});
}

#[test]
fn award_partial_payout_failure_then_retry() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: active 2-asset parent, awarded.
		let (parent, curator, _metadata) = active_multi_asset_parent();
		let beneficiary = 5;
		assert_ok!(Bounties::award_bounty(
			RuntimeOrigin::signed(curator),
			parent,
			None,
			beneficiary,
		));

		// When: asset 1 payout succeeds, asset 2 fails.
		let id1 = get_payment_id(parent, None, 1).unwrap();
		let id2 = get_payment_id(parent, None, 2).unwrap();
		set_status(id1, PaymentStatus::Success);
		set_status(id2, PaymentStatus::Failure);
		// The transfer never happened: undo the beneficiary credit and give the bounty account its
		// 40 back, as `reject_payment` does, so the retry has funds to move.
		unpay(beneficiary, 2, 40);
		top_up(Bounties::bounty_account(parent, 2).expect("conversion failed"), 2, 40);
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), parent, None));

		// Then: bounty NOT removed; still `Awarding`.
		assert_eq!(status(parent), BountyStatus::Awarding { curator, beneficiary });
		assert!(matches!(bounty_asset(parent, 1).unwrap().payment_state, PaymentState::Succeeded));
		assert!(matches!(bounty_asset(parent, 2).unwrap().payment_state, PaymentState::Failed));

		// When: retry the failed one and approve.
		assert_ok!(Bounties::retry_payment(RuntimeOrigin::signed(1), parent, None));
		let id2b = get_payment_id(parent, None, 2).expect("re-attempted");
		set_status(id2b, PaymentStatus::Success);
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), parent, None));

		// Then: both paid, bounty removed.
		assert_eq!(paid(beneficiary, 1), 50);
		assert_eq!(paid(beneficiary, 2), 40);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(parent), None);
	});
}

// -----------------------------------------------------------------------------
// close_bounty
// -----------------------------------------------------------------------------

#[test]
fn close_bounty_refunds_every_asset() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: a funded (not active) 2-asset bounty.
		let curator = 4;
		let metadata = note_preimage(1);
		let _ = Balances::mint_into(&curator, Balances::minimum_balance());
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::root(),
			basket(vec![(1, 30), (2, 20)]),
			curator,
			metadata
		));
		let index = 0;
		let id1 = get_payment_id(index, None, 1).unwrap();
		let id2 = get_payment_id(index, None, 2).unwrap();
		approve_all_payments(index, None, &[(1, id1), (2, id2)]);
		assert_eq!(status(index), BountyStatus::Funded { curator });

		let source1 = Bounties::funding_source_account(1).expect("conversion failed");
		let source2 = Bounties::funding_source_account(2).expect("conversion failed");
		let paid_before_1 = paid(source1, 1);
		let paid_before_2 = paid(source2, 2);

		// When: close.
		assert_ok!(Bounties::close_bounty(RuntimeOrigin::root(), index, None));
		assert_eq!(status(index), BountyStatus::Closing { curator: Some(curator) });
		assert_eq!(
			last_event(),
			BountiesEvent::BountyCanceled { index, child_index: None }
		);

		// Approve both refunds.
		let rid1 = get_payment_id(index, None, 1).unwrap();
		let rid2 = get_payment_id(index, None, 2).unwrap();
		approve_all_payments(index, None, &[(1, rid1), (2, rid2)]);

		// Then: refunds went back to the funding source per asset, bounty removed.
		assert_eq!(paid(source1, 1), paid_before_1 + 30);
		assert_eq!(paid(source2, 2), paid_before_2 + 20);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(index), None);
		assert!(bounty_asset(index, 1).is_none());
		assert!(bounty_asset(index, 2).is_none());
	});
}

#[test]
fn close_parent_with_active_child_fails() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: active parent with one funded child.
		let (parent, curator, metadata) = active_multi_asset_parent();
		assert_ok!(Bounties::fund_child_bounty(
			RuntimeOrigin::signed(curator),
			parent,
			basket(vec![(1, 20)]),
			metadata,
			Some(8),
		));
		assert_eq!(pallet_bounties::ChildBountiesPerParent::<Test>::get(parent), 1);

		// When/Then: closing the parent fails.
		assert_noop!(
			Bounties::close_bounty(RuntimeOrigin::root(), parent, None),
			Error::<Test>::HasActiveChildBounty
		);
	});
}

// -----------------------------------------------------------------------------
// curator deposit
// -----------------------------------------------------------------------------

#[test]
fn accept_curator_deposit_covers_basket() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: a funded multi-asset bounty; total value = 90.
		let curator = 4;
		let metadata = note_preimage(1);
		let total = 50 + 40;
		let expected_deposit = CuratorDepositCalc::convert(total);
		let _ = Balances::mint_into(&curator, Balances::minimum_balance() + expected_deposit);
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::root(),
			basket(vec![(1, 50), (2, 40)]),
			curator,
			metadata
		));
		let index = 0;
		let id1 = get_payment_id(index, None, 1).unwrap();
		let id2 = get_payment_id(index, None, 2).unwrap();
		approve_all_payments(index, None, &[(1, id1), (2, id2)]);

		// When: accept.
		assert_ok!(Bounties::accept_curator(RuntimeOrigin::signed(curator), index, None));

		// Then: deposit reserved equals the convert of the summed basket value.
		assert_eq!(Balances::reserved_balance(curator), expected_deposit);
		assert_eq!(status(index), BountyStatus::Active { curator });
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(index, None::<BountyIndex>).unwrap(),
			consideration(total)
		);
	});
}

#[test]
fn unassign_curator_burns_or_returns_deposit() {
	ExtBuilder::default().build_and_execute(|| {
		// Case 1: RejectOrigin unassigns an active multi-asset bounty -> deposit burned.
		let (parent, curator, _metadata) = active_multi_asset_parent();
		let deposit = Balances::reserved_balance(curator);
		assert!(deposit > 0);
		let free_before = Balances::free_balance(curator);

		assert_ok!(Bounties::unassign_curator(RuntimeOrigin::root(), parent, None));
		assert_eq!(status(parent), BountyStatus::CuratorUnassigned);
		// Burned: reserved gone and NOT returned to free balance.
		assert_eq!(Balances::reserved_balance(curator), 0);
		assert_eq!(Balances::free_balance(curator), free_before);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(parent, None::<BountyIndex>),
			None
		);

		// Case 2: curator self-unassigns an active multi-asset bounty -> deposit returned.
		let (parent2, curator2, _metadata2) = active_multi_asset_parent_at(1, 6);
		let deposit2 = Balances::reserved_balance(curator2);
		assert!(deposit2 > 0);
		let free_before2 = Balances::free_balance(curator2);

		assert_ok!(Bounties::unassign_curator(RuntimeOrigin::signed(curator2), parent2, None));
		assert_eq!(status(parent2), BountyStatus::CuratorUnassigned);
		// Returned: reserved freed back into free balance.
		assert_eq!(Balances::reserved_balance(curator2), 0);
		assert_eq!(Balances::free_balance(curator2), free_before2 + deposit2);
	});
}

/// Variant of [`active_multi_asset_parent`] that uses a distinct preimage and curator so two can be
/// created in the same test without colliding.
fn active_multi_asset_parent_at(
	preimage_seed: u128,
	curator: u128,
) -> (BountyIndex, u128, <Test as frame_system::Config>::Hash) {
	let metadata = note_preimage(preimage_seed);
	let total = 50 + 40;
	let _ = Balances::mint_into(&curator, Balances::minimum_balance() + CuratorDepositCalc::convert(total));
	assert_ok!(Bounties::fund_bounty(
		RuntimeOrigin::root(),
		basket(vec![(1, 50), (2, 40)]),
		curator,
		metadata
	));
	let index = pallet_bounties::BountyCount::<Test>::get() - 1;
	let id1 = get_payment_id(index, None, 1).unwrap();
	let id2 = get_payment_id(index, None, 2).unwrap();
	approve_all_payments(index, None, &[(1, id1), (2, id2)]);
	assert_ok!(Bounties::accept_curator(RuntimeOrigin::signed(curator), index, None));
	(index, curator, metadata)
}

// -----------------------------------------------------------------------------
// child bounty full lifecycle
// -----------------------------------------------------------------------------

#[test]
fn child_full_lifecycle_multi_asset() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: active parent with [(1,50),(2,40)].
		let (parent, curator, metadata) = active_multi_asset_parent();
		let child_curator = 8;
		let child_beneficiary = 9;
		// Fund the child curator to cover its deposit (child total = 20 + 10 = 30).
		let child_total = 20 + 10;
		let _ = Balances::mint_into(
			&child_curator,
			Balances::minimum_balance() + CuratorDepositCalc::convert(child_total),
		);

		// When: fund a child with BOTH assets.
		assert_ok!(Bounties::fund_child_bounty(
			RuntimeOrigin::signed(curator),
			parent,
			basket(vec![(1, 20), (2, 10)]),
			metadata,
			Some(child_curator),
		));
		let child = pallet_bounties::TotalChildBountiesPerParent::<Test>::get(parent) - 1;
		assert_eq!(pallet_bounties::ChildBountiesPerParent::<Test>::get(parent), 1);
		assert_eq!(pallet_bounties::ChildValuePerParentAsset::<Test>::get(parent, 1), 20);
		assert_eq!(pallet_bounties::ChildValuePerParentAsset::<Test>::get(parent, 2), 10);

		// Both child accounts funded.
		let cacc1 = Bounties::child_bounty_account(parent, child, 1).expect("conversion failed");
		let cacc2 = Bounties::child_bounty_account(parent, child, 2).expect("conversion failed");
		assert_eq!(paid(cacc1, 1), 20);
		assert_eq!(paid(cacc2, 2), 10);

		// Approve both funding payments -> child becomes Funded.
		let cid1 = get_payment_id(parent, Some(child), 1).unwrap();
		let cid2 = get_payment_id(parent, Some(child), 2).unwrap();
		approve_all_payments(parent, Some(child), &[(1, cid1), (2, cid2)]);
		assert_eq!(child_status(parent, child), BountyStatus::Funded { curator: child_curator });

		// Accept curator -> Active; deposit reserved.
		assert_ok!(Bounties::accept_curator(
			RuntimeOrigin::signed(child_curator),
			parent,
			Some(child),
		));
		assert_eq!(child_status(parent, child), BountyStatus::Active { curator: child_curator });
		assert_eq!(
			Balances::reserved_balance(child_curator),
			CuratorDepositCalc::convert(child_total)
		);

		// Award the child to a beneficiary.
		assert_ok!(Bounties::award_bounty(
			RuntimeOrigin::signed(child_curator),
			parent,
			Some(child),
			child_beneficiary,
		));
		assert_eq!(
			child_status(parent, child),
			BountyStatus::Awarding { curator: child_curator, beneficiary: child_beneficiary }
		);

		// Approve both payouts -> child removed.
		let pid1 = get_payment_id(parent, Some(child), 1).unwrap();
		let pid2 = get_payment_id(parent, Some(child), 2).unwrap();
		approve_all_payments(parent, Some(child), &[(1, pid1), (2, pid2)]);

		// Then: beneficiary paid for both assets, child removed, count decremented, deposit
		// returned.
		assert_eq!(paid(child_beneficiary, 1), 20);
		assert_eq!(paid(child_beneficiary, 2), 10);
		assert_eq!(pallet_bounties::ChildBounties::<Test>::get(parent, child), None);
		assert_eq!(pallet_bounties::ChildBountiesPerParent::<Test>::get(parent), 0);
		// Total count is NOT decremented (it tracks all-time children).
		assert_eq!(pallet_bounties::TotalChildBountiesPerParent::<Test>::get(parent), 1);
		assert_eq!(Balances::reserved_balance(child_curator), 0);
		// The parent's child allocation persists (only reverted on child close, not payout).
		assert_eq!(pallet_bounties::ChildValuePerParentAsset::<Test>::get(parent, 1), 20);
	});
}

#[test]
fn retry_payment_only_touches_failed_rows() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: a 3-asset bounty where the middle asset's funding fails.
		let curator = 4;
		let metadata = note_preimage(1);
		let _ = Balances::mint_into(&curator, Balances::minimum_balance());
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::root(),
			basket(vec![(1, 10), (2, 10), (3, 10)]),
			curator,
			metadata
		));
		let index = 0;
		let id1 = get_payment_id(index, None, 1).unwrap();
		let id2 = get_payment_id(index, None, 2).unwrap();
		let id3 = get_payment_id(index, None, 3).unwrap();
		set_status(id1, PaymentStatus::Success);
		set_status(id2, PaymentStatus::Failure);
		set_status(id3, PaymentStatus::Success);
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), index, None));
		assert_eq!(status(index), BountyStatus::Funding { curator });

		// When: retry.
		assert_ok!(Bounties::retry_payment(RuntimeOrigin::signed(1), index, None));

		// Then: only asset 2 got a new payment id; assets 1 and 3 keep their Succeeded state.
		assert!(matches!(bounty_asset(index, 1).unwrap().payment_state, PaymentState::Succeeded));
		assert!(matches!(bounty_asset(index, 3).unwrap().payment_state, PaymentState::Succeeded));
		let new_id2 = get_payment_id(index, None, 2).expect("re-attempted");
		assert_ne!(new_id2, id2);

		// Approve the retried asset -> Funded.
		set_status(new_id2, PaymentStatus::Success);
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), index, None));
		assert_eq!(status(index), BountyStatus::Funded { curator });
	});
}

// -----------------------------------------------------------------------------
// close_bounty: parent refund is net of paid-out child value (ported from #13258 / #13409)
// -----------------------------------------------------------------------------

#[test]
fn close_parent_refunds_only_remaining_value_after_child_payout() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: a parent (50) whose child (10) has been awarded; settle the child payout.
		let s = create_awarded_child_bounty();
		let child_payment_id =
			get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), s.asset_kind)
				.expect("no child payout attempt");
		set_status(child_payment_id, PaymentStatus::Success);
		assert_ok!(Bounties::check_status(
			RuntimeOrigin::signed(1),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));
		assert_eq!(
			pallet_bounties::ChildValuePerParentAsset::<Test>::get(s.parent_bounty_id, s.asset_kind),
			s.child_value
		);
		let funding_source =
			Bounties::funding_source_account(s.asset_kind).expect("conversion failed");
		let funding_source_balance = paid(funding_source, s.asset_kind);

		// When: the parent is closed.
		assert_ok!(Bounties::close_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			None
		));

		// Then: the refund requested is the parent value net of the paid-out child value.
		assert_eq!(
			paid(funding_source, s.asset_kind),
			funding_source_balance + s.value - s.child_value
		);
	});
}

/// Regression test for closing a parent bounty after one of its child-bounties was paid out.
///
/// The paid-out child value left the parent account for good, but `close_bounty` used to request
/// a refund of the full stored `value`. A paymaster that performs a real transfer (`LocalPay`)
/// rejects that, so the parent bounty could never be closed. Without the `calculate_payout`
/// adjustment in `do_process_refund_payment` this test fails with `RefundError`.
#[test]
fn close_parent_bounty_after_child_payout_refunds_remaining_value() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: parent bounty (50) whose only child-bounty (10) has been paid out
		let s = create_awarded_child_bounty();
		approve_payment(
			s.child_beneficiary,
			s.parent_bounty_id,
			Some(s.child_bounty_id),
			s.asset_kind,
			s.child_value,
		);
		assert_eq!(pallet_bounties::ChildBountiesPerParent::<Test>::get(s.parent_bounty_id), 0);
		assert_eq!(
			pallet_bounties::ChildValuePerParentAsset::<Test>::get(s.parent_bounty_id, s.asset_kind),
			s.child_value
		);
		let remaining = s.value - s.child_value;

		// Parent account holds only the remaining 40; a larger transfer must fail
		let parent_bounty_account =
			Bounties::bounty_account(s.parent_bounty_id, s.asset_kind).expect("conversion failed");
		let funding_source_account =
			Bounties::funding_source_account(s.asset_kind).expect("conversion failed");
		assert_eq!(paid(parent_bounty_account, s.asset_kind), remaining);

		// When
		assert_ok!(Bounties::close_bounty(RuntimeOrigin::root(), s.parent_bounty_id, None));

		// Then: only the remaining value is refunded
		assert_eq!(paid(funding_source_account, s.asset_kind), remaining);
		assert_eq!(paid(parent_bounty_account, s.asset_kind), 0);
		approve_payment(funding_source_account, s.parent_bounty_id, None, s.asset_kind, remaining);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id), None);
		assert_eq!(
			pallet_bounties::ChildValuePerParentAsset::<Test>::get(s.parent_bounty_id, s.asset_kind),
			0
		);
		expect_events(vec![
			BountiesEvent::BountyCanceled { index: s.parent_bounty_id, child_index: None },
			BountiesEvent::BountyRefundProcessed {
				index: s.parent_bounty_id,
				child_index: None,
				asset_kind: s.asset_kind,
			},
		]);
	});
}

// -----------------------------------------------------------------------------
// increase_value (ported from #12409 to the per-asset ledger)
// -----------------------------------------------------------------------------

#[test]
fn increase_value_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: an active parent bounty (value 50, curator deposit 25 held).
		let s = create_active_parent_bounty();
		assert_eq!(Balances::reserved_balance(s.curator), s.curator_deposit);
		let increase = 20;
		let new_value = s.value + increase; // 70

		// Give the curator enough free balance to cover the larger deposit hold.
		let _ = Balances::mint_into(&s.curator, 100);

		// When
		assert_ok!(Bounties::increase_value(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			Box::new(s.asset_kind),
			increase,
		));

		// Then: the asset row is raised and the event is emitted.
		assert_eq!(bounty_asset(s.parent_bounty_id, s.asset_kind).unwrap().value, new_value);
		expect_events(vec![BountiesEvent::BountyValueIncreased {
			index: s.parent_bounty_id,
			asset_kind: s.asset_kind,
			old_value: s.value,
			new_value,
		}]);

		// And: the curator deposit was rescaled to the new value (50% * 70 = 35).
		let expected_deposit = new_value / 2; // 50% multiplier, within [Min, Max]
		assert_eq!(Balances::reserved_balance(s.curator), expected_deposit);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(s.parent_bounty_id, None::<BountyIndex>)
				.unwrap(),
			consideration(new_value)
		);
	});
}

#[test]
fn increase_value_fails() {
	ExtBuilder::default().build_and_execute(|| {
		// When/Then: unknown bounty index.
		assert_noop!(
			Bounties::increase_value(RuntimeOrigin::signed(4), 99, Box::new(1), 10),
			Error::<Test>::InvalidIndex
		);

		// Given: a parent bounty that is not yet `Active` (`Funding`).
		let s = create_parent_bounty();

		// When/Then: cannot increase value before the bounty is active.
		assert_noop!(
			Bounties::increase_value(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				Box::new(s.asset_kind),
				10
			),
			Error::<Test>::UnexpectedStatus
		);

		// Given: an active parent bounty.
		let s = create_active_parent_bounty();

		// When/Then: unsigned origin is rejected.
		assert_noop!(
			Bounties::increase_value(
				RuntimeOrigin::none(),
				s.parent_bounty_id,
				Box::new(s.asset_kind),
				10
			),
			BadOrigin
		);

		// When/Then: only the curator may increase the value.
		assert_noop!(
			Bounties::increase_value(
				RuntimeOrigin::signed(1),
				s.parent_bounty_id,
				Box::new(s.asset_kind),
				10
			),
			Error::<Test>::RequireCurator
		);

		// When/Then: a zero increase is rejected.
		assert_noop!(
			Bounties::increase_value(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				Box::new(s.asset_kind),
				0
			),
			Error::<Test>::InvalidValue
		);

		// When/Then: an asset the bounty does not hold cannot be added through this call.
		assert_noop!(
			Bounties::increase_value(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				Box::new(s.asset_kind + 1),
				10
			),
			Error::<Test>::AssetNotInBounty
		);
	});
}

#[test]
fn increase_value_then_award_pays_new_value() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: an active parent bounty, value increased from 50 to 70.
		let s = create_active_parent_bounty();
		let _ = Balances::mint_into(&s.curator, 100);
		let increase = 20;
		let new_value = s.value + increase;
		// `increase_value` moves no funds; model the external top-up it registers.
		let parent_bounty_account =
			Bounties::bounty_account(s.parent_bounty_id, s.asset_kind).expect("conversion failed");
		top_up(parent_bounty_account, s.asset_kind, increase);
		assert_ok!(Bounties::increase_value(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			Box::new(s.asset_kind),
			increase,
		));

		// When: the bounty is awarded and the payout settles.
		assert_ok!(Bounties::award_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			None,
			s.beneficiary,
		));
		approve_payment(s.beneficiary, s.parent_bounty_id, None, s.asset_kind, new_value);

		// Then: the beneficiary is paid the NEW value, not the original.
		expect_events(vec![BountiesEvent::BountyPayoutProcessed {
			index: s.parent_bounty_id,
			child_index: None,
			asset_kind: s.asset_kind,
			value: new_value,
			beneficiary: s.beneficiary,
		}]);
		assert_eq!(paid(s.beneficiary, s.asset_kind), new_value);
	});
}

#[test]
fn increase_value_creates_child_bounty_headroom() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: an active parent bounty (value 50) with its entire value allocated to one child.
		let s = create_active_parent_bounty();
		let _ = Balances::mint_into(&s.curator, 100);
		assert_ok!(Bounties::fund_child_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			basket(vec![(s.asset_kind, s.value)]), // 50 — consumes all parent value
			s.metadata,
			None,
		));
		assert_eq!(
			pallet_bounties::ChildValuePerParentAsset::<Test>::get(s.parent_bounty_id, s.asset_kind),
			s.value
		);

		// When/Then: no headroom left, another child is rejected.
		assert_noop!(
			Bounties::fund_child_bounty(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				basket(vec![(s.asset_kind, 1)]),
				s.metadata,
				None,
			),
			Error::<Test>::InsufficientBountyValue
		);

		// Given: the parent value is increased, creating new headroom.
		let increase = 30;
		// `increase_value` moves no funds; model the external top-up it registers.
		let parent_bounty_account =
			Bounties::bounty_account(s.parent_bounty_id, s.asset_kind).expect("conversion failed");
		top_up(parent_bounty_account, s.asset_kind, increase);
		assert_ok!(Bounties::increase_value(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			Box::new(s.asset_kind),
			increase,
		));

		// When/Then: a child funded from the new headroom now succeeds, and the cumulative child
		// value equals the new parent value (per-asset invariant Σ child ≤ parent preserved).
		assert_ok!(Bounties::fund_child_bounty(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			basket(vec![(s.asset_kind, increase)]), // 30
			s.metadata,
			None,
		));
		assert_eq!(
			pallet_bounties::ChildValuePerParentAsset::<Test>::get(s.parent_bounty_id, s.asset_kind),
			s.value + increase, // 80
		);
	});
}

#[test]
fn increase_value_reverts_when_curator_cannot_fund_deposit() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: an active parent bounty. The curator's free balance is only the existential
		// deposit (their 25 deposit is already held), so they cannot fund a larger hold.
		let s = create_active_parent_bounty();
		assert_eq!(Balances::reserved_balance(s.curator), s.curator_deposit);

		// When/Then: the increase requires a larger deposit (50% * 70 = 35, i.e. +10) the curator
		// cannot afford, so the call fails.
		assert_noop!(
			Bounties::increase_value(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				Box::new(s.asset_kind),
				20
			),
			TokenError::FundsUnavailable
		);

		// And: the whole call is rolled back — the deposit was NOT orphaned by the earlier `take`,
		// and the asset row is unchanged.
		assert_eq!(bounty_asset(s.parent_bounty_id, s.asset_kind).unwrap().value, s.value);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(s.parent_bounty_id, None::<BountyIndex>)
				.unwrap(),
			consideration(s.value)
		);
		assert_eq!(Balances::reserved_balance(s.curator), s.curator_deposit);
	});
}
