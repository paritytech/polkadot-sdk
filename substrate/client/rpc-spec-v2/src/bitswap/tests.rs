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
use crate::bitswap::api::StreamEvent;
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
/// `raw` multicodec — used to exercise stage-B (digest-equal but string-different) dedup.
const RAW_CODEC: u64 = 0x55;

/// Create a CIDv1 string from a 32-byte hash digest, using the `dag-pb` codec.
fn make_cid_v1(code: u64, digest: &[u8; 32]) -> String {
	make_cid_v1_with_codec(DAG_PB_CODEC, code, digest)
}

/// Create a CIDv1 string with an explicit multicodec.
fn make_cid_v1_with_codec(codec: u64, hash_code: u64, digest: &[u8; 32]) -> String {
	let mh = cid::multihash::Multihash::<64>::wrap(hash_code, digest)
		.expect("32 bytes fits in Multihash<32>");
	let c = cid::Cid::new_v1(codec, mh);
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

// ------------------------------------------------------------------
// bitswap_unstable_get (and `bitswap_v1_get` legacy alias)
// ------------------------------------------------------------------

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

#[tokio::test]
async fn unstable_get_alias_returns_same_payload() {
	let (ws_client, _handle, mock_client) = setup(false).await;

	let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
	let digest = sp_crypto_hashing::blake2_256(&data);
	mock_client.insert_transaction(H256::from(digest), data.clone());
	let cid_str = make_cid_v1(BLAKE2B_256, &digest);

	let via_unstable: String = ws_client
		.request("bitswap_unstable_get", rpc_params![cid_str.clone()])
		.await
		.unwrap();
	let via_alias: String =
		ws_client.request("bitswap_v1_get", rpc_params![cid_str]).await.unwrap();

	assert_eq!(via_unstable, crate::hex_string(&data));
	assert_eq!(via_unstable, via_alias);
}

#[tokio::test]
async fn get_error_object_has_no_data_field() {
	let (ws_client, _handle, _mock_client) = setup(false).await;

	let cid_str = unknown_cid(0xCD);
	let err = ws_client
		.request::<String, _>("bitswap_unstable_get", rpc_params![cid_str])
		.await
		.unwrap_err();

	match err {
		jsonrpsee::core::ClientError::Call(obj) => {
			assert_eq!(obj.code(), -32810);
			assert!(
				obj.data().is_none(),
				"bitswap error object must not carry a `data` field, got {:?}",
				obj.data()
			);
		},
		other => panic!("Expected CallError, got {other:?}"),
	}
}

// ------------------------------------------------------------------
// bitswap_unstable_stream
// ------------------------------------------------------------------

/// Read the next subscription event, panicking if it is missing or fails to deserialize.
async fn next_event(sub: &mut Subscription<StreamEvent>) -> StreamEvent {
	tokio::time::timeout(Duration::from_secs(2), sub.next())
		.await
		.expect("event arrived within timeout")
		.expect("subscription stream still open")
		.expect("event deserialized successfully")
}

/// Assert that no further events arrive within [`NO_MORE_EVENTS_TIMEOUT`].
async fn assert_no_more_events(sub: &mut Subscription<StreamEvent>) {
	match tokio::time::timeout(NO_MORE_EVENTS_TIMEOUT, sub.next()).await {
		Err(_) => {},   // timeout — quiescent, as expected.
		Ok(None) => {}, // server-initiated close — also fine.
		Ok(Some(Ok(extra))) => panic!("unexpected extra event: {extra:?}"),
		Ok(Some(Err(e))) => panic!("unexpected error after quiescence: {e:?}"),
	}
}

/// Unwrap a `StreamItem` event, panicking on any other variant.
fn assert_stream_item(ev: StreamEvent, expected_cid: &str) -> String {
	match ev {
		StreamEvent::StreamItem { cid, value } => {
			assert_eq!(cid, expected_cid);
			value
		},
		other => panic!("expected StreamItem for {expected_cid}, got {other:?}"),
	}
}

/// Unwrap a `StreamItemError` event, panicking on any other variant.
fn assert_stream_item_error(ev: StreamEvent, expected_cid: &str, expected_code: i32) {
	match ev {
		StreamEvent::StreamItemError { cid, code, .. } => {
			assert_eq!(cid, expected_cid);
			assert_eq!(code, expected_code);
		},
		other => panic!("expected StreamItemError for {expected_cid}, got {other:?}"),
	}
}

/// Assert that the next event is `StreamDone`.
async fn assert_stream_done(sub: &mut Subscription<StreamEvent>) {
	let ev = next_event(sub).await;
	assert!(matches!(ev, StreamEvent::StreamDone), "expected StreamDone, got {ev:?}");
}

#[tokio::test]
async fn stream_happy_path_emits_every_cid() {
	// Spec contract is *arrival order*, not input order. The current implementation
	// happens to deliver in input order because lookups are sequential and synchronous,
	// but pinning that down would break the test as soon as peer-fetch (or any other
	// parallel resolver) lands. Assert set-membership instead.
	let (ws_client, _handle, mock_client) = setup(false).await;

	let cid_a = store_chunk(&mock_client, vec![1u8], BLAKE2B_256);
	let cid_b = store_chunk(&mock_client, vec![2u8], BLAKE2B_256);
	let cid_c = store_chunk(&mock_client, vec![3u8], BLAKE2B_256);
	let cids = vec![cid_a.clone(), cid_b.clone(), cid_c.clone()];

	let mut sub: Subscription<StreamEvent> = ws_client
		.subscribe(
			"bitswap_unstable_stream",
			rpc_params![cids.clone()],
			"bitswap_unstable_unstream",
		)
		.await
		.unwrap();

	let mut seen: HashMap<String, StreamEvent> = HashMap::new();
	for _ in 0..cids.len() {
		let ev = next_event(&mut sub).await;
		match &ev {
			StreamEvent::StreamItem { cid, .. } => {
				assert!(seen.insert(cid.clone(), ev).is_none(), "duplicate item for cid");
			},
			other => panic!("expected StreamItem, got {other:?}"),
		}
	}
	for cid in &cids {
		assert!(seen.contains_key(cid), "missing StreamItem for cid={cid}");
	}
	assert_stream_done(&mut sub).await;
	assert_no_more_events(&mut sub).await;
}

#[tokio::test]
async fn stream_mixed_batch() {
	let (ws_client, _handle, mock_client) = setup(false).await;

	let cid_ok = store_chunk(&mock_client, vec![1u8, 2, 3], BLAKE2B_256);
	let cid_missing = unknown_cid(0x33);
	let cid_invalid = "still-not-a-cid".to_string();

	let cids = vec![cid_ok.clone(), cid_missing.clone(), cid_invalid.clone()];

	let mut sub: Subscription<StreamEvent> = ws_client
		.subscribe("bitswap_unstable_stream", rpc_params![cids], "bitswap_unstable_unstream")
		.await
		.unwrap();

	let _ = assert_stream_item(next_event(&mut sub).await, &cid_ok);
	assert_stream_item_error(next_event(&mut sub).await, &cid_missing, -32810);
	assert_stream_item_error(next_event(&mut sub).await, &cid_invalid, -32602);
	assert_stream_done(&mut sub).await;
	assert_no_more_events(&mut sub).await;
}

#[tokio::test]
async fn stream_during_sync_emits_cached_hits_and_per_cid_backoff() {
	let (ws_client, _handle, mock_client) = setup(true).await;

	// During sync: subscription opens; cached chunks emit as StreamItem, missing chunks
	// emit per-CID FailRetryBackoff. No top-level rejection.
	let cid_in_db = store_chunk(&mock_client, vec![1u8, 2, 3], BLAKE2B_256);
	let cid_missing = unknown_cid(0xBB);

	let mut sub: Subscription<StreamEvent> = ws_client
		.subscribe(
			"bitswap_unstable_stream",
			rpc_params![vec![cid_in_db.clone(), cid_missing.clone()]],
			"bitswap_unstable_unstream",
		)
		.await
		.unwrap();

	let _ = assert_stream_item(next_event(&mut sub).await, &cid_in_db);
	assert_stream_item_error(next_event(&mut sub).await, &cid_missing, -32812);
	assert_stream_done(&mut sub).await;
	assert_no_more_events(&mut sub).await;
}

#[tokio::test]
async fn stream_over_limit_rejects_subscription() {
	let (ws_client, _handle, _mock_client) = setup(false).await;

	let cids: Vec<String> = (0..(crate::bitswap::bitswap::MAX_CIDS_PER_REQUEST as u8 + 1))
		.map(unknown_cid)
		.collect();

	let err = ws_client
		.subscribe::<StreamEvent, _>(
			"bitswap_unstable_stream",
			rpc_params![cids],
			"bitswap_unstable_unstream",
		)
		.await
		.unwrap_err();

	assert_error_code(&err, -32801);
}

#[tokio::test]
async fn stream_duplicate_valid_cids_rejects_subscription() {
	let (ws_client, _handle, mock_client) = setup(false).await;
	let cid = store_chunk(&mock_client, vec![1u8, 2, 3], BLAKE2B_256);

	let err = ws_client
		.subscribe::<StreamEvent, _>(
			"bitswap_unstable_stream",
			rpc_params![vec![cid.clone(), cid]],
			"bitswap_unstable_unstream",
		)
		.await
		.unwrap_err();

	assert_error_code(&err, -32803);
}

#[tokio::test]
async fn stream_duplicate_digest_different_codec_rejects_subscription() {
	// Stage-B dedup: two CID strings that differ only in their multicodec byte
	// (dag-pb vs raw) but decode to the same 32-byte content digest. Stage A
	// (literal string equality) lets them through; stage B (digest equality) must
	// catch them.
	let (ws_client, _handle, _mock_client) = setup(false).await;

	let digest = [0xAB; 32];
	let cid_dag_pb = make_cid_v1(BLAKE2B_256, &digest);
	let cid_raw = make_cid_v1_with_codec(RAW_CODEC, BLAKE2B_256, &digest);
	assert_ne!(cid_dag_pb, cid_raw, "test precondition: codec change must alter the CID string");

	let err = ws_client
		.subscribe::<StreamEvent, _>(
			"bitswap_unstable_stream",
			rpc_params![vec![cid_dag_pb, cid_raw]],
			"bitswap_unstable_unstream",
		)
		.await
		.unwrap_err();

	assert_error_code(&err, -32803);
}

#[tokio::test]
async fn stream_duplicate_malformed_strings_reject_subscription() {
	let (ws_client, _handle, _mock_client) = setup(false).await;

	// Two literally-identical malformed strings — caught by the string-stage dedup
	// before any parsing happens. Spec: top-level rejection with -32803, even if the
	// strings would individually fail to parse.
	let cids = vec!["bad-cid".to_string(), "bad-cid".to_string()];
	let err = ws_client
		.subscribe::<StreamEvent, _>(
			"bitswap_unstable_stream",
			rpc_params![cids],
			"bitswap_unstable_unstream",
		)
		.await
		.unwrap_err();

	assert_error_code(&err, -32803);
}

#[tokio::test]
async fn stream_distinct_malformed_strings_per_cid_invalid() {
	let (ws_client, _handle, _mock_client) = setup(false).await;

	// Two different malformed strings — share no identity to dedup against, so each
	// produces a per-CID StreamItemError instead of a top-level rejection. Stream still
	// completes with `streamDone`.
	let cids = vec!["bad-1".to_string(), "bad-2".to_string()];
	let mut sub: Subscription<StreamEvent> = ws_client
		.subscribe("bitswap_unstable_stream", rpc_params![cids], "bitswap_unstable_unstream")
		.await
		.unwrap();

	assert_stream_item_error(next_event(&mut sub).await, "bad-1", -32602);
	assert_stream_item_error(next_event(&mut sub).await, "bad-2", -32602);
	assert_stream_done(&mut sub).await;
	assert_no_more_events(&mut sub).await;
}

#[tokio::test]
async fn stream_empty_input_rejects_subscription() {
	let (ws_client, _handle, _mock_client) = setup(false).await;

	let cids: Vec<String> = vec![];
	let err = ws_client
		.subscribe::<StreamEvent, _>(
			"bitswap_unstable_stream",
			rpc_params![cids],
			"bitswap_unstable_unstream",
		)
		.await
		.unwrap_err();

	assert_error_code(&err, -32802);
}

#[tokio::test]
async fn stream_drop_does_not_panic_server() {
	let (ws_client, _handle, mock_client) = setup(false).await;

	// Long batch so the loop is in progress when we drop, exercising the
	// `sink.send().await.is_err()` early-return path in the handler.
	let mut cids = Vec::new();
	for i in 0..32u8 {
		cids.push(store_chunk(&mock_client, vec![i, i, i, i], BLAKE2B_256));
	}

	let mut sub: Subscription<StreamEvent> = ws_client
		.subscribe(
			"bitswap_unstable_stream",
			rpc_params![cids.clone()],
			"bitswap_unstable_unstream",
		)
		.await
		.unwrap();

	// Read a couple of events so we know the stream is alive, then drop to unsubscribe.
	// The spec requires that `streamDone` is NOT emitted on cancellation; that property is
	// enforced in `bitswap_unstable_stream` by returning before the final `streamDone` send
	// whenever any prior `sink.send` returned `Err`. We can't reliably observe it on the
	// wire — jsonrpsee can buffer the entire short batch (including `streamDone`) locally
	// before the client's drop reaches the server, and the spec explicitly permits
	// notifications to race with the unsubscribe call. So this test only verifies that
	// dropping the subscription doesn't panic the server task.
	let _ = next_event(&mut sub).await;
	let _ = next_event(&mut sub).await;
	drop(sub);
}

#[tokio::test]
async fn stream_event_wire_shape_matches_spec() {
	let (ws_client, _handle, mock_client) = setup(false).await;

	let cid_ok = store_chunk(&mock_client, vec![0x68, 0x69], BLAKE2B_256);
	let cid_missing = unknown_cid(0xEE);

	// Subscribe with `serde_json::Value` so we see the raw wire shape.
	let mut sub: Subscription<serde_json::Value> = ws_client
		.subscribe(
			"bitswap_unstable_stream",
			rpc_params![vec![cid_ok.clone(), cid_missing.clone()]],
			"bitswap_unstable_unstream",
		)
		.await
		.unwrap();

	let first = sub.next().await.unwrap().unwrap();
	let obj = first.as_object().expect("event must be an object");
	assert_eq!(obj.get("event").and_then(|v| v.as_str()).unwrap(), "streamItem");
	assert_eq!(obj.get("cid").and_then(|v| v.as_str()).unwrap(), cid_ok);
	let value = obj.get("value").and_then(|v| v.as_str()).unwrap();
	assert!(value.starts_with("0x"), "value must be 0x-prefixed hex, got {value}");
	assert!(!obj.contains_key("data"), "streamItem must not carry `data`");

	let second = sub.next().await.unwrap().unwrap();
	let obj = second.as_object().expect("event must be an object");
	assert_eq!(obj.get("event").and_then(|v| v.as_str()).unwrap(), "streamItemError");
	assert_eq!(obj.get("cid").and_then(|v| v.as_str()).unwrap(), cid_missing);
	assert_eq!(obj.get("code").and_then(|v| v.as_i64()).unwrap(), -32810);
	assert!(obj.get("message").and_then(|v| v.as_str()).is_some(), "message must be present");
	assert!(!obj.contains_key("data"), "streamItemError must not carry `data`");

	let third = sub.next().await.unwrap().unwrap();
	let obj = third.as_object().expect("event must be an object");
	assert_eq!(obj.get("event").and_then(|v| v.as_str()).unwrap(), "streamDone");
	assert_eq!(obj.len(), 1, "streamDone must only carry the `event` key, got {obj:?}");
}
