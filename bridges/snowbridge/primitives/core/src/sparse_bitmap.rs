// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

//! # Sparse Bitmap
//!
//! A module that provides an efficient way to track message nonces using a sparse bitmap.
//!
//! ## Overview
//!
//! The `SparseBitmap` uses a `StorageMap<u64, u128>` to store bit flags for a large range of
//! nonces. Each key (bucket) in the storage map contains a 128-bit value that can track 128
//! individual nonces.
//!
//! The implementation efficiently maps a u64 index (nonce) to:
//! 1. A bucket - calculated as `index >> 7` (dividing by 128)
//! 2. A bit position - calculated as `index & 127` (remainder when dividing by 128)
//!
//! ## Example
//!
//! For nonce 300:
//! - Bucket = 300 >> 7 = 2 (third bucket)
//! - Bit position = 300 & 127 = 44 (45th bit in the bucket)
//! - Corresponding bit mask = 1 << 44
//!
//! This approach allows tracking up to 2^64 nonces while only storing buckets that actually contain
//! data, making it suitable for sparse sets of nonces across a wide range.

use frame_support::storage::StorageMap;
use sp_std::marker::PhantomData;

/// Sparse bitmap interface.
pub trait SparseBitmap<BitMap>
where
	BitMap: StorageMap<u64, u128, Query = u128>,
{
	/// Get the bool at the provided index.
	fn get(index: u64) -> bool;
	/// Set the bool at the given index to true.
	fn set(index: u64);
}

/// Sparse bitmap implementation.
pub struct SparseBitmapImpl<BitMap>(PhantomData<BitMap>);

impl<BitMap> SparseBitmapImpl<BitMap>
where
	BitMap: StorageMap<u64, u128, Query = u128>,
{
	/// Computes the bucket index and the bit mask for a given bit index.
	/// Each bucket contains 128 bits.
	fn compute_bucket_and_mask(index: u64) -> (u64, u128) {
		(index >> 7, 1u128 << (index & 127))
	}
}

