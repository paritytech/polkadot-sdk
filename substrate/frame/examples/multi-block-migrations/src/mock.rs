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

//! # Mock runtime for testing Multi-Block-Migrations
//!
//! This runtime is for testing only and should *never* be used in production. Please see the
//! comments on the specific config items. The core part of this runtime is the
//! [`pallet_migrations::Config`] implementation, where you define the migrations you want to run
//! using the [`Migrations`] type.
//!
//! ## How to Read the Documentation
//!
//! To access and navigate this documentation in your browser, use the following command:
//!
//! - `cargo doc --package pallet-examples-runtime-mbm --open`
//!
//! This documentation is organized to help you understand how this runtime is configured and how
//! it uses the Multi-Block Migrations Framework.

use crate::migrations::{v1, v1::weights::SubstrateWeight};
use frame_support::{
	construct_runtime, derive_impl, migrations::FreezeChainOnFailedMigration,
	pallet_prelude::Weight, traits::ConstU32,
};

type Block = frame_system::mocking::MockBlock<Runtime>;

impl crate::Config for Runtime {}

impl pallet_migrations::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	// Here we inject the actual MBMs. Currently there is just one, but it accepts a tuple.
	//
	// # Example
	// ```ignore
	// type Migrations = (v1::Migration<Runtime>, v2::Migration<Runtime>, v3::Migration<Runtime>);
	// ```
	#[cfg(not(feature = "runtime-benchmarks"))]
	type Migrations = (v1::LazyMigrationV1<Runtime, SubstrateWeight<Runtime>>,);
	type CursorMaxLen = ConstU32<65_536>;
	type IdentifierMaxLen = ConstU32<256>;
	type MigrationStatusHandler = ();
	type FailedMigrationHandler = FreezeChainOnFailedMigration;
	type MaxServiceWeight = MigratorServiceWeight;
	type WeightInfo = ();
}

frame_support::parameter_types! {
	pub storage MigratorServiceWeight: Weight = Weight::from_parts(100, 100); // do not use in prod
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
	type MultiBlockMigrator = Migrator;
}

// Construct the runtime using the `construct_runtime` macro, specifying the pallet_migrations.
construct_runtime! {
	pub struct Runtime
	{
		System: frame_system,
		Pallet: crate,
		Migrator: pallet_migrations,
	}
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	sp_io::TestExternalities::new(Default::default())
}
