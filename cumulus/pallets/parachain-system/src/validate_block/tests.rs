// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

use crate::{validate_block::MemoryOptimizedValidationParams, *};
use codec::{Decode, DecodeAll, Encode};
use cumulus_primitives_core::{ParachainBlockData, PersistedValidationData};
use cumulus_test_client::{
	generate_extrinsic, generate_extrinsic_with_pair,
	runtime::{
		self as test_runtime, Block, Hash, Header, TestPalletCall, UncheckedExtrinsic, WASM_BINARY,
	},
	seal_parachain_block_data, transfer, BlockData, BlockOrigin, BuildParachainBlockData, Client,
	ClientBlockImportExt, DefaultTestClientBuilderExt, HeadData, InitBlockBuilder,
	Sr25519Keyring::{Alice, Bob, Charlie},
	TestClientBuilder, TestClientBuilderExt, ValidationParams,
};
use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
use polkadot_parachain_primitives::primitives::ValidationResult;
use sc_consensus::{BlockImport, BlockImportParams, ForkChoiceStrategy};
use sp_api::{ApiExt, Core, ProofRecorder, ProvideRuntimeApi};
use sp_consensus_slots::SlotDuration;
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, Block as BlockT, Header as HeaderT},
	DigestItem,
};
use sp_trie::{proof_size_extension::ProofSizeExt, recorder::IgnoredNodes, StorageProof};
use std::{env, process::Command};

fn call_validate_block_validation_result(
	validation_code: &[u8],
	parent_head: Header,
	block_data: ParachainBlockData<Block>,
	relay_parent_storage_root: Hash,
) -> cumulus_test_client::ExecutorResult<ValidationResult> {
	cumulus_test_client::validate_block(
		ValidationParams {
			block_data: BlockData(block_data.encode()),
			parent_head: HeadData(parent_head.encode()),
			relay_parent_number: 1,
			relay_parent_storage_root,
		},
		validation_code,
	)
}

fn call_validate_block(
	parent_head: Header,
	block_data: ParachainBlockData<Block>,
	relay_parent_storage_root: Hash,
) -> cumulus_test_client::ExecutorResult<Header> {
	call_validate_block_validation_result(
		WASM_BINARY.expect("You need to build the WASM binaries to run the tests!"),
		parent_head,
		block_data,
		relay_parent_storage_root,
	)
	.map(|v| Header::decode(&mut &v.head_data.0[..]).expect("Decodes `Header`."))
}

/// Call `validate_block` in the runtime with `elastic-scaling` activated.
fn call_validate_block_elastic_scaling(
	parent_head: Header,
	block_data: ParachainBlockData<Block>,
	relay_parent_storage_root: Hash,
) -> cumulus_test_client::ExecutorResult<Header> {
	call_validate_block_validation_result(
		test_runtime::elastic_scaling_500ms::WASM_BINARY
			.expect("You need to build the WASM binaries to run the tests!"),
		parent_head,
		block_data,
		relay_parent_storage_root,
	)
	.map(|v| Header::decode(&mut &v.head_data.0[..]).expect("Decodes `Header`."))
}

fn create_test_client() -> (Client, Header) {
	let client = TestClientBuilder::new().enable_import_proof_recording().build();

	let genesis_header = client
		.header(client.chain_info().genesis_hash)
		.ok()
		.flatten()
		.expect("Genesis header exists; qed");

	(client, genesis_header)
}

/// Create test client using the runtime with `elastic-scaling` feature enabled.
fn create_elastic_scaling_test_client() -> (Client, Header) {
	let mut builder = TestClientBuilder::new();
	builder.genesis_init_mut().wasm = Some(
		test_runtime::elastic_scaling_500ms::WASM_BINARY
			.expect("You need to build the WASM binaries to run the tests!")
			.to_vec(),
	);
	let client = builder.enable_import_proof_recording().build();

	let genesis_header = client
		.header(client.chain_info().genesis_hash)
		.ok()
		.flatten()
		.expect("Genesis header exists; qed");

	(client, genesis_header)
}

struct TestBlockData {
	block: ParachainBlockData<Block>,
	validation_data: PersistedValidationData,
}

