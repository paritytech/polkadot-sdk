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

//! Mock file for system benchmarking.

#![cfg(test)]

use codec::Encode;
use frame_support::{derive_impl, weights::Weight};
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
	}
);

pub struct MockWeights;
impl frame_system::ExtensionsWeightInfo for MockWeights {
	fn check_genesis() -> Weight {
		Weight::from_parts(10, 0)
	}

	fn check_mortality_mortal_transaction() -> Weight {
		Weight::from_parts(10, 0)
	}

	fn check_mortality_immortal_transaction() -> Weight {
		Weight::from_parts(10, 0)
	}

	fn check_non_zero_sender() -> Weight {
		Weight::from_parts(10, 0)
	}

	fn check_nonce() -> Weight {
		Weight::from_parts(10, 0)
	}

	fn check_spec_version() -> Weight {
		Weight::from_parts(10, 0)
	}

	fn check_tx_version() -> Weight {
		Weight::from_parts(10, 0)
	}

	fn check_weight() -> Weight {
		Weight::from_parts(10, 0)
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type ExtensionsWeightInfo = MockWeights;
}

impl crate::Config for Test {}

struct MockedReadRuntimeVersion(Vec<u8>);

impl sp_core::traits::ReadRuntimeVersion for MockedReadRuntimeVersion {
	fn read_runtime_version(
		&self,
		_wasm_code: &[u8],
		_ext: &mut dyn sp_externalities::Externalities,
	) -> Result<Vec<u8>, String> {
		Ok(self.0.clone())
	}
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

	let version = sp_version::RuntimeVersion {
		spec_name: "spec_name".into(),
		spec_version: 123,
		impl_version: 456,
		..Default::default()
	};
	let read_runtime_version = MockedReadRuntimeVersion(version.encode());
	let mut ext = sp_io::TestExternalities::new(t);
	ext.register_extension(sp_core::traits::ReadRuntimeVersionExt::new(read_runtime_version));
	ext
}
