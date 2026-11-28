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
	assert_err_ignore_postinfo, assert_noop, assert_ok,
	traits::{fungible::Mutate, Currency},
};
use sp_runtime::{
	traits::{BadOrigin, Convert, Dispatchable},
	TokenError,
};

type UtilityCall = pallet_utility::Call<Test>;
type BountiesCall = crate::Call<Test>;

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
		assert_ok!(Bounties::fund_bounty_with_assets(
			RuntimeOrigin::root(),
			BoundedVec::try_from(vec![(asset_kind, value)]).unwrap(),
			curator,
			metadata
		));

		// Then
		let index = 0;
		let bounty_account =
			Bounties::bounty_account(index, asset_kind).expect("conversion failed");
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
			BountiesEvent::Paid { index, child_index: None, asset_kind, payment_id },
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
		assert_ok!(Bounties::fund_bounty_with_assets(
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
		assert!(matches!(bounty_asset(index, 1).unwrap().payment_state, PaymentState::Succeeded));
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
			Bounties::fund_bounty_with_assets(
				RuntimeOrigin::root(),
				basket(vec![]),
				curator,
				metadata
			),
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
			Bounties::fund_bounty_with_assets(
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
			Bounties::fund_bounty_with_assets(
				RuntimeOrigin::signed(11),
				basket(vec![(1, 6), (2, 5)]),
				curator,
				metadata
			),
			Error::<Test>::InsufficientPermission
		);

		// A basket summing exactly to the limit (10) succeeds.
		assert_ok!(Bounties::fund_bounty_with_assets(
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
		assert_ok!(Bounties::fund_bounty_with_assets(
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
	let _ = Balances::mint_into(
		&curator,
		Balances::minimum_balance() + CuratorDepositCalc::convert(total),
	);
	assert_ok!(Bounties::fund_bounty_with_assets(
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
		assert_ok!(Bounties::fund_child_bounty_with_assets(
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
			pallet_bounties::ChildBountyAssets::<Test>::get((parent, child, 1)).unwrap(),
			AssetEntry { value: 20, payment_state: PaymentState::Attempted { id } }
		);
		assert!(pallet_bounties::ChildBountyAssets::<Test>::get((parent, child, 2)).is_none());
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
		assert_ok!(Bounties::fund_child_bounty_with_assets(
			RuntimeOrigin::signed(curator),
			parent,
			basket(vec![(1, 40)]),
			metadata,
			Some(8),
		));
		assert_eq!(pallet_bounties::ChildValuePerParentAsset::<Test>::get(parent, 1), 40);

		// When/Then: requesting 20 more of asset 1 (only 10 remaining) -> InsufficientBountyValue.
		assert_noop!(
			Bounties::fund_child_bounty_with_assets(
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
			Bounties::fund_child_bounty_with_assets(
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
		assert_ok!(Bounties::fund_child_bounty_with_assets(
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
		assert_ok!(Bounties::close_bounty(RuntimeOrigin::signed(curator), parent, Some(child),));
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
		assert_ok!(Bounties::fund_bounty_with_assets(
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
		assert_eq!(last_event(), BountiesEvent::BountyCanceled { index, child_index: None });

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
		assert_ok!(Bounties::fund_child_bounty_with_assets(
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
		assert_ok!(Bounties::fund_bounty_with_assets(
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
		assert_eq!(pallet_bounties::CuratorDeposit::<Test>::get(parent, None::<BountyIndex>), None);

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
	let _ = Balances::mint_into(
		&curator,
		Balances::minimum_balance() + CuratorDepositCalc::convert(total),
	);
	assert_ok!(Bounties::fund_bounty_with_assets(
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
		assert_ok!(Bounties::fund_child_bounty_with_assets(
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
		assert_ok!(Bounties::fund_bounty_with_assets(
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
			pallet_bounties::ChildValuePerParentAsset::<Test>::get(
				s.parent_bounty_id,
				s.asset_kind
			),
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
			pallet_bounties::ChildValuePerParentAsset::<Test>::get(
				s.parent_bounty_id,
				s.asset_kind
			),
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
			pallet_bounties::ChildValuePerParentAsset::<Test>::get(
				s.parent_bounty_id,
				s.asset_kind
			),
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
		assert_ok!(Bounties::increase_asset_value(
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
			Bounties::increase_asset_value(RuntimeOrigin::signed(4), 99, Box::new(1), 10),
			Error::<Test>::InvalidIndex
		);

		// Given: a parent bounty that is not yet `Active` (`Funding`).
		let s = create_parent_bounty();

		// When/Then: cannot increase value before the bounty is active.
		assert_noop!(
			Bounties::increase_asset_value(
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
			Bounties::increase_asset_value(
				RuntimeOrigin::none(),
				s.parent_bounty_id,
				Box::new(s.asset_kind),
				10
			),
			BadOrigin
		);

		// When/Then: only the curator may increase the value.
		assert_noop!(
			Bounties::increase_asset_value(
				RuntimeOrigin::signed(1),
				s.parent_bounty_id,
				Box::new(s.asset_kind),
				10
			),
			Error::<Test>::RequireCurator
		);

		// When/Then: a zero increase is rejected.
		assert_noop!(
			Bounties::increase_asset_value(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				Box::new(s.asset_kind),
				0
			),
			Error::<Test>::InvalidValue
		);

		// When/Then: an asset the bounty does not hold cannot be added through this call.
		assert_noop!(
			Bounties::increase_asset_value(
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
		assert_ok!(Bounties::increase_asset_value(
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
		assert_ok!(Bounties::fund_child_bounty_with_assets(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			basket(vec![(s.asset_kind, s.value)]), // 50 — consumes all parent value
			s.metadata,
			None,
		));
		assert_eq!(
			pallet_bounties::ChildValuePerParentAsset::<Test>::get(
				s.parent_bounty_id,
				s.asset_kind
			),
			s.value
		);

		// When/Then: no headroom left, another child is rejected.
		assert_noop!(
			Bounties::fund_child_bounty_with_assets(
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
		assert_ok!(Bounties::increase_asset_value(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			Box::new(s.asset_kind),
			increase,
		));

		// When/Then: a child funded from the new headroom now succeeds, and the cumulative child
		// value equals the new parent value (per-asset invariant Σ child ≤ parent preserved).
		assert_ok!(Bounties::fund_child_bounty_with_assets(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			basket(vec![(s.asset_kind, increase)]), // 30
			s.metadata,
			None,
		));
		assert_eq!(
			pallet_bounties::ChildValuePerParentAsset::<Test>::get(
				s.parent_bounty_id,
				s.asset_kind
			),
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
			Bounties::increase_asset_value(
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

// -----------------------------------------------------------------------------
// Leg isolation, zero legs and closing from `Funding`
// -----------------------------------------------------------------------------

/// Empties the bounty account in `asset_kind`, so the mock `Paymaster` rejects the next payment of
/// that asset inside the call, as `LocalPay` does for a frozen asset or a recipient that cannot
/// hold it. Returns what the account held.
fn drain(index: BountyIndex, asset_kind: u32) -> u64 {
	let account = Bounties::bounty_account(index, asset_kind).expect("conversion failed");
	let held = paid(account, asset_kind);
	unpay(account, asset_kind, held);
	held
}

fn has_event(event: BountiesEvent<Test>) -> bool {
	System::events()
		.iter()
		.any(|r| r.event == RuntimeEvent::Bounties(event.clone()))
}

#[test]
fn award_isolates_a_rejected_leg() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: an active 2-asset parent whose account cannot pay asset 2.
		let (parent, curator, _metadata) = active_multi_asset_parent();
		let beneficiary = 5;
		let held = drain(parent, 2);
		System::reset_events();

		// When: award.
		assert_ok!(Bounties::award_bounty(
			RuntimeOrigin::signed(curator),
			parent,
			None,
			beneficiary
		));

		// Then: asset 1 is paid, asset 2 is marked failed, and the bounty is awarding.
		assert_eq!(status(parent), BountyStatus::Awarding { curator, beneficiary });
		assert_eq!(paid(beneficiary, 1), 50);
		assert_eq!(paid(beneficiary, 2), 0);
		assert!(matches!(
			bounty_asset(parent, 1).unwrap().payment_state,
			PaymentState::Attempted { .. }
		));
		assert_eq!(bounty_asset(parent, 2).unwrap().payment_state, PaymentState::Failed);
		assert!(has_event(BountiesEvent::PaymentFailed {
			index: parent,
			child_index: None,
			asset_kind: 2,
			payment_id: None,
		}));

		// When: asset 2 can be paid again and the failed leg is retried.
		top_up(Bounties::bounty_account(parent, 2).unwrap(), 2, held);
		assert_ok!(Bounties::retry_payment(RuntimeOrigin::signed(1), parent, None));
		let id1 = get_payment_id(parent, None, 1).unwrap();
		let id2 = get_payment_id(parent, None, 2).unwrap();
		approve_all_payments(parent, None, &[(1, id1), (2, id2)]);

		// Then: both assets are paid and the bounty is removed.
		assert_eq!(paid(beneficiary, 2), 40);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(parent), None);
	});
}

#[test]
fn award_reverts_when_every_leg_is_rejected() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: an active 2-asset parent whose account cannot pay either asset.
		let (parent, curator, _metadata) = active_multi_asset_parent();
		drain(parent, 1);
		drain(parent, 2);

		// Then: nothing could be paid, so the call reverts and the bounty stays `Active`, and the
		// curator can award it to another beneficiary, as with a single-asset bounty.
		assert_noop!(
			Bounties::award_bounty(RuntimeOrigin::signed(curator), parent, None, 5),
			Error::<Test>::PayoutError
		);
	});
}

#[test]
fn close_isolates_a_rejected_leg_and_retry_reverts_if_it_is_rejected_again() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: an active 2-asset parent whose account cannot pay asset 2.
		let (parent, curator, _metadata) = active_multi_asset_parent();
		let source = Bounties::funding_source_account(1).expect("conversion failed");
		let before = paid(source, 1);
		let held = drain(parent, 2);

		// When: close.
		assert_ok!(Bounties::close_bounty(RuntimeOrigin::root(), parent, None));

		// Then: asset 1 is refunded and asset 2 is marked failed.
		assert_eq!(status(parent), BountyStatus::Closing { curator: Some(curator) });
		assert_eq!(paid(source, 1), before + 50);
		assert_eq!(bounty_asset(parent, 2).unwrap().payment_state, PaymentState::Failed);

		// A retry that is rejected again reverts.
		assert_noop!(
			Bounties::retry_payment(RuntimeOrigin::signed(1), parent, None),
			Error::<Test>::RefundError
		);

		// Once asset 2 can be paid, the retry goes through and the close completes.
		top_up(Bounties::bounty_account(parent, 2).unwrap(), 2, held);
		assert_ok!(Bounties::retry_payment(RuntimeOrigin::signed(1), parent, None));
		let id1 = get_payment_id(parent, None, 1).unwrap();
		let id2 = get_payment_id(parent, None, 2).unwrap();
		approve_all_payments(parent, None, &[(1, id1), (2, id2)]);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(parent), None);
	});
}

#[test]
fn funding_rejects_a_zero_leg() {
	ExtBuilder::default().build_and_execute(|| {
		let metadata = note_preimage(1);
		assert_noop!(
			Bounties::fund_bounty_with_assets(
				RuntimeOrigin::root(),
				basket(vec![(1, 50), (2, 0)]),
				4,
				metadata
			),
			Error::<Test>::InvalidValue
		);

		let (parent, curator, metadata) = active_multi_asset_parent();
		assert_noop!(
			Bounties::fund_child_bounty_with_assets(
				RuntimeOrigin::signed(curator),
				parent,
				basket(vec![(1, 20), (2, 0)]),
				metadata,
				None,
			),
			Error::<Test>::InvalidValue
		);
	});
}

#[test]
fn award_parent_settles_a_zero_leg_without_paying() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: a child took all 40 of the parent's asset 2 and was paid out.
		let (parent, curator, metadata) = active_multi_asset_parent();
		assert_ok!(Bounties::fund_child_bounty_with_assets(
			RuntimeOrigin::signed(curator),
			parent,
			basket(vec![(2, 40)]),
			metadata,
			None,
		));
		let child = 0;
		let fund_id = get_payment_id(parent, Some(child), 2).unwrap();
		approve_all_payments(parent, Some(child), &[(2, fund_id)]);
		assert_ok!(Bounties::award_bounty(RuntimeOrigin::signed(curator), parent, Some(child), 9));
		let payout_id = get_payment_id(parent, Some(child), 2).unwrap();
		approve_all_payments(parent, Some(child), &[(2, payout_id)]);
		assert_eq!(paid(9, 2), 40);
		System::reset_events();

		// When: the parent is awarded.
		assert_ok!(Bounties::award_bounty(RuntimeOrigin::signed(curator), parent, None, 5));

		// Then: asset 2 has nothing left, so it settles at once without a payment.
		assert_eq!(bounty_asset(parent, 2).unwrap().payment_state, PaymentState::Succeeded);
		assert!(!System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Bounties(BountiesEvent::Paid { asset_kind: 2, .. })
		)));
		let id1 = get_payment_id(parent, None, 1).unwrap();
		approve_all_payments(parent, None, &[(1, id1)]);
		assert_eq!(paid(5, 1), 50);
		assert_eq!(paid(5, 2), 0);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(parent), None);
	});
}

#[test]
fn close_from_funding_refunds_only_the_assets_that_arrived() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: a 2-asset bounty whose funding payments are in flight.
		let curator = 4;
		let metadata = note_preimage(1);
		assert_ok!(Bounties::fund_bounty_with_assets(
			RuntimeOrigin::root(),
			basket(vec![(1, 30), (2, 20)]),
			curator,
			metadata
		));
		let index = 0;

		// While a funding payment is in flight, the bounty cannot be closed.
		assert_noop!(
			Bounties::close_bounty(RuntimeOrigin::root(), index, None),
			Error::<Test>::FundingInconclusive
		);

		// When: asset 1 arrives and asset 2's funding payment fails.
		let id1 = get_payment_id(index, None, 1).unwrap();
		let id2 = get_payment_id(index, None, 2).unwrap();
		set_status(id1, PaymentStatus::Success);
		let account = Bounties::bounty_account(index, 2).expect("conversion failed");
		reject_payment(account, index, None, 2, 20);
		assert!(has_event(BountiesEvent::PaymentFailed {
			index,
			child_index: None,
			asset_kind: 2,
			payment_id: Some(id2),
		}));
		assert_eq!(status(index), BountyStatus::Funding { curator });

		// Then: the bounty can be closed, and only asset 1 is refunded.
		let source = Bounties::funding_source_account(1).expect("conversion failed");
		let before = paid(source, 1);
		assert_ok!(Bounties::close_bounty(RuntimeOrigin::root(), index, None));
		assert_eq!(status(index), BountyStatus::Closing { curator: Some(curator) });
		assert_eq!(bounty_asset(index, 2).unwrap().payment_state, PaymentState::Succeeded);
		let refund_id = get_payment_id(index, None, 1).unwrap();
		approve_all_payments(index, None, &[(1, refund_id)]);
		assert_eq!(paid(source, 1), before + 30);
		assert_eq!(pallet_bounties::Bounties::<Test>::get(index), None);
	});
}

#[test]
fn close_child_from_funding_hands_the_allocation_back() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: a child funded with two assets, one of whose funding payments failed.
		let (parent, curator, metadata) = active_multi_asset_parent();
		assert_ok!(Bounties::fund_child_bounty_with_assets(
			RuntimeOrigin::signed(curator),
			parent,
			basket(vec![(1, 20), (2, 10)]),
			metadata,
			Some(8),
		));
		let child = 0;
		let id1 = get_payment_id(parent, Some(child), 1).unwrap();
		set_status(id1, PaymentStatus::Success);
		let account = Bounties::child_bounty_account(parent, child, 2).expect("conversion failed");
		reject_payment(account, parent, Some(child), 2, 10);
		assert!(matches!(child_status(parent, child), BountyStatus::Funding { .. }));

		// When: the parent curator closes the child and the refund of asset 1 settles.
		assert_ok!(Bounties::close_bounty(RuntimeOrigin::signed(curator), parent, Some(child)));
		let refund_id = get_payment_id(parent, Some(child), 1).unwrap();
		approve_all_payments(parent, Some(child), &[(1, refund_id)]);

		// Then: the child is gone and the parent can allocate both assets in full again.
		assert_eq!(pallet_bounties::ChildBounties::<Test>::get(parent, child), None);
		assert_eq!(pallet_bounties::ChildValuePerParentAsset::<Test>::get(parent, 1), 0);
		assert_eq!(pallet_bounties::ChildValuePerParentAsset::<Test>::get(parent, 2), 0);
		assert_eq!(Bounties::bounty_account(parent, 1).map(|a| paid(a, 1)), Ok(50));
		assert_eq!(Bounties::bounty_account(parent, 2).map(|a| paid(a, 2)), Ok(40));
	});
}

// -----------------------------------------------------------------------------
// try_state
// -----------------------------------------------------------------------------

#[test]
fn try_state_catches_broken_baskets() {
	ExtBuilder::default().build().execute_with(|| {
		let (parent, curator, metadata) = active_multi_asset_parent();
		assert_ok!(Bounties::do_try_state());
		let row1 = bounty_asset(parent, 1).unwrap();

		// I4: a bounty without asset rows.
		let rows = pallet_bounties::BountyAssets::<Test>::iter_prefix(parent).collect::<Vec<_>>();
		let _ = pallet_bounties::BountyAssets::<Test>::clear_prefix(parent, u32::MAX, None);
		assert!(Bounties::do_try_state().is_err());
		for (asset_kind, entry) in rows {
			pallet_bounties::BountyAssets::<Test>::insert(parent, asset_kind, entry);
		}
		assert_ok!(Bounties::do_try_state());

		// I4: an asset row without a bounty.
		pallet_bounties::BountyAssets::<Test>::insert(parent + 1, 1, row1.clone());
		assert!(Bounties::do_try_state().is_err());
		pallet_bounties::BountyAssets::<Test>::remove(parent + 1, 1);

		// I1: the guard exceeds the parent's value.
		pallet_bounties::ChildValuePerParentAsset::<Test>::insert(parent, 1, row1.value + 1);
		assert!(Bounties::do_try_state().is_err());
		pallet_bounties::ChildValuePerParentAsset::<Test>::remove(parent, 1);

		// I2: a guard for an asset the parent does not hold.
		pallet_bounties::ChildValuePerParentAsset::<Test>::insert(parent, 3, 1);
		assert!(Bounties::do_try_state().is_err());
		pallet_bounties::ChildValuePerParentAsset::<Test>::remove(parent, 3);

		// I3: an active bounty whose funding did not settle.
		pallet_bounties::BountyAssets::<Test>::insert(
			parent,
			1,
			AssetEntry { value: row1.value, payment_state: PaymentState::Failed },
		);
		assert!(Bounties::do_try_state().is_err());
		pallet_bounties::BountyAssets::<Test>::insert(parent, 1, row1);
		assert_ok!(Bounties::do_try_state());

		// I1: a live child-bounty above the guard.
		assert_ok!(Bounties::fund_child_bounty_with_assets(
			RuntimeOrigin::signed(curator),
			parent,
			basket(vec![(1, 20)]),
			metadata,
			None,
		));
		assert_ok!(Bounties::do_try_state());
		pallet_bounties::ChildValuePerParentAsset::<Test>::insert(parent, 1, 10);
		assert!(Bounties::do_try_state().is_err());
		pallet_bounties::ChildValuePerParentAsset::<Test>::insert(parent, 1, 20);

		// I2: a child-bounty holding an asset its parent does not hold.
		let child_row = pallet_bounties::ChildBountyAssets::<Test>::get((parent, 0, 1)).unwrap();
		pallet_bounties::ChildBountyAssets::<Test>::insert((parent, 0, 3), child_row);
		assert!(Bounties::do_try_state().is_err());
		pallet_bounties::ChildBountyAssets::<Test>::remove((parent, 0, 3));
		assert_ok!(Bounties::do_try_state());
	});
}

// -----------------------------------------------------------------------------
// Single-asset coverage carried over from the pre-basket pallet
// -----------------------------------------------------------------------------

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
				RuntimeCall::from(BountiesCall::fund_bounty_with_assets {
					assets: basket(vec![(asset_kind, value)]),
					curator,
					metadata
				}),
				RuntimeCall::from(BountiesCall::fund_bounty_with_assets {
					assets: basket(vec![(asset_kind, value)]),
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
					RuntimeCall::from(BountiesCall::fund_bounty_with_assets {
						assets: basket(vec![(asset_kind, value)]),
						curator,
						metadata
					}),
					RuntimeCall::from(BountiesCall::fund_bounty_with_assets {
						assets: basket(vec![(asset_kind, value)]),
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
		assert_ok!(Bounties1::fund_bounty_with_assets(
			RuntimeOrigin::root(),
			basket(vec![(asset_kind, value)]),
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
			Bounties::fund_bounty_with_assets(
				invalid_origin,
				basket(vec![(asset_kind, value)]),
				curator,
				metadata
			),
			BadOrigin
		);

		// When/Then
		let invalid_origin = RuntimeOrigin::signed(0);
		assert_noop!(
			Bounties::fund_bounty_with_assets(
				invalid_origin,
				basket(vec![(asset_kind, value)]),
				curator,
				metadata
			),
			BadOrigin
		);

		// When/Then
		let invalid_value = 1;
		assert_noop!(
			Bounties::fund_bounty_with_assets(
				RuntimeOrigin::root(),
				basket(vec![(asset_kind, invalid_value)]),
				curator,
				metadata
			),
			Error::<Test>::InvalidValue
		);

		// When/Then
		let invalid_value = 11;
		assert_noop!(
			Bounties::fund_bounty_with_assets(
				RuntimeOrigin::signed(10), // max spending of 10
				basket(vec![(asset_kind, invalid_value)]),
				curator,
				metadata
			),
			Error::<Test>::InsufficientPermission
		);

		// When/Then
		let invalid_metadata: <Test as frame_system::Config>::Hash = [1u8; 32].into();
		assert_noop!(
			Bounties::fund_bounty_with_assets(
				RuntimeOrigin::root(),
				basket(vec![(asset_kind, value)]),
				curator,
				invalid_metadata
			),
			Error::<Test>::PreimageNotExist
		);

		// When/Then
		SpendLimit::set(50);
		let invalid_value = 51;
		assert_noop!(
			Bounties::fund_bounty_with_assets(
				RuntimeOrigin::root(),
				basket(vec![(asset_kind, invalid_value)]),
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
		assert_ok!(Bounties::fund_child_bounty_with_assets(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			basket(vec![(1, s.child_value)]),
			s.metadata,
			Some(s.child_curator)
		));
		s.child_bounty_id =
			pallet_bounties::TotalChildBountiesPerParent::<Test>::get(s.parent_bounty_id) - 1;

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), 1)
			.expect("no payment attempt");
		let child_bounty_account =
			Bounties::child_bounty_account(s.parent_bounty_id, s.child_bounty_id, s.asset_kind)
				.expect("conversion failed");
		assert_eq!(paid(child_bounty_account, s.asset_kind), s.child_value);
		expect_events(vec![
			BountiesEvent::Paid {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id),
				asset_kind: 1,
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
				metadata: s.metadata,
				status: BountyStatus::Funding { curator: s.child_curator }
			}
		);
		assert_eq!(
			pallet_bounties::TotalChildBountiesPerParent::<Test>::get(s.parent_bounty_id),
			1
		);
		assert_eq!(pallet_bounties::ChildBountiesPerParent::<Test>::get(s.parent_bounty_id), 1);
		assert_eq!(
			pallet_bounties::ChildValuePerParentAsset::<Test>::get(s.parent_bounty_id, 1),
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
		assert_ok!(Bounties::fund_child_bounty_with_assets(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			basket(vec![(1, s.child_value)]),
			s.metadata,
			None
		));
		s.child_bounty_id =
			pallet_bounties::TotalChildBountiesPerParent::<Test>::get(s.parent_bounty_id) - 1;

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), 1)
			.expect("no payment attempt");
		expect_events(vec![
			BountiesEvent::Paid {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id),
				asset_kind: 1,
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
				metadata: s.metadata,
				status: BountyStatus::Funding { curator: s.curator }
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
			Bounties::fund_child_bounty_with_assets(
				invalid_origin,
				s.parent_bounty_id,
				basket(vec![(1, s.child_value)]),
				s.metadata,
				Some(s.child_curator)
			),
			BadOrigin
		);

		// When/Then
		let invalid_parent_index = 2;
		assert_noop!(
			Bounties::fund_child_bounty_with_assets(
				RuntimeOrigin::signed(s.curator),
				invalid_parent_index,
				basket(vec![(1, s.child_value)]),
				s.metadata,
				None
			),
			Error::<Test>::InvalidIndex
		);

		// When/Then
		let invalid_value = 0;
		assert_noop!(
			Bounties::fund_child_bounty_with_assets(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				basket(vec![(1, invalid_value)]),
				s.metadata,
				None
			),
			Error::<Test>::InvalidValue
		);

		// When/Then
		let invalid_metadata: <Test as frame_system::Config>::Hash = [1u8; 32].into();
		assert_noop!(
			Bounties::fund_child_bounty_with_assets(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				basket(vec![(1, s.child_value)]),
				invalid_metadata,
				None
			),
			Error::<Test>::PreimageNotExist
		);

		// When/Then
		let invalid_origin = RuntimeOrigin::signed(1);
		assert_noop!(
			Bounties::fund_child_bounty_with_assets(
				invalid_origin,
				s.parent_bounty_id,
				basket(vec![(1, s.child_value)]),
				s.metadata,
				None
			),
			Error::<Test>::RequireCurator
		);

		// When/Then
		let invalid_value = s.value + 1;
		assert_noop!(
			Bounties::fund_child_bounty_with_assets(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				basket(vec![(1, invalid_value)]),
				s.metadata,
				None
			),
			Error::<Test>::InsufficientBountyValue
		);

		// Given
		MaxActiveChildBountyCount::set(1);
		assert_ok!(Bounties::fund_child_bounty_with_assets(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			basket(vec![(1, s.child_value)]),
			s.metadata,
			None
		));

		// When/Then
		assert_noop!(
			Bounties::fund_child_bounty_with_assets(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				basket(vec![(1, s.child_value)]),
				s.metadata,
				None
			),
			Error::<Test>::TooManyChildBounties
		);

		// Given
		let s = create_awarded_parent_bounty();

		// When/Then
		assert_noop!(
			Bounties::fund_child_bounty_with_assets(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				basket(vec![(1, s.child_value)]),
				s.metadata,
				None
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
		let payment_id = get_payment_id(s.parent_bounty_id, None, 1).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Failure);

		// When
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::PaymentFailed {
				index: s.parent_bounty_id,
				child_index: None,
				asset_kind: 1,
				payment_id: Some(payment_id)
			}
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap(),
			Bounty { metadata: s.metadata, status: BountyStatus::Funding { curator: s.curator } }
		);
		assert_eq!(
			pallet_bounties::CuratorDeposit::<Test>::get(s.parent_bounty_id, None::<BountyIndex>),
			None
		);

		// Given: parent bounty status is `FundingAttempted` and payment succeeds
		let s = create_parent_bounty();
		let payment_id = get_payment_id(s.parent_bounty_id, None, 1).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);

		// When
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::BountyFundingProcessed {
				index: s.parent_bounty_id,
				child_index: None,
				asset_kind: 1
			}
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap().status,
			BountyStatus::Funded { curator: s.curator }
		);

		// Given: parent bounty status is `RefundAttempted` and payment fails
		let s = create_canceled_parent_bounty();
		let payment_id = get_payment_id(s.parent_bounty_id, None, 1).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Failure);

		// When
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::PaymentFailed {
				index: s.parent_bounty_id,
				child_index: None,
				asset_kind: 1,
				payment_id: Some(payment_id)
			}
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap().status,
			BountyStatus::Closing { curator: Some(s.curator) }
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
		let payment_id = get_payment_id(s.parent_bounty_id, None, 1).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);

		// When
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		assert_eq!(
			last_event(),
			BountiesEvent::BountyRefundProcessed {
				index: s.parent_bounty_id,
				child_index: None,
				asset_kind: 1
			}
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
		let payment_id = get_payment_id(s.parent_bounty_id, None, 1).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Failure);

		// When
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		expect_events(vec![BountiesEvent::PaymentFailed {
			index: s.parent_bounty_id,
			child_index: None,
			asset_kind: 1,
			payment_id: Some(payment_id),
		}]);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap().status,
			BountyStatus::Awarding { curator: s.curator, beneficiary: s.beneficiary }
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
		let payment_id = get_payment_id(s.parent_bounty_id, None, 1).expect("no payment attempt");
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
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), 1)
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
				asset_kind: 1,
				payment_id: Some(payment_id)
			}
		);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap(),
			ChildBounty {
				parent_bounty: s.parent_bounty_id,
				metadata: s.metadata,
				status: BountyStatus::Funding { curator: s.child_curator }
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
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), 1)
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
				asset_kind: 1
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
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), 1)
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
				asset_kind: 1,
				payment_id: Some(payment_id)
			}
		);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap()
				.status,
			BountyStatus::Closing { curator: Some(s.child_curator) },
		);
		assert_eq!(
			pallet_bounties::ChildValuePerParentAsset::<Test>::get(s.parent_bounty_id, 1),
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
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), 1)
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
				child_index: Some(s.child_bounty_id),
				asset_kind: 1
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
			pallet_bounties::ChildValuePerParentAsset::<Test>::get(s.parent_bounty_id, 1),
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
		assert_ok!(Bounties::fund_child_bounty_with_assets(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			basket(vec![(1, s.child_value)]),
			s.metadata,
			None
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
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), 1)
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
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), 1)
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
			pallet_bounties::ChildValuePerParentAsset::<Test>::get(s.parent_bounty_id, 1),
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
		let payment_id = get_payment_id(s.parent_bounty_id, None, 1).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then: BountyPayoutProcessed should emit the net payout (parent value minus child
		// value), not the full parent value.
		let expected_payout = s.value - s.child_value;
		expect_events(vec![BountiesEvent::BountyPayoutProcessed {
			index: s.parent_bounty_id,
			child_index: None,
			asset_kind: s.asset_kind,
			value: expected_payout,
			beneficiary: s.beneficiary,
		}]);
		assert_eq!(
			pallet_bounties::ChildValuePerParentAsset::<Test>::get(s.parent_bounty_id, 1),
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

		// When/Then: a funding payment still in flight is left as it is. (The single-asset
		// pallet returned `FundingInconclusive` here.)
		let payment_id = get_payment_id(s.parent_bounty_id, None, 1).expect("no payment attempt");
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));
		assert_eq!(get_payment_id(s.parent_bounty_id, None, 1), Some(payment_id));
		assert!(matches!(status(s.parent_bounty_id), BountyStatus::Funding { .. }));

		// Given
		set_status(payment_id, PaymentStatus::Failure);
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then: nothing is in flight any more, so checking again changes nothing. (The
		// single-asset pallet returned `UnexpectedStatus` here.)
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));
		assert_eq!(
			bounty_asset(s.parent_bounty_id, 1).unwrap().payment_state,
			PaymentState::Failed
		);
		assert!(matches!(status(s.parent_bounty_id), BountyStatus::Funding { .. }));

		// Given
		let s = create_canceled_parent_bounty();

		// Then: an in-flight refund leaves the bounty closing. (Was `RefundInconclusive`.)
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));
		assert!(matches!(status(s.parent_bounty_id), BountyStatus::Closing { .. }));

		// Given
		let s = create_awarded_parent_bounty();

		// Then: an in-flight payout leaves the bounty awarding. (Was `PayoutInconclusive`.)
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), s.parent_bounty_id, None));
		assert!(matches!(status(s.parent_bounty_id), BountyStatus::Awarding { .. }));
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
		let payment_id = get_payment_id(s.parent_bounty_id, None, 1).expect("no payment attempt");
		assert_eq!(paid(parent_bounty_account, s.asset_kind), s.value);
		assert_eq!(
			last_event(),
			BountiesEvent::Paid {
				index: s.parent_bounty_id,
				child_index: None,
				asset_kind: 1,
				payment_id
			}
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap(),
			Bounty { metadata: s.metadata, status: BountyStatus::Funding { curator: s.curator } }
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
		let payment_id = get_payment_id(s.parent_bounty_id, None, 1).expect("no payment attempt");
		assert_eq!(paid(funding_source_account, s.asset_kind), s.value);
		assert_eq!(
			last_event(),
			BountiesEvent::Paid {
				index: s.parent_bounty_id,
				child_index: None,
				asset_kind: 1,
				payment_id
			}
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap().status,
			BountyStatus::Closing { curator: Some(s.curator) },
		);

		// Given: parent bounty status `PayoutAttempted`
		let s = create_awarded_parent_bounty();
		reject_payment(s.beneficiary, s.parent_bounty_id, None, s.asset_kind, s.value);

		// When
		assert_ok!(Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, None));

		// Then
		let payment_id = get_payment_id(s.parent_bounty_id, None, 1).expect("no payment attempt");
		assert_eq!(paid(s.beneficiary, s.asset_kind), s.value);
		assert_eq!(
			last_event(),
			BountiesEvent::Paid {
				index: s.parent_bounty_id,
				child_index: None,
				asset_kind: 1,
				payment_id
			}
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap().status,
			BountyStatus::Awarding { curator: s.curator, beneficiary: s.beneficiary }
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
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), 1)
			.expect("no payment attempt");
		assert_eq!(paid(child_bounty_account, s.asset_kind), s.child_value);
		assert_eq!(
			last_event(),
			BountiesEvent::Paid {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id),
				asset_kind: 1,
				payment_id
			}
		);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap(),
			ChildBounty {
				parent_bounty: s.parent_bounty_id,
				metadata: s.metadata,
				status: BountyStatus::Funding { curator: s.child_curator }
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

		// Then: the child value is back in the parent account
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), 1)
			.expect("no payment attempt");
		assert_eq!(paid(child_bounty_account, s.asset_kind), 0);
		assert_eq!(paid(parent_bounty_account, s.asset_kind), s.value);
		assert_eq!(
			last_event(),
			BountiesEvent::Paid {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id),
				asset_kind: 1,
				payment_id
			}
		);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap()
				.status,
			BountyStatus::Closing { curator: Some(s.child_curator) },
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
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), 1)
			.expect("no payment attempt");
		assert_eq!(paid(s.child_beneficiary, s.asset_kind), s.child_value);
		expect_events(vec![BountiesEvent::Paid {
			index: s.parent_bounty_id,
			child_index: Some(s.child_bounty_id),
			asset_kind: 1,
			payment_id,
		}]);
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap()
				.status,
			BountyStatus::Awarding { curator: s.child_curator, beneficiary: s.child_beneficiary }
		);
	});
}