fn build_block_with_witness(
	client: &Client,
	extra_extrinsics: Vec<UncheckedExtrinsic>,
	parent_head: Header,
	mut sproof_builder: RelayStateSproofBuilder,
	pre_digests: Vec<DigestItem>,
) -> TestBlockData {
	sproof_builder.para_id = test_runtime::PARACHAIN_ID.into();
	sproof_builder.included_para_head = Some(HeadData(parent_head.encode()));

	let validation_data = PersistedValidationData {
		relay_parent_number: 1,
		parent_head: parent_head.encode().into(),
		..Default::default()
	};

	let cumulus_test_client::BlockBuilderAndSupportData {
		mut block_builder,
		persisted_validation_data,
	} = client.init_block_builder_with_pre_digests(Some(validation_data), sproof_builder, pre_digests);

	extra_extrinsics.into_iter().for_each(|e| block_builder.push(e).unwrap());

	let block = block_builder.build_parachain_block(*parent_head.state_root());

	TestBlockData { block, validation_data: persisted_validation_data }
}

fn build_multiple_blocks_with_witness(
	client: &Client,
	mut parent_head: Header,
	mut sproof_builder: RelayStateSproofBuilder,
	num_blocks: u32,
	extra_extrinsics: impl Fn(u32) -> Vec<UncheckedExtrinsic>,
) -> TestBlockData {
	let parent_head_root = *parent_head.state_root();
	sproof_builder.para_id = test_runtime::PARACHAIN_ID.into();
	sproof_builder.included_para_head = Some(HeadData(parent_head.encode()));

	let timestamp = if sproof_builder.current_slot == 0u64 {
		let timestamp = std::time::SystemTime::now()
			.duration_since(std::time::SystemTime::UNIX_EPOCH)
			.expect("Time is always after UNIX_EPOCH; qed")
			.as_millis() as u64;
		sproof_builder.current_slot = (timestamp / 6000).into();

		timestamp
	} else {
		sproof_builder
			.current_slot
			.timestamp(SlotDuration::from_millis(6000))
			.unwrap()
			.as_millis()
	};

	let validation_data = PersistedValidationData {
		relay_parent_number: 1,
		parent_head: parent_head.encode().into(),
		..Default::default()
	};

	let mut persisted_validation_data = None;
	let mut blocks = Vec::new();
	let mut proof = StorageProof::empty();
	let mut ignored_nodes = IgnoredNodes::<H256>::default();

	for i in 0..num_blocks {
		let cumulus_test_client::BlockBuilderAndSupportData {
			mut block_builder,
			persisted_validation_data: p_v_data,
		} = client.init_block_builder_with_ignored_nodes(
			parent_head.hash(),
			Some(validation_data.clone()),
			sproof_builder.clone(),
			timestamp,
			ignored_nodes.clone(),
		);

		persisted_validation_data = Some(p_v_data);

		for ext in (extra_extrinsics)(i) {
			block_builder.push(ext).unwrap();
		}

		let built_block = block_builder.build().unwrap();

		futures::executor::block_on({
			let parent_hash = *built_block.block.header.parent_hash();
			let state = client.state_at(parent_hash).unwrap();

			let mut api = client.runtime_api();
			let proof_recorder = ProofRecorder::<Block>::with_ignored_nodes(ignored_nodes.clone());
			api.record_proof_with_recorder(proof_recorder.clone());
			api.register_extension(ProofSizeExt::new(proof_recorder));
			api.execute_block(parent_hash, built_block.block.clone()).unwrap();

			let (header, extrinsics) = built_block.block.clone().deconstruct();

			let mut import = BlockImportParams::new(BlockOrigin::Own, header);
			import.body = Some(extrinsics);
			import.fork_choice = Some(ForkChoiceStrategy::Custom(true));
			import.state_action = api.into_storage_changes(&state, parent_hash).unwrap().into();

			BlockImport::import_block(&client, import)
		})
		.unwrap();

		ignored_nodes.extend(IgnoredNodes::from_storage_proof::<BlakeTwo256>(
			&built_block.proof.clone().unwrap(),
		));
		ignored_nodes.extend(IgnoredNodes::from_memory_db(built_block.storage_changes.transaction));
		proof = StorageProof::merge([proof, built_block.proof.unwrap()]);

		parent_head = built_block.block.header.clone();

		blocks.push(built_block.block);
	}

	let proof = proof.into_compact_proof::<BlakeTwo256>(parent_head_root).unwrap();

	TestBlockData {
		block: ParachainBlockData::new(blocks, proof),
		validation_data: persisted_validation_data.unwrap(),
	}
}

