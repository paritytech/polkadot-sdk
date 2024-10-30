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

//! Runtime used in `your_first_runtime`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
use alloc::{vec, vec::Vec};
use first_pallet::pallet_v2 as our_first_pallet;
use frame::{
	prelude::*,
	runtime::{apis, prelude::*},
};
use pallet_transaction_payment_rpc_runtime_api::{FeeDetails, RuntimeDispatchInfo};

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
	system_version: 1,
};

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
		frame::runtime::types_common::SystemTransactionExtensionsOf<Runtime>,
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
		type OnChargeTransaction = pallet_transaction_payment::FungibleAdapter<Balances, ()>;
		// We specify a fixed length to fee here, which essentially means all transactions charge
		// exactly 1 unit of fee.
		type LengthToFee = FixedFee<1, <Self as pallet_balances::Config>::Balance>;
		type WeightToFee = NoFee<<Self as pallet_balances::Config>::Balance>;
	}
}

#[docify::export(our_config_impl)]
impl our_first_pallet::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
}

/// Provides getters for genesis configuration presets.
pub mod genesis_config_presets {
	use super::*;
	use crate::{
		interface::{Balance, MinimumBalance},
		BalancesConfig, RuntimeGenesisConfig, SudoConfig,
	};
	use serde_json::Value;

	/// Returns a development genesis config preset.
	#[docify::export]
	pub fn development_config_genesis() -> Value {
		let endowment = <MinimumBalance as Get<Balance>>::get().max(1) * 1000;
		let config = RuntimeGenesisConfig {
			balances: BalancesConfig {
				balances: AccountKeyring::iter()
					.map(|a| (a.to_account_id(), endowment))
					.collect::<Vec<_>>(),
			},
			sudo: SudoConfig { key: Some(AccountKeyring::Alice.to_account_id()) },
			..Default::default()
		};

		serde_json::to_value(config).expect("Could not build genesis config.")
	}

	/// Get the set of the available genesis config presets.
	#[docify::export]
	pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
		let patch = match id.try_into() {
			Ok(DEV_RUNTIME_PRESET) => development_config_genesis(),
			_ => return None,
		};
		Some(
			serde_json::to_string(&patch)
				.expect("serialization to json is expected to work. qed.")
				.into_bytes(),
		)
	}

	/// List of supported presets.
	#[docify::export]
	pub fn preset_names() -> Vec<PresetId> {
		vec![PresetId::from(DEV_RUNTIME_PRESET)]
	}
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
		fn build_state(config: Vec<u8>) -> GenesisBuilderResult {
			build_state::<RuntimeGenesisConfig>(config)
		}

		fn get_preset(id: &Option<PresetId>) -> Option<Vec<u8>> {
			get_preset::<RuntimeGenesisConfig>(id, self::genesis_config_presets::get_preset)
		}

		fn preset_names() -> Vec<PresetId> {
			crate::genesis_config_presets::preset_names()
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
