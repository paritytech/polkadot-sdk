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

//! Tests for the scarcity pallet's ownership and transaction-extension invariants.

#![cfg(test)]

use crate::{
	extension::{AsScarcity, AsScarcityInfo, CustomInvalidity, Pre, Val},
	mock::*,
	CollectionMetadata, Collections, Error, Event, InstanceDeposits, InstanceMetadata,
	InstanceMetadataCount, Instances, ItemDefs, ItemMetadata, LockInfo, Locked, MetadataKeyOf,
	MetadataValueOf, MintWithoutDeposit, NextCollectionId, NextInstanceId, Nft, NftsByOwner,
	Origin,
};
use codec::Encode;
#[cfg(feature = "try-runtime")]
use frame_support::traits::Hooks;
use frame_support::{assert_noop, assert_ok, dispatch::Pays, traits::OriginTrait};
use sp_runtime::{
	traits::{TransactionExtension, TxBaseImplication},
	transaction_validity::{
		InvalidTransaction, TransactionSource, TransactionValidityError, ValidTransaction,
	},
	DispatchResult, TryRuntimeError,
};

const OWNER: u64 = 1;
const OTHER: u64 = 2;
const RECIPIENT: u64 = 3;

fn key(value: &[u8]) -> MetadataKeyOf<Test> {
	value.to_vec().try_into().expect("metadata key fits the test bound")
}

fn value(value: &[u8]) -> MetadataValueOf<Test> {
	value.to_vec().try_into().expect("metadata value fits the test bound")
}

fn metadata(entries: &[(&[u8], &[u8])]) -> Vec<(MetadataKeyOf<Test>, MetadataValueOf<Test>)> {
	entries
		.iter()
		.map(|(key_bytes, value_bytes)| (key(key_bytes), value(value_bytes)))
		.collect::<Vec<_>>()
}

fn define(collection: u32) {
	assert_ok!(Scarcity::define_item(RuntimeOrigin::signed(OWNER), collection, metadata(&[]),));
}

fn setup_item() {
	assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));
	define(0);
}

fn mint(item: u32, to: u64) {
	assert_ok!(Scarcity::mint(RuntimeOrigin::signed(OWNER), 0, item, to, metadata(&[])));
}

fn mint_with_metadata(item: u32, to: u64, entries: &[(&[u8], &[u8])]) {
	assert_ok!(Scarcity::mint(RuntimeOrigin::signed(OWNER), 0, item, to, metadata(entries),));
}

fn nft_origin(owner: u64, nft: Nft) -> RuntimeOrigin {
	RuntimeOrigin::from(Origin::<Test>::Nft { owner, nft })
}

fn transfer_call(to: u64) -> RuntimeCall {
	RuntimeCall::Scarcity(crate::Call::transfer { to })
}

fn burn_call() -> RuntimeCall {
	RuntimeCall::Scarcity(crate::Call::burn {})
}

fn authorization(instance: u64, state_nonce: u64) -> AsScarcityInfo {
	AsScarcityInfo::AsNft { instance, state_nonce }
}

fn current_authorization(owner: u64) -> AsScarcityInfo {
	let nft = NftsByOwner::<Test>::get(owner).expect("authorization requires an NFT");
	authorization(nft.instance, nft.state_nonce)
}

fn scarcity_extension(info: AsScarcityInfo) -> AsScarcity<Test> {
	AsScarcity::new(Some(info))
}

fn extension_for_val(val: &Val<Test>) -> AsScarcity<Test> {
	match val {
		Val::NotUsing => AsScarcity::new(None),
		Val::UsingNft { instance, state_nonce, .. } => {
			scarcity_extension(authorization(*instance, *state_nonce))
		},
	}
}

fn validate_transfer_as(
	signer: u64,
	to: u64,
	info: AsScarcityInfo,
) -> Result<(ValidTransaction, Val<Test>, RuntimeOrigin), TransactionValidityError> {
	let call = transfer_call(to);
	scarcity_extension(info).validate(
		RuntimeOrigin::signed(signer),
		&call,
		&Default::default(),
		0,
		(),
		&TxBaseImplication(()),
		TransactionSource::External,
	)
}

fn validate_transfer(
	signer: u64,
	to: u64,
) -> Result<(ValidTransaction, Val<Test>, RuntimeOrigin), TransactionValidityError> {
	validate_transfer_as(signer, to, current_authorization(signer))
}

fn prepare_transfer(val: Val<Test>, origin: &RuntimeOrigin, to: u64) -> Pre<Test> {
	let call = transfer_call(to);
	extension_for_val(&val)
		.prepare(val, origin, &call, &Default::default(), 0)
		.unwrap()
}

fn validate_burn_as(
	signer: u64,
	info: AsScarcityInfo,
) -> Result<(ValidTransaction, Val<Test>, RuntimeOrigin), TransactionValidityError> {
	let call = burn_call();
	scarcity_extension(info).validate(
		RuntimeOrigin::signed(signer),
		&call,
		&Default::default(),
		0,
		(),
		&TxBaseImplication(()),
		TransactionSource::External,
	)
}

fn validate_burn(
	signer: u64,
) -> Result<(ValidTransaction, Val<Test>, RuntimeOrigin), TransactionValidityError> {
	validate_burn_as(signer, current_authorization(signer))
}

fn prepare_burn(val: Val<Test>, origin: &RuntimeOrigin) -> Pre<Test> {
	let call = burn_call();
	extension_for_val(&val)
		.prepare(val, origin, &call, &Default::default(), 0)
		.unwrap()
}

fn assert_no_nft(error: TransactionValidityError) {
	assert!(matches!(
		error,
		TransactionValidityError::Invalid(InvalidTransaction::Custom(code))
			if code == CustomInvalidity::NoNft as u8
	));
}

fn assert_state_mismatch(error: TransactionValidityError) {
	assert!(matches!(
		error,
		TransactionValidityError::Invalid(InvalidTransaction::Custom(code))
			if code == CustomInvalidity::NftStateMismatch as u8
	));
}

fn post_dispatch(pre: Pre<Test>, result: DispatchResult) {
	assert_ok!(AsScarcity::<Test>::post_dispatch_details(
		pre,
		&Default::default(),
		&Default::default(),
		0,
		&result,
	));
}

fn assert_try_state_error(expected: &'static str) {
	match Scarcity::do_try_state() {
		Err(TryRuntimeError::Other(actual)) => assert_eq!(actual, expected),
		other => panic!("expected try-state error {expected:?}, got {other:?}"),
	}
}

#[test]
fn create_collection_assigns_incremental_ids_and_owner() {
	new_test_ext().execute_with(|| {
		assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));
		assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OTHER)));

		assert_eq!(Collections::<Test>::get(0).unwrap().owner, OWNER);
		assert_eq!(Collections::<Test>::get(1).unwrap().owner, OTHER);
		System::assert_has_event(
			Event::<Test>::CollectionCreated { collection: 0, owner: OWNER }.into(),
		);
		System::assert_has_event(
			Event::<Test>::CollectionCreated { collection: 1, owner: OTHER }.into(),
		);
	});
}

#[test]
fn collection_owner_can_nominate_or_clear_a_successor() {
	new_test_ext().execute_with(|| {
		assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));

		assert_noop!(
			Scarcity::nominate_collection_owner(RuntimeOrigin::signed(OTHER), 0, Some(RECIPIENT)),
			Error::<Test>::NoPermission
		);
		assert_noop!(
			Scarcity::nominate_collection_owner(RuntimeOrigin::signed(OWNER), 99, Some(OTHER)),
			Error::<Test>::UnknownCollection
		);
		assert_noop!(
			Scarcity::nominate_collection_owner(RuntimeOrigin::signed(OWNER), 0, Some(OWNER)),
			Error::<Test>::AlreadyCollectionOwner
		);

		assert_ok!(Scarcity::nominate_collection_owner(
			RuntimeOrigin::signed(OWNER),
			0,
			Some(OTHER),
		));
		let nominated = Collections::<Test>::get(0).expect("collection exists");
		assert_eq!(nominated.owner, OWNER);
		assert_eq!(nominated.pending_owner, Some(OTHER));
		System::assert_has_event(
			Event::<Test>::CollectionOwnerNominated { collection: 0, pending_owner: Some(OTHER) }
				.into(),
		);

		assert_ok!(Scarcity::nominate_collection_owner(RuntimeOrigin::signed(OWNER), 0, None,));
		assert_eq!(Collections::<Test>::get(0).unwrap().pending_owner, None);
	});
}

