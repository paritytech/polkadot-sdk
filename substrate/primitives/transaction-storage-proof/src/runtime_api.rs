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

//! Runtime API definition for the transaction storage proof processing.

use alloc::vec::Vec;
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_runtime::traits::NumberFor;

pub type ContentHash = [u8; 32];

pub type CidCodec = u64;

pub const RAW_CID_CODEC: CidCodec = 0x55;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[non_exhaustive]
pub enum HashingAlgorithm {
	Blake2b256,
	Sha2_256,
	Keccak256,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct IndexedTransactionInfo {
	pub content_hash: ContentHash,
	pub size: u32,
	pub hashing: HashingAlgorithm,
	pub cid_codec: CidCodec,
	/// Extrinsic index within the block that originally indexed this data
	/// (via `sp_io::transaction_index::index` / `renew`). For renewed entries
	/// this holds the renewer's extrinsic index, not the original. Downstream
	/// pallets that did not previously persist this value may emit `u32::MAX`
	/// as a sentinel for entries that pre-date the field.
	pub extrinsic_index: u32,
}

sp_api::decl_runtime_apis! {
	/// Runtime API trait for transaction storage support.
	#[api_version(2)]
	pub trait TransactionStorageApi {
		/// Get the actual value of a retention period in blocks.
		fn retention_period() -> NumberFor<Block>;

		/// Get indexed-transaction metadata for `block`.
		///
		/// Returns an empty vector if the block has no indexed transactions or
		/// is outside the retention window.
		fn indexed_transactions(block: NumberFor<Block>) -> Vec<IndexedTransactionInfo>;
	}
}
