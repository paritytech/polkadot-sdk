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

//! Test helpers for internal and external usage.

#![cfg(any(feature = "std", test))]
#![allow(missing_docs)]

use codec::{Decode, Encode};
use core::cell::RefCell;
use frame_support::{
	migrations::*,
	weights::{Weight, WeightMeter},
};
use sp_core::ConstU32;
use sp_runtime::BoundedVec;

/// Opaque identifier of a migration.
pub type MockedIdentifier = BoundedVec<u8, ConstU32<256>>;

/// How a mocked migration should behave.
#[derive(Debug, Clone, Copy, Encode)]
pub enum MockedMigrationKind {
	/// Succeed after its number of steps elapsed.
	SucceedAfter,
	/// Fail after its number of steps elapsed.
	FailAfter,
	/// Never terminate.
	TimeoutAfter,
	/// Cause an [`SteppedMigrationError::InsufficientWeight`] error after its number of steps
	/// elapsed.
	HighWeightAfter(Weight),
}
use MockedMigrationKind::*; // C style

/// Creates a migration identifier with a specific `kind` and `steps`.
pub fn mocked_id(kind: MockedMigrationKind, steps: u32) -> MockedIdentifier {
	(b"MockedMigration", kind, steps).encode().try_into().unwrap()
}

thread_local! {
	/// The configs for the migrations to run.
	static MIGRATIONS: RefCell<Vec<(MockedMigrationKind, u32)>> = RefCell::new(vec![]);
}

/// Allows to set the migrations to run at runtime instead of compile-time.
///
/// It achieves this by using a thread-local storage to store the migrations to run.
pub struct MigrationsStorage;
impl SteppedMigrations for MigrationsStorage {
	fn len() -> u32 {
		MIGRATIONS.with(|m| m.borrow().len()) as u32
	}

	fn nth_id(n: u32) -> Option<Vec<u8>> {
		let k = MIGRATIONS.with(|m| m.borrow().get(n as usize).map(|k| *k));
		k.map(|(kind, steps)| mocked_id(kind, steps).into_inner())
	}

	fn nth_step(
		n: u32,
		cursor: Option<Vec<u8>>,
		_meter: &mut WeightMeter,
	) -> Option<Result<Option<Vec<u8>>, SteppedMigrationError>> {
		Some(MIGRATIONS.with(|m| {
			let (kind, steps) = m.borrow()[n as usize];

			let mut count: u32 =
				cursor.as_ref().and_then(|c| Decode::decode(&mut &c[..]).ok()).unwrap_or(0);
			log::debug!("MockedMigration: Step {}", count);
			if count != steps || matches!(kind, TimeoutAfter) {
				count += 1;
				return Ok(Some(count.encode().try_into().unwrap()))
			}

			match kind {
				SucceedAfter => {
					log::debug!("MockedMigration: Succeeded after {} steps", count);
					Ok(None)
				},
				HighWeightAfter(required) => {
					log::debug!("MockedMigration: Not enough weight after {} steps", count);
					Err(SteppedMigrationError::InsufficientWeight { required })
				},
				FailAfter => {
					log::debug!("MockedMigration: Failed after {} steps", count);
					Err(SteppedMigrationError::Failed)
				},
				TimeoutAfter => unreachable!(),
			}
		}))
	}

	fn nth_transactional_step(
		n: u32,
		cursor: Option<Vec<u8>>,
		meter: &mut WeightMeter,
	) -> Option<Result<Option<Vec<u8>>, SteppedMigrationError>> {
		// This is a hack but should be fine. We dont need it in testing.
		Self::nth_step(n, cursor, meter)
	}

	fn nth_max_steps(n: u32) -> Option<Option<u32>> {
		MIGRATIONS
			.with(|m| m.borrow().get(n as usize).map(|s| *s))
			.map(|(_, s)| Some(s))
	}

	fn cursor_max_encoded_len() -> usize {
		65_536
	}

	fn identifier_max_encoded_len() -> usize {
		256
	}
}

impl MigrationsStorage {
	/// Set the migrations to run.
	pub fn set(migrations: Vec<(MockedMigrationKind, u32)>) {
		MIGRATIONS.with(|m| *m.borrow_mut() = migrations);
	}
}

frame_support::parameter_types! {
	/// The number of started upgrades.
	pub static UpgradesStarted: u32 = 0;
	/// The number of completed upgrades.
	pub static UpgradesCompleted: u32 = 0;
	/// The migrations that failed.
	pub static UpgradesFailed: Vec<Option<u32>> = vec![];
	/// Return value of [`MockedFailedMigrationHandler::failed`].
	pub static FailedUpgradeResponse: FailedUpgradeHandling = FailedUpgradeHandling::KeepStuck;
}

/// Records all started and completed upgrades in `UpgradesStarted` and `UpgradesCompleted`.
pub struct MockedMigrationStatusHandler;
impl MigrationStatusHandler for MockedMigrationStatusHandler {
	fn started() {
		log::info!("MigrationStatusHandler started");
		UpgradesStarted::mutate(|v| *v += 1);
	}

	fn completed() {
		log::info!("MigrationStatusHandler completed");
		UpgradesCompleted::mutate(|v| *v += 1);
	}
}

/// Records all failed upgrades in `UpgradesFailed`.
pub struct MockedFailedMigrationHandler;
impl FailedMigrationHandler for MockedFailedMigrationHandler {
	fn failed(migration: Option<u32>) -> Option<FailedUpgradeHandling> {
		UpgradesFailed::mutate(|v| v.push(migration));
		let res = FailedUpgradeResponse::get();
		log::error!("FailedMigrationHandler failed at: {migration:?}, handling as {res:?}");
		Some(res)
	}
}

/// Returns the number of `(started, completed, failed)` upgrades and resets their numbers.
pub fn upgrades_started_completed_failed() -> (u32, u32, u32) {
	(UpgradesStarted::take(), UpgradesCompleted::take(), UpgradesFailed::take().len() as u32)
}
