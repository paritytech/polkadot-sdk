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