#[test]
fn only_the_current_nominee_can_claim_collection_ownership() {
	new_test_ext().execute_with(|| {
		assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));
		assert_noop!(
			Scarcity::claim_collection_ownership(RuntimeOrigin::signed(OTHER), 0),
			Error::<Test>::NotPendingCollectionOwner
		);

		assert_ok!(Scarcity::nominate_collection_owner(
			RuntimeOrigin::signed(OWNER),
			0,
			Some(OTHER),
		));
		assert_noop!(
			Scarcity::claim_collection_ownership(RuntimeOrigin::signed(RECIPIENT), 0),
			Error::<Test>::NotPendingCollectionOwner
		);
		assert_noop!(
			Scarcity::claim_collection_ownership(RuntimeOrigin::signed(OTHER), 99),
			Error::<Test>::UnknownCollection
		);
	});
}

#[test]
fn claim_fails_atomically_when_nominee_cannot_back_the_deposit() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);
		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			key(b"name"),
			Some(value(b"collection")),
		));
		let before = Collections::<Test>::get(0).expect("collection exists");
		let owner_hold = held(OWNER);
		assert_eq!(held(99), 0);

		assert_ok!(Scarcity::nominate_collection_owner(RuntimeOrigin::signed(OWNER), 0, Some(99),));
		assert!(
			Scarcity::claim_collection_ownership(RuntimeOrigin::signed(99), 0).is_err(),
			"an unfunded nominee cannot assume the collection deposit",
		);

		let after = Collections::<Test>::get(0).expect("collection remains");
		assert_eq!(after.owner, OWNER);
		assert_eq!(after.pending_owner, Some(99));
		assert_eq!(after.owner_deposit, before.owner_deposit);
		assert_eq!(held(OWNER), owner_hold);
		assert_eq!(held(99), 0);
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn claim_moves_exact_collection_deposit_and_authority() {
	new_test_ext().execute_with(|| {
		setup_item();
		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			key(b"name"),
			Some(value(b"collection")),
		));
		mint_with_metadata(0, RECIPIENT, &[(b"unique", b"claimed")]);
		let instance_deposit =
			InstanceDeposits::<Test>::get(0).expect("ordinary mint has a deposit");
		let instance_metadata_deposit = InstanceMetadata::<Test>::get(0, key(b"unique"))
			.expect("metadata exists")
			.deposit;

		// A second collection proves that claiming one collection does not release the old
		// owner's unrelated holds.
		assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));
		let old_owner_hold = held(OWNER);
		let moved_deposit = Collections::<Test>::get(0).unwrap().owner_deposit;
		let remaining_deposit = Collections::<Test>::get(1).unwrap().owner_deposit;
		assert_eq!(old_owner_hold, moved_deposit + remaining_deposit);

		assert_ok!(Scarcity::nominate_collection_owner(
			RuntimeOrigin::signed(OWNER),
			0,
			Some(OTHER),
		));
		assert_ok!(Scarcity::claim_collection_ownership(RuntimeOrigin::signed(OTHER), 0));

		let claimed = Collections::<Test>::get(0).expect("claimed collection exists");
		assert_eq!(claimed.owner, OTHER);
		assert_eq!(claimed.pending_owner, None);
		assert_eq!(claimed.owner_deposit, moved_deposit);
		assert_eq!(held(OWNER), remaining_deposit);
		assert_eq!(held(OTHER), moved_deposit);
		assert_eq!(Collections::<Test>::get(1).unwrap().owner, OWNER);
		System::assert_has_event(
			Event::<Test>::CollectionOwnerChanged {
				collection: 0,
				old_owner: OWNER,
				new_owner: OTHER,
			}
			.into(),
		);

		assert_noop!(
			Scarcity::define_item(RuntimeOrigin::signed(OWNER), 0, metadata(&[])),
			Error::<Test>::NoPermission
		);
		assert_ok!(Scarcity::define_item(RuntimeOrigin::signed(OTHER), 0, metadata(&[])));
		assert_noop!(
			Scarcity::set_collection_metadata(
				RuntimeOrigin::signed(OWNER),
				0,
				key(b"name"),
				Some(value(b"old owner")),
			),
			Error::<Test>::NoPermission
		);
		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OTHER),
			0,
			key(b"name"),
			Some(value(b"new owner")),
		));
		assert_noop!(
			Scarcity::set_instance_metadata(
				RuntimeOrigin::signed(OWNER),
				0,
				key(b"unique"),
				Some(value(b"claimed")),
			),
			Error::<Test>::NoPermission
		);
		assert_ok!(Scarcity::set_instance_metadata(
			RuntimeOrigin::signed(OTHER),
			0,
			key(b"unique"),
			Some(value(b"updated")),
		));

		let before_burn = held(OTHER);
		assert_noop!(
			Scarcity::force_burn(RuntimeOrigin::signed(OWNER), 0),
			Error::<Test>::NoPermission
		);
		assert_ok!(Scarcity::force_burn(RuntimeOrigin::signed(OTHER), 0));
		assert_eq!(held(OTHER), before_burn - instance_deposit - instance_metadata_deposit);
		assert_eq!(held(OWNER), remaining_deposit);
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn create_collection_holds_and_tracks_its_deposit() {
	new_test_ext().execute_with(|| {
		assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));

		let info = Collections::<Test>::get(0).expect("collection exists");
		assert!(info.collection_deposit > 0);
		assert_eq!(info.owner_deposit, info.collection_deposit);
		assert_eq!(info.pending_owner, None);
		assert_eq!(info.item_count, 0);
		assert_eq!(info.metadata_count, 0);
		assert_eq!(held(OWNER), info.owner_deposit);
	});
}

#[test]
fn define_item_holds_and_tracks_its_deposit() {
	new_test_ext().execute_with(|| {
		assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));
		let held_before = held(OWNER);

		define(0);
		let definition = ItemDefs::<Test>::get(0, 0).expect("item definition exists");
		assert!(definition.deposit > 0);
		assert_eq!(definition.supply, 0);
		assert_eq!(definition.live_supply, 0);
		assert_eq!(definition.metadata_count, 0);
		assert_eq!(Collections::<Test>::get(0).unwrap().item_count, 1);
		assert_eq!(held(OWNER), held_before + definition.deposit);
		assert_eq!(Collections::<Test>::get(0).unwrap().owner_deposit, held(OWNER),);
	});
}

#[test]
fn mint_charges_collection_owner_and_stores_instance_deposit() {
	new_test_ext().execute_with(|| {
		setup_item();
		let held_before = held(OWNER);

		mint(0, RECIPIENT);
		let deposit = InstanceDeposits::<Test>::get(0).expect("paid mint stores its deposit");
		assert!(deposit > 0);
		assert_eq!(held(OWNER), held_before + deposit);
		assert_eq!(Collections::<Test>::get(0).unwrap().owner_deposit, held(OWNER),);
		assert_eq!(frame_system::Account::<Test>::get(RECIPIENT).sufficients, 0);
	});
}

#[test]
fn define_item_requires_collection_owner() {
	new_test_ext().execute_with(|| {
		assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));
		assert_noop!(
			Scarcity::define_item(RuntimeOrigin::signed(OTHER), 0, metadata(&[]),),
			Error::<Test>::NoPermission
		);
		assert_noop!(
			Scarcity::define_item(RuntimeOrigin::signed(OWNER), 99, metadata(&[]),),
			Error::<Test>::UnknownCollection
		);
	});
}

#[test]
fn define_item_assigns_incremental_indexes() {
	new_test_ext().execute_with(|| {
		assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));
		define(0);
		define(0);
		define(0);

		assert!(ItemDefs::<Test>::contains_key(0, 0));
		assert!(ItemDefs::<Test>::contains_key(0, 1));
		assert!(ItemDefs::<Test>::contains_key(0, 2));
		assert_eq!(Collections::<Test>::get(0).unwrap().next_item_index, 3);
		assert_eq!(ItemDefs::<Test>::get(0, 0).unwrap().supply, 0);
		assert_eq!(ItemDefs::<Test>::get(0, 1).unwrap().supply, 0);
		assert_eq!(ItemDefs::<Test>::get(0, 2).unwrap().supply, 0);
		System::assert_has_event(Event::<Test>::ItemDefined { collection: 0, item: 2 }.into());
	});
}

