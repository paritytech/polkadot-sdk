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
use crate::mock::{new_test_ext, Balances, Broadcaster, RuntimeOrigin, Test};
use frame_support::{
	assert_err, assert_ok,
	traits::fungible::{hold::Inspect as HoldInspect, Inspect},
};
use polkadot_primitives::Id as ParaId;

const ALICE: u64 = 1;
const BOB: u64 = 2;

// Helper function to set up an account with balance
fn setup_account(who: u64, balance: u128) {
	let _ = Balances::mint_into(&who, balance);
}

// Helper function to register a publisher for testing
fn register_test_publisher(para_id: ParaId) {
	setup_account(ALICE, 10000);
	assert_ok!(Broadcaster::register_publisher(RuntimeOrigin::signed(ALICE), para_id));
}

// ========== Registration Tests ==========

#[test]
fn register_publisher_works() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(2000);
		setup_account(ALICE, 1000);

		// Register publisher
		assert_ok!(Broadcaster::register_publisher(RuntimeOrigin::signed(ALICE), para_id));

		// Verify registration
		let info = RegisteredPublishers::<Test>::get(para_id).unwrap();
		assert_eq!(info.manager, ALICE);
		assert_eq!(info.deposit, 100);

		// Verify deposit is held
		assert_eq!(Balances::balance_on_hold(&HoldReason::PublisherDeposit.into(), &ALICE), 100);
		assert_eq!(Balances::balance(&ALICE), 900);
	});
}

#[test]
fn force_register_system_chain_works() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(1000); // System chain
		setup_account(ALICE, 1000);

		// Force register with zero deposit
		assert_ok!(Broadcaster::force_register_publisher(
			RuntimeOrigin::root(),
			ALICE,
			0,
			para_id
		));

		// Verify registration
		let info = RegisteredPublishers::<Test>::get(para_id).unwrap();
		assert_eq!(info.manager, ALICE);
		assert_eq!(info.deposit, 0);

		// Verify no deposit held
		assert_eq!(Balances::balance_on_hold(&HoldReason::PublisherDeposit.into(), &ALICE), 0);
		assert_eq!(Balances::balance(&ALICE), 1000);
	});
}

#[test]
fn force_register_with_custom_deposit_works() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(2000);
		setup_account(BOB, 1000);

		// Force register with custom deposit
		assert_ok!(Broadcaster::force_register_publisher(
			RuntimeOrigin::root(),
			BOB,
			500,
			para_id
		));

		// Verify registration
		let info = RegisteredPublishers::<Test>::get(para_id).unwrap();
		assert_eq!(info.manager, BOB);
		assert_eq!(info.deposit, 500);

		// Verify custom deposit held
		assert_eq!(Balances::balance_on_hold(&HoldReason::PublisherDeposit.into(), &BOB), 500);
		assert_eq!(Balances::balance(&BOB), 500);
	});
}

#[test]
fn cannot_register_twice() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(2000);
		setup_account(ALICE, 1000);
		setup_account(BOB, 1000);

		// First registration succeeds
		assert_ok!(Broadcaster::register_publisher(RuntimeOrigin::signed(ALICE), para_id));

		// Second registration by same account fails
		assert_err!(
			Broadcaster::register_publisher(RuntimeOrigin::signed(ALICE), para_id),
			Error::<Test>::AlreadyRegistered
		);

		// Second registration by different account also fails
		assert_err!(
			Broadcaster::register_publisher(RuntimeOrigin::signed(BOB), para_id),
			Error::<Test>::AlreadyRegistered
		);

		// Only one deposit held
		assert_eq!(Balances::balance_on_hold(&HoldReason::PublisherDeposit.into(), &ALICE), 100);
		assert_eq!(Balances::balance_on_hold(&HoldReason::PublisherDeposit.into(), &BOB), 0);
	});
}

