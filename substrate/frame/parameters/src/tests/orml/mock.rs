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

use frame_support::{
	construct_runtime, derive_impl,
	dynamic_params::{dynamic_pallet_params, dynamic_params},
	traits::{ConstU32, ConstU64, EnsureOriginWithArg},
};
use frame_system::{ensure_root, ensure_signed};
use orml_traits::parameters::{AggregratedKeyValue, ParameterStore};
use sp_core::H256;
use sp_runtime::{traits::IdentityLookup, BuildStorage, Permill};

use super::pallet::{self as pallet_orml_params, Config, *};
use crate as parameters;

/// This is the ORML parameter storage:
pub struct ParameterStoreImpl;
impl ParameterStore<Parameters> for ParameterStoreImpl {
	fn get<K>(key: K) -> Option<K::Value>
	where
		K: orml_traits::parameters::Key + Into<<Parameters as AggregratedKeyValue>::AggregratedKey>,
		<Parameters as orml_traits::parameters::AggregratedKeyValue>::AggregratedValue:
			TryInto<K::WrappedValue>,
	{
		let key = key.into();
		match key {
			ParametersKey::InstantUnstakeFee(_) => Some(
				ParametersValue::InstantUnstakeFee(Permill::from_percent(10))
					.try_into()
					.ok()?
					.into(),
			),
		}
	}
}

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

impl Config for Runtime {
	//type ParameterStore = ParameterStoreImpl;
	type RuntimeEvent = RuntimeEvent;
}

construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		ModuleParameters: parameters,
		Balances: pallet_balances,
		OrmlPallet: pallet_orml_params,
	}
);

pub fn new_test_ext() -> sp_io::TestExternalities {
	sp_io::TestExternalities::new(Default::default())
}