#[test]
fn metadata_resolution_prefers_item_then_falls_back_to_collection() {
	new_test_ext().execute_with(|| {
		setup_item();
		let shared = key(b"shared");
		let inherited = key(b"inherited");
		let absent = key(b"absent");

		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			shared.clone(),
			Some(value(b"collection")),
		));
		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			inherited.clone(),
			Some(value(b"default")),
		));
		assert_ok!(Scarcity::set_item_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			0,
			shared.clone(),
			Some(value(b"item")),
		));

		assert_eq!(Scarcity::collection_metadata_of(0, &shared), Some(value(b"collection")));
		assert_eq!(Scarcity::item_metadata_of(0, 0, &shared), Some(value(b"item")));
		assert_eq!(Scarcity::item_metadata_of(0, 0, &inherited), Some(value(b"default")));
		assert_eq!(Scarcity::item_metadata_of(0, 0, &absent), None);
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn instance_metadata_overrides_item_and_collection_without_affecting_other_mints() {
	new_test_ext().execute_with(|| {
		setup_item();
		let shared = key(b"shared");
		let inherited = key(b"inherited");
		let unique = key(b"unique");

		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			shared.clone(),
			Some(value(b"collection")),
		));
		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			inherited.clone(),
			Some(value(b"default")),
		));
		assert_ok!(Scarcity::set_item_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			0,
			shared.clone(),
			Some(value(b"item")),
		));

		mint_with_metadata(0, RECIPIENT, &[(b"shared", b"instance"), (b"unique", b"first")]);
		mint(0, OTHER);

		assert_eq!(Scarcity::instance_metadata_of(0, &shared), Some(value(b"instance")));
		assert_eq!(Scarcity::instance_metadata_of(0, &inherited), Some(value(b"default")));
		assert_eq!(Scarcity::instance_metadata_of(0, &unique), Some(value(b"first")));
		assert_eq!(Scarcity::instance_metadata_of(1, &shared), Some(value(b"item")));
		assert_eq!(Scarcity::instance_metadata_of(1, &inherited), Some(value(b"default")));
		assert_eq!(Scarcity::instance_metadata_of(1, &unique), None);
		assert_eq!(Scarcity::item_metadata_of(0, 0, &shared), Some(value(b"item")));
		assert_eq!(InstanceMetadataCount::<Test>::get(0), 2);
		assert_eq!(InstanceMetadataCount::<Test>::get(1), 0);
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn instance_metadata_mutation_is_collection_owner_only_and_updates_deposits() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);
		let metadata_key = key(b"instance");
		let held_before = held(OWNER);

		assert_noop!(
			Scarcity::set_instance_metadata(
				RuntimeOrigin::signed(OTHER),
				0,
				metadata_key.clone(),
				Some(value(b"denied")),
			),
			Error::<Test>::NoPermission
		);
		assert_noop!(
			Scarcity::set_instance_metadata(
				RuntimeOrigin::signed(OWNER),
				99,
				metadata_key.clone(),
				Some(value(b"missing")),
			),
			Error::<Test>::UnknownInstance
		);

		assert_ok!(Scarcity::set_instance_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			metadata_key.clone(),
			Some(value(b"one")),
		));
		let first_deposit =
			InstanceMetadata::<Test>::get(0, &metadata_key).expect("entry exists").deposit;
		assert!(first_deposit > 0);
		assert_eq!(InstanceMetadataCount::<Test>::get(0), 1);
		assert_eq!(held(OWNER), held_before + first_deposit);

		assert_ok!(Scarcity::set_instance_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			metadata_key.clone(),
			Some(value(b"longer")),
		));
		let replacement_deposit =
			InstanceMetadata::<Test>::get(0, &metadata_key).expect("entry exists").deposit;
		assert!(replacement_deposit > first_deposit);
		assert_eq!(InstanceMetadataCount::<Test>::get(0), 1);
		assert_eq!(held(OWNER), held_before + replacement_deposit);
		System::assert_has_event(
			Event::<Test>::InstanceMetadataSet { instance: 0, key: metadata_key.clone() }.into(),
		);

		assert_ok!(Scarcity::set_instance_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			metadata_key.clone(),
			None,
		));
		assert!(!InstanceMetadata::<Test>::contains_key(0, &metadata_key));
		assert_eq!(InstanceMetadataCount::<Test>::get(0), 0);
		assert_eq!(held(OWNER), held_before);
		System::assert_has_event(
			Event::<Test>::InstanceMetadataRemoved { instance: 0, key: metadata_key }.into(),
		);
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn metadata_mutation_is_owner_only_at_both_levels() {
	new_test_ext().execute_with(|| {
		setup_item();
		let collection_key = key(b"collection");
		let item_key = key(b"item");

		assert_noop!(
			Scarcity::set_collection_metadata(
				RuntimeOrigin::signed(OTHER),
				0,
				collection_key.clone(),
				Some(value(b"denied")),
			),
			Error::<Test>::NoPermission
		);
		assert_noop!(
			Scarcity::set_item_metadata(
				RuntimeOrigin::signed(OTHER),
				0,
				0,
				item_key.clone(),
				Some(value(b"denied")),
			),
			Error::<Test>::NoPermission
		);

		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			collection_key.clone(),
			Some(value(b"first")),
		));
		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			collection_key.clone(),
			Some(value(b"second")),
		));
		assert_ok!(Scarcity::set_item_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			0,
			item_key.clone(),
			Some(value(b"first")),
		));
		assert_ok!(Scarcity::set_item_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			0,
			item_key.clone(),
			Some(value(b"second")),
		));
		System::assert_has_event(
			Event::<Test>::CollectionMetadataSet { collection: 0, key: collection_key.clone() }
				.into(),
		);
		System::assert_has_event(
			Event::<Test>::ItemMetadataSet { collection: 0, item: 0, key: item_key.clone() }.into(),
		);
		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			collection_key.clone(),
			None,
		));
		assert_ok!(Scarcity::set_item_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			0,
			item_key.clone(),
			None,
		));
		System::assert_has_event(
			Event::<Test>::CollectionMetadataRemoved { collection: 0, key: collection_key.clone() }
				.into(),
		);
		System::assert_has_event(
			Event::<Test>::ItemMetadataRemoved { collection: 0, item: 0, key: item_key.clone() }
				.into(),
		);

		assert!(!CollectionMetadata::<Test>::contains_key(0, collection_key));
		assert!(!ItemMetadata::<Test>::contains_key((0, 0, item_key)));
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn metadata_mutation_rejects_unknown_collection_and_item() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Scarcity::set_collection_metadata(
				RuntimeOrigin::signed(OWNER),
				99,
				key(b"key"),
				Some(value(b"value")),
			),
			Error::<Test>::UnknownCollection
		);
		assert_noop!(
			Scarcity::set_item_metadata(
				RuntimeOrigin::signed(OWNER),
				99,
				0,
				key(b"key"),
				Some(value(b"value")),
			),
			Error::<Test>::UnknownCollection
		);

		assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));
		assert_noop!(
			Scarcity::set_item_metadata(
				RuntimeOrigin::signed(OWNER),
				0,
				99,
				key(b"key"),
				Some(value(b"value")),
			),
			Error::<Test>::UnknownItem
		);
	});
}

