// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! End-to-end integration test for the `block-additional-data` feature.
//!
//! Proves the full round trip: build -> sync -> db -> re-fetch -> validate ->
//! regression-check, using `cumulus-test-service`'s in-process `TestNode`
//! infrastructure (no zombienet).
//!
//! Sub-assertions:
//! - (a) the collator's own DB stores the additional data for a block whose runtime pushed it
//!   (`Backend::block_additional_data`);
//! - (b) a separate full node syncing from the collator receives `ADDITIONAL_DATA` over sync,
//!   stores it in its own DB, and its generic import path accepted it (replay registration on the
//!   executing import path);
//! - (c) a direct `validate_block` call on the produced `ParachainBlockData::V3` candidate
//!   succeeds;
//! - (d) additional data corrupted "in transit" is rejected. Network-level corruption injection is
//!   impractical in-process, so a corrupted blob is fed directly into `validate_block` (the same
//!   explicit hash-vs-digest assertion that would fire on the generic import path); what was
//!   corrupted and where rejection happened is logged explicitly;
//! - (e) a control block that never calls `push_additional_data` builds, syncs and validates
//!   normally and carries no `AdditionalData` digest in its header (proving chains that do not opt
//!   in are unaffected).

use codec::{Decode, Encode};
use cumulus_client_cli::get_raw_genesis_header;
use cumulus_primitives_core::{
	relay_chain, ParaId, ParachainBlockData, PersistedValidationData, SchedulingProof,
};
use cumulus_test_client::{
	seal_block, BlockBuilderAndSupportData, BlockData, BuildBlockBuilder, BuildParachainBlockData,
	DefaultTestClientBuilderExt, HeadData, TestClientBuilderExt, ValidationParams,
};
use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
use cumulus_test_service::{
	construct_extrinsic, run_relay_chain_validator_node, runtime, Client, Keyring, TestNode,
	TestNodeBuilder, TransactionPool,
};
use polkadot_primitives::HeadData as RelayHeadData;
use sc_client_api::backend::Backend as _;
use sc_transaction_pool_api::{TransactionPool as _, TransactionSource};
use sp_additional_data::{encode_items, hash_blob};
use sp_blockchain::{Backend as _, HeaderBackend};
use sp_runtime::{
	traits::{Block as BlockT, Header as HeaderT},
	OpaqueExtrinsic,
};
use std::time::Duration;

/// The sample bytes the test runtime's `push_additional_data` dispatchable pushes.
///
/// Must match the value hard-coded in `cumulus/test/runtime/src/test_pallet.rs`.
const ADDITIONAL_DATA_SAMPLE: &[u8] = b"additional-data-test";

fn hex(bytes: &[u8]) -> String {
	format!("0x{}", sp_core::hexdisplay::HexDisplay::from(&bytes.to_vec()))
}

/// The `AdditionalData` digest hashes present in a header (at most one is valid).
fn additional_data_digests(header: &runtime::Header) -> Vec<[u8; 32]> {
	header
		.digest()
		.logs()
		.iter()
		.filter_map(|d| d.as_additional_data().copied())
		.collect()
}

/// Wait for `count` imported blocks, failing fast instead of hanging forever.
async fn wait_for_blocks_or_timeout(node: &TestNode, count: u64, label: &str) {
	tokio::time::timeout(Duration::from_secs(300), node.wait_for_blocks(count as usize))
		.await
		.unwrap_or_else(|_| panic!("Timeout waiting for {label} to reach block #{count}"));
}

/// Submit an extrinsic to a node's transaction pool (fire-and-forget; inclusion is
/// detected by polling the chain).
async fn submit_tx(
	tx_pool: &TransactionPool,
	client: &Client,
	function: impl Into<runtime::RuntimeCall>,
) {
	let best_hash = client.chain_info().best_hash;
	let extrinsic =
		OpaqueExtrinsic::from(construct_extrinsic(client, function, Keyring::Alice.pair(), None));
	let _watcher = tx_pool
		.submit_and_watch(best_hash, TransactionSource::External, extrinsic)
		.await
		.expect("Submits tx to pool; qed");
}

