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

//! Tests for Uniques pallet.

use crate::{mock::*, Event, *};
use frame_support::{assert_noop, assert_ok, traits::Currency};
use pallet_balances::Error as BalancesError;
use sp_runtime::{traits::Dispatchable, DispatchError};

fn items() -> Vec<(u64, u32, u32)> {
	let mut r: Vec<_> = Account::<Test>::iter().map(|x| x.0).collect();
	r.sort();
	let mut s: Vec<_> = Item::<Test>::iter().map(|x| (x.2.owner, x.0, x.1)).collect();
	s.sort();
	assert_eq!(r, s);
	for collection in Item::<Test>::iter()
		.map(|x| x.0)
		.scan(None, |s, item| {
			if s.map_or(false, |last| last == item) {
				*s = Some(item);
				Some(None)
			} else {
				Some(Some(item))
			}
		})
		.flatten()
	{
		let details = Collection::<Test>::get(collection).unwrap();
		let items = Item::<Test>::iter_prefix(collection).count() as u32;
		assert_eq!(details.items, items);
	}
	r
}

fn collections() -> Vec<(u64, u32)> {
	let mut r: Vec<_> = CollectionAccount::<Test>::iter().map(|x| (x.0, x.1)).collect();
	r.sort();
	let mut s: Vec<_> = Collection::<Test>::iter().map(|x| (x.1.owner, x.0)).collect();
	s.sort();
	assert_eq!(r, s);
	r
}

macro_rules! bvec {
	($( $x:tt )*) => {
		vec![$( $x )*].try_into().unwrap()
	}
}

fn attributes(collection: u32) -> Vec<(Option<u32>, Vec<u8>, Vec<u8>)> {
	let mut s: Vec<_> = Attribute::<Test>::iter_prefix((collection,))
		.map(|(k, v)| (k.0, k.1.into(), v.0.into()))
		.collect();
	s.sort();
	s
}

fn events() -> Vec<Event<Test>> {
	let result = System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| if let mock::RuntimeEvent::Uniques(inner) = e { Some(inner) } else { None })
		.collect::<Vec<_>>();

	System::reset_events();

	result
}

#[test]
fn basic_setup_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(items(), vec![]);
	});
}

#[test]
fn basic_minting_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, 1, true));
		assert_eq!(collections(), vec![(1, 0)]);
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, 42, 1));
		assert_eq!(items(), vec![(1, 0, 42)]);

		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 1, 2, true));
		assert_eq!(collections(), vec![(1, 0), (2, 1)]);
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(2), 1, 69, 1));
		assert_eq!(items(), vec![(1, 0, 42), (1, 1, 69)]);
	});
}

#[test]
fn lifecycle_should_work() {
	new_test_ext().execute_with(|| {
		Balances::make_free_balance_be(&1, 100);
		assert_ok!(Uniques::create(RuntimeOrigin::signed(1), 0, 1));
		assert_eq!(Balances::reserved_balance(&1), 2);
		assert_eq!(collections(), vec![(1, 0)]);
		assert_ok!(Uniques::set_collection_metadata(
			RuntimeOrigin::signed(1),
			0,
			bvec![0, 0],
			false
		));
		assert_eq!(Balances::reserved_balance(&1), 5);
		assert!(CollectionMetadataOf::<Test>::contains_key(0));

		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, 42, 10));
		assert_eq!(Balances::reserved_balance(&1), 6);
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, 69, 20));
		assert_eq!(Balances::reserved_balance(&1), 7);
		assert_eq!(items(), vec![(10, 0, 42), (20, 0, 69)]);
		assert_eq!(Collection::<Test>::get(0).unwrap().items, 2);
		assert_eq!(Collection::<Test>::get(0).unwrap().item_metadatas, 0);

		assert_ok!(Uniques::set_metadata(RuntimeOrigin::signed(1), 0, 42, bvec![42, 42], false));
		assert_eq!(Balances::reserved_balance(&1), 10);
		assert!(ItemMetadataOf::<Test>::contains_key(0, 42));
		assert_ok!(Uniques::set_metadata(RuntimeOrigin::signed(1), 0, 69, bvec![69, 69], false));
		assert_eq!(Balances::reserved_balance(&1), 13);
		assert!(ItemMetadataOf::<Test>::contains_key(0, 69));

		let w = Collection::<Test>::get(0).unwrap().destroy_witness();
		assert_eq!(w.items, 2);
		assert_eq!(w.item_metadatas, 2);
		assert_ok!(Uniques::destroy(RuntimeOrigin::signed(1), 0, w));
		assert_eq!(Balances::reserved_balance(&1), 0);

		assert!(!Collection::<Test>::contains_key(0));
		assert!(!Item::<Test>::contains_key(0, 42));
		assert!(!Item::<Test>::contains_key(0, 69));
		assert!(!CollectionMetadataOf::<Test>::contains_key(0));
		assert!(!ItemMetadataOf::<Test>::contains_key(0, 42));
		assert!(!ItemMetadataOf::<Test>::contains_key(0, 69));
		assert_eq!(collections(), vec![]);
		assert_eq!(items(), vec![]);
	});
}

#[test]
fn destroy_with_bad_witness_should_not_work() {
	new_test_ext().execute_with(|| {
		Balances::make_free_balance_be(&1, 100);
		assert_ok!(Uniques::create(RuntimeOrigin::signed(1), 0, 1));

		let w = Collection::<Test>::get(0).unwrap().destroy_witness();
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, 42, 1));
		assert_noop!(Uniques::destroy(RuntimeOrigin::signed(1), 0, w), Error::<Test>::BadWitness);
	});
}

#[test]
fn mint_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, 1, true));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, 42, 1));
		assert_eq!(Uniques::owner(0, 42).unwrap(), 1);
		assert_eq!(collections(), vec![(1, 0)]);
		assert_eq!(items(), vec![(1, 0, 42)]);
	});
}

#[test]
fn transfer_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, 1, true));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, 42, 2));

		assert_ok!(Uniques::transfer(RuntimeOrigin::signed(2), 0, 42, 3));
		assert_eq!(items(), vec![(3, 0, 42)]);
		assert_noop!(
			Uniques::transfer(RuntimeOrigin::signed(2), 0, 42, 4),
			Error::<Test>::NoPermission
		);

		assert_ok!(Uniques::approve_transfer(RuntimeOrigin::signed(3), 0, 42, 2));
		assert_ok!(Uniques::transfer(RuntimeOrigin::signed(2), 0, 42, 4));
	});
}

#[test]
fn freezing_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, 1, true));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, 42, 1));
		assert_ok!(Uniques::freeze(RuntimeOrigin::signed(1), 0, 42));
		assert_noop!(Uniques::transfer(RuntimeOrigin::signed(1), 0, 42, 2), Error::<Test>::Frozen);

		assert_ok!(Uniques::thaw(RuntimeOrigin::signed(1), 0, 42));
		assert_ok!(Uniques::freeze_collection(RuntimeOrigin::signed(1), 0));
		assert_noop!(Uniques::transfer(RuntimeOrigin::signed(1), 0, 42, 2), Error::<Test>::Frozen);

		assert_ok!(Uniques::thaw_collection(RuntimeOrigin::signed(1), 0));
		assert_ok!(Uniques::transfer(RuntimeOrigin::signed(1), 0, 42, 2));
	});
}

#[test]
fn origin_guards_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, 1, true));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, 42, 1));

		Balances::make_free_balance_be(&2, 100);
		assert_ok!(Uniques::set_accept_ownership(RuntimeOrigin::signed(2), Some(0)));
		assert_noop!(
			Uniques::transfer_ownership(RuntimeOrigin::signed(2), 0, 2),
			Error::<Test>::NoPermission
		);
		assert_noop!(
			Uniques::set_team(RuntimeOrigin::signed(2), 0, 2, 2, 2),
			Error::<Test>::NoPermission
		);
		assert_noop!(Uniques::freeze(RuntimeOrigin::signed(2), 0, 42), Error::<Test>::NoPermission);
		assert_noop!(Uniques::thaw(RuntimeOrigin::signed(2), 0, 42), Error::<Test>::NoPermission);
		assert_noop!(
			Uniques::mint(RuntimeOrigin::signed(2), 0, 69, 2),
			Error::<Test>::NoPermission
		);
		assert_noop!(
			Uniques::burn(RuntimeOrigin::signed(2), 0, 42, None),
			Error::<Test>::NoPermission
		);
		let w = Collection::<Test>::get(0).unwrap().destroy_witness();
		assert_noop!(Uniques::destroy(RuntimeOrigin::signed(2), 0, w), Error::<Test>::NoPermission);
	});
}