#[test]
fn retry_payment_fails() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let s = create_parent_bounty();

		// When/Then: a payment still in flight is not retried. (The single-asset pallet returned
		// `UnexpectedStatus` here.)
		let payment_id = get_payment_id(s.parent_bounty_id, None, 1).expect("no payment attempt");
		assert_ok!(Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, None));
		assert_eq!(get_payment_id(s.parent_bounty_id, None, 1), Some(payment_id));

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
		let payment_id = get_payment_id(s.parent_bounty_id, None, 1).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);

		// When/Then: the payment has not been checked yet, so it is still in flight and is not
		// retried. (The single-asset pallet returned `UnexpectedStatus` here.)
		assert_ok!(Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, None));
		assert_eq!(get_payment_id(s.parent_bounty_id, None, 1), Some(payment_id));

		// Given: parent bounty status is `RefundAttempted` and payment succeeds
		let s = create_canceled_parent_bounty();
		let payment_id = get_payment_id(s.parent_bounty_id, None, 1).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);

		// When/Then: the payment has not been checked yet, so it is still in flight and is not
		// retried. (The single-asset pallet returned `UnexpectedStatus` here.)
		assert_ok!(Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, None));
		assert_eq!(get_payment_id(s.parent_bounty_id, None, 1), Some(payment_id));

		// Given: parent bounty status is `PayoutAttempted` and payments succeed
		let s = create_awarded_parent_bounty();
		let payment_id = get_payment_id(s.parent_bounty_id, None, 1).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);

		// When/Then: the payment has not been checked yet, so it is still in flight and is not
		// retried. (The single-asset pallet returned `UnexpectedStatus` here.)
		assert_ok!(Bounties::retry_payment(RuntimeOrigin::signed(1), s.parent_bounty_id, None));
		assert_eq!(get_payment_id(s.parent_bounty_id, None, 1), Some(payment_id));

		// Given: child-bounty status is `FundingAttempted` and payment succeeds
		let s = create_child_bounty_with_curator();
		let payment_id = get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), 1)
			.expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);

		// When/Then: still in flight, so not retried.
		assert_ok!(Bounties::retry_payment(
			RuntimeOrigin::signed(1),
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		));
		assert_eq!(
			get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), 1),
			Some(payment_id)
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
			Bounty { metadata: s.metadata, status: BountyStatus::Active { curator: s.curator } }
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
				metadata: s.metadata,
				status: BountyStatus::Active { curator: s.child_curator }
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
		assert_ok!(Bounties::fund_child_bounty_with_assets(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			basket(vec![(1, s.child_value)]),
			s.metadata,
			Some(s.child_curator)
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
		assert_ok!(Bounties::fund_bounty_with_assets(
			RuntimeOrigin::root(),
			basket(vec![(asset_kind, value)]),
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
		assert_ok!(Bounties::fund_bounty_with_assets(
			RuntimeOrigin::root(),
			basket(vec![(asset_kind, value)]),
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
			Bounty { metadata: s.metadata, status: BountyStatus::CuratorUnassigned }
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
				metadata: s.metadata,
				status: BountyStatus::CuratorUnassigned
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
		assert_eq!(Balances::free_balance(&s.child_curator), Balances::minimum_balance());
		// burned
	});
}

#[test]
fn unassign_curator_fails() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let s = setup_bounty();
		assert_ok!(Bounties::fund_bounty_with_assets(
			RuntimeOrigin::root(),
			basket(vec![(s.asset_kind, s.value)]),
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
			Bounty { metadata: s.metadata, status: BountyStatus::Funded { curator: s.curator } }
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
				metadata: s.metadata,
				status: BountyStatus::Funded { curator: s.child_curator }
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
		assert_ok!(Bounties::fund_child_bounty_with_assets(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			basket(vec![(1, s.child_value)]),
			s.metadata,
			Some(s.child_curator)
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
		// A payment of the asset is in flight.
		assert!(get_payment_id(s.parent_bounty_id, None, 1).is_some());
		assert_eq!(
			last_event(),
			BountiesEvent::BountyCanceled { index: s.parent_bounty_id, child_index: None }
		);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap(),
			Bounty {
				metadata: s.metadata,
				status: BountyStatus::Closing { curator: Some(s.curator) }
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
		// A payment of the asset is in flight.
		assert!(get_payment_id(s.parent_bounty_id, None, 1).is_some());
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap().status,
			BountyStatus::Closing { curator: Some(s.curator) }
		);

		// Given: parent bounty status is `UnassignedCurator`
		let s = create_parent_bounty_with_unassigned_curator();

		// When: sender is `RejectOrigin`
		assert_ok!(Bounties::close_bounty(RuntimeOrigin::root(), s.parent_bounty_id, None,));

		// Then
		// A payment of the asset is in flight.
		assert!(get_payment_id(s.parent_bounty_id, None, 1).is_some());
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(s.parent_bounty_id).unwrap().status,
			BountyStatus::Closing { curator: None }
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
		// A payment of the asset is in flight.
		assert!(get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), 1).is_some());
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
				metadata: s.metadata,
				status: BountyStatus::Closing { curator: Some(s.child_curator) }
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
		// A payment of the asset is in flight.
		assert!(get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), 1).is_some());
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap()
				.status,
			BountyStatus::Closing { curator: Some(s.child_curator) }
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
		// A payment of the asset is in flight.
		assert!(get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), 1).is_some());
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap()
				.status,
			BountyStatus::Closing { curator: Some(s.child_curator) }
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
		// A payment of the asset is in flight.
		assert!(get_payment_id(s.parent_bounty_id, Some(s.child_bounty_id), 1).is_some());
		assert_eq!(
			pallet_bounties::ChildBounties::<Test>::get(s.parent_bounty_id, s.child_bounty_id)
				.unwrap()
				.status,
			BountyStatus::Closing { curator: None }
		);
	})
}