/// Poll the canonical chain of `node` for a block whose header carries exactly the
/// given `AdditionalData` digest. Robust against tx-pool watcher races.
async fn wait_for_additional_data_block(
	node: &TestNode,
	expected_digest: [u8; 32],
) -> runtime::Hash {
	let deadline = std::time::Instant::now() + Duration::from_secs(300);
	loop {
		let best_number = u64::from(node.client.chain_info().best_number);
		let mut seen_digests: Vec<([u8; 32], runtime::Hash, u64)> = Vec::new();
		for number in 1..=best_number {
			if let Some(hash) = node.client.hash(number as u32).ok().flatten() {
				if let Some(header) = node.client.header(hash).ok().flatten() {
					let digests = additional_data_digests(&header);
					if digests == vec![expected_digest] {
						return hash;
					}
					for d in digests {
						seen_digests.push((d, hash, number));
					}
				}
			}
		}
		assert!(
			std::time::Instant::now() < deadline,
			"Timeout waiting for a block with the AdditionalData digest. best_number={best_number}, \
			 other digest blocks seen: {:?}",
			seen_digests,
		);
		tokio::time::sleep(Duration::from_millis(500)).await;
	}
}

/// The canonical `runtime::Hash` of the block at `number` on `node`'s canonical chain.
fn block_hash_at(node: &TestNode, number: u32) -> runtime::Hash {
	node.client
		.hash(number)
		.expect("hash query ok; qed")
		.expect("block at number exists; qed")
}

/// A scheduling proof that passes V3 validation for the default test runtime (the
/// scheduling proof is not validated when the `v3-descriptor` feature is off).
fn dummy_scheduling_proof() -> SchedulingProof {
	SchedulingProof {
		header_chain: vec![],
		internal_scheduling_parent_header: relay_chain::Header {
			parent_hash: Default::default(),
			number: 0,
			state_root: Default::default(),
			extrinsics_root: Default::default(),
			digest: Default::default(),
		},
		signed_scheduling_info: None,
	}
}

/// Build a single-block `V3` candidate whose block actually called
/// `push_additional_data` during block building, so the runtime deposited
/// `DigestItem::AdditionalData` into the header at finalization time.
fn build_v3_with_runtime_pushed_additional_data(
	client: &cumulus_test_client::Client,
	parent_head: runtime::Header,
) -> (ParachainBlockData<runtime::Block>, PersistedValidationData) {
	let sproof_builder = RelayStateSproofBuilder {
		para_id: ParaId::from(runtime::PARACHAIN_ID),
		included_para_head: Some(RelayHeadData(parent_head.encode())),
		..Default::default()
	};
	let validation_data = PersistedValidationData {
		relay_parent_number: 1,
		parent_head: parent_head.encode().into(),
		..Default::default()
	};

	let BlockBuilderAndSupportData { mut block_builder, persisted_validation_data, .. } = client
		.init_block_builder_builder()
		.with_validation_data(validation_data)
		.with_relay_sproof_builder(sproof_builder)
		.build();

	block_builder
		.push(cumulus_test_client::generate_extrinsic(
			client,
			Keyring::Alice,
			runtime::TestPalletCall::push_additional_data {},
		))
		.expect("Push extrinsic accepted; qed");

	let mut block = block_builder.build_parachain_block(*parent_head.state_root());
	block.blocks_mut()[0] = seal_block(block.blocks()[0].clone(), client);

	// The runtime deposited the digest at finalization; sanity-check it matches the
	// canonical encoding before handing the candidate to `validate_block`.
	let expected_hash = hash_blob(&encode_items(&[ADDITIONAL_DATA_SAMPLE.to_vec()]));
	let digest_hashes: Vec<[u8; 32]> = additional_data_digests(block.blocks()[0].header());
	assert_eq!(digest_hashes, vec![expected_hash], "header must carry exactly one digest");

	let (blocks, proof) = block.into_inner();
	let v3 = ParachainBlockData::V3 {
		blocks,
		proof,
		scheduling_proof: dummy_scheduling_proof(),
		additional_data: vec![Some(encode_items(&[ADDITIONAL_DATA_SAMPLE.to_vec()]))],
	};
	(v3, persisted_validation_data)
}

/// Call `validate_block` in the reference runtime WASM for the given candidate.
fn call_validate_block(
	parent_head: runtime::Header,
	block_data: ParachainBlockData<runtime::Block>,
	relay_parent_storage_root: runtime::Hash,
) -> cumulus_test_client::ExecutorResult<runtime::Header> {
	cumulus_test_client::validate_block(
		ValidationParams {
			block_data: BlockData(block_data.encode()),
			parent_head: HeadData(parent_head.encode()),
			relay_parent_number: 1,
			relay_parent_storage_root,
		},
		runtime::WASM_BINARY.expect("WASM binary built; qed"),
	)
	.map(|v| runtime::Header::decode(&mut &v.head_data.0[..]).expect("Decodes header; qed"))
}

