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
//! Utility impl for the RPC types.
use super::*;
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::{H160, U256};

/// Configuration specific to a dry-run execution.
///
/// Passed as an argument to the `eth_transact_with_config` runtime API method. Contains optional
/// overrides that control how the dry-run is executed, such as timestamp simulation and state
/// injection.
///
/// # Backwards Compatibility
///
/// This type is SCALE-encoded when passed across the runtime API boundary via `state_call`.
/// SCALE is a non-self-describing format: fields are encoded sequentially with no field names,
/// delimiters, or end-of-message markers. This has important implications when the struct evolves
/// over time.
///
/// ## Adding new trailing fields
///
/// New fields may be appended to the end of this struct without requiring a new runtime API
/// method, provided that:
///
/// 1. **New RPC, old runtime (trailing bytes are ignored):** The `sp_api` runtime API argument
///    decoding machinery uses `Decode::decode` (not `decode_all`) for parameterized calls. This
///    means bytes remaining after decoding all known fields are silently ignored. An old runtime
///    that does not know about a newly appended field will decode the fields it recognizes and
///    discard the rest. This is intentional behavior in `sp_api` — see the generated code in
///    `substrate/primitives/api/proc-macro/src/impl_runtime_apis.rs`.
///
/// 2. **Old RPC, new runtime (missing bytes are defaulted):** A new runtime expecting more fields
///    than an old RPC provides would hit EOF during decoding and fail. To guard against this, this
///    type uses a **custom `Decode` implementation** that falls back to `Default` for any trailing
///    fields that are absent from the input. This ensures that an old RPC sending a shorter
///    encoding is handled gracefully.
///
/// ## Constraints on fields
///
/// - New fields **must** be appended to the end. Inserting or reordering fields changes the byte
///   layout of all subsequent fields, breaking both directions.
/// - New fields **must** implement `Default` so that the custom `Decode` fallback can produce a
///   sensible value when the field is absent from the input. This is the only requirement on the
///   field's type — it does not need to be `Option`.
/// - This pattern relies on `sp_api` continuing to use `Decode::decode` rather than `decode_all`.
///   If that ever changes, a new runtime API method would be needed instead.
///
/// ## Constraints on runtime API placement
///
/// The trailing-bytes trick described in point 1 above only works because `sp_api` discards
/// unconsumed bytes **at the end of the entire argument buffer**. This means `DryRunConfig`
/// must be the **last argument** of any runtime API method that uses it (which is currently
/// the case for both `eth_transact_with_config` and `eth_estimate_gas`). If it were placed
/// before another argument, the extra bytes from newly appended fields would shift the
/// decoding offset and corrupt the subsequent argument.
#[derive(Debug, Encode, TypeInfo, Clone)]
pub struct DryRunConfig<Moment> {
	/// Optional timestamp override for dry-run in pending block.
	pub timestamp_override: Option<Moment>,
	/// Used to control if the dry run logic should perform the balance checks or not.
	pub perform_balance_checks: Option<bool>,
	/// Optional state overrides to apply before executing the call. Each entry maps an account
	/// address to a set of fields (balance, nonce, code, storage) that should be temporarily
	/// replaced for the duration of the dry-run.
	pub state_overrides: Option<StateOverrideSet>,
}

impl<Moment> Default for DryRunConfig<Moment> {
	fn default() -> Self {
		Self { timestamp_override: None, perform_balance_checks: Some(true), state_overrides: None }
	}
}

/// A custom implementation of [`Decode`] to ensure forward and backward compatibility of the
/// [`DryRunConfig`] type.
///
/// # Backwards Compatibility
///
/// Please review the documentation on the [`DryRunConfig`] for more information about how we
/// manage and handle compatibility for this type and instructions on what you should do when adding
/// a new field to this type.
impl<Moment: Decode> Decode for DryRunConfig<Moment> {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let timestamp_override = Option::<Moment>::decode(input)?;
		let perform_balance_checks = Option::<bool>::decode(input)?;
		let state_overrides = Option::<StateOverrideSet>::decode(input).unwrap_or_default();
		Ok(Self { timestamp_override, perform_balance_checks, state_overrides })
	}
}

