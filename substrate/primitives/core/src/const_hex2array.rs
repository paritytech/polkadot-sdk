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

//! Provides a const function for converting a hex string to a `u8` array at compile time, when used
//! in the proper context.

/// Provides a const array from given string literal.
///
/// Valid characters are `[0-9a-fA-F]`, and the hex string should not start
/// with the `0x` prefix.
#[macro_export]
macro_rules! hex2array {
	($input:expr) => {{
		const BYTES: [u8; $input.len() / 2] = $crate::const_hex2array::private_hex2array($input);
		BYTES
	}};
}

/// Generates array from (static) string literal.
///
/// Valid characters are `[0-9a-fA-F]`, and the hex string should not start
/// with the `0x` prefix.
///
/// # Panics
///
/// The function will panic at compile time when used in a const context if:
/// - The given hex string has an invalid length.
/// - It contains invalid characters.
///
/// The function will panic at runtime when used in a non-const context if the above conditions are
/// met.
#[doc(hidden)]
pub const fn private_hex2array<const N: usize>(hex: &str) -> [u8; N] {
	const fn c2b(c: u8) -> u8 {
		match c as char {
			'0'..='9' => c - b'0',
			'a'..='f' => c - (b'a' - 10),
			'A'..='F' => c - (b'A' - 10),
			_ => panic!("hex string contains invalid character"),
		}
	}
	let mut output = [0; N];
	let mut i = 0;
	if hex.len() != 2 * N {
		panic!("hex string length is not valid");
	}
	while i < N {
		output[i] = 16 * c2b(hex.as_bytes()[2 * i]) + c2b(hex.as_bytes()[2 * i + 1]);
		i += 1;
	}
	output
}

#[cfg(test)]
mod testh2b {
	use super::private_hex2array;

	#[test]
	fn t00() {
		const T0: [u8; 0] = private_hex2array("");
		const EMPTY: [u8; 0] = [];
		assert_eq!(T0, EMPTY);
	}

	macro_rules! test_byte {
		($a:expr, $b:expr) => {{
			const X: [u8; 1] = private_hex2array($a);
			assert_eq!(X, [$b]);
		}};
	}

	#[test]
	fn t01() {
		test_byte!("00", 0);
		test_byte!("01", 1);
		test_byte!("02", 2);
		test_byte!("03", 3);
		test_byte!("04", 4);
		test_byte!("05", 5);
		test_byte!("06", 6);
		test_byte!("07", 7);
		test_byte!("08", 8);
		test_byte!("09", 9);
		test_byte!("0a", 10);
		test_byte!("0A", 10);
		test_byte!("0b", 11);
		test_byte!("0B", 11);
		test_byte!("0c", 12);
		test_byte!("0C", 12);
		test_byte!("0d", 13);
		test_byte!("0D", 13);
		test_byte!("0e", 14);
		test_byte!("0E", 14);
		test_byte!("0f", 15);
		test_byte!("0F", 15);
	}

	#[test]
	fn t02() {
		const T0: [u8; 2] = private_hex2array("0a10");
		assert_eq!(T0, [10, 16]);
		const T1: [u8; 2] = private_hex2array("4545");
		assert_eq!(T1, [69, 69]);
	}

	#[test]
	fn t02m() {
		assert_eq!(hex2array!("0a10"), [10, 16]);
		assert_eq!(hex2array!("4545"), [69, 69]);
		assert_eq!(
			hex2array!("000102030405060708090a0b0c0d0e0f"),
			[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
		);
	}

	#[test]
	fn t16() {
		const T16: [u8; 16] = private_hex2array("000102030405060708090a0b0c0d0e0f");

		assert_eq!(T16, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
	}

	#[test]
	fn t33() {
		const T33: [u8; 33] =
			private_hex2array("9c8af77d3a4e3f6f076853922985b9e6724fc9675329087f47aff1ceaaae772180");

		assert_eq!(
			T33,
			[
				156, 138, 247, 125, 58, 78, 63, 111, 7, 104, 83, 146, 41, 133, 185, 230, 114, 79,
				201, 103, 83, 41, 8, 127, 71, 175, 241, 206, 170, 174, 119, 33, 128
			]
		);
	}

	#[test]
	#[should_panic = "hex string length is not valid"]
	fn t_panic_incorrect_length2() {
		let _ = private_hex2array::<2>("454");
	}

	#[test]
	#[should_panic = "hex string contains invalid character"]
	fn t_panic_invalid_character() {
		let _ = private_hex2array::<2>("45ag");
	}
}
