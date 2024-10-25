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

//! Presets for the chain-spec demo runtime.

use crate::{
	pallets::{FooEnum, SomeFooData1, SomeFooData2},
	runtime::{BarConfig, FooConfig, RuntimeGenesisConfig},
};
use alloc::vec;
use frame_support::build_struct_json_patch;
use serde_json::{json, to_string, Value};
use sp_application_crypto::Ss58Codec;
use sp_keyring::AccountKeyring;

/// A demo preset with strings only.
pub const PRESET_1: &str = "preset_1";
/// A demo preset with real types.
pub const PRESET_2: &str = "preset_2";
/// Another demo preset with real types and manually created json object.
pub const PRESET_3: &str = "preset_3";
/// A single value patch preset.
pub const PRESET_4: &str = "preset_4";
/// A single value patch preset.
pub const PRESET_INVALID: &str = "preset_invalid";

#[docify::export]
/// Function provides a preset demonstrating how use string representation of preset's internal
/// values.
fn preset_1() -> Value {
	json!({
		"bar": {
			"initialAccount": "5CiPPseXPECbkjWCa6MnjNokrgYjMqmKndv2rSnekmSK2DjL",
		},
		"foo": {
			"someEnum": {
				"Data2": {
					"values": "0x0c0f"
				}
			},
			"someStruct" : {
				"fieldA": 10,
				"fieldB": 20
			},
			"someInteger": 100
		},
	})
}

#[docify::export]
/// Function provides a preset demonstrating how to create a preset using
/// [`build_struct_json_patch`] macro.
fn preset_2() -> Value {
	build_struct_json_patch!(RuntimeGenesisConfig {
		foo: FooConfig {
			some_integer: 200,
			some_enum: FooEnum::Data2(SomeFooData2 { values: vec![0x0c, 0x10] })
		},
		bar: BarConfig { initial_account: Some(AccountKeyring::Ferdie.public().into()) },
	})
}

#[docify::export]
/// Function provides a preset demonstrating how use the actual types to manually create a JSON
/// representing the preset.
fn preset_3() -> Value {
	json!({
		"bar": {
			"initialAccount": AccountKeyring::Alice.public().to_ss58check(),
		},
		"foo": {
			"someEnum": FooEnum::Data1(
				SomeFooData1 {
					a: 12,
					b: 16
				}
			),
			"someInteger": 300
		},
	})
}

#[docify::export]
/// Function provides a minimal preset demonstrating how to patch single key in
/// `RuntimeGenesisConfig` using [`build_struct_json_patch`] macro.
pub fn preset_4() -> Value {
	build_struct_json_patch!(RuntimeGenesisConfig {
		foo: FooConfig { some_enum: FooEnum::Data2(SomeFooData2 { values: vec![0x0c, 0x10] }) },
	})
}

#[docify::export]
/// Function provides an invalid preset demonstrating how important is use of
/// `deny_unknown_fields` in data structures used in `GenesisConfig`.
fn preset_invalid() -> Value {
	json!({
		"foo": {
			"someStruct": {
				"fieldC": 5
			},
		},
	})
}

/// Provides a JSON representation of preset identified by given `id`.
///
/// If no preset with given `id` exits `None` is returned.
#[docify::export]
pub fn get_builtin_preset(id: &sp_genesis_builder::PresetId) -> Option<alloc::vec::Vec<u8>> {
	let preset = match id.try_into() {
		Ok(PRESET_1) => preset_1(),
		Ok(PRESET_2) => preset_2(),
		Ok(PRESET_3) => preset_3(),
		Ok(PRESET_4) => preset_4(),
		Ok(PRESET_INVALID) => preset_invalid(),
		_ => return None,
	};

	Some(
		to_string(&preset)
			.expect("serialization to json is expected to work. qed.")
			.into_bytes(),
	)
}

#[test]
#[docify::export]
fn check_presets() {
	let builder = sc_chain_spec::GenesisConfigBuilderRuntimeCaller::<()>::new(
		crate::WASM_BINARY.expect("wasm binary shall exists"),
	);
	assert!(builder.get_storage_for_named_preset(Some(&PRESET_1.to_string())).is_ok());
	assert!(builder.get_storage_for_named_preset(Some(&PRESET_2.to_string())).is_ok());
	assert!(builder.get_storage_for_named_preset(Some(&PRESET_3.to_string())).is_ok());
	assert!(builder.get_storage_for_named_preset(Some(&PRESET_4.to_string())).is_ok());
}

#[test]
#[docify::export]
fn invalid_preset_works() {
	let builder = sc_chain_spec::GenesisConfigBuilderRuntimeCaller::<()>::new(
		crate::WASM_BINARY.expect("wasm binary shall exists"),
	);
	// Even though a preset contains invalid_key, conversion to raw storage does not fail. This is
	// because the [`FooStruct`] structure is not annotated with `deny_unknown_fields` [`serde`]
	// attribute.
	// This may lead to hard to debug problems, that's why using ['deny_unknown_fields'] is
	// recommended.
	assert!(builder.get_storage_for_named_preset(Some(&PRESET_INVALID.to_string())).is_ok());
}
