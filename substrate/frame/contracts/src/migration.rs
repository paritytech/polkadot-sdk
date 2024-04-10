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
pub mod v09;
pub mod v10;
pub mod v11;
pub mod v12;
pub mod v13;
pub mod v14;
pub mod v15;

use crate::{weights::WeightInfo, Config, Pallet, Weight};
use codec::FullCodec;
use frame_support::{
	migrations::{SteppedMigration, SteppedMigrationError},
	pallet_prelude::*,
	traits::ConstU32,
	weights::WeightMeter,
};
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use sp_std::prelude::*;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

/// The cursor used to encode the position (usually the last iterated key) of the current migration
/// step.
pub type Cursor = BoundedVec<u8, ConstU32<1024>>;

/// IsFinished describes whether a migration is finished or not.
pub enum IsFinished {
	Yes,
	No,
}

/// A trait that allows to migrate storage from one version to another.
///
/// The migration is done in steps. The migration is finished when
/// `step()` returns `IsFinished::Yes`.
pub trait ContractsMigrationStep: FullCodec + MaxEncodedLen + Default {
	/// Returns the version of the migration.
	const VERSION: u16;

	/// Returns the maximum weight that can be consumed in a single step.
	fn max_step_weight() -> Weight;

	/// Process one step of the migration.
	///
	/// Returns whether the migration is finished.
	fn step(&mut self, meter: &mut WeightMeter) -> IsFinished;

	/// Verify that the migration step fits into `Cursor`, and that `max_step_weight` is not greater
	/// than `max_block_weight`.
	fn integrity_test(max_block_weight: Weight) {
		if Self::max_step_weight().any_gt(max_block_weight) {
			panic!(
				"Invalid max_step_weight for Migration {}. Value should be lower than {}",
				Self::VERSION,
				max_block_weight
			);
		}

		let len = <Self as MaxEncodedLen>::max_encoded_len();
		let max = Cursor::bound();
		if len > max {
			panic!(
				"Migration {} has size {} which is bigger than the maximum of {}",
				Self::VERSION,
				len,
				max,
			);
		}
	}

	/// Execute some pre-checks prior to running the first step of this migration.
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade_step() -> Result<Vec<u8>, TryRuntimeError> {
		Ok(Vec::new())
	}

	/// Execute some post-checks after running the last step of this migration.
	#[cfg(feature = "try-runtime")]
	fn post_upgrade_step(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
		Ok(())
	}
}

/// The identifier used for the pallet mbm migrations.
pub const PALLET_MIGRATIONS_ID: &[u8; 20] = b"pallet-contracts-mbm";

/// A migration identifier used to identify pallet-contracts mbm migrations.
#[derive(MaxEncodedLen, Encode, Decode)]
pub struct MigrationId {
	pub pallet_id: [u8; 20],
	pub version_to: u16,
}

/// A wrapper around a migration step that allows to use it in the context of the `SteppedMigration`
pub struct SteppedMigrationAdapter<T: Config, S: ContractsMigrationStep>(PhantomData<(S, T)>);

impl<T: Config, V: ContractsMigrationStep> SteppedMigration for SteppedMigrationAdapter<T, V> {
	type Cursor = V;
	type Identifier = MigrationId;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_to: V::VERSION }
	}

	fn step(
		cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		let required = V::max_step_weight();

		let mut cursor = match cursor {
			None => {
				let required = required.saturating_add(T::WeightInfo::migration_version_bump());
				if !meter.can_consume(required) {
					return Err(SteppedMigrationError::InsufficientWeight { required })
				}
				StorageVersion::new(V::VERSION).put::<Pallet<T>>();
				Default::default()
			},
			Some(cursor) => {
				if !meter.can_consume(required) {
					return Err(SteppedMigrationError::InsufficientWeight { required })
				}
				cursor
			},
		};

		#[cfg(feature = "try-runtime")]
		{
			assert!(
				cursor.is_empty(),
				"try-runtime should run all ContractsMigrationStep in one go"
			);
			let data = V::pre_upgrade_step(&cursor).expect("pre_upgrade_step failed");
		}

		loop {
			if meter.try_consume(required).is_err() {
				break;
			}

			let result = V::step(&mut cursor, meter);

			if let IsFinished::Yes = result {
				#[cfg(feature = "try-runtime")]
				V::post_upgrade_step(data).expect("post_upgrade_step failed");
				return Ok(None)
			}
		}

		Ok(Some(cursor))
	}
}