#[test]
fn metadata_deposits_update_in_place_and_release() {
	new_test_ext().execute_with(|| {
		setup_item();
		let collection_key = key(b"c");
		let base_hold = held(OWNER);

		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			collection_key.clone(),
			Some(value(b"one")),
		));
		let first_deposit = CollectionMetadata::<Test>::get(0, &collection_key).unwrap().deposit;
		assert!(first_deposit > 0);
		assert_eq!(Collections::<Test>::get(0).unwrap().metadata_count, 1);
		assert_eq!(held(OWNER), base_hold + first_deposit);

		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			collection_key.clone(),
			Some(value(b"longer")),
		));
		let replacement_deposit =
			CollectionMetadata::<Test>::get(0, &collection_key).unwrap().deposit;
		assert!(replacement_deposit > first_deposit);
		assert_eq!(Collections::<Test>::get(0).unwrap().metadata_count, 1);
		assert_eq!(held(OWNER), base_hold + replacement_deposit);

		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			collection_key,
			None,
		));
		assert_eq!(Collections::<Test>::get(0).unwrap().metadata_count, 0);
		assert_eq!(held(OWNER), base_hold);

		let item_key = key(b"i");
		assert_ok!(Scarcity::set_item_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			0,
			item_key.clone(),
			Some(value(b"one")),
		));
		let first_deposit = ItemMetadata::<Test>::get((0, 0, item_key.clone())).unwrap().deposit;
		assert_eq!(ItemDefs::<Test>::get(0, 0).unwrap().metadata_count, 1);
		assert_eq!(held(OWNER), base_hold + first_deposit);
		assert_ok!(Scarcity::set_item_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			0,
			item_key.clone(),
			Some(value(b"longer")),
		));
		let replacement_deposit =
			ItemMetadata::<Test>::get((0, 0, item_key.clone())).unwrap().deposit;
		assert!(replacement_deposit > first_deposit);
		assert_eq!(ItemDefs::<Test>::get(0, 0).unwrap().metadata_count, 1);
		assert_eq!(held(OWNER), base_hold + replacement_deposit);
		assert_ok!(
			Scarcity::set_item_metadata(RuntimeOrigin::signed(OWNER), 0, 0, item_key, None,)
		);
		assert_eq!(ItemDefs::<Test>::get(0, 0).unwrap().metadata_count, 0);
		assert_eq!(held(OWNER), base_hold);
		assert_eq!(Collections::<Test>::get(0).unwrap().owner_deposit, base_hold);
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn removing_absent_metadata_is_a_no_op() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);
		let events_before = System::events().len();
		let held_before = held(OWNER);

		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			key(b"missing"),
			None,
		));
		assert_ok!(Scarcity::set_item_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			0,
			key(b"missing"),
			None,
		));
		assert_ok!(Scarcity::set_instance_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			key(b"missing"),
			None,
		));

		assert_eq!(held(OWNER), held_before);
		assert_eq!(System::events().len(), events_before);
		assert_eq!(InstanceMetadataCount::<Test>::get(0), 0);
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn define_item_accepts_more_than_old_cap_and_charges_each_metadata_entry() {
	new_test_ext().execute_with(|| {
		assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));
		let held_before = held(OWNER);
		let metadata = (0..41)
			.map(|index| {
				(key(format!("key-{index}").as_bytes()), value(format!("value-{index}").as_bytes()))
			})
			.collect::<Vec<_>>();
		assert_ok!(Scarcity::define_item(RuntimeOrigin::signed(OWNER), 0, metadata,));

		let definition = ItemDefs::<Test>::get(0, 0).expect("item definition exists");
		assert_eq!(ItemMetadata::<Test>::iter_prefix((0, 0)).count(), 41);
		assert_eq!(Scarcity::item_metadata_of(0, 0, &key(b"key-40")), Some(value(b"value-40")),);
		let metadata_deposit = ItemMetadata::<Test>::iter_prefix((0, 0))
			.map(|(_, entry)| entry.deposit)
			.sum::<u64>();
		assert_eq!(held(OWNER), held_before + definition.deposit + metadata_deposit);
		assert_eq!(Collections::<Test>::get(0).unwrap().owner_deposit, held(OWNER));
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn mint_requires_owner_and_existing_def() {
	new_test_ext().execute_with(|| {
		setup_item();
		assert_noop!(
			Scarcity::mint(RuntimeOrigin::signed(OTHER), 0, 0, RECIPIENT, metadata(&[])),
			Error::<Test>::NoPermission
		);
		assert_noop!(
			Scarcity::mint(RuntimeOrigin::signed(OWNER), 0, 1, RECIPIENT, metadata(&[])),
			Error::<Test>::UnknownItem
		);
	});
}

#[test]
fn mint_enforces_one_nft_per_key() {
	new_test_ext().execute_with(|| {
		setup_item();
		define(0);
		mint(0, RECIPIENT);
		assert_noop!(
			Scarcity::mint(RuntimeOrigin::signed(OWNER), 0, 1, RECIPIENT, metadata(&[])),
			Error::<Test>::AddressOccupied
		);
	});
}

#[test]
fn mint_enforces_instance_metadata_limit_atomically() {
	new_test_ext().execute_with(|| {
		setup_item();
		let too_many =
			metadata(&[(b"one", b"1"), (b"two", b"2"), (b"three", b"3"), (b"four", b"4")]);

		assert_noop!(
			Scarcity::mint(RuntimeOrigin::signed(OWNER), 0, 0, RECIPIENT, too_many),
			Error::<Test>::TooManyInstanceMetadata
		);
		assert_eq!(NextInstanceId::<Test>::get(), 0);
		assert_eq!(ItemDefs::<Test>::get(0, 0).unwrap().supply, 0);
		assert!(!NftsByOwner::<Test>::contains_key(RECIPIENT));
		assert!(!InstanceMetadataCount::<Test>::contains_key(0));

		mint_with_metadata(0, RECIPIENT, &[(b"one", b"1"), (b"two", b"2"), (b"three", b"3")]);
		assert_noop!(
			Scarcity::set_instance_metadata(
				RuntimeOrigin::signed(OWNER),
				0,
				key(b"four"),
				Some(value(b"4")),
			),
			Error::<Test>::TooManyInstanceMetadata
		);
		assert_ok!(Scarcity::set_instance_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			key(b"one"),
			Some(value(b"updated")),
		));
		assert_eq!(InstanceMetadataCount::<Test>::get(0), 3);
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn mint_writes_consistent_state() {
	new_test_ext().execute_with(|| {
		setup_item();
		define(0);
		MockNow::set(1_234);
		mint(0, RECIPIENT);

		let nft = NftsByOwner::<Test>::get(RECIPIENT).expect("minted NFT is stored by owner");
		assert_eq!(nft.instance, 0);
		assert_eq!(Instances::<Test>::get(0), Some(RECIPIENT));
		assert!(InstanceMetadataCount::<Test>::contains_key(0));
		assert_eq!(InstanceMetadataCount::<Test>::get(0), 0);
		let definition = ItemDefs::<Test>::get(0, 0).unwrap();
		assert_eq!(definition.supply, 1);
		assert_eq!(definition.live_supply, 1);
		assert_eq!(nft.minted_at, 1_234);
		assert_eq!(nft.last_moved, 1_234);
		System::assert_has_event(
			Event::<Test>::Minted { instance: 0, collection: 0, item: 0, owner: RECIPIENT }.into(),
		);

		MockNow::set(1_235);
		mint(1, 4);
		let second = NftsByOwner::<Test>::get(4).expect("second NFT is stored by owner");
		assert_eq!(second.instance, 1);
		assert_eq!(Instances::<Test>::get(1), Some(4));
		assert_eq!(second.minted_at, 1_235);
	});
}

#[test]
fn mint_charges_for_initial_instance_metadata() {
	new_test_ext().execute_with(|| {
		setup_item();
		let held_before = held(OWNER);

		mint_with_metadata(0, RECIPIENT, &[(b"unique", b"value")]);

		let instance_deposit =
			InstanceDeposits::<Test>::get(0).expect("ordinary mint has a deposit");
		let metadata_deposit = InstanceMetadata::<Test>::get(0, key(b"unique"))
			.expect("metadata exists")
			.deposit;
		assert!(metadata_deposit > 0);
		assert_eq!(InstanceMetadataCount::<Test>::get(0), 1);
		assert_eq!(held(OWNER), held_before + instance_deposit + metadata_deposit);
		assert_eq!(Collections::<Test>::get(0).unwrap().owner_deposit, held(OWNER));
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn mint_without_deposit_waives_deposit_and_increments_supply() {
	new_test_ext().execute_with(|| {
		setup_item();
		let held_before = held(OWNER);
		MockNow::set(1_234);

		assert_eq!(Scarcity::mint_without_deposit(0, 0, RECIPIENT, metadata(&[])), Ok(0));

		let nft = NftsByOwner::<Test>::get(RECIPIENT).expect("depositless NFT is stored by owner");
		assert_eq!(nft.instance, 0);
		assert_eq!(nft.collection, 0);
		assert_eq!(nft.item, 0);
		assert_eq!(nft.minted_at, 1_234);
		assert_eq!(Instances::<Test>::get(0), Some(RECIPIENT));
		assert_eq!(ItemDefs::<Test>::get(0, 0).unwrap().supply, 1);
		assert!(!InstanceDeposits::<Test>::contains_key(0));
		assert_eq!(held(OWNER), held_before);
		System::assert_has_event(
			Event::<Test>::Minted { instance: 0, collection: 0, item: 0, owner: RECIPIENT }.into(),
		);
	});
}

#[test]
fn mint_without_deposit_waives_initial_instance_metadata_deposits() {
	new_test_ext().execute_with(|| {
		setup_item();
		let held_before = held(OWNER);
		let metadata_key = key(b"unique");

		assert_eq!(
			Scarcity::mint_without_deposit(
				0,
				0,
				RECIPIENT,
				vec![(metadata_key.clone(), value(b"free"))],
			),
			Ok(0)
		);

		let entry = InstanceMetadata::<Test>::get(0, &metadata_key).expect("metadata exists");
		assert_eq!(entry.value, value(b"free"));
		assert_eq!(entry.deposit, 0);
		assert_eq!(InstanceMetadataCount::<Test>::get(0), 1);
		assert_eq!(held(OWNER), held_before);

		assert_ok!(Scarcity::set_instance_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			metadata_key.clone(),
			Some(value(b"now-deposited")),
		));
		let charged = InstanceMetadata::<Test>::get(0, &metadata_key).expect("metadata remains");
		assert!(charged.deposit > 0);
		assert_eq!(held(OWNER), held_before + charged.deposit);
		assert_eq!(Scarcity::instance_metadata_of(0, &metadata_key), Some(value(b"now-deposited")),);
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn mint_without_deposit_checks_collection_item_and_destination() {
	new_test_ext().execute_with(|| {
		setup_item();
		define(0);

		assert_noop!(
			Scarcity::mint_without_deposit(99, 0, RECIPIENT, metadata(&[])),
			Error::<Test>::UnknownCollection
		);
		assert_noop!(
			Scarcity::mint_without_deposit(0, 99, RECIPIENT, metadata(&[])),
			Error::<Test>::UnknownItem
		);
		assert_ok!(Scarcity::mint_without_deposit(0, 0, RECIPIENT, metadata(&[])));
		assert_noop!(
			Scarcity::mint_without_deposit(0, 1, RECIPIENT, metadata(&[])),
			Error::<Test>::AddressOccupied
		);
	});
}

#[test]
fn item_defs_are_immutable() {
	new_test_ext().execute_with(|| {
		setup_item();
		let before = ItemDefs::<Test>::get(0, 0).expect("first definition exists");

		define(0);
		assert_eq!(ItemDefs::<Test>::get(0, 0), Some(before));
	});
}

#[test]
fn transfer_moves_ownership_and_updates_reverse_index() {
	new_test_ext().execute_with(|| {
		setup_item();
		MockNow::set(10);
		mint_with_metadata(0, RECIPIENT, &[(b"unique", b"moves")]);
		assert_eq!(frame_system::Account::<Test>::get(RECIPIENT).sufficients, 0);

		MockNow::set(20);
		let (validity, val, origin) = validate_transfer(RECIPIENT, OTHER).unwrap();
		assert_eq!(validity.priority, 10);
		assert_eq!(validity.provides, vec![("Scarcity", (0u64, 0u64)).encode()]);
		let pre = prepare_transfer(val, &origin, OTHER);
		assert!(!NftsByOwner::<Test>::contains_key(RECIPIENT));
		let post_info = Scarcity::transfer(origin, OTHER).unwrap();
		post_dispatch(pre, Ok(()));
		assert_eq!(post_info.pays_fee, Pays::No);
		assert!(!NftsByOwner::<Test>::contains_key(RECIPIENT));
		let moved = NftsByOwner::<Test>::get(OTHER).expect("recipient has NFT");
		assert_eq!(moved.instance, 0);
		assert_eq!(moved.last_moved, 20);
		assert_eq!(moved.state_nonce, 1);
		assert_eq!(Instances::<Test>::get(0), Some(OTHER));
		assert_eq!(frame_system::Account::<Test>::get(RECIPIENT).sufficients, 0);
		assert_eq!(frame_system::Account::<Test>::get(OTHER).sufficients, 0);
		assert_eq!(Scarcity::instance_metadata_of(0, &key(b"unique")), Some(value(b"moves")),);
		System::assert_has_event(
			Event::<Test>::Transferred { instance: 0, from: RECIPIENT, to: OTHER }.into(),
		);
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn depositless_instance_transfers_through_extension_pipeline() {
	new_test_ext().execute_with(|| {
		setup_item();
		let held_before = held(OWNER);
		MockNow::set(10);
		assert_ok!(Scarcity::mint_without_deposit(0, 0, RECIPIENT, metadata(&[])));

		MockNow::set(20);
		let (validity, val, origin) = validate_transfer(RECIPIENT, OTHER).unwrap();
		assert_eq!(validity.priority, 10);
		let pre = prepare_transfer(val, &origin, OTHER);
		assert_ok!(Scarcity::transfer(origin, OTHER));
		post_dispatch(pre, Ok(()));

		assert!(!NftsByOwner::<Test>::contains_key(RECIPIENT));
		let moved = NftsByOwner::<Test>::get(OTHER).expect("recipient has depositless NFT");
		assert_eq!(moved.instance, 0);
		assert_eq!(moved.last_moved, 20);
		assert_eq!(moved.state_nonce, 1);
		assert_eq!(Instances::<Test>::get(0), Some(OTHER));
		assert!(!InstanceDeposits::<Test>::contains_key(0));
		assert_eq!(held(OWNER), held_before);
	});
}

#[test]
fn transfer_priority_scales_with_rest_time() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);

		MockNow::set(0);
		let (fresh, _, _) = validate_transfer(RECIPIENT, OTHER).unwrap();
		assert_eq!(fresh.priority, 0);

		MockNow::set(100);
		let (rested, _, _) = validate_transfer(RECIPIENT, OTHER).unwrap();
		assert!(rested.priority > fresh.priority);

		MockNow::set(2_000_000);
		let (capped, _, _) = validate_transfer(RECIPIENT, OTHER).unwrap();
		assert_eq!(capped.priority, 1_000_000);
	});
}

#[test]
fn transfer_requires_owner_signature() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);

		assert_no_nft(
			validate_transfer_as(OTHER, OWNER, current_authorization(RECIPIENT))
				.err()
				.expect("a key without an NFT cannot use another NFT's authorization"),
		);
	});
}

#[test]
fn stale_authorization_cannot_act_on_reused_purse() {
	new_test_ext().execute_with(|| {
		setup_item();
		define(0);
		mint(0, RECIPIENT);
		let stale_authorization = current_authorization(RECIPIENT);

		let (_, val, origin) = validate_burn_as(RECIPIENT, stale_authorization.clone()).unwrap();
		let pre = prepare_burn(val, &origin);
		assert_ok!(Scarcity::burn(origin));
		post_dispatch(pre, Ok(()));

		mint(1, RECIPIENT);
		let replacement =
			NftsByOwner::<Test>::get(RECIPIENT).expect("a different NFT reused the purse");
		assert_eq!(replacement.instance, 1);

		assert_state_mismatch(
			validate_transfer_as(RECIPIENT, OTHER, stale_authorization.clone())
				.err()
				.expect("the transfer authorization names the burned instance"),
		);
		assert_state_mismatch(
			validate_burn_as(RECIPIENT, stale_authorization)
				.err()
				.expect("the burn authorization names the burned instance"),
		);
		assert_eq!(NftsByOwner::<Test>::get(RECIPIENT), Some(replacement));
	});
}

#[test]
fn prepare_rechecks_authorized_state_before_consuming_nft() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);
		let (_, val, origin) = validate_transfer(RECIPIENT, OTHER).unwrap();

		NftsByOwner::<Test>::mutate(RECIPIENT, |maybe_nft| {
			maybe_nft.as_mut().expect("minted NFT exists").state_nonce = 1;
		});
		let call = transfer_call(OTHER);
		let error = extension_for_val(&val)
			.prepare(val, &origin, &call, &Default::default(), 0)
			.err()
			.expect("changed state must fail preparation");

		assert_state_mismatch(error);
		assert_eq!(
			NftsByOwner::<Test>::get(RECIPIENT).map(|nft| nft.state_nonce),
			Some(1),
			"a failed preparation must not consume the changed state",
		);
	});
}

