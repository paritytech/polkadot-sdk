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

use alloc::vec::Vec;
use core::marker::PhantomData;
use hash_db::Hasher;
use sp_trie::{NodeCodec, TrieConfiguration, TrieLayout};
use trie_root::{self, Value as TrieStreamValue};

/// Backported from
/// https://github.com/rust-ethereum/ethereum/blob/cf3076f07e61102eec686f6816da668f97d94f1f/src/util.rs#L26
#[derive(Default)]
pub struct Hash256RlpTrieStream {
	stream: rlp::RlpStream,
}

impl trie_root::TrieStream for Hash256RlpTrieStream {
	fn new() -> Self {
		Self { stream: rlp::RlpStream::new() }
	}

	fn append_empty_data(&mut self) {
		self.stream.append_empty_data();
	}

	fn begin_branch(
		&mut self,
		_maybe_key: Option<&[u8]>,
		_maybe_value: Option<TrieStreamValue>,
		_has_children: impl Iterator<Item = bool>,
	) {
		// an item for every possible nibble/suffix
		// + 1 for data
		self.stream.begin_list(17);
	}

	fn append_empty_child(&mut self) {
		self.stream.append_empty_data();
	}

	fn end_branch(&mut self, value: Option<TrieStreamValue>) {
		match value {
			Some(value) => match value {
				TrieStreamValue::Inline(value) => self.stream.append(&value),
				TrieStreamValue::Node(value) => self.stream.append(&value),
			},
			None => self.stream.append_empty_data(),
		};
	}

	fn append_leaf(&mut self, key: &[u8], value: TrieStreamValue) {
		self.stream.begin_list(2);
		self.stream.append_iter(hex_prefix_encode(key, true));
		match value {
			TrieStreamValue::Inline(value) => self.stream.append(&value),
			TrieStreamValue::Node(value) => self.stream.append(&value),
		};
	}

	fn append_extension(&mut self, key: &[u8]) {
		self.stream.begin_list(2);
		self.stream.append_iter(hex_prefix_encode(key, false));
	}

	fn append_substream<H: Hasher>(&mut self, other: Self) {
		let out = other.out();
		match out.len() {
			0..=31 => self.stream.append_raw(&out, 1),
			_ => self.stream.append(&H::hash(&out).as_ref()),
		};
	}

	fn out(self) -> Vec<u8> {
		self.stream.out().freeze().into()
	}
}

fn hex_prefix_encode(nibbles: &[u8], leaf: bool) -> impl Iterator<Item = u8> + '_ {
	let inlen = nibbles.len();
	let oddness_factor = inlen % 2;

	let first_byte = {
		let mut bits = ((inlen as u8 & 1) + (2 * leaf as u8)) << 4;
		if oddness_factor == 1 {
			bits += nibbles[0];
		}
		bits
	};
	core::iter::once(first_byte)
		.chain(nibbles[oddness_factor..].chunks(2).map(|ch| ch[0] << 4 | ch[1]))
}

/// Ethereum trie layout
pub struct EthTrieLayout<H>(PhantomData<H>);

impl<H> TrieLayout for EthTrieLayout<H>
where
	H: Hasher,
{
	const USE_EXTENSION: bool = false;
	const ALLOW_EMPTY: bool = true;
	const MAX_INLINE_VALUE: Option<u32> = None;

	type Hash = H;
	type Codec = NodeCodec<Self::Hash>;
}

