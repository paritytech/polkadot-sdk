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
use codec::{Decode, Encode, MaxEncodedLen};
use pallet_revive_proc_macro::define_versioned_type;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::{bounded::BoundedVec, ConstU32};

use crate::common::{BoundedBytes, Bytes};

define_versioned_type! {
	/// Version 1 of the `PristineCode` storage value.
	#[versioned_type(encode_like = "Bytes; Vec<u8>")]
	#[derive(
		Debug,
		Default,
		Clone,
		Eq,
		PartialEq,
		Ord,
		PartialOrd,
		Hash,
		TypeInfo,
		Encode,
		Decode,
		Serialize,
		Deserialize,
	)]
	#[serde(transparent)]
	pub struct PristineCodeV1(
		/// Raw contract code stored under its hash.
		pub Bytes,
	);
}

impl From<Vec<u8>> for PristineCodeV1 {
	fn from(value: Vec<u8>) -> Self {
		Self(value.into())
	}
}

impl From<PristineCodeV1> for Vec<u8> {
	fn from(value: PristineCodeV1) -> Self {
		let PristineCodeV1(bytes) = value;
		bytes.0
	}
}

define_versioned_type! {
	/// Version 1 of a contract child-trie identifier stored on chain.
	#[versioned_type(
		encode_like = "BoundedBytes<LIMIT>; BoundedVec<u8, ConstU32<LIMIT>>"
	)]
	#[derive(
		Debug,
		Default,
		Clone,
		Eq,
		PartialEq,
		Ord,
		PartialOrd,
		Hash,
		TypeInfo,
		Encode,
		Decode,
		Serialize,
		Deserialize,
		MaxEncodedLen,
	)]
	#[serde(transparent)]
	pub struct TrieIdV1<const LIMIT: u32>(
		/// Bounded bytes that uniquely identify one contract child trie.
		pub BoundedBytes<LIMIT>,
	);
}

define_versioned_type! {
	/// Version 1 of the `ImmutableDataOf` storage value.
	#[versioned_type(
		encode_like = "BoundedBytes<LIMIT>; BoundedVec<u8, ConstU32<LIMIT>>"
	)]
	#[derive(
		Debug,
		Default,
		Clone,
		Eq,
		PartialEq,
		Ord,
		PartialOrd,
		Hash,
		TypeInfo,
		Encode,
		Decode,
		Serialize,
		Deserialize,
		MaxEncodedLen,
	)]
	#[serde(transparent)]
	pub struct ImmutableDataV1<const LIMIT: u32>(
		/// Bounded immutable bytes stored for one contract.
		pub BoundedBytes<LIMIT>,
	);
}

impl<const LIMIT: u32> From<BoundedVec<u8, ConstU32<LIMIT>>> for ImmutableDataV1<LIMIT> {
	fn from(value: BoundedVec<u8, ConstU32<LIMIT>>) -> Self {
		Self(value.into())
	}
}

impl<const LIMIT: u32> From<ImmutableDataV1<LIMIT>> for BoundedVec<u8, ConstU32<LIMIT>> {
	fn from(value: ImmutableDataV1<LIMIT>) -> Self {
		(value.0).0
	}
}

define_versioned_type! {
	/// Version 1 of one `DeletionQueue` storage entry.
	#[versioned_type(
		encode_like = "TrieIdV1<LIMIT>; BoundedBytes<LIMIT>; BoundedVec<u8, ConstU32<LIMIT>>"
	)]
	#[derive(
		Debug,
		Default,
		Clone,
		Eq,
		PartialEq,
		Ord,
		PartialOrd,
		Hash,
		TypeInfo,
		Encode,
		Decode,
		Serialize,
		Deserialize,
		MaxEncodedLen,
	)]
	#[serde(transparent)]
	pub struct DeletionQueueV1<const LIMIT: u32>(
		/// Trie identifier queued for lazy deletion.
		pub TrieIdV1<LIMIT>,
	);
}

impl<const LIMIT: u32> From<BoundedVec<u8, ConstU32<LIMIT>>> for DeletionQueueV1<LIMIT> {
	fn from(value: BoundedVec<u8, ConstU32<LIMIT>>) -> Self {
		Self(TrieIdV1(value.into()))
	}
}

impl<const LIMIT: u32> From<DeletionQueueV1<LIMIT>> for BoundedVec<u8, ConstU32<LIMIT>> {
	fn from(value: DeletionQueueV1<LIMIT>) -> Self {
		let DeletionQueueV1(trie_id) = value;
		(trie_id.0).0
	}
}