#[test]
fn close_bounty_fails() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let s = create_parent_bounty();

		// When/Then: a bounty can be closed from `Funding`, but not while a funding payment is
		// in flight. (The single-asset pallet returned `UnexpectedStatus` here.)
		assert_noop!(
			Bounties::close_bounty(RuntimeOrigin::root(), s.parent_bounty_id, None),
			Error::<Test>::FundingInconclusive
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
		assert_ok!(Bounties::fund_child_bounty_with_assets(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			basket(vec![(1, s.child_value)]),
			s.metadata,
			Some(s.child_curator)
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
			s.value, // child value returned to the parent it was funded from
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

/// Regression test for closing a parent bounty after one of its child-bounties was paid out.
///
/// The paid-out child value left the parent account for good, but `close_bounty` used to request
/// a refund of the full stored `value`. A paymaster that performs a real transfer (`LocalPay`)
/// rejects that, so the parent bounty could never be closed. Without the `calculate_payout`
/// adjustment in `do_process_refund_payment` this test fails with `RefundError`.
#[test]
fn fund_and_award_child_bounty_without_curator_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let s = create_active_parent_bounty();

		// When
		assert_ok!(Bounties::fund_child_bounty_with_assets(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			basket(vec![(1, s.child_value)]),
			s.metadata,
			None
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
			pallet_bounties::ChildValuePerParentAsset::<Test>::get(s.parent_bounty_id, 1),
			s.child_value
		);
		assert_eq!(
			Balances::free_balance(s.child_curator),
			Balances::minimum_balance() + s.child_curator_deposit
		); // initial
	})
}

#[test]
fn unprivileged_caller_cannot_unassign_active_child_curator_when_parent_not_active() {
	ExtBuilder::default().build_and_execute(|| {
		let s = create_active_child_bounty();
		let attacker: u128 = 99;

		// Verify preconditions: child bounty is Active with curator deposit held.
		assert_eq!(Balances::reserved_balance(&s.child_curator), s.child_curator_deposit);
		assert!(pallet_bounties::CuratorDeposit::<Test>::get(
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		)
		.is_some());

		// Step 1: Parent curator voluntarily unassigns from parent bounty.
		assert_ok!(Bounties::unassign_curator(
			RuntimeOrigin::signed(s.curator),
			s.parent_bounty_id,
			None,
		));

		// Parent is now CuratorUnassigned; child is still Active.
		let (parent_status, _, _) =
			Bounties::get_bounty_details(s.parent_bounty_id, None).expect("parent bounty exists");
		assert_eq!(parent_status, BountyStatus::CuratorUnassigned);

		let (child_status, _, parent_curator) =
			Bounties::get_bounty_details(s.parent_bounty_id, Some(s.child_bounty_id))
				.expect("child bounty exists");
		assert!(matches!(child_status, BountyStatus::Active { .. }));
		assert!(
			parent_curator.is_none(),
			"parent_curator should be None since parent is not Active"
		);

		// Step 2: An unprivileged attacker tries to unassign the active child bounty curator.
		// This must be rejected with BadOrigin.
		assert_noop!(
			Bounties::unassign_curator(
				RuntimeOrigin::signed(attacker),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			),
			BadOrigin
		);

		// Child bounty remains Active the unprivileged caller had no effect.
		let (child_status, _, _) =
			Bounties::get_bounty_details(s.parent_bounty_id, Some(s.child_bounty_id))
				.expect("child bounty exists");
		assert!(matches!(child_status, BountyStatus::Active { .. }));

		// Curator deposit is still in storage and the hold is intact.
		assert!(
			pallet_bounties::CuratorDeposit::<Test>::get(
				s.parent_bounty_id,
				Some(s.child_bounty_id)
			)
			.is_some(),
			"curator deposit must remain in storage"
		);
		assert_eq!(
			Balances::reserved_balance(&s.child_curator),
			s.child_curator_deposit,
			"curator deposit hold must remain intact"
		);

		// Step 3: Verify that the child curator can still voluntarily unassign themselves.
		assert_ok!(Bounties::unassign_curator(
			RuntimeOrigin::signed(s.child_curator),
			s.parent_bounty_id,
			Some(s.child_bounty_id),
		));

		let (child_status, _, _) =
			Bounties::get_bounty_details(s.parent_bounty_id, Some(s.child_bounty_id))
				.expect("child bounty exists");
		assert_eq!(child_status, BountyStatus::CuratorUnassigned);

		// Curator's deposit was properly released.
		assert_eq!(
			Balances::reserved_balance(&s.child_curator),
			0,
			"curator deposit hold must be released after voluntary unassign"
		);
	});
}

#[test]
fn multi_asset_bounty_accounts_differ_from_legacy_bounty_accounts() {
	ExtBuilder::default().build_and_execute(|| {
		use sp_runtime::traits::AccountIdConversion;

		let bounty_id: BountyIndex = 0;

		// Old derivation (what pallet-bounties uses with &str)
		let old_bounty_account: u128 =
			BountyPalletId::get().into_sub_account_truncating(("bt", bounty_id));
		// New derivation (what multi-asset-bounties uses with [u8; 3])
		let new_bounty_account: u128 = BountyPalletId::get()
			.into_sub_account_truncating((BountyAccountPrefix::get(), bounty_id));

		assert_ne!(
			old_bounty_account, new_bounty_account,
			"multi-asset bounty account must differ from legacy bounty account"
		);

		let parent_bounty_id: BountyIndex = 0;
		let child_bounty_id: BountyIndex = 0;

		// Old derivation (what pallet-child-bounties uses with &str)
		let old_child_account: u128 = BountyPalletId::get().into_sub_account_truncating((
			"cb",
			parent_bounty_id,
			child_bounty_id,
		));
		// New derivation (what multi-asset-bounties uses with [u8; 3])
		let new_child_account: u128 = BountyPalletId::get().into_sub_account_truncating((
			ChildBountyAccountPrefix::get(),
			parent_bounty_id,
			child_bounty_id,
		));

		assert_ne!(
			old_child_account, new_child_account,
			"multi-asset child bounty account must differ from legacy child bounty account"
		);

		// Also verify bounty and child-bounty accounts are distinct from each other
		// when using the same indices
		assert_ne!(
			new_bounty_account, new_child_account,
			"bounty and child-bounty accounts must differ even with the same indices"
		);
	});
}

#[test]
fn integrity_test() {
	ExtBuilder::default().build_and_execute(|| {
		Bounties::integrity_test();
	});
}

// -----------------------------------------------------------------------------
// Synchronous `Paymaster`, as `LocalPay` on Asset Hub
// -----------------------------------------------------------------------------

#[test]
fn sync_paymaster_runs_a_basket_with_a_child_to_completion() {
	ExtBuilder::default().build_and_execute(|| {
		use_sync_paymaster();
		let curator = 4;
		let metadata = note_preimage(1);
		let _ = Balances::mint_into(
			&curator,
			Balances::minimum_balance() + CuratorDepositCalc::convert(50 + 40 + 30),
		);

		// Fund a 3-asset bounty. Every payment has the same id, so only `asset_kind` tells the
		// `Paid` events apart.
		assert_ok!(Bounties::fund_bounty_with_assets(
			RuntimeOrigin::root(),
			basket(vec![(1, 50), (2, 40), (3, 30)]),
			curator,
			metadata
		));
		let index = 0;
		for asset_kind in [1, 2, 3] {
			assert!(has_event(BountiesEvent::Paid {
				index,
				child_index: None,
				asset_kind,
				payment_id: SYNC_PAYMENT_ID,
			}));
		}
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), index, None));
		assert_eq!(status(index), BountyStatus::Funded { curator });
		assert_ok!(Bounties::accept_curator(RuntimeOrigin::signed(curator), index, None));

		// A child curated by the parent curator takes part of assets 1 and 3.
		assert_ok!(Bounties::fund_child_bounty_with_assets(
			RuntimeOrigin::signed(curator),
			index,
			basket(vec![(1, 10), (3, 20)]),
			metadata,
			None,
		));
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), index, Some(0)));
		assert_eq!(child_status(index, 0), BountyStatus::Active { curator });
		assert_ok!(Bounties::award_bounty(RuntimeOrigin::signed(curator), index, Some(0), 9));
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), index, Some(0)));
		assert_eq!(pallet_bounties::ChildBounties::<Test>::get(index, 0), None);
		assert_eq!((paid(9, 1), paid(9, 3)), (10, 20));

		// The parent pays out what is left of every asset.
		assert_ok!(Bounties::award_bounty(RuntimeOrigin::signed(curator), index, None, 5));
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), index, None));
		assert_eq!(pallet_bounties::Bounties::<Test>::get(index), None);
		assert_eq!((paid(5, 1), paid(5, 2), paid(5, 3)), (40, 40, 10));
	});
}

