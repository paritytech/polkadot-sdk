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

#![cfg(test)]

use crate::{Event, Historic, WeightMeter};

use codec::{Decode, Encode};
use core::cell::RefCell;
use frame_support::{
	derive_impl,
	migrations::*,
	traits::{OnFinalize, OnInitialize},
	weights::Weight,
};
use frame_system::EventRecord;
use sp_core::{ConstU32, H256};
use sp_runtime::BoundedVec;

type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Migrations: crate,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type BaseCallFilter = frame_support::traits::Everything;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type PalletInfo = PalletInfo;
	type OnSetCode = ();
}

/// An opaque identifier of a migration.
pub type MockedIdentifier = BoundedVec<u8, ConstU32<256>>;

/// How a [`MockedMigration`] should behave.
#[derive(Debug, Clone, Copy, Encode)]
#[allow(dead_code)]
pub enum MockedMigrationKind {
	/// Succeed after its number of steps elapsed.
	SucceedAfter,
	/// Fail after its number of steps elapsed.
	FailAfter,
	/// Never terminate.
	TimeoutAfter,
	/// Cause an [`InsufficientWeight`] error after its number of steps elapsed.
	HighWeightAfter(Weight),
}
use MockedMigrationKind::*; // C style

impl From<u8> for MockedMigrationKind {
	fn from(v: u8) -> Self {
		match v {
			0 => SucceedAfter,
			1 => FailAfter,
			2 => TimeoutAfter,
			3 => HighWeightAfter(Weight::MAX),
			_ => unreachable!(),
		}
	}
}

impl Into<u8> for MockedMigrationKind {
	fn into(self) -> u8 {
		match self {
			SucceedAfter => 0,
			FailAfter => 1,
			TimeoutAfter => 2,
			HighWeightAfter(_) => 3,
		}
	}
}

/// Calculate the identifier of a mocked migration.
pub fn mocked_id(kind: MockedMigrationKind, steps: u32) -> MockedIdentifier {
	raw_mocked_id(kind.into(), steps)
}

/// FAIL-CI
pub fn raw_mocked_id(kind: u8, steps: u32) -> MockedIdentifier {
	(b"MockedMigration", kind, steps).encode().try_into().unwrap()
}

frame_support::parameter_types! {
	pub const ServiceWeight: Weight = Weight::MAX.div(10);
}

thread_local! {
	/// The configs for the migrations to run.
	pub static MIGRATIONS: RefCell<Vec<(MockedMigrationKind, u32)>> = RefCell::new(vec![]);
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
		// FAIL-CI
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

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Migrations = MigrationsStorage;
	type CursorMaxLen = ConstU32<65_536>;
	type IdentifierMaxLen = ConstU32<256>;
	type OnMigrationUpdate = MockedOnMigrationUpdate;
	type ServiceWeight = ServiceWeight;
	type WeightInfo = ();
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	sp_io::TestExternalities::new(Default::default())
}

/// Run this closure in test externalities.
pub fn test_closure<R>(f: impl FnOnce() -> R) -> R {
	let mut ext = new_test_ext();
	ext.execute_with(f)
}

pub fn run_to_block(n: u32) {
	while System::block_number() < n as u64 {
		if System::block_number() > 1 {
			Migrations::on_finalize(System::block_number());
			System::on_finalize(System::block_number());
		}
		log::debug!("Block {}", System::block_number());
		System::set_block_number(System::block_number() + 1);
		System::on_initialize(System::block_number());
		Migrations::on_initialize(System::block_number());
		// Executive calls this:
		<Migrations as MultiStepMigrator>::step();
	}
}

// Traits to make using events less insufferable:
pub trait IntoRecord {
	fn into_record(self) -> EventRecord<<Test as frame_system::Config>::RuntimeEvent, H256>;
}

impl IntoRecord for Event<Test> {
	fn into_record(self) -> EventRecord<<Test as frame_system::Config>::RuntimeEvent, H256> {
		let re: <Test as frame_system::Config>::RuntimeEvent = self.into();
		EventRecord { phase: frame_system::Phase::Initialization, event: re, topics: vec![] }
	}
}

pub trait IntoRecords {
	fn into_records(self) -> Vec<EventRecord<<Test as frame_system::Config>::RuntimeEvent, H256>>;
}

impl<E: IntoRecord> IntoRecords for Vec<E> {
	fn into_records(self) -> Vec<EventRecord<<Test as frame_system::Config>::RuntimeEvent, H256>> {
		self.into_iter().map(|e| e.into_record()).collect()
	}
}

pub fn assert_events<E: IntoRecord>(events: Vec<E>) {
	pretty_assertions::assert_eq!(events.into_records(), System::events());
	System::reset_events();
}

frame_support::parameter_types! {
	/// The number of started upgrades.
	pub static UpgradesStarted: u32 = 0;
	/// The number of completed upgrades.
	pub static UpgradesCompleted: u32 = 0;
	/// The migrations that failed.
	pub static UpgradesFailed: Vec<Option<u32>> = vec![];
	/// Return value of `MockedOnMigrationUpdate::failed`.
	pub static FailedUpgradeResponse: FailedUpgradeHandling = FailedUpgradeHandling::KeepStuck;
}

pub struct MockedOnMigrationUpdate;
impl OnMigrationUpdate for MockedOnMigrationUpdate {
	fn started() {
		log::info!("OnMigrationUpdate started");
		UpgradesStarted::mutate(|v| *v += 1);
	}

	fn completed() {
		log::info!("OnMigrationUpdate completed");
		UpgradesCompleted::mutate(|v| *v += 1);
	}

	fn failed(migration: Option<u32>) -> FailedUpgradeHandling {
		UpgradesFailed::mutate(|v| v.push(migration));
		let res = FailedUpgradeResponse::get();
		log::error!("OnMigrationUpdate failed at: {migration:?}, handling as {res:?}");
		res
	}
}

/// Returns the number of `(started, completed, failed)` upgrades and resets their numbers.
pub fn upgrades_started_completed_failed() -> (u32, u32, u32) {
	(UpgradesStarted::take(), UpgradesCompleted::take(), UpgradesFailed::take().len() as u32)
}

pub fn historic() -> Vec<MockedIdentifier> {
	let mut historic = Historic::<Test>::iter_keys().collect::<Vec<_>>();
	historic.sort();
	historic
}