#[test]
fn force_register_requires_root() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(1000);
		setup_account(ALICE, 1000);

		// Non-root origin fails
		assert_err!(
			Broadcaster::force_register_publisher(RuntimeOrigin::signed(ALICE), ALICE, 0, para_id),
			sp_runtime::DispatchError::BadOrigin
		);

		// Para not registered
		assert!(!RegisteredPublishers::<Test>::contains_key(para_id));
	});
}

#[test]
fn register_publisher_requires_sufficient_balance() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(2000);
		setup_account(ALICE, 50); // Less than required deposit

		// Registration fails due to insufficient funds
		let result = Broadcaster::register_publisher(RuntimeOrigin::signed(ALICE), para_id);
		assert!(result.is_err());

		// Para not registered
		assert!(!RegisteredPublishers::<Test>::contains_key(para_id));
	});
}

#[test]
fn publish_requires_registration() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(2000);
		let data = vec![(b"key".to_vec(), b"value".to_vec())];

		// Publish without registration fails
		assert_err!(
			Broadcaster::handle_publish(para_id, data),
			Error::<Test>::PublishNotAuthorized
		);

		// No data stored
		assert!(!PublisherExists::<Test>::get(para_id));
	});
}

#[test]
fn registered_publisher_can_publish() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(2000);
		setup_account(ALICE, 1000);

		// Register first
		assert_ok!(Broadcaster::register_publisher(RuntimeOrigin::signed(ALICE), para_id));

		// Now publish works
		let data = vec![(b"key".to_vec(), b"value".to_vec())];
		assert_ok!(Broadcaster::handle_publish(para_id, data));

		// Verify data stored
		assert_eq!(Broadcaster::get_published_value(para_id, b"key"), Some(b"value".to_vec()));
	});
}

// ========== Existing Tests (updated to register first) ==========

#[test]
fn publish_store_retrieve_and_update_data() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(2000);
		setup_account(ALICE, 1000);

		// Register publisher first
		assert_ok!(Broadcaster::register_publisher(RuntimeOrigin::signed(ALICE), para_id));

		// Initial state: publisher doesn't exist, no child root
		assert!(!PublisherExists::<Test>::get(para_id));
		assert!(Broadcaster::get_publisher_child_root(para_id).is_none());

		// Publish initial data
		let initial_data =
			vec![(b"key1".to_vec(), b"value1".to_vec()), (b"key2".to_vec(), b"value2".to_vec())];
		Broadcaster::handle_publish(para_id, initial_data.clone()).unwrap();

		// Verify publisher exists and has a child root
		assert!(PublisherExists::<Test>::get(para_id));
		let root_after_initial = Broadcaster::get_publisher_child_root(para_id);
		assert!(root_after_initial.is_some());
		assert!(!root_after_initial.as_ref().unwrap().is_empty());

		// Verify the actual stored data matches what was published
		assert_eq!(Broadcaster::get_published_value(para_id, b"key1"), Some(b"value1".to_vec()));
		assert_eq!(Broadcaster::get_published_value(para_id, b"key2"), Some(b"value2".to_vec()));

		// Non-existent key should return None
		assert_eq!(Broadcaster::get_published_value(para_id, b"key3"), None);

		// Update existing key and add new key
		let update_data = vec![
			(b"key1".to_vec(), b"updated_value1".to_vec()),
			(b"key3".to_vec(), b"value3".to_vec()),
		];
		Broadcaster::handle_publish(para_id, update_data).unwrap();

		// Verify child root changed after update
		let root_after_update = Broadcaster::get_publisher_child_root(para_id);
		assert!(root_after_update.is_some());
		assert_ne!(root_after_initial.unwrap(), root_after_update.unwrap());

		// Verify the data was correctly updated
		assert_eq!(
			Broadcaster::get_published_value(para_id, b"key1"),
			Some(b"updated_value1".to_vec())
		);
		assert_eq!(
			Broadcaster::get_published_value(para_id, b"key2"),
			Some(b"value2".to_vec()) // Should remain unchanged
		);
		assert_eq!(Broadcaster::get_published_value(para_id, b"key3"), Some(b"value3".to_vec()));
	});
}

