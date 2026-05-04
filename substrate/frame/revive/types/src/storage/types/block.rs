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

use super::transaction::HashesOrTransactionInfosV1;
use crate::common::{Bytes, Bytes256, Bytes8};
use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
use ethereum_types::{Address, H256, U256};
use pallet_revive_proc_macro::define_versioned_type;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

define_versioned_type! {
	/// Version 1 of the pallet's stored Ethereum block payload.
	#[derive(
		Debug,
		Default,
		Clone,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct EthBlockV1 {
		/// Base fee per gas.
		pub base_fee_per_gas: U256,
		/// Blob gas used.
		pub blob_gas_used: U256,
		/// Difficulty.
		pub difficulty: U256,
		/// Excess blob gas.
		pub excess_blob_gas: U256,
		/// Extra data.
		pub extra_data: Bytes,
		/// Gas limit.
		pub gas_limit: U256,
		/// Gas used.
		pub gas_used: U256,
		/// Hash.
		pub hash: H256,
		/// Bloom filter.
		pub logs_bloom: Bytes256,
		/// Coinbase.
		pub miner: Address,
		/// Mix hash.
		pub mix_hash: H256,
		/// Nonce.
		pub nonce: Bytes8,
		/// Number.
		pub number: U256,
		/// Parent Beacon Block Root.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub parent_beacon_block_root: Option<H256>,
		/// Parent block hash.
		pub parent_hash: H256,
		/// Receipts root.
		pub receipts_root: H256,
		/// Requests root.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub requests_hash: Option<H256>,
		/// Ommers hash.
		pub sha_3_uncles: H256,
		/// Block size.
		pub size: U256,
		/// State root.
		pub state_root: H256,
		/// Timestamp.
		pub timestamp: U256,
		/// Total difficulty.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub total_difficulty: Option<U256>,
		/// Transactions represented as hashes or full transaction objects.
		pub transactions: HashesOrTransactionInfosV1,
		/// Transactions root.
		pub transactions_root: H256,
		/// Uncles.
		pub uncles: Vec<H256>,
		/// Withdrawals.
		pub withdrawals: Vec<WithdrawalV1>,
		/// Withdrawals root.
		pub withdrawals_root: H256,
	}
}

define_versioned_type! {
	/// Version 1 of a validator withdrawal included in a block.
	#[derive(
		Debug,
		Default,
		Clone,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct WithdrawalV1 {
		/// Recipient address for withdrawal value.
		pub address: Address,
		/// Value contained in withdrawal.
		pub amount: U256,
		/// Withdrawal index.
		pub index: U256,
		/// Validator index that generated the withdrawal.
		pub validator_index: U256,
	}
}

define_versioned_type! {
	/// Version 1 of the `EthereumBlock` storage value.
	#[versioned_type(encode_like = "EthBlockV1")]
	#[derive(
		Debug,
		Default,
		Clone,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
	)]
	#[serde(transparent)]
	pub struct EthereumBlockV1(
		/// Stored Ethereum block payload.
		pub EthBlockV1,
	);
}

define_versioned_type! {
	/// Version 1 of the `BlockHash` storage value.
	#[versioned_type(encode_like = "H256")]
	#[derive(
		Debug,
		Default,
		Clone,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
	)]
	#[serde(transparent)]
	pub struct BlockHashV1(
		/// Block hash stored for one block number.
		pub H256,
	);
}

impl From<H256> for BlockHashV1 {
	fn from(value: H256) -> Self {
		Self(value)
	}
}

impl From<BlockHashV1> for H256 {
	fn from(value: BlockHashV1) -> Self {
		value.0
	}
}

define_versioned_type! {
	/// Version 1 of the gas data needed to reconstruct an Ethereum receipt.
	#[derive(
		Debug,
		Default,
		Clone,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
	)]
	pub struct ReceiptGasInfoV1 {
		/// The amount of gas used by one transaction.
		pub gas_used: U256,
		/// The effective gas price paid by one transaction.
		pub effective_gas_price: U256,
	}
}

define_versioned_type! {
	/// Version 1 of the `ReceiptInfoData` storage value.
	#[versioned_type(encode_like = "Vec<ReceiptGasInfoV1>")]
	#[derive(
		Debug,
		Default,
		Clone,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
	)]
	#[serde(transparent)]
	pub struct ReceiptInfoDataV1(
		/// Gas metadata for each receipt in the stored Ethereum block.
		pub Vec<ReceiptGasInfoV1>,
	);
}

define_versioned_type! {
	/// Version 1 of the first encoded transaction and receipt cached by the block builder.
	#[derive(
		Debug,
		Default,
		Clone,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
	)]
	pub struct BlockBuilderFirstValuesV1 {
		/// SCALE-encoded first transaction inserted into the block builder.
		pub first_transaction: Bytes,
		/// SCALE-encoded first receipt inserted into the block builder.
		pub first_receipt: Bytes,
	}
}

define_versioned_type! {
	/// Version 1 of the `EthBlockBuilderFirstValues` storage value.
	#[versioned_type(encode_like = "Option<BlockBuilderFirstValuesV1>")]
	#[derive(
		Debug,
		Default,
		Clone,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
	)]
	#[serde(transparent)]
	pub struct EthBlockBuilderFirstValuesV1(
		/// Optional first transaction and receipt pair cached outside the builder IR.
		pub Option<BlockBuilderFirstValuesV1>,
	);
}