#[test]
fn sync_paymaster_isolates_a_rejected_payout_leg() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: an active 2-asset parent whose account cannot pay asset 2.
		let (parent, curator, _metadata) = active_multi_asset_parent();
		use_sync_paymaster();
		let held = drain(parent, 2);

		// When: award. Asset 1 settles inside the call, asset 2 is rejected.
		assert_ok!(Bounties::award_bounty(RuntimeOrigin::signed(curator), parent, None, 5));
		assert_eq!(paid(5, 1), 50);
		assert_eq!(bounty_asset(parent, 2).unwrap().payment_state, PaymentState::Failed);
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), parent, None));
		assert_eq!(bounty_asset(parent, 1).unwrap().payment_state, PaymentState::Succeeded);
		assert_eq!(status(parent), BountyStatus::Awarding { curator, beneficiary: 5 });

		// Then: once asset 2 can move, a retry and a check complete the award.
		top_up(Bounties::bounty_account(parent, 2).unwrap(), 2, held);
		assert_ok!(Bounties::retry_payment(RuntimeOrigin::signed(1), parent, None));
		assert_ok!(Bounties::check_status(RuntimeOrigin::signed(1), parent, None));
		assert_eq!(pallet_bounties::Bounties::<Test>::get(parent), None);
		assert_eq!(paid(5, 2), 40);
	});
}

