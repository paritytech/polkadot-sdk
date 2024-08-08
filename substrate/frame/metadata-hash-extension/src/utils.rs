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

/// Function to convert hex string to Option<[u8; 32]> in a const context
/// panics if the input is not valid.
pub const fn hex_str_to_32_bytes_panic(hex_str: &str, msg: &str) -> [u8; 32] {
	let len = hex_str.len();

	let start = if len == 64 {
		0
	} else if len == 66 && hex_str.as_bytes()[0] == b'0' && hex_str.as_bytes()[1] == b'x' {
		2
	} else {
		panic!("{}", msg)
	};

	let mut bytes = [0u8; 32];
	let mut i = 0;

	while i < 32 {
		let high = from_hex_digit_panic(hex_str.as_bytes()[start + i * 2], msg);
		let low = from_hex_digit_panic(hex_str.as_bytes()[start + i * 2 + 1], msg);
		bytes[i] = (high << 4) | low;
		i += 1;
	}

	bytes
}

// Helper function to convert a single hex character to a byte
const fn from_hex_digit_panic(digit: u8, msg: &str) -> u8 {
	match digit {
		b'0'..=b'9' => digit - b'0',
		b'a'..=b'f' => digit - b'a' + 10,
		b'A'..=b'F' => digit - b'A' + 10,
		_ => panic!("{}", msg),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_from_hex_digit_panic() {
		assert_eq!(from_hex_digit_panic(b'0', ""), 0);
		assert_eq!(from_hex_digit_panic(b'1', ""), 1);
		assert_eq!(from_hex_digit_panic(b'2', ""), 2);
		assert_eq!(from_hex_digit_panic(b'3', ""), 3);
		assert_eq!(from_hex_digit_panic(b'4', ""), 4);
		assert_eq!(from_hex_digit_panic(b'5', ""), 5);
		assert_eq!(from_hex_digit_panic(b'6', ""), 6);
		assert_eq!(from_hex_digit_panic(b'7', ""), 7);
		assert_eq!(from_hex_digit_panic(b'8', ""), 8);
		assert_eq!(from_hex_digit_panic(b'9', ""), 9);
		assert_eq!(from_hex_digit_panic(b'a', ""), 10);
		assert_eq!(from_hex_digit_panic(b'b', ""), 11);
		assert_eq!(from_hex_digit_panic(b'c', ""), 12);
		assert_eq!(from_hex_digit_panic(b'd', ""), 13);
		assert_eq!(from_hex_digit_panic(b'e', ""), 14);
		assert_eq!(from_hex_digit_panic(b'f', ""), 15);
		assert_eq!(from_hex_digit_panic(b'A', ""), 10);
		assert_eq!(from_hex_digit_panic(b'B', ""), 11);
		assert_eq!(from_hex_digit_panic(b'C', ""), 12);
		assert_eq!(from_hex_digit_panic(b'D', ""), 13);
		assert_eq!(from_hex_digit_panic(b'E', ""), 14);
		assert_eq!(from_hex_digit_panic(b'F', ""), 15);
	}

	#[test]
	#[should_panic(expected = "TEST1")]
	fn test_from_hex_digit_panic_should_panic() {
		from_hex_digit_panic(b'g', "TEST1");
	}

	#[test]
	fn test_hex_str_to_32_bytes_panic() {
		assert_eq!(
			hex_str_to_32_bytes_panic(
				"0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
				""
			),
			[
				0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab,
				0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78,
				0x90, 0xab, 0xcd, 0xef,
			]
		);
		assert_eq!(
			hex_str_to_32_bytes_panic(
				"1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
				""
			),
			[
				0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab,
				0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78,
				0x90, 0xab, 0xcd, 0xef,
			]
		);
		assert_eq!(
			hex_str_to_32_bytes_panic(
				"0x1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF",
				""
			),
			[
				0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab,
				0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78,
				0x90, 0xab, 0xcd, 0xef,
			]
		);
		assert_eq!(
			hex_str_to_32_bytes_panic(
				"1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF",
				""
			),
			[
				0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab,
				0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78,
				0x90, 0xab, 0xcd, 0xef,
			]
		);
	}

	#[test]
	#[should_panic(expected = "TEST")]
	fn test_hex_str_to_32_bytes_panic_should_panic() {
		hex_str_to_32_bytes_panic("0x1234", "TEST");
	}
}
