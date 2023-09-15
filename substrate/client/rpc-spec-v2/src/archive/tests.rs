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

use crate::chain_head::hex_string;

use super::{archive::Archive, *};

use codec::{Decode, Encode};
use jsonrpsee::{types::EmptyServerParams as EmptyParams, RpcModule};
use sc_block_builder::BlockBuilderProvider;

use sp_consensus::BlockOrigin;
use std::sync::Arc;
use substrate_test_runtime_client::{
	prelude::*, runtime, Backend, BlockBuilderExt, Client, ClientBlockImportExt,
};

const CHAIN_GENESIS: [u8; 32] = [0; 32];
const INVALID_HASH: [u8; 32] = [1; 32];

type Header = substrate_test_runtime_client::runtime::Header;
type Block = substrate_test_runtime_client::runtime::Block;

fn setup_api() -> (Arc<Client<Backend>>, RpcModule<Archive<Backend, Block, Client<Backend>>>) {
	let builder = TestClientBuilder::new();
	let client = Arc::new(builder.build());

	let api = Archive::new(client.clone(), CHAIN_GENESIS).into_rpc();

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