#[test]
fn empty_publish_still_creates_publisher() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(2000);
		register_test_publisher(para_id);

		let _ = Broadcaster::handle_publish(para_id, vec![]);

		assert!(PublisherExists::<Test>::get(para_id));
	});
}

#[test]
fn handle_publish_respects_max_items_limit() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(2000);
		register_test_publisher(para_id);

		let mut data = Vec::new();
		for i in 0..17 {
			data.push((format!("key{}", i).into_bytes(), b"value".to_vec()));
		}

		let result = Broadcaster::handle_publish(para_id, data);
		assert!(result.is_err());
	});
}

#[test]
fn handle_publish_respects_key_length_limit() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(2000);
		register_test_publisher(para_id);

		let long_key = vec![b'a'; 257];
		let data = vec![(long_key, b"value".to_vec())];

		let result = Broadcaster::handle_publish(para_id, data);
		assert!(result.is_err());
	});
}

#[test]
fn handle_publish_respects_value_length_limit() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(2000);
		register_test_publisher(para_id);

		let long_value = vec![b'v'; 1025];
		let data = vec![(b"key".to_vec(), long_value)];

		let result = Broadcaster::handle_publish(para_id, data);
		assert!(result.is_err());
	});
}

#[test]
fn max_stored_keys_limit_enforced() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(2000);
		register_test_publisher(para_id);

		for batch in 0..7 {
			let mut data = Vec::new();
			for i in 0..16 {
				let key_num = batch * 16 + i;
				if key_num < 100 {
					data.push((format!("key{}", key_num).into_bytes(), b"value".to_vec()));
				}
			}
			if !data.is_empty() {
				assert_ok!(Broadcaster::handle_publish(para_id, data));
			}
		}

		let published_keys = PublishedKeys::<Test>::get(para_id);
		assert_eq!(published_keys.len(), 100);

		let result =
			Broadcaster::handle_publish(para_id, vec![(b"new_key".to_vec(), b"value".to_vec())]);
		assert_err!(result, Error::<Test>::TooManyStoredKeys);

		let result = Broadcaster::handle_publish(
			para_id,
			vec![(b"key0".to_vec(), b"updated_value".to_vec())],
		);
		assert_ok!(result);

		assert_eq!(
			Broadcaster::get_published_value(para_id, b"key0"),
			Some(b"updated_value".to_vec())
		);
	});
}

#[test]
fn published_keys_storage_matches_child_trie() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(2000);
		register_test_publisher(para_id);

		// Publish multiple batches to ensure consistency maintained across updates
		let data1 = vec![
			(b"key1".to_vec(), b"value1".to_vec()),
			(b"key2".to_vec(), b"value2".to_vec()),
		];
		Broadcaster::handle_publish(para_id, data1).unwrap();

		// Update some keys, add new ones
		let data2 = vec![
			(b"key1".to_vec(), b"updated_value1".to_vec()),
			(b"key3".to_vec(), b"value3".to_vec()),
		];
		Broadcaster::handle_publish(para_id, data2).unwrap();

		let tracked_keys = PublishedKeys::<Test>::get(para_id);
		let actual_data = Broadcaster::get_all_published_data(para_id);

		// Counts must match
		assert_eq!(tracked_keys.len(), actual_data.len());

		// Every tracked key must exist in child trie
		for tracked_key in tracked_keys.iter() {
			let key: Vec<u8> = tracked_key.clone().into();
			assert!(actual_data.iter().any(|(k, _)| k == &key));
		}

		// Every child trie key must be tracked
		for (actual_key, _) in actual_data.iter() {
			assert!(tracked_keys.iter().any(|tracked| {
				let k: Vec<u8> = tracked.clone().into();
				&k == actual_key
			}));
		}
	});
}