#[test]
fn transfer_owner_should_work() {
	new_test_ext().execute_with(|| {
		Balances::make_free_balance_be(&1, 100);
		Balances::make_free_balance_be(&2, 100);
		Balances::make_free_balance_be(&3, 100);
		assert_ok!(Uniques::create(RuntimeOrigin::signed(1), 0, 1));
		assert_eq!(collections(), vec![(1, 0)]);
		assert_noop!(
			Uniques::transfer_ownership(RuntimeOrigin::signed(1), 0, 2),
			Error::<Test>::Unaccepted
		);
		assert_eq!(System::consumers(&2), 0);
		assert_ok!(Uniques::set_accept_ownership(RuntimeOrigin::signed(2), Some(0)));
		assert_eq!(System::consumers(&2), 1);
		assert_ok!(Uniques::transfer_ownership(RuntimeOrigin::signed(1), 0, 2));
		assert_eq!(System::consumers(&2), 1);

		assert_eq!(collections(), vec![(2, 0)]);
		assert_eq!(Balances::total_balance(&1), 98);
		assert_eq!(Balances::total_balance(&2), 102);
		assert_eq!(Balances::reserved_balance(&1), 0);
		assert_eq!(Balances::reserved_balance(&2), 2);

		assert_ok!(Uniques::set_accept_ownership(RuntimeOrigin::signed(1), Some(0)));
		assert_noop!(
			Uniques::transfer_ownership(RuntimeOrigin::signed(1), 0, 1),
			Error::<Test>::NoPermission
		);

		// Mint and set metadata now and make sure that deposit gets transferred back.
		assert_ok!(Uniques::set_collection_metadata(
			RuntimeOrigin::signed(2),
			0,
			bvec![0u8; 20],
			false
		));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, 42, 1));
		assert_ok!(Uniques::set_metadata(RuntimeOrigin::signed(2), 0, 42, bvec![0u8; 20], false));
		assert_ok!(Uniques::set_accept_ownership(RuntimeOrigin::signed(3), Some(0)));
		assert_ok!(Uniques::transfer_ownership(RuntimeOrigin::signed(2), 0, 3));
		assert_eq!(collections(), vec![(3, 0)]);
		assert_eq!(Balances::total_balance(&2), 57);
		assert_eq!(Balances::total_balance(&3), 145);
		assert_eq!(Balances::reserved_balance(&2), 0);
		assert_eq!(Balances::reserved_balance(&3), 45);

		// 2's acceptance from before is reset when it became owner, so it cannot be transferred
		// without a fresh acceptance.
		assert_noop!(
			Uniques::transfer_ownership(RuntimeOrigin::signed(3), 0, 2),
			Error::<Test>::Unaccepted
		);
	});
}

#[test]
fn set_team_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, 1, true));
		assert_ok!(Uniques::set_team(RuntimeOrigin::signed(1), 0, 2, 3, 4));

		assert_ok!(Uniques::mint(RuntimeOrigin::signed(2), 0, 42, 2));
		assert_ok!(Uniques::freeze(RuntimeOrigin::signed(4), 0, 42));
		assert_ok!(Uniques::thaw(RuntimeOrigin::signed(3), 0, 42));
		assert_ok!(Uniques::transfer(RuntimeOrigin::signed(3), 0, 42, 3));
		assert_ok!(Uniques::burn(RuntimeOrigin::signed(3), 0, 42, None));
	});
}

#[test]
fn set_collection_metadata_should_work() {
	new_test_ext().execute_with(|| {
		// Cannot add metadata to unknown item
		assert_noop!(
			Uniques::set_collection_metadata(RuntimeOrigin::signed(1), 0, bvec![0u8; 20], false),
			Error::<Test>::UnknownCollection,
		);
		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, 1, false));
		// Cannot add metadata to unowned item
		assert_noop!(
			Uniques::set_collection_metadata(RuntimeOrigin::signed(2), 0, bvec![0u8; 20], false),
			Error::<Test>::NoPermission,
		);

		// Successfully add metadata and take deposit
		Balances::make_free_balance_be(&1, 30);
		assert_ok!(Uniques::set_collection_metadata(
			RuntimeOrigin::signed(1),
			0,
			bvec![0u8; 20],
			false
		));
		assert_eq!(Balances::free_balance(&1), 9);
		assert!(CollectionMetadataOf::<Test>::contains_key(0));

		// Force origin works, too.
		assert_ok!(Uniques::set_collection_metadata(
			RuntimeOrigin::root(),
			0,
			bvec![0u8; 18],
			false
		));

		// Update deposit
		assert_ok!(Uniques::set_collection_metadata(
			RuntimeOrigin::signed(1),
			0,
			bvec![0u8; 15],
			false
		));
		assert_eq!(Balances::free_balance(&1), 14);
		assert_ok!(Uniques::set_collection_metadata(
			RuntimeOrigin::signed(1),
			0,
			bvec![0u8; 25],
			false
		));
		assert_eq!(Balances::free_balance(&1), 4);

		// Cannot over-reserve
		assert_noop!(
			Uniques::set_collection_metadata(RuntimeOrigin::signed(1), 0, bvec![0u8; 40], false),
			BalancesError::<Test, _>::InsufficientBalance,
		);

		// Can't set or clear metadata once frozen
		assert_ok!(Uniques::set_collection_metadata(
			RuntimeOrigin::signed(1),
			0,
			bvec![0u8; 15],
			true
		));
		assert_noop!(
			Uniques::set_collection_metadata(RuntimeOrigin::signed(1), 0, bvec![0u8; 15], false),
			Error::<Test, _>::Frozen,
		);
		assert_noop!(
			Uniques::clear_collection_metadata(RuntimeOrigin::signed(1), 0),
			Error::<Test>::Frozen
		);

		// Clear Metadata
		assert_ok!(Uniques::set_collection_metadata(
			RuntimeOrigin::root(),
			0,
			bvec![0u8; 15],
			false
		));
		assert_noop!(
			Uniques::clear_collection_metadata(RuntimeOrigin::signed(2), 0),
			Error::<Test>::NoPermission
		);
		assert_noop!(
			Uniques::clear_collection_metadata(RuntimeOrigin::signed(1), 1),
			Error::<Test>::UnknownCollection
		);
		assert_ok!(Uniques::clear_collection_metadata(RuntimeOrigin::signed(1), 0));
		assert!(!CollectionMetadataOf::<Test>::contains_key(0));
	});
}

#[test]
fn set_item_metadata_should_work() {
	new_test_ext().execute_with(|| {
		Balances::make_free_balance_be(&1, 30);

		// Cannot add metadata to unknown item
		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, 1, false));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, 42, 1));
		// Cannot add metadata to unowned item
		assert_noop!(
			Uniques::set_metadata(RuntimeOrigin::signed(2), 0, 42, bvec![0u8; 20], false),
			Error::<Test>::NoPermission,
		);

		// Successfully add metadata and take deposit
		assert_ok!(Uniques::set_metadata(RuntimeOrigin::signed(1), 0, 42, bvec![0u8; 20], false));
		assert_eq!(Balances::free_balance(&1), 8);
		assert!(ItemMetadataOf::<Test>::contains_key(0, 42));

		// Force origin works, too.
		assert_ok!(Uniques::set_metadata(RuntimeOrigin::root(), 0, 42, bvec![0u8; 18], false));

		// Update deposit
		assert_ok!(Uniques::set_metadata(RuntimeOrigin::signed(1), 0, 42, bvec![0u8; 15], false));
		assert_eq!(Balances::free_balance(&1), 13);
		assert_ok!(Uniques::set_metadata(RuntimeOrigin::signed(1), 0, 42, bvec![0u8; 25], false));
		assert_eq!(Balances::free_balance(&1), 3);

		// Cannot over-reserve
		assert_noop!(
			Uniques::set_metadata(RuntimeOrigin::signed(1), 0, 42, bvec![0u8; 40], false),
			BalancesError::<Test, _>::InsufficientBalance,
		);

		// Can't set or clear metadata once frozen
		assert_ok!(Uniques::set_metadata(RuntimeOrigin::signed(1), 0, 42, bvec![0u8; 15], true));
		assert_noop!(
			Uniques::set_metadata(RuntimeOrigin::signed(1), 0, 42, bvec![0u8; 15], false),
			Error::<Test, _>::Frozen,
		);
		assert_noop!(
			Uniques::clear_metadata(RuntimeOrigin::signed(1), 0, 42),
			Error::<Test>::Frozen
		);

		// Clear Metadata
		assert_ok!(Uniques::set_metadata(RuntimeOrigin::root(), 0, 42, bvec![0u8; 15], false));
		assert_noop!(
			Uniques::clear_metadata(RuntimeOrigin::signed(2), 0, 42),
			Error::<Test>::NoPermission
		);
		assert_noop!(
			Uniques::clear_metadata(RuntimeOrigin::signed(1), 1, 42),
			Error::<Test>::UnknownCollection
		);
		assert_ok!(Uniques::clear_metadata(RuntimeOrigin::signed(1), 0, 42));
		assert!(!ItemMetadataOf::<Test>::contains_key(0, 42));
	});
}

#[test]
fn set_attribute_should_work() {
	new_test_ext().execute_with(|| {
		Balances::make_free_balance_be(&1, 100);

		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, 1, false));

		assert_ok!(Uniques::set_attribute(RuntimeOrigin::signed(1), 0, None, bvec![0], bvec![0]));
		assert_ok!(Uniques::set_attribute(
			RuntimeOrigin::signed(1),
			0,
			Some(0),
			bvec![0],
			bvec![0]
		));
		assert_ok!(Uniques::set_attribute(
			RuntimeOrigin::signed(1),
			0,
			Some(0),
			bvec![1],
			bvec![0]
		));
		assert_eq!(
			attributes(0),
			vec![
				(None, bvec![0], bvec![0]),
				(Some(0), bvec![0], bvec![0]),
				(Some(0), bvec![1], bvec![0]),
			]
		);
		assert_eq!(Balances::reserved_balance(1), 9);

		assert_ok!(Uniques::set_attribute(
			RuntimeOrigin::signed(1),
			0,
			None,
			bvec![0],
			bvec![0; 10]
		));
		assert_eq!(
			attributes(0),
			vec![
				(None, bvec![0], bvec![0; 10]),
				(Some(0), bvec![0], bvec![0]),
				(Some(0), bvec![1], bvec![0]),
			]
		);
		assert_eq!(Balances::reserved_balance(1), 18);

		assert_ok!(Uniques::clear_attribute(RuntimeOrigin::signed(1), 0, Some(0), bvec![1]));
		assert_eq!(
			attributes(0),
			vec![(None, bvec![0], bvec![0; 10]), (Some(0), bvec![0], bvec![0]),]
		);
		assert_eq!(Balances::reserved_balance(1), 15);

		let w = Collection::<Test>::get(0).unwrap().destroy_witness();
		assert_ok!(Uniques::destroy(RuntimeOrigin::signed(1), 0, w));
		assert_eq!(attributes(0), vec![]);
		assert_eq!(Balances::reserved_balance(1), 0);
	});
}

