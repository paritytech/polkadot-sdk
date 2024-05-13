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
use frame_metadata::RuntimeMetadataPrefixed;
use frame_support::{
	derive_impl,
	pallet_prelude::{InvalidTransaction, TransactionValidityError},
};
use merkleized_metadata::{generate_metadata_digest, ExtraInfo};
use sp_api::ProvideRuntimeApi;
use sp_runtime::{
	traits::{Extrinsic as _, SignedExtension},
	transaction_validity::{TransactionSource, UnknownTransaction},
};
use sp_transaction_pool::runtime_api::TaggedTransactionQueue;
use substrate_test_runtime_client::{
	prelude::*,
	runtime::{self, ExtrinsicBuilder},
	DefaultTestClientBuilderExt, TestClientBuilder,
};

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

/// Generate the metadata hash for the `test-runtime`.
fn generate_metadata_hash() -> [u8; 32] {
	let metadata = runtime::Runtime::metadata_at_version(15).unwrap();

	let metadata = RuntimeMetadataPrefixed::decode(&mut &metadata[..])
		.expect("Invalid encoded metadata?")
		.1;

	let runtime_version = runtime::VERSION;
	let base58_prefix = 0;

	let extra_info = ExtraInfo {
		spec_version: runtime_version.spec_version,
		spec_name: runtime_version.spec_name.into(),
		base58_prefix,
		decimals: 10,
		token_symbol: "TOKEN".into(),
	};

	generate_metadata_digest(&metadata, extra_info).unwrap().hash()
}

#[test]
fn ensure_check_metadata_works_on_real_extrinsics() {
	let client = TestClientBuilder::new().build();
	let runtime_api = client.runtime_api();
	let best_hash = client.chain_info().best_hash;

	let valid_transaction = ExtrinsicBuilder::new_include_data(vec![1, 2, 3])
		.metadata_hash(generate_metadata_hash())
		.build();
	// Ensure that the transaction is signed.
	assert!(valid_transaction.is_signed().unwrap());

	assert!(runtime_api
		.validate_transaction(best_hash, TransactionSource::External, valid_transaction, best_hash)
		.unwrap()
		.is_ok());

	// Including some random metadata hash should make the transaction invalid.
	let invalid_transaction = ExtrinsicBuilder::new_include_data(vec![1, 2, 3])
		.metadata_hash([10u8; 32])
		.build();
	// Ensure that the transaction is signed.
	assert!(invalid_transaction.is_signed().unwrap());

	assert_eq!(
		TransactionValidityError::from(InvalidTransaction::BadProof),
		runtime_api
			.validate_transaction(
				best_hash,
				TransactionSource::External,
				invalid_transaction,
				best_hash
			)
			.unwrap()
			.unwrap_err()
	);
}

#[allow(unused)]
mod docs {
	use super::*;

	#[docify::export]
	mod add_metadata_hash_extension {
		frame_support::construct_runtime! {
			pub enum Runtime {
				System: frame_system,
			}
		}

		/// The `SignedExtension` to the basic transaction logic.
		pub type SignedExtra = (
			frame_system::CheckNonZeroSender<Runtime>,
			frame_system::CheckSpecVersion<Runtime>,
			frame_system::CheckTxVersion<Runtime>,
			frame_system::CheckGenesis<Runtime>,
			frame_system::CheckMortality<Runtime>,
			frame_system::CheckNonce<Runtime>,
			frame_system::CheckWeight<Runtime>,
			// Add the `CheckMetadataHash` extension.
			// The position in this list is not important, so we could also add it to beginning.
			frame_metadata_hash_extension::CheckMetadataHash<Runtime>,
		);

		/// In your runtime this will be your real address type.
		type Address = ();
		/// In your runtime this will be your real signature type.
		type Signature = ();

		/// Unchecked extrinsic type as expected by this runtime.
		pub type UncheckedExtrinsic =
			sp_runtime::generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;
	}

	// Put here to not have it in the docs as well.
	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for add_metadata_hash_extension::Runtime {
		type Block = Block;
		type RuntimeEvent = add_metadata_hash_extension::RuntimeEvent;
		type RuntimeOrigin = add_metadata_hash_extension::RuntimeOrigin;
		type RuntimeCall = add_metadata_hash_extension::RuntimeCall;
		type PalletInfo = add_metadata_hash_extension::PalletInfo;
	}

	#[docify::export]
	fn enable_metadata_hash_in_wasm_builder() {
		substrate_wasm_builder::WasmBuilder::init_with_defaults()
			// Requires the `metadata-hash` feature to be activated.
			// You need to pass the main token symbol and its number of decimals.
			.enable_metadata_hash("TOKEN", 12)
			// The runtime will be build twice and the second time the `RUNTIME_METADATA_HASH`
			// environment variable will be set for the `CheckMetadataHash` extension.
			.build()
	}
}
