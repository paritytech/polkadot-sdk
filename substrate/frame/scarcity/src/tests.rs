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
	CollectionMetadata, Collections, Error, Event, InstanceDeposits, Instances, ItemDefs,
	ItemMetadata, Kind, LockInfo, Locked, MetadataKeyOf, MetadataValueOf, Nft, NftsByOwner, Origin,
};
use codec::Encode;
#[cfg(feature = "try-runtime")]
use frame_support::traits::Hooks;
use frame_support::{
	assert_noop, assert_ok,
	dispatch::Pays,
	traits::{Footprint, OriginTrait},
};
use sp_runtime::{
	traits::{TransactionExtension, TxBaseImplication},
	transaction_validity::{
		InvalidTransaction, TransactionSource, TransactionValidityError, ValidTransaction,
	},
	DispatchError, DispatchResult,
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

fn define(collection: u32, kind: Kind, next_variant: Option<u32>) {
	assert_ok!(Scarcity::define_item(
		RuntimeOrigin::signed(OWNER),
		collection,
		kind,
		next_variant,
		metadata(&[]),
	));
}

fn setup_item() {
	assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));
	define(0, Kind::Normal, None);
}

fn mint(item: u32, to: u64) {
	assert_ok!(Scarcity::mint(RuntimeOrigin::signed(OWNER), 0, item, to));
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

fn scarcity_extension() -> AsScarcity<Test> {
	AsScarcity::new(Some(AsScarcityInfo::AsNft))
}

fn validate_transfer(
	signer: u64,
	to: u64,
) -> Result<(ValidTransaction, Val<Test>, RuntimeOrigin), TransactionValidityError> {
	let call = transfer_call(to);
	scarcity_extension().validate(
		RuntimeOrigin::signed(signer),
		&call,
		&Default::default(),
		0,
		(),
		&TxBaseImplication(()),
		TransactionSource::External,
	)
}

fn prepare_transfer(val: Val<Test>, origin: &RuntimeOrigin, to: u64) -> Pre<Test> {
	let call = transfer_call(to);
	scarcity_extension()
		.prepare(val, origin, &call, &Default::default(), 0)
		.unwrap()
}

fn validate_burn(
	signer: u64,
) -> Result<(ValidTransaction, Val<Test>, RuntimeOrigin), TransactionValidityError> {
	let call = burn_call();
	scarcity_extension().validate(
		RuntimeOrigin::signed(signer),
		&call,
		&Default::default(),
		0,
		(),
		&TxBaseImplication(()),
		TransactionSource::External,
	)
}

fn prepare_burn(val: Val<Test>, origin: &RuntimeOrigin) -> Pre<Test> {
	let call = burn_call();
	scarcity_extension()
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

fn post_dispatch(pre: Pre<Test>, result: DispatchResult) {
	assert_ok!(AsScarcity::<Test>::post_dispatch_details(
		pre,
		&Default::default(),
		&Default::default(),
		0,
		&result,
	));
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
fn create_collection_charges_creator_and_stores_ticket() {
	new_test_ext().execute_with(|| {
		assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));

		let info = Collections::<Test>::get(0).expect("collection exists");
		assert_eq!(info.ticket, TestConsideration);
		assert!(matches!(
			consideration_events().as_slice(),
			[ConsiderationEvent::New { who: OWNER, footprint }]
				if footprint.count == 1 && footprint.size > 0
		));
	});
}

#[test]
fn define_item_charges_issuer_by_encoded_size() {
	new_test_ext().execute_with(|| {
		assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));
		clear_consideration_events();

		define(0, Kind::Normal, None);
		let definition = ItemDefs::<Test>::get(0, 0).expect("item definition exists");
		assert_eq!(definition.ticket, TestConsideration);
		assert!(matches!(
			consideration_events().as_slice(),
			[ConsiderationEvent::New { who: OWNER, footprint }]
				if footprint.count == 1
					&& footprint.size == definition.encoded_size() as u64
					&& footprint.size > 0
		));
	});
}

#[test]
fn mint_charges_collection_owner_and_stores_instance_deposit() {
	new_test_ext().execute_with(|| {
		setup_item();
		clear_consideration_events();

		mint(0, RECIPIENT);
		assert!(InstanceDeposits::<Test>::contains_key(0));
		assert!(matches!(
			consideration_events().as_slice(),
			[ConsiderationEvent::New { who: OWNER, footprint }]
				if footprint.count == 3 && footprint.size > 0
		));
	});
}