#[test]
fn block_additional_data_end_to_end() {
	sp_tracing::try_init_simple();

	let runtime = tokio::runtime::Runtime::new().expect("creating tokio runtime; qed");
	let handle = runtime.handle().clone();

	// Two in-process relay-chain validators (a single one stalls the collator after a
	// couple of blocks). They spawn synchronously (block on their own tokio runtime
	// internally), so they must be created outside `block_on`.
	let relay_alice =
		run_relay_chain_validator_node(handle.clone(), Keyring::Alice, || {}, vec![], None);
	let relay_bob = run_relay_chain_validator_node(
		handle.clone(),
		Keyring::Bob,
		|| {},
		vec![relay_alice.addr.clone()],
		None,
	);

	runtime.block_on(async move {
		let para_id = ParaId::from(runtime::PARACHAIN_ID);
		let expected_blob = encode_items(&[ADDITIONAL_DATA_SAMPLE.to_vec()]);
		let expected_digest_hash = hash_blob(&expected_blob);

		// Collator for the reference parachain.
		let collator = TestNodeBuilder::new(para_id, handle.clone(), Keyring::Charlie)
			.enable_collator()
			.connect_to_relay_chain_nodes(vec![&relay_alice, &relay_bob])
			.build()
			.await;

		// Register the parachain on the relay chain so the collator can author.
		relay_alice
			.register_parachain(
				para_id,
				runtime::WASM_BINARY.expect("WASM binary built; qed").to_vec(),
				RelayHeadData(
					get_raw_genesis_header(collator.client.clone()).expect("Genesis header; qed"),
				),
			)
			.await
			.expect("Register parachain; qed");

		// Wait until the collator has produced its first parachain block. Block #1
		// contains only inherents (no user extrinsic, so no `push_additional_data`)
		// and is therefore the real-chain CONTROL block for sub-assertion (e).
		wait_for_blocks_or_timeout(&collator, 1, "collator").await;
		let control_hash = block_hash_at(&collator, 1);
		let control_header = collator
			.client
			.header(control_hash)
			.expect("header query ok; qed")
			.expect("control block header exists; qed");
		let control_digests = additional_data_digests(&control_header);
		let control_collator_db = collator
			.backend
			.blockchain()
			.block_additional_data(control_hash)
			.expect("db ok; qed");
		println!(
			"[e] control block {} (no push): header AdditionalData digests = {:?}, \
			 collator DB additional_data = {:?}",
			control_hash, control_digests, control_collator_db,
		);
		assert!(
			control_digests.is_empty(),
			"(e) control block must not carry an AdditionalData digest"
		);
		assert_eq!(
			control_collator_db, None,
			"(e) collator DB must have no additional data for the control block"
		);
		println!("[e] control block built by collator (no digest, no db entry): PASS");

		// ---------------------------------------------------------------------------
		// (a) Collator's own DB for the push block.
		// ---------------------------------------------------------------------------
		// The push tx goes through the collator's real authoring (todo 11 packs the
		// collected data). It is pool-valid now (the runtime pushes at finalization,
		// not in the dispatchable), so the collator includes it in the next block.
		// Inclusion is detected by polling the canonical chain for the digest block.
		submit_tx(
			&collator.transaction_pool,
			&collator.client,
			runtime::TestPalletCall::push_additional_data {},
		)
		.await;
		let push_hash = wait_for_additional_data_block(&collator, expected_digest_hash).await;
		let push_header = collator
			.client
			.header(push_hash)
			.expect("header query ok; qed")
			.expect("push block header exists; qed");
		let push_digests = additional_data_digests(&push_header);
		let collator_db = collator
			.backend
			.blockchain()
			.block_additional_data(push_hash)
			.expect("db ok; qed");
		println!(
			"[a] push block {} header AdditionalData digests = {:?}, \
			 collator DB additional_data = {:?}",
			push_hash, push_digests, collator_db,
		);
		assert_eq!(
			push_digests,
			vec![expected_digest_hash],
			"(a) push block header must carry exactly the expected AdditionalData digest"
		);
		assert_eq!(
			collator_db.as_deref(),
			Some(expected_blob.as_slice()),
			"(a) collator's own DB must have the additional data"
		);
		println!("[a] collator DB round-trip (build -> persist -> re-fetch): PASS");

		// ---------------------------------------------------------------------------
		// (b) Separate full node syncing from the collator.
		// ---------------------------------------------------------------------------
		let full_node = TestNodeBuilder::new(para_id, handle.clone(), Keyring::Dave)
			.connect_to_relay_chain_nodes(vec![&relay_alice, &relay_bob])
			.connect_to_parachain_node(&collator)
			.build()
			.await;

		let sync_target = u64::from(*push_header.number());
		wait_for_blocks_or_timeout(&full_node, sync_target, "full node").await;

		// The full node must have imported the push block (generic executing import
		// path with the replay provider registered) and stored the additional data it
		// received over sync.
		let full_push_header = full_node
			.client
			.header(push_hash)
			.expect("header query ok; qed")
			.expect("full node must have imported the push block; qed");
		assert_eq!(
			full_push_header.digest().logs(),
			push_header.digest().logs(),
			"(b) full node must import the same block"
		);
		let full_node_db = full_node
			.backend
			.blockchain()
			.block_additional_data(push_hash)
			.expect("db ok; qed");
		println!(
			"[b] full node {} synced to #{}, block {} DB additional_data = {:?}",
			full_node.client.chain_info().best_hash,
			full_node.client.chain_info().best_number,
			push_hash,
			full_node_db,
		);
		assert_eq!(
			full_node_db.as_deref(),
			Some(expected_blob.as_slice()),
			"(b) full node must receive ADDITIONAL_DATA over sync, accept it on its \
			 generic import path and store it in its DB"
		);
		println!("[b] full node sync + generic import + db round-trip: PASS");

		// (e, full node) the control block also synced with no additional data.
		let full_control_db = full_node
			.backend
			.blockchain()
			.block_additional_data(control_hash)
			.expect("db ok; qed");
		assert_eq!(
			full_control_db, None,
			"(e) full node must have no additional data for the control block"
		);
		println!("[e] control block synced by full node (no additional data): PASS");

		// ---------------------------------------------------------------------------
		// (c) Direct `validate_block` on a produced `ParachainBlockData::V3` candidate.
		// ---------------------------------------------------------------------------
		let vc_client = cumulus_test_client::TestClientBuilder::new()
			.enable_import_proof_recording()
			.build();
		let genesis_header = vc_client
			.header(vc_client.chain_info().genesis_hash)
			.ok()
			.flatten()
			.expect("Genesis header exists; qed");

		let (v3, v3_validation_data) =
			build_v3_with_runtime_pushed_additional_data(&vc_client, genesis_header.clone());
		let v3_header = v3.blocks()[0].header().clone();
		let validated_header = call_validate_block(
			genesis_header.clone(),
			v3,
			v3_validation_data.relay_parent_storage_root,
		)
		.expect("(c) V3 candidate with correct additional data must validate");
		assert_eq!(validated_header, v3_header, "(c) validate_block must return the same header");
		println!("[c] validate_block accepted the V3 candidate: PASS");

		// ---------------------------------------------------------------------------
		// (d) Corrupted additional data is rejected.
		//
		// Network-level corruption injection is impractical in-process, so a corrupted
		// blob is fed directly into `validate_block` (the explicit hash-vs-digest
		// assertion, the same check that would fire on the generic import path). The
		// header keeps the original digest; only the data blob is tampered.
		// ---------------------------------------------------------------------------
		let (mut tampered, tampered_validation_data) =
			build_v3_with_runtime_pushed_additional_data(&vc_client, genesis_header.clone());
		let corrupted = vec![0xABu8; 16];
		if let ParachainBlockData::V3 { ref mut additional_data, .. } = tampered {
			additional_data[0] = Some(corrupted.clone());
		}
		println!(
			"[d] CORRUPTED additional_data blob: replaced {:?} (hash {}) with {:?} (hash {}); \
			 header digest remains {}",
			expected_blob,
			hex(&hash_blob(&expected_blob)),
			corrupted,
			hex(&hash_blob(&corrupted)),
			hex(&expected_digest_hash),
		);
		let res = call_validate_block(
			genesis_header,
			tampered,
			tampered_validation_data.relay_parent_storage_root,
		);
		assert!(res.is_err(), "(d) corrupted additional data must be rejected");
		println!(
			"[d] rejection occurred in validate_block (explicit hash-vs-digest assertion): \
			 Err: {:?}",
			res.unwrap_err(),
		);
		println!("[d] corrupted additional data rejected: PASS");

		println!(
			"\nALL SUB-ASSERTIONS PASSED: (a) collator DB, (b) full-node sync+import+DB, \
			 (c) validate_block V3, (d) corruption rejected, (e) non-opted-in control unaffected"
		);
	});
}
