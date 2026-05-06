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

use pallet_revive_proc_macro::define_versioned_type;

define_versioned_type! {
	pub struct MacroStructV1 {
		pub first: u8,
	}

	#[versioned_type(extend)]
	pub struct MacroStructV2 {
		pub second: u16,
	}
}

define_versioned_type! {
	pub enum MacroEnumV1 {
		First {
			value: u8,
		},
	}

	#[versioned_type(extend)]
	pub enum MacroEnumV2 {
		Second {
			other: u16,
		},
	}
}

define_versioned_type! {
	pub struct MacroStructOverrideV1 {
		pub first: u8,
		pub second: u16,
	}

	#[versioned_type(extend)]
	pub struct MacroStructOverrideV2 {
		#[versioned_type(override)]
		pub second: u32,
		pub third: u64,
	}
}

define_versioned_type! {
	pub enum MacroEnumOverrideV1 {
		First {
			first: u8,
			second: u16,
		},
		Second {
			other: u32,
		},
	}

	#[versioned_type(extend)]
	pub enum MacroEnumOverrideV2 {
		#[versioned_type(override, extend)]
		First {
			#[versioned_type(override)]
			second: u32,
			third: u64,
		},
	}
}

#[test]
fn function_like_macro_expands_struct_extensions() {
	// Arrange
	let value = MacroStructV2 { first: 1, second: 2 };

	// Act
	let observed = (value.first, value.second);

	// Assert
	assert_eq!(observed, (1, 2));
}

#[test]
fn function_like_macro_expands_struct_field_overrides() {
	// Arrange
	let value = MacroStructOverrideV2 { first: 1, second: 2, third: 3 };

	// Act
	let observed = (value.first, value.second, value.third);

	// Assert
	assert_eq!(observed, (1, 2, 3));
}

#[test]
fn function_like_macro_expands_enum_variant_and_field_overrides() {
	// Arrange
	let overridden = MacroEnumOverrideV2::First { first: 1, second: 2, third: 3 };
	let inherited = MacroEnumOverrideV2::Second { other: 4 };

	// Act
	let observed = match (overridden, inherited) {
		(
			MacroEnumOverrideV2::First { first, second, third },
			MacroEnumOverrideV2::Second { other },
		) => (first, second, third, other),
		_ => panic!("expected overridden and inherited variants"),
	};

	// Assert
	assert_eq!(observed, (1, 2, 3, 4));
}

#[test]
fn function_like_macro_expands_enum_extensions() {
	// Arrange
	let inherited = MacroEnumV2::First { value: 1 };
	let added = MacroEnumV2::Second { other: 2 };

	// Act
	let observed = match (inherited, added) {
		(MacroEnumV2::First { value }, MacroEnumV2::Second { other }) => (value, other),
		_ => panic!("expected inherited and added variants"),
	};

	// Assert
	assert_eq!(observed, (1, 2));
}