#[test]
fn same_block_double_use_blocked() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);
		let authorization = current_authorization(RECIPIENT);

		let (_, val, origin) =
			validate_transfer_as(RECIPIENT, OTHER, authorization.clone()).unwrap();
		let _pre = prepare_transfer(val, &origin, OTHER);
		assert!(!NftsByOwner::<Test>::contains_key(RECIPIENT));
		assert_no_nft(
			validate_transfer_as(RECIPIENT, OTHER, authorization)
				.err()
				.expect("the NFT is held by the prepared transaction"),
		);
	});
}

#[test]
fn failed_dispatch_restores_and_locks() {
	new_test_ext().execute_with(|| {
		setup_item();
		define(0);
		mint(0, OWNER);

		// Race shape: the destination is empty at validation time and becomes occupied before
		// dispatch — the only failure path that still reaches dispatch now that validate
		// pre-checks the destination.
		let (_, val, origin) = validate_transfer(OWNER, 4).unwrap();
		mint(1, 4);
		let pre = prepare_transfer(val, &origin, 4);
		let dispatch = Scarcity::transfer(origin, 4);
		assert_noop!(dispatch, Error::<Test>::AddressOccupied);
		post_dispatch(pre, Err(Error::<Test>::AddressOccupied.into()));
		assert_eq!(NftsByOwner::<Test>::get(OWNER).map(|nft| nft.instance), Some(0));
		assert_eq!(Locked::<Test>::get(OWNER), Some(LockInfo { retries: 1, until: 60 }));
		assert_ok!(Scarcity::do_try_state());
		// While locked, even a fresh empty destination is rejected at the pool.
		assert!(validate_transfer(OWNER, 5).is_err());

		MockNow::set(60);
		let (_, val, origin) = validate_transfer(OWNER, 5).unwrap();
		mint(1, 5);
		let pre = prepare_transfer(val, &origin, 5);
		let dispatch = Scarcity::transfer(origin, 5);
		assert_noop!(dispatch, Error::<Test>::AddressOccupied);
		post_dispatch(pre, Err(Error::<Test>::AddressOccupied.into()));
		assert_eq!(Locked::<Test>::get(OWNER), Some(LockInfo { retries: 2, until: 180 }));
	});
}