#[test]
fn sync_paymaster_single_asset_award_reverts_like_before() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: an active single-asset bounty whose account cannot pay.
		let s = create_active_parent_bounty();
		use_sync_paymaster();
		drain(s.parent_bounty_id, s.asset_kind);

		// Then: the only leg is rejected, so the call reverts and the bounty stays `Active`, as
		// in the single-asset pallet.
		assert_noop!(
			Bounties::award_bounty(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				None,
				s.beneficiary
			),
			Error::<Test>::PayoutError
		);
	});
}

#[test]
fn child_funding_reverts_the_legs_that_went_through_when_one_is_rejected() {
	ExtBuilder::default().build_and_execute(|| {
		// Given: an active 2-asset parent whose account cannot pay asset 2.
		let (parent, curator, metadata) = active_multi_asset_parent();
		drain(parent, 2);
		let account = Bounties::bounty_account(parent, 1).expect("conversion failed");

		// When: a child is funded with both assets. Asset 1 goes through first, asset 2 is
		// rejected.
		assert_noop!(
			Bounties::fund_child_bounty_with_assets(
				RuntimeOrigin::signed(curator),
				parent,
				basket(vec![(1, 20), (2, 10)]),
				metadata,
				None,
			),
			Error::<Test>::FundingError
		);

		// Then: funding is all-or-nothing, so asset 1 never left the parent either.
		assert_eq!(paid(account, 1), 50);
		assert_eq!(paid(Bounties::child_bounty_account(parent, 0, 1).unwrap(), 1), 0);
	});
}

