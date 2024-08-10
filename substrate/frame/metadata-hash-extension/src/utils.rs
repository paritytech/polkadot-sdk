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

/// Function to convert hex string to Option<[u8; 32]> in a const context.
/// Returns `None` if fails to decode.
pub const fn hex_str_to_32_bytes(hex_str: &str) -> Option<[u8; 32]> {
	let len = hex_str.len();

	let start = if len == 64 {
		0
	} else if len == 66 && hex_str.as_bytes()[0] == b'0' && hex_str.as_bytes()[1] == b'x' {
		2
	} else {
		return None;
	};

	let mut bytes = [0u8; 32];
	let mut i = 0;

	while i < 32 {
		let high = from_hex_digit(hex_str.as_bytes()[start + i * 2]);
		let low = from_hex_digit(hex_str.as_bytes()[start + i * 2 + 1]);

		let (Some(high), Some(low)) = (high, low) else { return None };

		bytes[i] = (high << 4) | low;
		i += 1;
	}

	Some(bytes)
}

// Helper function to convert a single hex character to a byte
const fn from_hex_digit(digit: u8) -> Option<u8> {
	match digit {
		b'0'..=b'9' => Some(digit - b'0'),
		b'a'..=b'f' => Some(digit - b'a' + 10),
		b'A'..=b'F' => Some(digit - b'A' + 10),
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_from_hex_digit() {
		assert_eq!(from_hex_digit(b'0').unwrap(), 0);
		assert_eq!(from_hex_digit(b'1').unwrap(), 1);
		assert_eq!(from_hex_digit(b'2').unwrap(), 2);
		assert_eq!(from_hex_digit(b'3').unwrap(), 3);
		assert_eq!(from_hex_digit(b'4').unwrap(), 4);
		assert_eq!(from_hex_digit(b'5').unwrap(), 5);
		assert_eq!(from_hex_digit(b'6').unwrap(), 6);
		assert_eq!(from_hex_digit(b'7').unwrap(), 7);
		assert_eq!(from_hex_digit(b'8').unwrap(), 8);
		assert_eq!(from_hex_digit(b'9').unwrap(), 9);
		assert_eq!(from_hex_digit(b'a').unwrap(), 10);
		assert_eq!(from_hex_digit(b'b').unwrap(), 11);
		assert_eq!(from_hex_digit(b'c').unwrap(), 12);
		assert_eq!(from_hex_digit(b'd').unwrap(), 13);
		assert_eq!(from_hex_digit(b'e').unwrap(), 14);
		assert_eq!(from_hex_digit(b'f').unwrap(), 15);
		assert_eq!(from_hex_digit(b'A').unwrap(), 10);
		assert_eq!(from_hex_digit(b'B').unwrap(), 11);
		assert_eq!(from_hex_digit(b'C').unwrap(), 12);
		assert_eq!(from_hex_digit(b'D').unwrap(), 13);
		assert_eq!(from_hex_digit(b'E').unwrap(), 14);
		assert_eq!(from_hex_digit(b'F').unwrap(), 15);

		assert_eq!(from_hex_digit(b'g'), None);
	}

	#[test]
	fn test_hex_str_to_32_bytes() {
		assert_eq!(
			hex_str_to_32_bytes(
				"0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
			)
			.unwrap(),
			[
				0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab,
				0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78,
				0x90, 0xab, 0xcd, 0xef,
			]
		);
		assert_eq!(
			hex_str_to_32_bytes(
				"1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
			)
			.unwrap(),
			[
				0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab,
				0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78,
				0x90, 0xab, 0xcd, 0xef,
			]
		);
		assert_eq!(
			hex_str_to_32_bytes(
				"0x1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF",
			)
			.unwrap(),
			[
				0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab,
				0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78,
				0x90, 0xab, 0xcd, 0xef,
			]
		);
		assert_eq!(
			hex_str_to_32_bytes(
				"1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF",
			)
			.unwrap(),
			[
				0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab,
				0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78,
				0x90, 0xab, 0xcd, 0xef,
			]
		);

		assert_eq!(hex_str_to_32_bytes("0x1234"), None);
	}
}
