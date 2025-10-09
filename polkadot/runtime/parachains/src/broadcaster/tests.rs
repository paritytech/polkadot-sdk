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
use frame_support::{assert_err, assert_ok};
use polkadot_primitives::Id as ParaId;

#[test]
fn publish_store_retrieve_and_update_data() {
	new_test_ext(Default::default()).execute_with(|| {
		let para_id = ParaId::from(1000);

		// Publisher doesn't exist
		assert!(!PublisherExists::<Test>::get(para_id));

		// Publish initial data
		let initial_data =
			vec![(b"key1".to_vec(), b"value1".to_vec()), (b"key2".to_vec(), b"value2".to_vec())];
		Broadcaster::handle_publish(para_id, initial_data.clone()).unwrap();

		// Verify publisher exists
		assert!(PublisherExists::<Test>::get(para_id));

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
		let para_id = ParaId::from(1000);

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
		let para1 = ParaId::from(1000);
		let para2 = ParaId::from(2000);
		let para3 = ParaId::from(3000);

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
fn get_all_published_data_map_returns_all_publishers() {
	new_test_ext(Default::default()).execute_with(|| {
		let para1 = ParaId::from(1000);
		let para2 = ParaId::from(2000);

		// Publish data from two parachains
		Broadcaster::handle_publish(para1, vec![(b"key1".to_vec(), b"value1".to_vec())]).unwrap();
		Broadcaster::handle_publish(para2, vec![(b"key2".to_vec(), b"value2".to_vec())]).unwrap();

		// Get all published data
		let all_data = Broadcaster::get_all_published_data_map();

		// Should include both publishers
		assert_eq!(all_data.len(), 2);
		assert!(all_data.contains_key(&para1));
		assert!(all_data.contains_key(&para2));

		// Verify data content
		assert_eq!(all_data.get(&para1).unwrap(), &vec![(b"key1".to_vec(), b"value1".to_vec())]);
		assert_eq!(all_data.get(&para2).unwrap(), &vec![(b"key2".to_vec(), b"value2".to_vec())]);
	});
}
