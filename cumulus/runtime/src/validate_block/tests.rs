// Copyright 2019 Parity Technologies (UK) Ltd.
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

use crate::ParachainBlockData;

use parachain::primitives::{BlockData, HeadData, ValidationParams, ValidationResult};
use sc_block_builder::BlockBuilderProvider;
use sc_executor::{
	error::Result, sp_wasm_interface::HostFunctions, WasmExecutionMethod, WasmExecutor,
};
use sp_blockchain::HeaderBackend;
use sp_consensus::SelectChain;
use sp_core::traits::CallInWasm;
use sp_io::TestExternalities;
use sp_keyring::AccountKeyring::*;
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, Header as HeaderT},
};
use test_client::{
	generate_block_inherents,
	runtime::{Block, Hash, Header, UncheckedExtrinsic, WASM_BINARY},
	transfer, Client, DefaultTestClientBuilderExt, LongestChain, TestClientBuilder,
	TestClientBuilderExt,
};

use codec::{Decode, Encode};

fn call_validate_block(
	parent_head: Header,
	block_data: ParachainBlockData<Block>,
) -> Result<Header> {
	let mut ext = TestExternalities::default();
	let mut ext_ext = ext.ext();
	let params = ValidationParams {
		block_data: BlockData(block_data.encode()),
		parent_head: HeadData(parent_head.encode()),
		code_upgrade_allowed: None,
		max_code_size: 1024,
		max_head_data_size: 1024,
		relay_chain_height: 1,
	}
	.encode();

	let executor = WasmExecutor::new(
		WasmExecutionMethod::Interpreted,
		Some(1024),
		sp_io::SubstrateHostFunctions::host_functions(),
		1,
	);

	executor
		.call_in_wasm(
			&WASM_BINARY.expect("You need to build the WASM binaries to run the tests!"),
			None,
			"validate_block",
			&params,
			&mut ext_ext,
			sp_core::traits::MissingHostFunctions::Disallow,
		)
		.map(|v| ValidationResult::decode(&mut &v[..]).expect("Decode `ValidationResult`."))
		.map(|v| Header::decode(&mut &v.head_data.0[..]).expect("Decode `Header`."))
		.map_err(|err| err.into())
}

fn create_test_client() -> (Client, LongestChain) {
	TestClientBuilder::new()
		// NOTE: this allows easier debugging
		.set_execution_strategy(sc_client_api::ExecutionStrategy::NativeWhenPossible)
		.build_with_longest_chain()
}

fn build_block_with_proof(
	client: &Client,
	extra_extrinsics: Vec<UncheckedExtrinsic>,
) -> (Block, sp_trie::StorageProof) {
	let block_id = BlockId::Hash(client.info().best_hash);
	let mut builder = client
		.new_block_at(&block_id, Default::default(), true)
		.expect("Initializes new block");

	generate_block_inherents(client)
		.into_iter()
		.for_each(|e| builder.push(e).expect("Pushes an inherent"));

	extra_extrinsics
		.into_iter()
		.for_each(|e| builder.push(e).expect("Pushes an extrinsic"));

	let built_block = builder.build().expect("Creates block");

	(
		built_block.block,
		built_block
			.proof
			.expect("We enabled proof recording before."),
	)
}

#[test]
fn validate_block_no_extra_extrinsics() {
	let (client, longest_chain) = create_test_client();
	let parent_head = longest_chain.best_chain().expect("Best block exists");
	let (block, witness_data) = build_block_with_proof(&client, vec![]);
	let (header, extrinsics) = block.deconstruct();

	let block_data = ParachainBlockData::new(header.clone(), extrinsics, witness_data);

	let res_header = call_validate_block(parent_head, block_data).expect("Calls `validate_block`");
	assert_eq!(header, res_header);
}

#[test]
fn validate_block_with_extra_extrinsics() {
	let (client, longest_chain) = create_test_client();
	let parent_head = longest_chain.best_chain().expect("Best block exists");
	let extra_extrinsics = vec![
		transfer(&client, Alice, Bob, 69),
		transfer(&client, Bob, Charlie, 100),
		transfer(&client, Charlie, Alice, 500),
	];

	let (block, witness_data) = build_block_with_proof(&client, extra_extrinsics);
	let (header, extrinsics) = block.deconstruct();

	let block_data = ParachainBlockData::new(header.clone(), extrinsics, witness_data);

	let res_header = call_validate_block(parent_head, block_data).expect("Calls `validate_block`");
	assert_eq!(header, res_header);
}

#[test]
#[should_panic(expected = "Calls `validate_block`: Other(\"Trap: Trap { kind: Unreachable }\")")]
fn validate_block_invalid_parent_hash() {
	let (client, longest_chain) = create_test_client();
	let parent_head = longest_chain.best_chain().expect("Best block exists");
	let (block, witness_data) = build_block_with_proof(&client, vec![]);
	let (mut header, extrinsics) = block.deconstruct();
	header.set_parent_hash(Hash::from_low_u64_be(1));

	let block_data = ParachainBlockData::new(header, extrinsics, witness_data);
	call_validate_block(parent_head, block_data).expect("Calls `validate_block`");
}
