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

use super::*;
use crate::bitswap::api::BlockResult;
use jsonrpsee::{
	core::client::{ClientT, Subscription, SubscriptionClientT},
	rpc_params,
	server::ServerBuilder,
};
use sp_core::{testing::TaskExecutor, H256};
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::HashMap,
	sync::{Arc, Mutex},
	time::Duration,
};

/// How long we wait for "no more events" assertions before giving up.
/// A jsonrpsee subscription does not receive a server-initiated close when the
/// server-side sink drops, so we rely on a short timeout to assert quiescence.
const NO_MORE_EVENTS_TIMEOUT: Duration = Duration::from_millis(500);

fn assert_error_code(err: &jsonrpsee::core::ClientError, expected_code: i32) {
	match err {
		jsonrpsee::core::ClientError::Call(obj) => {
			assert_eq!(obj.code(), expected_code, "Unexpected error code: {obj:?}");
		},
		other => panic!("Expected CallError, got: {other:?}"),
	}
}

type Block = substrate_test_runtime::Block;

/// Mock BlockBackend that only implements `indexed_transaction`.
struct MockClient {
	transactions: Mutex<HashMap<H256, Vec<u8>>>,
}

impl MockClient {
	fn new() -> Self {
		Self { transactions: Mutex::new(HashMap::new()) }
	}

	fn insert_transaction(&self, hash: H256, data: Vec<u8>) {
		self.transactions.lock().unwrap().insert(hash, data);
	}
}

impl sc_client_api::BlockBackend<Block> for MockClient {
	fn block_body(
		&self,
		_hash: <Block as BlockT>::Hash,
	) -> sp_blockchain::Result<Option<Vec<<Block as BlockT>::Extrinsic>>> {
		unimplemented!()
	}

	fn block(
		&self,
		_hash: <Block as BlockT>::Hash,
	) -> sp_blockchain::Result<Option<sp_runtime::generic::SignedBlock<Block>>> {
		unimplemented!()
	}

	fn block_status(
		&self,
		_hash: <Block as BlockT>::Hash,
	) -> sp_blockchain::Result<sp_consensus::BlockStatus> {
		unimplemented!()
	}

	fn justifications(
		&self,
		_hash: <Block as BlockT>::Hash,
	) -> sp_blockchain::Result<Option<sp_runtime::Justifications>> {
		unimplemented!()
	}

	fn block_hash(
		&self,
		_number: sp_runtime::traits::NumberFor<Block>,
	) -> sp_blockchain::Result<Option<<Block as BlockT>::Hash>> {
		unimplemented!()
	}

	fn indexed_transaction(&self, hash: H256) -> sp_blockchain::Result<Option<Vec<u8>>> {
		Ok(self.transactions.lock().unwrap().get(&hash).cloned())
	}

	fn has_indexed_transaction(&self, hash: H256) -> sp_blockchain::Result<bool> {
		Ok(self.transactions.lock().unwrap().contains_key(&hash))
	}

	fn block_indexed_body(
		&self,
		_hash: <Block as BlockT>::Hash,
	) -> sp_blockchain::Result<Option<Vec<Vec<u8>>>> {
		unimplemented!()
	}

	fn block_indexed_hashes(
		&self,
		_hash: <Block as BlockT>::Hash,
	) -> sp_blockchain::Result<Option<Vec<H256>>> {
		unimplemented!()
	}

	fn requires_full_sync(&self) -> bool {
		false
	}
}

/// Mock SyncOracle with configurable `is_major_syncing` flag.
struct MockSyncOracle {
	major_syncing: bool,
}

impl MockSyncOracle {
	fn new(major_syncing: bool) -> Self {
		Self { major_syncing }
	}
}

impl sp_consensus::SyncOracle for MockSyncOracle {
	fn is_major_syncing(&self) -> bool {
		self.major_syncing
	}

	fn is_offline(&self) -> bool {
		false
	}
}

/// Blake2b-256 multihash code.
const BLAKE2B_256: u64 = 0xb220;

