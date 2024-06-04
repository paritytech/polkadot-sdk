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

//! Presets for demo runtime.

use crate::pallets::{FooEnum, SomeFooData1, SomeFooData2};
use serde_json::{json, to_string, Value};
use sp_application_crypto::Ss58Codec;
use sp_keyring::AccountKeyring;
use sp_std::vec;

pub const PRESET_1: &str = "preset_1";
pub const PRESET_2: &str = "preset_2";
pub const PRESET_3: &str = "preset_3";
pub const PRESET_4: &str = "preset_4";

#[docify::export]
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
			"someInteger": 100
		},
	})
}

#[docify::export]
fn preset_2() -> Value {
	json!({
		"bar": {
			"initialAccount": AccountKeyring::Ferdie.public().to_ss58check(),
		},
		"foo": {
			"someEnum": FooEnum::Data2(SomeFooData2 { values: vec![12,16] }),
			"someInteger": 200
		},
	})
}

#[docify::export]
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
fn preset_4() -> Value {
	json!({
		"foo": {
			"someEnum": {
				"Data2": {
					"values": "0x0c0f"
				}
			},
		},
	})
}

/// Provides a json representation of preset identified by given `id`.
///
/// If no preset with given `id` exits `None` is returned.
#[docify::export]
pub fn get_builtin_preset(id: &sp_genesis_builder::PresetId) -> Option<sp_std::vec::Vec<u8>> {
	let preset = match id.try_into() {
		Ok(PRESET_1) => preset_1(),
		Ok(PRESET_2) => preset_2(),
		Ok(PRESET_3) => preset_3(),
		Ok(PRESET_4) => preset_4(),
		_ => return None,
	};

	Some(
		to_string(&preset)
			.expect("serialization to json is expected to work. qed.")
			.into_bytes(),
	)
}