#[test]
fn success_clears_lock() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);
		Locked::<Test>::insert(RECIPIENT, LockInfo { retries: 3, until: 10 });
		MockNow::set(10);

		let (_, val, origin) = validate_transfer(RECIPIENT, OTHER).unwrap();
		let pre = prepare_transfer(val, &origin, OTHER);
		assert_ok!(Scarcity::transfer(origin, OTHER));
		post_dispatch(pre, Ok(()));
		assert!(!Locked::<Test>::contains_key(RECIPIENT));
	});
}

#[test]
fn non_transfer_calls_pass_through() {
	new_test_ext().execute_with(|| {
		let call = RuntimeCall::Scarcity(crate::Call::create_collection {});
		let (validity, val, origin) = scarcity_extension(authorization(0, 0))
			.validate(
				RuntimeOrigin::signed(OWNER),
				&call,
				&Default::default(),
				0,
				(),
				&TxBaseImplication(()),
				TransactionSource::External,
			)
			.unwrap();

		assert_eq!(validity.priority, 0);
		assert!(matches!(val, Val::NotUsing));
		assert!(matches!(origin.as_system_ref(), Some(frame_system::Origin::<Test>::Signed(who)) if *who == OWNER));
	});
}

#[test]
fn pool_rejects_self_transfer_without_lock() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);
		assert!(validate_transfer(RECIPIENT, RECIPIENT).is_err());
		// Pool rejection is side-effect free: NFT untouched, no failure lock written.
		assert!(NftsByOwner::<Test>::contains_key(RECIPIENT));
		assert_eq!(Locked::<Test>::get(RECIPIENT), None);
	});
}

#[test]
fn pool_rejects_occupied_destination_without_lock() {
	new_test_ext().execute_with(|| {
		setup_item();
		define(0);
		mint(0, RECIPIENT);
		mint(1, OTHER);
		assert!(validate_transfer(RECIPIENT, OTHER).is_err());
		assert!(NftsByOwner::<Test>::contains_key(RECIPIENT));
		assert_eq!(Locked::<Test>::get(RECIPIENT), None);
	});
}

#[test]
fn one_nft_per_key_on_transfer() {
	new_test_ext().execute_with(|| {
		setup_item();
		define(0);
		mint(0, RECIPIENT);
		mint(1, OTHER);
		let nft = NftsByOwner::<Test>::take(RECIPIENT).expect("minted NFT exists");

		assert_noop!(
			Scarcity::transfer(nft_origin(RECIPIENT, nft), OTHER),
			Error::<Test>::AddressOccupied
		);
	});
}

#[test]
fn transfer_to_self_rejected() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);
		let nft = NftsByOwner::<Test>::take(RECIPIENT).expect("minted NFT exists");

		assert_noop!(
			Scarcity::transfer(nft_origin(RECIPIENT, nft), RECIPIENT),
			Error::<Test>::SelfTransfer
		);
	});
}

#[test]
fn burn_releases_instance_deposit_and_preserves_supply_and_item_metadata() {
	new_test_ext().execute_with(|| {
		setup_item();
		let metadata_key = key(b"survives");
		assert_ok!(Scarcity::set_item_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			0,
			metadata_key.clone(),
			Some(value(b"burn")),
		));
		MockNow::set(10);
		mint_with_metadata(0, RECIPIENT, &[(b"unique", b"removed")]);
		let instance_deposit =
			InstanceDeposits::<Test>::get(0).expect("ordinary mint has a deposit");
		let instance_metadata_deposit = InstanceMetadata::<Test>::get(0, key(b"unique"))
			.expect("metadata exists")
			.deposit;
		let held_before = held(OWNER);
		let supply = ItemDefs::<Test>::get(0, 0).unwrap().supply;
		let burned_authorization = current_authorization(RECIPIENT);

		MockNow::set(25);
		let (validity, val, origin) =
			validate_burn_as(RECIPIENT, burned_authorization.clone()).unwrap();
		assert_eq!(validity.priority, 15);
		let pre = prepare_burn(val, &origin);
		assert!(!NftsByOwner::<Test>::contains_key(RECIPIENT));
		let post_info = Scarcity::burn(origin).unwrap();
		post_dispatch(pre, Ok(()));

		assert_eq!(post_info.pays_fee, Pays::No);
		assert_eq!(post_info.actual_weight, Some(<() as crate::weights::WeightInfo>::burn(1)),);
		assert!(!NftsByOwner::<Test>::contains_key(RECIPIENT));
		assert_eq!(frame_system::Account::<Test>::get(RECIPIENT).sufficients, 0);
		assert!(!Instances::<Test>::contains_key(0));
		assert!(!InstanceDeposits::<Test>::contains_key(0));
		assert!(!InstanceMetadataCount::<Test>::contains_key(0));
		assert_eq!(InstanceMetadata::<Test>::iter_prefix(0).count(), 0);
		assert_eq!(Scarcity::instance_metadata_of(0, &key(b"unique")), None);
		let definition = ItemDefs::<Test>::get(0, 0).unwrap();
		assert_eq!(definition.supply, supply);
		assert_eq!(definition.live_supply, 0);
		assert_eq!(
			Scarcity::item_metadata_of(0, 0, &metadata_key),
			Some(value(b"burn")),
			"item-definition metadata must outlive a burned instance",
		);
		assert_eq!(held(OWNER), held_before - instance_deposit - instance_metadata_deposit);
		System::assert_has_event(Event::<Test>::Burned { instance: 0 }.into());

		assert_no_nft(
			validate_transfer_as(RECIPIENT, OTHER, burned_authorization.clone())
				.err()
				.expect("burned purse has no NFT"),
		);
		assert_no_nft(
			validate_burn_as(RECIPIENT, burned_authorization)
				.err()
				.expect("burned purse has no NFT"),
		);
	});
}

