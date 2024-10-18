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

//! Mock runtime for `view-functions-example` tests.
#![cfg(test)]

use crate::{pallet, pallet2};
use frame_support::derive_impl;
use sp_runtime::testing::TestXt;

pub type AccountId = u32;
pub type Balance = u32;

type Block = frame_system::mocking::MockBlock<Runtime>;
frame_support::construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		ViewFunctionsExample: pallet,
		ViewFunctionsInstance: pallet2,
		ViewFunctionsInstance1: pallet2::<Instance1>,
	}
);

pub type Extrinsic = TestXt<RuntimeCall, ()>;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
}

impl pallet::Config for Runtime {}
impl pallet2::Config<pallet2::Instance1> for Runtime {}

impl pallet2::Config for Runtime {}

pub fn new_test_ext() -> sp_io::TestExternalities {
	use sp_runtime::BuildStorage;

	let t = RuntimeGenesisConfig { system: Default::default() }.build_storage().unwrap();
	t.into()
}
