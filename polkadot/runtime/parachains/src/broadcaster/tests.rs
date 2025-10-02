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
use crate::mock::{new_test_ext, Broadcaster, Test};
use frame_support::{assert_ok, assert_err};
use polkadot_primitives::Id as ParaId;

#[test]
fn publish_store_retrieve_and_update_data() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(1000);

		// Initial state: publisher doesn't exist, no child root
		assert!(!PublisherExists::<Test>::get(para_id));
		assert!(Broadcaster::get_publisher_child_root(para_id).is_none());

		// Publish initial data
		let initial_data = vec![
			(b"key1".to_vec(), b"value1".to_vec()),
			(b"key2".to_vec(), b"value2".to_vec()),
		];
		Broadcaster::handle_publish(para_id, initial_data.clone()).unwrap();

		// Verify publisher exists and has a child root
		assert!(PublisherExists::<Test>::get(para_id));
		let root_after_initial = Broadcaster::get_publisher_child_root(para_id);
		assert!(root_after_initial.is_some());
		assert!(!root_after_initial.as_ref().unwrap().is_empty());

		// Verify the actual stored data matches what was published
		assert_eq!(
			Broadcaster::get_published_value(para_id, b"key1"),
			Some(b"value1".to_vec())
		);
		assert_eq!(
			Broadcaster::get_published_value(para_id, b"key2"),
			Some(b"value2".to_vec())
		);

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
			Some(b"value2".to_vec())  // Should remain unchanged
		);
		assert_eq!(
			Broadcaster::get_published_value(para_id, b"key3"),
			Some(b"value3".to_vec())
		);
	});
}

#[test]
fn empty_publish_still_creates_publisher() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(1000);
		
		let _ = Broadcaster::handle_publish(para_id, vec![]);
		
		assert!(PublisherExists::<Test>::get(para_id));
	});
}

#[test]
fn handle_publish_respects_max_items_limit() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(1000);

		let mut data = Vec::new();
		for i in 0..17 {
			data.push((format!("key{}", i).into_bytes(), b"value".to_vec()));
		}

		let result = Broadcaster::handle_publish(para_id, data);
		assert!(result.is_err());
		assert!(!PublisherExists::<Test>::get(para_id));
	});
}

#[test]
fn handle_publish_respects_key_length_limit() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(1000);

		let long_key = vec![b'a'; 257];
		let data = vec![(long_key, b"value".to_vec())];

		let result = Broadcaster::handle_publish(para_id, data);
		assert!(result.is_err());
		assert!(!PublisherExists::<Test>::get(para_id));
	});
}

#[test]
fn handle_publish_respects_value_length_limit() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(1000);

		let long_value = vec![b'v'; 1025];
		let data = vec![(b"key".to_vec(), long_value)];

		let result = Broadcaster::handle_publish(para_id, data);
		assert!(result.is_err());
		assert!(!PublisherExists::<Test>::get(para_id));
	});
}

#[test]
fn get_storage_key() {
	new_test_ext(Default::default()).execute_with(|| {
		let key = PublishedDataRoots::<Test>::hashed_key();
		println!("PublishedDataRoots storage key (bytes): {:?}", key);

		// Print as hex manually
		print!("PublishedDataRoots storage key (hex): ");
		for byte in &key {
			print!("{:02x}", byte);
		}
		println!();
	});
}

#[test]
fn subscribe_toggle_works() {
	new_test_ext(Default::default()).execute_with(|| {
		let subscriber = ParaId::from(1000);
		let publisher = ParaId::from(2000);

		// Initially not subscribed
		assert!(!Broadcaster::is_subscribed(subscriber, publisher));
		assert_eq!(Broadcaster::get_subscriptions(subscriber), vec![]);

		// First toggle: subscribe
		assert_ok!(Broadcaster::handle_subscribe_toggle(subscriber, publisher));
		assert!(Broadcaster::is_subscribed(subscriber, publisher));
		assert_eq!(Broadcaster::get_subscriptions(subscriber), vec![publisher]);

		// Second toggle: unsubscribe
		assert_ok!(Broadcaster::handle_subscribe_toggle(subscriber, publisher));
		assert!(!Broadcaster::is_subscribed(subscriber, publisher));
		assert_eq!(Broadcaster::get_subscriptions(subscriber), vec![]);
	});
}

#[test]
fn multiple_subscriptions_work() {
	new_test_ext(Default::default()).execute_with(|| {
		let subscriber = ParaId::from(1000);
		let publisher1 = ParaId::from(2000);
		let publisher2 = ParaId::from(3000);
		let publisher3 = ParaId::from(4000);

		// Subscribe to multiple publishers
		assert_ok!(Broadcaster::handle_subscribe_toggle(subscriber, publisher1));
		assert_ok!(Broadcaster::handle_subscribe_toggle(subscriber, publisher2));
		assert_ok!(Broadcaster::handle_subscribe_toggle(subscriber, publisher3));

		let subscriptions = Broadcaster::get_subscriptions(subscriber);
		assert_eq!(subscriptions.len(), 3);
		assert!(subscriptions.contains(&publisher1));
		assert!(subscriptions.contains(&publisher2));
		assert!(subscriptions.contains(&publisher3));

		// Unsubscribe from middle one
		assert_ok!(Broadcaster::handle_subscribe_toggle(subscriber, publisher2));

		let subscriptions = Broadcaster::get_subscriptions(subscriber);
		assert_eq!(subscriptions.len(), 2);
		assert!(subscriptions.contains(&publisher1));
		assert!(!subscriptions.contains(&publisher2));
		assert!(subscriptions.contains(&publisher3));
	});
}

#[test]
fn max_subscriptions_limit_enforced() {
	new_test_ext(Default::default()).execute_with(|| {
		let subscriber = ParaId::from(1000);

		// Subscribe up to MaxSubscriptions (10 in mock)
		for i in 0..10 {
			let publisher = ParaId::from(2000 + i);
			assert_ok!(Broadcaster::handle_subscribe_toggle(subscriber, publisher));
		}

		// Try to add one more subscription - should fail
		let publisher = ParaId::from(3000);
		assert_err!(
			Broadcaster::handle_subscribe_toggle(subscriber, publisher),
			Error::<Test>::TooManySubscriptions
		);

		// But can still unsubscribe and resubscribe
		let existing_publisher = ParaId::from(2000);
		assert_ok!(Broadcaster::handle_subscribe_toggle(subscriber, existing_publisher)); // Unsubscribe
		assert_ok!(Broadcaster::handle_subscribe_toggle(subscriber, publisher)); // Subscribe to new one
	});
}

#[test]
fn max_stored_keys_limit_enforced() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(1000);

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

		let result = Broadcaster::handle_publish(
			para_id,
			vec![(b"new_key".to_vec(), b"value".to_vec())]
		);
		assert_err!(result, Error::<Test>::TooManyStoredKeys);

		let result = Broadcaster::handle_publish(
			para_id,
			vec![(b"key0".to_vec(), b"updated_value".to_vec())]
		);
		assert_ok!(result);

		assert_eq!(
			Broadcaster::get_published_value(para_id, b"key0"),
			Some(b"updated_value".to_vec())
		);
	});
}