#[test]
fn validate_block_works() {
	sp_tracing::try_init_simple();

	let (client, parent_head) = create_test_client();
	let TestBlockData { block, validation_data } = build_block_with_witness(
		&client,
		Vec::new(),
		parent_head.clone(),
		Default::default(),
		Default::default(),
	);

	let block = seal_parachain_block_data(block, &client);
	let header = block.blocks()[0].header().clone();
	let res_header =
		call_validate_block(parent_head, block, validation_data.relay_parent_storage_root)
			.expect("Calls `validate_block`");
	assert_eq!(header, res_header);
}

#[test]
fn validate_multiple_blocks_work() {
	sp_tracing::try_init_simple();

	let blocks_per_pov = 4;
	let (client, parent_head) = create_elastic_scaling_test_client();
	let TestBlockData { block, validation_data } = build_multiple_blocks_with_witness(
		&client,
		parent_head.clone(),
		Default::default(),
		blocks_per_pov,
		|i| {
			vec![generate_extrinsic_with_pair(
				&client,
				Charlie.into(),
				TestPalletCall::read_and_write_big_value {},
				Some(i),
			)]
		},
	);

	assert!(block.proof().encoded_size() < 3 * 1024 * 1024);

	let block = seal_parachain_block_data(block, &client);
	let header = block.blocks().last().unwrap().header().clone();
	let res_header = call_validate_block_elastic_scaling(
		parent_head,
		block,
		validation_data.relay_parent_storage_root,
	)
	.expect("Calls `validate_block`");
	assert_eq!(header, res_header);
}

#[test]
fn validate_block_with_extra_extrinsics() {
	sp_tracing::try_init_simple();

	let (client, parent_head) = create_test_client();
	let extra_extrinsics = vec![
		transfer(&client, Alice, Bob, 69),
		transfer(&client, Bob, Charlie, 100),
		transfer(&client, Charlie, Alice, 500),
	];

	let TestBlockData { block, validation_data } = build_block_with_witness(
		&client,
		extra_extrinsics,
		parent_head.clone(),
		Default::default(),
		Default::default(),
	);
	let block = seal_parachain_block_data(block, &client);
	let header = block.blocks()[0].header().clone();

	let res_header =
		call_validate_block(parent_head, block, validation_data.relay_parent_storage_root)
			.expect("Calls `validate_block`");
	assert_eq!(header, res_header);
}

#[test]
fn validate_block_returns_custom_head_data() {
	sp_tracing::try_init_simple();

	let expected_header = vec![1, 3, 3, 7, 4, 5, 6];

	let (client, parent_head) = create_test_client();
	let extra_extrinsics = vec![
		transfer(&client, Alice, Bob, 69),
		generate_extrinsic(
			&client,
			Charlie,
			TestPalletCall::set_custom_validation_head_data {
				custom_header: expected_header.clone(),
			},
		),
		transfer(&client, Bob, Charlie, 100),
	];

	let TestBlockData { block, validation_data } = build_block_with_witness(
		&client,
		extra_extrinsics,
		parent_head.clone(),
		Default::default(),
		Default::default(),
	);
	let header = block.blocks()[0].header().clone();
	assert_ne!(expected_header, header.encode());

	let block = seal_parachain_block_data(block, &client);
	let res_header = call_validate_block_validation_result(
		WASM_BINARY.expect("You need to build the WASM binaries to run the tests!"),
		parent_head,
		block,
		validation_data.relay_parent_storage_root,
	)
	.expect("Calls `validate_block`")
	.head_data
	.0;
	assert_eq!(expected_header, res_header);
}