#[test]
fn burn_of_depositless_instance_releases_nothing_and_cleans_indexes() {
	new_test_ext().execute_with(|| {
		setup_item();
		assert_ok!(Scarcity::mint_without_deposit(
			0,
			0,
			RECIPIENT,
			metadata(&[(b"unique", b"free")]),
		));
		assert!(!InstanceDeposits::<Test>::contains_key(0));
		let held_before = held(OWNER);

		let (_, val, origin) = validate_burn(RECIPIENT).unwrap();
		let pre = prepare_burn(val, &origin);
		assert_ok!(Scarcity::burn(origin));
		post_dispatch(pre, Ok(()));

		assert!(!NftsByOwner::<Test>::contains_key(RECIPIENT));
		assert!(!Instances::<Test>::contains_key(0));
		assert!(!InstanceDeposits::<Test>::contains_key(0));
		assert!(!InstanceMetadataCount::<Test>::contains_key(0));
		assert_eq!(InstanceMetadata::<Test>::iter_prefix(0).count(), 0);
		assert_eq!(held(OWNER), held_before);
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn collection_owner_can_force_burn_an_instance() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint_with_metadata(0, RECIPIENT, &[(b"effect", b"healing")]);
		Locked::<Test>::insert(RECIPIENT, LockInfo { retries: 1, until: 60 });
		let instance_deposit =
			InstanceDeposits::<Test>::get(0).expect("ordinary mint has a deposit");
		let metadata_deposit = InstanceMetadata::<Test>::get(0, key(b"effect"))
			.expect("instance metadata exists")
			.deposit;
		let held_before = held(OWNER);

		assert_noop!(
			Scarcity::force_burn(RuntimeOrigin::signed(OTHER), 0),
			Error::<Test>::NoPermission
		);
		assert_noop!(
			Scarcity::force_burn(RuntimeOrigin::signed(OWNER), 99),
			Error::<Test>::UnknownInstance
		);
		let post_info = Scarcity::force_burn(RuntimeOrigin::signed(OWNER), 0).unwrap();
		assert_eq!(post_info.pays_fee, Pays::Yes);
		assert_eq!(
			post_info.actual_weight,
			Some(<() as crate::weights::WeightInfo>::force_burn(1)),
		);

		assert!(!NftsByOwner::<Test>::contains_key(RECIPIENT));
		assert!(!Instances::<Test>::contains_key(0));
		assert!(!InstanceDeposits::<Test>::contains_key(0));
		assert!(!InstanceMetadataCount::<Test>::contains_key(0));
		assert_eq!(InstanceMetadata::<Test>::iter_prefix(0).count(), 0);
		assert!(!Locked::<Test>::contains_key(RECIPIENT));
		assert_eq!(frame_system::Account::<Test>::get(RECIPIENT).sufficients, 0);
		let definition = ItemDefs::<Test>::get(0, 0).expect("definition remains");
		assert_eq!(definition.supply, 1);
		assert_eq!(definition.live_supply, 0);
		assert_eq!(held(OWNER), held_before - instance_deposit - metadata_deposit);
		System::assert_has_event(Event::<Test>::Burned { instance: 0 }.into());
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn collection_owner_can_force_transfer_an_instance() {
	new_test_ext().execute_with(|| {
		setup_item();
		define(0);
		MockNow::set(10);
		mint_with_metadata(0, RECIPIENT, &[(b"effect", b"healing")]);
		mint(1, OTHER);
		Locked::<Test>::insert(RECIPIENT, LockInfo { retries: 1, until: 60 });
		let deposit = InstanceDeposits::<Test>::get(0).expect("ordinary mint has a deposit");
		let held_before = held(OWNER);
		let target = 4;

		assert_noop!(
			Scarcity::force_transfer(RuntimeOrigin::signed(OTHER), 0, target),
			Error::<Test>::NoPermission
		);
		assert_noop!(
			Scarcity::force_transfer(RuntimeOrigin::signed(OWNER), 99, target),
			Error::<Test>::UnknownInstance
		);
		assert_noop!(
			Scarcity::force_transfer(RuntimeOrigin::signed(OWNER), 0, RECIPIENT),
			Error::<Test>::SelfTransfer
		);
		assert_noop!(
			Scarcity::force_transfer(RuntimeOrigin::signed(OWNER), 0, OTHER),
			Error::<Test>::AddressOccupied
		);

		MockNow::set(25);
		assert_ok!(Scarcity::force_transfer(RuntimeOrigin::signed(OWNER), 0, target));

		assert!(!NftsByOwner::<Test>::contains_key(RECIPIENT));
		let moved = NftsByOwner::<Test>::get(target).expect("target owns the NFT");
		assert_eq!(moved.instance, 0);
		assert_eq!(moved.last_moved, 25);
		assert_eq!(moved.state_nonce, 1);
		assert_eq!(Instances::<Test>::get(0), Some(target));
		assert_eq!(frame_system::Account::<Test>::get(RECIPIENT).sufficients, 0);
		assert_eq!(frame_system::Account::<Test>::get(target).sufficients, 0);
		assert_eq!(InstanceDeposits::<Test>::get(0), Some(deposit));
		assert_eq!(Scarcity::instance_metadata_of(0, &key(b"effect")), Some(value(b"healing")),);
		assert_eq!(held(OWNER), held_before);
		assert!(!Locked::<Test>::contains_key(RECIPIENT));
		System::assert_has_event(
			Event::<Test>::ForceTransferred { instance: 0, from: RECIPIENT, to: target }.into(),
		);
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn force_transfer_away_and_back_invalidates_old_authorization() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);
		let stale = current_authorization(RECIPIENT);

		assert_ok!(Scarcity::force_transfer(RuntimeOrigin::signed(OWNER), 0, OTHER));
		assert_ok!(Scarcity::force_transfer(RuntimeOrigin::signed(OWNER), 0, RECIPIENT));
		let returned = NftsByOwner::<Test>::get(RECIPIENT).expect("NFT returned to its purse");
		assert_eq!(returned.instance, 0);
		assert_eq!(returned.state_nonce, 2);

		assert_state_mismatch(
			validate_transfer_as(RECIPIENT, OTHER, stale.clone())
				.err()
				.expect("the transfer authorization names an old ownership state"),
		);
		assert_state_mismatch(
			validate_burn_as(RECIPIENT, stale)
				.err()
				.expect("the burn authorization names an old ownership state"),
		);
		assert!(validate_transfer(RECIPIENT, OTHER).is_ok());
	});
}

#[test]
fn force_transfer_nonce_overflow_is_atomic() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);
		Locked::<Test>::insert(RECIPIENT, LockInfo { retries: 1, until: 60 });
		NftsByOwner::<Test>::mutate(RECIPIENT, |maybe_nft| {
			maybe_nft.as_mut().expect("minted NFT exists").state_nonce = u64::MAX;
		});
		let before = NftsByOwner::<Test>::get(RECIPIENT).expect("minted NFT exists");

		assert_noop!(
			Scarcity::force_transfer(RuntimeOrigin::signed(OWNER), 0, OTHER),
			Error::<Test>::StateNonceOverflow
		);

		assert_eq!(NftsByOwner::<Test>::get(RECIPIENT), Some(before));
		assert!(!NftsByOwner::<Test>::contains_key(OTHER));
		assert_eq!(Instances::<Test>::get(0), Some(RECIPIENT));
		assert_eq!(Locked::<Test>::get(RECIPIENT), Some(LockInfo { retries: 1, until: 60 }));
	});
}

#[test]
fn holder_transfer_nonce_overflow_restores_the_nft() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);
		NftsByOwner::<Test>::mutate(RECIPIENT, |maybe_nft| {
			maybe_nft.as_mut().expect("minted NFT exists").state_nonce = u64::MAX;
		});

		let (_, val, origin) = validate_transfer(RECIPIENT, OTHER).unwrap();
		let pre = prepare_transfer(val, &origin, OTHER);
		let dispatch = Scarcity::transfer(origin, OTHER);
		assert_noop!(dispatch, Error::<Test>::StateNonceOverflow);
		post_dispatch(pre, Err(Error::<Test>::StateNonceOverflow.into()));

		assert_eq!(NftsByOwner::<Test>::get(RECIPIENT).map(|nft| nft.state_nonce), Some(u64::MAX),);
		assert!(!NftsByOwner::<Test>::contains_key(OTHER));
		assert_eq!(Instances::<Test>::get(0), Some(RECIPIENT));
	});
}

#[test]
fn delete_item_requires_dependencies_to_be_removed_and_never_reuses_its_id() {
	new_test_ext().execute_with(|| {
		setup_item();
		assert_ok!(Scarcity::set_item_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			0,
			key(b"default"),
			Some(value(b"potion")),
		));
		mint(0, RECIPIENT);

		assert_noop!(
			Scarcity::delete_item(RuntimeOrigin::signed(OTHER), 0, 0),
			Error::<Test>::NoPermission
		);
		assert_noop!(
			Scarcity::delete_item(RuntimeOrigin::signed(OWNER), 0, 0),
			Error::<Test>::ItemInUse
		);
		assert_ok!(Scarcity::force_burn(RuntimeOrigin::signed(OWNER), 0));
		assert_noop!(
			Scarcity::delete_item(RuntimeOrigin::signed(OWNER), 0, 0),
			Error::<Test>::ItemMetadataNotEmpty
		);
		assert_ok!(Scarcity::set_item_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			0,
			key(b"default"),
			None,
		));

		let definition_deposit = ItemDefs::<Test>::get(0, 0).unwrap().deposit;
		let held_before = held(OWNER);
		assert_ok!(Scarcity::delete_item(RuntimeOrigin::signed(OWNER), 0, 0));
		assert!(!ItemDefs::<Test>::contains_key(0, 0));
		let info = Collections::<Test>::get(0).expect("collection remains");
		assert_eq!(info.item_count, 0);
		assert_eq!(info.next_item_index, 1);
		assert_eq!(held(OWNER), held_before - definition_deposit);
		System::assert_has_event(Event::<Test>::ItemDeleted { collection: 0, item: 0 }.into());

		define(0);
		assert!(!ItemDefs::<Test>::contains_key(0, 0));
		assert!(ItemDefs::<Test>::contains_key(0, 1));
		assert_eq!(Collections::<Test>::get(0).unwrap().item_count, 1);
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn delete_collection_requires_dependencies_and_releases_its_deposit() {
	new_test_ext().execute_with(|| {
		setup_item();
		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			key(b"name"),
			Some(value(b"potions")),
		));

		assert_noop!(
			Scarcity::delete_collection(RuntimeOrigin::signed(OTHER), 0),
			Error::<Test>::NoPermission
		);
		assert_noop!(
			Scarcity::delete_collection(RuntimeOrigin::signed(OWNER), 0),
			Error::<Test>::CollectionItemsNotEmpty
		);
		assert_ok!(Scarcity::delete_item(RuntimeOrigin::signed(OWNER), 0, 0));
		assert_noop!(
			Scarcity::delete_collection(RuntimeOrigin::signed(OWNER), 0),
			Error::<Test>::CollectionMetadataNotEmpty
		);
		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			key(b"name"),
			None,
		));

		let collection_deposit = Collections::<Test>::get(0).unwrap().collection_deposit;
		assert_eq!(held(OWNER), collection_deposit);
		assert_ok!(Scarcity::delete_collection(RuntimeOrigin::signed(OWNER), 0));
		assert!(!Collections::<Test>::contains_key(0));
		assert_eq!(held(OWNER), 0);
		System::assert_has_event(Event::<Test>::CollectionDeleted { collection: 0 }.into());

		assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));
		assert!(!Collections::<Test>::contains_key(0));
		assert!(Collections::<Test>::contains_key(1));
		assert_eq!(NextCollectionId::<Test>::get(), 2);
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn burn_uses_rest_time_priority_and_rejects_locked_keys() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);

		MockNow::set(100);
		let (validity, _, _) = validate_burn(RECIPIENT).unwrap();
		assert_eq!(validity.priority, 100);

		Locked::<Test>::insert(RECIPIENT, LockInfo { retries: 1, until: 101 });
		assert!(validate_burn(RECIPIENT).is_err());
		assert!(NftsByOwner::<Test>::contains_key(RECIPIENT));

		MockNow::set(101);
		assert!(validate_burn(RECIPIENT).is_ok());
	});
}