#[test]
fn set_attribute_should_respect_freeze() {
	new_test_ext().execute_with(|| {
		Balances::make_free_balance_be(&1, 100);

		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, 1, false));

		assert_ok!(Uniques::set_attribute(RuntimeOrigin::signed(1), 0, None, bvec![0], bvec![0]));
		assert_ok!(Uniques::set_attribute(
			RuntimeOrigin::signed(1),
			0,
			Some(0),
			bvec![0],
			bvec![0]
		));
		assert_ok!(Uniques::set_attribute(
			RuntimeOrigin::signed(1),
			0,
			Some(1),
			bvec![0],
			bvec![0]
		));
		assert_eq!(
			attributes(0),
			vec![
				(None, bvec![0], bvec![0]),
				(Some(0), bvec![0], bvec![0]),
				(Some(1), bvec![0], bvec![0]),
			]
		);
		assert_eq!(Balances::reserved_balance(1), 9);

		assert_ok!(Uniques::set_collection_metadata(RuntimeOrigin::signed(1), 0, bvec![], true));
		let e = Error::<Test>::Frozen;
		assert_noop!(
			Uniques::set_attribute(RuntimeOrigin::signed(1), 0, None, bvec![0], bvec![0]),
			e
		);
		assert_ok!(Uniques::set_attribute(
			RuntimeOrigin::signed(1),
			0,
			Some(0),
			bvec![0],
			bvec![1]
		));

		assert_ok!(Uniques::set_metadata(RuntimeOrigin::signed(1), 0, 0, bvec![], true));
		let e = Error::<Test>::Frozen;
		assert_noop!(
			Uniques::set_attribute(RuntimeOrigin::signed(1), 0, Some(0), bvec![0], bvec![1]),
			e
		);
		assert_ok!(Uniques::set_attribute(
			RuntimeOrigin::signed(1),
			0,
			Some(1),
			bvec![0],
			bvec![1]
		));
	});
}

#[test]
fn force_item_status_should_work() {
	new_test_ext().execute_with(|| {
		Balances::make_free_balance_be(&1, 100);

		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, 1, false));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, 42, 1));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, 69, 2));
		assert_ok!(Uniques::set_collection_metadata(
			RuntimeOrigin::signed(1),
			0,
			bvec![0; 20],
			false
		));
		assert_ok!(Uniques::set_metadata(RuntimeOrigin::signed(1), 0, 42, bvec![0; 20], false));
		assert_ok!(Uniques::set_metadata(RuntimeOrigin::signed(1), 0, 69, bvec![0; 20], false));
		assert_eq!(Balances::reserved_balance(1), 65);

		// force item status to be free holding
		assert_ok!(Uniques::force_item_status(RuntimeOrigin::root(), 0, 1, 1, 1, 1, true, false));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, 142, 1));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, 169, 2));
		assert_ok!(Uniques::set_metadata(RuntimeOrigin::signed(1), 0, 142, bvec![0; 20], false));
		assert_ok!(Uniques::set_metadata(RuntimeOrigin::signed(1), 0, 169, bvec![0; 20], false));
		assert_eq!(Balances::reserved_balance(1), 65);

		assert_ok!(Uniques::redeposit(RuntimeOrigin::signed(1), 0, bvec![0, 42, 50, 69, 100]));
		assert_eq!(Balances::reserved_balance(1), 63);

		assert_ok!(Uniques::set_metadata(RuntimeOrigin::signed(1), 0, 42, bvec![0; 20], false));
		assert_eq!(Balances::reserved_balance(1), 42);

		assert_ok!(Uniques::set_metadata(RuntimeOrigin::signed(1), 0, 69, bvec![0; 20], false));
		assert_eq!(Balances::reserved_balance(1), 21);

		assert_ok!(Uniques::set_collection_metadata(
			RuntimeOrigin::signed(1),
			0,
			bvec![0; 20],
			false
		));
		assert_eq!(Balances::reserved_balance(1), 0);
	});
}

#[test]
fn burn_works() {
	new_test_ext().execute_with(|| {
		Balances::make_free_balance_be(&1, 100);
		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, 1, false));
		assert_ok!(Uniques::set_team(RuntimeOrigin::signed(1), 0, 2, 3, 4));

		assert_noop!(
			Uniques::burn(RuntimeOrigin::signed(5), 0, 42, Some(5)),
			Error::<Test>::UnknownCollection
		);

		assert_ok!(Uniques::mint(RuntimeOrigin::signed(2), 0, 42, 5));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(2), 0, 69, 5));
		assert_eq!(Balances::reserved_balance(1), 2);

		assert_noop!(
			Uniques::burn(RuntimeOrigin::signed(0), 0, 42, None),
			Error::<Test>::NoPermission
		);
		assert_noop!(
			Uniques::burn(RuntimeOrigin::signed(5), 0, 42, Some(6)),
			Error::<Test>::WrongOwner
		);

		assert_ok!(Uniques::burn(RuntimeOrigin::signed(5), 0, 42, Some(5)));
		assert_ok!(Uniques::burn(RuntimeOrigin::signed(3), 0, 69, Some(5)));
		assert_eq!(Balances::reserved_balance(1), 0);
	});
}

#[test]
fn approval_lifecycle_works() {
	new_test_ext().execute_with(|| {
		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, 1, true));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, 42, 2));
		assert_ok!(Uniques::approve_transfer(RuntimeOrigin::signed(2), 0, 42, 3));
		assert_ok!(Uniques::transfer(RuntimeOrigin::signed(3), 0, 42, 4));
		assert_noop!(
			Uniques::transfer(RuntimeOrigin::signed(3), 0, 42, 3),
			Error::<Test>::NoPermission
		);
		assert!(Item::<Test>::get(0, 42).unwrap().approved.is_none());

		assert_ok!(Uniques::approve_transfer(RuntimeOrigin::signed(4), 0, 42, 2));
		assert_ok!(Uniques::transfer(RuntimeOrigin::signed(2), 0, 42, 2));
	});
}

#[test]
fn approved_account_gets_reset_after_transfer() {
	new_test_ext().execute_with(|| {
		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, 1, true));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, 42, 2));

		assert_ok!(Uniques::approve_transfer(RuntimeOrigin::signed(2), 0, 42, 3));
		assert_ok!(Uniques::transfer(RuntimeOrigin::signed(2), 0, 42, 5));

		// this shouldn't work because we have just transferred the item to another account.
		assert_noop!(
			Uniques::transfer(RuntimeOrigin::signed(3), 0, 42, 4),
			Error::<Test>::NoPermission
		);
		// The new owner can transfer fine:
		assert_ok!(Uniques::transfer(RuntimeOrigin::signed(5), 0, 42, 6));
	});
}

#[test]
fn approved_account_gets_reset_after_buy_item() {
	new_test_ext().execute_with(|| {
		let item = 1;
		let price = 15;

		Balances::make_free_balance_be(&2, 100);

		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, 1, true));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, item, 1));
		assert_ok!(Uniques::approve_transfer(RuntimeOrigin::signed(1), 0, item, 5));

		assert_ok!(Uniques::set_price(RuntimeOrigin::signed(1), 0, item, Some(price), None));

		assert_ok!(Uniques::buy_item(RuntimeOrigin::signed(2), 0, item, price));

		// this shouldn't work because the item has been bough and the approved account should be
		// reset.
		assert_noop!(
			Uniques::transfer(RuntimeOrigin::signed(5), 0, item, 4),
			Error::<Test>::NoPermission
		);
	});
}

#[test]
fn cancel_approval_works() {
	new_test_ext().execute_with(|| {
		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, 1, true));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, 42, 2));

		assert_ok!(Uniques::approve_transfer(RuntimeOrigin::signed(2), 0, 42, 3));
		assert_noop!(
			Uniques::cancel_approval(RuntimeOrigin::signed(2), 1, 42, None),
			Error::<Test>::UnknownCollection
		);
		assert_noop!(
			Uniques::cancel_approval(RuntimeOrigin::signed(2), 0, 43, None),
			Error::<Test>::UnknownCollection
		);
		assert_noop!(
			Uniques::cancel_approval(RuntimeOrigin::signed(3), 0, 42, None),
			Error::<Test>::NoPermission
		);
		assert_noop!(
			Uniques::cancel_approval(RuntimeOrigin::signed(2), 0, 42, Some(4)),
			Error::<Test>::WrongDelegate
		);

		assert_ok!(Uniques::cancel_approval(RuntimeOrigin::signed(2), 0, 42, Some(3)));
		assert_noop!(
			Uniques::cancel_approval(RuntimeOrigin::signed(2), 0, 42, None),
			Error::<Test>::NoDelegate
		);
	});
}

