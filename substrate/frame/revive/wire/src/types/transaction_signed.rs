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

use super::transaction_unsigned::{
	Transaction1559UnsignedV1, Transaction2930UnsignedV1, Transaction4844UnsignedV1,
	Transaction7702UnsignedV1, TransactionLegacyUnsignedV1,
};
use codec::{Decode, Encode};
use ethereum_types::U256;
use pallet_revive_proc_macro::define_versioned_type;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

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
		TransactionSignedV1::TransactionLegacySigned(Default::default())
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
