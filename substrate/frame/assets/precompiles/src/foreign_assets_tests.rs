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

//! Tests for foreign assets functionality.

use super::*;
use crate::{
	foreign_assets::{pallet::Pallet as ForeignAssetsPallet, ForeignAssetId},
	mock::{new_test_ext, Test},
};
use frame_support::assert_ok;
use pallet_assets::AssetsCallback;

#[test]
fn asset_mapping_insert_works() {
	new_test_ext().execute_with(|| {
		let asset_id = 123u32;

		// Insert mapping - now returns the allocated index
		let asset_index = ForeignAssetsPallet::<Test>::insert_asset_mapping(&asset_id).unwrap();

		// First asset should get index 0
		assert_eq!(asset_index, 0);

		// Verify both directions of lookup work
		assert_eq!(ForeignAssetsPallet::<Test>::asset_id_of(asset_index), Some(asset_id));
		assert_eq!(ForeignAssetsPallet::<Test>::asset_index_of(&asset_id), Some(asset_index));
	});
}

#[test]
fn asset_mapping_insert_sequential_indices() {
	new_test_ext().execute_with(|| {
		let asset_id1 = 100u32;
		let asset_id2 = 200u32;
		let asset_id3 = 300u32;

		// Insert mappings - should get sequential indices
		let index1 = ForeignAssetsPallet::<Test>::insert_asset_mapping(&asset_id1).unwrap();
		let index2 = ForeignAssetsPallet::<Test>::insert_asset_mapping(&asset_id2).unwrap();
		let index3 = ForeignAssetsPallet::<Test>::insert_asset_mapping(&asset_id3).unwrap();

		assert_eq!(index1, 0);
		assert_eq!(index2, 1);
		assert_eq!(index3, 2);

		// Verify lookups
		assert_eq!(ForeignAssetsPallet::<Test>::asset_id_of(0), Some(asset_id1));
		assert_eq!(ForeignAssetsPallet::<Test>::asset_id_of(1), Some(asset_id2));
		assert_eq!(ForeignAssetsPallet::<Test>::asset_id_of(2), Some(asset_id3));
	});
}

#[test]
fn asset_mapping_insert_prevents_duplicate_asset_id() {
	new_test_ext().execute_with(|| {
		let asset_id = 123u32;

		// Insert first mapping
		let index1 = ForeignAssetsPallet::<Test>::insert_asset_mapping(&asset_id).unwrap();

		// Try to insert same asset again - should fail
		assert!(ForeignAssetsPallet::<Test>::insert_asset_mapping(&asset_id).is_err());

		// Original mapping should still exist
		assert_eq!(ForeignAssetsPallet::<Test>::asset_index_of(&asset_id), Some(index1));
	});
}

#[test]
fn asset_mapping_remove_works() {
	new_test_ext().execute_with(|| {
		let asset_id = 123u32;

		// Insert and verify
		let asset_index = ForeignAssetsPallet::<Test>::insert_asset_mapping(&asset_id).unwrap();
		assert_eq!(ForeignAssetsPallet::<Test>::asset_id_of(asset_index), Some(asset_id));

		// Remove mapping
		ForeignAssetsPallet::<Test>::remove_asset_mapping(&asset_id);

		// Verify both directions are removed
		assert_eq!(ForeignAssetsPallet::<Test>::asset_id_of(asset_index), None);
		assert_eq!(ForeignAssetsPallet::<Test>::asset_index_of(&asset_id), None);
	});
}

#[test]
fn asset_mapping_remove_nonexistent_is_safe() {
	new_test_ext().execute_with(|| {
		let asset_id = 999u32;

		// Remove mapping that doesn't exist - should not panic
		ForeignAssetsPallet::<Test>::remove_asset_mapping(&asset_id);

		// Should still be None
		assert_eq!(ForeignAssetsPallet::<Test>::asset_index_of(&asset_id), None);
	});
}