// -----------------------------------------------------------------------------
// Deprecated single-asset calls
// -----------------------------------------------------------------------------

/// `fund_bounty` (0), `fund_child_bounty` (1) and `increase_value` (9) keep master's signatures as
/// deprecated wrappers over the basket calls. The tests marked "master" are copied from master
/// unchanged, so callers built against the old API see the same behaviour.
#[allow(deprecated)]
mod deprecated_calls {
	use super::*;
	use codec::{Compact, Decode, Encode};

	#[test]
	fn the_old_calls_keep_their_indices_and_encoding() {
		ExtBuilder::default().build_and_execute(|| {
			let metadata = note_preimage(1);
			let cases: Vec<(BountiesCall, Vec<u8>)> = vec![
				(
					BountiesCall::fund_bounty {
						asset_kind: Box::new(1),
						value: 50,
						curator: 4,
						metadata,
					},
					(0u8, 1u32, Compact(50u64), 4u128, metadata).encode(),
				),
				(
					BountiesCall::fund_child_bounty {
						parent_bounty_id: 0,
						value: 10,
						metadata,
						curator: Some(6),
					},
					(1u8, Compact(0u32), Compact(10u64), metadata, Some(6u128)).encode(),
				),
				(
					BountiesCall::increase_value { parent_bounty_id: 0, amount: 20 },
					(9u8, Compact(0u32), Compact(20u64)).encode(),
				),
				(
					BountiesCall::fund_bounty_with_assets {
						assets: basket(vec![(1, 50)]),
						curator: 4,
						metadata,
					},
					(10u8, vec![(1u32, 50u64)], 4u128, metadata).encode(),
				),
				(
					BountiesCall::fund_child_bounty_with_assets {
						parent_bounty_id: 0,
						assets: basket(vec![(1, 10)]),
						metadata,
						curator: Some(6),
					},
					(11u8, Compact(0u32), vec![(1u32, 10u64)], metadata, Some(6u128)).encode(),
				),
				(
					BountiesCall::increase_asset_value {
						parent_bounty_id: 0,
						asset_kind: Box::new(1),
						amount: 20,
					},
					(12u8, Compact(0u32), 1u32, Compact(20u64)).encode(),
				),
			];
			for (call, encoded) in cases {
				assert_eq!(call.encode(), encoded);
				assert_eq!(BountiesCall::decode(&mut &encoded[..]).unwrap(), call);
			}
		});
	}