impl<H> TrieConfiguration for EthTrieLayout<H>
where
	H: Hasher,
{
	fn trie_root<I, A, B>(input: I) -> <Self::Hash as Hasher>::Out
	where
		I: IntoIterator<Item = (A, B)>,
		A: AsRef<[u8]> + Ord,
		B: AsRef<[u8]>,
	{
		trie_root::trie_root::<H, Hash256RlpTrieStream, _, _, _>(input, Self::MAX_INLINE_VALUE)
	}

	fn encode_index(input: u32) -> Vec<u8> {
		// be for byte ordering
		rlp::encode(&input).to_vec()
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use sp_core::H256;
	use trie_root::TrieStream;

	#[test]
	fn test_hex_prefix_encode_even_length_leaf() {
		let nibbles = &[0x1, 0x2, 0x3, 0x4];
		let result: Vec<u8> = hex_prefix_encode(nibbles, true).collect();
		// Even length leaf: first byte = 0x20 (0010 0000), then pairs
		assert_eq!(result, vec![0x20, 0x12, 0x34]);
	}

	#[test]
	fn test_hex_prefix_encode_odd_length_leaf() {
		let nibbles = &[0x1, 0x2, 0x3];
		let result: Vec<u8> = hex_prefix_encode(nibbles, true).collect();
		// Odd length leaf: first byte = 0x31 (0011 0001), then pairs
		assert_eq!(result, vec![0x31, 0x23]);
	}

	#[test]
	fn test_hex_prefix_encode_even_length_extension() {
		let nibbles = &[0x1, 0x2, 0x3, 0x4];
		let result: Vec<u8> = hex_prefix_encode(nibbles, false).collect();
		// Even length extension: first byte = 0x00 (0000 0000), then pairs
		assert_eq!(result, vec![0x00, 0x12, 0x34]);
	}

	#[test]
	fn test_hex_prefix_encode_odd_length_extension() {
		let nibbles = &[0x1, 0x2, 0x3];
		let result: Vec<u8> = hex_prefix_encode(nibbles, false).collect();
		// Odd length extension: first byte = 0x11 (0001 0001), then pairs
		assert_eq!(result, vec![0x11, 0x23]);
	}

	#[test]
	fn test_hex_prefix_encode_empty() {
		let nibbles = &[];
		let result: Vec<u8> = hex_prefix_encode(nibbles, true).collect();
		// Empty leaf: first byte = 0x20 (0010 0000)
		assert_eq!(result, vec![0x20]);
	}

	#[test]
	fn test_hex_prefix_encode_single_nibble() {
		let nibbles = &[0xa];
		let result: Vec<u8> = hex_prefix_encode(nibbles, false).collect();
		// Single nibble extension: first byte = 0x1a (0001 1010)
		assert_eq!(result, vec![0x1a]);
	}

	#[test]
	fn test_hash256_rlp_trie_stream_new() {
		let stream = Hash256RlpTrieStream::new();
		assert_eq!(stream.out(), Vec::<u8>::new());
	}

	#[test]
	fn test_hash256_rlp_trie_stream_append_empty_data() {
		let mut stream = Hash256RlpTrieStream::new();
		stream.append_empty_data();
		assert_eq!(stream.out(), vec![0x80]); // RLP encoding of empty data
	}

	#[test]
	fn test_hash256_rlp_trie_stream_append_leaf() {
		let mut stream = Hash256RlpTrieStream::new();
		let key = &[0x1, 0x2];
		let value = TrieStreamValue::Inline(b"test");
		stream.append_leaf(key, value);

		let result = stream.out();
		// Should be RLP list with 2 items: hex-prefix encoded key and value
		assert!(result.len() > 0);
		assert_eq!(result[0], 0xc0 + result.len() as u8 - 1); // RLP list marker
	}

	#[test]
	fn test_hash256_rlp_trie_stream_begin_end_branch() {
		let mut stream = Hash256RlpTrieStream::new();
		stream.begin_branch(None, None, [false; 16].iter().copied());

		// Add 16 empty children
		for _ in 0..16 {
			stream.append_empty_child();
		}

		stream.end_branch(Some(TrieStreamValue::Inline(b"value")));

		let result = stream.out();
		// Should be RLP list with 17 items (16 children + 1 value)
		assert!(result.len() > 0);
	}

	#[test]
	fn test_eth_trie_layout_encode_index() {
		let encoded = EthTrieLayout::<sp_core::KeccakHasher>::encode_index(0);
		assert_eq!(encoded, vec![0x80]); // RLP encoding of 0

		let encoded = EthTrieLayout::<sp_core::KeccakHasher>::encode_index(1);
		assert_eq!(encoded, vec![0x01]); // RLP encoding of 1

		let encoded = EthTrieLayout::<sp_core::KeccakHasher>::encode_index(255);
		assert_eq!(encoded, vec![0x81, 0xff]); // RLP encoding of 255
	}

	#[test]
	fn test_trie_root_empty() {
		let empty_input: Vec<(Vec<u8>, Vec<u8>)> = vec![];
		let root = EthTrieLayout::<sp_core::KeccakHasher>::trie_root(empty_input.clone());
		let root_ethereum = ethereum::util::trie_root(empty_input);

		// Empty trie should have a specific root hash
		assert_eq!(
			root,
			H256(hex_literal::hex!(
				"56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
			))
		);
		assert_eq!(root, root_ethereum);
	}

	// Below tests should assure compatibility with Ethereum trie.
	// As a reference we use `ethereum` crate
	#[test]
	fn test_trie_root_single_item() {
		let input = vec![(b"key".to_vec(), b"value".to_vec())];
		let root = EthTrieLayout::<sp_core::KeccakHasher>::trie_root(input.clone());
		let root_ethereum = ethereum::util::trie_root(input);

		assert_eq!(
			root,
			H256(hex_literal::hex!(
				"98021eec76a352d4214ee9d22f2670f3abe01d5805441249f4b70dda75a0e07a"
			))
		);
		assert_eq!(root, root_ethereum);
	}

	#[test]
	fn test_trie_root_multiple_items() {
		let input = vec![
			(b"key1".to_vec(), b"value1".to_vec()),
			(b"key2".to_vec(), b"value2".to_vec()),
			(b"key3".to_vec(), b"value3".to_vec()),
		];
		let root = EthTrieLayout::<sp_core::KeccakHasher>::trie_root(input.clone());
		let root_ethereum = ethereum::util::trie_root(input);

		assert_eq!(
			root,
			H256(hex_literal::hex!(
				"7ed101460e293510184889c18501b03f553342c13d50235290fb707360c46ef5"
			))
		);
		assert_eq!(root, root_ethereum);
	}

	#[test]
	fn test_ordered_trie_root_deterministic() {
		let input1 = vec![b"value1".to_vec(), b"value2".to_vec(), b"value3".to_vec()];
		let input2 = vec![b"value1".to_vec(), b"value2".to_vec(), b"value3".to_vec()];

		let root1 = EthTrieLayout::<sp_core::KeccakHasher>::ordered_trie_root(input1.iter());
		let root2 = EthTrieLayout::<sp_core::KeccakHasher>::ordered_trie_root(input2.iter());
		let root1_ethereum = ethereum::util::ordered_trie_root(input1);
		let root2_ethereum = ethereum::util::ordered_trie_root(input2);

		assert_eq!(
			root1,
			H256(hex_literal::hex!(
				"8f0bd34adc1414631673dac4e396ec419d5d7884267c4ebaf22e219286c3b1b5"
			))
		);
		assert_eq!(root1, root2);
		assert_eq!(root1_ethereum, root2_ethereum);
		assert_eq!(root1, root1_ethereum);
	}

	#[test]
	fn test_trie_layout_constants() {
		assert_eq!(EthTrieLayout::<sp_core::KeccakHasher>::USE_EXTENSION, false);
		assert_eq!(EthTrieLayout::<sp_core::KeccakHasher>::ALLOW_EMPTY, true);
		assert_eq!(EthTrieLayout::<sp_core::KeccakHasher>::MAX_INLINE_VALUE, None);
	}
}
