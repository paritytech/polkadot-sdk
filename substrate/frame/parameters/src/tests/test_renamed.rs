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

#![cfg(any(test, feature = "runtime-benchmarks"))]

//! Tests that the runtime params can be renamed.

use frame_support::{
	assert_noop, assert_ok, construct_runtime, derive_impl,
	dynamic_params::{dynamic_pallet_params, dynamic_params},
	traits::AsEnsureOriginWithArg,
};
use frame_system::EnsureRoot;

use crate as pallet_parameters;
use crate::*;
use dynamic_params::*;
use RuntimeParametersRenamed::*;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = frame_system::mocking::MockBlock<Runtime>;
	type AccountData = pallet_balances::AccountData<<Self as pallet_balances::Config>::Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type ReserveIdentifier = [u8; 8];
	type AccountStore = System;
}

#[dynamic_params(RuntimeParametersRenamed, pallet_parameters::Parameters::<Runtime>)]
pub mod dynamic_params {
	use super::*;

	#[dynamic_pallet_params]
	#[codec(index = 3)]
	pub mod pallet1 {
		#[codec(index = 0)]
		pub static Key1: u64 = 0;
		#[codec(index = 1)]
		pub static Key2: u32 = 1;
		#[codec(index = 2)]
		pub static Key3: u128 = 2;
	}

	#[dynamic_pallet_params]
	#[codec(index = 1)]
	pub mod pallet2 {
		#[codec(index = 2)]
		pub static Key1: u64 = 0;
		#[codec(index = 1)]
		pub static Key2: u32 = 2;
		#[codec(index = 0)]
		pub static Key3: u128 = 4;
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl Default for RuntimeParametersRenamed {
	fn default() -> Self {
		RuntimeParametersRenamed::Pallet1(dynamic_params::pallet1::Parameters::Key1(
			dynamic_params::pallet1::Key1,
			Some(123),
		))
	}
}

#[derive_impl(pallet_parameters::config_preludes::TestDefaultConfig)]
impl Config for Runtime {
	type AdminOrigin = AsEnsureOriginWithArg<EnsureRoot<Self::AccountId>>;
	type RuntimeParameters = RuntimeParametersRenamed;
	// RuntimeEvent is injected by the `derive_impl` macro.
	// WeightInfo is injected by the `derive_impl` macro.
}

impl pallet_example_basic::Config for Runtime {
	// Use the dynamic key in the pallet config:
	type MagicNumber = dynamic_params::pallet1::Key1;

	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
}

construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		PalletParameters: crate,
		Example: pallet_example_basic,
		Balances: pallet_balances,
	}
);

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext = sp_io::TestExternalities::new(Default::default());
	ext.execute_with(|| System::set_block_number(1));
	ext
}

pub(crate) fn assert_last_event(generic_event: RuntimeEvent) {
	let events = frame_system::Pallet::<Runtime>::events();
	// compare to the last event record
	let frame_system::EventRecord { event, .. } = &events.last().expect("Event expected");
	assert_eq!(event, &generic_event);
}

#[test]
fn set_parameters_example() {
	new_test_ext().execute_with(|| {
		assert_eq!(pallet1::Key3::get(), 2, "Default works");

		// This gets rejected since the origin is not root.
		assert_noop!(
			PalletParameters::set_parameter(
				RuntimeOrigin::signed(1),
				Pallet1(pallet1::Parameters::Key3(pallet1::Key3, Some(123))),
			),
			DispatchError::BadOrigin
		);

		assert_ok!(PalletParameters::set_parameter(
			RuntimeOrigin::root(),
			Pallet1(pallet1::Parameters::Key3(pallet1::Key3, Some(123))),
		));

		assert_eq!(pallet1::Key3::get(), 123, "Update works");
		assert_last_event(
			crate::Event::Updated {
				key: RuntimeParametersRenamedKey::Pallet1(pallet1::ParametersKey::Key3(
					pallet1::Key3,
				)),
				old_value: None,
				new_value: Some(RuntimeParametersRenamedValue::Pallet1(
					pallet1::ParametersValue::Key3(123),
				)),
			}
			.into(),
		);
	});
}

#[test]
fn get_through_external_pallet_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(<Runtime as pallet_example_basic::Config>::MagicNumber::get(), 0);

		assert_ok!(PalletParameters::set_parameter(
			RuntimeOrigin::root(),
			Pallet1(pallet1::Parameters::Key1(pallet1::Key1, Some(123))),
		));

		assert_eq!(<Runtime as pallet_example_basic::Config>::MagicNumber::get(), 123);
	});
}
