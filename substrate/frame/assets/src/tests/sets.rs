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

//! Tests for [`ItemOf`], [`fungible::UnionOf`] and [`fungibles::UnionOf`] set types.

use super::*;
use frame_support::{
	parameter_types,
	traits::{
		fungible,
		fungible::ItemOf,
		fungibles,
		tokens::{
			fungibles::{
				Balanced as FungiblesBalanced, Create as FungiblesCreate,
				Inspect as FungiblesInspect, Mutate as FungiblesMutate,
			},
			Fortitude, Precision, Preservation,
		},
	},
};
use sp_runtime::{traits::ConvertToValue, Either};

const FIRST_ASSET: u32 = 0;
const UNKNOWN_ASSET: u32 = 10;

parameter_types! {
	pub const LeftAsset: Either<(), u32> = Either::Left(());
	pub const RightAsset: Either<u32, ()> = Either::Right(());
	pub const RightUnitAsset: Either<(), ()> = Either::Right(());
}

/// Implementation of the `fungible` traits through the [`ItemOf`] type, specifically for a
/// single asset class from [`T`] identified by [`FIRST_ASSET`].
type FirstFungible<T> = ItemOf<T, frame_support::traits::ConstU32<{ FIRST_ASSET }>, u64>;

/// Implementation of the `fungible` traits through the [`ItemOf`] type, specifically for a
/// single asset class from [`T`] identified by [`UNKNOWN_ASSET`].
type UnknownFungible<T> = ItemOf<T, frame_support::traits::ConstU32<{ UNKNOWN_ASSET }>, u64>;

/// Implementation of `fungibles` traits using [`fungibles::UnionOf`] that exclusively utilizes
/// the [`FirstFungible`] from the left.
type LeftFungible<T> = fungible::UnionOf<FirstFungible<T>, T, ConvertToValue<LeftAsset>, (), u64>;

/// Implementation of `fungibles` traits using [`fungibles::UnionOf`] that exclusively utilizes
/// the [`LeftFungible`] from the right.
type RightFungible<T> =
	fungible::UnionOf<UnknownFungible<T>, LeftFungible<T>, ConvertToValue<RightUnitAsset>, (), u64>;

/// Implementation of `fungibles` traits using [`fungibles::UnionOf`] that exclusively utilizes
/// the [`RightFungible`] from the left.
type LeftFungibles<T> = fungibles::UnionOf<RightFungible<T>, T, ConvertToValue<LeftAsset>, (), u64>;

/// Implementation of `fungibles` traits using [`fungibles::UnionOf`] that exclusively utilizes
/// the [`LeftFungibles`] from the right.
///
/// By using this type, we can navigate through each branch of [`fungible::UnionOf`],
/// [`fungibles::UnionOf`], and [`ItemOf`] to access the underlying `fungibles::*`
/// implementation provided by the pallet.
type First<T> = fungibles::UnionOf<T, LeftFungibles<T>, ConvertToValue<RightAsset>, (), u64>;

#[test]
fn deposit_from_set_types_works() {
	new_test_ext().execute_with(|| {
		let asset1 = 0;
		let account1 = 1;
		let account2 = 2;

		assert_ok!(<Assets as FungiblesCreate<u64>>::create(asset1, account1, true, 1));
		assert_ok!(Assets::mint_into(asset1, &account1, 100));

		assert_eq!(First::<Assets>::total_issuance(()), 100);
		assert_eq!(First::<Assets>::total_issuance(()), Assets::total_issuance(asset1));

		let imb = First::<Assets>::deposit((), &account2, 50, Precision::Exact).unwrap();
		assert_eq!(First::<Assets>::balance((), &account2), 50);
		assert_eq!(First::<Assets>::total_issuance(()), 100);

		System::assert_has_event(RuntimeEvent::Assets(crate::Event::Deposited {
			asset_id: asset1,
			who: account2,
			amount: 50,
		}));

		assert_eq!(imb.peek(), 50);

		let (imb1, imb2) = imb.split(30);
		assert_eq!(imb1.peek(), 30);
		assert_eq!(imb2.peek(), 20);

		drop(imb2);
		assert_eq!(First::<Assets>::total_issuance(()), 120);

		assert!(First::<Assets>::settle(&account1, imb1, Preservation::Preserve).is_ok());
		assert_eq!(First::<Assets>::balance((), &account1), 70);
		assert_eq!(First::<Assets>::balance((), &account2), 50);
		assert_eq!(First::<Assets>::total_issuance(()), 120);

		assert_eq!(First::<Assets>::total_issuance(()), Assets::total_issuance(asset1));
	});
}

