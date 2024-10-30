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

// Need to rerun!

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::Weight};
use core::marker::PhantomData;

/// Weight functions for `pallet_migrations`.
pub struct WeightInfo<T>(PhantomData<T>);
impl<T: frame_system::Config> pallet_migrations::WeightInfo for WeightInfo<T> {
	/// Storage: `MultiBlockMigrations::Cursor` (r:1 w:1)
	/// Proof: `MultiBlockMigrations::Cursor` (`max_values`: Some(1), `max_size`: Some(65550), added: 66045, mode: `MaxEncodedLen`)
	/// Storage: UNKNOWN KEY `0x583359fe0e84d953a9dd84e8addb08a5` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0x583359fe0e84d953a9dd84e8addb08a5` (r:1 w:0)
	fn onboard_new_mbms() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `276`
		//  Estimated: `67035`
		// Minimum execution time: 7_762_000 picoseconds.
		Weight::from_parts(8_100_000, 67035)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `MultiBlockMigrations::Cursor` (r:1 w:0)
	/// Proof: `MultiBlockMigrations::Cursor` (`max_values`: Some(1), `max_size`: Some(65550), added: 66045, mode: `MaxEncodedLen`)
	fn progress_mbms_none() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `142`
		//  Estimated: `67035`
		// Minimum execution time: 2_077_000 picoseconds.
		Weight::from_parts(2_138_000, 67035)
			.saturating_add(T::DbWeight::get().reads(1_u64))
	}
	/// Storage: UNKNOWN KEY `0x583359fe0e84d953a9dd84e8addb08a5` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0x583359fe0e84d953a9dd84e8addb08a5` (r:1 w:0)
	/// Storage: `MultiBlockMigrations::Cursor` (r:0 w:1)
	/// Proof: `MultiBlockMigrations::Cursor` (`max_values`: Some(1), `max_size`: Some(65550), added: 66045, mode: `MaxEncodedLen`)
	fn exec_migration_completed() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `134`
		//  Estimated: `3599`
		// Minimum execution time: 5_868_000 picoseconds.
		Weight::from_parts(6_143_000, 3599)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: UNKNOWN KEY `0x583359fe0e84d953a9dd84e8addb08a5` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0x583359fe0e84d953a9dd84e8addb08a5` (r:1 w:0)
	/// Storage: `MultiBlockMigrations::Historic` (r:1 w:0)
	/// Proof: `MultiBlockMigrations::Historic` (`max_values`: None, `max_size`: Some(266), added: 2741, mode: `MaxEncodedLen`)
	fn exec_migration_skipped_historic() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `330`
		//  Estimated: `3795`
		// Minimum execution time: 10_283_000 picoseconds.
		Weight::from_parts(10_964_000, 3795)
			.saturating_add(T::DbWeight::get().reads(2_u64))
	}
	/// Storage: UNKNOWN KEY `0x583359fe0e84d953a9dd84e8addb08a5` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0x583359fe0e84d953a9dd84e8addb08a5` (r:1 w:0)
	/// Storage: `MultiBlockMigrations::Historic` (r:1 w:0)
	/// Proof: `MultiBlockMigrations::Historic` (`max_values`: None, `max_size`: Some(266), added: 2741, mode: `MaxEncodedLen`)
	fn exec_migration_advance() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `276`
		//  Estimated: `3741`
		// Minimum execution time: 9_900_000 picoseconds.
		Weight::from_parts(10_396_000, 3741)
			.saturating_add(T::DbWeight::get().reads(2_u64))
	}
	/// Storage: UNKNOWN KEY `0x583359fe0e84d953a9dd84e8addb08a5` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0x583359fe0e84d953a9dd84e8addb08a5` (r:1 w:0)
	/// Storage: `MultiBlockMigrations::Historic` (r:1 w:1)
	/// Proof: `MultiBlockMigrations::Historic` (`max_values`: None, `max_size`: Some(266), added: 2741, mode: `MaxEncodedLen`)
	fn exec_migration_complete() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `276`
		//  Estimated: `3741`
		// Minimum execution time: 11_411_000 picoseconds.
		Weight::from_parts(11_956_000, 3741)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: UNKNOWN KEY `0x583359fe0e84d953a9dd84e8addb08a5` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0x583359fe0e84d953a9dd84e8addb08a5` (r:1 w:0)
	/// Storage: `MultiBlockMigrations::Historic` (r:1 w:0)
	/// Proof: `MultiBlockMigrations::Historic` (`max_values`: None, `max_size`: Some(266), added: 2741, mode: `MaxEncodedLen`)
	/// Storage: `MultiBlockMigrations::Cursor` (r:0 w:1)
	/// Proof: `MultiBlockMigrations::Cursor` (`max_values`: Some(1), `max_size`: Some(65550), added: 66045, mode: `MaxEncodedLen`)
	fn exec_migration_fail() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `276`
		//  Estimated: `3741`
		// Minimum execution time: 12_398_000 picoseconds.
		Weight::from_parts(12_910_000, 3741)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	fn on_init_loop() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 166_000 picoseconds.
		Weight::from_parts(193_000, 0)
	}
	/// Storage: `MultiBlockMigrations::Cursor` (r:0 w:1)
	/// Proof: `MultiBlockMigrations::Cursor` (`max_values`: Some(1), `max_size`: Some(65550), added: 66045, mode: `MaxEncodedLen`)
	fn force_set_cursor() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 2_686_000 picoseconds.
		Weight::from_parts(2_859_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `MultiBlockMigrations::Cursor` (r:0 w:1)
	/// Proof: `MultiBlockMigrations::Cursor` (`max_values`: Some(1), `max_size`: Some(65550), added: 66045, mode: `MaxEncodedLen`)
	fn force_set_active_cursor() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 3_070_000 picoseconds.
		Weight::from_parts(3_250_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `MultiBlockMigrations::Cursor` (r:1 w:0)
	/// Proof: `MultiBlockMigrations::Cursor` (`max_values`: Some(1), `max_size`: Some(65550), added: 66045, mode: `MaxEncodedLen`)
	/// Storage: UNKNOWN KEY `0x583359fe0e84d953a9dd84e8addb08a5` (r:1 w:0)
	/// Proof: UNKNOWN KEY `0x583359fe0e84d953a9dd84e8addb08a5` (r:1 w:0)
	fn force_onboard_mbms() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `251`
		//  Estimated: `67035`
		// Minimum execution time: 5_901_000 picoseconds.
		Weight::from_parts(6_320_000, 67035)
			.saturating_add(T::DbWeight::get().reads(2_u64))
	}
	/// Storage: `MultiBlockMigrations::Historic` (r:256 w:256)
	/// Proof: `MultiBlockMigrations::Historic` (`max_values`: None, `max_size`: Some(266), added: 2741, mode: `MaxEncodedLen`)
	/// The range of component `n` is `[0, 256]`.
	fn clear_historic(n: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `1122 + n * (271 ±0)`
		//  Estimated: `3834 + n * (2740 ±0)`
		// Minimum execution time: 15_952_000 picoseconds.
		Weight::from_parts(14_358_665, 3834)
			// Standard Error: 3_358
			.saturating_add(Weight::from_parts(1_323_674, 0).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().reads((1_u64).saturating_mul(n.into())))
			.saturating_add(T::DbWeight::get().writes((1_u64).saturating_mul(n.into())))
			.saturating_add(Weight::from_parts(0, 2740).saturating_mul(n.into()))
	}
}