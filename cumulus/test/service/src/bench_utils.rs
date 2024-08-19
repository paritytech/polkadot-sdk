// This file is part of Cumulus.

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

use codec::Encode;
use sc_block_builder::BlockBuilderBuilder;

use crate::{construct_extrinsic, Client as TestClient};
use cumulus_client_parachain_inherent::ParachainInherentData;
use cumulus_primitives_core::{relay_chain::AccountId, PersistedValidationData};
use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
use cumulus_test_runtime::{
	BalancesCall, GluttonCall, NodeBlock, SudoCall, UncheckedExtrinsic, WASM_BINARY,
};
use frame_system_rpc_runtime_api::AccountNonceApi;
use polkadot_primitives::HeadData;
use sc_client_api::UsageProvider;
use sc_consensus::{
	block_import::{BlockImportParams, ForkChoiceStrategy},
	BlockImport, ImportResult, StateAction,
};
use sc_executor::DEFAULT_HEAP_ALLOC_STRATEGY;
use sc_executor_common::runtime_blob::RuntimeBlob;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::{ApplyExtrinsicFailed::Validity, Error::ApplyExtrinsicFailed};
use sp_consensus::BlockOrigin;
use sp_core::{sr25519, Pair};
use sp_keyring::Sr25519Keyring::Alice;
use sp_runtime::{
	transaction_validity::{InvalidTransaction, TransactionValidityError},
	AccountId32, FixedU64, OpaqueExtrinsic,
};

/// Accounts to use for transfer transactions. Enough for 5000 transactions.
const NUM_ACCOUNTS: usize = 10000;

/// Create accounts by deriving from Alice
pub fn create_benchmark_accounts() -> (Vec<sr25519::Pair>, Vec<sr25519::Pair>, Vec<AccountId32>) {
	let accounts: Vec<sr25519::Pair> = (0..NUM_ACCOUNTS)
		.map(|idx| {
			Pair::from_string(&format!("{}/{}", Alice.to_seed(), idx), None)
				.expect("Creates account pair")
		})
		.collect();
	let account_ids = accounts
		.iter()
		.map(|account| AccountId::from(account.public()))
		.collect::<Vec<AccountId>>();
	let (src_accounts, dst_accounts) = accounts.split_at(NUM_ACCOUNTS / 2);
	(src_accounts.to_vec(), dst_accounts.to_vec(), account_ids)
}

/// Create a timestamp extrinsic ahead by `MinimumPeriod` of the last known timestamp
pub fn extrinsic_set_time(client: &TestClient) -> OpaqueExtrinsic {
	let best_number = client.usage_info().chain.best_number;

	let timestamp = best_number as u64 * cumulus_test_runtime::MinimumPeriod::get();
	cumulus_test_runtime::UncheckedExtrinsic::new_bare(
		cumulus_test_runtime::RuntimeCall::Timestamp(pallet_timestamp::Call::set {
			now: timestamp,
		}),
	)
	.into()
}

/// Create a set validation data extrinsic
pub fn extrinsic_set_validation_data(
	parent_header: cumulus_test_runtime::Header,
) -> OpaqueExtrinsic {
	let parent_head = HeadData(parent_header.encode());
	let sproof_builder = RelayStateSproofBuilder {
		para_id: cumulus_test_runtime::PARACHAIN_ID.into(),
		included_para_head: parent_head.clone().into(),
		..Default::default()
	};

	let (relay_parent_storage_root, relay_chain_state) = sproof_builder.into_state_root_and_proof();
	let data = ParachainInherentData {
		validation_data: PersistedValidationData {
			parent_head,
			relay_parent_number: 10,
			relay_parent_storage_root,
			max_pov_size: 10000,
		},
		relay_chain_state,
		downward_messages: Default::default(),
		horizontal_messages: Default::default(),
	};

	cumulus_test_runtime::UncheckedExtrinsic::new_bare(
		cumulus_test_runtime::RuntimeCall::ParachainSystem(
			cumulus_pallet_parachain_system::Call::set_validation_data { data },
		),
	)
	.into()
}

/// Import block into the given client and make sure the import was successful
pub async fn import_block(client: &TestClient, block: &NodeBlock, import_existing: bool) {
	let mut params = BlockImportParams::new(BlockOrigin::File, block.header.clone());
	params.body = Some(block.extrinsics.clone());
	params.state_action = StateAction::Execute;
	params.fork_choice = Some(ForkChoiceStrategy::LongestChain);
	params.import_existing = import_existing;
	let import_result = client.import_block(params).await;
	assert!(
		matches!(import_result, Ok(ImportResult::Imported(_))),
		"Unexpected block import result: {:?}!",
		import_result
	);
}