#[test]
fn foreign_asset_callback_created_inserts_mapping() {
	new_test_ext().execute_with(|| {
		let asset_id = 42u32;
		let owner = 123u64;

		// Simulate asset creation callback
		assert_ok!(ForeignAssetId::<Test>::created(&asset_id, &owner));

		// Verify mapping was inserted with sequential index (first asset gets 0)
		let asset_index = ForeignAssetsPallet::<Test>::asset_index_of(&asset_id).unwrap();
		assert_eq!(asset_index, 0);
		assert_eq!(ForeignAssetsPallet::<Test>::asset_id_of(asset_index), Some(asset_id));
	});
}

#[test]
fn foreign_asset_callback_destroyed_removes_mapping() {
	new_test_ext().execute_with(|| {
		let asset_id = 42u32;
		let owner = 123u64;

		// Setup: create asset mapping via callback
		assert_ok!(ForeignAssetId::<Test>::created(&asset_id, &owner));
		let asset_index = ForeignAssetsPallet::<Test>::asset_index_of(&asset_id).unwrap();
		assert_eq!(ForeignAssetsPallet::<Test>::asset_id_of(asset_index), Some(asset_id));

		// Simulate asset destruction callback
		assert_ok!(ForeignAssetId::<Test>::destroyed(&asset_id));

		// Verify mapping was removed
		assert_eq!(ForeignAssetsPallet::<Test>::asset_id_of(asset_index), None);
		assert_eq!(ForeignAssetsPallet::<Test>::asset_index_of(&asset_id), None);
	});
}

#[test]
fn foreign_asset_id_extractor_works_with_valid_mapping() {
	new_test_ext().execute_with(|| {
		let asset_id = 555u32;

		// Setup mapping - gets index 0
		let asset_index = ForeignAssetsPallet::<Test>::insert_asset_mapping(&asset_id).unwrap();

		// Create address with the asset index in the first 4 bytes
		let mut address = [0u8; 20];
		address[0..4].copy_from_slice(&asset_index.to_be_bytes());

		// Test extraction
		let result = ForeignAssetIdExtractor::<Test>::asset_id_from_address(&address);
		assert_eq!(result.unwrap(), asset_id);
	});
}

#[test]
fn foreign_asset_id_extractor_fails_without_mapping() {
	new_test_ext().execute_with(|| {
		let asset_index = 0x0000_9999u32;

		// Create address without setting up mapping
		let mut address = [0u8; 20];
		address[0..4].copy_from_slice(&asset_index.to_be_bytes());

		// Test extraction should fail
		let result = ForeignAssetIdExtractor::<Test>::asset_id_from_address(&address);
		assert!(result.is_err());
	});
}

#[test]
fn foreign_id_config_matcher_works() {
	// Test that the prefix matcher works correctly
	const PREFIX: u16 = 0x0220;
	let matcher = ForeignIdConfig::<PREFIX, Test>::MATCHER;

	// Address with correct prefix should match
	let mut matching_address = [0u8; 20];
	matching_address[16..18].copy_from_slice(&PREFIX.to_be_bytes());
	assert!(matcher.matches(&matching_address));

	// Address with wrong prefix should not match
	let mut non_matching_address = [0u8; 20];
	non_matching_address[16..18].copy_from_slice(&0x0120u16.to_be_bytes());
	assert!(!matcher.matches(&non_matching_address));
}

#[test]
fn next_asset_index_increments_correctly() {
	new_test_ext().execute_with(|| {
		// Initial state
		assert_eq!(ForeignAssetsPallet::<Test>::next_asset_index(), 0);

		// Insert first asset
		let index1 = ForeignAssetsPallet::<Test>::insert_asset_mapping(&100u32).unwrap();
		assert_eq!(index1, 0);
		assert_eq!(ForeignAssetsPallet::<Test>::next_asset_index(), 1);

		// Insert second asset
		let index2 = ForeignAssetsPallet::<Test>::insert_asset_mapping(&200u32).unwrap();
		assert_eq!(index2, 1);
		assert_eq!(ForeignAssetsPallet::<Test>::next_asset_index(), 2);

		// Insert third asset
		let index3 = ForeignAssetsPallet::<Test>::insert_asset_mapping(&300u32).unwrap();
		assert_eq!(index3, 2);
		assert_eq!(ForeignAssetsPallet::<Test>::next_asset_index(), 3);
	});
}
