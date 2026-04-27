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

extern crate alloc;

use pallet_revive_proc_macro::define_versioned_interface;

define_versioned_interface! {
	#[derive(Clone, Debug, PartialEq)]
	pub struct EthTransactInputPayloadV1<T: Clone> {
		pub tx: u8,
		pub marker: T,
	}

	#[derive(Clone, Debug, PartialEq)]
	pub struct EthTransactOutputPayloadV1 {
		pub result: u8,
	}

	#[derive(Clone, Debug, PartialEq)]
	pub struct EthTransactInputPayloadV2<T: Default>
	where
		T: Clone,
	{
		pub tx: u8,
		pub marker: T,
		pub timestamp: u64,
	}

	#[derive(Clone, Debug, PartialEq)]
	pub struct EthTransactOutputPayloadV2 {
		pub result: u16,
	}
}

define_versioned_interface! {
	#[derive(Clone, Debug, PartialEq)]
	pub struct TransferInputPayloadV4 {
		pub account: u64,
		pub amount: u128,
		pub memo: Option<&'static str>,
	}

	#[derive(Clone, Debug, PartialEq)]
	pub struct TransferOutputPayloadV4 {
		pub accepted: bool,
		pub receipt: Option<u64>,
	}

	#[derive(Clone, Debug, PartialEq)]
	pub struct TransferInputPayloadV3 {
		pub account: u64,
		pub amount: u128,
	}

	#[derive(Clone, Debug, PartialEq)]
	pub struct TransferOutputPayloadV3 {
		pub accepted: bool,
	}
}

define_versioned_interface! {
	#[derive(Clone, Debug, PartialEq, Eq)]
	pub struct AuditInputPayloadV1 {
		pub id: u64,
	}

	#[derive(Debug, PartialEq, Eq)]
	pub struct AuditOutputPayloadV1 {
		pub ok: bool,
	}

	#[derive(Clone, Debug, PartialEq, Eq)]
	pub struct AuditInputPayloadV2 {
		pub id: u64,
		pub tag: &'static str,
	}

	#[derive(Debug, PartialEq, Eq)]
	pub struct AuditOutputPayloadV2 {
		pub ok: bool,
		pub code: u16,
	}
}

/// A boundary type that intentionally has no runtime `Config` trait bound.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct NoRuntimeConfig {
	/// An arbitrary payload value carried through generic payload structs.
	value: u8,
}

define_versioned_interface! {
	#[derive(Clone, Debug, PartialEq)]
	pub struct QueryInputPayloadV1<'a, T: Clone>
	where
		T: PartialEq,
	{
		pub key: &'a T,
	}

	#[derive(Clone, Debug, PartialEq)]
	pub struct QueryOutputPayloadV1<R>
	where
		R: Clone + PartialEq,
	{
		pub value: Option<R>,
	}

	#[derive(Clone, Debug, PartialEq)]
	pub struct QueryInputPayloadV2<'a, T: Default, const N: usize>
	where
		T: Clone + PartialEq,
	{
		pub key: T,
		pub borrowed: Option<&'a T>,
		pub bytes: [u8; N],
	}

	#[derive(Clone, Debug, PartialEq)]
	pub struct QueryOutputPayloadV2<R, E>
	where
		R: Clone + PartialEq,
		E: Clone + PartialEq,
	{
		pub value: Option<R>,
		pub error: Option<E>,
	}
}

define_versioned_interface! {
	#[derive(Clone, Debug, PartialEq)]
	pub struct SingleInputPayloadV7 {
		pub value: u8,
	}

	#[derive(Clone, Debug, PartialEq)]
	pub struct SingleOutputPayloadV7 {
		pub value: u8,
	}
}

#[test]
fn function_like_macro_expands_versioned_interface_input_helpers() {
	// Arrange
	let payload = EthTransactInputPayloadV1 { tx: 1, marker: 2u8 };

	// Act
	let versioned = VersionedEthTransactInputPayload::new_v1(payload.clone());

	// Assert
	assert_eq!(versioned.version(), 1);
	assert_eq!(versioned.as_v1(), Some(&payload));
	assert_eq!(versioned.clone().into_v1(), Some(payload.clone()));
	assert_eq!(versioned.unwrap_v1(), payload);
}