#[test]
fn failed_burn_restores_nft_and_locks_purse_key() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);
		ItemDefs::<Test>::mutate(0, 0, |maybe_definition| {
			maybe_definition.as_mut().expect("item exists").live_supply = 0;
		});

		let (_, val, origin) = validate_burn(RECIPIENT).unwrap();
		let pre = prepare_burn(val, &origin);
		let dispatch = Scarcity::burn(origin);
		let dispatch_error =
			dispatch.expect_err("the inconsistent live supply must make burn fail").error;
		// The burn's storage transaction restores its reverse index and deposit. The extension
		// still owns the NFT until post-dispatch handles the failed capability call.
		assert!(!NftsByOwner::<Test>::contains_key(RECIPIENT));
		assert_eq!(Instances::<Test>::get(0), Some(RECIPIENT));
		assert!(InstanceDeposits::<Test>::contains_key(0));

		post_dispatch(pre, Err(dispatch_error));
		assert!(NftsByOwner::<Test>::contains_key(RECIPIENT));
		assert_eq!(Locked::<Test>::get(RECIPIENT), Some(LockInfo { retries: 1, until: 60 }));
	});
}

#[test]
fn try_state_rejects_collection_identifier_at_or_above_next() {
	new_test_ext().execute_with(|| {
		setup_item();
		NextCollectionId::<Test>::put(0);

		assert_try_state_error("collection identifier is not below NextCollectionId");
	});
}

#[test]
fn try_state_rejects_non_sequential_item_catalogue() {
	new_test_ext().execute_with(|| {
		setup_item();
		let definition = ItemDefs::<Test>::take(0, 0).expect("item definition exists");
		ItemDefs::<Test>::insert(0, 1, definition);

		assert_try_state_error("item index is not below the collection's next item index");
	});
}

#[test]
fn try_state_rejects_item_counter_mismatch() {
	new_test_ext().execute_with(|| {
		setup_item();
		Collections::<Test>::mutate(0, |maybe_info| {
			maybe_info.as_mut().expect("collection exists").item_count = 2;
		});

		assert_try_state_error("collection item count does not match stored definitions");
	});
}

#[test]
fn try_state_rejects_collection_metadata_counter_mismatch() {
	new_test_ext().execute_with(|| {
		assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));
		Collections::<Test>::mutate(0, |maybe_info| {
			maybe_info.as_mut().expect("collection exists").metadata_count = 1;
		});

		assert_try_state_error("collection metadata count does not match stored entries");
	});
}

#[test]
fn try_state_rejects_item_metadata_counter_mismatch() {
	new_test_ext().execute_with(|| {
		setup_item();
		ItemDefs::<Test>::mutate(0, 0, |maybe_definition| {
			maybe_definition.as_mut().expect("item definition exists").metadata_count = 1;
		});

		assert_try_state_error("item metadata count does not match stored entries");
	});
}

#[test]
fn try_state_rejects_orphaned_item_definition() {
	new_test_ext().execute_with(|| {
		setup_item();
		let definition = ItemDefs::<Test>::get(0, 0).expect("item definition exists");
		ItemDefs::<Test>::insert(99, 0, definition);

		assert_try_state_error("ItemDefs entry has no matching collection");
	});
}

#[test]
fn try_state_rejects_instance_identifier_at_or_above_next() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);
		NextInstanceId::<Test>::put(0);

		assert_try_state_error("NFT instance is not below NextInstanceId");
	});
}

#[test]
fn try_state_accepts_nft_owner_without_system_account() {
	new_test_ext().execute_with(|| {
		setup_item();
		let nft_only_purse = 99;
		mint(0, nft_only_purse);

		assert!(!frame_system::Account::<Test>::contains_key(nft_only_purse));
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn try_state_rejects_instance_metadata_count_mismatch() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint_with_metadata(0, RECIPIENT, &[(b"unique", b"value")]);
		InstanceMetadataCount::<Test>::insert(0, 2);

		assert_try_state_error("instance metadata count does not match stored entries");
	});
}

#[test]
fn try_state_rejects_live_instance_without_metadata_count() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);
		InstanceMetadataCount::<Test>::remove(0);

		assert_try_state_error("live instance has no metadata count entry");
	});
}

#[test]
fn try_state_rejects_orphaned_instance_metadata() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint_with_metadata(0, RECIPIENT, &[(b"unique", b"value")]);
		let entry = InstanceMetadata::<Test>::take(0, key(b"unique")).expect("metadata exists");
		InstanceMetadata::<Test>::insert(99, key(b"unique"), entry);

		assert_try_state_error("InstanceMetadata identifier is not below NextInstanceId");
	});
}

#[test]
fn try_state_rejects_nft_without_item_definition() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);
		NftsByOwner::<Test>::mutate(RECIPIENT, |maybe_nft| {
			maybe_nft.as_mut().expect("minted NFT exists").item = 99;
		});

		assert_try_state_error("NFT has no matching item definition");
	});
}

#[test]
fn try_state_rejects_live_supply_below_stored_instances() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);
		ItemDefs::<Test>::mutate(0, 0, |maybe_definition| {
			maybe_definition.as_mut().expect("item definition exists").live_supply = 0;
		});

		assert_try_state_error("item live supply is below its stored instance count");
	});
}

#[test]
fn try_state_rejects_live_supply_above_minted_supply() {
	new_test_ext().execute_with(|| {
		setup_item();
		ItemDefs::<Test>::mutate(0, 0, |maybe_definition| {
			maybe_definition.as_mut().expect("item definition exists").live_supply = 1;
		});

		assert_try_state_error("item live supply exceeds its minted supply");
	});
}

#[test]
fn try_state_rejects_lock_without_nft() {
	new_test_ext().execute_with(|| {
		Locked::<Test>::insert(RECIPIENT, LockInfo { retries: 1, until: 60 });

		assert_try_state_error("Locked entry has no matching NFT");
	});
}

#[test]
fn try_state_rejects_zero_retry_count() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);
		Locked::<Test>::insert(RECIPIENT, LockInfo { retries: 0, until: 60 });

		assert_try_state_error("Locked retry count must begin at one");
	});
}

#[test]
fn try_state_rejects_incorrect_collection_deposit_aggregate() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);
		Collections::<Test>::mutate(0, |maybe_info| {
			maybe_info.as_mut().expect("collection exists").owner_deposit += 1;
		});

		assert_try_state_error("collection owner deposit does not match its stored components");
	});
}

#[test]
fn try_state_accepts_issuer_depositless_transferred_and_burned_states() {
	new_test_ext().execute_with(|| {
		setup_item();
		define(0);
		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			key(b"default"),
			Some(value(b"collection")),
		));
		assert_ok!(Scarcity::set_item_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			0,
			key(b"default"),
			Some(value(b"item")),
		));
		mint_with_metadata(0, RECIPIENT, &[(b"unique", b"ordinary")]);
		mint(1, OTHER);

		let (_, val, origin) = validate_transfer(RECIPIENT, 4).unwrap();
		let pre = prepare_transfer(val, &origin, 4);
		assert_ok!(Scarcity::transfer(origin, 4));
		post_dispatch(pre, Ok(()));

		assert_ok!(Scarcity::mint_without_deposit(0, 0, 5, metadata(&[(b"unique", b"free")]),));
		let (_, val, origin) = validate_burn(5).unwrap();
		let pre = prepare_burn(val, &origin);
		assert_ok!(Scarcity::burn(origin));
		post_dispatch(pre, Ok(()));
		assert_ok!(Scarcity::mint_without_deposit(0, 0, 6, metadata(&[(b"unique", b"live")]),));

		assert_ok!(Scarcity::do_try_state());
		#[cfg(feature = "try-runtime")]
		assert_ok!(<Scarcity as Hooks<u64>>::try_state(System::block_number()));
	});
}