impl<Moment> DryRunConfig<Moment> {
	/// Create a new `DryRunConfig` with default values.
	///
	/// Balance checks are enabled by default. Use the builder methods to customize.
	pub fn new() -> Self {
		Self::default()
	}

	/// A builder method which consumes the object and modifies the `timestamp_override` field.
	pub fn with_timestamp_override(
		mut self,
		timestamp_override: impl Into<Option<Moment>>,
	) -> Self {
		self.timestamp_override = timestamp_override.into();
		self
	}

	/// A builder method which consumes the object and modifies the `perform_balance_checks` field.
	pub fn with_perform_balance_checks(
		mut self,
		perform_balance_checks: impl Into<Option<bool>>,
	) -> Self {
		self.perform_balance_checks = perform_balance_checks.into();
		self
	}

	/// A builder method which consumes the object and sets the state overrides.
	pub fn with_state_overrides(
		mut self,
		state_overrides: impl Into<Option<StateOverrideSet>>,
	) -> Self {
		self.state_overrides = state_overrides.into();
		self
	}
}

/// Configuration specific to a tracing execution.
///
/// Passed as the last argument to the `trace_call_with_config` runtime API method. Contains
/// optional overrides that affect how the traced execution is performed.
///
/// # Backwards Compatibility
///
/// This type follows the same backwards compatibility strategy as [`DryRunConfig`]. SCALE is a
/// non-self-describing format: fields are encoded sequentially with no names or delimiters. This
/// type uses a custom [`Decode`] implementation that defaults missing trailing fields, and relies
/// on `sp_api`'s use of `Decode::decode` (not `decode_all`) to silently discard trailing bytes
/// that an old runtime does not recognize.
///
/// ## Constraints on fields
///
/// - New fields **must** be appended to the end. Inserting or reordering fields breaks the byte
///   layout in both directions.
/// - New fields **must** implement `Default` so the custom `Decode` fallback can produce a sensible
///   value when the field is absent from the input.
///
/// ## Constraints on runtime API placement
///
/// `TracingConfig` must be the **last argument** of any runtime API method that uses it. If it
/// were placed before another argument, extra bytes from newly appended fields would shift the
/// decoding offset and corrupt the subsequent argument.
#[derive(Debug, Default, Encode, TypeInfo, Clone)]
pub struct TracingConfig {
	/// Optional state overrides to apply before executing the traced call. Each entry maps an
	/// account address to a set of fields (balance, nonce, code, storage) that should be
	/// temporarily replaced for the duration of the trace.
	pub state_overrides: Option<StateOverrideSet>,
}

/// A custom implementation of [`Decode`] to ensure forward and backward compatibility of the
/// [`TracingConfig`] type.
///
/// # Backwards Compatibility
///
/// Please review the documentation on [`TracingConfig`] for more information about how we manage
/// and handle compatibility for this type and instructions on what you should do when adding a
/// new field.
impl Decode for TracingConfig {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let state_overrides = Option::<StateOverrideSet>::decode(input).unwrap_or_default();
		Ok(Self { state_overrides })
	}
}

impl TracingConfig {
	/// Create a new `TracingConfig` with default values.
	pub fn new() -> Self {
		Self::default()
	}

	/// A builder method which consumes the object and sets the state overrides.
	pub fn with_state_overrides(
		mut self,
		state_overrides: impl Into<Option<StateOverrideSet>>,
	) -> Self {
		self.state_overrides = state_overrides.into();
		self
	}
}

impl From<BlockNumberOrTag> for BlockNumberOrTagOrHash {
	fn from(b: BlockNumberOrTag) -> Self {
		match b {
			BlockNumberOrTag::U256(n) => BlockNumberOrTagOrHash::BlockNumber(n),
			BlockNumberOrTag::BlockTag(t) => BlockNumberOrTagOrHash::BlockTag(t),
		}
	}
}

