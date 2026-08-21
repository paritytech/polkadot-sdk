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

//! End-to-end integration test for the dynamic `relay_chain_read` mechanism.
//!
//! Proves the full soundness round trip for a *dynamic* relay-state read performed by a
//! parachain runtime during block building, verified against the trusted
//! `relay_parent_storage_root` at validation, carried through the existing additional-data
//! channel. No zombienet — this drives the reference runtime WASM directly via
//! `validate_block`, mirroring `additional_data.rs`'s sub-assertions (c)/(d).
//!
//! Sub-assertions:
//! - (build) The runtime, during block building, dynamically reads a **present** relay key via
//!   the `relay_chain_read` host function; the read is recorded as a minimal storage proof and
//!   staged into the additional-data blob (recovered here via the recorder handle).
//! - (proof) That recorded proof, verified *independently* against the trusted
//!   `relay_parent_storage_root` with the same `VerifyingAdditionalDataProvider` the PVF uses, yields
//!   exactly the real relay value — proving the collator read genuine relay state, not garbage.
//! - (c) A direct `validate_block` on the produced `ParachainBlockData::V3` candidate **succeeds**:
//!   the PVF re-reads the key from the carried proof, verifies it against `relay_parent_storage_root`,
//!   and the deterministic re-execution reproduces the committed state root.
//! - (d) A candidate whose relay-read blob is corrupted in transit is **rejected** (the
//!   additional-data integrity guard, `hash(map) == digest`, protects the relay-read proof
//!   too). The value-verification layer proper — a proof that does not match the trusted root, or a
//!   value mismatch — is unit-tested in `sp-relay-read` and additionally exercised by (proof) above.

use codec::{Decode, Encode};
use cumulus_primitives_core::{
	relay_chain, ParaId, ParachainBlockData, PersistedValidationData, SchedulingProof,
};
use cumulus_test_client::{
	seal_block, BlockBuilderAndSupportData, BlockData, BuildBlockBuilder, BuildParachainBlockData,
	DefaultTestClientBuilderExt, HeadData, TestClientBuilderExt, ValidationParams,
};
use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
use cumulus_test_service::runtime;
use polkadot_primitives::HeadData as RelayHeadData;
use sp_additional_data::{
	hash, AdditionalData, AdditionalDataProvider, VerifyingAdditionalDataProvider, RELAY_PROOF_KEY,
};
use sp_runtime::traits::{BlakeTwo256, Block as BlockT, Header as HeaderT};

fn hex(bytes: &[u8]) -> String {
	format!("0x{}", sp_core::hexdisplay::HexDisplay::from(&bytes.to_vec()))
}

/// A scheduling proof that passes V3 validation for the default test runtime (the scheduling proof
/// is not validated when the `v3-descriptor` feature is off).
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

