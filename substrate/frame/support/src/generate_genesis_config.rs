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

//! Helper macro allowing to construct JSON representation of partially initialized structs.

use serde_json::Value;
extern crate alloc;
use alloc::{borrow::Cow, format, string::String};

/// Represents the initialization method of a field within a struct.
///
/// This enum provides information about how it was initialized and the field name (as a `String`).
///
/// Intended to be used in `build_struct_json_patch` macro.
#[derive(Debug)]
pub enum InitializedField<'a> {
	/// The field was partially initialized (e.g., specific fields within the struct were set
	/// manually).
	Partial(Cow<'a, str>),
	/// The field was fully initialized (e.g., using `new()` or `default()` like methods).
	Full(Cow<'a, str>),
}

impl<'a> InitializedField<'a> {
	/// Returns a name of the field.
	pub fn get_name(&'a self) -> &'a str {
		match self {
			Self::Partial(s) | Self::Full(s) => s,
		}
	}

	/// Injects a prefix to the field name.
	pub fn add_prefix(&mut self, prefix: &str) {
		match self {
			Self::Partial(s) | Self::Full(s) => *s = format!("{prefix}.{s}").into(),
		};
	}

	/// Creates new partial field instiance.
	pub fn partial(s: &'a str) -> Self {
		Self::Partial(s.into())
	}

	/// Creates new full field instiance.
	pub fn full(s: &'a str) -> Self {
		Self::Full(s.into())
	}
}

impl PartialEq<String> for InitializedField<'_> {
	fn eq(&self, other: &String) -> bool {
		#[inline]
		/// We need to respect the `camelCase` naming for field names. This means that
		/// `"camelCaseKey"` should be considered equal to `"camel_case_key"`. This
		/// function implements this comparison.
		fn compare_keys(ident_chars: core::str::Chars, camel_chars: core::str::Chars) -> bool {
			ident_chars
				.filter(|c| *c != '_')
				.map(|c| c.to_ascii_uppercase())
				.eq(camel_chars.map(|c| c.to_ascii_uppercase()))
		}
		match self {
			InitializedField::Partial(field_name) | InitializedField::Full(field_name) =>
				field_name == other || compare_keys(field_name.chars(), other.chars()),
		}
	}
}

/// Recursively removes keys from provided `json_value` object, retaining only specified keys.
///
/// This function modifies the provided `json_value` in-place, keeping only the keys listed in
/// `keys_to_retain`. The keys are matched recursively by combining the current key with
/// the `current_root`, allowing for nested field retention.
///
/// Keys marked as `Full`, are retained as-is. For keys marked as `Partial`, the
/// function recurses into nested objects to retain matching subfields.
///
/// Function respects the `camelCase` serde_json attribute for structures. This means that
/// `"camelCaseKey"` key will be retained in JSON blob if `"camel_case_key"` exists in
/// `keys_to_retain`.
///
/// Intended to be used from `build_struct_json_patch` macro.
pub fn retain_initialized_fields(
	json_value: &mut Value,
	keys_to_retain: &[InitializedField],
	current_root: String,
) {
	if let serde_json::Value::Object(ref mut map) = json_value {
		map.retain(|key, value| {
			let current_key =
				if current_root.is_empty() { key.clone() } else { format!("{current_root}.{key}") };
			match keys_to_retain.iter().find(|key| **key == current_key) {
				Some(InitializedField::Full(_)) => true,
				Some(InitializedField::Partial(_)) => {
					retain_initialized_fields(value, keys_to_retain, current_key.clone());
					true
				},
				None => false,
			}
		})
	}
}

/// Creates a JSON patch for given `struct_type`, supporting recursive field initialization.
///
/// This macro creates a default `struct_type`, initializing specified fields (which can be nested)
/// with the provided values. Any fields not explicitly given are initialized with their default
/// values. The macro then serializes the fully initialized structure into a JSON blob, retaining
/// only the fields that were explicitly provided, either partially or fully initialized.
///
/// Using this macro prevents errors from manually creating JSON objects, such as typos or
/// inconsistencies with the `struct_type` structure, by relying on the actual
/// struct definition. This ensures the generated JSON is valid and reflects any future changes
/// to the structure.
///
/// # Example
///
/// ```rust
/// use frame_support::build_struct_json_patch;
/// #[derive(Default, serde::Serialize, serde::Deserialize)]
/// #[serde(rename_all = "camelCase")]
/// struct RuntimeGenesisConfig {
///     a_field: u32,
///     b_field: B,
///     c_field: u32,
/// }
///
/// #[derive(Default, serde::Serialize, serde::Deserialize)]
/// #[serde(rename_all = "camelCase")]
/// struct B {
/// 	i_field: u32,
/// 	j_field: u32,
/// }
/// impl B {
/// 	fn new() -> Self {
/// 		Self { i_field: 0, j_field: 2 }
/// 	}
/// }
///
/// assert_eq!(
/// 	build_struct_json_patch! ( RuntimeGenesisConfig {
/// 		a_field: 66,
/// 	}),
/// 	serde_json::json!({
/// 			"aField": 66,
/// 	})
/// );
///
/// assert_eq!(
/// 	build_struct_json_patch! ( RuntimeGenesisConfig {
/// 		//"partial" initialization of `b_field`
/// 		b_field: B {
/// 			i_field: 2,
/// 		}
/// 	}),
/// 	serde_json::json!({
/// 		"bField": {"iField": 2}
/// 	})
/// );
///
/// assert_eq!(
/// 	build_struct_json_patch! ( RuntimeGenesisConfig {
/// 		a_field: 66,
/// 		//"full" initialization of `b_field`
/// 		b_field: B::new()
/// 	}),
/// 	serde_json::json!({
/// 		"aField": 66,
/// 		"bField": {"iField": 0, "jField": 2}
/// 	})
/// );
/// ```
///
/// In this example:
/// ```ignore
/// 	build_struct_json_patch! ( RuntimeGenesisConfig {
/// 		b_field: B {
/// 			i_field: 2,
/// 		}
/// 	}),
/// ```
/// `b_field` is partially initialized, it will be expanded to:
/// ```ignore
/// RuntimeGenesisConfig {
/// 		b_field {
/// 			i_field: 2,
/// 			..Default::default()
/// 		},
/// 		..Default::default()
/// }
/// ```
/// While all other fields are initialized with default values. The macro serializes this, retaining
/// only the provided fields.
#[macro_export]
macro_rules! build_struct_json_patch {
	(
		$($struct_type:ident)::+ { $($tail:tt)* }
	) => {
		{
			let mut keys = $crate::__private::Vec::<$crate::generate_genesis_config::InitializedField>::default();
			#[allow(clippy::needless_update)]
			let struct_instance = $crate::build_struct_json_patch!($($struct_type)::+, keys @  { $($tail)* });
			let mut json_value =
				$crate::__private::serde_json::to_value(struct_instance).expect("serialization to json should work. qed");
			$crate::generate_genesis_config::retain_initialized_fields(&mut json_value, &keys, Default::default());
			json_value
		}
	};
	($($struct_type:ident)::+, $all_keys:ident @ { $($tail:tt)* }) => {
		$($struct_type)::+ {
			..$crate::build_struct_json_patch!($($struct_type)::+, $all_keys @ $($tail)*)
		}
	};
	($($struct_type:ident)::+, $all_keys:ident  @  $key:ident: $($type:ident)::+ { $keyi:ident : $value:tt }  ) => {
		$($struct_type)::+ {
			$key: {
				$all_keys.push($crate::generate_genesis_config::InitializedField::partial(stringify!($key)));
				$all_keys.push(
					$crate::generate_genesis_config::InitializedField::full(concat!(stringify!($key), ".", stringify!($keyi)))
				);
				$($type)::+ {
					$keyi:$value,
					..Default::default()
				}
			},
			..Default::default()
		}
	};
	($($struct_type:ident)::+, $all_keys:ident  @  $key:ident:  $($type:ident)::+ { $($body:tt)* } ) => {
		$($struct_type)::+ {
			$key: {
				$all_keys.push($crate::generate_genesis_config::InitializedField::partial(stringify!($key)));
				let mut inner_keys = $crate::__private::Vec::<$crate::generate_genesis_config::InitializedField>::default();
				let value = $crate::build_struct_json_patch!($($type)::+, inner_keys @ { $($body)* });
				for i in inner_keys.iter_mut() {
					i.add_prefix(stringify!($key));
				};
				$all_keys.extend(inner_keys);
				value
			},
			..Default::default()
		}
	};
	($($struct_type:ident)::+, $all_keys:ident  @  $key:ident:  $($type:ident)::+ { $($body:tt)* },  $($tail:tt)*  ) => {
		$($struct_type)::+ {
			$key : {
				$all_keys.push($crate::generate_genesis_config::InitializedField::partial(stringify!($key)));
				let mut inner_keys = $crate::__private::Vec::<$crate::generate_genesis_config::InitializedField>::default();
				let value = $crate::build_struct_json_patch!($($type)::+, inner_keys @ { $($body)* });
				for i in inner_keys.iter_mut() {
					i.add_prefix(stringify!($key));
				};
				$all_keys.extend(inner_keys);
				value
			},
			.. $crate::build_struct_json_patch!($($struct_type)::+, $all_keys @ $($tail)*)
		}
	};
	($($struct_type:ident)::+, $all_keys:ident  @  $key:ident: $value:expr, $($tail:tt)* ) => {
		$($struct_type)::+ {
			$key: {
				$all_keys.push($crate::generate_genesis_config::InitializedField::full(stringify!($key)));
				$value
			},
			..$crate::build_struct_json_patch!($($struct_type)::+, $all_keys @ $($tail)*)
		}
	};
	($($struct_type:ident)::+, $all_keys:ident  @  $key:ident: $value:expr ) => {
		$($struct_type)::+ {
			$key: {
				$all_keys.push($crate::generate_genesis_config::InitializedField::full(stringify!($key)));
				$value
			},
			..Default::default()
		}
	};

	($($struct_type:ident)::+, $all_keys:ident  @ $(,)?) => {
		$($struct_type)::+ { ..Default::default() }
	};
}

#[cfg(test)]
mod test {
	mod nested_mod {
		#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
		pub struct InsideMod {
			pub a: u32,
			pub b: u32,
		}

		pub mod nested_mod2 {
			pub mod nested_mod3 {
				#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
				pub struct InsideMod3 {
					pub a: u32,
					pub b: u32,
					pub s: super::super::InsideMod,
				}
			}
		}
	}

	#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
	struct TestStruct {
		a: u32,
		b: u32,
		s: S,
		s3: S3,
		t3: S3,
		i: Nested1,
		e: E,
		t: nested_mod::InsideMod,
		u: nested_mod::nested_mod2::nested_mod3::InsideMod3,
	}

	#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
	struct S {
		x: u32,
	}

	impl S {
		fn new(c: u32) -> Self {
			Self { x: c }
		}
	}

	#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
	struct E(u8);

	#[derive(Default, Debug, serde::Serialize, serde::Deserialize)]
	enum SomeEnum<T> {
		#[default]
		A,
		B(T),
	}

	#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
	struct S3 {
		x: u32,
		y: u32,
		z: u32,
	}

	impl S3 {
		fn new(c: u32) -> Self {
			Self { x: c, y: c, z: c }
		}

		fn new_from_s(s: S) -> Self {
			Self { x: s.x, ..Default::default() }
		}
	}

	#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
	struct Nested3 {
		a: u32,
		b: u32,
		s: S,
		v: Vec<(u32, u32, u32, SomeEnum<u32>)>,
	}

	#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
	struct Nested2 {
		a: u32,
		iii: Nested3,
		v: Vec<u32>,
		s3: S3,
	}

	impl Nested2 {
		fn new(a: u32) -> Self {
			Nested2 {
				a,
				v: vec![a, a, a],
				iii: Nested3 { a, b: a, ..Default::default() },
				s3: S3 { x: a, ..Default::default() },
			}
		}
	}

	#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
	struct Nested1 {
		a: u32,
		ii: Nested2,
	}

	macro_rules! test {
		($($struct:ident)::+ { $($v:tt)* }, { $($j:tt)* } ) => {{
			println!("--");
			let expected = serde_json::json!({ $($j)* });
			println!("json: {}", serde_json::to_string_pretty(&expected).unwrap());
			let value = build_struct_json_patch!($($struct)::+ { $($v)* });
			println!("gc: {}", serde_json::to_string_pretty(&value).unwrap());
			assert_eq!(value, expected);
		}};
	}

	#[test]
	fn test_generate_config_macro() {
		let t = 5;
		const C: u32 = 5;
		test!(TestStruct { b: 5 }, { "b": 5 });
		#[allow(unused_braces)]
		{
			test!(TestStruct { b: { 4 + 34 } } , { "b": 38 });
		}
		test!(TestStruct { s: S { x: 5 } }, { "s": { "x": 5 } });
		test!(
			TestStruct { s: S::new(C) },
			{
				"s": { "x": 5 }
			}
		);
		test!(
			TestStruct { s: S { x: t } },
			{
				"s": { "x": t }
			}
		);
		test!(
			TestStruct {
				b: 5,
				s: S { x: t }
			},
			{
				"b": 5,
				"s": { "x": 5 }
			}
		);
		test!(
			TestStruct { s: S::new(C), b: 5 },
			{
				"s": { "x": 5 }, "b": 5
			}
		);
		test!(
			TestStruct { s3: S3 { x: t } },
			{
				"s3": { "x": 5 }
			}
		);
		test!(
			TestStruct {
				s3: S3 { x: t, y: 2 }
			},
			{
				"s3": { "x": 5, "y": 2 }
			}
		);
		// //
		test!(
			TestStruct {
				s3: S3 { x: t, y: 2 },
				t3: S3 { x: 2 }
			},
			{
				"s3": { "x": t, "y": 2 },
				"t3": { "x": 2 }
			}

		);
		test!(
			TestStruct {
				i: Nested1 {
					ii: Nested2 { iii: Nested3 { a: 2 } }
				}
			}
			,
			{
				"i":  {
					"ii": { "iii": { "a": 2 } }
				}
			}

		);
		test!(
			TestStruct {
				i: Nested1 {
					ii: Nested2 {
						iii: Nested3 { a: 2, s: S::new(C) }
					}
				}
			},
			{
				"i": {
					"ii": {
						"iii": { "a": 2, "s": { "x": 5} }
					}
				}
			}
		);
		test!(
			TestStruct {
				i: Nested1 {
					ii: Nested2 {
						iii: Nested3 { s: S::new(C), a: 2 }
					},
					a: 44
				},
				a: 3,
				s3: S3 { x: 5 },
				b: 4
			},
			{
				"i": {
					"ii": {
						"iii": { "a": 2, "s": { "x": 5} }
					},
					"a": 44
				},
				"a": 3,
				"s3": { "x": 5 },
				"b": 4
			}
		);
		test!(
			TestStruct {
				i: Nested1 {
					ii: Nested2::new(66),
					a: 44,
				},
				a: 3,
				s3: S3 { x: 5 },
				b: 4
			},
			{
				"i": {
					"ii": {
						"a": 66,
						"s3": { "x":66, "y": 0, "z": 0 },
						"iii": { "a": 66,"b":66, "s": { "x": 0 }, "v": Vec::<u32>::default() },
						"v": vec![66,66,66]
					},
					"a": 44
				},
				"a": 3,
				"s3": { "x": 5 },
				"b": 4
			}
		);

		test!(
			TestStruct {
				i: Nested1 {
					ii: Nested2 {
						a: 66,
						s3: S3 { x: 66 },
						iii: Nested3 {
							a: 66,b:66
						},
						v: vec![66,66,66]
					},
					a: 44,
				},
				a: 3,
				s3: S3 { x: 5 },
				b: 4
			},
			{
				"i": {
					"ii": {
						"a": 66,
						"s3": { "x":66,  },
						"iii": { "a": 66,"b":66, },
						"v": vec![66,66,66]
					},
					"a": 44
				},
				"a": 3,
				"s3": { "x": 5 },
				"b": 4
			}
		);

		test!(
			TestStruct {
				i: Nested1 {
					ii: Nested2 {
						iii: Nested3 { a: 2, s: S::new(C) },
					},
					a: 44,
				},
				a: 3,
				s3: S3 { x: 5 },
				b: 4,
			},
			{
				"i": {
					"ii": {
						"iii": { "a": 2, "s": { "x": 5 } },
					},
					"a" : 44,
				},
				"a": 3,
				"s3": { "x": 5 },
				"b": 4
			}
		);
		test!(
			TestStruct {
				i: Nested1 {
					ii: Nested2 {
						s3: S3::new(5),
						iii: Nested3 { a: 2, s: S::new(C) },
					},
					a: 44,
				},
				a: 3,
				s3: S3 { x: 5 },
				b: 4,
			},
			{
				"i": {
					"ii": {
						"iii": { "a": 2, "s": { "x": 5 } },
						"s3": {"x": 5, "y": 5, "z": 5 }
					},
					"a" : 44,
				},
				"a": 3,
				"s3": { "x": 5 },
				"b": 4
			}
		);
		test!(
			TestStruct {
				a: 3,
				s3: S3 { x: 5 },
				b: 4,
				i: Nested1 {
					ii: Nested2 {
						iii: Nested3 { a: 2, s: S::new(C) },
						s3: S3::new_from_s(S { x: 4 })
					},
					a: 44,
				}
			},
			{
				"i": {
					"ii": {
						"iii": { "a": 2, "s": { "x": 5 } },
						"s3": {"x": 4, "y": 0, "z": 0 }
					},
					"a" : 44,
				},
				"a": 3,
				"s3": { "x": 5 },
				"b": 4
			}
		);
		let i = [0u32, 1u32, 2u32];
		test!(
			TestStruct {
				i: Nested1 {
					ii: Nested2 {
						iii: Nested3 {
							a: 2,
							s: S::new(C),
							v: i.iter()
								.map(|x| (*x, 2 * x, 100 + x, SomeEnum::<u32>::A))
								.collect::<Vec<_>>(),
						},
						s3: S3::new_from_s(S { x: 4 })
					},
					a: 44,
				},
				a: 3,
				s3: S3 { x: 5 },
				b: 4,
			},

			{
				"i": {
					"ii": {
						"iii": {
							"a": 2,
							"s": { "x": 5 },
							"v": i.iter()
								.map(|x| (*x, 2 * x, 100 + x, SomeEnum::<u32>::A))
								.collect::<Vec<_>>(),
						},
						"s3": {"x": 4, "y": 0, "z": 0 }
					},
					"a" : 44,
				},
				"a": 3,
				"s3": { "x": 5 },
				"b": 4
			}
		);
	}

	#[test]
	fn test_generate_config_macro_with_nested_mods() {
		test!(
			TestStruct { t: nested_mod::InsideMod { a: 32 } },
			{
				"t" : { "a": 32 }
			}
		);
		test!(
			TestStruct {
				t: nested_mod::InsideMod { a: 32 },
				u: nested_mod::nested_mod2::nested_mod3::InsideMod3 { a: 32 }
			},
			{
				"t" : { "a": 32 },
				"u" : { "a": 32 }
			}
		);
		test!(
			TestStruct {
				t: nested_mod::InsideMod { a: 32 },
				u: nested_mod::nested_mod2::nested_mod3::InsideMod3 {
					a: 32,
					s: nested_mod::InsideMod { a: 34 },
				}
			},
			{
				"t" : { "a": 32 },
				"u" : { "a": 32, "s": { "a": 34 } }
			}
		);
		test!(
			TestStruct {
				t: nested_mod::InsideMod { a: 32 },
				u: nested_mod::nested_mod2::nested_mod3::InsideMod3::default()
			},
			{
				"t" : { "a": 32 },
				"u" : { "a": 0, "b": 0,  "s": { "a": 0, "b": 0} }
			}
		);

		let i = [0u32, 1u32, 2u32];
		const C: u32 = 5;
		test!(
			TestStruct {
				t: nested_mod::InsideMod { a: 32 },
				u: nested_mod::nested_mod2::nested_mod3::InsideMod3::default(),
				i: Nested1 {
					ii: Nested2 {
						iii: Nested3 {
							a: 2,
							s: S::new(C),
							v: i.iter()
								.map(|x| (*x, 2 * x, 100 + x, SomeEnum::<u32>::A))
								.collect::<Vec<_>>(),
						},
						s3: S3::new_from_s(S { x: 4 })
					},
					a: 44,
				},
			},
			{
				"t" : { "a": 32 },
				"u" : { "a": 0, "b": 0,  "s": { "a": 0, "b": 0} } ,
				"i": {
					"ii": {
						"iii": {
							"a": 2,
							"s": { "x": 5 },
							"v": i.iter()
								.map(|x| (*x, 2 * x, 100 + x, SomeEnum::<u32>::A))
								.collect::<Vec<_>>(),
						},
						"s3": {"x": 4, "y": 0, "z": 0 }
					},
					"a" : 44,
				},
			}
		);
	}
}

#[cfg(test)]
mod retain_keys_test {
	use super::*;
	use serde_json::json;

	macro_rules! check_initialized_field_eq_cc(
		( $s:literal ) => {
			let field = InitializedField::full($s);
			let cc = inflector::cases::camelcase::to_camel_case($s);
			println!("field: {:?}, cc: {}", field, cc);
			assert_eq!(field,cc);
		} ;
		( &[ $f:literal $(, $r:literal)* ]) => {
			let field = InitializedField::full(
				concat!( $f $(,".",$r)+ )
			);
			let cc = [ $f $(,$r)+  ].into_iter()
				.map(|s| inflector::cases::camelcase::to_camel_case(s))
				.collect::<Vec<_>>()
				.join(".");
			println!("field: {:?}, cc: {}", field, cc);
			assert_eq!(field,cc);
		} ;
	);

	#[test]
	fn test_initialized_field_eq_cc_string() {
		check_initialized_field_eq_cc!("a_");
		check_initialized_field_eq_cc!("abc");
		check_initialized_field_eq_cc!("aBc");
		check_initialized_field_eq_cc!("aBC");
		check_initialized_field_eq_cc!("ABC");
		check_initialized_field_eq_cc!("2abs");
		check_initialized_field_eq_cc!("2Abs");
		check_initialized_field_eq_cc!("2ABs");
		check_initialized_field_eq_cc!("2aBs");
		check_initialized_field_eq_cc!("AlreadyCamelCase");
		check_initialized_field_eq_cc!("alreadyCamelCase");
		check_initialized_field_eq_cc!("C");
		check_initialized_field_eq_cc!("1a");
		check_initialized_field_eq_cc!("_1a");
		check_initialized_field_eq_cc!("a_b");
		check_initialized_field_eq_cc!("_a_b");
		check_initialized_field_eq_cc!("a___b");
		check_initialized_field_eq_cc!("__a_b");
		check_initialized_field_eq_cc!("_a___b_C");
		check_initialized_field_eq_cc!("__A___B_C");
		check_initialized_field_eq_cc!(&["a_b", "b_c"]);
		check_initialized_field_eq_cc!(&["al_pha", "_a___b_C"]);
		check_initialized_field_eq_cc!(&["al_pha_", "_a___b_C"]);
		check_initialized_field_eq_cc!(&["first_field", "al_pha_", "_a___b_C"]);
		check_initialized_field_eq_cc!(&["al_pha_", "__2nd_field", "_a___b_C"]);
		check_initialized_field_eq_cc!(&["al_pha_", "__2nd3and_field", "_a___b_C"]);
		check_initialized_field_eq_cc!(&["_a1", "_a2", "_a3_"]);
	}

	#[test]
	fn test01() {
		let mut v = json!({
			"a":1
		});
		let e = v.clone();
		retain_initialized_fields(&mut v, &[InitializedField::full("a")], String::default());
		assert_eq!(e, v);
	}

	#[test]
	fn test02() {
		let mut v = json!({
			"a":1
		});
		retain_initialized_fields(&mut v, &[InitializedField::full("b")], String::default());
		assert_eq!(Value::Object(Default::default()), v);
	}

	#[test]
	fn test03() {
		let mut v = json!({});
		retain_initialized_fields(&mut v, &[], String::default());
		assert_eq!(Value::Object(Default::default()), v);
	}

	#[test]
	fn test04() {
		let mut v = json!({});
		retain_initialized_fields(&mut v, &[InitializedField::full("b")], String::default());
		assert_eq!(Value::Object(Default::default()), v);
	}

	#[test]
	fn test05() {
		let mut v = json!({
			"a":1
		});
		retain_initialized_fields(&mut v, &[], String::default());
		assert_eq!(Value::Object(Default::default()), v);
	}

	#[test]
	fn test06() {
		let mut v = json!({
			"a": {
				"b":1,
				"c":2
			}
		});
		retain_initialized_fields(&mut v, &[], String::default());
		assert_eq!(Value::Object(Default::default()), v);
	}

	#[test]
	fn test07() {
		let mut v = json!({
			"a": {
				"b":1,
				"c":2
			}
		});
		retain_initialized_fields(&mut v, &[InitializedField::full("a.b")], String::default());
		assert_eq!(Value::Object(Default::default()), v);
	}

	#[test]
	fn test08() {
		let mut v = json!({
			"a": {
				"b":1,
				"c":2
			}
		});
		let e = json!({
			"a": {
				"b":1,
			}
		});
		retain_initialized_fields(
			&mut v,
			&[InitializedField::partial("a"), InitializedField::full("a.b")],
			String::default(),
		);
		assert_eq!(e, v);
	}

	#[test]
	fn test09() {
		let mut v = json!({
			"a": {
				"b":1,
				"c":2
			}
		});
		let e = json!({
			"a": {
				"b":1,
				"c":2,
			}
		});
		retain_initialized_fields(&mut v, &[InitializedField::full("a")], String::default());
		assert_eq!(e, v);
	}
}
