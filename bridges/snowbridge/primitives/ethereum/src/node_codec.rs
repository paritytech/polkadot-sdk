// Copyright 2015-2020 Parity Technologies (UK) Ltd.
// This file is part of OpenEthereum.

// OpenEthereum is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// OpenEthereum is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with OpenEthereum.  If not, see <http://www.gnu.org/licenses/>.

//! [`NodeCodec`] implementation for [`TrieDb`]

use alloc::vec::Vec;
use core::{borrow::Borrow, marker::PhantomData};
use ethereum_types::H256;
use hash_db::Hasher;
use rlp::{DecoderError, Prototype, Rlp, RlpStream};
use trie_db::{
	node::{NibbleSlicePlan, NodeHandlePlan, NodePlan, Value, ValuePlan},
	ChildReference, NodeCodec,
};

/// Concrete implementation of a `NodeCodec` with Rlp encoding, generic over the `Hasher`
#[derive(Default, Clone)]
pub struct RlpNodeCodec<H: Hasher> {
	mark: PhantomData<H>,
}

const HASHED_NULL_NODE: [u8; 32] = [
	0x56, 0xe8, 0x1f, 0x17, 0x1b, 0xcc, 0x55, 0xa6, 0xff, 0x83, 0x45, 0xe6, 0x92, 0xc0, 0xf8, 0x6e,
	0x5b, 0x48, 0xe0, 0x1b, 0x99, 0x6c, 0xad, 0xc0, 0x01, 0x62, 0x2f, 0xb5, 0xe3, 0x63, 0xb4, 0x21,
];

// NOTE: what we'd really like here is:
// `impl<H: Hasher> NodeCodec<H> for RlpNodeCodec<H> where H::Out: Decodable`
// but due to the current limitations of Rust const evaluation we can't
// do `const HASHED_NULL_NODE: H::Out = H::Out( … … )`. Perhaps one day soon?
impl<H> NodeCodec for RlpNodeCodec<H>
where
	H: Hasher<Out = H256>,
{
	type Error = DecoderError;
	type HashOut = H::Out;

	fn hashed_null_node() -> H::Out {
		H256(HASHED_NULL_NODE)
	}

	fn decode_plan(data: &[u8]) -> Result<NodePlan, Self::Error> {
		if data == &HASHED_NULL_NODE {
			// early return if this is == keccak(rlp(null)), aka empty trie root
			// source: https://ethereum.github.io/execution-specs/diffs/frontier_homestead/trie/index.html#empty-trie-root
			return Ok(NodePlan::Empty);
		}

		let r = Rlp::new(data);
		match r.prototype()? {
			// either leaf or extension - decode first item with NibbleSlice::???
			// and use is_leaf return to figure out which.
			// if leaf, second item is a value (is_data())
			// if extension, second item is a node (either SHA3 to be looked up and
			// fed back into this function or inline RLP which can be fed back into this function).
			Prototype::List(2) => {
				let (rlp, offset) = r.at_with_offset(0)?;
				let (data, i) = (rlp.data()?, rlp.payload_info()?);
				match (
					NibbleSlicePlan::new(
						(offset + i.header_len)..(offset + i.header_len + i.value_len),
						if data[0] & 16 == 16 { 1 } else { 2 },
					),
					data[0] & 32 == 32,
				) {
					(slice, true) => Ok(NodePlan::Leaf {
						partial: slice,
						value: {
							let (item, offset) = r.at_with_offset(1)?;
							let i = item.payload_info()?;
							ValuePlan::Inline(
								(offset + i.header_len)..(offset + i.header_len + i.value_len),
							)
						},
					}),
					(slice, false) => Ok(NodePlan::Extension {
						partial: slice,
						child: {
							let (item, offset) = r.at_with_offset(1)?;
							let i = item.payload_info()?;
							NodeHandlePlan::Hash(
								(offset + i.header_len)..(offset + i.header_len + i.value_len),
							)
						},
					}),
				}
			},
			// branch - first 16 are nodes, 17th is a value (or empty).
			Prototype::List(17) => {
				let mut nodes = [
					None, None, None, None, None, None, None, None, None, None, None, None, None,
					None, None, None,
				];

				for index in 0..16 {
					let (item, offset) = r.at_with_offset(index)?;
					let i = item.payload_info()?;
					if item.is_empty() {
						nodes[index] = None;
					} else {
						nodes[index] = Some(NodeHandlePlan::Hash(
							(offset + i.header_len)..(offset + i.header_len + i.value_len),
						));
					}
				}

				Ok(NodePlan::Branch {
					children: nodes,
					value: {
						let (item, offset) = r.at_with_offset(16)?;
						let i = item.payload_info()?;
						if item.is_empty() {
							None
						} else {
							Some(ValuePlan::Inline(
								(offset + i.header_len)..(offset + i.header_len + i.value_len),
							))
						}
					},
				})
			},
			// an empty branch index.
			Prototype::Data(0) => Ok(NodePlan::Empty),
			// something went wrong.
			_ => Err(DecoderError::Custom("Rlp is not valid."))?,
		}
	}

	fn is_empty_node(data: &[u8]) -> bool {
		Rlp::new(data).is_empty()
	}

	fn empty_node() -> &'static [u8] {
		&[0x80]
	}

	fn leaf_node(
		partial: impl Iterator<Item = u8>,
		_number_nibble: usize,
		value: Value,
	) -> Vec<u8> {
		let mut stream = RlpStream::new_list(2);
		let partial = partial.collect::<Vec<_>>();
		stream.append(&partial);
		let value = match value {
			Value::Node(bytes) => bytes,
			Value::Inline(bytes) => bytes,
		};
		stream.append(&value);
		stream.out().to_vec()
	}

	fn extension_node(
		partial: impl Iterator<Item = u8>,
		_number_nibble: usize,
		child_ref: ChildReference<Self::HashOut>,
	) -> Vec<u8> {
		let mut stream = RlpStream::new_list(2);
		stream.append(&partial.collect::<Vec<_>>());
		match child_ref {
			ChildReference::Hash(h) => stream.append(&h.as_ref()),
			ChildReference::Inline(inline_data, len) => {
				let bytes = &AsRef::<[u8]>::as_ref(&inline_data)[..len];
				stream.append_raw(bytes, 1)
			},
		};
		stream.out().to_vec()
	}

	fn branch_node(
		children: impl Iterator<Item = impl Borrow<Option<ChildReference<Self::HashOut>>>>,
		value: Option<Value>,
	) -> Vec<u8> {
		let mut stream = RlpStream::new_list(17);
		for child_ref in children {
			match child_ref.borrow() {
				Some(c) => match c {
					ChildReference::Hash(h) => stream.append(&h.as_ref()),
					ChildReference::Inline(inline_data, len) => {
						let bytes = &inline_data[..*len];
						stream.append_raw(bytes, 1)
					},
				},
				None => stream.append_empty_data(),
			};
		}
		if let Some(value) = value {
			let value = match value {
				Value::Node(bytes) => bytes,
				Value::Inline(bytes) => bytes,
			};
			stream.append(&value);
		} else {
			stream.append_empty_data();
		}
		stream.out().to_vec()
	}

	fn branch_node_nibbled(
		_partial: impl Iterator<Item = u8>,
		_number_nibble: usize,
		_children: impl Iterator<Item = impl Borrow<Option<ChildReference<Self::HashOut>>>>,
		_value: Option<Value>,
	) -> Vec<u8> {
		unimplemented!("Ethereum branch nodes do not have partial key; qed")
	}
}
