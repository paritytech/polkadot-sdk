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

//! Mocked runtime for testing the migrations pallet.

#![cfg(test)]

use crate::{mock_helpers::*, Event, Historic};

use frame_support::{
	derive_impl,
	migrations::*,
	traits::{OnFinalize, OnInitialize},
	weights::Weight,
};
use frame_system::EventRecord;
use sp_core::{ConstU32, H256};

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
	type PalletInfo = PalletInfo;
	type MultiBlockMigrator = Migrations;
}

frame_support::parameter_types! {
	pub const ServiceWeight: Weight = Weight::MAX.div(10);
}

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Migrations = MigrationsStorage;
	type CursorMaxLen = ConstU32<65_536>;
	type IdentifierMaxLen = ConstU32<256>;
	type MigrationStatusHandler = MockedMigrationStatusHandler;
	type FailedMigrationHandler = MockedFailedMigrationHandler;
	type ServiceWeight = ServiceWeight;
	type WeightInfo = ();
}

/// Build genesis storage according to the mock runtime.
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

/// Returns the historic migrations, sorted by their identifier.
pub fn historic() -> Vec<MockedIdentifier> {
	let mut historic = Historic::<Test>::iter_keys().collect::<Vec<_>>();
	historic.sort();
	historic
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
