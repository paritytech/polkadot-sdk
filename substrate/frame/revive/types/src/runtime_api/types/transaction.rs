// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use alloc::vec::Vec;
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use serde::{Deserialize, Deserializer, Serialize};
use sp_core::{H160, H256, U256};

use crate::common::*;

#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
)]
#[serde(rename_all = "camelCase")]
pub struct GenericTransactionV1 {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub access_list: Option<AccessListV1>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub authorization_list: Vec<AuthorizationListEntryV1>,
	#[serde(default)]
	pub blob_versioned_hashes: Vec<H256>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub blobs: Vec<Bytes>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub chain_id: Option<U256>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub from: Option<H160>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub gas: Option<U256>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub gas_price: Option<U256>,
	#[serde(flatten, deserialize_with = "deserialize_input_or_data")]
	pub input: InputOrDataV1,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_fee_per_blob_gas: Option<U256>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_fee_per_gas: Option<U256>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_priority_fee_per_gas: Option<U256>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub nonce: Option<U256>,
	pub to: Option<H160>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub r#type: Option<Byte>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub value: Option<U256>,
}

pub type AccessListV1 = Vec<AccessListEntryV1>;

#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
)]
#[serde(rename_all = "camelCase")]
pub struct AccessListEntryV1 {
	pub address: H160,
	pub storage_keys: Vec<H256>,
}

#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizationListEntryV1 {
	pub chain_id: U256,
	pub address: H160,
	pub nonce: U256,
	pub y_parity: U256,
	pub r: U256,
	pub s: U256,
}

#[derive(
	Debug, Default, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize, Eq, PartialEq,
)]
pub struct InputOrDataV1 {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub input: Option<Bytes>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub data: Option<Bytes>,
}

impl From<Bytes> for InputOrDataV1 {
	fn from(value: Bytes) -> Self {
		InputOrDataV1 { input: Some(value), data: None }
	}
}

impl From<Vec<u8>> for InputOrDataV1 {
	fn from(value: Vec<u8>) -> Self {
		InputOrDataV1 { input: Some(Bytes(value)), data: None }
	}
}

impl InputOrDataV1 {
	pub fn to_bytes(self) -> Bytes {
		match self {
			InputOrDataV1 { input: Some(input), data: _ } => input,
			InputOrDataV1 { input: None, data: Some(data) } => data,
			_ => Default::default(),
		}
	}

	pub fn to_vec(self) -> Vec<u8> {
		self.to_bytes().0
	}

	pub fn as_slice(&self) -> &[u8] {
		self.input
			.as_ref()
			.or(self.data.as_ref())
			.map(|bytes| bytes.0.as_slice())
			.unwrap_or_default()
	}

	pub fn is_empty(&self) -> bool {
		self.as_slice().is_empty()
	}
}

fn deserialize_input_or_data<'d, D: Deserializer<'d>>(d: D) -> Result<InputOrDataV1, D::Error> {
	let value = InputOrDataV1::deserialize(d)?;
	match &value {
		InputOrDataV1 { input: Some(input), data: Some(data) } if input != data => {
			Err(serde::de::Error::custom(
				"Both \"data\" and \"input\" are set and not equal. Please use \"input\" to pass transaction call data",
			))
		},
		_ => Ok(value),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::{string::ToString, vec};

	#[test]
	fn can_deserialize_input_or_data_field_from_generic_transaction() {
		let cases = [
			("with input", r#"{"input": "0x01"}"#),
			("with data", r#"{"data": "0x01"}"#),
			("with both", r#"{"data": "0x01", "input": "0x01"}"#),
		];

		for (name, json) in cases {
			let tx = serde_json::from_str::<GenericTransactionV1>(json).unwrap();
			assert_eq!(tx.input.to_vec(), vec![1u8], "{}", name);
		}

		let err =
			serde_json::from_str::<GenericTransactionV1>(r#"{"data": "0x02", "input": "0x01"}"#)
				.unwrap_err();
		assert!(
			err.to_string().starts_with(
			"Both \"data\" and \"input\" are set and not equal. Please use \"input\" to pass transaction call data"
			)
		);
	}
}
