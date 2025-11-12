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

use crate::*;
use codec::{Decode, DecodeAll, Encode};
use cumulus_primitives_core::{relay_chain, ParachainBlockData, PersistedValidationData};
use cumulus_test_client::{
	generate_extrinsic, generate_extrinsic_with_pair,
	runtime::{
		self as test_runtime, Block, Hash, Header, SudoCall, SystemCall, TestPalletCall,
		UncheckedExtrinsic, WASM_BINARY,
	},
	seal_block, transfer, BlockData, BlockOrigin, BuildParachainBlockData, Client,
	DefaultTestClientBuilderExt, HeadData, InitBlockBuilder,
	Sr25519Keyring::{Alice, Bob, Charlie},
	TestClientBuilder, TestClientBuilderExt, ValidationParams,
};
use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
use polkadot_parachain_primitives::primitives::ValidationResult;
#[cfg(feature = "experimental-ump-signals")]
use relay_chain::vstaging::{UMPSignal, UMP_SEPARATOR};
use sc_consensus::{BlockImport, BlockImportParams, ForkChoiceStrategy};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};

use std::{env, process::Command};

use crate::validate_block::MemoryOptimizedValidationParams;

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
		test_runtime::elastic_scaling::WASM_BINARY
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
		test_runtime::elastic_scaling::WASM_BINARY
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
	} = client.init_block_builder(Some(validation_data), sproof_builder);

	extra_extrinsics.into_iter().for_each(|e| block_builder.push(e).unwrap());

	let mut block = block_builder.build_parachain_block(*parent_head.state_root());

	block.blocks_mut()[0] = seal_block(block.blocks()[0].clone(), client);

	TestBlockData { block, validation_data: persisted_validation_data }
}

fn build_multiple_blocks_with_witness(
	client: &Client,
	parent_head: Header,
	mut sproof_builder: RelayStateSproofBuilder,
	num_blocks: usize,
) -> TestBlockData {
	sproof_builder.para_id = test_runtime::PARACHAIN_ID.into();
	sproof_builder.included_para_head = Some(HeadData(parent_head.encode()));
	sproof_builder.current_slot = (std::time::SystemTime::now()
		.duration_since(std::time::SystemTime::UNIX_EPOCH)
		.expect("Time is always after UNIX_EPOCH; qed")
		.as_millis() as u64 /
		6000)
		.into();

	let validation_data = PersistedValidationData {
		relay_parent_number: 1,
		parent_head: parent_head.encode().into(),
		..Default::default()
	};

	let mut persisted_validation_data = None;
	let mut blocks = Vec::new();
	//TODO: Fix this, not correct.
	let mut proof = None;

	for _ in 0..num_blocks {
		let cumulus_test_client::BlockBuilderAndSupportData {
			block_builder,
			persisted_validation_data: p_v_data,
		} = client.init_block_builder(Some(validation_data.clone()), sproof_builder.clone());

		persisted_validation_data = Some(p_v_data);

		let (build_blocks, build_proof) =
			block_builder.build_parachain_block(*parent_head.state_root()).into_inner();

		proof.get_or_insert_with(|| build_proof);
		blocks.extend(build_blocks);
	}

	TestBlockData {
		block: ParachainBlockData::new(blocks, proof.unwrap()),
		validation_data: persisted_validation_data.unwrap(),
	}
}

#[test]
fn validate_block_works() {
	sp_tracing::try_init_simple();

	let (client, parent_head) = create_test_client();
	let TestBlockData { block, validation_data } =
		build_block_with_witness(&client, Vec::new(), parent_head.clone(), Default::default());

	let header = block.blocks()[0].header().clone();
	let res_header =
		call_validate_block(parent_head, block, validation_data.relay_parent_storage_root)
			.expect("Calls `validate_block`");
	assert_eq!(header, res_header);
}

#[test]
#[ignore = "Needs another pr to work"]
fn validate_multiple_blocks_work() {
	sp_tracing::try_init_simple();

	let (client, parent_head) = create_elastic_scaling_test_client();
	let TestBlockData { block, validation_data } =
		build_multiple_blocks_with_witness(&client, parent_head.clone(), Default::default(), 4);

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
	);
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
	);
	let header = block.blocks()[0].header().clone();
	assert_ne!(expected_header, header.encode());

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
		let TestBlockData { mut block, validation_data, .. } =
			build_block_with_witness(&client, Vec::new(), parent_head.clone(), Default::default());
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
		let TestBlockData { block, .. } =
			build_block_with_witness(&client, Vec::new(), parent_head.clone(), Default::default());

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

#[test]
fn check_inherents_are_unsigned_and_before_all_other_extrinsics() {
	sp_tracing::try_init_simple();

	if env::var("RUN_TEST").is_ok() {
		let (client, parent_head) = create_test_client();

		let TestBlockData { mut block, validation_data, .. } =
			build_block_with_witness(&client, Vec::new(), parent_head.clone(), Default::default());

		block.blocks_mut()[0].extrinsics.insert(0, transfer(&client, Alice, Bob, 69));

		call_validate_block(parent_head, block, validation_data.relay_parent_storage_root)
			.unwrap_err();
	} else {
		let output = Command::new(env::current_exe().unwrap())
			.args([
				"check_inherents_are_unsigned_and_before_all_other_extrinsics",
				"--",
				"--nocapture",
			])
			.env("RUN_TEST", "1")
			.output()
			.expect("Runs the test");
		assert!(output.status.success());

		assert!(String::from_utf8(output.stderr)
			.unwrap()
			.contains("Could not find `set_validation_data` inherent"));
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
	);

	let (mut header, extrinsics) = block.blocks()[0].clone().deconstruct();
	let seal = header.digest.pop().unwrap();

	let mut import = BlockImportParams::new(BlockOrigin::Own, header.clone());
	import.body = Some(extrinsics);
	import.post_digests.push(seal);
	import.fork_choice = Some(ForkChoiceStrategy::Custom(true));

	futures::executor::block_on(BlockImport::import_block(&client, import)).unwrap();

	let parent_head = block.blocks()[0].header.clone();

	let TestBlockData { block, validation_data } = build_block_with_witness(
		&client,
		vec![generate_extrinsic(&client, Alice, TestPalletCall::read_and_write_child_tries {})],
		parent_head.clone(),
		Default::default(),
	);

	let header = block.blocks()[0].header().clone();
	let res_header =
		call_validate_block(parent_head, block, validation_data.relay_parent_storage_root)
			.expect("Calls `validate_block`");
	assert_eq!(header, res_header);
}

