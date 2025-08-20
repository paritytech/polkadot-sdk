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

use crate as pallet_bridge_proof_root_sync;
use frame_support::{derive_impl, sp_runtime::BuildStorage};
use std::cell::RefCell;

type Block = frame_system::mocking::MockBlock<TestRuntime>;

// Configure mock runtime
frame_support::parameter_types! {
	pub const MaxRootsToSend: u32 = 2;
	pub const RootsToKeep: u32 = 5;
}

frame_support::construct_runtime! {
	pub enum TestRuntime {
		System: frame_system,
		HeadersSync: pallet_bridge_proof_root_sync,
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for TestRuntime {
	type Block = Block;
}

#[derive(Default)]
pub struct OnSendConsumer;
impl OnSendConsumer {
	pub fn get_consumed_roots() -> Vec<(u32, u32)> {
		CONSUMED_ROOTS.with(|roots_cell| roots_cell.borrow().clone())
	}
}
thread_local! {
	static CONSUMED_ROOTS: RefCell<Vec<(u32, u32)>> = RefCell::new(Vec::new());
}

impl crate::OnSend<u32, u32> for OnSendConsumer {
	fn on_send(roots: &Vec<(u32, u32)>) {
		CONSUMED_ROOTS.with(|roots_cell| {
			roots_cell.borrow_mut().clear();
			roots_cell.borrow_mut().extend(roots.iter().cloned());
		});
	}
}

#[cfg(feature = "runtime-benchmarks")]
pub struct MockBenchmarkHelper;
#[cfg(feature = "runtime-benchmarks")]
impl crate::benchmarking::BenchmarkHelper<u32, u32> for MockBenchmarkHelper {
	fn create_key_value_for(id: u32) -> (u32, u32) {
		(id, id)
	}
}

impl pallet_bridge_proof_root_sync::Config for TestRuntime {
	type WeightInfo = ();
	type Key = u32;
	type Value = u32;
	type RootsToKeep = RootsToKeep;
	type MaxRootsToSend = MaxRootsToSend;
	type OnSend = OnSendConsumer;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = MockBenchmarkHelper;
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
