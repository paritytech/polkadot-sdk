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

use crate::common::Bytes;
use alloc::{string::String, vec::Vec};
use codec::{Decode, Encode, Input};
use ethereum_types::{Address, H256, U256};
use pallet_revive_proc_macro::define_versioned_type;
use scale_info::TypeInfo;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

define_versioned_type! {
	/// Version 1 of block transactions represented as hashes or full transaction objects.
	#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode)]
	#[serde(untagged)]
	pub enum HashesOrTransactionInfosV1 {
		/// Transaction hashes.
		Hashes(Vec<H256>),
		/// Full transaction information objects.
		TransactionInfos(Vec<TransactionInfoV1>),
	}
}

impl Default for HashesOrTransactionInfosV1 {
	fn default() -> Self {
		Self::Hashes(Default::default())
	}
}

define_versioned_type! {
	/// Version 1 of transaction information embedded in full transaction block responses.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct TransactionInfoV1 {
		/// Block hash.
		pub block_hash: H256,
		/// Block number.
		pub block_number: U256,
		/// Sender address.
		pub from: Address,
		/// Transaction hash.
		pub hash: H256,
		/// Transaction index.
		pub transaction_index: U256,
		/// Signed transaction payload.
		#[serde(flatten)]
		pub transaction_signed: TransactionSignedV1,
	}
}

define_versioned_type! {
	/// Version 1 of a signed transaction payload.
	#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode)]
	#[serde(untagged)]
	pub enum TransactionSignedV1 {
		/// EIP-7702 signed transaction.
		Transaction7702Signed(Transaction7702SignedV1),
		/// EIP-4844 signed transaction.
		Transaction4844Signed(Transaction4844SignedV1),
		/// EIP-1559 signed transaction.
		Transaction1559Signed(Transaction1559SignedV1),
		/// EIP-2930 signed transaction.
		Transaction2930Signed(Transaction2930SignedV1),
		/// Legacy signed transaction.
		TransactionLegacySigned(TransactionLegacySignedV1),
	}
}

impl Default for TransactionSignedV1 {
	fn default() -> Self {
		Self::TransactionLegacySigned(Default::default())
	}
}

define_versioned_type! {
	/// Version 1 of an EIP-7702 signed transaction.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct Transaction7702SignedV1 {
		/// Unsigned EIP-7702 transaction fields.
		#[serde(flatten)]
		pub transaction_7702_unsigned: Transaction7702UnsignedV1,
		/// Signature r component.
		pub r: U256,
		/// Signature s component.
		pub s: U256,
		/// Backward-compatible y-parity value.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub v: Option<U256>,
		/// Signature y parity.
		pub y_parity: U256,
	}
}

define_versioned_type! {
	/// Version 1 of an EIP-4844 signed transaction.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct Transaction4844SignedV1 {
		/// Unsigned EIP-4844 transaction fields.
		#[serde(flatten)]
		pub transaction_4844_unsigned: Transaction4844UnsignedV1,
		/// Signature r component.
		pub r: U256,
		/// Signature s component.
		pub s: U256,
		/// Signature y parity.
		pub y_parity: U256,
	}
}

define_versioned_type! {
	/// Version 1 of an EIP-1559 signed transaction.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct Transaction1559SignedV1 {
		/// Unsigned EIP-1559 transaction fields.
		#[serde(flatten)]
		pub transaction_1559_unsigned: Transaction1559UnsignedV1,
		/// Signature r component.
		pub r: U256,
		/// Signature s component.
		pub s: U256,
		/// Backward-compatible y-parity value.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub v: Option<U256>,
		/// Signature y parity.
		pub y_parity: U256,
	}
}

define_versioned_type! {
	/// Version 1 of an EIP-2930 signed transaction.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct Transaction2930SignedV1 {
		/// Unsigned EIP-2930 transaction fields.
		#[serde(flatten)]
		pub transaction_2930_unsigned: Transaction2930UnsignedV1,
		/// Signature r component.
		pub r: U256,
		/// Signature s component.
		pub s: U256,
		/// Backward-compatible y-parity value.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub v: Option<U256>,
		/// Signature y parity.
		pub y_parity: U256,
	}
}

define_versioned_type! {
	/// Version 1 of a legacy signed transaction.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct TransactionLegacySignedV1 {
		/// Unsigned legacy transaction fields.
		#[serde(flatten)]
		pub transaction_legacy_unsigned: TransactionLegacyUnsignedV1,
		/// Signature r component.
		pub r: U256,
		/// Signature s component.
		pub s: U256,
		/// Signature recovery value.
		pub v: U256,
	}
}

define_versioned_type! {
	/// Version 1 of an EIP-7702 unsigned transaction.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct Transaction7702UnsignedV1 {
		/// EIP-2930 access list.
		pub access_list: Vec<AccessListEntryV1>,
		/// List of account code authorizations.
		pub authorization_list: Vec<AuthorizationListEntryV1>,
		/// Chain ID.
		pub chain_id: U256,
		/// Gas limit.
		pub gas: U256,
		/// Input data.
		pub input: Bytes,
		/// Maximum total fee per gas.
		pub max_fee_per_gas: U256,
		/// Maximum priority fee per gas.
		pub max_priority_fee_per_gas: U256,
		/// Nonce.
		pub nonce: U256,
		/// Destination address.
		pub to: Address,
		/// Transaction type.
		pub r#type: TypeEip7702V1,
		/// Value.
		pub value: U256,
	}
}

