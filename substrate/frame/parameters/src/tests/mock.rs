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

//! Test that the `pallet_example_basic` can use the parameters pallet as storage.

use frame_support::{
	construct_runtime, derive_impl,
	dynamic_params::{dynamic_pallet_params, dynamic_params},
	traits::{ConstU32, ConstU64, EnsureOriginWithArg},
};
use sp_core::H256;
use sp_runtime::traits::IdentityLookup;

use crate as parameters;
use crate::*;

pub type AccountId = u128;
type Block = frame_system::mocking::MockBlock<Runtime>;

#[derive_impl(frame_system::config_preludes::ParaChainDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = ::sp_runtime::traits::BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = ConstU64<250>;
	type BlockWeights = ();
	type BlockLength = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<u64>;
	type MaxConsumers = ConstU32<16>;
}

impl pallet_balances::Config for Runtime {
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
	type RuntimeHoldReason = ();
	type RuntimeFreezeReason = ();
	type MaxHolds = ();
}

#[docify::export]
#[dynamic_params(RuntimeParameters)]
pub mod dynamic_params {
	use super::*;

	#[dynamic_pallet_params(crate::Parameters::<Runtime>, Parameters)]
	pub mod pallet1 {
		#[codec(index = 0)]
		pub static Key1: u64 = 0;
		#[codec(index = 1)]
		pub static Key2: u32 = 1;
		#[codec(index = 2)]
		pub static Key3: u128 = 2;
	}

	#[dynamic_pallet_params(crate::Parameters::<Runtime>, Parameters)]
	pub mod pallet2 {
		#[codec(index = 0)]
		pub static Key1: u64 = 0;
		#[codec(index = 1)]
		pub static Key2: u32 = 2;
		#[codec(index = 2)]
		pub static Key3: u128 = 4;
	}
}

#[docify::export(impl_config)]
impl Config for Runtime {
	// Inject the aggregated parameters into the runtime:
	type AggregratedKeyValue = RuntimeParameters;

	type RuntimeEvent = RuntimeEvent;
	type AdminOrigin = EnsureOriginImpl;
	type WeightInfo = ();
}

#[docify::export(usage)]
impl pallet_example_basic::Config for Runtime {
	// Use the dynamic key in the pallet config:
	type MagicNumber = dynamic_params::pallet1::Key1;

	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
}

pub struct EnsureOriginImpl;

impl EnsureOriginWithArg<RuntimeOrigin, RuntimeParametersKey> for EnsureOriginImpl {
	type Success = ();

	fn try_origin(
		origin: RuntimeOrigin,
		key: &RuntimeParametersKey,
	) -> Result<Self::Success, RuntimeOrigin> {
		match key {
			RuntimeParametersKey::Pallet1(_) => {
				ensure_root(origin.clone()).map_err(|_| origin)?;
				return Ok(())
			},
			RuntimeParametersKey::Pallet2(_) => {
				ensure_signed(origin.clone()).map_err(|_| origin)?;
				return Ok(())
			},
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin(_key: &RuntimeParametersKey) -> Result<RuntimeOrigin, ()> {
		Err(())
	}
}

construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		PalletParameters: parameters,
		Example: pallet_example_basic,
		Balances: pallet_balances,
	}
);

pub fn new_test_ext() -> sp_io::TestExternalities {
	sp_io::TestExternalities::new(Default::default())
}
