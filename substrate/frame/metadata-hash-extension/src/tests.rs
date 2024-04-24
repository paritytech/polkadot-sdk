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

use crate::CheckMetadataHash;
use codec::{Decode, Encode};
use frame_support::derive_impl;
use sp_runtime::{traits::SignedExtension, transaction_validity::UnknownTransaction};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime! {
	pub enum Test {
		System: frame_system,
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

#[test]
fn rejects_when_no_metadata_hash_was_passed() {
	let ext = CheckMetadataHash::<Test>::decode(&mut &1u8.encode()[..]).unwrap();
	assert_eq!(Err(UnknownTransaction::CannotLookup.into()), ext.additional_signed());
}

#[test]
fn rejects_unknown_mode() {
	let ext = CheckMetadataHash::<Test>::decode(&mut &50u8.encode()[..]).unwrap();
	assert_eq!(Err(UnknownTransaction::CannotLookup.into()), ext.additional_signed());
}

#[test]
fn when_metadata_check_is_disabled_it_encodes_to_nothing() {
	let ext = CheckMetadataHash::<Test>::decode(&mut &0u8.encode()[..]).unwrap();
	assert!(ext.additional_signed().unwrap().encode().is_empty());
}

#[docify::export]
#[test]
// The test can not be executed, because the wasm builder would
// fail in this test context.
#[ignore]
fn enable_metadata_hash_in_wasm_builder() {
	substrate_wasm_builder::WasmBuilder::init_with_defaults()
		// Requires the `metadata-hash` feature to be activated.
		// You need to pass the main token symbol and its number of decimals.
		.enable_metadata_hash("TOKEN", 12)
		// The runtime will be build twice and the second time the `RUNTIME_METADATA_HASH`
		// environment variable will be set for the `CheckMetadataHash` extension.
		.build()
}