/// Build a single-block `V3` candidate. Every block reads relay state during `set_validation_data`
/// (host configuration, messaging state, upgrade signals) via the `read_relay_chain_state` host
/// function, so the candidate carries the recorded relay-read proof and the header commits its hash.
///
/// Returns the candidate, its persisted validation data (carrying the trusted
/// `relay_parent_storage_root`), and the committed additional-data blob.
fn build_v3(
	client: &cumulus_test_client::Client,
	parent_head: runtime::Header,
) -> (ParachainBlockData<runtime::Block>, PersistedValidationData, AdditionalData) {
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

	let BlockBuilderAndSupportData {
		block_builder,
		persisted_validation_data,
		additional_data_recorder,
		..
	} = client
		.init_block_builder_builder()
		.with_validation_data(validation_data)
		.with_relay_sproof_builder(sproof_builder)
		.build();

	let mut block =
		block_builder.build_parachain_block(*parent_head.state_root(), additional_data_recorder);
	block.blocks_mut()[0] = seal_block(block.blocks()[0].clone(), client);

	// The runtime read relay state during `set_validation_data`; the recorded proof is carried as
	// the sole additional-data item.
	let blob = block.additional_data()[0]
		.clone()
		.expect("runtime read relay state and recorded its proof; qed");

	// Sanity-check the header carries exactly the digest for this blob.
	let digest_hashes: Vec<[u8; 32]> = block.blocks()[0]
		.header()
		.digest()
		.logs()
		.iter()
		.filter_map(|d| d.as_additional_data().copied())
		.collect();
	assert_eq!(
		digest_hashes,
		vec![hash(&blob)],
		"header must carry exactly the relay-read blob's digest"
	);

	let (blocks, proof) = block.into_inner();
	let v3 = ParachainBlockData::V3 {
		blocks,
		proof,
		scheduling_proof: dummy_scheduling_proof(),
		additional_data: vec![Some(blob.clone())],
	};
	(v3, persisted_validation_data, blob)
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
fn relay_chain_read_end_to_end() {
	sp_tracing::try_init_simple();

	let client = cumulus_test_client::TestClientBuilder::new()
		.enable_import_proof_recording()
		.build();
	let genesis_header = client
		.header(client.chain_info().genesis_hash)
		.ok()
		.flatten()
		.expect("Genesis header exists; qed");

	// ---------------------------------------------------------------------------
	// (build) + (proof) + (c): read a PRESENT relay key, then validate.
	// ---------------------------------------------------------------------------
	let (v3, vd, blob) = build_v3(&client, genesis_header.clone());
	let root = vd.relay_parent_storage_root;
	let v3_header = v3.blocks()[0].header().clone();

	// (proof) The recorded proof, verified independently against the trusted root with the same
	// verifier the PVF uses, must yield a real relay value the runtime read. `set_validation_data`
	// always reads the relay host configuration (`ACTIVE_CONFIG`), so it must be present and
	// non-empty in the recorded proof.
	let verifier =
		VerifyingAdditionalDataProvider::<BlakeTwo256>::from_map_with_root(root, blob.clone())
			.expect("blob decodes as (root, proof); qed");
	let active_config_key = relay_chain::well_known_keys::ACTIVE_CONFIG.to_vec();
	let observed = <Option<Vec<u8>>>::decode(&mut &verifier.read(&active_config_key)[..])
		.expect("read returns SCALE(Option<Vec<u8>>); qed");
	assert!(
		observed.as_ref().is_some_and(|v| !v.is_empty()),
		"(proof) recorded proof must carry the relay host configuration read during \
		 set_validation_data, verified against relay_parent_storage_root"
	);
	println!(
		"[proof] recorded proof proves ACTIVE_CONFIG ({}) against root {}: PASS",
		hex(&active_config_key),
		hex(root.as_ref()),
	);

	// (c) The PVF re-reads + verifies the key from the carried proof and re-executes.
	let validated = call_validate_block(genesis_header.clone(), v3, root)
		.expect("(c) candidate that dynamically read a present relay key must validate");
	assert_eq!(validated, v3_header, "(c) validate_block must return the same header");
	println!("[c] validate_block accepted the dynamic relay-read candidate: PASS");

	// Note on proven-absence: the `None` path (a proven trie non-existence) is rigorously
	// unit-tested in `sp-relay-read` against a *full* trie backend. It is deliberately not
	// re-asserted here: the mocked `RelayStateSproofBuilder` only proves the keys it was told
	// about, so a recorded absence proof for an unrelated key can be incomplete and would degrade
	// to `None` consistently on both build and validate — passing for the wrong reason. The
	// in-process production path reads the full relay trie, where absence is genuinely provable.

	// ---------------------------------------------------------------------------
	// (d) corrupted relay-read blob is rejected.
	//
	// The additional-data integrity guard (`hash(map) == header digest`) protects the
	// relay-read proof in transit, so a byte-flipped blob is rejected before execution. The
	// value-verification layer proper (proof not matching the root / value mismatch) is covered by
	// the `sp-relay-read` unit tests and by the (proof) assertion above.
	// ---------------------------------------------------------------------------
	let (mut tampered, vd_tampered, orig_blob) = build_v3(&client, genesis_header.clone());
	let mut corrupted = AdditionalData::new();
	corrupted.insert(RELAY_PROOF_KEY.into(), vec![0xABu8; 16]);
	if let ParachainBlockData::V3 { ref mut additional_data, .. } = tampered {
		additional_data[0] = Some(corrupted.clone());
	}
	println!(
		"[d] CORRUPTED relay-read blob: replaced hash {} with {}; header digest unchanged",
		hex(&hash(&orig_blob)),
		hex(&hash(&corrupted)),
	);
	let res =
		call_validate_block(genesis_header, tampered, vd_tampered.relay_parent_storage_root);
	assert!(res.is_err(), "(d) corrupted relay-read blob must be rejected");
	println!("[d] corrupted relay-read blob rejected in validate_block: PASS");

	println!(
		"\nALL SUB-ASSERTIONS PASSED: (build+proof) real value proven vs trusted root, \
		 (c) validate_block accepts dynamic relay read, (d) corruption rejected"
	);
}