/// Sha2-256 multihash code.
const SHA2_256: u64 = 0x12;

/// `dag-pb` multicodec — used for the CID `codec` field in test CIDs.
/// See <https://github.com/multiformats/multicodec/blob/master/table.csv>.
const DAG_PB_CODEC: u64 = 0x70;

/// Create a CIDv1 string from a 32-byte hash digest.
fn make_cid_v1(code: u64, digest: &[u8; 32]) -> String {
	let mh = cid::multihash::Multihash::<64>::wrap(code, digest)
		.expect("32 bytes fits in Multihash<32>");
	let c = cid::Cid::new_v1(DAG_PB_CODEC, mh);
	c.to_string()
}

/// Create a CIDv0 string.
fn make_cid_v0() -> String {
	// CIDv0 is a bare base58btc-encoded multihash (SHA2-256)
	let digest = [0u8; 32];
	let mh = cid::multihash::Multihash::<64>::wrap(SHA2_256, &digest)
		.expect("32 bytes fits in Multihash<32>");
	let c = cid::Cid::new_v0(mh).expect("SHA2-256 is valid for CIDv0");
	c.to_string()
}

/// Create a CIDv1 with a non-32-byte digest.
fn make_cid_v1_short_digest() -> String {
	let digest = [0u8; 16];
	let mh = cid::multihash::Multihash::<64>::wrap(BLAKE2B_256, &digest)
		.expect("16 bytes fits in Multihash<64>");
	let c = cid::Cid::new_v1(DAG_PB_CODEC, mh);
	c.to_string()
}

/// Create a CIDv1 string with unsupported multihash code.
fn make_cid_v1_unsupported_hash_function() -> String {
	let digest = [0u8; 32];
	let mh = cid::multihash::Multihash::<64>::wrap(0x1b, &digest)
		.expect("32 bytes fits in Multihash<64>");
	let c = cid::Cid::new_v1(DAG_PB_CODEC, mh);
	c.to_string()
}

async fn setup(
	major_syncing: bool,
) -> (jsonrpsee::ws_client::WsClient, jsonrpsee::server::ServerHandle, Arc<MockClient>) {
	let client = Arc::new(MockClient::new());
	let sync_oracle = Arc::new(MockSyncOracle::new(major_syncing));
	let executor = Arc::new(TaskExecutor::default());

	let bitswap = Bitswap::<Block, _>::new(client.clone(), sync_oracle, executor);

	let server = ServerBuilder::default().build("127.0.0.1:0").await.unwrap();
	let addr = server.local_addr().unwrap();
	let handle = server.start(bitswap.into_rpc());

	let url = format!("ws://{}", addr);
	let ws_client = jsonrpsee::ws_client::WsClientBuilder::default().build(&url).await.unwrap();

	(ws_client, handle, client)
}

/// Insert a chunk into the mock client and return its CID.
fn store_chunk(mock_client: &MockClient, data: Vec<u8>, hash_code: u64) -> String {
	let digest = if hash_code == BLAKE2B_256 {
		sp_crypto_hashing::blake2_256(&data)
	} else {
		sp_crypto_hashing::sha2_256(&data)
	};
	mock_client.insert_transaction(H256::from(digest), data);
	make_cid_v1(hash_code, &digest)
}

/// Build a CID for a payload that is *not* stored in the mock client.
fn unknown_cid(seed: u8) -> String {
	let digest = [seed; 32];
	make_cid_v1(BLAKE2B_256, &digest)
}

#[tokio::test]
async fn valid_cid_data_found_sha256() {
	let (ws_client, _handle, mock_client) = setup(false).await;

	let data = vec![1u8, 2, 3, 4, 5];
	let digest = sp_crypto_hashing::sha2_256(&data);
	mock_client.insert_transaction(H256::from(digest), data.clone());

	let cid_str = make_cid_v1(SHA2_256, &digest);
	let result: String = ws_client.request("bitswap_v1_get", rpc_params![cid_str]).await.unwrap();

	assert_eq!(result, crate::hex_string(&data));
}