#[test]
fn cancel_approval_works_with_admin() {
	new_test_ext().execute_with(|| {
		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, 1, true));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, 42, 2));

		assert_ok!(Uniques::approve_transfer(RuntimeOrigin::signed(2), 0, 42, 3));
		assert_noop!(
			Uniques::cancel_approval(RuntimeOrigin::signed(1), 1, 42, None),
			Error::<Test>::UnknownCollection
		);
		assert_noop!(
			Uniques::cancel_approval(RuntimeOrigin::signed(1), 0, 43, None),
			Error::<Test>::UnknownCollection
		);
		assert_noop!(
			Uniques::cancel_approval(RuntimeOrigin::signed(1), 0, 42, Some(4)),
			Error::<Test>::WrongDelegate
		);

		assert_ok!(Uniques::cancel_approval(RuntimeOrigin::signed(1), 0, 42, Some(3)));
		assert_noop!(
			Uniques::cancel_approval(RuntimeOrigin::signed(1), 0, 42, None),
			Error::<Test>::NoDelegate
		);
	});
}

#[test]
fn cancel_approval_works_with_force() {
	new_test_ext().execute_with(|| {
		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), 0, 1, true));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(1), 0, 42, 2));

		assert_ok!(Uniques::approve_transfer(RuntimeOrigin::signed(2), 0, 42, 3));
		assert_noop!(
			Uniques::cancel_approval(RuntimeOrigin::root(), 1, 42, None),
			Error::<Test>::UnknownCollection
		);
		assert_noop!(
			Uniques::cancel_approval(RuntimeOrigin::root(), 0, 43, None),
			Error::<Test>::UnknownCollection
		);
		assert_noop!(
			Uniques::cancel_approval(RuntimeOrigin::root(), 0, 42, Some(4)),
			Error::<Test>::WrongDelegate
		);

		assert_ok!(Uniques::cancel_approval(RuntimeOrigin::root(), 0, 42, Some(3)));
		assert_noop!(
			Uniques::cancel_approval(RuntimeOrigin::root(), 0, 42, None),
			Error::<Test>::NoDelegate
		);
	});
}

#[test]
fn max_supply_should_work() {
	new_test_ext().execute_with(|| {
		let collection_id = 0;
		let user_id = 1;
		let max_supply = 2;

		// validate set_collection_max_supply
		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), collection_id, user_id, true));
		assert!(!CollectionMaxSupply::<Test>::contains_key(collection_id));

		assert_ok!(Uniques::set_collection_max_supply(
			RuntimeOrigin::signed(user_id),
			collection_id,
			max_supply
		));
		assert_eq!(CollectionMaxSupply::<Test>::get(collection_id).unwrap(), max_supply);

		assert!(events().contains(&Event::<Test>::CollectionMaxSupplySet {
			collection: collection_id,
			max_supply,
		}));

		assert_noop!(
			Uniques::set_collection_max_supply(
				RuntimeOrigin::signed(user_id),
				collection_id,
				max_supply + 1
			),
			Error::<Test>::MaxSupplyAlreadySet
		);

		// validate we can't mint more to max supply
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(user_id), collection_id, 0, user_id));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(user_id), collection_id, 1, user_id));
		assert_noop!(
			Uniques::mint(RuntimeOrigin::signed(user_id), collection_id, 2, user_id),
			Error::<Test>::MaxSupplyReached
		);

		// validate we remove the CollectionMaxSupply record when we destroy the collection
		assert_ok!(Uniques::destroy(
			RuntimeOrigin::signed(user_id),
			collection_id,
			Collection::<Test>::get(collection_id).unwrap().destroy_witness()
		));
		assert!(!CollectionMaxSupply::<Test>::contains_key(collection_id));
	});
}

#[test]
fn set_price_should_work() {
	new_test_ext().execute_with(|| {
		let user_id = 1;
		let collection_id = 0;
		let item_1 = 1;
		let item_2 = 2;

		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), collection_id, user_id, true));

		assert_ok!(Uniques::mint(RuntimeOrigin::signed(user_id), collection_id, item_1, user_id));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(user_id), collection_id, item_2, user_id));

		assert_ok!(Uniques::set_price(
			RuntimeOrigin::signed(user_id),
			collection_id,
			item_1,
			Some(1),
			None,
		));

		assert_ok!(Uniques::set_price(
			RuntimeOrigin::signed(user_id),
			collection_id,
			item_2,
			Some(2),
			Some(3)
		));

		let item = ItemPriceOf::<Test>::get(collection_id, item_1).unwrap();
		assert_eq!(item.0, 1);
		assert_eq!(item.1, None);

		let item = ItemPriceOf::<Test>::get(collection_id, item_2).unwrap();
		assert_eq!(item.0, 2);
		assert_eq!(item.1, Some(3));

		assert!(events().contains(&Event::<Test>::ItemPriceSet {
			collection: collection_id,
			item: item_1,
			price: 1,
			whitelisted_buyer: None,
		}));

		// validate we can unset the price
		assert_ok!(Uniques::set_price(
			RuntimeOrigin::signed(user_id),
			collection_id,
			item_2,
			None,
			None
		));
		assert!(events().contains(&Event::<Test>::ItemPriceRemoved {
			collection: collection_id,
			item: item_2
		}));
		assert!(!ItemPriceOf::<Test>::contains_key(collection_id, item_2));
	});
}

#[test]
fn buy_item_should_work() {
	new_test_ext().execute_with(|| {
		let user_1 = 1;
		let user_2 = 2;
		let user_3 = 3;
		let collection_id = 0;
		let item_1 = 1;
		let item_2 = 2;
		let item_3 = 3;
		let price_1 = 20;
		let price_2 = 30;
		let initial_balance = 100;

		Balances::make_free_balance_be(&user_1, initial_balance);
		Balances::make_free_balance_be(&user_2, initial_balance);
		Balances::make_free_balance_be(&user_3, initial_balance);

		assert_ok!(Uniques::force_create(RuntimeOrigin::root(), collection_id, user_1, true));

		assert_ok!(Uniques::mint(RuntimeOrigin::signed(user_1), collection_id, item_1, user_1));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(user_1), collection_id, item_2, user_1));
		assert_ok!(Uniques::mint(RuntimeOrigin::signed(user_1), collection_id, item_3, user_1));

		assert_ok!(Uniques::set_price(
			RuntimeOrigin::signed(user_1),
			collection_id,
			item_1,
			Some(price_1),
			None,
		));

		assert_ok!(Uniques::set_price(
			RuntimeOrigin::signed(user_1),
			collection_id,
			item_2,
			Some(price_2),
			Some(user_3),
		));

		// can't buy for less
		assert_noop!(
			Uniques::buy_item(RuntimeOrigin::signed(user_2), collection_id, item_1, 1),
			Error::<Test>::BidTooLow
		);

		// pass the higher price to validate it will still deduct correctly
		assert_ok!(Uniques::buy_item(
			RuntimeOrigin::signed(user_2),
			collection_id,
			item_1,
			price_1 + 1,
		));

		// validate the new owner & balances
		let item = Item::<Test>::get(collection_id, item_1).unwrap();
		assert_eq!(item.owner, user_2);
		assert_eq!(Balances::total_balance(&user_1), initial_balance + price_1);
		assert_eq!(Balances::total_balance(&user_2), initial_balance - price_1);

		// can't buy from yourself
		assert_noop!(
			Uniques::buy_item(RuntimeOrigin::signed(user_1), collection_id, item_2, price_2),
			Error::<Test>::NoPermission
		);

		// can't buy when the item is listed for a specific buyer
		assert_noop!(
			Uniques::buy_item(RuntimeOrigin::signed(user_2), collection_id, item_2, price_2),
			Error::<Test>::NoPermission
		);

		// can buy when I'm a whitelisted buyer
		assert_ok!(Uniques::buy_item(
			RuntimeOrigin::signed(user_3),
			collection_id,
			item_2,
			price_2,
		));

		assert!(events().contains(&Event::<Test>::ItemBought {
			collection: collection_id,
			item: item_2,
			price: price_2,
			seller: user_1,
			buyer: user_3,
		}));

		// ensure we reset the buyer field
		assert!(!ItemPriceOf::<Test>::contains_key(collection_id, item_2));

		// can't buy when item is not for sale
		assert_noop!(
			Uniques::buy_item(RuntimeOrigin::signed(user_2), collection_id, item_3, price_2),
			Error::<Test>::NotForSale
		);

		// ensure we can't buy an item when the collection or an item is frozen
		{
			assert_ok!(Uniques::set_price(
				RuntimeOrigin::signed(user_1),
				collection_id,
				item_3,
				Some(price_1),
				None,
			));

			// freeze collection
			assert_ok!(Uniques::freeze_collection(RuntimeOrigin::signed(user_1), collection_id));

			let buy_item_call = mock::RuntimeCall::Uniques(crate::Call::<Test>::buy_item {
				collection: collection_id,
				item: item_3,
				bid_price: price_1,
			});
			assert_noop!(
				buy_item_call.dispatch(RuntimeOrigin::signed(user_2)),
				Error::<Test>::Frozen
			);

			assert_ok!(Uniques::thaw_collection(RuntimeOrigin::signed(user_1), collection_id));

			// freeze item
			assert_ok!(Uniques::freeze(RuntimeOrigin::signed(user_1), collection_id, item_3));

			let buy_item_call = mock::RuntimeCall::Uniques(crate::Call::<Test>::buy_item {
				collection: collection_id,
				item: item_3,
				bid_price: price_1,
			});
			assert_noop!(
				buy_item_call.dispatch(RuntimeOrigin::signed(user_2)),
				Error::<Test>::Frozen
			);
		}
	});
}

