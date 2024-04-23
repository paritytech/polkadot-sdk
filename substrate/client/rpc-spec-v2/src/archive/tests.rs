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

use crate::{
	common::events::{
		ArchiveStorageMethodOk, ArchiveStorageResult, PaginatedStorageQuery, StorageQueryType,
		StorageResultType,
	},
	hex_string, MethodResult,
};

use super::{
	archive::{Archive, ArchiveConfig},
	*,
};

use assert_matches::assert_matches;
use codec::{Decode, Encode};
use jsonrpsee::{
	core::EmptyServerParams as EmptyParams, rpc_params, MethodsError as Error, RpcModule,
};
use sc_block_builder::BlockBuilderBuilder;
use sc_client_api::ChildInfo;
use sp_blockchain::HeaderBackend;
use sp_consensus::BlockOrigin;
use sp_core::{Blake2Hasher, Hasher};
use sp_runtime::{
	traits::{Block as BlockT, Header as HeaderT},
	SaturatedConversion,
};
use std::{collections::HashMap, sync::Arc};
use substrate_test_runtime::Transfer;
use substrate_test_runtime_client::{
	prelude::*, runtime, Backend, BlockBuilderExt, Client, ClientBlockImportExt,
};

const CHAIN_GENESIS: [u8; 32] = [0; 32];
const INVALID_HASH: [u8; 32] = [1; 32];
const MAX_PAGINATION_LIMIT: usize = 5;
const MAX_QUERIED_LIMIT: usize = 5;
const KEY: &[u8] = b":mock";
const VALUE: &[u8] = b"hello world";
const CHILD_STORAGE_KEY: &[u8] = b"child";
const CHILD_VALUE: &[u8] = b"child value";

type Header = substrate_test_runtime_client::runtime::Header;
type Block = substrate_test_runtime_client::runtime::Block;

fn setup_api(
	max_descendant_responses: usize,
	max_queried_items: usize,
) -> (Arc<Client<Backend>>, RpcModule<Archive<Backend, Block, Client<Backend>>>) {
	let child_info = ChildInfo::new_default(CHILD_STORAGE_KEY);
	let builder = TestClientBuilder::new().add_extra_child_storage(
		&child_info,
		KEY.to_vec(),
		CHILD_VALUE.to_vec(),
	);
	let backend = builder.backend();
	let client = Arc::new(builder.build());

	let api = Archive::new(
		client.clone(),
		backend,
		CHAIN_GENESIS,
		ArchiveConfig { max_descendant_responses, max_queried_items },
	)
	.into_rpc();

	(client, api)
}

#[tokio::test]
async fn archive_genesis() {
	let (_client, api) = setup_api(MAX_PAGINATION_LIMIT, MAX_QUERIED_LIMIT);

	let genesis: String =
		api.call("archive_unstable_genesisHash", EmptyParams::new()).await.unwrap();
	assert_eq!(genesis, hex_string(&CHAIN_GENESIS));
}

#[tokio::test]
async fn archive_body() {
	let (mut client, api) = setup_api(MAX_PAGINATION_LIMIT, MAX_QUERIED_LIMIT);

	// Invalid block hash.
	let invalid_hash = hex_string(&INVALID_HASH);
	let res: Option<Vec<String>> = api.call("archive_unstable_body", [invalid_hash]).await.unwrap();
	assert!(res.is_none());

	// Import a new block with an extrinsic.
	let mut builder = BlockBuilderBuilder::new(&*client)
		.on_parent_block(client.chain_info().genesis_hash)
		.with_parent_block_number(0)
		.build()
		.unwrap();

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
	let (mut client, api) = setup_api(MAX_PAGINATION_LIMIT, MAX_QUERIED_LIMIT);

	// Invalid block hash.
	let invalid_hash = hex_string(&INVALID_HASH);
	let res: Option<String> = api.call("archive_unstable_header", [invalid_hash]).await.unwrap();
	assert!(res.is_none());

	// Import a new block with an extrinsic.
	let mut builder = BlockBuilderBuilder::new(&*client)
		.on_parent_block(client.chain_info().genesis_hash)
		.with_parent_block_number(0)
		.build()
		.unwrap();

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
	let (client, api) = setup_api(MAX_PAGINATION_LIMIT, MAX_QUERIED_LIMIT);

	let client_height: u32 = client.info().finalized_number.saturated_into();

	let height: u32 =
		api.call("archive_unstable_finalizedHeight", EmptyParams::new()).await.unwrap();

	assert_eq!(client_height, height);
}

