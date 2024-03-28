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

#![cfg_attr(not(feature = "std"), no_std)]

use frame::{
	deps::frame_support::genesis_builder_helper::{build_config, create_default_config},
	prelude::*,
	runtime::{
		apis::{self, *},
		prelude::*,
	},
};
use pallet_transaction_payment_rpc_runtime_api::{FeeDetails, RuntimeDispatchInfo};

use first_pallet::pallet_v2 as our_first_pallet;

#[docify::export]
#[runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: create_runtime_str!("first-runtime"),
	impl_name: create_runtime_str!("first-runtime"),
	authoring_version: 1,
	spec_version: 0,
	impl_version: 1,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 1,
	state_version: 1,
};

#[docify::export_content]
#[cfg(feature = "std")]
mod native_only_setup {
	use super::*;
	/// The version information used to identify this runtime when compiled natively.
	pub fn native_version() -> NativeVersion {
		NativeVersion { runtime_version: VERSION, can_author_with: Default::default() }
	}

	// Make the WASM binary available.
	include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}
#[cfg(feature = "std")]
pub use native_only_setup::*;

#[docify::export(cr)]
construct_runtime!(
	pub struct Runtime {
		// Mandatory for all runtimes
		System: frame_system,

		// A number of other pallets from FRAME.
		Timestamp: pallet_timestamp,
		Balances: pallet_balances,
		Sudo: pallet_sudo,
		TransactionPayment: pallet_transaction_payment,

		// Our local pallet
		FirstPallet: our_first_pallet,
	}
);

#[docify::export_content]
mod runtime_types {
	use super::*;
	pub(super) type SignedExtra = (
		// `frame` already provides all the signed extensions from `frame-system`. We just add the
		// one related to tx-payment here.
		frame::runtime::types_common::SystemSignedExtensionsOf<Runtime>,
		pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
	);

	pub(super) type Block = frame::runtime::types_common::BlockOf<Runtime, SignedExtra>;
	pub(super) type Header = HeaderFor<Runtime>;

	pub(super) type RuntimeExecutive = Executive<
		Runtime,
		Block,
		frame_system::ChainContext<Runtime>,
		Runtime,
		AllPalletsWithSystem,
	>;
}
use runtime_types::*;

#[docify::export_content]
mod config_impls {
	use super::*;

	parameter_types! {
		pub const Version: RuntimeVersion = VERSION;
	}

	#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
	impl frame_system::Config for Runtime {
		type Block = Block;
		type Version = Version;
		type AccountData =
			pallet_balances::AccountData<<Runtime as pallet_balances::Config>::Balance>;
	}

	#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
	impl pallet_balances::Config for Runtime {
		type AccountStore = System;
	}

	#[derive_impl(pallet_sudo::config_preludes::TestDefaultConfig)]
	impl pallet_sudo::Config for Runtime {}

	#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig)]
	impl pallet_timestamp::Config for Runtime {}

	#[derive_impl(pallet_transaction_payment::config_preludes::TestDefaultConfig)]
	impl pallet_transaction_payment::Config for Runtime {
		type OnChargeTransaction = pallet_transaction_payment::CurrencyAdapter<Balances, ()>;
		// We specify a fixed length to fee here, which essentially means all transactions charge
		// exactly 1 unit of fee.
		type LengthToFee = FixedFee<1, <Self as pallet_balances::Config>::Balance>;
	}
}

#[docify::export(our_config_impl)]
impl our_first_pallet::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
}

impl_runtime_apis! {
	impl apis::Core<Block> for Runtime {
		fn version() -> RuntimeVersion {
			VERSION
		}

		fn execute_block(block: Block) {
			RuntimeExecutive::execute_block(block)
		}

		fn initialize_block(header: &Header) -> ExtrinsicInclusionMode {
			RuntimeExecutive::initialize_block(header)
		}
	}

	impl apis::Metadata<Block> for Runtime {
		fn metadata() -> OpaqueMetadata {
			OpaqueMetadata::new(Runtime::metadata().into())
		}

		fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
			Runtime::metadata_at_version(version)
		}

		fn metadata_versions() -> Vec<u32> {
			Runtime::metadata_versions()
		}
	}

	impl apis::BlockBuilder<Block> for Runtime {
		fn apply_extrinsic(extrinsic: ExtrinsicFor<Runtime>) -> ApplyExtrinsicResult {
			RuntimeExecutive::apply_extrinsic(extrinsic)
		}

		fn finalize_block() -> HeaderFor<Runtime> {
			RuntimeExecutive::finalize_block()
		}

		fn inherent_extrinsics(data: InherentData) -> Vec<ExtrinsicFor<Runtime>> {
			data.create_extrinsics()
		}

		fn check_inherents(
			block: Block,
			data: InherentData,
		) -> CheckInherentsResult {
			data.check_extrinsics(&block)
		}
	}

	impl apis::TaggedTransactionQueue<Block> for Runtime {
		fn validate_transaction(
			source: TransactionSource,
			tx: ExtrinsicFor<Runtime>,
			block_hash: <Runtime as frame_system::Config>::Hash,
		) -> TransactionValidity {
			RuntimeExecutive::validate_transaction(source, tx, block_hash)
		}
	}

	impl apis::OffchainWorkerApi<Block> for Runtime {
		fn offchain_worker(header: &HeaderFor<Runtime>) {
			RuntimeExecutive::offchain_worker(header)
		}
	}

	impl apis::SessionKeys<Block> for Runtime {
		fn generate_session_keys(_seed: Option<Vec<u8>>) -> Vec<u8> {
			Default::default()
		}

		fn decode_session_keys(
			_encoded: Vec<u8>,
		) -> Option<Vec<(Vec<u8>, apis::KeyTypeId)>> {
			Default::default()
		}
	}

	impl apis::AccountNonceApi<Block, interface::AccountId, interface::Nonce> for Runtime {
		fn account_nonce(account: interface::AccountId) -> interface::Nonce {
			System::account_nonce(account)
		}
	}

	impl apis::GenesisBuilder<Block> for Runtime {
		fn create_default_config() -> Vec<u8> {
			create_default_config::<RuntimeGenesisConfig>()
		}

		fn build_config(config: Vec<u8>) -> apis::GenesisBuildResult {
			build_config::<RuntimeGenesisConfig>(config)
		}
	}

	impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<
		Block,
		interface::Balance,
	> for Runtime {
		fn query_info(uxt: ExtrinsicFor<Runtime>, len: u32) -> RuntimeDispatchInfo<interface::Balance> {
			TransactionPayment::query_info(uxt, len)
		}
		fn query_fee_details(uxt: ExtrinsicFor<Runtime>, len: u32) -> FeeDetails<interface::Balance> {
			TransactionPayment::query_fee_details(uxt, len)
		}
		fn query_weight_to_fee(weight: Weight) -> interface::Balance {
			TransactionPayment::weight_to_fee(weight)
		}
		fn query_length_to_fee(length: u32) -> interface::Balance {
			TransactionPayment::length_to_fee(length)
		}
	}
}

/// Just a handy re-definition of some types based on what is already provided to the pallet
/// configs.
pub mod interface {
	use super::Runtime;
	use frame::prelude::frame_system;

	pub type AccountId = <Runtime as frame_system::Config>::AccountId;
	pub type Nonce = <Runtime as frame_system::Config>::Nonce;
	pub type Hash = <Runtime as frame_system::Config>::Hash;
	pub type Balance = <Runtime as pallet_balances::Config>::Balance;
	pub type MinimumBalance = <Runtime as pallet_balances::Config>::ExistentialDeposit;
}
