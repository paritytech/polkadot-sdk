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

//! A minimal runtime that includes the template [`pallet`](`pallet_minimal_template`).

#![cfg_attr(not(feature = "std"), no_std)]

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

use frame::{
	deps::frame_support::{
		genesis_builder_helper::{build_state, get_preset},
		runtime,
		weights::{FixedFee, NoFee},
	},
	prelude::*,
	runtime::{
		apis::{
			self, impl_runtime_apis, ApplyExtrinsicResult, CheckInherentsResult,
			ExtrinsicInclusionMode, OpaqueMetadata,
		},
		prelude::*,
	},
};

/// The runtime version.
#[runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: create_runtime_str!("minimal-template-runtime"),
	impl_name: create_runtime_str!("minimal-template-runtime"),
	authoring_version: 1,
	spec_version: 0,
	impl_version: 1,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 1,
	state_version: 1,
};

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
	NativeVersion { runtime_version: VERSION, can_author_with: Default::default() }
}

/// The signed extensions that are added to the runtime.
type SignedExtra = (
	// Checks that the sender is not the zero address.
	frame_system::CheckNonZeroSender<Runtime>,
	// Checks that the runtime version is correct.
	frame_system::CheckSpecVersion<Runtime>,
	// Checks that the transaction version is correct.
	frame_system::CheckTxVersion<Runtime>,
	// Checks that the genesis hash is correct.
	frame_system::CheckGenesis<Runtime>,
	// Checks that the era is valid.
	frame_system::CheckEra<Runtime>,
	// Checks that the nonce is valid.
	frame_system::CheckNonce<Runtime>,
	// Checks that the weight is valid.
	frame_system::CheckWeight<Runtime>,
	// Ensures that the sender has enough funds to pay for the transaction
	// and deducts the fee from the sender's account.
	pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
);

// Composes the runtime by adding all the used pallets and deriving necessary types.
#[runtime]
mod runtime {
	/// The main runtime type.
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeSlashReason,
		RuntimeLockId,
		RuntimeTask
	)]
	pub struct Runtime;

	/// Mandatory system pallet that should always be included in a FRAME runtime.
	#[runtime::pallet_index(0)]
	pub type System = frame_system;

	/// Provides a way for consensus systems to set and check the onchain time.
	#[runtime::pallet_index(1)]
	pub type Timestamp = pallet_timestamp;

	/// Provides the ability to keep track of balances.
	#[runtime::pallet_index(2)]
	pub type Balances = pallet_balances;

	/// Provides a way to execute privileged functions.
	#[runtime::pallet_index(3)]
	pub type Sudo = pallet_sudo;

	/// Provides the ability to charge for extrinsic execution.
	#[runtime::pallet_index(4)]
	pub type TransactionPayment = pallet_transaction_payment;

	/// A minimal pallet template.
	#[runtime::pallet_index(5)]
	pub type Template = pallet_minimal_template;
}

parameter_types! {
	pub const Version: RuntimeVersion = VERSION;
}

/// Implements the types required for the system pallet.
#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
	type Version = Version;
	// Use the account data from the balances pallet
	type AccountData = pallet_balances::AccountData<<Runtime as pallet_balances::Config>::Balance>;
}

// Implements the types required for the balances pallet.
#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type AccountStore = System;
}

// Implements the types required for the sudo pallet.
#[derive_impl(pallet_sudo::config_preludes::TestDefaultConfig)]
impl pallet_sudo::Config for Runtime {}

// Implements the types required for the sudo pallet.
#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig)]
impl pallet_timestamp::Config for Runtime {}

// Implements the types required for the transaction payment pallet.
#[derive_impl(pallet_transaction_payment::config_preludes::TestDefaultConfig)]
impl pallet_transaction_payment::Config for Runtime {
	type OnChargeTransaction = pallet_transaction_payment::FungibleAdapter<Balances, ()>;
	// Setting fee as independent of the weight of the extrinsic for demo purposes
	type WeightToFee = NoFee<<Self as pallet_balances::Config>::Balance>;
	// Setting fee as fixed for any length of the call data for demo purposes
	type LengthToFee = FixedFee<1, <Self as pallet_balances::Config>::Balance>;
}

// Implements the types required for the template pallet.
impl pallet_minimal_template::Config for Runtime {}

type Block = frame::runtime::types_common::BlockOf<Runtime, SignedExtra>;
type Header = HeaderFor<Runtime>;

type RuntimeExecutive =
	Executive<Runtime, Block, frame_system::ChainContext<Runtime>, Runtime, AllPalletsWithSystem>;

use pallet_transaction_payment::{FeeDetails, RuntimeDispatchInfo};

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

	impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
		fn build_state(config: Vec<u8>) -> sp_genesis_builder::Result {
			build_state::<RuntimeGenesisConfig>(config)
		}

		fn get_preset(id: &Option<sp_genesis_builder::PresetId>) -> Option<Vec<u8>> {
			get_preset::<RuntimeGenesisConfig>(id, |_| None)
		}

		fn preset_names() -> Vec<sp_genesis_builder::PresetId> {
			vec![]
		}
	}
}

/// Some re-exports that the node side code needs to know. Some are useful in this context as well.
///
/// Other types should preferably be private.
// TODO: this should be standardized in some way, see:
// https://github.com/paritytech/substrate/issues/10579#issuecomment-1600537558
pub mod interface {
	use super::Runtime;
	use frame::deps::frame_system;

	pub type Block = super::Block;
	pub use frame::runtime::types_common::OpaqueBlock;
	pub type AccountId = <Runtime as frame_system::Config>::AccountId;
	pub type Nonce = <Runtime as frame_system::Config>::Nonce;
	pub type Hash = <Runtime as frame_system::Config>::Hash;
	pub type Balance = <Runtime as pallet_balances::Config>::Balance;
	pub type MinimumBalance = <Runtime as pallet_balances::Config>::ExistentialDeposit;
}