#[test]
fn define_item_requires_collection_owner() {
	new_test_ext().execute_with(|| {
		assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));
		assert_noop!(
			Scarcity::define_item(
				RuntimeOrigin::signed(OTHER),
				0,
				Kind::Normal,
				None,
				metadata(&[]),
			),
			Error::<Test>::NoPermission
		);
		assert_noop!(
			Scarcity::define_item(
				RuntimeOrigin::signed(OWNER),
				99,
				Kind::Normal,
				None,
				metadata(&[]),
			),
			Error::<Test>::UnknownCollection
		);
	});
}

#[test]
fn define_item_assigns_incremental_indexes() {
	new_test_ext().execute_with(|| {
		assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));
		define(0, Kind::Normal, None);
		define(0, Kind::Special, None);
		define(0, Kind::Charm, None);

		assert!(ItemDefs::<Test>::contains_key(0, 0));
		assert!(ItemDefs::<Test>::contains_key(0, 1));
		assert!(ItemDefs::<Test>::contains_key(0, 2));
		assert_eq!(Collections::<Test>::get(0).unwrap().next_item_index, 3);
		assert_eq!(ItemDefs::<Test>::get(0, 0).unwrap().supply, 0);
		assert_eq!(ItemDefs::<Test>::get(0, 1).unwrap().supply, 0);
		assert_eq!(ItemDefs::<Test>::get(0, 2).unwrap().supply, 0);
	});
}

#[test]
fn define_item_validates_next_variant() {
	new_test_ext().execute_with(|| {
		assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));
		assert_noop!(
			Scarcity::define_item(
				RuntimeOrigin::signed(OWNER),
				0,
				Kind::Special,
				Some(0),
				metadata(&[]),
			),
			Error::<Test>::UnknownVariant
		);

		define(0, Kind::Charm, None);
		define(0, Kind::Special, Some(0));
		assert_eq!(ItemDefs::<Test>::get(0, 1).unwrap().next_variant, Some(0));
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
		assert_eq!(Scarcity::metadata_of(0, 0, &shared), Some(value(b"item")));
		assert_eq!(Scarcity::metadata_of(0, 0, &inherited), Some(value(b"default")));
		assert_eq!(Scarcity::metadata_of(0, 0, &absent), None);
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
		clear_consideration_events();

		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			collection_key.clone(),
			Some(value(b"one")),
		));
		assert_eq!(
			consideration_events(),
			vec![ConsiderationEvent::New { who: OWNER, footprint: Footprint::from_parts(1, 4) }]
		);

		clear_consideration_events();
		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			collection_key.clone(),
			Some(value(b"longer")),
		));
		assert_eq!(
			consideration_events(),
			vec![ConsiderationEvent::Update { who: OWNER, footprint: Footprint::from_parts(1, 7) }],
			"overwrite must update the existing ticket, not charge a second ticket",
		);

		clear_consideration_events();
		assert_ok!(Scarcity::set_collection_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			collection_key,
			None,
		));
		assert_eq!(consideration_events(), vec![ConsiderationEvent::Drop { who: OWNER }]);

		let item_key = key(b"i");
		clear_consideration_events();
		assert_ok!(Scarcity::set_item_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			0,
			item_key.clone(),
			Some(value(b"one")),
		));
		assert_ok!(Scarcity::set_item_metadata(
			RuntimeOrigin::signed(OWNER),
			0,
			0,
			item_key.clone(),
			Some(value(b"longer")),
		));
		assert_ok!(
			Scarcity::set_item_metadata(RuntimeOrigin::signed(OWNER), 0, 0, item_key, None,)
		);
		assert_eq!(
			consideration_events(),
			vec![
				ConsiderationEvent::New { who: OWNER, footprint: Footprint::from_parts(1, 4) },
				ConsiderationEvent::Update { who: OWNER, footprint: Footprint::from_parts(1, 7) },
				ConsiderationEvent::Drop { who: OWNER },
			]
		);
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn removing_absent_metadata_is_a_no_op() {
	new_test_ext().execute_with(|| {
		setup_item();
		clear_consideration_events();
		let events_before = System::events().len();

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

		assert!(consideration_events().is_empty());
		assert_eq!(System::events().len(), events_before);
	});
}

