// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{chain_head::hex_string, MethodResult};

use super::{archive::Archive, *};

use assert_matches::assert_matches;
use codec::{Decode, Encode};
use jsonrpsee::{
	core::error::Error,
	types::{error::CallError, EmptyServerParams as EmptyParams},
	RpcModule,
};
use sc_block_builder::BlockBuilderProvider;
use sp_blockchain::HeaderBackend;
use sp_consensus::BlockOrigin;
use sp_runtime::SaturatedConversion;
use std::sync::Arc;
use substrate_test_runtime::Transfer;
use substrate_test_runtime_client::{
	prelude::*, runtime, Backend, BlockBuilderExt, Client, ClientBlockImportExt,
};

const CHAIN_GENESIS: [u8; 32] = [0; 32];
const INVALID_HASH: [u8; 32] = [1; 32];

type Header = substrate_test_runtime_client::runtime::Header;
type Block = substrate_test_runtime_client::runtime::Block;

fn setup_api() -> (Arc<Client<Backend>>, RpcModule<Archive<Backend, Block, Client<Backend>>>) {
	let builder = TestClientBuilder::new();
	let backend = builder.backend();
	let client = Arc::new(builder.build());

	let api = Archive::new(client.clone(), backend, CHAIN_GENESIS).into_rpc();

	(client, api)
}

#[tokio::test]
async fn archive_genesis() {
	let (_client, api) = setup_api();

	let genesis: String =
		api.call("archive_unstable_genesisHash", EmptyParams::new()).await.unwrap();
	assert_eq!(genesis, hex_string(&CHAIN_GENESIS));
}

#[tokio::test]
async fn archive_body() {
	let (mut client, api) = setup_api();

	// Invalid block hash.
	let invalid_hash = hex_string(&INVALID_HASH);
	let res: Option<Vec<String>> = api.call("archive_unstable_body", [invalid_hash]).await.unwrap();
	assert!(res.is_none());

	// Import a new block with an extrinsic.
	let mut builder = client.new_block(Default::default()).unwrap();
	builder
		.push_transfer(runtime::Transfer {
			from: AccountKeyring::Alice.into(),
			to: AccountKeyring::Ferdie.into(),
			amount: 42,
			nonce: 0,
		})
		.unwrap();
	let block = builder.build().unwrap().block;
	let block_hash = format!("{:?}", block.header.hash());
	client.import(BlockOrigin::Own, block.clone()).await.unwrap();

	let expected_tx = hex_string(&block.extrinsics[0].encode());

	let body: Vec<String> = api.call("archive_unstable_body", [block_hash]).await.unwrap();
	assert_eq!(vec![expected_tx], body);
}

#[tokio::test]
async fn archive_header() {
	let (mut client, api) = setup_api();

	// Invalid block hash.
	let invalid_hash = hex_string(&INVALID_HASH);
	let res: Option<String> = api.call("archive_unstable_header", [invalid_hash]).await.unwrap();
	assert!(res.is_none());

	// Import a new block with an extrinsic.
	let mut builder = client.new_block(Default::default()).unwrap();
	builder
		.push_transfer(runtime::Transfer {
			from: AccountKeyring::Alice.into(),
			to: AccountKeyring::Ferdie.into(),
			amount: 42,
			nonce: 0,
		})
		.unwrap();
	let block = builder.build().unwrap().block;
	let block_hash = format!("{:?}", block.header.hash());
	client.import(BlockOrigin::Own, block.clone()).await.unwrap();

	let header: String = api.call("archive_unstable_header", [block_hash]).await.unwrap();
	let bytes = array_bytes::hex2bytes(&header).unwrap();
	let header: Header = Decode::decode(&mut &bytes[..]).unwrap();
	assert_eq!(header, block.header);
}

#[tokio::test]
async fn archive_finalized_height() {
	let (client, api) = setup_api();

	let client_height: u32 = client.info().finalized_number.saturated_into();

	let height: u32 =
		api.call("archive_unstable_finalizedHeight", EmptyParams::new()).await.unwrap();

	assert_eq!(client_height, height);
}

