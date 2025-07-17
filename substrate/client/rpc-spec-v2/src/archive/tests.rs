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
	archive::MethodResult,
	common::events::{
		ArchiveStorageDiffEvent, ArchiveStorageDiffItem, ArchiveStorageDiffOperationType,
		ArchiveStorageDiffResult, ArchiveStorageDiffType, ArchiveStorageEvent, StorageQuery,
		StorageQueryType, StorageResult, StorageResultType,
	},
	hex_string,
};

use super::{archive::Archive, *};

use assert_matches::assert_matches;
use codec::{Decode, Encode};
use jsonrpsee::{
	core::{server::Subscription as RpcSubscription, EmptyServerParams as EmptyParams},
	rpc_params, MethodsError as Error, RpcModule,
};

use sc_block_builder::BlockBuilderBuilder;
use sc_client_api::ChildInfo;
use sc_rpc::testing::TokioTestExecutor;
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
const KEY: &[u8] = b":mock";
const VALUE: &[u8] = b"hello world";
const CHILD_STORAGE_KEY: &[u8] = b"child";
const CHILD_VALUE: &[u8] = b"child value";

type Header = substrate_test_runtime_client::runtime::Header;
type Block = substrate_test_runtime_client::runtime::Block;

fn setup_api() -> (Arc<Client<Backend>>, RpcModule<Archive<Backend, Block, Client<Backend>>>) {
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
		Arc::new(TokioTestExecutor::default()),
	)
	.into_rpc();

	(client, api)
}

async fn get_next_event<T: serde::de::DeserializeOwned>(sub: &mut RpcSubscription) -> T {
	let (event, _sub_id) = tokio::time::timeout(std::time::Duration::from_secs(60), sub.next())
		.await
		.unwrap()
		.unwrap()
		.unwrap();
	event
}

#[tokio::test]
async fn archive_genesis() {
	let (_client, api) = setup_api();

	let genesis: String = api.call("archive_v1_genesisHash", EmptyParams::new()).await.unwrap();
	assert_eq!(genesis, hex_string(&CHAIN_GENESIS));
}

#[tokio::test]
async fn archive_body() {
	let (client, api) = setup_api();

	// Invalid block hash.
	let invalid_hash = hex_string(&INVALID_HASH);
	let res: Option<Vec<String>> = api.call("archive_v1_body", [invalid_hash]).await.unwrap();
	assert!(res.is_none());

	// Import a new block with an extrinsic.
	let mut builder = BlockBuilderBuilder::new(&*client)
		.on_parent_block(client.chain_info().genesis_hash)
		.with_parent_block_number(0)
		.build()
		.unwrap();

	builder
		.push_transfer(runtime::Transfer {
			from: Sr25519Keyring::Alice.into(),
			to: Sr25519Keyring::Ferdie.into(),
			amount: 42,
			nonce: 0,
		})
		.unwrap();
	let block = builder.build().unwrap().block;
	let block_hash = format!("{:?}", block.header.hash());
	client.import(BlockOrigin::Own, block.clone()).await.unwrap();

	let expected_tx = hex_string(&block.extrinsics[0].encode());

	let body: Vec<String> = api.call("archive_v1_body", [block_hash]).await.unwrap();
	assert_eq!(vec![expected_tx], body);
}

#[tokio::test]
async fn archive_header() {
	let (client, api) = setup_api();

	// Invalid block hash.
	let invalid_hash = hex_string(&INVALID_HASH);
	let res: Option<String> = api.call("archive_v1_header", [invalid_hash]).await.unwrap();
	assert!(res.is_none());

	// Import a new block with an extrinsic.
	let mut builder = BlockBuilderBuilder::new(&*client)
		.on_parent_block(client.chain_info().genesis_hash)
		.with_parent_block_number(0)
		.build()
		.unwrap();

	builder
		.push_transfer(runtime::Transfer {
			from: Sr25519Keyring::Alice.into(),
			to: Sr25519Keyring::Ferdie.into(),
			amount: 42,
			nonce: 0,
		})
		.unwrap();
	let block = builder.build().unwrap().block;
	let block_hash = format!("{:?}", block.header.hash());
	client.import(BlockOrigin::Own, block.clone()).await.unwrap();

	let header: String = api.call("archive_v1_header", [block_hash]).await.unwrap();
	let bytes = array_bytes::hex2bytes(&header).unwrap();
	let header: Header = Decode::decode(&mut &bytes[..]).unwrap();
	assert_eq!(header, block.header);
}

