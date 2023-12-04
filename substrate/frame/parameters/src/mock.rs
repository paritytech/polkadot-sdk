// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

#![cfg(test)]

use frame_support::{
	construct_runtime,
	dynamic_params::{dynamic_pallet_params, dynamic_params},
	traits::{ConstU32, ConstU64, EnsureOriginWithArg, Everything},
};
use sp_core::H256;
use sp_runtime::{traits::IdentityLookup, BuildStorage};

use super::*;

use crate as parameters;

pub type AccountId = u128;
type Block = frame_system::mocking::MockBlock<Runtime>;

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
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type DbWeight = ();
	type BaseCallFilter = Everything;
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
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
		ModuleParameters: parameters,
		Example: pallet_example_basic,
		Balances: pallet_balances,
	}
);

pub struct ExtBuilder;

impl ExtBuilder {
	pub fn new() -> sp_io::TestExternalities {
		let t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

		let mut ext = sp_io::TestExternalities::new(t);
		ext.execute_with(|| System::set_block_number(1));
		ext
	}
}