	#[test]
	fn fund_bounty_funds_a_basket_of_one() {
		ExtBuilder::default().build_and_execute(|| {
			let (asset_kind, value, curator) = (1, 50, 4);
			let metadata = note_preimage(1);
			let _ = Balances::mint_into(&curator, Balances::minimum_balance());

			assert_ok!(Bounties::fund_bounty(
				RuntimeOrigin::root(),
				Box::new(asset_kind),
				value,
				curator,
				metadata
			));

			let index = 0;
			let payment_id = get_payment_id(index, None, asset_kind).expect("no payment attempt");
			assert_eq!(
				pallet_bounties::BountyAssets::<Test>::iter_prefix(index).collect::<Vec<_>>(),
				vec![(
					asset_kind,
					AssetEntry { value, payment_state: PaymentState::Attempted { id: payment_id } }
				)]
			);
			assert_eq!(status(index), BountyStatus::Funding { curator });
			expect_events(vec![
				BountiesEvent::Paid { index, child_index: None, asset_kind, payment_id },
				BountiesEvent::BountyCreated { index },
			]);
		});
	}

	#[test]
	fn fund_child_bounty_uses_the_parents_only_asset() {
		ExtBuilder::default().build_and_execute(|| {
			let s = create_active_parent_bounty();

			assert_ok!(Bounties::fund_child_bounty(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				s.child_value,
				s.metadata,
				Some(s.child_curator),
			));

			let (parent, child) = (s.parent_bounty_id, 0);
			let id = get_payment_id(parent, Some(child), s.asset_kind).expect("no payment attempt");
			assert_eq!(
				pallet_bounties::ChildBountyAssets::<Test>::iter_prefix((parent, child))
					.collect::<Vec<_>>(),
				vec![(
					s.asset_kind,
					AssetEntry {
						value: s.child_value,
						payment_state: PaymentState::Attempted { id }
					}
				)]
			);
			assert_eq!(
				pallet_bounties::ChildValuePerParentAsset::<Test>::get(parent, s.asset_kind),
				s.child_value
			);
			assert_eq!(
				child_status(parent, child),
				BountyStatus::Funding { curator: s.child_curator }
			);
		});
	}

