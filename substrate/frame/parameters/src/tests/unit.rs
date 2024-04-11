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

//! Unit tests for the parameters pallet.

#![cfg(test)]

use crate::tests::mock::{
	assert_last_event, dynamic_params::*, new_test_ext, PalletParameters, Runtime,
	RuntimeOrigin as Origin, RuntimeParameters, RuntimeParameters::*, RuntimeParametersKey,
	RuntimeParametersValue,
};
use codec::Encode;
use frame_support::{assert_noop, assert_ok, traits::dynamic_params::AggregatedKeyValue};
use sp_core::Get;
use sp_runtime::DispatchError;

#[docify::export]
#[test]
fn set_parameters_example() {
	new_test_ext().execute_with(|| {
		assert_eq!(pallet1::Key3::get(), 2, "Default works");

		// This gets rejected since the origin is not root.
		assert_noop!(
			PalletParameters::set_parameter(
				Origin::signed(1),
				Pallet1(pallet1::Parameters::Key3(pallet1::Key3, Some(123))),
			),
			DispatchError::BadOrigin
		);

		assert_ok!(PalletParameters::set_parameter(
			Origin::root(),
			Pallet1(pallet1::Parameters::Key3(pallet1::Key3, Some(123))),
		));

		assert_eq!(pallet1::Key3::get(), 123, "Update works");
		assert_last_event(
			crate::Event::Updated {
				key: RuntimeParametersKey::Pallet1(pallet1::ParametersKey::Key3(pallet1::Key3)),
				old_value: None,
				new_value: Some(RuntimeParametersValue::Pallet1(pallet1::ParametersValue::Key3(
					123,
				))),
			}
			.into(),
		);
	});
}

#[test]
fn set_parameters_same_is_noop() {
	new_test_ext().execute_with(|| {
		assert_ok!(PalletParameters::set_parameter(
			Origin::root(),
			Pallet1(pallet1::Parameters::Key3(pallet1::Key3, Some(123))),
		));

		assert_ok!(PalletParameters::set_parameter(
			Origin::root(),
			Pallet1(pallet1::Parameters::Key3(pallet1::Key3, Some(123))),
		));

		assert_eq!(pallet1::Key3::get(), 123, "Update works");
	});
}

#[test]
fn set_parameters_twice_works() {
	new_test_ext().execute_with(|| {
		assert_ok!(PalletParameters::set_parameter(
			Origin::root(),
			Pallet1(pallet1::Parameters::Key3(pallet1::Key3, Some(123))),
		));

		assert_ok!(PalletParameters::set_parameter(
			Origin::root(),
			Pallet1(pallet1::Parameters::Key3(pallet1::Key3, Some(432))),
		));

		assert_eq!(pallet1::Key3::get(), 432, "Update works");
	});
}

#[test]
fn set_parameters_removing_restores_default_works() {
	new_test_ext().execute_with(|| {
		assert_ok!(PalletParameters::set_parameter(
			Origin::root(),
			Pallet1(pallet1::Parameters::Key3(pallet1::Key3, Some(123))),
		));

		assert_eq!(pallet1::Key3::get(), 123, "Update works");
		assert!(
			crate::Parameters::<Runtime>::contains_key(RuntimeParametersKey::Pallet1(
				pallet1::ParametersKey::Key3(pallet1::Key3)
			)),
			"Key inserted"
		);

		// Removing the value restores the default.
		assert_ok!(PalletParameters::set_parameter(
			Origin::root(),
			Pallet1(pallet1::Parameters::Key3(pallet1::Key3, None)),
		));

		assert_eq!(pallet1::Key3::get(), 2, "Default restored");
		assert!(
			!crate::Parameters::<Runtime>::contains_key(RuntimeParametersKey::Pallet1(
				pallet1::ParametersKey::Key3(pallet1::Key3)
			)),
			"Key removed"
		);
	});
}

#[test]
fn set_parameters_to_default_emits_events_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(pallet1::Key3::get(), 2);
		assert_ok!(PalletParameters::set_parameter(
			Origin::root(),
			Pallet1(pallet1::Parameters::Key3(pallet1::Key3, Some(2))),
		));
		assert_eq!(pallet1::Key3::get(), 2);

		assert!(
			crate::Parameters::<Runtime>::contains_key(RuntimeParametersKey::Pallet1(
				pallet1::ParametersKey::Key3(pallet1::Key3)
			)),
			"Key inserted"
		);
		assert_last_event(
			crate::Event::Updated {
				key: RuntimeParametersKey::Pallet1(pallet1::ParametersKey::Key3(pallet1::Key3)),
				old_value: None,
				new_value: Some(RuntimeParametersValue::Pallet1(pallet1::ParametersValue::Key3(2))),
			}
			.into(),
		);

		// It will also emit a second event:
		assert_ok!(PalletParameters::set_parameter(
			Origin::root(),
			Pallet1(pallet1::Parameters::Key3(pallet1::Key3, Some(2))),
		));
		assert_eq!(frame_system::Pallet::<Runtime>::events().len(), 2);
	});
}

#[test]
fn set_parameters_wrong_origin_errors() {
	new_test_ext().execute_with(|| {
		// Pallet1 is root origin only:
		assert_ok!(PalletParameters::set_parameter(
			Origin::root(),
			Pallet1(pallet1::Parameters::Key3(pallet1::Key3, Some(123))),
		));

		assert_noop!(
			PalletParameters::set_parameter(
				Origin::signed(1),
				Pallet1(pallet1::Parameters::Key3(pallet1::Key3, Some(432))),
			),
			DispatchError::BadOrigin
		);

		// Pallet2 is signed origin only:
		assert_ok!(PalletParameters::set_parameter(
			Origin::signed(1),
			Pallet2(pallet2::Parameters::Key3(pallet2::Key3, Some(123))),
		));

		assert_noop!(
			PalletParameters::set_parameter(
				Origin::root(),
				Pallet2(pallet2::Parameters::Key3(pallet2::Key3, Some(432))),
			),
			DispatchError::BadOrigin
		);
	});
}