#[tokio::test]
async fn valid_cid_data_found_blake2b() {
	let (ws_client, _handle, mock_client) = setup(false).await;

	let data = vec![1u8, 2, 3, 4, 5];
	let digest = sp_crypto_hashing::blake2_256(&data);
	mock_client.insert_transaction(H256::from(digest), data.clone());

	let cid_str = make_cid_v1(BLAKE2B_256, &digest);
	let result: String = ws_client.request("bitswap_v1_get", rpc_params![cid_str]).await.unwrap();

	assert_eq!(result, crate::hex_string(&data));
}

#[tokio::test]
async fn valid_cid_not_found_not_syncing() {
	let (ws_client, _handle, _mock_client) = setup(false).await;

	let digest = [42u8; 32];
	let cid_str = make_cid_v1(BLAKE2B_256, &digest);
	let err = ws_client
		.request::<String, _>("bitswap_v1_get", rpc_params![cid_str])
		.await
		.unwrap_err();

	assert_error_code(&err, -32810);
}

#[tokio::test]
async fn valid_cid_not_found_major_syncing() {
	let (ws_client, _handle, _mock_client) = setup(true).await;

	let digest = [42u8; 32];
	let cid_str = make_cid_v1(BLAKE2B_256, &digest);
	let err = ws_client
		.request::<String, _>("bitswap_v1_get", rpc_params![cid_str])
		.await
		.unwrap_err();

	assert_error_code(&err, -32812);
}

#[tokio::test]
async fn invalid_cid_string() {
	let (ws_client, _handle, _mock_client) = setup(false).await;

	let err = ws_client
		.request::<String, _>("bitswap_v1_get", rpc_params!["not-a-valid-cid"])
		.await
		.unwrap_err();

	assert_error_code(&err, -32602);
}

#[tokio::test]
async fn cid_v0_rejected() {
	let (ws_client, _handle, _mock_client) = setup(false).await;

	let cid_str = make_cid_v0();
	let err = ws_client
		.request::<String, _>("bitswap_v1_get", rpc_params![cid_str])
		.await
		.unwrap_err();

	assert_error_code(&err, -32602);
}

#[tokio::test]
async fn cid_v1_unsupported_hash_function_rejected() {
	let (ws_client, _handle, _mock_client) = setup(false).await;

	let cid_str = make_cid_v1_unsupported_hash_function();
	let err = ws_client
		.request::<String, _>("bitswap_v1_get", rpc_params![cid_str])
		.await
		.unwrap_err();

	assert_error_code(&err, -32602);
}

#[tokio::test]
async fn cid_v1_non_32_byte_digest_rejected() {
	let (ws_client, _handle, _mock_client) = setup(false).await;

	let cid_str = make_cid_v1_short_digest();
	let err = ws_client
		.request::<String, _>("bitswap_v1_get", rpc_params![cid_str])
		.await
		.unwrap_err();

	assert_error_code(&err, -32602);
}

// ------------------------------------------------------------------
// bitswap_v1_getMany
// ------------------------------------------------------------------

#[tokio::test]
async fn get_many_happy_path_preserves_order() {
	let (ws_client, _handle, mock_client) = setup(false).await;

	let cid_a = store_chunk(&mock_client, vec![1u8, 2, 3], BLAKE2B_256);
	let cid_b = store_chunk(&mock_client, vec![4u8, 5, 6, 7], SHA2_256);
	let cid_c = store_chunk(&mock_client, vec![8u8, 9], BLAKE2B_256);

	let cids = vec![cid_a.clone(), cid_b.clone(), cid_c.clone()];
	let result: Vec<(String, BlockResult)> = ws_client
		.request("bitswap_v1_getMany", rpc_params![cids.clone()])
		.await
		.unwrap();

	assert_eq!(result.len(), 3);
	assert_eq!(result[0].0, cid_a);
	assert_eq!(result[1].0, cid_b);
	assert_eq!(result[2].0, cid_c);

	let expected: Vec<Vec<u8>> = vec![vec![1, 2, 3], vec![4, 5, 6, 7], vec![8, 9]];
	for (entry, exp) in result.iter().zip(expected) {
		match &entry.1 {
			BlockResult::Ok(data) => assert_eq!(data, &crate::hex_string(&exp)),
			other @ BlockResult::Err { .. } =>
				panic!("Expected Ok at {}, got error: {other:?}", entry.0),
		}
	}
}