#[tokio::test]
async fn archive_hash_by_height() {
	let (mut client, api) = setup_api(MAX_PAGINATION_LIMIT, MAX_QUERIED_LIMIT);

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
	let finalized = BlockBuilderBuilder::new(&*client)
		.on_parent_block(client.chain_info().genesis_hash)
		.with_parent_block_number(0)
		.build()
		.unwrap()
		.build()
		.unwrap()
		.block;
	let finalized_hash = finalized.header.hash();
	client.import(BlockOrigin::Own, finalized.clone()).await.unwrap();
	client.finalize_block(finalized_hash, None).unwrap();

	let block_1 = BlockBuilderBuilder::new(&*client)
		.on_parent_block(finalized.hash())
		.with_parent_block_number(*finalized.header().number())
		.build()
		.unwrap()
		.build()
		.unwrap()
		.block;
	let block_1_hash = block_1.header.hash();
	client.import(BlockOrigin::Own, block_1.clone()).await.unwrap();

	let block_2 = BlockBuilderBuilder::new(&*client)
		.on_parent_block(block_1.hash())
		.with_parent_block_number(*block_1.header().number())
		.build()
		.unwrap()
		.build()
		.unwrap()
		.block;
	let block_2_hash = block_2.header.hash();
	client.import(BlockOrigin::Own, block_2.clone()).await.unwrap();
	let block_3 = BlockBuilderBuilder::new(&*client)
		.on_parent_block(block_2.hash())
		.with_parent_block_number(*block_2.header().number())
		.build()
		.unwrap()
		.build()
		.unwrap()
		.block;
	let block_3_hash = block_3.header.hash();
	client.import(BlockOrigin::Own, block_3.clone()).await.unwrap();

	// Import block 4 fork.
	let mut block_builder = BlockBuilderBuilder::new(&*client)
		.on_parent_block(block_1_hash)
		.with_parent_block_number(*block_1.header().number())
		.build()
		.unwrap();

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
	let (mut client, api) = setup_api(MAX_PAGINATION_LIMIT, MAX_QUERIED_LIMIT);
	let invalid_hash = hex_string(&INVALID_HASH);

	// Invalid parameter (non-hex).
	let err = api
		.call::<_, serde_json::Value>(
			"archive_unstable_call",
			[&invalid_hash, "BabeApi_current_epoch", "0x00X"],
		)
		.await
		.unwrap_err();
	assert_matches!(err, Error::JsonRpc(err) if err.code() == 3001 && err.message().contains("Invalid parameter"));

	// Pass an invalid parameters that cannot be decode.
	let err = api
		.call::<_, serde_json::Value>(
			"archive_unstable_call",
			// 0x0 is invalid.
			[&invalid_hash, "BabeApi_current_epoch", "0x0"],
		)
		.await
		.unwrap_err();
	assert_matches!(err, Error::JsonRpc(err) if err.code() == 3001 && err.message().contains("Invalid parameter"));

	// Invalid hash.
	let result: MethodResult = api
		.call("archive_unstable_call", [&invalid_hash, "BabeApi_current_epoch", "0x00"])
		.await
		.unwrap();
	assert_matches!(result, MethodResult::Err(_));

	let block_1 = BlockBuilderBuilder::new(&*client)
		.on_parent_block(client.chain_info().genesis_hash)
		.with_parent_block_number(0)
		.build()
		.unwrap()
		.build()
		.unwrap()
		.block;
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

#[tokio::test]
async fn archive_storage_hashes_values() {
	let (mut client, api) = setup_api(MAX_PAGINATION_LIMIT, MAX_QUERIED_LIMIT);

	let block = BlockBuilderBuilder::new(&*client)
		.on_parent_block(client.chain_info().genesis_hash)
		.with_parent_block_number(0)
		.build()
		.unwrap()
		.build()
		.unwrap()
		.block;
	client.import(BlockOrigin::Own, block.clone()).await.unwrap();
	let block_hash = format!("{:?}", block.header.hash());
	let key = hex_string(&KEY);

	let items: Vec<PaginatedStorageQuery<String>> = vec![
		PaginatedStorageQuery {
			key: key.clone(),
			query_type: StorageQueryType::DescendantsHashes,
			pagination_start_key: None,
		},
		PaginatedStorageQuery {
			key: key.clone(),
			query_type: StorageQueryType::DescendantsValues,
			pagination_start_key: None,
		},
		PaginatedStorageQuery {
			key: key.clone(),
			query_type: StorageQueryType::Hash,
			pagination_start_key: None,
		},
		PaginatedStorageQuery {
			key: key.clone(),
			query_type: StorageQueryType::Value,
			pagination_start_key: None,
		},
	];

	let result: ArchiveStorageResult = api
		.call("archive_unstable_storage", rpc_params![&block_hash, items.clone()])
		.await
		.unwrap();

	match result {
		ArchiveStorageResult::Ok(ArchiveStorageMethodOk { result, discarded_items }) => {
			// Key has not been imported yet.
			assert_eq!(result.len(), 0);
			assert_eq!(discarded_items, 0);
		},
		_ => panic!("Unexpected result"),
	};

	// Import a block with the given key value pair.
	let mut builder = BlockBuilderBuilder::new(&*client)
		.on_parent_block(block.hash())
		.with_parent_block_number(1)
		.build()
		.unwrap();
	builder.push_storage_change(KEY.to_vec(), Some(VALUE.to_vec())).unwrap();
	let block = builder.build().unwrap().block;
	client.import(BlockOrigin::Own, block.clone()).await.unwrap();

	let block_hash = format!("{:?}", block.header.hash());
	let expected_hash = format!("{:?}", Blake2Hasher::hash(&VALUE));
	let expected_value = hex_string(&VALUE);

	let result: ArchiveStorageResult = api
		.call("archive_unstable_storage", rpc_params![&block_hash, items])
		.await
		.unwrap();

	match result {
		ArchiveStorageResult::Ok(ArchiveStorageMethodOk { result, discarded_items }) => {
			assert_eq!(result.len(), 4);
			assert_eq!(discarded_items, 0);

			assert_eq!(result[0].key, key);
			assert_eq!(result[0].result, StorageResultType::Hash(expected_hash.clone()));
			assert_eq!(result[1].key, key);
			assert_eq!(result[1].result, StorageResultType::Value(expected_value.clone()));
			assert_eq!(result[2].key, key);
			assert_eq!(result[2].result, StorageResultType::Hash(expected_hash));
			assert_eq!(result[3].key, key);
			assert_eq!(result[3].result, StorageResultType::Value(expected_value));
		},
		_ => panic!("Unexpected result"),
	};
}

#[tokio::test]
async fn archive_storage_closest_merkle_value() {
	let (mut client, api) = setup_api(MAX_PAGINATION_LIMIT, MAX_QUERIED_LIMIT);

	/// The core of this test.
	///
	/// Checks keys that are exact match, keys with descendant and keys that should not return
	/// values.
	///
	/// Returns (key, merkle value) pairs.
	async fn expect_merkle_request(
		api: &RpcModule<Archive<Backend, Block, Client<Backend>>>,
		block_hash: String,
	) -> HashMap<String, String> {
		let result: ArchiveStorageResult = api
			.call(
				"archive_unstable_storage",
				rpc_params![
					&block_hash,
					vec![
						PaginatedStorageQuery {
							key: hex_string(b":AAAA"),
							query_type: StorageQueryType::ClosestDescendantMerkleValue,
							pagination_start_key: None,
						},
						PaginatedStorageQuery {
							key: hex_string(b":AAAB"),
							query_type: StorageQueryType::ClosestDescendantMerkleValue,
							pagination_start_key: None,
						},
						// Key with descendant.
						PaginatedStorageQuery {
							key: hex_string(b":A"),
							query_type: StorageQueryType::ClosestDescendantMerkleValue,
							pagination_start_key: None,
						},
						PaginatedStorageQuery {
							key: hex_string(b":AA"),
							query_type: StorageQueryType::ClosestDescendantMerkleValue,
							pagination_start_key: None,
						},
						// Keys below this comment do not produce a result.
						// Key that exceed the keyspace of the trie.
						PaginatedStorageQuery {
							key: hex_string(b":AAAAX"),
							query_type: StorageQueryType::ClosestDescendantMerkleValue,
							pagination_start_key: None,
						},
						PaginatedStorageQuery {
							key: hex_string(b":AAABX"),
							query_type: StorageQueryType::ClosestDescendantMerkleValue,
							pagination_start_key: None,
						},
						// Key that are not part of the trie.
						PaginatedStorageQuery {
							key: hex_string(b":AAX"),
							query_type: StorageQueryType::ClosestDescendantMerkleValue,
							pagination_start_key: None,
						},
						PaginatedStorageQuery {
							key: hex_string(b":AAAX"),
							query_type: StorageQueryType::ClosestDescendantMerkleValue,
							pagination_start_key: None,
						},
					]
				],
			)
			.await
			.unwrap();

		let merkle_values: HashMap<_, _> = match result {
			ArchiveStorageResult::Ok(ArchiveStorageMethodOk { result, .. }) => result
				.into_iter()
				.map(|res| {
					let value = match res.result {
						StorageResultType::ClosestDescendantMerkleValue(value) => value,
						_ => panic!("Unexpected StorageResultType"),
					};
					(res.key, value)
				})
				.collect(),
			_ => panic!("Unexpected result"),
		};

		// Response for AAAA, AAAB, A and AA.
		assert_eq!(merkle_values.len(), 4);

		// While checking for expected merkle values to align,
		// the following will check that the returned keys are
		// expected.

		// Values for AAAA and AAAB are different.
		assert_ne!(
			merkle_values.get(&hex_string(b":AAAA")).unwrap(),
			merkle_values.get(&hex_string(b":AAAB")).unwrap()
		);

		// Values for A and AA should be on the same branch node.
		assert_eq!(
			merkle_values.get(&hex_string(b":A")).unwrap(),
			merkle_values.get(&hex_string(b":AA")).unwrap()
		);
		// The branch node value must be different than the leaf of either
		// AAAA and AAAB.
		assert_ne!(
			merkle_values.get(&hex_string(b":A")).unwrap(),
			merkle_values.get(&hex_string(b":AAAA")).unwrap()
		);
		assert_ne!(
			merkle_values.get(&hex_string(b":A")).unwrap(),
			merkle_values.get(&hex_string(b":AAAB")).unwrap()
		);

		merkle_values
	}

	// Import a new block with storage changes.
	let mut builder = BlockBuilderBuilder::new(&*client)
		.on_parent_block(client.chain_info().genesis_hash)
		.with_parent_block_number(0)
		.build()
		.unwrap();
	builder.push_storage_change(b":AAAA".to_vec(), Some(vec![1; 64])).unwrap();
	builder.push_storage_change(b":AAAB".to_vec(), Some(vec![2; 64])).unwrap();
	let block = builder.build().unwrap().block;
	let block_hash = format!("{:?}", block.header.hash());
	client.import(BlockOrigin::Own, block.clone()).await.unwrap();

	let merkle_values_lhs = expect_merkle_request(&api, block_hash).await;

	// Import a new block with and change AAAB value.
	let mut builder = BlockBuilderBuilder::new(&*client)
		.on_parent_block(block.hash())
		.with_parent_block_number(1)
		.build()
		.unwrap();
	builder.push_storage_change(b":AAAA".to_vec(), Some(vec![1; 64])).unwrap();
	builder.push_storage_change(b":AAAB".to_vec(), Some(vec![3; 64])).unwrap();
	let block = builder.build().unwrap().block;
	let block_hash = format!("{:?}", block.header.hash());
	client.import(BlockOrigin::Own, block.clone()).await.unwrap();

	let merkle_values_rhs = expect_merkle_request(&api, block_hash).await;

	// Change propagated to the root.
	assert_ne!(
		merkle_values_lhs.get(&hex_string(b":A")).unwrap(),
		merkle_values_rhs.get(&hex_string(b":A")).unwrap()
	);
	assert_ne!(
		merkle_values_lhs.get(&hex_string(b":AAAB")).unwrap(),
		merkle_values_rhs.get(&hex_string(b":AAAB")).unwrap()
	);
	// However the AAAA branch leaf remains unchanged.
	assert_eq!(
		merkle_values_lhs.get(&hex_string(b":AAAA")).unwrap(),
		merkle_values_rhs.get(&hex_string(b":AAAA")).unwrap()
	);
}

#[tokio::test]
async fn archive_storage_paginate_iterations() {
	// 1 iteration allowed before pagination kicks in.
	let (mut client, api) = setup_api(1, MAX_QUERIED_LIMIT);

	// Import a new block with storage changes.
	let mut builder = BlockBuilderBuilder::new(&*client)
		.on_parent_block(client.chain_info().genesis_hash)
		.with_parent_block_number(0)
		.build()
		.unwrap();
	builder.push_storage_change(b":m".to_vec(), Some(b"a".to_vec())).unwrap();
	builder.push_storage_change(b":mo".to_vec(), Some(b"ab".to_vec())).unwrap();
	builder.push_storage_change(b":moc".to_vec(), Some(b"abc".to_vec())).unwrap();
	builder.push_storage_change(b":moD".to_vec(), Some(b"abcmoD".to_vec())).unwrap();
	builder.push_storage_change(b":mock".to_vec(), Some(b"abcd".to_vec())).unwrap();
	let block = builder.build().unwrap().block;
	let block_hash = format!("{:?}", block.header.hash());
	client.import(BlockOrigin::Own, block.clone()).await.unwrap();

	// Calling with an invalid hash.
	let invalid_hash = hex_string(&INVALID_HASH);
	let result: ArchiveStorageResult = api
		.call(
			"archive_unstable_storage",
			rpc_params![
				&invalid_hash,
				vec![PaginatedStorageQuery {
					key: hex_string(b":m"),
					query_type: StorageQueryType::DescendantsValues,
					pagination_start_key: None,
				}]
			],
		)
		.await
		.unwrap();
	match result {
		ArchiveStorageResult::Err(_) => (),
		_ => panic!("Unexpected result"),
	};

	// Valid call with storage at the key.
	let result: ArchiveStorageResult = api
		.call(
			"archive_unstable_storage",
			rpc_params![
				&block_hash,
				vec![PaginatedStorageQuery {
					key: hex_string(b":m"),
					query_type: StorageQueryType::DescendantsValues,
					pagination_start_key: None,
				}]
			],
		)
		.await
		.unwrap();
	match result {
		ArchiveStorageResult::Ok(ArchiveStorageMethodOk { result, discarded_items }) => {
			assert_eq!(result.len(), 1);
			assert_eq!(discarded_items, 0);

			assert_eq!(result[0].key, hex_string(b":m"));
			assert_eq!(result[0].result, StorageResultType::Value(hex_string(b"a")));
		},
		_ => panic!("Unexpected result"),
	};

	// Continue with pagination.
	let result: ArchiveStorageResult = api
		.call(
			"archive_unstable_storage",
			rpc_params![
				&block_hash,
				vec![PaginatedStorageQuery {
					key: hex_string(b":m"),
					query_type: StorageQueryType::DescendantsValues,
					pagination_start_key: Some(hex_string(b":m")),
				}]
			],
		)
		.await
		.unwrap();
	match result {
		ArchiveStorageResult::Ok(ArchiveStorageMethodOk { result, discarded_items }) => {
			assert_eq!(result.len(), 1);
			assert_eq!(discarded_items, 0);

			assert_eq!(result[0].key, hex_string(b":mo"));
			assert_eq!(result[0].result, StorageResultType::Value(hex_string(b"ab")));
		},
		_ => panic!("Unexpected result"),
	};

	// Continue with pagination.
	let result: ArchiveStorageResult = api
		.call(
			"archive_unstable_storage",
			rpc_params![
				&block_hash,
				vec![PaginatedStorageQuery {
					key: hex_string(b":m"),
					query_type: StorageQueryType::DescendantsValues,
					pagination_start_key: Some(hex_string(b":mo")),
				}]
			],
		)
		.await
		.unwrap();
	match result {
		ArchiveStorageResult::Ok(ArchiveStorageMethodOk { result, discarded_items }) => {
			assert_eq!(result.len(), 1);
			assert_eq!(discarded_items, 0);

			assert_eq!(result[0].key, hex_string(b":moD"));
			assert_eq!(result[0].result, StorageResultType::Value(hex_string(b"abcmoD")));
		},
		_ => panic!("Unexpected result"),
	};

	// Continue with pagination.
	let result: ArchiveStorageResult = api
		.call(
			"archive_unstable_storage",
			rpc_params![
				&block_hash,
				vec![PaginatedStorageQuery {
					key: hex_string(b":m"),
					query_type: StorageQueryType::DescendantsValues,
					pagination_start_key: Some(hex_string(b":moD")),
				}]
			],
		)
		.await
		.unwrap();
	match result {
		ArchiveStorageResult::Ok(ArchiveStorageMethodOk { result, discarded_items }) => {
			assert_eq!(result.len(), 1);
			assert_eq!(discarded_items, 0);

			assert_eq!(result[0].key, hex_string(b":moc"));
			assert_eq!(result[0].result, StorageResultType::Value(hex_string(b"abc")));
		},
		_ => panic!("Unexpected result"),
	};

	// Continue with pagination.
	let result: ArchiveStorageResult = api
		.call(
			"archive_unstable_storage",
			rpc_params![
				&block_hash,
				vec![PaginatedStorageQuery {
					key: hex_string(b":m"),
					query_type: StorageQueryType::DescendantsValues,
					pagination_start_key: Some(hex_string(b":moc")),
				}]
			],
		)
		.await
		.unwrap();
	match result {
		ArchiveStorageResult::Ok(ArchiveStorageMethodOk { result, discarded_items }) => {
			assert_eq!(result.len(), 1);
			assert_eq!(discarded_items, 0);

			assert_eq!(result[0].key, hex_string(b":mock"));
			assert_eq!(result[0].result, StorageResultType::Value(hex_string(b"abcd")));
		},
		_ => panic!("Unexpected result"),
	};

	// Continue with pagination until no keys are returned.
	let result: ArchiveStorageResult = api
		.call(
			"archive_unstable_storage",
			rpc_params![
				&block_hash,
				vec![PaginatedStorageQuery {
					key: hex_string(b":m"),
					query_type: StorageQueryType::DescendantsValues,
					pagination_start_key: Some(hex_string(b":mock")),
				}]
			],
		)
		.await
		.unwrap();
	match result {
		ArchiveStorageResult::Ok(ArchiveStorageMethodOk { result, discarded_items }) => {
			assert_eq!(result.len(), 0);
			assert_eq!(discarded_items, 0);
		},
		_ => panic!("Unexpected result"),
	};
}

#[tokio::test]
async fn archive_storage_discarded_items() {
	// One query at a time
	let (mut client, api) = setup_api(MAX_PAGINATION_LIMIT, 1);

	// Import a new block with storage changes.
	let mut builder = BlockBuilderBuilder::new(&*client)
		.on_parent_block(client.chain_info().genesis_hash)
		.with_parent_block_number(0)
		.build()
		.unwrap();
	builder.push_storage_change(b":m".to_vec(), Some(b"a".to_vec())).unwrap();
	let block = builder.build().unwrap().block;
	let block_hash = format!("{:?}", block.header.hash());
	client.import(BlockOrigin::Own, block.clone()).await.unwrap();

	// Valid call with storage at the key.
	let result: ArchiveStorageResult = api
		.call(
			"archive_unstable_storage",
			rpc_params![
				&block_hash,
				vec![
					PaginatedStorageQuery {
						key: hex_string(b":m"),
						query_type: StorageQueryType::Value,
						pagination_start_key: None,
					},
					PaginatedStorageQuery {
						key: hex_string(b":m"),
						query_type: StorageQueryType::Hash,
						pagination_start_key: None,
					},
					PaginatedStorageQuery {
						key: hex_string(b":m"),
						query_type: StorageQueryType::Hash,
						pagination_start_key: None,
					}
				]
			],
		)
		.await
		.unwrap();
	match result {
		ArchiveStorageResult::Ok(ArchiveStorageMethodOk { result, discarded_items }) => {
			assert_eq!(result.len(), 1);
			assert_eq!(discarded_items, 2);

			assert_eq!(result[0].key, hex_string(b":m"));
			assert_eq!(result[0].result, StorageResultType::Value(hex_string(b"a")));
		},
		_ => panic!("Unexpected result"),
	};
}
