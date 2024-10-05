// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use super::*;

#[test]
fn alias_foreign_account_sibling_prefix() {
	// Accounts Differ
	assert!(!AliasForeignAccountId32::<SiblingPrefix>::contains(
		&(Parent, Parachain(1), AccountId32 { network: None, id: [0; 32] }).into(),
		&(AccountId32 { network: None, id: [1; 32] }).into()
	));

	assert!(AliasForeignAccountId32::<SiblingPrefix>::contains(
		&(Parent, Parachain(1), AccountId32 { network: None, id: [0; 32] }).into(),
		&(AccountId32 { network: None, id: [0; 32] }).into()
	));
}

#[test]
fn alias_foreign_account_child_prefix() {
	// Accounts Differ
	assert!(!AliasForeignAccountId32::<ChildPrefix>::contains(
		&(Parachain(1), AccountId32 { network: None, id: [0; 32] }).into(),
		&(AccountId32 { network: None, id: [1; 32] }).into()
	));

	assert!(AliasForeignAccountId32::<ChildPrefix>::contains(
		&(Parachain(1), AccountId32 { network: None, id: [0; 32] }).into(),
		&(AccountId32 { network: None, id: [0; 32] }).into()
	));
}

#[test]
fn alias_foreign_account_parent_prefix() {
	// Accounts Differ
	assert!(!AliasForeignAccountId32::<ParentPrefix>::contains(
		&(Parent, AccountId32 { network: None, id: [0; 32] }).into(),
		&(AccountId32 { network: None, id: [1; 32] }).into()
	));

	assert!(AliasForeignAccountId32::<ParentPrefix>::contains(
		&(Parent, AccountId32 { network: None, id: [0; 32] }).into(),
		&(AccountId32 { network: None, id: [0; 32] }).into()
	));
}

#[test]
fn alias_origin_should_work() {
	AllowUnpaidFrom::set(vec![
		(Parent, Parachain(1), AccountId32 { network: None, id: [0; 32] }).into(),
		(Parachain(1), AccountId32 { network: None, id: [0; 32] }).into(),
	]);

	let message = Xcm(vec![AliasOrigin((AccountId32 { network: None, id: [0; 32] }).into())]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		(Parachain(1), AccountId32 { network: None, id: [0; 32] }),
		message.clone(),
		&mut hash,
		Weight::from_parts(50, 50),
		Weight::zero(),
	);
	assert_eq!(
		r,
		Outcome::Incomplete { used: Weight::from_parts(10, 10), error: XcmError::NoPermission }
	);

	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		(Parent, Parachain(1), AccountId32 { network: None, id: [0; 32] }),
		message.clone(),
		&mut hash,
		Weight::from_parts(50, 50),
		Weight::zero(),
	);
	assert_eq!(r, Outcome::Complete { used: Weight::from_parts(10, 10) });
}

#[test]
fn alias_child_location() {
	// parents differ
	assert!(!AliasChildLocation::contains(
		&Location::new(0, Parachain(1)),
		&Location::new(1, Parachain(1)),
	));
	assert!(!AliasChildLocation::contains(
		&Location::new(0, Here),
		&Location::new(1, Parachain(1)),
	));
	assert!(!AliasChildLocation::contains(&Location::new(1, Here), &Location::new(2, Here),));

	// interiors differ
	assert!(!AliasChildLocation::contains(
		&Location::new(1, Parachain(1)),
		&Location::new(1, OnlyChild),
	));
	assert!(!AliasChildLocation::contains(
		&Location::new(1, Parachain(1)),
		&Location::new(1, Parachain(12)),
	));
	assert!(!AliasChildLocation::contains(
		&Location::new(1, [Parachain(1), AccountId32 { network: None, id: [0; 32] }]),
		&Location::new(1, [Parachain(1), AccountId32 { network: None, id: [1; 32] }]),
	));
	assert!(!AliasChildLocation::contains(
		&Location::new(1, [Parachain(1), AccountId32 { network: None, id: [0; 32] }]),
		&Location::new(1, [Parachain(1), AccountId32 { network: None, id: [1; 32] }]),
	));

	// child to parent not allowed
	assert!(!AliasChildLocation::contains(
		&Location::new(1, [Parachain(1), AccountId32 { network: None, id: [0; 32] }]),
		&Location::new(1, [Parachain(1)]),
	));
	assert!(!AliasChildLocation::contains(
		&Location::new(1, [Parachain(1), AccountId32 { network: None, id: [0; 32] }]),
		&Location::new(1, Here),
	));

	// parent to child should work
	assert!(AliasChildLocation::contains(
		&Location::new(1, Here),
		&Location::new(1, [Parachain(1), AccountId32 { network: None, id: [1; 32] }]),
	));
	assert!(
		AliasChildLocation::contains(&Location::new(1, Here), &Location::new(1, Parachain(1)),)
	);
	assert!(AliasChildLocation::contains(
		&Location::new(0, Here),
		&Location::new(0, PalletInstance(42)),
	));
	assert!(AliasChildLocation::contains(
		&Location::new(2, GlobalConsensus(Kusama)),
		&Location::new(2, [GlobalConsensus(Kusama), Parachain(42), GeneralIndex(12)]),
	));
}

#[test]
fn alias_trusted_root_location() {
	// TODO
}