#[test]
fn clear_collection_metadata_works() {
	new_test_ext().execute_with(|| {
		// Start with an account with 100 balance, 10 of which are reserved
		Balances::make_free_balance_be(&1, 100);
		Balances::reserve(&1, 10).unwrap();

		// Create a Unique which increases total_deposit by 2
		assert_ok!(Uniques::create(RuntimeOrigin::signed(1), 0, 123));
		assert_eq!(Collection::<Test>::get(0).unwrap().total_deposit, 2);
		assert_eq!(Balances::reserved_balance(&1), 12);

		// Set collection metadata which increases total_deposit by 10
		assert_ok!(Uniques::set_collection_metadata(
			RuntimeOrigin::signed(1),
			0,
			bvec![0, 0, 0, 0, 0, 0, 0, 0, 0],
			false
		));
		assert_eq!(Collection::<Test>::get(0).unwrap().total_deposit, 12);
		assert_eq!(Balances::reserved_balance(&1), 22);

		// Clearing collection metadata reduces total_deposit by the expected amount
		assert_ok!(Uniques::clear_collection_metadata(RuntimeOrigin::signed(1), 0));
		assert_eq!(Collection::<Test>::get(0).unwrap().total_deposit, 2);
		assert_eq!(Balances::reserved_balance(&1), 12);

		// Destroying the collection removes it from storage
		assert_ok!(Uniques::destroy(
			RuntimeOrigin::signed(1),
			0,
			DestroyWitness { items: 0, item_metadatas: 0, attributes: 0 }
		));
		assert_eq!(Collection::<Test>::get(0), None);
		assert_eq!(Balances::reserved_balance(&1), 10);
	});
}

mod asset_ops_tests {
	use super::*;
	use crate::asset_strategies::*;
	use frame_support::traits::tokens::asset_ops::{common_strategies::*, *};

	type Collection = asset_ops::Collection<Uniques>;
	type Item = asset_ops::Item<Uniques>;

	#[test]
	fn create_collection() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;

			let collection_owner = 1;
			let collection_admin = 2;

			Balances::make_free_balance_be(&collection_owner, 100);

			assert_ok!(Collection::create(WithConfig::new(
				(
					Owner::with_config_value(collection_owner),
					Admin::with_config_value(collection_admin)
				),
				PredefinedId::from(collection_id),
			)));