#[tokio::test]
async fn get_many_mixed_batch() {
	let (ws_client, _handle, mock_client) = setup(false).await;

	let cid_present = store_chunk(&mock_client, vec![10u8, 20, 30], BLAKE2B_256);
	let cid_missing = unknown_cid(0xAA);
	let cid_invalid = "definitely-not-a-cid".to_string();

	let cids = vec![cid_present.clone(), cid_missing.clone(), cid_invalid.clone()];
	let result: Vec<(String, BlockResult)> = ws_client
		.request("bitswap_v1_getMany", rpc_params![cids])
		.await
		.unwrap();

	assert_eq!(result.len(), 3);
	assert_eq!(result[0].0, cid_present);
	assert!(matches!(result[0].1, BlockResult::Ok(_)), "expected Ok, got {:?}", result[0].1);

	assert_eq!(result[1].0, cid_missing);
	match &result[1].1 {
		BlockResult::Err { code, .. } => assert_eq!(*code, -32810, "expected Fail for missing CID"),
		other => panic!("expected NotFound error, got {other:?}"),
	}

	assert_eq!(result[2].0, cid_invalid);
	match &result[2].1 {
		BlockResult::Err { code, .. } =>
			assert_eq!(*code, -32602, "expected InvalidParams for malformed CID"),
		other => panic!("expected InvalidCid error, got {other:?}"),
	}
}

#[tokio::test]
async fn get_many_all_missing() {
	let (ws_client, _handle, _mock_client) = setup(false).await;

	let cids = vec![unknown_cid(1), unknown_cid(2), unknown_cid(3)];
	let result: Vec<(String, BlockResult)> =
		ws_client.request("bitswap_v1_getMany", rpc_params![cids]).await.unwrap();

	assert_eq!(result.len(), 3);
	for entry in &result {
		match &entry.1 {
			BlockResult::Err { code, .. } => assert_eq!(*code, -32810),
			BlockResult::Ok(_) => panic!("did not expect Ok for unknown CID"),
		}
	}
}

#[tokio::test]
async fn get_many_during_sync_serves_cached_hits_and_per_cid_backoff_for_missing() {
	let (ws_client, _handle, mock_client) = setup(true).await;

	// During sync: chunks already in the local DB are served as `Ok`; misses surface
	// per-CID `FailRetryBackoff` so the caller knows to retry after sync completes.
	let cid_in_db = store_chunk(&mock_client, vec![1u8, 2, 3], BLAKE2B_256);
	let cid_missing = unknown_cid(0xAA);

	let result: Vec<(String, BlockResult)> = ws_client
		.request(
			"bitswap_v1_getMany",
			rpc_params![vec![cid_in_db.clone(), cid_missing.clone()]],
		)
		.await
		.unwrap();

	assert_eq!(result.len(), 2);
	assert_eq!(result[0].0, cid_in_db);
	assert!(matches!(result[0].1, BlockResult::Ok(_)), "expected Ok, got {:?}", result[0].1);

	assert_eq!(result[1].0, cid_missing);
	match &result[1].1 {
		BlockResult::Err { code, .. } => assert_eq!(*code, -32812, "expected FailRetryBackoff"),
		other => panic!("expected per-CID FailRetryBackoff, got {other:?}"),
	}
}