#[tokio::test]
async fn archive_finalized_height() {
	let (client, api) = setup_api();

	let client_height: u32 = client.info().finalized_number.saturated_into();

	let height: u32 = api.call("archive_v1_finalizedHeight", EmptyParams::new()).await.unwrap();

	assert_eq!(client_height, height);
}

#[tokio::test]
async fn archive_hash_by_height() {
	let (client, api) = setup_api();

	// Genesis height.
	let hashes: Vec<String> = api.call("archive_v1_hashByHeight", [0]).await.unwrap();
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
			from: Sr25519Keyring::Alice.into(),
			to: Sr25519Keyring::Ferdie.into(),
			amount: 41,
			nonce: 0,
		})
		.unwrap();
	let block_4 = block_builder.build().unwrap().block;
	let block_4_hash = block_4.header.hash();
	client.import(BlockOrigin::Own, block_4.clone()).await.unwrap();

	// Check finalized height.
	let hashes: Vec<String> = api.call("archive_v1_hashByHeight", [1]).await.unwrap();
	assert_eq!(hashes, vec![format!("{:?}", finalized_hash)]);

	// Test nonfinalized heights.
	// Height N must include block 1.
	let mut height = block_1.header.number;
	let hashes: Vec<String> = api.call("archive_v1_hashByHeight", [height]).await.unwrap();
	assert_eq!(hashes, vec![format!("{:?}", block_1_hash)]);

	// Height (N + 1) must include block 2 and 4.
	height += 1;
	let hashes: Vec<String> = api.call("archive_v1_hashByHeight", [height]).await.unwrap();
	assert_eq!(hashes, vec![format!("{:?}", block_4_hash), format!("{:?}", block_2_hash)]);

	// Height (N + 2) must include block 3.
	height += 1;
	let hashes: Vec<String> = api.call("archive_v1_hashByHeight", [height]).await.unwrap();
	assert_eq!(hashes, vec![format!("{:?}", block_3_hash)]);

	// Height (N + 3) has no blocks.
	height += 1;
	let hashes: Vec<String> = api.call("archive_v1_hashByHeight", [height]).await.unwrap();
	assert!(hashes.is_empty());
}

#[tokio::test]
async fn archive_call() {
	let (client, api) = setup_api();
	let invalid_hash = hex_string(&INVALID_HASH);

	// Invalid parameter (non-hex).
	let err = api
		.call::<_, serde_json::Value>(
			"archive_v1_call",
			[&invalid_hash, "BabeApi_current_epoch", "0x00X"],
		)
		.await
		.unwrap_err();
	assert_matches!(err, Error::JsonRpc(err) if err.code() == 3001 && err.message().contains("Invalid parameter"));

	// Pass an invalid parameters that cannot be decode.
	let err = api
		.call::<_, serde_json::Value>(
			"archive_v1_call",
			// 0x0 is invalid.
			[&invalid_hash, "BabeApi_current_epoch", "0x0"],
		)
		.await
		.unwrap_err();
	assert_matches!(err, Error::JsonRpc(err) if err.code() == 3001 && err.message().contains("Invalid parameter"));

	// Invalid hash.
	let result: MethodResult = api
		.call("archive_v1_call", [&invalid_hash, "BabeApi_current_epoch", "0x00"])
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
	let alice_id = Sr25519Keyring::Alice.to_account_id();
	// Hex encoded scale encoded bytes representing the call parameters.
	let call_parameters = hex_string(&alice_id.encode());
	let result: MethodResult = api
		.call(
			"archive_v1_call",
			[&format!("{:?}", block_1_hash), "AccountNonceApi_account_nonce", &call_parameters],
		)
		.await
		.unwrap();
	let expected = MethodResult::ok("0x0000000000000000");
	assert_eq!(result, expected);
}