#[test]
fn validate_block_invalid_parent_hash() {
	sp_tracing::try_init_simple();

	if env::var("RUN_TEST").is_ok() {
		let (client, parent_head) = create_test_client();
		let TestBlockData { mut block, validation_data, .. } = build_block_with_witness(
			&client,
			Vec::new(),
			parent_head.clone(),
			Default::default(),
			Default::default(),
		);
		block.blocks_mut()[0].header.set_parent_hash(Hash::from_low_u64_be(1));

		call_validate_block(parent_head, block, validation_data.relay_parent_storage_root)
			.unwrap_err();
	} else {
		let output = Command::new(env::current_exe().unwrap())
			.args(["validate_block_invalid_parent_hash", "--", "--nocapture"])
			.env("RUN_TEST", "1")
			.output()
			.expect("Runs the test");
		assert!(output.status.success());

		assert!(dbg!(String::from_utf8(output.stderr).unwrap())
			.contains("Parachain head needs to be the parent of the first block"));
	}
}

#[test]
fn validate_block_fails_on_invalid_validation_data() {
	sp_tracing::try_init_simple();

	if env::var("RUN_TEST").is_ok() {
		let (client, parent_head) = create_test_client();
		let TestBlockData { block, .. } = build_block_with_witness(
			&client,
			Vec::new(),
			parent_head.clone(),
			Default::default(),
			Default::default(),
		);

		let block = seal_parachain_block_data(block, &client);

		call_validate_block(parent_head, block, Hash::random()).unwrap_err();
	} else {
		let output = Command::new(env::current_exe().unwrap())
			.args(["validate_block_fails_on_invalid_validation_data", "--", "--nocapture"])
			.env("RUN_TEST", "1")
			.output()
			.expect("Runs the test");
		assert!(output.status.success());

		assert!(dbg!(String::from_utf8(output.stderr).unwrap())
			.contains("Relay parent storage root doesn't match"));
	}
}

/// Test that ensures that `ValidationParams` and `MemoryOptimizedValidationParams`
/// are encoding/decoding.
#[test]
fn validation_params_and_memory_optimized_validation_params_encode_and_decode() {
	const BLOCK_DATA: &[u8] = &[1, 2, 3, 4, 5];
	const PARENT_HEAD: &[u8] = &[1, 3, 4, 5, 6, 7, 9];

	let validation_params = ValidationParams {
		block_data: BlockData(BLOCK_DATA.encode()),
		parent_head: HeadData(PARENT_HEAD.encode()),
		relay_parent_number: 1,
		relay_parent_storage_root: Hash::random(),
	};

	let encoded = validation_params.encode();

	let decoded = MemoryOptimizedValidationParams::decode_all(&mut &encoded[..]).unwrap();
	assert_eq!(decoded.relay_parent_number, validation_params.relay_parent_number);
	assert_eq!(decoded.relay_parent_storage_root, validation_params.relay_parent_storage_root);
	assert_eq!(decoded.block_data, validation_params.block_data.0);
	assert_eq!(decoded.parent_head, validation_params.parent_head.0);

	let encoded = decoded.encode();

	let decoded = ValidationParams::decode_all(&mut &encoded[..]).unwrap();
	assert_eq!(decoded, validation_params);
}

/// Test for ensuring that we are differentiating in the `validation::trie_cache` between different
/// child tries.
///
/// This is achieved by first building a block using `read_and_write_child_tries` that should set
/// the values in the child tries. In the second step we are building a second block with the same
/// extrinsic that reads the values from the child tries and it asserts that we read the correct
/// data from the state.
#[test]
fn validate_block_works_with_child_tries() {
	sp_tracing::try_init_simple();

	let (client, parent_head) = create_test_client();
	let TestBlockData { block, .. } = build_block_with_witness(
		&client,
		vec![generate_extrinsic(&client, Charlie, TestPalletCall::read_and_write_child_tries {})],
		parent_head.clone(),
		Default::default(),
		Default::default(),
	);

	let block = block.blocks()[0].clone();

	futures::executor::block_on(client.import(BlockOrigin::Own, block.clone())).unwrap();

	let parent_head = block.header().clone();

	let TestBlockData { block, validation_data } = build_block_with_witness(
		&client,
		vec![generate_extrinsic(&client, Alice, TestPalletCall::read_and_write_child_tries {})],
		parent_head.clone(),
		Default::default(),
		Default::default(),
	);

	let block = seal_parachain_block_data(block, &client);
	let header = block.blocks()[0].header().clone();
	let res_header =
		call_validate_block(parent_head, block, validation_data.relay_parent_storage_root)
			.expect("Calls `validate_block`");
	assert_eq!(header, res_header);
}