#[tokio::test]
async fn get_many_over_limit_top_level_error() {
	let (ws_client, _handle, _mock_client) = setup(false).await;

	let cids: Vec<String> = (0..(crate::bitswap::bitswap::MAX_CIDS_PER_REQUEST as u8 + 1))
		.map(unknown_cid)
		.collect();

	let err = ws_client
		.request::<Vec<(String, BlockResult)>, _>("bitswap_v1_getMany", rpc_params![cids])
		.await
		.unwrap_err();

	assert_error_code(&err, -32602);
}

#[tokio::test]
async fn get_many_empty_input_returns_empty_vec() {
	let (ws_client, _handle, _mock_client) = setup(false).await;

	let cids: Vec<String> = vec![];
	let result: Vec<(String, BlockResult)> =
		ws_client.request("bitswap_v1_getMany", rpc_params![cids]).await.unwrap();

	assert!(result.is_empty());
}

#[tokio::test]
async fn get_many_wire_shape_matches_documented_format() {
	let (ws_client, _handle, mock_client) = setup(false).await;

	let cid_ok = store_chunk(&mock_client, vec![0xDE, 0xAD, 0xBE, 0xEF], BLAKE2B_256);
	let cid_missing = unknown_cid(0xBB);

	// Request as raw JSON to inspect the wire shape directly.
	let raw: serde_json::Value = ws_client
		.request("bitswap_v1_getMany", rpc_params![vec![cid_ok.clone(), cid_missing.clone()]])
		.await
		.unwrap();

	let arr = raw.as_array().expect("response should be an array");
	assert_eq!(arr.len(), 2);

	// Tuple form: [cid, payload].
	// Ok payload is a hex string; Err payload is `{ "code": ..., "message": "..." }`.
	let tuple_ok = arr[0].as_array().expect("entry should be a 2-array");
	assert_eq!(tuple_ok[0].as_str().unwrap(), cid_ok);
	let ok_str = tuple_ok[1]
		.as_str()
		.unwrap_or_else(|| panic!("Ok payload must be a string, got {:?}", tuple_ok[1]));
	assert!(ok_str.starts_with("0x"), "Ok payload must be 0x-prefixed hex, got {ok_str}");

	let tuple_err = arr[1].as_array().expect("entry should be a 2-array");
	assert_eq!(tuple_err[0].as_str().unwrap(), cid_missing);
	let err_obj = tuple_err[1]
		.as_object()
		.unwrap_or_else(|| panic!("Err payload must be an object, got {:?}", tuple_err[1]));
	assert_eq!(err_obj.get("code").and_then(|v| v.as_i64()).unwrap(), -32810);
	assert!(err_obj.get("message").and_then(|v| v.as_str()).is_some(), "Err must have message");
	assert!(!err_obj.contains_key("data"), "Err must not have data");
}

#[tokio::test]
async fn get_many_duplicate_valid_cids_top_level_error() {
	let (ws_client, _handle, mock_client) = setup(false).await;
	let cid = store_chunk(&mock_client, vec![1u8, 2, 3], BLAKE2B_256);

	let err = ws_client
		.request::<Vec<(String, BlockResult)>, _>(
			"bitswap_v1_getMany",
			rpc_params![vec![cid.clone(), cid]],
		)
		.await
		.unwrap_err();

	assert_error_code(&err, -32602);
}

#[tokio::test]
async fn get_many_duplicate_malformed_strings_top_level_error() {
	let (ws_client, _handle, _mock_client) = setup(false).await;

	// Two literally-identical malformed strings — caught by the string-stage dedup
	// before any parsing happens.
	let cids = vec!["bad-cid".to_string(), "bad-cid".to_string()];
	let err = ws_client
		.request::<Vec<(String, BlockResult)>, _>("bitswap_v1_getMany", rpc_params![cids])
		.await
		.unwrap_err();

	assert_error_code(&err, -32602);
}

