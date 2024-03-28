// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use codec::{Decode, DecodeAll, Encode};
use cumulus_primitives_core::{ParachainBlockData, PersistedValidationData};
use cumulus_test_client::{
	generate_extrinsic,
	runtime::{
		self as test_runtime, Block, Hash, Header, TestPalletCall, UncheckedExtrinsic, WASM_BINARY,
	},
	transfer, BlockData, BlockOrigin, BuildParachainBlockData, Client, ClientBlockImportExt,
	DefaultTestClientBuilderExt, HeadData, InitBlockBuilder, TestClientBuilder,
	TestClientBuilderExt, ValidationParams,
};
use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
use sp_keyring::AccountKeyring::*;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use std::{env, process::Command};

use crate::validate_block::MemoryOptimizedValidationParams;

fn call_validate_block_encoded_header(
	parent_head: Header,
	block_data: ParachainBlockData<Block>,
	relay_parent_storage_root: Hash,
) -> cumulus_test_client::ExecutorResult<Vec<u8>> {
	cumulus_test_client::validate_block(
		ValidationParams {
			block_data: BlockData(block_data.encode()),
			parent_head: HeadData(parent_head.encode()),
			relay_parent_number: 1,
			relay_parent_storage_root,
		},
		WASM_BINARY.expect("You need to build the WASM binaries to run the tests!"),
	)
	.map(|v| v.head_data.0)
}

fn call_validate_block(
	parent_head: Header,
	block_data: ParachainBlockData<Block>,
	relay_parent_storage_root: Hash,
) -> cumulus_test_client::ExecutorResult<Header> {
	call_validate_block_encoded_header(parent_head, block_data, relay_parent_storage_root)
		.map(|v| Header::decode(&mut &v[..]).expect("Decodes `Header`."))
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
	let (relay_parent_storage_root, _) = sproof_builder.clone().into_state_root_and_proof();
	let mut validation_data = PersistedValidationData {
		relay_parent_number: 1,
		parent_head: parent_head.encode().into(),
		..Default::default()
	};
	let mut builder = client.init_block_builder(Some(validation_data.clone()), sproof_builder);

	validation_data.relay_parent_storage_root = relay_parent_storage_root;

	extra_extrinsics.into_iter().for_each(|e| builder.push(e).unwrap());

	let block = builder.build_parachain_block(*parent_head.state_root());

	TestBlockData { block, validation_data }
}

#[test]
fn validate_block_works() {
	sp_tracing::try_init_simple();

	let (client, parent_head) = create_test_client();
	let TestBlockData { block, validation_data } =
		build_block_with_witness(&client, Vec::new(), parent_head.clone(), Default::default());
	let header = block.header().clone();

	let res_header =
		call_validate_block(parent_head, block, validation_data.relay_parent_storage_root)
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
	let header = block.header().clone();

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
	let header = block.header().clone();
	assert_ne!(expected_header, header.encode());

	let res_header = call_validate_block_encoded_header(
		parent_head,
		block,
		validation_data.relay_parent_storage_root,
	)
	.expect("Calls `validate_block`");
	assert_eq!(expected_header, res_header);
}

#[test]
fn validate_block_invalid_parent_hash() {
	sp_tracing::try_init_simple();

	if env::var("RUN_TEST").is_ok() {
		let (client, parent_head) = create_test_client();
		let TestBlockData { block, validation_data } =
			build_block_with_witness(&client, Vec::new(), parent_head.clone(), Default::default());
		let (mut header, extrinsics, witness) = block.deconstruct();
		header.set_parent_hash(Hash::from_low_u64_be(1));

		let block_data = ParachainBlockData::new(header, extrinsics, witness);
		call_validate_block(parent_head, block_data, validation_data.relay_parent_storage_root)
			.unwrap_err();
	} else {
		let output = Command::new(env::current_exe().unwrap())
			.args(["validate_block_invalid_parent_hash", "--", "--nocapture"])
			.env("RUN_TEST", "1")
			.output()
			.expect("Runs the test");
		assert!(output.status.success());

		assert!(dbg!(String::from_utf8(output.stderr).unwrap()).contains("Invalid parent hash"));
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

		let TestBlockData { block, validation_data } =
			build_block_with_witness(&client, Vec::new(), parent_head.clone(), Default::default());

		let (header, mut extrinsics, proof) = block.deconstruct();

		extrinsics.insert(0, transfer(&client, Alice, Bob, 69));

		call_validate_block(
			parent_head,
			ParachainBlockData::new(header, extrinsics, proof),
			validation_data.relay_parent_storage_root,
		)
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

	let (mut client, parent_head) = create_test_client();
	let TestBlockData { block, .. } = build_block_with_witness(
		&client,
		vec![generate_extrinsic(&client, Charlie, TestPalletCall::read_and_write_child_tries {})],
		parent_head.clone(),
		Default::default(),
	);
	let block = block.into_block();

	futures::executor::block_on(client.import(BlockOrigin::Own, block.clone())).unwrap();

	let parent_head = block.header().clone();

	let TestBlockData { block, validation_data } = build_block_with_witness(
		&client,
		vec![generate_extrinsic(&client, Alice, TestPalletCall::read_and_write_child_tries {})],
		parent_head.clone(),
		Default::default(),
	);

	let header = block.header().clone();

	let res_header =
		call_validate_block(parent_head, block, validation_data.relay_parent_storage_root)
			.expect("Calls `validate_block`");
	assert_eq!(header, res_header);
}