#[cfg(feature = "experimental-ump-signals")]
#[test]
fn validate_block_handles_ump_signal() {
	sp_tracing::try_init_simple();

	let (client, parent_head) = create_elastic_scaling_test_client();
	let extra_extrinsics =
		vec![transfer(&client, Alice, Bob, 69), transfer(&client, Bob, Charlie, 100)];

	let TestBlockData { block, validation_data } = build_block_with_witness(
		&client,
		extra_extrinsics,
		parent_head.clone(),
		Default::default(),
	);

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
		vec![
			UMP_SEPARATOR,
			UMPSignal::SelectCore(CoreSelector(1), ClaimQueueOffset(DEFAULT_CLAIM_QUEUE_OFFSET))
				.encode()
		]
	);
}

#[test]
fn ensure_we_only_like_blockchains() {
	sp_tracing::try_init_simple();

	if env::var("RUN_TEST").is_ok() {
		let (client, parent_head) = create_elastic_scaling_test_client();
		let TestBlockData { mut block, validation_data } =
			build_multiple_blocks_with_witness(&client, parent_head.clone(), Default::default(), 4);

		// Reference some non existing parent.
		block.blocks_mut()[2].header.parent_hash = Hash::default();

		call_validate_block_elastic_scaling(
			parent_head,
			block,
			validation_data.relay_parent_storage_root,
		)
		.unwrap_err();
	} else {
		let output = Command::new(env::current_exe().unwrap())
			.args(["ensure_we_only_like_blockchains", "--", "--nocapture"])
			.env("RUN_TEST", "1")
			.output()
			.expect("Runs the test");

		assert!(output.status.success());

		assert!(dbg!(String::from_utf8(output.stderr).unwrap())
			.contains("Not a valid chain of blocks :("));
	}
}

#[test]
fn rejects_multiple_blocks_per_pov_when_applying_runtime_upgrade() {
	sp_tracing::try_init_simple();

	if env::var("RUN_TEST").is_ok() {
		let (client, genesis_head) = create_elastic_scaling_test_client();

		let code = test_runtime::elastic_scaling_500ms::WASM_BINARY
			.expect("You need to build the WASM binaries to run the tests!")
			.to_vec();
		let code_len = code.len() as u32;

		let mut proof_builder =
			RelayStateSproofBuilder { current_slot: 1.into(), ..Default::default() };
		proof_builder.host_config.max_code_size = code_len * 2;

		// Build the block that send the runtime upgrade.
		let TestBlockData { block: initial_block_data, .. } = build_block_with_witness(
			&client,
			vec![generate_extrinsic_with_pair(
				&client,
				Alice.into(),
				SudoCall::sudo {
					call: Box::new(SystemCall::set_code_without_checks { code }.into()),
				},
				Some(0),
			)],
			genesis_head.clone(),
			proof_builder,
		);

		let initial_block = initial_block_data.blocks()[0].clone();
		let (mut header, extrinsics) = initial_block.clone().deconstruct();
		let seal = header.digest.pop().unwrap();

		let mut import = BlockImportParams::new(BlockOrigin::Own, header.clone());
		import.body = Some(extrinsics);
		import.post_digests.push(seal);
		import.fork_choice = Some(ForkChoiceStrategy::Custom(true));

		futures::executor::block_on(BlockImport::import_block(&client, import)).unwrap();
		let initial_block_header = initial_block.header().clone();

		let mut proof_builder = RelayStateSproofBuilder {
			current_slot: 2.into(),
			upgrade_go_ahead: Some(relay_chain::UpgradeGoAhead::GoAhead),
			..Default::default()
		};
		proof_builder.host_config.max_code_size = code_len * 2;

		// 2. Build a PoV that consists of multiple blocks.
		let TestBlockData { block: pov_block_data, validation_data: pov_validation_data } =
			build_multiple_blocks_with_witness(
				&client,
				initial_block_header.clone(), // Start building PoV from the initial block's header
				proof_builder,
				4,
			);

		// 3. Validate the PoV.
		call_validate_block_elastic_scaling(
			initial_block_header, // The parent is the head of the initial block before the PoV
			pov_block_data,
			pov_validation_data.relay_parent_storage_root,
		)
		.unwrap_err();
	} else {
		let output = Command::new(env::current_exe().unwrap())
			.args([
				"rejects_multiple_blocks_per_pov_when_applying_runtime_upgrade",
				"--",
				"--nocapture",
			])
			.env("RUN_TEST", "1")
			.output()
			.expect("Runs the test");

		assert!(output.status.success());

		assert!(dbg!(String::from_utf8(output.stderr).unwrap())
			.contains("only one block per PoV is allowed"));
	}
}