#[test]
fn function_like_macro_expands_versioned_interface_output_helpers() {
	// Arrange
	let payload = EthTransactOutputPayloadV2 { result: 2 };

	// Act
	let versioned = VersionedEthTransactOutputPayload::new_v2(payload.clone());

	// Assert
	assert_eq!(versioned.version(), 2);
	assert_eq!(versioned.as_v2(), Some(&payload));
	assert_eq!(versioned.clone().into_v2(), Some(payload.clone()));
	assert_eq!(versioned.unwrap_v2(), payload);
}

#[test]
fn function_like_macro_supports_out_of_order_non_one_versions_and_mismatch_accessors() {
	// Arrange
	let payload_v3 = TransferInputPayloadV3 { account: 7, amount: 9 };
	let payload_v4 = TransferInputPayloadV4 { account: 7, amount: 9, memo: Some("memo") };

	// Act
	let versioned_v3 = VersionedTransferInputPayload::new_v3(payload_v3.clone());
	let versioned_v4 = VersionedTransferInputPayload::new_v4(payload_v4.clone());

	// Assert
	assert_eq!(versioned_v3.version(), 3);
	assert_eq!(versioned_v4.version(), 4);
	assert_eq!(versioned_v3.as_v3(), Some(&payload_v3));
	assert_eq!(versioned_v3.as_v4(), None);
	assert_eq!(versioned_v4.clone().into_v3(), None);
	assert_eq!(versioned_v4.clone().into_v4(), Some(payload_v4.clone()));
	assert_eq!(versioned_v4.unwrap_v4(), payload_v4);
}