#[test]
fn state_changes_in_multiple_blocks_are_applied_in_exact_order() {
	sp_tracing::try_init_simple();

	let blocks_per_pov = 12;
	// disable the core selection logic
	let (client, genesis_head) = create_elastic_scaling_test_client();

	// 1. Build the initial block that stores values in the map.
	let TestBlockData { block: initial_block_data, .. } = build_block_with_witness(
		&client,
		vec![generate_extrinsic_with_pair(
			&client,
			Alice.into(),
			TestPalletCall::store_values_in_map { max_key: 4095 },
			Some(0),
		)],
		genesis_head.clone(),
		RelayStateSproofBuilder { current_slot: 1.into(), ..Default::default() },
		Vec::new(),
	);

	let initial_block = initial_block_data.blocks()[0].clone();
	futures::executor::block_on(client.import(BlockOrigin::Own, initial_block.clone())).unwrap();
	let initial_block_header = initial_block.header().clone();

	// 2. Build the PoV block that removes values from the map.
	let TestBlockData { block: pov_block_data, validation_data: pov_validation_data } =
		build_multiple_blocks_with_witness(
			&client,
			initial_block_header.clone(), // Start building PoV from the initial block's header
			RelayStateSproofBuilder { current_slot: 2.into(), ..Default::default() },
			blocks_per_pov,
			|i| {
				// Each block `i` (0-11) removes key `116 + i`.
				let key_to_remove = 116 + i;
				vec![generate_extrinsic_with_pair(
					&client,
					Bob.into(), // Use Bob to avoid nonce conflicts with Alice
					TestPalletCall::remove_value_from_map { key: key_to_remove },
					Some(i),
				)]
			},
		);

	// 3. Validate the PoV.
	let sealed_pov_block = seal_parachain_block_data(pov_block_data, &client);
	let final_pov_header = sealed_pov_block.blocks().last().unwrap().header().clone();
	let res_header = call_validate_block_elastic_scaling(
		initial_block_header, // The parent is the head of the initial block before the PoV
		sealed_pov_block,
		pov_validation_data.relay_parent_storage_root,
	)
	.expect("Calls `validate_block` after building the PoV");
	assert_eq!(final_pov_header, res_header);
}

#[test]
#[cfg(feature = "experimental-ump-signals")]
fn validate_block_handles_ump_signal() {
	use cumulus_primitives_core::{
		relay_chain::{UMPSignal, UMP_SEPARATOR},
		ClaimQueueOffset, CoreInfo, CoreSelector,
	};

	sp_tracing::try_init_simple();

	let (client, parent_head) = create_elastic_scaling_test_client();
	let extra_extrinsics =
		vec![transfer(&client, Alice, Bob, 69), transfer(&client, Bob, Charlie, 100)];

	let TestBlockData { block, validation_data } = build_block_with_witness(
		&client,
		extra_extrinsics,
		parent_head.clone(),
		Default::default(),
		vec![CumulusDigestItem::CoreInfo(CoreInfo {
			selector: CoreSelector(0),
			claim_queue_offset: ClaimQueueOffset(0),
			number_of_cores: 1.into(),
		})
		.to_digest_item()],
	);

	let block = seal_parachain_block_data(block, &client);
	let upward_messages = call_validate_block_validation_result(
		test_runtime::elastic_scaling::WASM_BINARY
			.expect("You need to build the WASM binaries to run the tests!"),
		parent_head,
		block,
		validation_data.relay_parent_storage_root,
	)
	.expect("Calls `validate_block`")
	.upward_messages;

	assert_eq!(
		upward_messages,
		vec![UMP_SEPARATOR, UMPSignal::SelectCore(CoreSelector(0), ClaimQueueOffset(0)).encode()]
	);
}