/// Creates transfer extrinsics pair-wise from elements of `src_accounts` to `dst_accounts`.
pub fn create_benchmarking_transfer_extrinsics(
	client: &TestClient,
	src_accounts: &[sr25519::Pair],
	dst_accounts: &[sr25519::Pair],
) -> (usize, Vec<OpaqueExtrinsic>) {
	let chain = client.usage_info().chain;
	// Add as many transfer extrinsics as possible into a single block.
	let mut block_builder = BlockBuilderBuilder::new(client)
		.on_parent_block(chain.best_hash)
		.with_parent_block_number(chain.best_number)
		.build()
		.expect("Creates block builder");
	let mut max_transfer_count = 0;
	let mut extrinsics = Vec::new();
	// Every block needs one timestamp extrinsic.
	let time_ext = extrinsic_set_time(client);
	extrinsics.push(time_ext);

	// Every block needs tone set_validation_data extrinsic.
	let parent_hash = client.usage_info().chain.best_hash;
	let parent_header = client.header(parent_hash).expect("Just fetched this hash.").unwrap();
	let set_validation_data_extrinsic = extrinsic_set_validation_data(parent_header);
	extrinsics.push(set_validation_data_extrinsic);

	for (src, dst) in src_accounts.iter().zip(dst_accounts.iter()) {
		let extrinsic: UncheckedExtrinsic = construct_extrinsic(
			client,
			BalancesCall::transfer_keep_alive { dest: AccountId::from(dst.public()), value: 10000 },
			src.clone(),
			Some(0),
		);

		match block_builder.push(extrinsic.clone().into()) {
			Ok(_) => {},
			Err(ApplyExtrinsicFailed(Validity(TransactionValidityError::Invalid(
				InvalidTransaction::ExhaustsResources,
			)))) => break,
			Err(error) => panic!("{}", error),
		}

		extrinsics.push(extrinsic.into());
		max_transfer_count += 1;
	}

	if max_transfer_count >= src_accounts.len() {
		panic!("Block could fit more transfers, increase NUM_ACCOUNTS to generate more accounts.");
	}

	(max_transfer_count, extrinsics)
}

/// Prepare cumulus test runtime for execution
pub fn get_wasm_module() -> Box<dyn sc_executor_common::wasm_runtime::WasmModule> {
	let blob = RuntimeBlob::uncompress_if_needed(
		WASM_BINARY.expect("You need to build the WASM binaries to run the benchmark!"),
	)
	.unwrap();

	let config = sc_executor_wasmtime::Config {
		allow_missing_func_imports: true,
		cache_path: None,
		semantics: sc_executor_wasmtime::Semantics {
			heap_alloc_strategy: DEFAULT_HEAP_ALLOC_STRATEGY,
			instantiation_strategy: sc_executor::WasmtimeInstantiationStrategy::PoolingCopyOnWrite,
			deterministic_stack_limit: None,
			canonicalize_nans: false,
			parallel_compilation: true,
			wasm_multi_value: false,
			wasm_bulk_memory: false,
			wasm_reference_types: false,
			wasm_simd: false,
		},
	};
	Box::new(
		sc_executor_wasmtime::create_runtime::<sp_io::SubstrateHostFunctions>(blob, config)
			.expect("Unable to create wasm module."),
	)
}

/// Create a block containing setup extrinsics for the glutton pallet.
pub fn set_glutton_parameters(
	client: &TestClient,
	initialize: bool,
	compute_ratio: &FixedU64,
	storage_ratio: &FixedU64,
) -> NodeBlock {
	let parent_hash = client.usage_info().chain.best_hash;
	let parent_header = client.header(parent_hash).expect("Just fetched this hash.").unwrap();

	let mut last_nonce = client
		.runtime_api()
		.account_nonce(parent_hash, Alice.into())
		.expect("Fetching account nonce works; qed");

	let mut extrinsics = vec![];
	if initialize {
		// Initialize the pallet
		extrinsics.push(construct_extrinsic(
			client,
			SudoCall::sudo {
				call: Box::new(
					GluttonCall::initialize_pallet { new_count: 5000, witness_count: None }.into(),
				),
			},
			Alice.into(),
			Some(last_nonce),
		));
		last_nonce += 1;
	}

	// Set compute weight that should be consumed per block
	let set_compute = construct_extrinsic(
		client,
		SudoCall::sudo {
			call: Box::new(GluttonCall::set_compute { compute: *compute_ratio }.into()),
		},
		Alice.into(),
		Some(last_nonce),
	);
	last_nonce += 1;
	extrinsics.push(set_compute);

	// Set storage weight that should be consumed per block
	let set_storage = construct_extrinsic(
		client,
		SudoCall::sudo {
			call: Box::new(GluttonCall::set_storage { storage: *storage_ratio }.into()),
		},
		Alice.into(),
		Some(last_nonce),
	);
	extrinsics.push(set_storage);
	let chain = client.usage_info().chain;

	let mut block_builder = BlockBuilderBuilder::new(client)
		.on_parent_block(chain.best_hash)
		.with_parent_block_number(chain.best_number)
		.build()
		.unwrap();
	block_builder.push(extrinsic_set_time(client)).unwrap();
	block_builder.push(extrinsic_set_validation_data(parent_header)).unwrap();
	for extrinsic in extrinsics {
		block_builder.push(extrinsic.into()).unwrap();
	}

	let built_block = block_builder.build().unwrap();
	built_block.block
}