#[test]
fn issue_from_set_types_works() {
	new_test_ext().execute_with(|| {
		let asset1: u32 = 0;
		let account1: u64 = 1;

		assert_ok!(<Assets as FungiblesCreate<u64>>::create(asset1, account1, true, 1));
		assert_ok!(Assets::mint_into(asset1, &account1, 100));

		assert_eq!(First::<Assets>::balance((), &account1), 100);
		assert_eq!(First::<Assets>::total_issuance(()), 100);
		assert_eq!(First::<Assets>::total_issuance(()), Assets::total_issuance(asset1));

		let imb = First::<Assets>::issue((), 100);
		assert_eq!(First::<Assets>::total_issuance(()), 200);
		assert_eq!(imb.peek(), 100);

		let (imb1, imb2) = imb.split(30);
		assert_eq!(imb1.peek(), 30);
		assert_eq!(imb2.peek(), 70);

		drop(imb2);
		assert_eq!(First::<Assets>::total_issuance(()), 130);

		assert!(First::<Assets>::resolve(&account1, imb1).is_ok());
		assert_eq!(First::<Assets>::balance((), &account1), 130);
		assert_eq!(First::<Assets>::total_issuance(()), 130);

		assert_eq!(First::<Assets>::total_issuance(()), Assets::total_issuance(asset1));
	});
}

#[test]
fn pair_from_set_types_works() {
	new_test_ext().execute_with(|| {
		let asset1: u32 = 0;
		let account1: u64 = 1;

		assert_ok!(<Assets as FungiblesCreate<u64>>::create(asset1, account1, true, 1));
		assert_ok!(Assets::mint_into(asset1, &account1, 100));

		assert_eq!(First::<Assets>::balance((), &account1), 100);
		assert_eq!(First::<Assets>::total_issuance(()), 100);
		assert_eq!(First::<Assets>::total_issuance(()), Assets::total_issuance(asset1));

		let (debt, credit) = First::<Assets>::pair((), 100).unwrap();
		assert_eq!(First::<Assets>::total_issuance(()), 100);
		assert_eq!(debt.peek(), 100);
		assert_eq!(credit.peek(), 100);

		let (debt1, debt2) = debt.split(30);
		assert_eq!(debt1.peek(), 30);
		assert_eq!(debt2.peek(), 70);

		drop(debt2);
		assert_eq!(First::<Assets>::total_issuance(()), 170);

		assert!(First::<Assets>::settle(&account1, debt1, Preservation::Preserve).is_ok());
		assert_eq!(First::<Assets>::balance((), &account1), 70);
		assert_eq!(First::<Assets>::total_issuance(()), 170);

		let (credit1, credit2) = credit.split(40);
		assert_eq!(credit1.peek(), 40);
		assert_eq!(credit2.peek(), 60);

		drop(credit2);
		assert_eq!(First::<Assets>::total_issuance(()), 110);

		assert!(First::<Assets>::resolve(&account1, credit1).is_ok());
		assert_eq!(First::<Assets>::balance((), &account1), 110);
		assert_eq!(First::<Assets>::total_issuance(()), 110);

		assert_eq!(First::<Assets>::total_issuance(()), Assets::total_issuance(asset1));
	});
}

#[test]
fn rescind_from_set_types_works() {
	new_test_ext().execute_with(|| {
		let asset1: u32 = 0;
		let account1: u64 = 1;

		assert_ok!(<Assets as FungiblesCreate<u64>>::create(asset1, account1, true, 1));
		assert_ok!(Assets::mint_into(asset1, &account1, 100));

		assert_eq!(First::<Assets>::total_issuance(()), 100);
		assert_eq!(First::<Assets>::total_issuance(()), Assets::total_issuance(asset1));

		let imb = First::<Assets>::rescind((), 20);
		assert_eq!(First::<Assets>::total_issuance(()), 80);

		assert_eq!(imb.peek(), 20);

		let (imb1, imb2) = imb.split(15);
		assert_eq!(imb1.peek(), 15);
		assert_eq!(imb2.peek(), 5);

		drop(imb2);
		assert_eq!(First::<Assets>::total_issuance(()), 85);

		assert!(First::<Assets>::settle(&account1, imb1, Preservation::Preserve).is_ok());
		assert_eq!(First::<Assets>::balance((), &account1), 85);
		assert_eq!(First::<Assets>::total_issuance(()), 85);

		assert_eq!(First::<Assets>::total_issuance(()), Assets::total_issuance(asset1));
	});
}