impl<BitMap> SparseBitmap<BitMap> for SparseBitmapImpl<BitMap>
where
	BitMap: StorageMap<u64, u128, Query = u128>,
{
	/// Checks if the bit at the specified index is set.
	/// Returns `true` if the bit is set, `false` otherwise.
	/// * `index`: The index (nonce) to check.
	fn get(index: u64) -> bool {
		// Calculate bucket and mask
		let (bucket, mask) = Self::compute_bucket_and_mask(index);

		// Retrieve bucket and check bit
		let bucket_value = BitMap::get(bucket);
		bucket_value & mask != 0
	}

	/// Sets the bit at the specified index.
	/// This marks the nonce as processed by setting its corresponding bit in the bitmap.
	/// * `index`: The index (nonce) to set.
	fn set(index: u64) {
		// Calculate bucket and mask
		let (bucket, mask) = Self::compute_bucket_and_mask(index);

		// Mutate the storage to set the bit
		BitMap::mutate(bucket, |value| {
			*value |= mask; // Set the bit in the bucket
		});
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::{
		storage::{generator::StorageMap as StorageMapHelper, storage_prefix},
		Twox64Concat,
	};
	use sp_io::TestExternalities;
	pub struct MockStorageMap;

	impl StorageMapHelper<u64, u128> for MockStorageMap {
		type Query = u128;
		type Hasher = Twox64Concat;
		fn pallet_prefix() -> &'static [u8] {
			b"MyModule"
		}

		fn storage_prefix() -> &'static [u8] {
			b"MyStorageMap"
		}

		fn prefix_hash() -> [u8; 32] {
			storage_prefix(Self::pallet_prefix(), Self::storage_prefix())
		}

		fn from_optional_value_to_query(v: Option<u128>) -> Self::Query {
			v.unwrap_or_default()
		}

		fn from_query_to_optional_value(v: Self::Query) -> Option<u128> {
			Some(v)
		}
	}

	type TestSparseBitmap = SparseBitmapImpl<MockStorageMap>;

	#[test]
	fn test_sparse_bitmap_set_and_get() {
		TestExternalities::default().execute_with(|| {
			let index = 300u64;
			let (bucket, mask) = TestSparseBitmap::compute_bucket_and_mask(index);

			// Test initial state
			assert_eq!(MockStorageMap::get(bucket), 0);
			assert!(!TestSparseBitmap::get(index));

			// Set the bit
			TestSparseBitmap::set(index);

			// Test after setting
			assert_eq!(MockStorageMap::get(bucket), mask);
			assert!(TestSparseBitmap::get(index));
		});
	}

	#[test]
	fn test_sparse_bitmap_multiple_sets() {
		TestExternalities::default().execute_with(|| {
			let index1 = 300u64;
			let index2 = 305u64; // Same bucket, different bit
			let (bucket, _) = TestSparseBitmap::compute_bucket_and_mask(index1);

			let (_, mask1) = TestSparseBitmap::compute_bucket_and_mask(index1);
			let (_, mask2) = TestSparseBitmap::compute_bucket_and_mask(index2);

			// Test initial state
			assert_eq!(MockStorageMap::get(bucket), 0);
			assert!(!TestSparseBitmap::get(index1));
			assert!(!TestSparseBitmap::get(index2));

			// Set the first bit
			TestSparseBitmap::set(index1);

			// Test after first set
			assert_eq!(MockStorageMap::get(bucket), mask1);
			assert!(TestSparseBitmap::get(index1));
			assert!(!TestSparseBitmap::get(index2));

			// Set the second bit
			TestSparseBitmap::set(index2);

			// Test after second set
			assert_eq!(MockStorageMap::get(bucket), mask1 | mask2); // Bucket should contain both masks
			assert!(TestSparseBitmap::get(index1));
			assert!(TestSparseBitmap::get(index2));
		})
	}

	#[test]
	fn test_sparse_bitmap_different_buckets() {
		TestExternalities::default().execute_with(|| {
			let index1 = 300u64; // Bucket 1
			let index2 = 300u64 + (1 << 7); // Bucket 2 (128 bits apart)

			let (bucket1, _) = TestSparseBitmap::compute_bucket_and_mask(index1);
			let (bucket2, _) = TestSparseBitmap::compute_bucket_and_mask(index2);

			let (_, mask1) = TestSparseBitmap::compute_bucket_and_mask(index1);
			let (_, mask2) = TestSparseBitmap::compute_bucket_and_mask(index2);

			// Test initial state
			assert_eq!(MockStorageMap::get(bucket1), 0);
			assert_eq!(MockStorageMap::get(bucket2), 0);

			// Set bits in different buckets
			TestSparseBitmap::set(index1);
			TestSparseBitmap::set(index2);

			// Test after setting
			assert_eq!(MockStorageMap::get(bucket1), mask1); // Bucket 1 should contain mask1
			assert_eq!(MockStorageMap::get(bucket2), mask2); // Bucket 2 should contain mask2

			assert!(TestSparseBitmap::get(index1));
			assert!(TestSparseBitmap::get(index2));
		})
	}

	#[test]
	fn test_sparse_bitmap_wide_range() {
		TestExternalities::default().execute_with(|| {
			// Test wide range of values across u64 spectrum
			let test_indices = [
				0u64,             // Smallest possible value
				1u64,             // Early value
				127u64,           // Last value in first bucket
				128u64,           // First value in second bucket
				255u64,           // End of second bucket
				1000u64,          // Medium-small value
				123456u64,        // Medium value
				(1u64 << 32) - 1, // Max u32 value
				1u64 << 32,       // First value after max u32
				(1u64 << 32) + 1, // Just after u32 max
				(1u64 << 40) - 1, // Large value near a power of 2
				(1u64 << 40),     // Power of 2 value
				(1u64 << 40) + 1, // Just after power of 2
				u64::MAX / 2,     // Middle of u64 range
				u64::MAX - 128,   // Near the end
				u64::MAX - 1,     // Second-to-last possible value
				u64::MAX,         // Largest possible value
			];

			// Verify each bit can be set and read correctly
			for &index in &test_indices {
				// Verify initial state - bit should be unset
				assert!(!TestSparseBitmap::get(index), "Index {} should initially be unset", index);

				// Set the bit
				TestSparseBitmap::set(index);

				// Verify bit was set
				assert!(
					TestSparseBitmap::get(index),
					"Index {} should be set after setting",
					index
				);

				// Calculate bucket and mask for verification
				let (bucket, mask) = TestSparseBitmap::compute_bucket_and_mask(index);

				// Verify the storage contains the bit
				let value = MockStorageMap::get(bucket);
				assert!(value & mask != 0, "Storage for index {} should have bit set", index);
			}

			// Verify all set bits can still be read correctly
			for &index in &test_indices {
				assert!(TestSparseBitmap::get(index), "Index {} should still be set", index);
			}
		})
	}

	#[test]
	fn test_sparse_bitmap_bucket_boundaries() {
		TestExternalities::default().execute_with(|| {
			// Test adjacent indices on bucket boundaries
			let boundary_pairs = [
				(127u64, 128u64),   // End of bucket 0, start of bucket 1
				(255u64, 256u64),   // End of bucket 1, start of bucket 2
				(1023u64, 1024u64), // End of bucket 7, start of bucket 8
			];

			for (i1, i2) in boundary_pairs {
				// Calculate buckets - should be different
				let (b1, m1) = TestSparseBitmap::compute_bucket_and_mask(i1);
				let (b2, m2) = TestSparseBitmap::compute_bucket_and_mask(i2);

				// Ensure they're in different buckets
				assert_ne!(b1, b2, "Indices {} and {} should be in different buckets", i1, i2);

				// Set both bits
				TestSparseBitmap::set(i1);
				TestSparseBitmap::set(i2);

				// Verify both are set
				assert!(TestSparseBitmap::get(i1), "Boundary index {} should be set", i1);
				assert!(TestSparseBitmap::get(i2), "Boundary index {} should be set", i2);

				// Verify storage contains correct masks
				let stored_b1_value = MockStorageMap::get(b1);
				let stored_b2_value = MockStorageMap::get(b2);

				// Just verify the bits are set in the masks (not checking exact mask values)
				assert_ne!(stored_b1_value, 0, "Storage for bucket {} should not be 0", b1);
				assert_ne!(stored_b2_value, 0, "Storage for bucket {} should not be 0", b2);
				assert!(
					stored_b1_value & m1 != 0,
					"Bit for index {} should be set in bucket {}",
					i1,
					b1
				);
				assert!(
					stored_b2_value & m2 != 0,
					"Bit for index {} should be set in bucket {}",
					i2,
					b2
				);
			}
		})
	}

	#[test]
	fn test_sparse_bitmap_large_buckets() {
		TestExternalities::default().execute_with(|| {
			// Test indices that produce large bucket numbers (near u64::MAX)
			let large_indices = [u64::MAX - 1, u64::MAX];

			for &index in &large_indices {
				let (bucket, mask) = TestSparseBitmap::compute_bucket_and_mask(index);

				// Verify bucket calculation is as expected
				assert_eq!(
					bucket,
					u64::from(index) >> 7,
					"Bucket calculation incorrect for {}",
					index
				);

				// Set and verify the bit
				TestSparseBitmap::set(index);
				assert!(TestSparseBitmap::get(index), "Large index {} should be set", index);

				// Verify the bit is set in storage
				let stored_value = MockStorageMap::get(bucket);
				assert_ne!(stored_value, 0, "Storage for bucket {} should not be 0", bucket);
				assert!(
					stored_value & mask != 0,
					"Bit for index {} should be set in bucket {}",
					index,
					bucket
				);
			}
		})
	}
}
