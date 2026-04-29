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

use super::bytes::TrieIdV1;
use crate::common::Bytes;
use codec::{Decode, Encode};
use ethereum_types::H256;
use pallet_revive_proc_macro::define_versioned_type;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

define_versioned_type! {
	/// Version 1 of the `AccountInfoOf` storage value keyed by account address.
	#[derive(
		Debug, Default, Clone, Eq, PartialEq, TypeInfo, Encode, Decode, Serialize, Deserialize,
	)]
	pub struct AccountInfoV1<Balance, const TRIE_ID_LIMIT: u32> {
		/// Whether the account is a contract or an externally owned account.
		pub account_type: AccountTypeV1<Balance, TRIE_ID_LIMIT>,
		/// Native-currency dust tracked alongside the EVM-facing account state.
		pub dust: u32,
	}
}

define_versioned_type! {
	/// Version 1 of the account kind stored inside `AccountInfoOf`.
	#[derive(
		Debug, Default, Clone, Eq, PartialEq, TypeInfo, Encode, Decode, Serialize, Deserialize,
	)]
	pub enum AccountTypeV1<Balance, const TRIE_ID_LIMIT: u32> {
		/// Contract state together with its child-trie accounting data.
		Contract(ContractInfoV1<Balance, TRIE_ID_LIMIT>),
		/// Externally owned account state with no contract metadata.
		#[default]
		EOA,
	}
}

define_versioned_type! {
	/// Version 1 of the contract metadata nested inside `AccountInfoOf`.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode, Serialize, Deserialize)]
	pub struct ContractInfoV1<Balance, const TRIE_ID_LIMIT: u32> {
		/// Unique identifier of the contract's child trie.
		pub trie_id: TrieIdV1<TRIE_ID_LIMIT>,
		/// Hash of the code currently associated with the contract.
		pub code_hash: H256,
		/// Total number of bytes stored in the contract's child trie.
		pub storage_bytes: u32,
		/// Total number of storage items stored in the contract's child trie.
		pub storage_items: u32,
		/// Deposit charged for the bytes currently stored in the child trie.
		pub storage_byte_deposit: Balance,
		/// Deposit charged for the items currently stored in the child trie.
		pub storage_item_deposit: Balance,
		/// Deposit charged for the contract record itself.
		pub storage_base_deposit: Balance,
		/// Number of immutable-data bytes associated with the contract.
		pub immutable_data_len: u32,
	}
}

define_versioned_type! {
	/// Version 1 of the `OriginalAccount` storage value keyed by Ethereum address.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode, Serialize, Deserialize)]
	#[serde(transparent)]
	pub struct OriginalAccountV1<AccountId>(
		/// Original `AccountId32` mapped from one Ethereum address.
		pub AccountId,
	);
}

define_versioned_type! {
	/// Version 1 of the previous-value outcome returned by storage writes.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode, Serialize, Deserialize)]
	pub enum WriteOutcomeV1 {
		/// No value existed at the written key.
		New,
		/// A value of the returned length was overwritten.
		Overwritten(u32),
		/// The previous value was taken out of storage before the write completed.
		Taken(Bytes),
	}
}