			assert_eq!(collections(), vec![(collection_owner, collection_id)]);
		});
	}

	#[test]
	fn create_collection_check_origin() {
		new_test_ext().execute_with(|| {
			let alice = 1;
			let bob = 2;
			let collection_admin = 3;

			Balances::make_free_balance_be(&alice, 100);
			Balances::make_free_balance_be(&bob, 100);

			// Signed origin, same owner
			assert_ok!(Collection::create(CheckOrigin(
				RuntimeOrigin::signed(alice),
				WithConfig::new(
					(Owner::with_config_value(alice), Admin::with_config_value(collection_admin)),
					PredefinedId::from(0),
				),
			)));

			// Signed origin, different owner
			assert_noop!(
				Collection::create(CheckOrigin(
					RuntimeOrigin::signed(alice),
					WithConfig::new(
						(Owner::with_config_value(bob), Admin::with_config_value(collection_admin)),
						PredefinedId::from(1),
					),
				)),
				Error::<Test>::NoPermission,
			);

			// Root origin, any owner
			assert_ok!(Collection::create(CheckOrigin(
				RuntimeOrigin::root(),
				WithConfig::new(
					(Owner::with_config_value(alice), Admin::with_config_value(collection_admin)),
					PredefinedId::from(2)
				),
			)));

			assert_ok!(Collection::create(CheckOrigin(
				RuntimeOrigin::root(),
				WithConfig::new(
					(Owner::with_config_value(bob), Admin::with_config_value(collection_admin)),
					PredefinedId::from(3)
				),
			)));
		});
	}

	#[test]
	fn destroy_collection_with_witness() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;

			let collection_owner = 1;
			let collection_admin = 2;
			let item_owner = 3;

			Balances::make_free_balance_be(&collection_owner, 100);

			assert_ok!(Collection::create(WithConfig::new(
				(
					Owner::with_config_value(collection_owner),
					Admin::with_config_value(collection_admin)
				),
				PredefinedId::from(collection_id),
			)));

			assert_ok!(Item::create(WithConfig::new(
				Owner::with_config_value(item_owner),
				PredefinedId::from((collection_id, 0)),
			)));

			let outdated_witness =
				crate::Collection::<Test>::get(collection_id).unwrap().destroy_witness();

			assert_ok!(Item::create(WithConfig::new(
				Owner::with_config_value(item_owner),
				PredefinedId::from((collection_id, 1)),
			)));

			let ok_witness =
				crate::Collection::<Test>::get(collection_id).unwrap().destroy_witness();

			assert_noop!(
				Collection::destroy(&collection_id, WithWitness::check(outdated_witness)),
				Error::<Test>::BadWitness,
			);

			assert_ok!(Collection::destroy(&collection_id, WithWitness::check(ok_witness)));

			assert_eq!(collections(), vec![]);
			assert_eq!(items(), vec![]);
		});
	}

	#[test]
	fn destroy_collection_if_owned_by_and_with_witness() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;

			let collection_owner = 1;
			let collection_admin = 2;
			let item_owner = 3;

			Balances::make_free_balance_be(&collection_owner, 100);

			assert_ok!(Collection::create(WithConfig::new(
				(
					Owner::with_config_value(collection_owner),
					Admin::with_config_value(collection_admin)
				),
				PredefinedId::from(collection_id),
			)));

			assert_ok!(Item::create(WithConfig::new(
				Owner::with_config_value(item_owner),
				PredefinedId::from((collection_id, 0)),
			)));

			let outdated_witness =
				crate::Collection::<Test>::get(collection_id).unwrap().destroy_witness();

			assert_ok!(Item::create(WithConfig::new(
				Owner::with_config_value(item_owner),
				PredefinedId::from((collection_id, 1)),
			)));

			let ok_witness =
				crate::Collection::<Test>::get(collection_id).unwrap().destroy_witness();

			assert_noop!(
				Collection::destroy(
					&collection_id,
					IfOwnedBy::new(collection_admin, WithWitness::check(ok_witness)),
				),
				Error::<Test>::NoPermission,
			);

			// A bad witness is rejected even for the owner
			assert_noop!(
				Collection::destroy(
					&collection_id,
					IfOwnedBy::new(collection_owner, WithWitness::check(outdated_witness)),
				),
				Error::<Test>::BadWitness,
			);

			assert_ok!(Collection::destroy(
				&collection_id,
				IfOwnedBy::new(collection_owner, WithWitness::check(ok_witness)),
			));

			assert_eq!(collections(), vec![]);
			assert_eq!(items(), vec![]);
		});
	}

	#[test]
	fn destroy_collection_check_origin() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;

			let collection_owner = 1;
			let collection_admin = 2;
			let item_owner = 3;

			Balances::make_free_balance_be(&collection_owner, 100);

			let setup_test_collection = || {
				assert_ok!(Collection::create(WithConfig::new(
					(
						Owner::with_config_value(collection_owner),
						Admin::with_config_value(collection_admin),
					),
					PredefinedId::from(collection_id),
				)));

				assert_ok!(Item::create(WithConfig::new(
					Owner::with_config_value(item_owner),
					PredefinedId::from((collection_id, 0)),
				)));

				let outdated_witness =
					crate::Collection::<Test>::get(collection_id).unwrap().destroy_witness();

				assert_ok!(Item::create(WithConfig::new(
					Owner::with_config_value(item_owner),
					PredefinedId::from((collection_id, 1)),
				)));

				let ok_witness =
					crate::Collection::<Test>::get(collection_id).unwrap().destroy_witness();

				(outdated_witness, ok_witness)
			};

			let (outdated_witness, ok_witness) = setup_test_collection();

			// Not an owner signed origin is rejected even if the `owner` parameter is correct
			assert_noop!(
				Collection::destroy(
					&collection_id,
					CheckOrigin(
						RuntimeOrigin::signed(collection_admin),
						WithWitness::check(ok_witness)
					),
				),
				Error::<Test>::NoPermission,
			);

			// A bad witness is rejected even for the owner
			assert_noop!(
				Collection::destroy(
					&collection_id,
					CheckOrigin(
						RuntimeOrigin::signed(collection_owner),
						WithWitness::check(outdated_witness),
					),
				),
				Error::<Test>::BadWitness,
			);

			assert_ok!(Collection::destroy(
				&collection_id,
				CheckOrigin(
					RuntimeOrigin::signed(collection_owner),
					WithWitness::check(ok_witness)
				),
			));

			assert_eq!(collections(), vec![]);
			assert_eq!(items(), vec![]);

			// Recreate the collection to the the root origin
			let (outdated_witness, ok_witness) = setup_test_collection();

			// A bad witness is rejected even for the root origin
			assert_noop!(
				Collection::destroy(
					&collection_id,
					CheckOrigin(RuntimeOrigin::root(), WithWitness::check(outdated_witness)),
				),
				Error::<Test>::BadWitness,
			);

			assert_ok!(Collection::destroy(
				&collection_id,
				CheckOrigin(RuntimeOrigin::root(), WithWitness::check(ok_witness)),
			));

			assert_eq!(collections(), vec![]);
			assert_eq!(items(), vec![]);
		});
	}

	#[test]
	fn inspect_collection_ownership() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;

			let collection_owner = 1;
			let collection_admin = 2;

			Balances::make_free_balance_be(&collection_owner, 100);

			assert_ok!(Collection::create(WithConfig::new(
				(
					Owner::with_config_value(collection_owner),
					Admin::with_config_value(collection_admin)
				),
				PredefinedId::from(collection_id),
			)));

			let retrieved_collection_owner =
				Collection::inspect(&collection_id, Owner::default()).unwrap();

			assert_eq!(retrieved_collection_owner, collection_owner);
		});
	}

	#[test]
	fn inspect_collection_metadata() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;

			let collection_owner = 1;
			let collection_admin = 2;

			Balances::make_free_balance_be(&collection_owner, 100);

			assert_ok!(Collection::create(WithConfig::new(
				(
					Owner::with_config_value(collection_owner),
					Admin::with_config_value(collection_admin)
				),
				PredefinedId::from(collection_id),
			)));

			assert_noop!(
				Collection::inspect(&collection_id, Bytes::default()),
				Error::<Test>::NoMetadata,
			);

			let metadata = vec![0xB, 0xE, 0xE, 0xF];
			let is_frozen = false;
			assert_ok!(Uniques::set_collection_metadata(
				RuntimeOrigin::signed(collection_owner),
				collection_id,
				metadata.clone().try_into().unwrap(),
				is_frozen,
			));

			let retreived_metadata = Collection::inspect(&collection_id, Bytes::default()).unwrap();

			assert_eq!(retreived_metadata, metadata);

			assert_ok!(Uniques::clear_collection_metadata(
				RuntimeOrigin::signed(collection_owner),
				collection_id,
			));

			assert_noop!(
				Collection::inspect(&collection_id, Bytes::default()),
				Error::<Test>::NoMetadata,
			);
		});
	}

	#[test]
	fn inspect_collection_attributes() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;

			let collection_owner = 1;
			let collection_admin = 2;

			let food_attr_key = vec![0xB, 0xE, 0xE, 0xF];
			let food_attr_value = vec![0xC, 0x0, 0x0, 0x1];

			let drink_attr_key = vec![0xC, 0x0, 0xF, 0xF, 0xE, 0xE];
			let drink_attr_value = vec![0xD, 0xE, 0xC, 0xA, 0xF];

			Balances::make_free_balance_be(&collection_owner, 100);

			let set_attribute = |key: &Vec<u8>, value: &Vec<u8>| {
				let item_id = None;

				assert_ok!(Uniques::set_attribute(
					RuntimeOrigin::signed(collection_owner),
					collection_id,
					item_id,
					key.clone().try_into().unwrap(),
					value.clone().try_into().unwrap(),
				));
			};

			let clear_attribute = |key: &Vec<u8>| {
				let item_id = None;

				assert_ok!(Uniques::clear_attribute(
					RuntimeOrigin::signed(collection_owner),
					collection_id,
					item_id,
					key.clone().try_into().unwrap(),
				));
			};

			assert_ok!(Collection::create(WithConfig::new(
				(
					Owner::with_config_value(collection_owner),
					Admin::with_config_value(collection_admin)
				),
				PredefinedId::from(collection_id),
			)));

			assert_noop!(
				Collection::inspect(&collection_id, Bytes(Attribute(food_attr_key.as_slice()))),
				Error::<Test>::AttributeNotFound,
			);

			assert_noop!(
				Collection::inspect(&collection_id, Bytes(Attribute(drink_attr_key.as_slice()))),
				Error::<Test>::AttributeNotFound,
			);

			set_attribute(&food_attr_key, &food_attr_value);

			let retreived_food_value =
				Collection::inspect(&collection_id, Bytes(Attribute(food_attr_key.as_slice())))
					.unwrap();

			assert_eq!(retreived_food_value, food_attr_value);

			assert_noop!(
				Collection::inspect(&collection_id, Bytes(Attribute(drink_attr_key.as_slice()))),
				Error::<Test>::AttributeNotFound,
			);

			set_attribute(&drink_attr_key, &drink_attr_value);

			let retreived_food_value =
				Collection::inspect(&collection_id, Bytes(Attribute(food_attr_key.as_slice())))
					.unwrap();

			assert_eq!(retreived_food_value, food_attr_value);

			let retreived_drink_value =
				Collection::inspect(&collection_id, Bytes(Attribute(drink_attr_key.as_slice())))
					.unwrap();

			assert_eq!(retreived_drink_value, drink_attr_value);

			clear_attribute(&food_attr_key);

			assert_noop!(
				Collection::inspect(&collection_id, Bytes(Attribute(food_attr_key.as_slice()))),
				Error::<Test>::AttributeNotFound,
			);

			let retreived_drink_value =
				Collection::inspect(&collection_id, Bytes(Attribute(drink_attr_key.as_slice())))
					.unwrap();

			assert_eq!(retreived_drink_value, drink_attr_value);

			clear_attribute(&drink_attr_key);

			assert_noop!(
				Collection::inspect(&collection_id, Bytes(Attribute(food_attr_key.as_slice()))),
				Error::<Test>::AttributeNotFound,
			);

			assert_noop!(
				Collection::inspect(&collection_id, Bytes(Attribute(drink_attr_key.as_slice()))),
				Error::<Test>::AttributeNotFound,
			);
		});
	}

	#[test]
	fn mint_item() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;
			let item_id = 111;

			let collection_owner = 1;
			let collection_admin = 2;
			let item_owner = 3;

			Balances::make_free_balance_be(&collection_owner, 100);

			assert_ok!(Collection::create(WithConfig::new(
				(
					Owner::with_config_value(collection_owner),
					Admin::with_config_value(collection_admin)
				),
				PredefinedId::from(collection_id),
			)));

			assert_eq!(collections(), vec![(collection_owner, collection_id)]);

			assert_ok!(Item::create(WithConfig::new(
				Owner::with_config_value(item_owner),
				PredefinedId::from((collection_id, item_id)),
			)));

			assert_eq!(items(), vec![(item_owner, collection_id, item_id)]);
		});
	}

	#[test]
	fn mint_item_by_admin() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;
			let item_id = 111;

			let collection_owner = 1;
			let collection_admin = 2;
			let item_owner = 3;

			Balances::make_free_balance_be(&collection_owner, 100);

			assert_ok!(Collection::create(WithConfig::new(
				(
					Owner::with_config_value(collection_owner),
					Admin::with_config_value(collection_admin)
				),
				PredefinedId::from(collection_id),
			)));

			assert_ok!(Uniques::mint(
				RuntimeOrigin::signed(collection_admin),
				collection_id,
				item_id,
				item_owner,
			));

			assert_eq!(items(), vec![(item_owner, collection_id, item_id)]);
		});
	}

	#[test]
	fn mint_item_check_origin() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;
			let item_id = 111;

			let collection_owner = 1;
			let collection_admin = 2;
			let item_owner = 3;

			Balances::make_free_balance_be(&collection_owner, 100);

			assert_ok!(Collection::create(WithConfig::new(
				(
					Owner::with_config_value(collection_owner),
					Admin::with_config_value(collection_admin)
				),
				PredefinedId::from(collection_id),
			)));

			// Not an admin (not an issuer) can't mint new tokens
			assert_noop!(
				Item::create(CheckOrigin(
					RuntimeOrigin::signed(collection_owner),
					WithConfig::new(
						Owner::with_config_value(item_owner),
						PredefinedId::from((collection_id, item_id)),
					),
				)),
				Error::<Test>::NoPermission,
			);

			// Force origin doesn't affect minting: only the admin (the issuer) can mint
			assert_noop!(
				Item::create(CheckOrigin(
					RuntimeOrigin::root(),
					WithConfig::new(
						Owner::with_config_value(item_owner),
						PredefinedId::from((collection_id, item_id)),
					),
				)),
				DispatchError::BadOrigin,
			);

			assert_ok!(Item::create(CheckOrigin(
				RuntimeOrigin::signed(collection_admin),
				WithConfig::new(
					Owner::with_config_value(item_owner),
					PredefinedId::from((collection_id, item_id)),
				),
			)));

			assert_eq!(items(), vec![(item_owner, collection_id, item_id)]);
		});
	}

	#[test]
	fn transfer_item_unchecked() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;
			let item_id = 111;

			let collection_owner = 1;
			let collection_admin = 2;
			let alice = 3;
			let bob = 4;

			Balances::make_free_balance_be(&collection_owner, 100);

			assert_ok!(Collection::create(WithConfig::new(
				(
					Owner::with_config_value(collection_owner),
					Admin::with_config_value(collection_admin)
				),
				PredefinedId::from(collection_id),
			)));

			assert_ok!(Item::create(WithConfig::new(
				Owner::with_config_value(alice),
				PredefinedId::from((collection_id, item_id)),
			)));

			assert_eq!(items(), vec![(alice, collection_id, item_id)]);

			assert_ok!(Item::update(&(collection_id, item_id), Owner::default(), &bob,));

			assert_eq!(items(), vec![(bob, collection_id, item_id)]);
		});
	}

	#[test]
	fn transfer_item_check_origin() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;
			let item_id = 111;

			let collection_owner = 1;
			let collection_admin = 2;
			let alice = 3;
			let bob = 4;

			Balances::make_free_balance_be(&collection_owner, 100);

			assert_ok!(Collection::create(WithConfig::new(
				(
					Owner::with_config_value(collection_owner),
					Admin::with_config_value(collection_admin)
				),
				PredefinedId::from(collection_id),
			)));

			assert_ok!(Item::create(WithConfig::new(
				Owner::with_config_value(alice),
				PredefinedId::from((collection_id, item_id)),
			)));

			assert_eq!(items(), vec![(alice, collection_id, item_id)]);

			// Bob is not the admin, not the owner and he's not been approved to transfer Alice's
			// token
			assert_noop!(
				Item::update(
					&(collection_id, item_id),
					CheckOrigin(RuntimeOrigin::signed(bob), Owner::default()),
					&bob,
				),
				Error::<Test>::NoPermission,
			);

			// The force origin can't transfer tokens
			assert_noop!(
				Item::update(
					&(collection_id, item_id),
					CheckOrigin(RuntimeOrigin::root(), Owner::default()),
					&bob,
				),
				DispatchError::BadOrigin,
			);

			// The owner can transfer the token
			assert_ok!(Item::update(
				&(collection_id, item_id),
				CheckOrigin(RuntimeOrigin::signed(alice), Owner::default()),
				&bob,
			));

			assert_eq!(items(), vec![(bob, collection_id, item_id)]);

			// The admin can transfer the token
			assert_ok!(Item::update(
				&(collection_id, item_id),
				CheckOrigin(RuntimeOrigin::signed(collection_admin), Owner::default()),
				&alice,
			));

			assert_eq!(items(), vec![(alice, collection_id, item_id)]);

			// Approve Bob to transfer Alice's token
			assert_ok!(Uniques::approve_transfer(
				RuntimeOrigin::signed(alice),
				collection_id,
				item_id,
				bob,
			));

			// Now Bob can transfer Alice's token
			assert_ok!(Item::update(
				&(collection_id, item_id),
				CheckOrigin(RuntimeOrigin::signed(bob), Owner::default()),
				&bob,
			));

			assert_eq!(items(), vec![(bob, collection_id, item_id)]);
		});
	}

	#[test]
	fn transfer_item_from_to() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;
			let item_id = 111;

			let collection_owner = 1;
			let collection_admin = 2;
			let alice = 3;
			let bob = 4;

			Balances::make_free_balance_be(&collection_owner, 100);

			assert_ok!(Collection::create(WithConfig::new(
				(
					Owner::with_config_value(collection_owner),
					Admin::with_config_value(collection_admin)
				),
				PredefinedId::from(collection_id),
			)));

			assert_ok!(Item::create(WithConfig::new(
				Owner::with_config_value(alice),
				PredefinedId::from((collection_id, item_id)),
			)));

			assert_eq!(items(), vec![(alice, collection_id, item_id)]);

			assert_ok!(Item::update(
				&(collection_id, item_id),
				ChangeOwnerFrom::check(alice),
				&bob,
			));

			assert_eq!(items(), vec![(bob, collection_id, item_id)]);

			assert_noop!(
				Item::update(&(collection_id, item_id), ChangeOwnerFrom::check(alice), &bob,),
				Error::<Test>::WrongOwner,
			);

			assert_ok!(Item::update(
				&(collection_id, item_id),
				ChangeOwnerFrom::check(bob),
				&alice,
			));

			assert_eq!(items(), vec![(alice, collection_id, item_id)]);
		});
	}

	#[test]
	fn stash_item_unchecked() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;
			let item_id = 111;

			let collection_owner = 1;
			let collection_admin = 2;
			let item_owner = 3;

			Balances::make_free_balance_be(&collection_owner, 100);

			assert_ok!(Collection::create(WithConfig::new(
				(
					Owner::with_config_value(collection_owner),
					Admin::with_config_value(collection_admin)
				),
				PredefinedId::from(collection_id),
			)));

			assert_ok!(Item::create(WithConfig::new(
				Owner::with_config_value(item_owner),
				PredefinedId::from((collection_id, item_id)),
			)));

			assert_eq!(items(), vec![(item_owner, collection_id, item_id)]);

			let test_key = vec![0xF, 0x0, 0x0, 0xD];
			let test_value = vec![0xC, 0x0, 0x0, 0x1];

			assert_ok!(Uniques::set_attribute(
				RuntimeOrigin::signed(collection_owner),
				collection_id,
				Some(item_id),
				test_key.clone().try_into().unwrap(),
				test_value.clone().try_into().unwrap(),
			));

			// Can't restore an already existing item
			assert_noop!(
				Item::restore(
					&(collection_id, item_id),
					WithConfig::from(Owner::with_config_value(item_owner)),
				),
				Error::<Test>::InUse,
			);

			assert_ok!(Item::stash(&(collection_id, item_id), NoParams));

			assert_eq!(items(), vec![]);

			let retreived_test_value =
				Item::inspect(&(collection_id, item_id), Bytes(Attribute(test_key.as_slice())))
					.unwrap();

			// the attributes are still available
			assert_eq!(retreived_test_value, test_value);

			// A stahed item can be restored
			assert_ok!(Item::restore(
				&(collection_id, item_id),
				WithConfig::from(Owner::with_config_value(item_owner)),
			));
			assert_eq!(items(), vec![(item_owner, collection_id, item_id)]);
		});
	}

	#[test]
	fn stash_item_if_owned_by() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;
			let item_id = 111;

			let collection_owner = 1;
			let collection_admin = 2;
			let item_owner = 3;

			Balances::make_free_balance_be(&collection_owner, 100);

			assert_ok!(Collection::create(WithConfig::new(
				(
					Owner::with_config_value(collection_owner),
					Admin::with_config_value(collection_admin)
				),
				PredefinedId::from(collection_id),
			)));

			assert_ok!(Item::create(WithConfig::new(
				Owner::with_config_value(item_owner),
				PredefinedId::from((collection_id, item_id)),
			)));

			assert_eq!(items(), vec![(item_owner, collection_id, item_id)]);

			let test_key = vec![0xF, 0x0, 0x0, 0xD];
			let test_value = vec![0xC, 0x0, 0x0, 0x1];

			assert_ok!(Uniques::set_attribute(
				RuntimeOrigin::signed(collection_owner),
				collection_id,
				Some(item_id),
				test_key.clone().try_into().unwrap(),
				test_value.clone().try_into().unwrap(),
			));

			// Can't restore an already existing item
			assert_noop!(
				Item::restore(
					&(collection_id, item_id),
					WithConfig::from(Owner::with_config_value(item_owner)),
				),
				Error::<Test>::InUse,
			);

			assert_noop!(
				Item::stash(&(collection_id, item_id), IfOwnedBy::check(collection_owner)),
				Error::<Test>::NoPermission,
			);

			assert_noop!(
				Item::stash(&(collection_id, item_id), IfOwnedBy::check(collection_admin)),
				Error::<Test>::NoPermission,
			);

			assert_ok!(Item::stash(&(collection_id, item_id), IfOwnedBy::check(item_owner)));

			assert_eq!(items(), vec![]);

			let retreived_test_value =
				Item::inspect(&(collection_id, item_id), Bytes(Attribute(test_key.as_slice())))
					.unwrap();

			// the attributes are still available
			assert_eq!(retreived_test_value, test_value);

			// A stahed item can be restored
			assert_ok!(Item::restore(
				&(collection_id, item_id),
				WithConfig::from(Owner::with_config_value(item_owner)),
			));
			assert_eq!(items(), vec![(item_owner, collection_id, item_id)]);
		});
	}

	#[test]
	fn stash_item_if_check_origin() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;
			let item_id = 111;

			let collection_owner = 1;
			let collection_admin = 2;
			let alice = 3;
			let bob = 4;

			Balances::make_free_balance_be(&collection_owner, 100);

			assert_ok!(Collection::create(WithConfig::new(
				(
					Owner::with_config_value(collection_owner),
					Admin::with_config_value(collection_admin)
				),
				PredefinedId::from(collection_id),
			)));

			assert_ok!(Item::create(WithConfig::new(
				Owner::with_config_value(alice),
				PredefinedId::from((collection_id, item_id)),
			)));

			assert_eq!(items(), vec![(alice, collection_id, item_id)]);

			let test_key = vec![0xF, 0x0, 0x0, 0xD];
			let test_value = vec![0xC, 0x0, 0x0, 0x1];

			assert_ok!(Uniques::set_attribute(
				RuntimeOrigin::signed(collection_owner),
				collection_id,
				Some(item_id),
				test_key.clone().try_into().unwrap(),
				test_value.clone().try_into().unwrap(),
			));

			// Can't restore an already existing item
			assert_noop!(
				Item::restore(
					&(collection_id, item_id),
					WithConfig::from(Owner::with_config_value(alice)),
				),
				Error::<Test>::InUse,
			);

			// Bob is not the admin and not the token owner
			// He can't stash the token
			assert_noop!(
				Item::stash(
					&(collection_id, item_id),
					CheckOrigin::check(RuntimeOrigin::signed(bob)),
				),
				Error::<Test>::NoPermission,
			);

			// Force origin can't stash tokens
			assert_noop!(
				Item::stash(&(collection_id, item_id), CheckOrigin::check(RuntimeOrigin::root())),
				DispatchError::BadOrigin,
			);

			// The collection admin can stash tokens
			assert_ok!(Item::stash(
				&(collection_id, item_id),
				CheckOrigin::check(RuntimeOrigin::signed(collection_admin)),
			));

			assert_eq!(items(), vec![]);

			let retreived_test_value =
				Item::inspect(&(collection_id, item_id), Bytes(Attribute(test_key.as_slice())))
					.unwrap();

			// the attributes are still available
			assert_eq!(retreived_test_value, test_value);

			// Restore the token
			assert_ok!(Item::restore(
				&(collection_id, item_id),
				WithConfig::from(Owner::with_config_value(alice)),
			));
			assert_eq!(items(), vec![(alice, collection_id, item_id)]);

			// The token owner can stash it
			assert_ok!(Item::stash(
				&(collection_id, item_id),
				CheckOrigin::check(RuntimeOrigin::signed(alice)),
			));

			assert_eq!(items(), vec![]);

			let retreived_test_value =
				Item::inspect(&(collection_id, item_id), Bytes(Attribute(test_key.as_slice())))
					.unwrap();

			// the attributes are still available
			assert_eq!(retreived_test_value, test_value);

			// A stahed item can be restored
			assert_ok!(Item::restore(
				&(collection_id, item_id),
				WithConfig::from(Owner::with_config_value(alice)),
			));
			assert_eq!(items(), vec![(alice, collection_id, item_id)]);
		});
	}

	#[test]
	fn inspect_item_ownership() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;
			let item_id = 111;

			let collection_owner = 1;
			let collection_admin = 2;
			let item_owner = 3;

			Balances::make_free_balance_be(&collection_owner, 100);

			assert_ok!(Collection::create(WithConfig::new(
				(
					Owner::with_config_value(collection_owner),
					Admin::with_config_value(collection_admin)
				),
				PredefinedId::from(collection_id),
			)));

			assert_ok!(Item::create(WithConfig::new(
				Owner::with_config_value(item_owner),
				PredefinedId::from((collection_id, item_id)),
			)));

			let retreived_item_owner =
				Item::inspect(&(collection_id, item_id), Owner::default()).unwrap();

			assert_eq!(retreived_item_owner, item_owner);
		});
	}

	#[test]
	fn inspect_item_metadata() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;
			let item_id = 111;

			let collection_owner = 1;
			let collection_admin = 2;
			let item_owner = 3;

			let metadata = vec![0xB, 0xE, 0xE, 0xF];
			let is_frozen = false;

			Balances::make_free_balance_be(&collection_owner, 100);

			assert_ok!(Collection::create(WithConfig::new(
				(
					Owner::with_config_value(collection_owner),
					Admin::with_config_value(collection_admin)
				),
				PredefinedId::from(collection_id),
			)));

			assert_ok!(Item::create(WithConfig::new(
				Owner::with_config_value(item_owner),
				PredefinedId::from((collection_id, item_id)),
			)));

			assert_noop!(
				Item::inspect(&(collection_id, item_id), Bytes::default()),
				Error::<Test>::NoMetadata,
			);

			assert_ok!(Uniques::set_metadata(
				RuntimeOrigin::root(),
				collection_id,
				item_id,
				metadata.clone().try_into().unwrap(),
				is_frozen,
			));

			let retreived_metadata =
				Item::inspect(&(collection_id, item_id), Bytes::default()).unwrap();

			assert_eq!(retreived_metadata, metadata);

			assert_ok!(Uniques::clear_metadata(RuntimeOrigin::root(), collection_id, item_id,));

			assert_noop!(
				Item::inspect(&(collection_id, item_id), Bytes::default()),
				Error::<Test>::NoMetadata,
			);
		});
	}

	#[test]
	fn inspect_item_attributes() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;
			let item_id = 111;

			let collection_owner = 1;
			let collection_admin = 2;
			let item_owner = 3;

			let food_attr_key = vec![0xB, 0xE, 0xE, 0xF];
			let food_attr_value = vec![0xC, 0x0, 0x0, 0x1];

			let drink_attr_key = vec![0xC, 0x0, 0xF, 0xF, 0xE, 0xE];
			let drink_attr_value = vec![0xD, 0xE, 0xC, 0xA, 0xF];

			Balances::make_free_balance_be(&collection_owner, 100);

			let set_attribute = |key: &Vec<u8>, value: &Vec<u8>| {
				assert_ok!(Uniques::set_attribute(
					RuntimeOrigin::signed(collection_owner),
					collection_id,
					Some(item_id),
					key.clone().try_into().unwrap(),
					value.clone().try_into().unwrap(),
				));
			};

			let clear_attribute = |key: &Vec<u8>| {
				assert_ok!(Uniques::clear_attribute(
					RuntimeOrigin::signed(collection_owner),
					collection_id,
					Some(item_id),
					key.clone().try_into().unwrap(),
				));
			};

			assert_ok!(Collection::create(WithConfig::new(
				(
					Owner::with_config_value(collection_owner),
					Admin::with_config_value(collection_admin)
				),
				PredefinedId::from(collection_id),
			)));

			assert_ok!(Item::create(WithConfig::new(
				Owner::with_config_value(item_owner),
				PredefinedId::from((collection_id, item_id)),
			)));

			assert_noop!(
				Item::inspect(
					&(collection_id, item_id),
					Bytes(Attribute(food_attr_key.as_slice())),
				),
				Error::<Test>::AttributeNotFound,
			);

			assert_noop!(
				Item::inspect(
					&(collection_id, item_id),
					Bytes(Attribute(drink_attr_key.as_slice())),
				),
				Error::<Test>::AttributeNotFound,
			);

			set_attribute(&food_attr_key, &food_attr_value);

			let retreived_food_value = Item::inspect(
				&(collection_id, item_id),
				Bytes(Attribute(food_attr_key.as_slice())),
			)
			.unwrap();

			assert_eq!(retreived_food_value, food_attr_value);

			assert_noop!(
				Item::inspect(
					&(collection_id, item_id),
					Bytes(Attribute(drink_attr_key.as_slice())),
				),
				Error::<Test>::AttributeNotFound,
			);

			set_attribute(&drink_attr_key, &drink_attr_value);

			let retreived_food_value = Item::inspect(
				&(collection_id, item_id),
				Bytes(Attribute(food_attr_key.as_slice())),
			)
			.unwrap();

			assert_eq!(retreived_food_value, food_attr_value);

			let retreived_drink_value = Item::inspect(
				&(collection_id, item_id),
				Bytes(Attribute(drink_attr_key.as_slice())),
			)
			.unwrap();

			assert_eq!(retreived_drink_value, drink_attr_value);

			clear_attribute(&food_attr_key);

			assert_noop!(
				Item::inspect(
					&(collection_id, item_id),
					Bytes(Attribute(food_attr_key.as_slice())),
				),
				Error::<Test>::AttributeNotFound,
			);

			let retreived_drink_value = Item::inspect(
				&(collection_id, item_id),
				Bytes(Attribute(drink_attr_key.as_slice())),
			)
			.unwrap();

			assert_eq!(retreived_drink_value, drink_attr_value);

			clear_attribute(&drink_attr_key);

			assert_noop!(
				Item::inspect(
					&(collection_id, item_id),
					Bytes(Attribute(food_attr_key.as_slice())),
				),
				Error::<Test>::AttributeNotFound,
			);

			assert_noop!(
				Item::inspect(
					&(collection_id, item_id),
					Bytes(Attribute(drink_attr_key.as_slice())),
				),
				Error::<Test>::AttributeNotFound,
			);
		});
	}

	#[test]
	fn inspect_item_can_update_owner() {
		new_test_ext().execute_with(|| {
			let collection_id = 10;
			let item_id = 111;

			let collection_owner = 1;
			let collection_admin = 2;
			let item_owner = 3;

			Balances::make_free_balance_be(&collection_owner, 100);

			assert_ok!(Collection::create(WithConfig::new(
				(
					Owner::with_config_value(collection_owner),
					Admin::with_config_value(collection_admin)
				),
				PredefinedId::from(collection_id),
			)));

			assert_ok!(Item::create(WithConfig::new(
				Owner::with_config_value(item_owner),
				PredefinedId::from((collection_id, item_id)),
			)));

			let can_update_owner =
				Item::inspect(&(collection_id, item_id), Owner::default().as_can_update()).unwrap();

			assert!(can_update_owner);

			assert_ok!(Uniques::freeze(
				RuntimeOrigin::signed(collection_admin),
				collection_id,
				item_id,
			));

			let can_update_owner =
				Item::inspect(&(collection_id, item_id), Owner::default().as_can_update()).unwrap();

			assert!(!can_update_owner);

			assert_ok!(Uniques::thaw(
				RuntimeOrigin::signed(collection_admin),
				collection_id,
				item_id,
			));

			let can_update_owner =
				Item::inspect(&(collection_id, item_id), Owner::default().as_can_update()).unwrap();

			assert!(can_update_owner);
		});
	}
}