#[test]
fn get_through_external_pallet_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(<Runtime as pallet_example_basic::Config>::MagicNumber::get(), 0);

		assert_ok!(PalletParameters::set_parameter(
			Origin::root(),
			Pallet1(pallet1::Parameters::Key1(pallet1::Key1, Some(123))),
		));

		assert_eq!(<Runtime as pallet_example_basic::Config>::MagicNumber::get(), 123);
	});
}

#[test]
fn test_define_parameters_key_convert() {
	let key1 = pallet1::Key1;
	let parameter_key: pallet1::ParametersKey = key1.clone().into();
	let key1_2: pallet1::Key1 = parameter_key.clone().try_into().unwrap();

	assert_eq!(key1, key1_2);
	assert_eq!(parameter_key, pallet1::ParametersKey::Key1(key1));

	let key2 = pallet1::Key2;
	let parameter_key: pallet1::ParametersKey = key2.clone().into();
	let key2_2: pallet1::Key2 = parameter_key.clone().try_into().unwrap();

	assert_eq!(key2, key2_2);
	assert_eq!(parameter_key, pallet1::ParametersKey::Key2(key2));
}

#[test]
fn test_define_parameters_value_convert() {
	let value1 = pallet1::Key1Value(1);
	let parameter_value: pallet1::ParametersValue = value1.clone().into();
	let value1_2: pallet1::Key1Value = parameter_value.clone().try_into().unwrap();

	assert_eq!(value1, value1_2);
	assert_eq!(parameter_value, pallet1::ParametersValue::Key1(1));

	let value2 = pallet1::Key2Value(2);
	let parameter_value: pallet1::ParametersValue = value2.clone().into();
	let value2_2: pallet1::Key2Value = parameter_value.clone().try_into().unwrap();

	assert_eq!(value2, value2_2);
	assert_eq!(parameter_value, pallet1::ParametersValue::Key2(2));
}

#[test]
fn test_define_parameters_aggregrated_key_value() {
	let kv1 = pallet1::Parameters::Key1(pallet1::Key1, None);
	let (key1, value1) = kv1.clone().into_parts();

	assert_eq!(key1, pallet1::ParametersKey::Key1(pallet1::Key1));
	assert_eq!(value1, None);

	let kv2 = pallet1::Parameters::Key2(pallet1::Key2, Some(2));
	let (key2, value2) = kv2.clone().into_parts();

	assert_eq!(key2, pallet1::ParametersKey::Key2(pallet1::Key2));
	assert_eq!(value2, Some(pallet1::ParametersValue::Key2(2)));
}

#[test]
fn test_define_aggregrated_parameters_key_convert() {
	use codec::Encode;

	let key1 = pallet1::Key1;
	let parameter_key: pallet1::ParametersKey = key1.clone().into();
	let runtime_key: RuntimeParametersKey = parameter_key.clone().into();

	assert_eq!(runtime_key, RuntimeParametersKey::Pallet1(pallet1::ParametersKey::Key1(key1)));
	assert_eq!(runtime_key.encode(), vec![3, 0]);

	let key2 = pallet2::Key2;
	let parameter_key: pallet2::ParametersKey = key2.clone().into();
	let runtime_key: RuntimeParametersKey = parameter_key.clone().into();

	assert_eq!(runtime_key, RuntimeParametersKey::Pallet2(pallet2::ParametersKey::Key2(key2)));
	assert_eq!(runtime_key.encode(), vec![1, 1]);
}

#[test]
fn test_define_aggregrated_parameters_aggregrated_key_value() {
	let kv1 = RuntimeParameters::Pallet1(pallet1::Parameters::Key1(pallet1::Key1, None));
	let (key1, value1) = kv1.clone().into_parts();

	assert_eq!(key1, RuntimeParametersKey::Pallet1(pallet1::ParametersKey::Key1(pallet1::Key1)));
	assert_eq!(value1, None);

	let kv2 = RuntimeParameters::Pallet2(pallet2::Parameters::Key2(pallet2::Key2, Some(2)));
	let (key2, value2) = kv2.clone().into_parts();

	assert_eq!(key2, RuntimeParametersKey::Pallet2(pallet2::ParametersKey::Key2(pallet2::Key2)));
	assert_eq!(value2, Some(RuntimeParametersValue::Pallet2(pallet2::ParametersValue::Key2(2))));
}

#[test]
fn codec_index_works() {
	let enc = RuntimeParametersKey::Pallet1(pallet1::ParametersKey::Key1(pallet1::Key1)).encode();
	assert_eq!(enc, vec![3, 0]);
	let enc = RuntimeParametersKey::Pallet1(pallet1::ParametersKey::Key2(pallet1::Key2)).encode();
	assert_eq!(enc, vec![3, 1]);
	let enc = RuntimeParametersKey::Pallet1(pallet1::ParametersKey::Key3(pallet1::Key3)).encode();
	assert_eq!(enc, vec![3, 2]);

	let enc = RuntimeParametersKey::Pallet2(pallet2::ParametersKey::Key1(pallet2::Key1)).encode();
	assert_eq!(enc, vec![1, 2]);
	let enc = RuntimeParametersKey::Pallet2(pallet2::ParametersKey::Key2(pallet2::Key2)).encode();
	assert_eq!(enc, vec![1, 1]);
	let enc = RuntimeParametersKey::Pallet2(pallet2::ParametersKey::Key3(pallet2::Key3)).encode();
	assert_eq!(enc, vec![1, 0]);
}