#[test]
fn function_like_macro_panics_when_unwrapping_a_different_version() {
	// Arrange
	let payload = TransferInputPayloadV4 { account: 7, amount: 9, memo: None };
	let versioned = VersionedTransferInputPayload::new_v4(payload);

	// Act
	let panic = std::panic::catch_unwind(|| versioned.unwrap_v3()).unwrap_err();

	// Assert
	let message = panic
		.downcast_ref::<String>()
		.map(String::as_str)
		.or_else(|| panic.downcast_ref::<&'static str>().copied())
		.unwrap();
	assert!(message.contains("Expected this to be a v3 variant, but it is a v4 variant"));
}

#[test]
fn function_like_macro_exposes_public_boxed_variants() {
	// Arrange
	let input = TransferInputPayloadV3 { account: 8, amount: 10 };
	let output = TransferOutputPayloadV3 { accepted: true };

	// Act
	let versioned_input = VersionedTransferInputPayload::V3(Box::new(input.clone()));
	let versioned_output = VersionedTransferOutputPayload::V3(Box::new(output.clone()));

	// Assert
	assert_eq!(versioned_input.into_v3(), Some(input));
	assert_eq!(versioned_output.into_v3(), Some(output));
}

#[test]
fn function_like_macro_copies_common_derives_separately_for_each_side() {
	// Arrange
	let input = VersionedAuditInputPayload::new_v2(AuditInputPayloadV2 { id: 1, tag: "tag" });
	let output = VersionedAuditOutputPayload::new_v2(AuditOutputPayloadV2 { ok: true, code: 3 });

	// Act
	let cloned_input = input.clone();
	let formatted_output = format!("{output:?}");

	// Assert
	assert_eq!(cloned_input, input);
	assert!(formatted_output.contains("V2"));
}

#[test]
fn function_like_macro_merges_generics_side_locally_without_runtime_config_bounds() {
	// Arrange
	let key = NoRuntimeConfig { value: 7 };
	let borrowed = NoRuntimeConfig { value: 8 };
	let input_v1 = QueryInputPayloadV1 { key: &key };
	let input_v2 =
		QueryInputPayloadV2 { key: key.clone(), borrowed: Some(&borrowed), bytes: [1; 4] };
	let output_v2 =
		QueryOutputPayloadV2 { value: Some("ok".to_owned()), error: Option::<u16>::None };

	// Act
	let versioned_input_v1: VersionedQueryInputPayload<'_, NoRuntimeConfig, 4> =
		VersionedQueryInputPayload::new_v1(input_v1.clone());
	let versioned_input_v2 = VersionedQueryInputPayload::new_v2(input_v2.clone());
	let versioned_output_v2: VersionedQueryOutputPayload<String, u16> =
		VersionedQueryOutputPayload::new_v2(output_v2.clone());

	// Assert
	assert_eq!(versioned_input_v1.as_v1(), Some(&input_v1));
	assert_eq!(versioned_input_v2.unwrap_v2(), input_v2);
	assert_eq!(versioned_output_v2.unwrap_v2(), output_v2);
}

#[test]
fn function_like_macro_accepts_a_single_payload_version_starting_after_v1() {
	// Arrange
	let input = SingleInputPayloadV7 { value: 7 };
	let output = SingleOutputPayloadV7 { value: 8 };

	// Act
	let versioned_input = VersionedSingleInputPayload::new_v7(input.clone());
	let versioned_output = VersionedSingleOutputPayload::new_v7(output.clone());

	// Assert
	assert_eq!(versioned_input.version(), 7);
	assert_eq!(versioned_output.version(), 7);
	assert_eq!(versioned_input.unwrap_v7(), input);
	assert_eq!(versioned_output.unwrap_v7(), output);
}

#[test]
fn from_payload_wraps_into_matching_versioned_variant() {
	// Arrange
	let payload_v1 = EthTransactInputPayloadV1 { tx: 1, marker: 2u8 };
	let payload_v2 = EthTransactInputPayloadV2 { tx: 3, marker: 4u8, timestamp: 5 };

	// Act
	let versioned_v1 = VersionedEthTransactInputPayload::<u8>::from(payload_v1.clone());
	let versioned_v2 = VersionedEthTransactInputPayload::<u8>::from(payload_v2.clone());

	// Assert
	assert_eq!(versioned_v1.version(), 1);
	assert_eq!(versioned_v1.as_v1(), Some(&payload_v1));
	assert_eq!(versioned_v2.version(), 2);
	assert_eq!(versioned_v2.as_v2(), Some(&payload_v2));
}

#[test]
fn try_from_versioned_returns_payload_when_variant_matches() {
	// Arrange
	let payload = EthTransactInputPayloadV2 { tx: 7, marker: 9u8, timestamp: 11 };
	let versioned = VersionedEthTransactInputPayload::<u8>::from(payload.clone());

	// Act
	let extracted = EthTransactInputPayloadV2::<u8>::try_from(versioned);

	// Assert
	assert_eq!(extracted, Ok(payload));
}

#[test]
fn try_from_versioned_returns_unit_error_when_variant_differs() {
	// Arrange
	let payload = EthTransactInputPayloadV1 { tx: 7, marker: 9u8 };
	let versioned = VersionedEthTransactInputPayload::<u8>::from(payload);

	// Act
	let extracted = EthTransactInputPayloadV2::<u8>::try_from(versioned);

	// Assert
	assert_eq!(extracted, Err(()));
}

#[test]
fn from_output_payload_wraps_into_matching_versioned_variant() {
	// Arrange
	let payload_v3 = TransferOutputPayloadV3 { accepted: true };
	let payload_v4 = TransferOutputPayloadV4 { accepted: false, receipt: Some(42) };

	// Act
	let versioned_v3 = VersionedTransferOutputPayload::from(payload_v3.clone());
	let versioned_v4 = VersionedTransferOutputPayload::from(payload_v4.clone());

	// Assert
	assert_eq!(versioned_v3.version(), 3);
	assert_eq!(versioned_v3.as_v3(), Some(&payload_v3));
	assert_eq!(versioned_v4.version(), 4);
	assert_eq!(versioned_v4.as_v4(), Some(&payload_v4));
}

#[test]
fn try_from_output_versioned_returns_unit_error_when_variant_differs() {
	// Arrange
	let payload = TransferOutputPayloadV3 { accepted: true };
	let versioned = VersionedTransferOutputPayload::from(payload);

	// Act
	let extracted = TransferOutputPayloadV4::try_from(versioned);

	// Assert
	assert_eq!(extracted, Err(()));
}

#[test]
fn try_from_versioned_returns_payload_for_single_variant_enum() {
	// Arrange
	let payload = SingleInputPayloadV7 { value: 17 };
	let versioned = VersionedSingleInputPayload::from(payload.clone());

	// Act
	let extracted = SingleInputPayloadV7::try_from(versioned);

	// Assert
	assert_eq!(extracted, Ok(payload));
}
