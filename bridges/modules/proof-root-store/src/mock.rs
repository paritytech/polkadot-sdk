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

use crate as pallet_bridge_proof_root_store;
use frame_support::{
	derive_impl,
	sp_runtime::{traits::ConstU32, BuildStorage},
};
use frame_system::EnsureRoot;

type Block = frame_system::mocking::MockBlock<TestRuntime>;

frame_support::construct_runtime! {
	pub enum TestRuntime {
		System: frame_system,
		ProofRootStore: pallet_bridge_proof_root_store,
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for TestRuntime {
	type Block = Block;
}

impl pallet_bridge_proof_root_store::Config for TestRuntime {
	type WeightInfo = ();
	type SubmitOrigin = EnsureRoot<u64>;
	type Key = u8;
	type Value = u8;
	type RootsToKeep = ConstU32<4>;
}

/// Return test externalities to use in tests.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<TestRuntime>::default().build_storage().unwrap();
	sp_io::TestExternalities::new(t)
}

/// Run pallet test.
pub fn run_test<T>(test: impl FnOnce() -> T) -> T {
	new_test_ext().execute_with(test)
}
