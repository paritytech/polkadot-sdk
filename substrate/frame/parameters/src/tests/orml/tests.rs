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

//! Tests for testing that the ORML params store and the parameters pallet work together.

#![cfg(test)]

use frame_support::{assert_ok, traits::dynamic_params::ParameterStore};

use super::{
	mock::{orml_params, RuntimeOrigin as Origin, *},
	pallet::Config,
};

#[test]
fn get_param_works() {
	new_test_ext().execute_with(|| {
		set_magic(123);

		// Querying the parameter through the parameter storage works:
		assert_eq!(
			<Runtime as Config>::ParameterStore::get(super::pallet::DynamicMagicNumber),
			Some(123),
		);
	});
}

#[test]
fn get_param_in_pallet_works() {
	new_test_ext().execute_with(|| {
		set_magic(123);

		assert_ok!(OrmlParams::check_param(Origin::root(), Some(123),));
	});
}

fn set_magic(v: u32) {
	assert_ok!(PalletParameters::set_parameter(
		Origin::root(),
		orml_params::RuntimeParameters::Shared(super::pallet::Parameters::DynamicMagicNumber(
			super::pallet::DynamicMagicNumber,
			Some(v)
		)),
	));
}
