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

//! Mock runtime that configures a pallet with the ORML parameters store and the non-ORML aggregated
//! macro to check that both work at the same time.

#![cfg(test)]

use frame_support::{
	construct_runtime, derive_impl,
	traits::{dynamic_params::ParameterStoreAdapter, ConstU64, EnsureOriginWithArg},
};
use frame_system::ensure_root;

use super::{pallet as pallet_orml_params, pallet::Config};
use crate as parameters;

// This is the ORML way of declaring the parameters:
pub(crate) mod orml_params {
	use crate::tests::orml::mock::pallet_orml_params;

	frame_support::define_aggregrated_parameters! {
		pub RuntimeParameters = {
			Shared: pallet_orml_params::Parameters = 0,
		}
	}
}

type Block = frame_system::mocking::MockBlock<Runtime>;

#[derive_impl(frame_system::config_preludes::ParaChainDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
	type BlockHashCount = ConstU64<250>;
}

impl crate::Config for Runtime {
	// Inject the aggregated parameters into the runtime:
	type AggregratedKeyValue = orml_params::RuntimeParameters;

	type RuntimeEvent = RuntimeEvent;
	type AdminOrigin = EnsureOriginImpl;
	type WeightInfo = ();
}

impl Config for Runtime {
	// ORML config:
	type ParameterStore = ParameterStoreAdapter<PalletParameters, super::pallet::Parameters>;
	// Non-ORML config:
	//type DynamicMagicNumber = dynamic_params::shared::DynamicMagicNumber;

	type RuntimeEvent = RuntimeEvent;
}

pub struct EnsureOriginImpl;

impl EnsureOriginWithArg<RuntimeOrigin, orml_params::RuntimeParametersKey> for EnsureOriginImpl {
	type Success = ();

	fn try_origin(
		origin: RuntimeOrigin,
		key: &orml_params::RuntimeParametersKey,
	) -> Result<Self::Success, RuntimeOrigin> {
		match key {
			orml_params::RuntimeParametersKey::Shared(_) => {
				ensure_root(origin.clone()).map_err(|_| origin)?;
				return Ok(())
			},
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin(_key: &RuntimeParametersKey) -> Result<RuntimeOrigin, ()> {
		unimplemented!()
	}
}

construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		PalletParameters: parameters,
		OrmlParams: pallet_orml_params,
	}
);

pub fn new_test_ext() -> sp_io::TestExternalities {
	sp_io::TestExternalities::new(Default::default())
}