	#[test]
	fn increase_value_raises_the_only_asset() {
		ExtBuilder::default().build_and_execute(|| {
			let s = create_active_parent_bounty();
			let _ = Balances::mint_into(&s.curator, 100);

			assert_ok!(Bounties::increase_value(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				20
			));

			let new_value = s.value + 20;
			assert_eq!(bounty_asset(s.parent_bounty_id, s.asset_kind).unwrap().value, new_value);
			expect_events(vec![BountiesEvent::BountyValueIncreased {
				index: s.parent_bounty_id,
				asset_kind: s.asset_kind,
				old_value: s.value,
				new_value,
			}]);
		});
	}

	#[test]
	fn calls_that_name_no_asset_reject_a_multi_asset_bounty() {
		ExtBuilder::default().build_and_execute(|| {
			// Given: an active parent holding assets 1 and 2.
			let (parent, curator, metadata) = active_multi_asset_parent();

			// When/Then: the old calls can't tell which asset is meant.
			assert_noop!(
				Bounties::fund_child_bounty(
					RuntimeOrigin::signed(curator),
					parent,
					10,
					metadata,
					None
				),
				Error::<Test>::AmbiguousAsset
			);
			assert_noop!(
				Bounties::increase_value(RuntimeOrigin::signed(curator), parent, 10),
				Error::<Test>::AmbiguousAsset
			);

			// And: the calls that name the asset work on the same bounty.
			assert_ok!(Bounties::fund_child_bounty_with_assets(
				RuntimeOrigin::signed(curator),
				parent,
				basket(vec![(2, 10)]),
				metadata,
				None
			));
		});
	}

	#[test]
	fn old_and_new_calls_share_the_spend_limit_in_a_batch() {
		ExtBuilder::default().build_and_execute(|| {
			let (asset_kind, curator, spend_origin) = (1, 4, 10); // max spending of 10
			let metadata = note_preimage(1);
			let _ = Balances::mint_into(&curator, Balances::minimum_balance());
			let calls = |value| {
				vec![
					RuntimeCall::from(BountiesCall::fund_bounty {
						asset_kind: Box::new(asset_kind),
						value,
						curator,
						metadata,
					}),
					RuntimeCall::from(BountiesCall::fund_bounty_with_assets {
						assets: basket(vec![(asset_kind, value)]),
						curator,
						metadata,
					}),
				]
			};

			assert_ok!(RuntimeCall::from(UtilityCall::batch_all { calls: calls(2) })
				.dispatch(RuntimeOrigin::signed(spend_origin)));
			assert_err_ignore_postinfo!(
				RuntimeCall::from(UtilityCall::batch_all { calls: calls(5) })
					.dispatch(RuntimeOrigin::signed(spend_origin)),
				Error::<Test>::InsufficientPermission
			);
		});
	}

	// master
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

	// master
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
				Bounties::fund_bounty(
					invalid_origin,
					Box::new(asset_kind),
					value,
					curator,
					metadata
				),
				BadOrigin
			);

			// When/Then
			let invalid_origin = RuntimeOrigin::signed(0);
			assert_noop!(
				Bounties::fund_bounty(
					invalid_origin,
					Box::new(asset_kind),
					value,
					curator,
					metadata
				),
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

	// master
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
					s.metadata,
					Some(s.child_curator),
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
					s.metadata,
					None,
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
					s.metadata,
					None,
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
					invalid_metadata,
					None,
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
					s.metadata,
					None,
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
					s.metadata,
					None,
				),
				Error::<Test>::InsufficientBountyValue
			);

			// Given
			MaxActiveChildBountyCount::set(1);
			assert_ok!(Bounties::fund_child_bounty(
				RuntimeOrigin::signed(s.curator),
				s.parent_bounty_id,
				s.child_value,
				s.metadata,
				None,
			));

			// When/Then
			assert_noop!(
				Bounties::fund_child_bounty(
					RuntimeOrigin::signed(s.curator),
					s.parent_bounty_id,
					s.child_value,
					s.metadata,
					None,
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
					s.metadata,
					None,
				),
				Error::<Test>::UnexpectedStatus
			);
		})
	}

	// master
	#[test]
	fn increase_value_fails() {
		ExtBuilder::default().build_and_execute(|| {
			// When/Then: unknown bounty index.
			assert_noop!(
				Bounties::increase_value(RuntimeOrigin::signed(4), 99, 10),
				Error::<Test>::InvalidIndex
			);

			// Given: a parent bounty that is not yet `Active` (`FundingAttempted`).
			let s = create_parent_bounty();

			// When/Then: cannot increase value before the bounty is active.
			assert_noop!(
				Bounties::increase_value(RuntimeOrigin::signed(s.curator), s.parent_bounty_id, 10),
				Error::<Test>::UnexpectedStatus
			);

			// Given: an active parent bounty.
			let s = create_active_parent_bounty();

			// When/Then: unsigned origin is rejected.
			assert_noop!(
				Bounties::increase_value(RuntimeOrigin::none(), s.parent_bounty_id, 10),
				BadOrigin
			);

			// When/Then: only the curator may increase the value.
			assert_noop!(
				Bounties::increase_value(RuntimeOrigin::signed(1), s.parent_bounty_id, 10),
				Error::<Test>::RequireCurator
			);

			// When/Then: a zero increase is rejected.
			assert_noop!(
				Bounties::increase_value(RuntimeOrigin::signed(s.curator), s.parent_bounty_id, 0),
				Error::<Test>::InvalidValue
			);
		});
	}

	// master, apart from reading the value from the asset row
	#[test]
	fn increase_value_reverts_when_curator_cannot_fund_deposit() {
		ExtBuilder::default().build_and_execute(|| {
			// Given: an active parent bounty. The curator's free balance is only the existential
			// deposit (their 25 deposit is already held), so they cannot fund a larger hold.
			let s = create_active_parent_bounty();
			assert_eq!(Balances::reserved_balance(s.curator), s.curator_deposit);

			// When/Then: the increase requires a larger deposit (50% * 70 = 35, i.e. +10) the
			// curator cannot afford, so the call fails.
			assert_noop!(
				Bounties::increase_value(RuntimeOrigin::signed(s.curator), s.parent_bounty_id, 20),
				TokenError::FundsUnavailable
			);

			// And: the whole call is rolled back — the deposit was NOT orphaned by the earlier
			// `take`, and the value is unchanged.
			// The value now lives in the asset row (the only change to master's test).
			assert_eq!(bounty_asset(s.parent_bounty_id, s.asset_kind).unwrap().value, s.value);
			assert_eq!(
				pallet_bounties::CuratorDeposit::<Test>::get(
					s.parent_bounty_id,
					None::<BountyIndex>
				)
				.unwrap(),
				consideration(s.value)
			);
			assert_eq!(Balances::reserved_balance(s.curator), s.curator_deposit);
		});
	}
}