#[test]
fn resolve_from_set_types_works() {
	new_test_ext().execute_with(|| {
		let asset1: u32 = 0;
		let account1: u64 = 1;
		let account2: u64 = 2;
		let ed = 11;

		assert_ok!(<Assets as FungiblesCreate<u64>>::create(asset1, account1, true, ed));
		assert_ok!(Assets::mint_into(asset1, &account1, 100));

		assert_eq!(First::<Assets>::balance((), &account1), 100);
		assert_eq!(First::<Assets>::total_issuance(()), 100);
		assert_eq!(First::<Assets>::total_issuance(()), Assets::total_issuance(asset1));

		let imb = First::<Assets>::issue((), 100);
		assert_eq!(First::<Assets>::total_issuance(()), 200);
		assert_eq!(imb.peek(), 100);

		let (imb1, imb2) = imb.split(10);
		assert_eq!(imb1.peek(), 10);
		assert_eq!(imb2.peek(), 90);
		assert_eq!(First::<Assets>::total_issuance(()), 200);

		// ed requirements not met.
		let imb1 = First::<Assets>::resolve(&account2, imb1).unwrap_err();
		assert_eq!(imb1.peek(), 10);
		drop(imb1);
		assert_eq!(First::<Assets>::total_issuance(()), 190);
		assert_eq!(First::<Assets>::balance((), &account2), 0);

		// resolve to new account `2`.
		assert_ok!(First::<Assets>::resolve(&account2, imb2));
		assert_eq!(First::<Assets>::total_issuance(()), 190);
		assert_eq!(First::<Assets>::balance((), &account2), 90);

		assert_eq!(First::<Assets>::total_issuance(()), Assets::total_issuance(asset1));
	});
}

#[test]
fn settle_from_set_types_works() {
	new_test_ext().execute_with(|| {
		let asset1: u32 = 0;
		let account1: u64 = 1;
		let account2: u64 = 2;
		let ed = 11;

		assert_ok!(<Assets as FungiblesCreate<u64>>::create(asset1, account1, true, ed));
		assert_ok!(Assets::mint_into(asset1, &account1, 100));
		assert_ok!(Assets::mint_into(asset1, &account2, 100));

		assert_eq!(First::<Assets>::balance((), &account2), 100);
		assert_eq!(First::<Assets>::total_issuance(()), 200);
		assert_eq!(First::<Assets>::total_issuance(()), Assets::total_issuance(asset1));

		let imb = First::<Assets>::rescind((), 100);
		assert_eq!(First::<Assets>::total_issuance(()), 100);
		assert_eq!(imb.peek(), 100);

		let (imb1, imb2) = imb.split(10);
		assert_eq!(imb1.peek(), 10);
		assert_eq!(imb2.peek(), 90);
		assert_eq!(First::<Assets>::total_issuance(()), 100);

		// ed requirements not met.
		let imb2 = First::<Assets>::settle(&account2, imb2, Preservation::Preserve).unwrap_err();
		assert_eq!(imb2.peek(), 90);
		drop(imb2);
		assert_eq!(First::<Assets>::total_issuance(()), 190);
		assert_eq!(First::<Assets>::balance((), &account2), 100);

		// settle to account `1`.
		assert_ok!(First::<Assets>::settle(&account2, imb1, Preservation::Preserve));
		assert_eq!(First::<Assets>::total_issuance(()), 190);
		assert_eq!(First::<Assets>::balance((), &account2), 90);

		let imb = First::<Assets>::rescind((), 85);
		assert_eq!(First::<Assets>::total_issuance(()), 105);
		assert_eq!(imb.peek(), 85);

		// settle to account `1` and expect some dust.
		let imb = First::<Assets>::settle(&account2, imb, Preservation::Expendable).unwrap();
		assert_eq!(imb.peek(), 5);
		assert_eq!(First::<Assets>::total_issuance(()), 105);
		assert_eq!(First::<Assets>::balance((), &account2), 0);

		drop(imb);
		assert_eq!(First::<Assets>::total_issuance(()), 100);

		assert_eq!(First::<Assets>::total_issuance(()), Assets::total_issuance(asset1));
	});
}

#[test]
fn withdraw_from_set_types_works() {
	new_test_ext().execute_with(|| {
		let asset1 = 0;
		let account1 = 1;
		let account2 = 2;

		assert_ok!(<Assets as FungiblesCreate<u64>>::create(asset1, account1, true, 1));
		assert_ok!(Assets::mint_into(asset1, &account1, 100));
		assert_ok!(Assets::mint_into(asset1, &account2, 100));

		assert_eq!(First::<Assets>::total_issuance(()), 200);
		assert_eq!(First::<Assets>::total_issuance(()), Assets::total_issuance(asset1));

		let imb = First::<Assets>::withdraw(
			(),
			&account2,
			50,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Polite,
		)
		.unwrap();
		assert_eq!(First::<Assets>::balance((), &account2), 50);
		assert_eq!(First::<Assets>::total_issuance(()), 200);

		System::assert_has_event(RuntimeEvent::Assets(crate::Event::Withdrawn {
			asset_id: asset1,
			who: account2,
			amount: 50,
		}));

		assert_eq!(imb.peek(), 50);
		drop(imb);
		assert_eq!(First::<Assets>::total_issuance(()), 150);
		assert_eq!(First::<Assets>::balance((), &account2), 50);

		assert_eq!(First::<Assets>::total_issuance(()), Assets::total_issuance(asset1));
	});
}