define_versioned_type! {
	/// Version 1 of an EIP-4844 unsigned transaction.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct Transaction4844UnsignedV1 {
		/// EIP-2930 access list.
		pub access_list: Vec<AccessListEntryV1>,
		/// Versioned blob hashes.
		pub blob_versioned_hashes: Vec<H256>,
		/// Chain ID.
		pub chain_id: U256,
		/// Gas limit.
		pub gas: U256,
		/// Input data.
		pub input: Bytes,
		/// Maximum fee per blob gas.
		pub max_fee_per_blob_gas: U256,
		/// Maximum total fee per gas.
		pub max_fee_per_gas: U256,
		/// Maximum priority fee per gas.
		pub max_priority_fee_per_gas: U256,
		/// Nonce.
		pub nonce: U256,
		/// Destination address.
		pub to: Address,
		/// Transaction type.
		pub r#type: TypeEip4844V1,
		/// Value.
		pub value: U256,
	}
}

define_versioned_type! {
	/// Version 1 of an EIP-1559 unsigned transaction.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct Transaction1559UnsignedV1 {
		/// EIP-2930 access list.
		pub access_list: Vec<AccessListEntryV1>,
		/// Chain ID.
		pub chain_id: U256,
		/// Gas limit.
		pub gas: U256,
		/// Effective gas price.
		pub gas_price: U256,
		/// Input data.
		pub input: Bytes,
		/// Maximum total fee per gas.
		pub max_fee_per_gas: U256,
		/// Maximum priority fee per gas.
		pub max_priority_fee_per_gas: U256,
		/// Nonce.
		pub nonce: U256,
		/// Destination address.
		pub to: Option<Address>,
		/// Transaction type.
		pub r#type: TypeEip1559V1,
		/// Value.
		pub value: U256,
	}
}

define_versioned_type! {
	/// Version 1 of an EIP-2930 unsigned transaction.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct Transaction2930UnsignedV1 {
		/// EIP-2930 access list.
		pub access_list: Vec<AccessListEntryV1>,
		/// Chain ID.
		pub chain_id: U256,
		/// Gas limit.
		pub gas: U256,
		/// Gas price.
		pub gas_price: U256,
		/// Input data.
		pub input: Bytes,
		/// Nonce.
		pub nonce: U256,
		/// Destination address.
		pub to: Option<Address>,
		/// Transaction type.
		pub r#type: TypeEip2930V1,
		/// Value.
		pub value: U256,
	}
}

define_versioned_type! {
	/// Version 1 of a legacy unsigned transaction.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct TransactionLegacyUnsignedV1 {
		/// Chain ID.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub chain_id: Option<U256>,
		/// Gas limit.
		pub gas: U256,
		/// Gas price.
		pub gas_price: U256,
		/// Input data.
		pub input: Bytes,
		/// Nonce.
		pub nonce: U256,
		/// Destination address.
		pub to: Option<Address>,
		/// Transaction type.
		pub r#type: TypeLegacyV1,
		/// Value.
		pub value: U256,
	}
}

define_versioned_type! {
	/// Version 1 of an access-list entry.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct AccessListEntryV1 {
		/// Address whose storage is accessed.
		pub address: Address,
		/// Storage keys accessed by the transaction.
		pub storage_keys: Vec<H256>,
	}
}

define_versioned_type! {
	/// Version 1 of an authorization-list entry for EIP-7702.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct AuthorizationListEntryV1 {
		/// Chain ID that this authorization is valid on.
		pub chain_id: U256,
		/// Address to authorize.
		pub address: Address,
		/// Nonce of the authorization.
		pub nonce: U256,
		/// Y-parity of the signature.
		pub y_parity: U256,
		/// Signature r component.
		pub r: U256,
		/// Signature s component.
		pub s: U256,
	}
}

macro_rules! transaction_type {
	($name:ident, $value:literal) => {
		define_versioned_type! {
			#[doc = concat!("Version 1 transaction type identifier: ", stringify!($value), ".")]
			#[derive(Clone, Default, Debug, Eq, PartialEq)]
			pub struct $name;
		}

		impl Encode for $name {
			fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
				f(&[$value])
			}
		}

		impl Decode for $name {
			fn decode<I: Input>(input: &mut I) -> Result<Self, codec::Error> {
				if $value == input.read_byte()? {
					Ok(Self)
				} else {
					Err(codec::Error::from(concat!("expected ", stringify!($value))))
				}
			}
		}

		impl TypeInfo for $name {
			type Identity = u8;

			fn type_info() -> scale_info::Type {
				<u8 as TypeInfo>::type_info()
			}
		}

		impl Serialize for $name {
			fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
			where
				S: Serializer,
			{
				serializer.serialize_str(concat!("0x", stringify!($value)))
			}
		}

		impl<'de> Deserialize<'de> for $name {
			fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
			where
				D: Deserializer<'de>,
			{
				let value = String::deserialize(deserializer)?;
				if value == concat!("0x", stringify!($value)) {
					Ok(Self)
				} else {
					Err(serde::de::Error::custom(concat!("expected ", stringify!($value))))
				}
			}
		}
	};
}

transaction_type!(TypeLegacyV1, 0);
transaction_type!(TypeEip2930V1, 1);
transaction_type!(TypeEip1559V1, 2);
transaction_type!(TypeEip4844V1, 3);
transaction_type!(TypeEip7702V1, 4);
