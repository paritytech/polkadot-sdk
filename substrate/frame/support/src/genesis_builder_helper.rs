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

//! Helper functions for implementing [`sp_genesis_builder::GenesisBuilder`] for runtimes.
//!
//! Provides common logic. For more info refer to [`sp_genesis_builder::GenesisBuilder`].

extern crate alloc;

use alloc::vec::Vec;
use frame_support::traits::BuildGenesisConfig;
use sp_genesis_builder::{PresetId, Result as BuildResult};
use sp_runtime::format_runtime_string;

/// Build `GenesisConfig` from a JSON blob not using any defaults and store it in the storage. For
/// more info refer to [`sp_genesis_builder::GenesisBuilder::build_state`].
pub fn build_state<GC: BuildGenesisConfig>(json: Vec<u8>) -> BuildResult {
	let gc = serde_json::from_slice::<GC>(&json)
		.map_err(|e| format_runtime_string!("Invalid JSON blob: {}", e))?;
	<GC as BuildGenesisConfig>::build(&gc);
	Ok(())
}

/// Get the default `GenesisConfig` as a JSON blob if `name` is None.
///
/// Query of named presets is delegetaed to provided `preset_for_name` closure. For more info refer
/// to [`sp_genesis_builder::GenesisBuilder::get_preset`].
pub fn get_preset<GC>(
	name: &Option<PresetId>,
	preset_for_name: impl FnOnce(&sp_genesis_builder::PresetId) -> Option<alloc::vec::Vec<u8>>,
) -> Option<Vec<u8>>
where
	GC: BuildGenesisConfig + Default,
{
	name.as_ref().map_or(
		Some(
			serde_json::to_string(&GC::default())
				.expect("serialization to json is expected to work. qed.")
				.into_bytes(),
		),
		preset_for_name,
	)
}

/// Creates a `RuntimeGenesisConfig` JSON patch.
///
/// This macro creates a default `RuntimeGenesisConfig` initializing provided fields with given
/// values, serialize it to JSON blob, and retain only the specified fields.
///
/// This macro helps to prevents errors that could occur from manually creating JSON objects, such
/// as typos or discrepancies caused by future changes to the `RuntimeGenesisConfig` structure. By
/// using the actual struct, it ensures that the JSON generated is valid and up-to-date.
///
/// This macro assumes that `serde(rename_all = "camelCase")` attribute is used for
/// RuntimeGenesisConfig, what should be the case for frame-based runtimes.
///
/// # Example
///
/// ```rust
/// use frame_support::runtime_genesis_config_json;
/// #[derive(Default, serde::Serialize, serde::Deserialize)]
/// #[serde(rename_all = "camelCase")]
/// struct RuntimeGenesisConfig {
///     a_field: u32,
///     b_field: u32,
///     c_field: u32,
/// }
/// assert_eq!(
///     runtime_genesis_config_json! ({a_field:31, b_field:127}),
///     serde_json::json!({"aField":31, "bField":127})
/// );
/// assert_eq!(
///     runtime_genesis_config_json! ({a_field:31}),
///     serde_json::json!({"aField":31})
/// );
/// ```
#[macro_export]
macro_rules! runtime_genesis_config_json {
	({ $( $key:ident : $value:expr ),* $(,)? }) => {{
        let config = RuntimeGenesisConfig {
            $( $key : $value, )*
            ..Default::default()
        };

		#[inline]
        fn compare_keys(
            mut snake_chars: core::str::Chars,
            mut camel_chars: core::str::Chars,
		) -> bool {
			loop {
				match (snake_chars.next(), camel_chars.next()) {
					(None, None) => return true,
					(None, Some(_)) | (Some(_), None) => return false,
					(Some('_'), Some(c)) => {
						if let Some(s) = snake_chars.next() {
							if s.to_ascii_uppercase() != c {
								return false;
							};
						};
					}
					(Some(s), Some(c)) => {
						if c != s {
							return false;
						}
					}
				}
			}
		}

		let mut json_value =
			serde_json::to_value(config).expect("serialization to json should work. qed");
		if let serde_json::Value::Object(ref mut map) = json_value {
            let keys_to_keep : Vec<&'static str> = vec![ $( stringify!($key) ),* ];
			map.retain(|json_key, _| {
				keys_to_keep.iter().any(|&struct_key| {
					compare_keys(struct_key.chars(), json_key.chars())
				})
			});
		}
        json_value
    }};
}