#[tokio::test]
async fn archive_hash_by_height() {
	let (mut client, api) = setup_api();

	// Genesis height.
	let hashes: Vec<String> = api.call("archive_unstable_hashByHeight", [0]).await.unwrap();
	assert_eq!(hashes, vec![format!("{:?}", client.genesis_hash())]);

	// Block tree:
	// genesis -> finalized -> block 1 -> block 2 -> block 3
	//                      -> block 1 -> block 4
	//
	//                          ^^^ h = N
	//                                     ^^^ h =  N + 1
	//                                                 ^^^ h = N + 2
	let finalized = client.new_block(Default::default()).unwrap().build().unwrap().block;
	let finalized_hash = finalized.header.hash();
	client.import(BlockOrigin::Own, finalized.clone()).await.unwrap();
	client.finalize_block(finalized_hash, None).unwrap();

	let block_1 = client.new_block(Default::default()).unwrap().build().unwrap().block;
	let block_1_hash = block_1.header.hash();
	client.import(BlockOrigin::Own, block_1.clone()).await.unwrap();

	let block_2 = client.new_block(Default::default()).unwrap().build().unwrap().block;
	let block_2_hash = block_2.header.hash();
	client.import(BlockOrigin::Own, block_2.clone()).await.unwrap();
	let block_3 = client.new_block(Default::default()).unwrap().build().unwrap().block;
	let block_3_hash = block_3.header.hash();
	client.import(BlockOrigin::Own, block_3.clone()).await.unwrap();

	// Import block 4 fork.
	let mut block_builder = client.new_block_at(block_1_hash, Default::default(), false).unwrap();
	// This push is required as otherwise block 3 has the same hash as block 1 and won't get
	// imported
	block_builder
		.push_transfer(Transfer {
			from: AccountKeyring::Alice.into(),
			to: AccountKeyring::Ferdie.into(),
			amount: 41,
			nonce: 0,
		})
		.unwrap();
	let block_4 = block_builder.build().unwrap().block;
	let block_4_hash = block_4.header.hash();
	client.import(BlockOrigin::Own, block_4.clone()).await.unwrap();

	// Check finalized height.
	let hashes: Vec<String> = api.call("archive_unstable_hashByHeight", [1]).await.unwrap();
	assert_eq!(hashes, vec![format!("{:?}", finalized_hash)]);

	// Test nonfinalized heights.
	// Height N must include block 1.
	let mut height = block_1.header.number;
	let hashes: Vec<String> = api.call("archive_unstable_hashByHeight", [height]).await.unwrap();
	assert_eq!(hashes, vec![format!("{:?}", block_1_hash)]);

	// Height (N + 1) must include block 2 and 4.
	height += 1;
	let hashes: Vec<String> = api.call("archive_unstable_hashByHeight", [height]).await.unwrap();
	assert_eq!(hashes, vec![format!("{:?}", block_4_hash), format!("{:?}", block_2_hash)]);

	// Height (N + 2) must include block 3.
	height += 1;
	let hashes: Vec<String> = api.call("archive_unstable_hashByHeight", [height]).await.unwrap();
	assert_eq!(hashes, vec![format!("{:?}", block_3_hash)]);

	// Height (N + 3) has no blocks.
	height += 1;
	let hashes: Vec<String> = api.call("archive_unstable_hashByHeight", [height]).await.unwrap();
	assert!(hashes.is_empty());
}

#[tokio::test]
async fn archive_call() {
	let (mut client, api) = setup_api();
	let invalid_hash = hex_string(&INVALID_HASH);

	// Invalid parameter (non-hex).
	let err = api
		.call::<_, serde_json::Value>(
			"archive_unstable_call",
			[&invalid_hash, "BabeApi_current_epoch", "0x00X"],
		)
		.await
		.unwrap_err();
	assert_matches!(err, Error::Call(CallError::Custom(ref err)) if err.code() == 3001 && err.message().contains("Invalid parameter"));

	// Pass an invalid parameters that cannot be decode.
	let err = api
		.call::<_, serde_json::Value>(
			"archive_unstable_call",
			// 0x0 is invalid.
			[&invalid_hash, "BabeApi_current_epoch", "0x0"],
		)
		.await
		.unwrap_err();
	assert_matches!(err, Error::Call(CallError::Custom(ref err)) if err.code() == 3001 && err.message().contains("Invalid parameter"));

	// Invalid hash.
	let result: MethodResult = api
		.call("archive_unstable_call", [&invalid_hash, "BabeApi_current_epoch", "0x00"])
		.await
		.unwrap();
	assert_matches!(result, MethodResult::Err(_));

	let block_1 = client.new_block(Default::default()).unwrap().build().unwrap().block;
	let block_1_hash = block_1.header.hash();
	client.import(BlockOrigin::Own, block_1.clone()).await.unwrap();

	// Valid call.
	let alice_id = AccountKeyring::Alice.to_account_id();
	// Hex encoded scale encoded bytes representing the call parameters.
	let call_parameters = hex_string(&alice_id.encode());
	let result: MethodResult = api
		.call(
			"archive_unstable_call",
			[&format!("{:?}", block_1_hash), "AccountNonceApi_account_nonce", &call_parameters],
		)
		.await
		.unwrap();
	let expected = MethodResult::ok("0x0000000000000000");
	assert_eq!(result, expected);
}
