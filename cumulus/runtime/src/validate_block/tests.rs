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

use crate::{ParachainBlockData, WitnessData};

use parachain::{ValidationParams, ValidationResult};
use sc_executor::{call_in_wasm, error::Result, WasmExecutionMethod};
use sp_blockchain::HeaderBackend;
use sp_consensus::SelectChain;
use sp_io::TestExternalities;
use sp_keyring::AccountKeyring;
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, Header as HeaderT},
};
use test_client::{
	runtime::{Block, Hash, Header, Transfer, WASM_BINARY},
	Client, DefaultTestClientBuilderExt, LongestChain, TestClientBuilder, TestClientBuilderExt,
};

use codec::{Decode, Encode};

fn call_validate_block(
	parent_head: Header,
	block_data: ParachainBlockData<Block>,
) -> Result<Header> {
	let mut ext = TestExternalities::default();
	let mut ext_ext = ext.ext();
	let params = ValidationParams {
		block_data: block_data.encode(),
		parent_head: parent_head.encode(),
	}
	.encode();

	call_in_wasm::<
		(
			sp_io::SubstrateHostFunctions,
			sc_executor::deprecated_host_interface::SubstrateExternals,
		),
	>(
		"validate_block",
		&params,
		WasmExecutionMethod::Interpreted,
		&mut ext_ext,
		&WASM_BINARY,
		1024,
		false,
	)
	.map(|v| ValidationResult::decode(&mut &v[..]).expect("Decode `ValidationResult`."))
	.map(|v| Header::decode(&mut &v.head_data[..]).expect("Decode `Header`."))
}

fn create_extrinsics() -> Vec<<Block as BlockT>::Extrinsic> {
	vec![
		Transfer {
			from: AccountKeyring::Alice.into(),
			to: AccountKeyring::Bob.into(),
			amount: 69,
			nonce: 0,
		}
		.into_signed_tx(),
		Transfer {
			from: AccountKeyring::Alice.into(),
			to: AccountKeyring::Charlie.into(),
			amount: 100,
			nonce: 1,
		}
		.into_signed_tx(),
		Transfer {
			from: AccountKeyring::Bob.into(),
			to: AccountKeyring::Charlie.into(),
			amount: 100,
			nonce: 0,
		}
		.into_signed_tx(),
		Transfer {
			from: AccountKeyring::Charlie.into(),
			to: AccountKeyring::Alice.into(),
			amount: 500,
			nonce: 0,
		}
		.into_signed_tx(),
	]
}

fn create_test_client() -> (Client, LongestChain) {
	TestClientBuilder::new().build_with_longest_chain()
}

fn build_block_with_proof(
	client: &Client,
	extrinsics: Vec<<Block as BlockT>::Extrinsic>,
) -> (Block, WitnessData) {
	let block_id = BlockId::Hash(client.info().best_hash);
	let mut builder = client
		.new_block_at(&block_id, Default::default(), true)
		.expect("Initializes new block");

	extrinsics
		.into_iter()
		.for_each(|e| builder.push(e).expect("Pushes an extrinsic"));

	let built_block = builder.build().expect("Creates block");

	(
		built_block.block,
		built_block
			.proof
			.expect("We enabled proof recording before.")
			.iter_nodes()
			.collect(),
	)
}

#[test]
fn validate_block_with_no_extrinsics() {
	let (client, longest_chain) = create_test_client();
	let parent_head = longest_chain.best_chain().expect("Best block exists");
	let witness_data_storage_root = *parent_head.state_root();
	let (block, witness_data) = build_block_with_proof(&client, Vec::new());
	let (header, extrinsics) = block.deconstruct();

	let block_data = ParachainBlockData::new(
		header.clone(),
		extrinsics,
		witness_data,
		witness_data_storage_root,
	);

	let res_header = call_validate_block(parent_head, block_data).expect("Calls `validate_block`");
	assert_eq!(header, res_header);
}

#[test]
fn validate_block_with_extrinsics() {
	let (client, longest_chain) = create_test_client();
	let parent_head = longest_chain.best_chain().expect("Best block exists");
	let witness_data_storage_root = *parent_head.state_root();
	let (block, witness_data) = build_block_with_proof(&client, create_extrinsics());
	let (header, extrinsics) = block.deconstruct();

	let block_data = ParachainBlockData::new(
		header.clone(),
		extrinsics,
		witness_data,
		witness_data_storage_root,
	);

	let res_header = call_validate_block(parent_head, block_data).expect("Calls `validate_block`");
	assert_eq!(header, res_header);
}

#[test]
#[should_panic]
fn validate_block_invalid_parent_hash() {
	let (client, longest_chain) = create_test_client();
	let parent_head = longest_chain.best_chain().expect("Best block exists");
	let witness_data_storage_root = *parent_head.state_root();
	let (block, witness_data) = build_block_with_proof(&client, Vec::new());
	let (mut header, extrinsics) = block.deconstruct();
	header.set_parent_hash(Hash::from_low_u64_be(1));

	let block_data =
		ParachainBlockData::new(header, extrinsics, witness_data, witness_data_storage_root);
	call_validate_block(parent_head, block_data).expect("Calls `validate_block`");
}
