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

use codec::{Decode, Encode};
use pallet_revive_proc_macro::define_versioned_type;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

use crate::common::{BoundedBytes, Bytes};

define_versioned_type! {
	/// Version 1 of the `PristineCode` storage value.
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

define_versioned_type! {
	/// Version 1 of a contract child-trie identifier stored on chain.
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
	pub struct TrieIdV1<const LIMIT: u32>(
		/// Bounded bytes that uniquely identify one contract child trie.
		pub BoundedBytes<LIMIT>,
	);
}

define_versioned_type! {
	/// Version 1 of the `ImmutableDataOf` storage value.
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
	pub struct ImmutableDataV1<const LIMIT: u32>(
		/// Bounded immutable bytes stored for one contract.
		pub BoundedBytes<LIMIT>,
	);
}

define_versioned_type! {
	/// Version 1 of one `DeletionQueue` storage entry.
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
	pub struct DeletionQueueV1<const LIMIT: u32>(
		/// Trie identifier queued for lazy deletion.
		pub TrieIdV1<LIMIT>,
	);
}
