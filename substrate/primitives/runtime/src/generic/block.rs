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

//! Generic implementation of a block and associated items.

#[cfg(feature = "std")]
use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{
	codec::{Codec, Decode, DecodeWithMemTracking, Encode, EncodeLike},
	traits::{
		self, Block as BlockT, Header as HeaderT, LazyExtrinsic, MaybeSerialize,
		MaybeSerializeDeserialize, Member, NumberFor,
	},
	Justifications, OpaqueExtrinsic,
};
use alloc::vec::Vec;
use core::marker::PhantomData;
use sp_core::RuntimeDebug;

/// Something to identify a block.
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum BlockId<Block: BlockT> {
	/// Identify by block header hash.
	Hash(Block::Hash),
	/// Identify by block number.
	Number(NumberFor<Block>),
}

impl<Block: BlockT> BlockId<Block> {
	/// Create a block ID from a hash.
	pub const fn hash(hash: Block::Hash) -> Self {
		BlockId::Hash(hash)
	}

	/// Create a block ID from a number.
	pub const fn number(number: NumberFor<Block>) -> Self {
		BlockId::Number(number)
	}

	/// Check if this block ID refers to the pre-genesis state.
	pub fn is_pre_genesis(&self) -> bool {
		match self {
			BlockId::Hash(hash) => hash == &Default::default(),
			BlockId::Number(_) => false,
		}
	}

	/// Create a block ID for a pre-genesis state.
	pub fn pre_genesis() -> Self {
		BlockId::Hash(Default::default())
	}
}

impl<Block: BlockT> Copy for BlockId<Block> {}

#[cfg(feature = "std")]
impl<Block: BlockT> fmt::Display for BlockId<Block> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{:?}", self)
	}
}

/// Abstraction over a substrate block that allows us to lazily decode its extrinsics.
#[derive(RuntimeDebug, Encode, Decode, scale_info::TypeInfo)]
pub struct LazyBlock<Header, Extrinsic> {
	/// The block header.
	pub header: Header,
	/// The accompanying extrinsics.
	pub extrinsics: Vec<OpaqueExtrinsic>,

	_phantom: PhantomData<Extrinsic>,
}

impl<Header, Extrinsic: Into<OpaqueExtrinsic>> LazyBlock<Header, Extrinsic> {
	/// Creates a new instance of `LazyBlock` from its parts.
	pub fn new(header: Header, extrinsics: Vec<Extrinsic>) -> Self {
		Self {
			header,
			extrinsics: extrinsics.into_iter().map(|xt| xt.into()).collect(),
			_phantom: Default::default(),
		}
	}
}

impl<Header, Extrinsic: Into<OpaqueExtrinsic>> From<Block<Header, Extrinsic>>
	for LazyBlock<Header, Extrinsic>
{
	fn from(block: Block<Header, Extrinsic>) -> Self {
		LazyBlock::new(block.header, block.extrinsics)
	}
}

impl<Header, Extrinsic> EncodeLike<LazyBlock<Header, Extrinsic>> for Block<Header, Extrinsic>
where
	Block<Header, Extrinsic>: Encode,
	LazyBlock<Header, Extrinsic>: Encode,
{
}

impl<Header, Extrinsic> EncodeLike<Block<Header, Extrinsic>> for LazyBlock<Header, Extrinsic>
where
	Block<Header, Extrinsic>: Encode,
	LazyBlock<Header, Extrinsic>: Encode,
{
}

impl<Header, Extrinsic> traits::LazyBlock for LazyBlock<Header, Extrinsic>
where
	Header: HeaderT,
	Extrinsic: core::fmt::Debug + LazyExtrinsic,
{
	type Extrinsic = Extrinsic;
	type Header = Header;

	fn header(&self) -> &Self::Header {
		&self.header
	}

	fn header_mut(&mut self) -> &mut Self::Header {
		&mut self.header
	}

	fn extrinsics(&self) -> impl Iterator<Item = Result<Self::Extrinsic, codec::Error>> {
		self.extrinsics.iter().map(|xt| Self::Extrinsic::decode_unprefixed(&xt.0))
	}
}

/// Abstraction over a substrate block.
#[derive(
	PartialEq, Eq, Clone, Encode, Decode, DecodeWithMemTracking, RuntimeDebug, scale_info::TypeInfo,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct Block<Header, Extrinsic> {
	/// The block header.
	pub header: Header,
	/// The accompanying extrinsics.
	pub extrinsics: Vec<Extrinsic>,
}

impl<Header, Extrinsic> traits::HeaderProvider for Block<Header, Extrinsic>
where
	Header: HeaderT,
{
	type HeaderT = Header;
}

impl<Header, Extrinsic: MaybeSerialize> traits::Block for Block<Header, Extrinsic>
where
	Header: HeaderT + MaybeSerializeDeserialize,
	Extrinsic: Member
		+ Codec
		+ DecodeWithMemTracking
		+ traits::ExtrinsicLike
		+ Into<OpaqueExtrinsic>
		+ LazyExtrinsic,
{
	type Extrinsic = Extrinsic;
	type Header = Header;
	type Hash = <Self::Header as traits::Header>::Hash;
	type LazyBlock = LazyBlock<Header, Extrinsic>;

	fn header(&self) -> &Self::Header {
		&self.header
	}
	fn extrinsics(&self) -> &[Self::Extrinsic] {
		&self.extrinsics[..]
	}
	fn deconstruct(self) -> (Self::Header, Vec<Self::Extrinsic>) {
		(self.header, self.extrinsics)
	}
	fn new(header: Self::Header, extrinsics: Vec<Self::Extrinsic>) -> Self {
		Block { header, extrinsics }
	}
}

/// Abstraction over a substrate block and justification.
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct SignedBlock<Block> {
	/// Full block.
	pub block: Block,
	/// Block justification.
	pub justifications: Option<Justifications>,
}
