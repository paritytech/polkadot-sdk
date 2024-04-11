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

#![cfg(any(all(feature = "try-runtime", test), doc))]

use crate::*;
use frame_support::{derive_impl, traits::ConstU64, weights::constants::ParityDbWeight};

// Re-export crate as its pallet name for construct_runtime.
use crate as pallet_example_storage_migration;

type Block = frame_system::mocking::MockBlock<MockRuntime>;

// For testing the pallet, we construct a mock runtime.
frame_support::construct_runtime!(
	pub struct MockRuntime {
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		Example: pallet_example_storage_migration::{Pallet, Call, Storage},
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for MockRuntime {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
	type DbWeight = ParityDbWeight;
}

impl pallet_balances::Config for MockRuntime {
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type Balance = u64;
	type DustRemoval = ();
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = ConstU64<1>;
	type AccountStore = System;
	type WeightInfo = ();
	type FreezeIdentifier = ();
	type MaxFreezes = ();
}

impl Config for MockRuntime {}

pub fn new_test_ext() -> sp_io::TestExternalities {
	use sp_runtime::BuildStorage;

	let t = RuntimeGenesisConfig { system: Default::default(), balances: Default::default() }
		.build_storage()
		.unwrap();
	t.into()
}