#[test]
fn define_item_accepts_more_than_old_cap_and_charges_each_metadata_entry() {
	new_test_ext().execute_with(|| {
		assert_ok!(Scarcity::create_collection(RuntimeOrigin::signed(OWNER)));
		clear_consideration_events();
		let metadata = (0..41)
			.map(|index| {
				(key(format!("key-{index}").as_bytes()), value(format!("value-{index}").as_bytes()))
			})
			.collect::<Vec<_>>();
		let expected_footprints = metadata
			.iter()
			.map(|(key, value)| Footprint::from_parts(1, key.len().saturating_add(value.len())))
			.collect::<Vec<_>>();

		assert_ok!(Scarcity::define_item(
			RuntimeOrigin::signed(OWNER),
			0,
			Kind::Normal,
			None,
			metadata,
		));

		let definition = ItemDefs::<Test>::get(0, 0).expect("item definition exists");
		assert_eq!(ItemMetadata::<Test>::iter_prefix((0, 0)).count(), 41);
		assert_eq!(Scarcity::metadata_of(0, 0, &key(b"key-40")), Some(value(b"value-40")),);
		let events = consideration_events();
		assert_eq!(
			events.first(),
			Some(&ConsiderationEvent::New {
				who: OWNER,
				footprint: Footprint::from_parts(1, definition.encoded_size()),
			}),
		);
		assert_eq!(events.len(), expected_footprints.len() + 1);
		for (event, footprint) in events[1..].iter().zip(expected_footprints) {
			assert_eq!(
				event,
				&ConsiderationEvent::New { who: OWNER, footprint },
				"every initial metadata entry must create its own deposit ticket",
			);
		}
		assert_ok!(Scarcity::do_try_state());
	});
}

#[test]
fn mint_requires_owner_and_existing_def() {
	new_test_ext().execute_with(|| {
		setup_item();
		assert_noop!(
			Scarcity::mint(RuntimeOrigin::signed(OTHER), 0, 0, RECIPIENT),
			Error::<Test>::NoPermission
		);
		assert_noop!(
			Scarcity::mint(RuntimeOrigin::signed(OWNER), 0, 1, RECIPIENT),
			Error::<Test>::UnknownItem
		);
	});
}

#[test]
fn mint_enforces_one_nft_per_key() {
	new_test_ext().execute_with(|| {
		setup_item();
		define(0, Kind::Special, None);
		mint(0, RECIPIENT);
		assert_noop!(
			Scarcity::mint(RuntimeOrigin::signed(OWNER), 0, 1, RECIPIENT),
			Error::<Test>::AddressOccupied
		);
	});
}