impl From<TransactionSigned> for TransactionUnsigned {
	fn from(tx: TransactionSigned) -> Self {
		use TransactionSigned::*;
		match tx {
			Transaction7702Signed(tx) => tx.transaction_7702_unsigned.into(),
			Transaction4844Signed(tx) => tx.transaction_4844_unsigned.into(),
			Transaction1559Signed(tx) => tx.transaction_1559_unsigned.into(),
			Transaction2930Signed(tx) => tx.transaction_2930_unsigned.into(),
			TransactionLegacySigned(tx) => tx.transaction_legacy_unsigned.into(),
		}
	}
}

#[test]
fn can_deserialize_input_or_data_field_from_generic_transaction() {
	let cases = [
		("with input", r#"{"input": "0x01"}"#),
		("with data", r#"{"data": "0x01"}"#),
		("with both", r#"{"data": "0x01", "input": "0x01"}"#),
	];

	for (name, json) in cases {
		let tx = serde_json::from_str::<GenericTransaction>(json).unwrap();
		assert_eq!(tx.input.to_vec(), vec![1u8], "{}", name);
	}

	let err = serde_json::from_str::<GenericTransaction>(r#"{"data": "0x02", "input": "0x01"}"#)
		.unwrap_err();
	assert!(
		err.to_string().starts_with(
		"Both \"data\" and \"input\" are set and not equal. Please use \"input\" to pass transaction call data"
		)
	);
}

#[test]
fn test_block_number_or_tag_or_hash_deserialization() {
	let val: BlockNumberOrTagOrHash = serde_json::from_str("\"latest\"").unwrap();
	assert_eq!(val, BlockTag::Latest.into());

	for s in ["\"0x1a\"", r#"{ "blockNumber": "0x1a" }"#] {
		let val: BlockNumberOrTagOrHash = serde_json::from_str(s).unwrap();
		assert!(matches!(val, BlockNumberOrTagOrHash::BlockNumber(n) if n == 26u64.into()));
	}

	for s in [
		"\"0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"",
		r#"{ "blockHash": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" }"#,
	] {
		let val: BlockNumberOrTagOrHash = serde_json::from_str(s).unwrap();
		assert_eq!(val, BlockNumberOrTagOrHash::BlockHash(H256([0xaau8; 32])));
	}
}

impl GenericTransaction {
	/// Create a new [`GenericTransaction`] from a signed transaction.
	pub fn from_signed(tx: TransactionSigned, base_gas_price: U256, from: Option<H160>) -> Self {
		Self::from_unsigned(tx.into(), base_gas_price, from)
	}

	/// The gas price that is actually paid (including priority fee).
	pub fn effective_gas_price(&self, base_gas_price: U256) -> Option<U256> {
		let effective_gas_price = if let Some(prio_price) = self.max_priority_fee_per_gas {
			let max_price = self.max_fee_per_gas?;
			Some(max_price.min(base_gas_price.saturating_add(prio_price)))
		} else {
			self.gas_price
		};

		// we do not implement priority fee as it does not map to tip well
		// hence the effective gas price cannot be higher than the base price
		effective_gas_price.map(|e| e.min(base_gas_price))
	}

	/// Create a new [`GenericTransaction`] from a unsigned transaction.
	pub fn from_unsigned(
		tx: TransactionUnsigned,
		base_gas_price: U256,
		from: Option<H160>,
	) -> Self {
		use TransactionUnsigned::*;
		let mut tx = match tx {
			TransactionLegacyUnsigned(tx) => GenericTransaction {
				from,
				r#type: Some(tx.r#type.as_byte()),
				chain_id: tx.chain_id,
				input: tx.input.into(),
				nonce: Some(tx.nonce),
				value: Some(tx.value),
				to: tx.to,
				gas: Some(tx.gas),
				gas_price: Some(tx.gas_price),
				..Default::default()
			},
			Transaction4844Unsigned(tx) => GenericTransaction {
				from,
				r#type: Some(tx.r#type.as_byte()),
				chain_id: Some(tx.chain_id),
				input: tx.input.into(),
				nonce: Some(tx.nonce),
				value: Some(tx.value),
				to: Some(tx.to),
				gas: Some(tx.gas),
				access_list: Some(tx.access_list),
				blob_versioned_hashes: tx.blob_versioned_hashes,
				max_fee_per_blob_gas: Some(tx.max_fee_per_blob_gas),
				max_fee_per_gas: Some(tx.max_fee_per_gas),
				max_priority_fee_per_gas: Some(tx.max_priority_fee_per_gas),
				..Default::default()
			},
			Transaction1559Unsigned(tx) => GenericTransaction {
				from,
				r#type: Some(tx.r#type.as_byte()),
				chain_id: Some(tx.chain_id),
				input: tx.input.into(),
				nonce: Some(tx.nonce),
				value: Some(tx.value),
				to: tx.to,
				gas: Some(tx.gas),
				access_list: Some(tx.access_list),
				max_fee_per_gas: Some(tx.max_fee_per_gas),
				max_priority_fee_per_gas: Some(tx.max_priority_fee_per_gas),
				..Default::default()
			},
			Transaction2930Unsigned(tx) => GenericTransaction {
				from,
				r#type: Some(tx.r#type.as_byte()),
				chain_id: Some(tx.chain_id),
				input: tx.input.into(),
				nonce: Some(tx.nonce),
				value: Some(tx.value),
				to: tx.to,
				gas: Some(tx.gas),
				gas_price: Some(tx.gas_price),
				access_list: Some(tx.access_list),
				..Default::default()
			},
			Transaction7702Unsigned(tx) => GenericTransaction {
				from,
				r#type: Some(tx.r#type.as_byte()),
				chain_id: Some(tx.chain_id),
				input: tx.input.into(),
				nonce: Some(tx.nonce),
				value: Some(tx.value),
				to: Some(tx.to),
				gas: Some(tx.gas),
				access_list: Some(tx.access_list),
				authorization_list: tx.authorization_list,
				max_fee_per_gas: Some(tx.max_fee_per_gas),
				max_priority_fee_per_gas: Some(tx.max_priority_fee_per_gas),
				..Default::default()
			},
		};
		tx.gas_price = tx.effective_gas_price(base_gas_price);
		tx
	}

	/// Convert to a [`TransactionUnsigned`].
	pub fn try_into_unsigned(self) -> Result<TransactionUnsigned, ()> {
		match self.r#type.unwrap_or_default().0 {
			TYPE_LEGACY => Ok(TransactionLegacyUnsigned {
				r#type: TypeLegacy {},
				chain_id: self.chain_id,
				input: self.input.to_bytes(),
				nonce: self.nonce.unwrap_or_default(),
				value: self.value.unwrap_or_default(),
				to: self.to,
				gas: self.gas.unwrap_or_default(),
				gas_price: self.gas_price.unwrap_or_default(),
			}
			.into()),
			TYPE_EIP1559 => Ok(Transaction1559Unsigned {
				r#type: TypeEip1559 {},
				chain_id: self.chain_id.unwrap_or_default(),
				input: self.input.to_bytes(),
				nonce: self.nonce.unwrap_or_default(),
				value: self.value.unwrap_or_default(),
				to: self.to,
				gas: self.gas.unwrap_or_default(),
				gas_price: self.max_fee_per_gas.unwrap_or_default(),
				access_list: self.access_list.unwrap_or_default(),
				max_fee_per_gas: self.max_fee_per_gas.unwrap_or_default(),
				max_priority_fee_per_gas: self.max_priority_fee_per_gas.unwrap_or_default(),
			}
			.into()),
			TYPE_EIP2930 => Ok(Transaction2930Unsigned {
				r#type: TypeEip2930 {},
				chain_id: self.chain_id.unwrap_or_default(),
				input: self.input.to_bytes(),
				nonce: self.nonce.unwrap_or_default(),
				value: self.value.unwrap_or_default(),
				to: self.to,
				gas: self.gas.unwrap_or_default(),
				gas_price: self.gas_price.unwrap_or_default(),
				access_list: self.access_list.unwrap_or_default(),
			}
			.into()),
			TYPE_EIP4844 => Ok(Transaction4844Unsigned {
				r#type: TypeEip4844 {},
				chain_id: self.chain_id.unwrap_or_default(),
				input: self.input.to_bytes(),
				nonce: self.nonce.unwrap_or_default(),
				value: self.value.unwrap_or_default(),
				to: self.to.unwrap_or_default(),
				gas: self.gas.unwrap_or_default(),
				max_fee_per_gas: self.max_fee_per_gas.unwrap_or_default(),
				max_fee_per_blob_gas: self.max_fee_per_blob_gas.unwrap_or_default(),
				max_priority_fee_per_gas: self.max_priority_fee_per_gas.unwrap_or_default(),
				access_list: self.access_list.unwrap_or_default(),
				blob_versioned_hashes: self.blob_versioned_hashes,
			}
			.into()),
			TYPE_EIP7702 => Ok(Transaction7702Unsigned {
				r#type: TypeEip7702 {},
				chain_id: self.chain_id.unwrap_or_default(),
				input: self.input.to_bytes(),
				nonce: self.nonce.unwrap_or_default(),
				value: self.value.unwrap_or_default(),
				to: self.to.unwrap_or_default(),
				gas: self.gas.unwrap_or_default(),
				max_fee_per_gas: self.max_fee_per_gas.unwrap_or_default(),
				max_priority_fee_per_gas: self.max_priority_fee_per_gas.unwrap_or_default(),
				access_list: self.access_list.unwrap_or_default(),
				authorization_list: self.authorization_list,
			}
			.into()),
			_ => Err(()),
		}
	}
}

#[test]
fn from_unsigned_works_for_legacy() {
	let base_gas_price = U256::from(10);
	let tx = TransactionUnsigned::from(TransactionLegacyUnsigned {
		chain_id: Some(U256::from(1)),
		input: Bytes::from(vec![1u8]),
		nonce: U256::from(1),
		value: U256::from(1),
		to: Some(H160::zero()),
		gas: U256::from(1),
		gas_price: U256::from(10),
		..Default::default()
	});

	let generic = GenericTransaction::from_unsigned(tx.clone(), base_gas_price, None);
	assert_eq!(generic.gas_price, Some(U256::from(10)));

	let tx2 = generic.try_into_unsigned().unwrap();
	assert_eq!(tx, tx2);
}

#[test]
fn from_unsigned_works_for_1559() {
	let base_gas_price = U256::from(10);
	let tx = TransactionUnsigned::from(Transaction1559Unsigned {
		chain_id: U256::from(1),
		input: Bytes::from(vec![1u8]),
		nonce: U256::from(1),
		value: U256::from(1),
		to: Some(H160::zero()),
		gas: U256::from(1),
		gas_price: U256::from(20),
		max_fee_per_gas: U256::from(20),
		max_priority_fee_per_gas: U256::from(1),
		..Default::default()
	});

	let generic = GenericTransaction::from_unsigned(tx.clone(), base_gas_price, None);
	assert_eq!(generic.gas_price, Some(U256::from(10)));

	let tx2 = generic.try_into_unsigned().unwrap();
	assert_eq!(tx, tx2);
}

#[test]
fn from_unsigned_works_for_7702() {
	let base_gas_price = U256::from(10);
	let tx = TransactionUnsigned::from(Transaction7702Unsigned {
		chain_id: U256::from(1),
		input: Bytes::from(vec![1u8]),
		nonce: U256::from(1),
		value: U256::from(1),
		to: H160::zero(),
		gas: U256::from(1),
		max_fee_per_gas: U256::from(20),
		max_priority_fee_per_gas: U256::from(1),
		authorization_list: vec![AuthorizationListEntry {
			chain_id: U256::from(1),
			address: H160::from_low_u64_be(42),
			nonce: U256::from(0),
			y_parity: U256::from(1),
			r: U256::from(1),
			s: U256::from(2),
		}],
		..Default::default()
	});

	let generic = GenericTransaction::from_unsigned(tx.clone(), base_gas_price, None);
	assert_eq!(generic.gas_price, Some(U256::from(10)));

	let tx2 = generic.try_into_unsigned().unwrap();
	assert_eq!(tx, tx2);
}