#[tokio::test]
async fn archive_storage_hashes_values() {
	let (client, api) = setup_api();

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

	let items: Vec<StorageQuery<String>> = vec![
		StorageQuery { key: key.clone(), query_type: StorageQueryType::DescendantsHashes },
		StorageQuery { key: key.clone(), query_type: StorageQueryType::DescendantsValues },
		StorageQuery { key: key.clone(), query_type: StorageQueryType::Hash },
		StorageQuery { key: key.clone(), query_type: StorageQueryType::Value },
	];

	let mut sub = api
		.subscribe_unbounded("archive_v1_storage", rpc_params![&block_hash, items.clone()])
		.await
		.unwrap();

	// Key has not been imported yet.
	assert_eq!(
		get_next_event::<ArchiveStorageEvent>(&mut sub).await,
		ArchiveStorageEvent::StorageDone,
	);

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

	let mut sub = api
		.subscribe_unbounded("archive_v1_storage", rpc_params![&block_hash, items])
		.await
		.unwrap();

	assert_eq!(
		get_next_event::<ArchiveStorageEvent>(&mut sub).await,
		ArchiveStorageEvent::Storage(StorageResult {
			key: key.clone(),
			result: StorageResultType::Hash(expected_hash.clone()),
			child_trie_key: None,
		}),
	);

	assert_eq!(
		get_next_event::<ArchiveStorageEvent>(&mut sub).await,
		ArchiveStorageEvent::Storage(StorageResult {
			key: key.clone(),
			result: StorageResultType::Value(expected_value.clone()),
			child_trie_key: None,
		}),
	);

	assert_eq!(
		get_next_event::<ArchiveStorageEvent>(&mut sub).await,
		ArchiveStorageEvent::Storage(StorageResult {
			key: key.clone(),
			result: StorageResultType::Hash(expected_hash),
			child_trie_key: None,
		}),
	);

	assert_eq!(
		get_next_event::<ArchiveStorageEvent>(&mut sub).await,
		ArchiveStorageEvent::Storage(StorageResult {
			key: key.clone(),
			result: StorageResultType::Value(expected_value),
			child_trie_key: None,
		}),
	);

	assert_matches!(
		get_next_event::<ArchiveStorageEvent>(&mut sub).await,
		ArchiveStorageEvent::StorageDone
	);
}

#[tokio::test]
async fn archive_storage_hashes_values_child_trie() {
	let (client, api) = setup_api();

	// Get child storage values set in `setup_api`.
	let child_info = hex_string(&CHILD_STORAGE_KEY);
	let key = hex_string(&KEY);
	let genesis_hash = format!("{:?}", client.genesis_hash());
	let expected_hash = format!("{:?}", Blake2Hasher::hash(&CHILD_VALUE));
	let expected_value = hex_string(&CHILD_VALUE);

	let items: Vec<StorageQuery<String>> = vec![
		StorageQuery { key: key.clone(), query_type: StorageQueryType::DescendantsHashes },
		StorageQuery { key: key.clone(), query_type: StorageQueryType::DescendantsValues },
	];
	let mut sub = api
		.subscribe_unbounded("archive_v1_storage", rpc_params![&genesis_hash, items, &child_info])
		.await
		.unwrap();

	assert_eq!(
		get_next_event::<ArchiveStorageEvent>(&mut sub).await,
		ArchiveStorageEvent::Storage(StorageResult {
			key: key.clone(),
			result: StorageResultType::Hash(expected_hash.clone()),
			child_trie_key: Some(child_info.clone()),
		})
	);

	assert_eq!(
		get_next_event::<ArchiveStorageEvent>(&mut sub).await,
		ArchiveStorageEvent::Storage(StorageResult {
			key: key.clone(),
			result: StorageResultType::Value(expected_value.clone()),
			child_trie_key: Some(child_info.clone()),
		})
	);

	assert_eq!(
		get_next_event::<ArchiveStorageEvent>(&mut sub).await,
		ArchiveStorageEvent::StorageDone,
	);
}

