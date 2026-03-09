// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Integration tests for the `cumulus-client-jam` crate.
//!
//! These tests require a running JAM testnet with RPC available at `ws://localhost:19800`.
//! Start one with:
//!
//! ```sh
//! POLKAVM_BACKEND=interpreter polkajam-testnet --base-rpc-port 19800 --num-ordinary-nodes 1
//! ```

use cumulus_client_jam::{JamClient, JamClientConfig};
use jam_std_common::Node;
use jam_types::{
	AuthConfig, Authorization, Authorizer, ProtocolParameters, RefineContext, WorkItem,
	WorkPackage,
};

fn rpc_url() -> String {
	std::env::var("JAM_RPC_URL").unwrap_or_else(|_| "ws://localhost:19800".into())
}

async fn connect() -> JamClient {
	let config = JamClientConfig { url: rpc_url() };
	JamClient::connect(config).await.expect("failed to connect to JAM node")
}

#[tokio::test]
async fn connect_to_jam_node() {
	let client = connect().await;
	assert_eq!(client.config().url, rpc_url());
}

#[tokio::test]
async fn best_block_returns_valid_data() {
	let client = connect().await;
	let best = client.best_block().await.expect("best_block failed");

	// Slot must be non-zero on a running chain
	assert!(best.slot > 0, "expected non-zero slot, got {}", best.slot);
	// Header hash must not be all zeros
	assert_ne!(best.header_hash, Default::default(), "header_hash should not be zeroed");
}

#[tokio::test]
async fn finalized_block_returns_valid_data() {
	let client = connect().await;
	let finalized = client.finalized_block().await.expect("finalized_block failed");

	assert!(finalized.slot > 0, "expected non-zero finalized slot, got {}", finalized.slot);
	assert_ne!(finalized.header_hash, Default::default());
}

#[tokio::test]
async fn best_block_slot_advances() {
	let client = connect().await;
	let first = client.best_block().await.expect("first best_block failed");

	// Wait for a new block (JAM slot time is 6 seconds)
	tokio::time::sleep(std::time::Duration::from_secs(7)).await;

	let second = client.best_block().await.expect("second best_block failed");
	assert!(second.slot > first.slot, "slot did not advance: {} -> {}", first.slot, second.slot);
}

#[tokio::test]
async fn state_root_for_best_block() {
	let client = connect().await;
	let best = client.best_block().await.expect("best_block failed");
	let state_root = client.state_root(best.header_hash).await.expect("state_root failed");

	// State root must not be all zeros on an initialized chain
	assert_ne!(state_root, Default::default(), "state_root should not be zeroed");
}

#[tokio::test]
async fn beefy_root_for_best_block() {
	let client = connect().await;
	let best = client.best_block().await.expect("best_block failed");
	let beefy_root = client.beefy_root(best.header_hash).await.expect("beefy_root failed");

	// BEEFY root should exist for any block
	assert_ne!(beefy_root, Default::default(), "beefy_root should not be zeroed");
}

#[tokio::test]
async fn parameters_via_node_trait() {
	let client = connect().await;
	// Use the Node trait directly through the inner WsClient
	let params = Node::parameters(client.inner()).await.expect("parameters failed");

	match params {
		jam_std_common::VersionedParameters::V1(p) => {
			// Tiny dev chain has 6 validators and 2 cores
			assert_eq!(p.val_count, 6, "expected 6 validators in tiny dev chain");
			assert_eq!(p.core_count, 2, "expected 2 cores in tiny dev chain");
		},
	}
}

#[tokio::test]
async fn list_services() {
	let client = connect().await;
	let best = client.best_block().await.expect("best_block failed");
	// This should not error even if the list is empty
	let services = Node::list_services(client.inner(), best.header_hash)
		.await
		.expect("list_services failed");
	// On a fresh dev chain there may or may not be services,
	// but the call should succeed
	let _ = services;
}

#[tokio::test]
async fn sync_state() {
	let client = connect().await;
	let sync = Node::sync_state(client.inner()).await.expect("sync_state failed");

	// The ordinary node should be synced with peers
	assert!(sync.num_peers > 0, "ordinary node should have peers, got {}", sync.num_peers);
	assert_eq!(sync.status, jam_std_common::SyncStatus::Completed, "node should be fully synced");
}

#[tokio::test]
async fn connection_to_invalid_url_fails() {
	let config = JamClientConfig { url: "ws://localhost:1".into() };
	let result = JamClient::connect(config).await;
	assert!(result.is_err(), "connecting to invalid URL should fail");
}

fn hash_raw(data: &[u8]) -> jam_types::Hash {
	let h = blake2b_simd::Params::new().hash_length(32).hash(data);
	h.as_bytes().try_into().expect("Hash length set to 32")
}

#[tokio::test]
async fn submit_work_package_to_jam_network() {
	// Apply tiny parameters so BoundedVec limits match the dev chain.
	ProtocolParameters::tiny().apply().unwrap();

	let client = connect().await;

	// Query the bootstrap service (id=0) code hash from the chain.
	let best = client.best_block().await.unwrap();
	let service = client
		.service_data(best.header_hash, 0)
		.await
		.unwrap()
		.expect("service 0 must exist on dev chain");
	let service_code_hash = service.code_hash;

	// The null authorizer code hash is the same as the service code hash on a dev chain
	// built with SKIP_PVM_BUILDS=1 (both are hash of empty bytes).
	let authorizer_code_hash = hash_raw(jam_null_authorizer_bin::BLOB).into();

	// Use the parent of finalized as lookup anchor (avoids finality lag).
	let final_head = client.finalized_block().await.unwrap().header_hash;
	let lookup_block = client.parent(final_head).await.unwrap();
	let lookup_anchor = lookup_block.header_hash;
	let lookup_anchor_slot = lookup_block.slot;

	// Use the parent of best block as anchor (some nodes may not have seen the tip).
	let head = client.best_block().await.unwrap().header_hash;
	let anchor = client.parent(head).await.unwrap().header_hash;
	let state_root = client.state_root(anchor).await.unwrap();
	let beefy_root = client.beefy_root(anchor).await.unwrap();

	// Build payload: Export instruction encoded via jam_bootstrap_service_common.
	use jam_types::Encode;
	let payload_data = b"hello from cumulus-client-jam".to_vec();
	let instruction =
		jam_bootstrap_service_common::Instruction::Export { data: vec![payload_data] };

	let work_item = WorkItem {
		service: 0,
		code_hash: service_code_hash,
		payload: instruction.encode().into(),
		refine_gas_limit: 1_000_000,
		accumulate_gas_limit: 1_000_000,
		import_segments: Default::default(),
		extrinsics: Default::default(),
		export_count: 1,
	};

	let work_package = WorkPackage {
		authorization: Authorization::new(),
		auth_code_host: 0,
		authorizer: Authorizer { code_hash: authorizer_code_hash, config: AuthConfig(vec![]) },
		context: RefineContext {
			anchor,
			state_root,
			beefy_root,
			lookup_anchor,
			lookup_anchor_slot,
			prerequisites: Default::default(),
		},
		items: vec![work_item].try_into().unwrap(),
	};

	let encoded = work_package.encode();
	let _hash = hash_raw(&encoded);

	// Submit to core 0 via the ordinary node's RPC.
	client
		.submit_work_package(0, encoded.into(), &[])
		.await
		.expect("submit_work_package failed");
}
