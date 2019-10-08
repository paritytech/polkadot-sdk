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

use rio::TestExternalities;
use keyring::AccountKeyring;
use runtime_primitives::{generic::BlockId, traits::{Block as BlockT, Header as HeaderT}};
use executor::{WasmExecutor, error::Result, wasmi::RuntimeValue::I32};
use test_client::{
	TestClientBuilder, TestClientBuilderExt, DefaultTestClientBuilderExt, Client, LongestChain,
	runtime::{Block, Transfer, Hash, WASM_BINARY, Header}
};
use consensus_common::SelectChain;
use parachain::ValidationParams;

use codec::Encode;

fn call_validate_block(parent_head: Header, block_data: ParachainBlockData<Block>) -> Result<()> {
	let mut ext = TestExternalities::default();
	WasmExecutor::new().call_with_custom_signature(
		&mut ext,
		1024,
		&WASM_BINARY,
		"validate_block",
		|alloc| {
			let params = ValidationParams {
				block_data: block_data.encode(),
				parent_head: parent_head.encode(),
				ingress: Vec::new(),
			}.encode();
			let params_offset = alloc(&params)?;

			Ok(
				vec![
					I32(params_offset as i32),
					I32(params.len() as i32),
				]
			)
		},
		|res, _| {
			if res.is_none() {
				Ok(Some(()))
			} else {
				Ok(None)
			}
		}
	)
}

fn create_extrinsics() -> Vec<<Block as BlockT>::Extrinsic> {
	vec![
		Transfer {
			from: AccountKeyring::Alice.into(),
			to: AccountKeyring::Bob.into(),
			amount: 69,
			nonce: 0,
		}.into_signed_tx(),
		Transfer {
			from: AccountKeyring::Alice.into(),
			to: AccountKeyring::Charlie.into(),
			amount: 100,
			nonce: 1,
		}.into_signed_tx(),
		Transfer {
			from: AccountKeyring::Bob.into(),
			to: AccountKeyring::Charlie.into(),
			amount: 100,
			nonce: 0,
		}.into_signed_tx(),
		Transfer {
			from: AccountKeyring::Charlie.into(),
			to: AccountKeyring::Alice.into(),
			amount: 500,
			nonce: 0,
		}.into_signed_tx(),
	]
}

fn create_test_client() -> (Client, LongestChain) {
	TestClientBuilder::new().build_with_longest_chain()
}

fn build_block_with_proof(
	client: &Client,
	extrinsics: Vec<<Block as BlockT>::Extrinsic>,
) -> (Block, WitnessData) {
	let block_id = BlockId::Hash(client.info().chain.best_hash);
	let mut builder = client.new_block_at_with_proof_recording(
		&block_id,
		Default::default()
	).expect("Initializes new block");

	extrinsics.into_iter().for_each(|e| builder.push(e).expect("Pushes an extrinsic"));

	let (block, proof) = builder
		.bake_and_extract_proof()
		.expect("Finalizes block");

	(block, proof.expect("We enabled proof recording before."))
}

#[test]
fn validate_block_with_no_extrinsics() {
	let (client, longest_chain) = create_test_client();
	let parent_head = longest_chain.best_chain().expect("Best block exists");
	let witness_data_storage_root = *parent_head.state_root();
	let (block, witness_data) = build_block_with_proof(&client, Vec::new());
	let (header, extrinsics) = block.deconstruct();

	let block_data = ParachainBlockData::new(
		header,
		extrinsics,
		witness_data,
		witness_data_storage_root
	);
	call_validate_block(parent_head, block_data).expect("Calls `validate_block`");
}

#[test]
fn validate_block_with_extrinsics() {
	let (client, longest_chain) = create_test_client();
	let parent_head = longest_chain.best_chain().expect("Best block exists");
	let witness_data_storage_root = *parent_head.state_root();
	let (block, witness_data) = build_block_with_proof(&client, create_extrinsics());
	let (header, extrinsics) = block.deconstruct();

	let block_data = ParachainBlockData::new(
		header,
		extrinsics,
		witness_data,
		witness_data_storage_root
	);
	call_validate_block(parent_head, block_data).expect("Calls `validate_block`");
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

	let block_data = ParachainBlockData::new(
		header,
		extrinsics,
		witness_data,
		witness_data_storage_root
	);
	call_validate_block(parent_head, block_data).expect("Calls `validate_block`");
}