#[tokio::test]
async fn archive_storage_closest_merkle_value() {
	let (client, api) = setup_api();

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
		let mut sub = api
			.subscribe_unbounded(
				"archive_v1_storage",
				rpc_params![
					&block_hash,
					vec![
						StorageQuery {
							key: hex_string(b":AAAA"),
							query_type: StorageQueryType::ClosestDescendantMerkleValue,
						},
						StorageQuery {
							key: hex_string(b":AAAB"),
							query_type: StorageQueryType::ClosestDescendantMerkleValue,
						},
						// Key with descendant.
						StorageQuery {
							key: hex_string(b":A"),
							query_type: StorageQueryType::ClosestDescendantMerkleValue,
						},
						StorageQuery {
							key: hex_string(b":AA"),
							query_type: StorageQueryType::ClosestDescendantMerkleValue,
						},
						// Keys below this comment do not produce a result.
						// Key that exceed the keyspace of the trie.
						StorageQuery {
							key: hex_string(b":AAAAX"),
							query_type: StorageQueryType::ClosestDescendantMerkleValue,
						},
						StorageQuery {
							key: hex_string(b":AAABX"),
							query_type: StorageQueryType::ClosestDescendantMerkleValue,
						},
						// Key that are not part of the trie.
						StorageQuery {
							key: hex_string(b":AAX"),
							query_type: StorageQueryType::ClosestDescendantMerkleValue,
						},
						StorageQuery {
							key: hex_string(b":AAAX"),
							query_type: StorageQueryType::ClosestDescendantMerkleValue,
						},
					]
				],
			)
			.await
			.unwrap();

		let mut merkle_values = HashMap::new();
		loop {
			let event = get_next_event::<ArchiveStorageEvent>(&mut sub).await;
			match event {
				ArchiveStorageEvent::Storage(result) => {
					let str_result = match result.result {
						StorageResultType::ClosestDescendantMerkleValue(value) => value,
						_ => panic!("Unexpected result type"),
					};
					merkle_values.insert(result.key, str_result);
				},
				ArchiveStorageEvent::StorageError(err) => panic!("Unexpected error {err:?}"),
				ArchiveStorageEvent::StorageDone => break,
			}
		}

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
async fn archive_storage_iterations() {
	// 1 iteration allowed before pagination kicks in.
	let (client, api) = setup_api();

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
	let mut sub = api
		.subscribe_unbounded(
			"archive_v1_storage",
			rpc_params![
				&invalid_hash,
				vec![StorageQuery {
					key: hex_string(b":m"),
					query_type: StorageQueryType::DescendantsValues,
				}]
			],
		)
		.await
		.unwrap();

	assert_matches!(
		get_next_event::<ArchiveStorageEvent>(&mut sub).await,
		ArchiveStorageEvent::StorageError(_)
	);

	// Valid call with storage at the key.
	let mut sub = api
		.subscribe_unbounded(
			"archive_v1_storage",
			rpc_params![
				&block_hash,
				vec![StorageQuery {
					key: hex_string(b":m"),
					query_type: StorageQueryType::DescendantsValues,
				}]
			],
		)
		.await
		.unwrap();

	assert_eq!(
		get_next_event::<ArchiveStorageEvent>(&mut sub).await,
		ArchiveStorageEvent::Storage(StorageResult {
			key: hex_string(b":m"),
			result: StorageResultType::Value(hex_string(b"a")),
			child_trie_key: None,
		})
	);

	assert_eq!(
		get_next_event::<ArchiveStorageEvent>(&mut sub).await,
		ArchiveStorageEvent::Storage(StorageResult {
			key: hex_string(b":mo"),
			result: StorageResultType::Value(hex_string(b"ab")),
			child_trie_key: None,
		})
	);

	assert_eq!(
		get_next_event::<ArchiveStorageEvent>(&mut sub).await,
		ArchiveStorageEvent::Storage(StorageResult {
			key: hex_string(b":moD"),
			result: StorageResultType::Value(hex_string(b"abcmoD")),
			child_trie_key: None,
		})
	);

	assert_eq!(
		get_next_event::<ArchiveStorageEvent>(&mut sub).await,
		ArchiveStorageEvent::Storage(StorageResult {
			key: hex_string(b":moc"),
			result: StorageResultType::Value(hex_string(b"abc")),
			child_trie_key: None,
		})
	);

	assert_eq!(
		get_next_event::<ArchiveStorageEvent>(&mut sub).await,
		ArchiveStorageEvent::Storage(StorageResult {
			key: hex_string(b":mock"),
			result: StorageResultType::Value(hex_string(b"abcd")),
			child_trie_key: None,
		})
	);

	assert_matches!(
		get_next_event::<ArchiveStorageEvent>(&mut sub).await,
		ArchiveStorageEvent::StorageDone
	);
}

#[tokio::test]
async fn archive_storage_diff_main_trie() {
	let (client, api) = setup_api();

	let mut builder = BlockBuilderBuilder::new(&*client)
		.on_parent_block(client.chain_info().genesis_hash)
		.with_parent_block_number(0)
		.build()
		.unwrap();
	builder.push_storage_change(b":A".to_vec(), Some(b"B".to_vec())).unwrap();
	builder.push_storage_change(b":AA".to_vec(), Some(b"BB".to_vec())).unwrap();
	let prev_block = builder.build().unwrap().block;
	let prev_hash = format!("{:?}", prev_block.header.hash());
	client.import(BlockOrigin::Own, prev_block.clone()).await.unwrap();

	let mut builder = BlockBuilderBuilder::new(&*client)
		.on_parent_block(prev_block.hash())
		.with_parent_block_number(1)
		.build()
		.unwrap();
	builder.push_storage_change(b":A".to_vec(), Some(b"11".to_vec())).unwrap();
	builder.push_storage_change(b":AA".to_vec(), Some(b"22".to_vec())).unwrap();
	builder.push_storage_change(b":AAA".to_vec(), Some(b"222".to_vec())).unwrap();
	let block = builder.build().unwrap().block;
	let block_hash = format!("{:?}", block.header.hash());
	client.import(BlockOrigin::Own, block.clone()).await.unwrap();

	// Search for items in the main trie:
	// - values of keys under ":A"
	// - hashes of keys under ":AA"
	let items = vec![
		ArchiveStorageDiffItem::<String> {
			key: hex_string(b":A"),
			return_type: ArchiveStorageDiffType::Value,
			child_trie_key: None,
		},
		ArchiveStorageDiffItem::<String> {
			key: hex_string(b":AA"),
			return_type: ArchiveStorageDiffType::Hash,
			child_trie_key: None,
		},
	];
	let mut sub = api
		.subscribe_unbounded(
			"archive_v1_storageDiff",
			rpc_params![&block_hash, items.clone(), &prev_hash],
		)
		.await
		.unwrap();

	let event = get_next_event::<ArchiveStorageDiffEvent>(&mut sub).await;
	assert_eq!(
		ArchiveStorageDiffEvent::StorageDiff(ArchiveStorageDiffResult {
			key: hex_string(b":A"),
			result: StorageResultType::Value(hex_string(b"11")),
			operation_type: ArchiveStorageDiffOperationType::Modified,
			child_trie_key: None,
		}),
		event,
	);

	let event = get_next_event::<ArchiveStorageDiffEvent>(&mut sub).await;
	assert_eq!(
		ArchiveStorageDiffEvent::StorageDiff(ArchiveStorageDiffResult {
			key: hex_string(b":AA"),
			result: StorageResultType::Value(hex_string(b"22")),
			operation_type: ArchiveStorageDiffOperationType::Modified,
			child_trie_key: None,
		}),
		event,
	);

	let event = get_next_event::<ArchiveStorageDiffEvent>(&mut sub).await;
	assert_eq!(
		ArchiveStorageDiffEvent::StorageDiff(ArchiveStorageDiffResult {
			key: hex_string(b":AA"),
			result: StorageResultType::Hash(format!("{:?}", Blake2Hasher::hash(b"22"))),
			operation_type: ArchiveStorageDiffOperationType::Modified,
			child_trie_key: None,
		}),
		event,
	);

	// Added key.
	let event = get_next_event::<ArchiveStorageDiffEvent>(&mut sub).await;
	assert_eq!(
		ArchiveStorageDiffEvent::StorageDiff(ArchiveStorageDiffResult {
			key: hex_string(b":AAA"),
			result: StorageResultType::Value(hex_string(b"222")),
			operation_type: ArchiveStorageDiffOperationType::Added,
			child_trie_key: None,
		}),
		event,
	);

	let event = get_next_event::<ArchiveStorageDiffEvent>(&mut sub).await;
	assert_eq!(
		ArchiveStorageDiffEvent::StorageDiff(ArchiveStorageDiffResult {
			key: hex_string(b":AAA"),
			result: StorageResultType::Hash(format!("{:?}", Blake2Hasher::hash(b"222"))),
			operation_type: ArchiveStorageDiffOperationType::Added,
			child_trie_key: None,
		}),
		event,
	);

	let event = get_next_event::<ArchiveStorageDiffEvent>(&mut sub).await;
	assert_eq!(ArchiveStorageDiffEvent::StorageDiffDone, event);
}

#[tokio::test]
async fn archive_storage_diff_no_changes() {
	let (client, api) = setup_api();

	// Build 2 identical blocks.
	let mut builder = BlockBuilderBuilder::new(&*client)
		.on_parent_block(client.chain_info().genesis_hash)
		.with_parent_block_number(0)
		.build()
		.unwrap();
	builder.push_storage_change(b":A".to_vec(), Some(b"B".to_vec())).unwrap();
	builder.push_storage_change(b":AA".to_vec(), Some(b"BB".to_vec())).unwrap();
	builder.push_storage_change(b":B".to_vec(), Some(b"CC".to_vec())).unwrap();
	builder.push_storage_change(b":BA".to_vec(), Some(b"CC".to_vec())).unwrap();
	let prev_block = builder.build().unwrap().block;
	let prev_hash = format!("{:?}", prev_block.header.hash());
	client.import(BlockOrigin::Own, prev_block.clone()).await.unwrap();

	let mut builder = BlockBuilderBuilder::new(&*client)
		.on_parent_block(prev_block.hash())
		.with_parent_block_number(1)
		.build()
		.unwrap();
	builder.push_storage_change(b":A".to_vec(), Some(b"B".to_vec())).unwrap();
	builder.push_storage_change(b":AA".to_vec(), Some(b"BB".to_vec())).unwrap();
	let block = builder.build().unwrap().block;
	let block_hash = format!("{:?}", block.header.hash());
	client.import(BlockOrigin::Own, block.clone()).await.unwrap();

	// Search for items in the main trie with keys prefixed with ":A".
	let items = vec![ArchiveStorageDiffItem::<String> {
		key: hex_string(b":A"),
		return_type: ArchiveStorageDiffType::Value,
		child_trie_key: None,
	}];
	let mut sub = api
		.subscribe_unbounded(
			"archive_v1_storageDiff",
			rpc_params![&block_hash, items.clone(), &prev_hash],
		)
		.await
		.unwrap();

	let event = get_next_event::<ArchiveStorageDiffEvent>(&mut sub).await;
	assert_eq!(ArchiveStorageDiffEvent::StorageDiffDone, event);
}

#[tokio::test]
async fn archive_storage_diff_deleted_changes() {
	let (client, api) = setup_api();

	// Blocks are imported as forks.
	let mut builder = BlockBuilderBuilder::new(&*client)
		.on_parent_block(client.chain_info().genesis_hash)
		.with_parent_block_number(0)
		.build()
		.unwrap();
	builder.push_storage_change(b":A".to_vec(), Some(b"B".to_vec())).unwrap();
	builder.push_storage_change(b":AA".to_vec(), Some(b"BB".to_vec())).unwrap();
	builder.push_storage_change(b":B".to_vec(), Some(b"CC".to_vec())).unwrap();
	builder.push_storage_change(b":BA".to_vec(), Some(b"CC".to_vec())).unwrap();
	let prev_block = builder.build().unwrap().block;
	let prev_hash = format!("{:?}", prev_block.header.hash());
	client.import(BlockOrigin::Own, prev_block.clone()).await.unwrap();

	let mut builder = BlockBuilderBuilder::new(&*client)
		.on_parent_block(client.chain_info().genesis_hash)
		.with_parent_block_number(0)
		.build()
		.unwrap();
	builder
		.push_transfer(Transfer {
			from: Sr25519Keyring::Alice.into(),
			to: Sr25519Keyring::Ferdie.into(),
			amount: 41,
			nonce: 0,
		})
		.unwrap();
	builder.push_storage_change(b":A".to_vec(), Some(b"B".to_vec())).unwrap();
	let block = builder.build().unwrap().block;
	let block_hash = format!("{:?}", block.header.hash());
	client.import(BlockOrigin::Own, block.clone()).await.unwrap();

	// Search for items in the main trie with keys prefixed with ":A".
	let items = vec![ArchiveStorageDiffItem::<String> {
		key: hex_string(b":A"),
		return_type: ArchiveStorageDiffType::Value,
		child_trie_key: None,
	}];

	let mut sub = api
		.subscribe_unbounded(
			"archive_v1_storageDiff",
			rpc_params![&block_hash, items.clone(), &prev_hash],
		)
		.await
		.unwrap();

	let event = get_next_event::<ArchiveStorageDiffEvent>(&mut sub).await;
	assert_eq!(
		ArchiveStorageDiffEvent::StorageDiff(ArchiveStorageDiffResult {
			key: hex_string(b":AA"),
			result: StorageResultType::Value(hex_string(b"BB")),
			operation_type: ArchiveStorageDiffOperationType::Deleted,
			child_trie_key: None,
		}),
		event,
	);

	let event = get_next_event::<ArchiveStorageDiffEvent>(&mut sub).await;
	assert_eq!(ArchiveStorageDiffEvent::StorageDiffDone, event);
}

#[tokio::test]
async fn archive_storage_diff_invalid_params() {
	let invalid_hash = hex_string(&INVALID_HASH);
	let (_, api) = setup_api();

	// Invalid shape for parameters.
	let items: Vec<ArchiveStorageDiffItem<String>> = Vec::new();
	let err = api
		.subscribe_unbounded(
			"archive_v1_storageDiff",
			rpc_params!["123", items.clone(), &invalid_hash],
		)
		.await
		.unwrap_err();
	assert_matches!(err,
		Error::JsonRpc(ref err) if err.code() == crate::chain_head::error::json_rpc_spec::INVALID_PARAM_ERROR && err.message() == "Invalid params"
	);

	// The shape is right, but the block hash is invalid.
	let items: Vec<ArchiveStorageDiffItem<String>> = Vec::new();
	let mut sub = api
		.subscribe_unbounded(
			"archive_v1_storageDiff",
			rpc_params![&invalid_hash, items.clone(), &invalid_hash],
		)
		.await
		.unwrap();

	let event = get_next_event::<ArchiveStorageDiffEvent>(&mut sub).await;
	assert_matches!(event,
		ArchiveStorageDiffEvent::StorageDiffError(ref err) if err.error.contains("Header was not found")
	);
}