#[tokio::test]
async fn get_many_distinct_malformed_strings_per_cid_invalid() {
	let (ws_client, _handle, _mock_client) = setup(false).await;

	// Two different malformed strings — share no identity to dedup against, so each
	// produces a per-CID `InvalidCid` instead of a top-level rejection.
	let cids = vec!["bad-1".to_string(), "bad-2".to_string()];
	let result: Vec<(String, BlockResult)> =
		ws_client.request("bitswap_v1_getMany", rpc_params![cids]).await.unwrap();

	assert_eq!(result.len(), 2);
	for (_, r) in result {
		match r {
			BlockResult::Err { code, .. } => assert_eq!(code, -32602),
			BlockResult::Ok(_) => panic!("expected Err for malformed CID"),
		}
	}
}

// ------------------------------------------------------------------
// bitswap_v1_stream
// ------------------------------------------------------------------

/// Read the next subscription event, panicking if it is missing or fails to deserialize.
async fn next_event(
	sub: &mut Subscription<(String, BlockResult)>,
) -> (String, BlockResult) {
	tokio::time::timeout(Duration::from_secs(2), sub.next())
		.await
		.expect("event arrived within timeout")
		.expect("subscription stream still open")
		.expect("event deserialized successfully")
}

/// Assert that no further events arrive within [`NO_MORE_EVENTS_TIMEOUT`].
async fn assert_no_more_events(sub: &mut Subscription<(String, BlockResult)>) {
	match tokio::time::timeout(NO_MORE_EVENTS_TIMEOUT, sub.next()).await {
		Err(_) => {}, // timeout — quiescent, as expected.
		Ok(None) => {}, // server-initiated close — also fine.
		Ok(Some(Ok(extra))) => panic!("unexpected extra event: {extra:?}"),
		Ok(Some(Err(e))) => panic!("unexpected error after quiescence: {e:?}"),
	}
}

#[tokio::test]
async fn stream_happy_path_emits_in_input_order() {
	let (ws_client, _handle, mock_client) = setup(false).await;

	let cid_a = store_chunk(&mock_client, vec![1u8], BLAKE2B_256);
	let cid_b = store_chunk(&mock_client, vec![2u8], BLAKE2B_256);
	let cid_c = store_chunk(&mock_client, vec![3u8], BLAKE2B_256);
	let cids = vec![cid_a.clone(), cid_b.clone(), cid_c.clone()];

	let mut sub: Subscription<(String, BlockResult)> = ws_client
		.subscribe("bitswap_v1_stream", rpc_params![cids.clone()], "bitswap_v1_unstream")
		.await
		.unwrap();

	for expected_cid in &[cid_a, cid_b, cid_c] {
		let (cid, result) = next_event(&mut sub).await;
		assert_eq!(&cid, expected_cid);
		assert!(matches!(result, BlockResult::Ok(_)));
	}
	assert_no_more_events(&mut sub).await;
}

#[tokio::test]
async fn stream_mixed_batch() {
	let (ws_client, _handle, mock_client) = setup(false).await;

	let cid_ok = store_chunk(&mock_client, vec![1u8, 2, 3], BLAKE2B_256);
	let cid_missing = unknown_cid(0x33);
	let cid_invalid = "still-not-a-cid".to_string();

	let cids = vec![cid_ok.clone(), cid_missing.clone(), cid_invalid.clone()];

	let mut sub: Subscription<(String, BlockResult)> = ws_client
		.subscribe("bitswap_v1_stream", rpc_params![cids], "bitswap_v1_unstream")
		.await
		.unwrap();

	let (c0, r0) = next_event(&mut sub).await;
	assert_eq!(c0, cid_ok);
	assert!(matches!(r0, BlockResult::Ok(_)));

	let (c1, r1) = next_event(&mut sub).await;
	assert_eq!(c1, cid_missing);
	match &r1 {
		BlockResult::Err { code, .. } => assert_eq!(*code, -32810),
		other => panic!("expected NotFound, got {other:?}"),
	}

	let (c2, r2) = next_event(&mut sub).await;
	assert_eq!(c2, cid_invalid);
	match &r2 {
		BlockResult::Err { code, .. } => assert_eq!(*code, -32602),
		other => panic!("expected InvalidCid, got {other:?}"),
	}
	assert_no_more_events(&mut sub).await;
}