#[test]
fn mint_writes_consistent_state() {
	new_test_ext().execute_with(|| {
		setup_item();
		define(0, Kind::Special, None);
		MockNow::set(1_234);
		mint(0, RECIPIENT);

		let nft = NftsByOwner::<Test>::get(RECIPIENT).expect("minted NFT is stored by owner");
		assert_eq!(nft.instance, 0);
		assert_eq!(Instances::<Test>::get(0), Some(RECIPIENT));
		assert_eq!(ItemDefs::<Test>::get(0, 0).unwrap().supply, 1);
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
fn do_mint_claimed_waives_deposit_and_increments_supply() {
	new_test_ext().execute_with(|| {
		setup_item();
		clear_consideration_events();
		MockNow::set(1_234);

		assert_eq!(Scarcity::do_mint_claimed(0, 0, RECIPIENT), Ok(0));

		let nft = NftsByOwner::<Test>::get(RECIPIENT).expect("claimed NFT is stored by owner");
		assert_eq!(nft.instance, 0);
		assert_eq!(nft.collection, 0);
		assert_eq!(nft.item, 0);
		assert_eq!(nft.minted_at, 1_234);
		assert_eq!(Instances::<Test>::get(0), Some(RECIPIENT));
		assert_eq!(ItemDefs::<Test>::get(0, 0).unwrap().supply, 1);
		assert!(!InstanceDeposits::<Test>::contains_key(0));
		assert!(consideration_events().is_empty());
		System::assert_has_event(
			Event::<Test>::Minted { instance: 0, collection: 0, item: 0, owner: RECIPIENT }.into(),
		);
	});
}

#[test]
fn do_mint_claimed_checks_collection_item_and_destination() {
	new_test_ext().execute_with(|| {
		setup_item();
		define(0, Kind::Special, None);

		assert_noop!(Scarcity::do_mint_claimed(99, 0, RECIPIENT), Error::<Test>::UnknownCollection);
		assert_noop!(Scarcity::do_mint_claimed(0, 99, RECIPIENT), Error::<Test>::UnknownItem);
		assert_ok!(Scarcity::do_mint_claimed(0, 0, RECIPIENT));
		assert_noop!(Scarcity::do_mint_claimed(0, 1, RECIPIENT), Error::<Test>::AddressOccupied);
	});
}

#[test]
fn item_defs_are_immutable() {
	new_test_ext().execute_with(|| {
		setup_item();
		let before = ItemDefs::<Test>::get(0, 0).expect("first definition exists");

		define(0, Kind::Special, None);
		assert_eq!(ItemDefs::<Test>::get(0, 0), Some(before));
	});
}

#[test]
fn transfer_moves_ownership_and_updates_reverse_index() {
	new_test_ext().execute_with(|| {
		setup_item();
		MockNow::set(10);
		mint(0, RECIPIENT);

		MockNow::set(20);
		let (validity, val, origin) = validate_transfer(RECIPIENT, OTHER).unwrap();
		assert_eq!(validity.priority, 10);
		let pre = prepare_transfer(val, &origin, OTHER);
		assert!(!NftsByOwner::<Test>::contains_key(RECIPIENT));
		let post_info = Scarcity::transfer(origin, OTHER).unwrap();
		post_dispatch(pre, Ok(()));
		assert_eq!(post_info.pays_fee, Pays::No);
		assert!(!NftsByOwner::<Test>::contains_key(RECIPIENT));
		let moved = NftsByOwner::<Test>::get(OTHER).expect("recipient has NFT");
		assert_eq!(moved.instance, 0);
		assert_eq!(moved.last_moved, 20);
		assert_eq!(Instances::<Test>::get(0), Some(OTHER));
		System::assert_has_event(Event::<Test>::Transferred { instance: 0, to: OTHER }.into());
	});
}

#[test]
fn claimed_instance_transfers_through_extension_pipeline() {
	new_test_ext().execute_with(|| {
		setup_item();
		clear_consideration_events();
		MockNow::set(10);
		assert_ok!(Scarcity::do_mint_claimed(0, 0, RECIPIENT));

		MockNow::set(20);
		let (validity, val, origin) = validate_transfer(RECIPIENT, OTHER).unwrap();
		assert_eq!(validity.priority, 10);
		let pre = prepare_transfer(val, &origin, OTHER);
		assert_ok!(Scarcity::transfer(origin, OTHER));
		post_dispatch(pre, Ok(()));

		assert!(!NftsByOwner::<Test>::contains_key(RECIPIENT));
		let moved = NftsByOwner::<Test>::get(OTHER).expect("recipient has claimed NFT");
		assert_eq!(moved.instance, 0);
		assert_eq!(moved.last_moved, 20);
		assert_eq!(Instances::<Test>::get(0), Some(OTHER));
		assert!(!InstanceDeposits::<Test>::contains_key(0));
		assert!(consideration_events().is_empty());
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

		assert!(validate_transfer(OTHER, OWNER).is_err());
	});
}

#[test]
fn same_block_double_use_blocked() {
	new_test_ext().execute_with(|| {
		setup_item();
		mint(0, RECIPIENT);

		let (_, val, origin) = validate_transfer(RECIPIENT, OTHER).unwrap();
		let _pre = prepare_transfer(val, &origin, OTHER);
		assert!(!NftsByOwner::<Test>::contains_key(RECIPIENT));
		assert!(validate_transfer(RECIPIENT, OTHER).is_err());
	});
}

#[test]
fn failed_dispatch_restores_and_locks() {
	new_test_ext().execute_with(|| {
		setup_item();
		define(0, Kind::Special, None);
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
		assert!(NftsByOwner::<Test>::contains_key(OWNER));
		assert_eq!(Locked::<Test>::get(OWNER), Some(LockInfo { retries: 0, until: 60 }));
		// While locked, even a fresh empty destination is rejected at the pool.
		assert!(validate_transfer(OWNER, 5).is_err());

		MockNow::set(60);
		let (_, val, origin) = validate_transfer(OWNER, 5).unwrap();
		mint(1, 5);
		let pre = prepare_transfer(val, &origin, 5);
		let dispatch = Scarcity::transfer(origin, 5);
		assert_noop!(dispatch, Error::<Test>::AddressOccupied);
		post_dispatch(pre, Err(Error::<Test>::AddressOccupied.into()));
		assert_eq!(Locked::<Test>::get(OWNER), Some(LockInfo { retries: 1, until: 180 }));
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
		let (validity, val, origin) = scarcity_extension()
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
		define(0, Kind::Special, None);
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
		define(0, Kind::Special, None);
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
		mint(0, RECIPIENT);
		clear_consideration_events();
		let supply = ItemDefs::<Test>::get(0, 0).unwrap().supply;

		MockNow::set(25);
		let (validity, val, origin) = validate_burn(RECIPIENT).unwrap();
		assert_eq!(validity.priority, 15);
		let pre = prepare_burn(val, &origin);
		assert!(!NftsByOwner::<Test>::contains_key(RECIPIENT));
		let post_info = Scarcity::burn(origin).unwrap();
		post_dispatch(pre, Ok(()));

		assert_eq!(post_info.pays_fee, Pays::No);
		assert!(!NftsByOwner::<Test>::contains_key(RECIPIENT));
		assert!(!Instances::<Test>::contains_key(0));
		assert!(!InstanceDeposits::<Test>::contains_key(0));
		assert_eq!(ItemDefs::<Test>::get(0, 0).unwrap().supply, supply);
		assert_eq!(
			Scarcity::metadata_of(0, 0, &metadata_key),
			Some(value(b"burn")),
			"item-definition metadata must outlive a burned instance",
		);
		assert_eq!(consideration_events(), vec![ConsiderationEvent::Drop { who: OWNER }]);
		System::assert_has_event(Event::<Test>::Burned { instance: 0 }.into());

		assert_no_nft(validate_transfer(RECIPIENT, OTHER).err().expect("burned purse has no NFT"));
		assert_no_nft(validate_burn(RECIPIENT).err().expect("burned purse has no NFT"));
	});
}

#[test]
fn burn_of_claimed_instance_releases_nothing_and_cleans_indexes() {
	new_test_ext().execute_with(|| {
		setup_item();
		assert_ok!(Scarcity::do_mint_claimed(0, 0, RECIPIENT));
		assert!(!InstanceDeposits::<Test>::contains_key(0));
		clear_consideration_events();

		let (_, val, origin) = validate_burn(RECIPIENT).unwrap();
		let pre = prepare_burn(val, &origin);
		assert_ok!(Scarcity::burn(origin));
		post_dispatch(pre, Ok(()));

		assert!(!NftsByOwner::<Test>::contains_key(RECIPIENT));
		assert!(!Instances::<Test>::contains_key(0));
		assert!(!InstanceDeposits::<Test>::contains_key(0));
		assert!(consideration_events().is_empty());
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

		Locked::<Test>::insert(RECIPIENT, LockInfo { retries: 0, until: 101 });
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
		MockDropFails::set(true);

		let (_, val, origin) = validate_burn(RECIPIENT).unwrap();
		let pre = prepare_burn(val, &origin);
		let dispatch = Scarcity::burn(origin);
		assert_noop!(dispatch, DispatchError::Other("test consideration drop failed"));
		// The burn's storage transaction restores its reverse index and ticket. The extension
		// still owns the NFT until post-dispatch handles the failed capability call.
		assert!(!NftsByOwner::<Test>::contains_key(RECIPIENT));
		assert_eq!(Instances::<Test>::get(0), Some(RECIPIENT));
		assert!(InstanceDeposits::<Test>::contains_key(0));

		post_dispatch(pre, Err(DispatchError::Other("test consideration drop failed")));
		assert!(NftsByOwner::<Test>::contains_key(RECIPIENT));
		assert_eq!(Locked::<Test>::get(RECIPIENT), Some(LockInfo { retries: 0, until: 60 }));
	});
}

#[test]
fn try_state_accepts_issuer_claimed_transferred_and_burned_states() {
	new_test_ext().execute_with(|| {
		setup_item();
		define(0, Kind::Special, None);
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
		mint(0, RECIPIENT);
		mint(1, OTHER);

		let (_, val, origin) = validate_transfer(RECIPIENT, 4).unwrap();
		let pre = prepare_transfer(val, &origin, 4);
		assert_ok!(Scarcity::transfer(origin, 4));
		post_dispatch(pre, Ok(()));

		assert_ok!(Scarcity::do_mint_claimed(0, 0, 5));
		let (_, val, origin) = validate_burn(5).unwrap();
		let pre = prepare_burn(val, &origin);
		assert_ok!(Scarcity::burn(origin));
		post_dispatch(pre, Ok(()));
		assert_ok!(Scarcity::do_mint_claimed(0, 0, 6));

		assert_ok!(Scarcity::do_try_state());
		#[cfg(feature = "try-runtime")]
		assert_ok!(<Scarcity as Hooks<u64>>::try_state(System::block_number()));
	});
}
