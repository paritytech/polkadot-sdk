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

use core::marker::PhantomData;
use hash_db::Hasher;
use sp_trie::{NodeCodec, TrieConfiguration, TrieLayout};
use trie_root::{self, Value as TrieStreamValue};

// Backported from
// https://github.com/rust-ethereum/ethereum/blob/cf3076f07e61102eec686f6816da668f97d94f1f/src/util.rs#L26
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

	#[test]
	fn trie_root_is_ethereum_compatible() {
		let test_data = [
            (
                "1f64201e2af914014247a6f033f44ae4a36bdb49031a020e1a83d431fccf1dd4",
                "f86c80843b9aca0082520894ff64d3f6efe2317ee2807d223a0bdc4c0c49dfdb893635c9adc5dea000008077a0883980438421ed0516088291915e01bebcd4240ddb9e709a710b7172ef6a9262a055bba5741dd52c3d43ff11b77ad9ba4b6e43afd8f61f6395da6ddaf933ea822e",
            )
        ];

		for (tx_root, rlp_encoded_tx) in test_data {
			use std::str::FromStr;
			let expected_txs_root = H256::from_str(tx_root).unwrap();
			let txs_blob = vec![alloy_core::hex::decode(&rlp_encoded_tx).unwrap()];

			use sp_trie::TrieConfiguration;
			// Transactions root using this implementation
			let txs_root_revive =
				EthTrieLayout::<sp_core::KeccakHasher>::ordered_trie_root(txs_blob.iter());

			// Transactions root using ethereum crate's
			let txs_root_ethereum = ethereum::util::ordered_trie_root(txs_blob.iter());

			assert_eq!(expected_txs_root, txs_root_ethereum);
			assert_eq!(expected_txs_root, txs_root_revive);
		}
	}
}