#[tokio::test]
async fn stream_during_sync_emits_cached_hits_and_per_cid_backoff() {
	let (ws_client, _handle, mock_client) = setup(true).await;

	// During sync: subscription opens; cached chunks emit as Ok, missing chunks
	// emit per-CID FailRetryBackoff. No top-level rejection.
	let cid_in_db = store_chunk(&mock_client, vec![1u8, 2, 3], BLAKE2B_256);
	let cid_missing = unknown_cid(0xBB);

	let mut sub: Subscription<(String, BlockResult)> = ws_client
		.subscribe(
			"bitswap_v1_stream",
			rpc_params![vec![cid_in_db.clone(), cid_missing.clone()]],
			"bitswap_v1_unstream",
		)
		.await
		.unwrap();

	let (c0, r0) = next_event(&mut sub).await;
	assert_eq!(c0, cid_in_db);
	assert!(matches!(r0, BlockResult::Ok(_)), "expected Ok, got {r0:?}");

	let (c1, r1) = next_event(&mut sub).await;
	assert_eq!(c1, cid_missing);
	match &r1 {
		BlockResult::Err { code, .. } => assert_eq!(*code, -32812, "expected FailRetryBackoff"),
		other => panic!("expected per-CID FailRetryBackoff, got {other:?}"),
	}
	assert_no_more_events(&mut sub).await;
}

#[tokio::test]
async fn stream_over_limit_rejects_subscription() {
	let (ws_client, _handle, _mock_client) = setup(false).await;

	let cids: Vec<String> = (0..(crate::bitswap::bitswap::MAX_CIDS_PER_REQUEST as u8 + 1))
		.map(unknown_cid)
		.collect();

	let err = ws_client
		.subscribe::<(String, BlockResult), _>(
			"bitswap_v1_stream",
			rpc_params![cids],
			"bitswap_v1_unstream",
		)
		.await
		.unwrap_err();

	assert_error_code(&err, -32602);
}

#[tokio::test]
async fn stream_duplicate_valid_cids_rejects_subscription() {
	let (ws_client, _handle, mock_client) = setup(false).await;
	let cid = store_chunk(&mock_client, vec![1u8, 2, 3], BLAKE2B_256);

	let err = ws_client
		.subscribe::<(String, BlockResult), _>(
			"bitswap_v1_stream",
			rpc_params![vec![cid.clone(), cid]],
			"bitswap_v1_unstream",
		)
		.await
		.unwrap_err();

	assert_error_code(&err, -32602);
}

#[tokio::test]
async fn stream_empty_input_emits_no_events() {
	let (ws_client, _handle, _mock_client) = setup(false).await;

	let cids: Vec<String> = vec![];
	let mut sub: Subscription<(String, BlockResult)> = ws_client
		.subscribe("bitswap_v1_stream", rpc_params![cids], "bitswap_v1_unstream")
		.await
		.unwrap();

	// Subscription opens but emits nothing. No server-side close signal is sent
	// (jsonrpsee subscriptions don't get a close on sink-drop), so we assert
	// quiescence via timeout.
	assert_no_more_events(&mut sub).await;
}

#[tokio::test]
async fn stream_unsubscribe_stops_emission() {
	let (ws_client, _handle, mock_client) = setup(false).await;

	// Provide a long batch so there's plenty of time to unsubscribe between events.
	let mut cids = Vec::new();
	for i in 0..32u8 {
		cids.push(store_chunk(&mock_client, vec![i, i, i, i], BLAKE2B_256));
	}

	let mut sub: Subscription<(String, BlockResult)> = ws_client
		.subscribe("bitswap_v1_stream", rpc_params![cids.clone()], "bitswap_v1_unstream")
		.await
		.unwrap();

	// Read a couple of events, then drop the subscription to trigger unsubscribe.
	let _ = sub.next().await;
	let _ = sub.next().await;
	drop(sub);
	// Test passes if the server does not panic; nothing further to assert from the client side.
}