#[test]
fn multiple_publishers_in_same_block() {
	new_test_ext(Default::default()).execute_with(|| {
		let para1 = ParaId::from(2000);
		let para2 = ParaId::from(2001);
		let para3 = ParaId::from(2002);

		// Register all publishers
		register_test_publisher(para1);
		setup_account(BOB, 10000);
		assert_ok!(Broadcaster::register_publisher(RuntimeOrigin::signed(BOB), para2));
		setup_account(3, 10000);
		assert_ok!(Broadcaster::register_publisher(RuntimeOrigin::signed(3), para3));

		// Multiple parachains publish data in the same block
		let data1 = vec![(b"key1".to_vec(), b"value1".to_vec())];
		let data2 = vec![(b"key2".to_vec(), b"value2".to_vec())];
		let data3 = vec![(b"key3".to_vec(), b"value3".to_vec())];

		Broadcaster::handle_publish(para1, data1).unwrap();
		Broadcaster::handle_publish(para2, data2).unwrap();
		Broadcaster::handle_publish(para3, data3).unwrap();

		// Verify all three publishers exist
		assert!(PublisherExists::<Test>::get(para1));
		assert!(PublisherExists::<Test>::get(para2));
		assert!(PublisherExists::<Test>::get(para3));

		// Verify PublishedDataRoots contains all three
		assert_eq!(PublishedDataRoots::<Test>::count(), 3);

		// Verify each para has its root in the map
		assert!(PublishedDataRoots::<Test>::contains_key(para1));
		assert!(PublishedDataRoots::<Test>::contains_key(para2));
		assert!(PublishedDataRoots::<Test>::contains_key(para3));

		// Verify each para's data is independently accessible
		assert_eq!(Broadcaster::get_published_value(para1, b"key1"), Some(b"value1".to_vec()));
		assert_eq!(Broadcaster::get_published_value(para2, b"key2"), Some(b"value2".to_vec()));
		assert_eq!(Broadcaster::get_published_value(para3, b"key3"), Some(b"value3".to_vec()));

		// Verify no cross-contamination
		assert_eq!(Broadcaster::get_published_value(para1, b"key2"), None);
		assert_eq!(Broadcaster::get_published_value(para2, b"key3"), None);
		assert_eq!(Broadcaster::get_published_value(para3, b"key1"), None);
	});
}

#[test]
fn max_publishers_limit_enforced() {
	new_test_ext(Default::default()).execute_with(|| {
		// Register and publish for max publishers
		for i in 0..1000 {
			let para_id = ParaId::from(2000 + i);
			setup_account(100 + i as u64, 10000);
			assert_ok!(Broadcaster::register_publisher(
				RuntimeOrigin::signed(100 + i as u64),
				para_id
			));
			let data = vec![(b"key".to_vec(), b"value".to_vec())];
			assert_ok!(Broadcaster::handle_publish(para_id, data));
		}

		assert_eq!(PublishedDataRoots::<Test>::count(), 1000);

		// Cannot register new publisher when limit reached
		let new_para = ParaId::from(3000);
		setup_account(ALICE, 10000);
		let data = vec![(b"key".to_vec(), b"value".to_vec())];

		// Registration should fail due to max publishers
		// (registration checks this in get_or_create_publisher_child_info)
		assert_ok!(Broadcaster::register_publisher(RuntimeOrigin::signed(ALICE), new_para));
		assert_err!(Broadcaster::handle_publish(new_para, data), Error::<Test>::TooManyPublishers);

		// Existing publisher can still update
		let existing_para = ParaId::from(2000);
		let update_data = vec![(b"key".to_vec(), b"updated".to_vec())];
		assert_ok!(Broadcaster::handle_publish(existing_para, update_data));
		assert_eq!(
			Broadcaster::get_published_value(existing_para, b"key"),
			Some(b"updated".to_vec())
		);
	});
}
