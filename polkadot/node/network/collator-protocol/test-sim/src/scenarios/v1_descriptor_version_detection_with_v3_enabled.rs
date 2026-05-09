// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Mirrors `validator_side/tests/prospective_parachains.rs::v1_descriptor_version_detection_with_v3_enabled`.
//!
//! V3 node feature enabled but a peer connects via V1 protocol and delivers a V1-shape
//! candidate descriptor. Validator must detect V1 (non-zero reserved bytes) and second
//! the candidate via the legacy PVD path → emits `Effect::SecondCandidate`.

use crate::scenarios::shared::WorldExt as _;
use crate::{
	builders::{Candidate, ProtocolVersion::V1},
	chain::CoreSchedule,
	harness::CollatorSut,
	scenarios::shared::{build_with_ancestors_world_with_config, ChainConfig},
};
use polkadot_node_primitives::{BlockData, PoV};
use polkadot_primitives::{
	CandidateCommitments, CollatorId, CollatorSignature, CoreIndex, HeadData, Id as ParaId,
};
use polkadot_primitives_test_helpers::CandidateDescriptor;

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn v1_shape_descriptor_via_v1_protocol_under_v3_node_feature<S: CollatorSut>() {
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA))
		.with_v3_descriptors_enabled();
	let mut w = build_with_ancestors_world_with_config::<S>(0, config);
	let leaf = w.leaf();

	// Build a V1-shape descriptor with non-zero reserved bytes (so V1 detection hits even
	// under a V3-enabled validator).
	let mut collator_bytes = [0u8; 32];
	collator_bytes.iter_mut().enumerate().for_each(|(i, b)| *b = i as u8);
	let mut signature_bytes = [0u8; 64];
	signature_bytes.iter_mut().enumerate().for_each(|(i, b)| *b = i as u8);

	let leaf_n = w.leaf_number();
	let pvd = polkadot_primitives::PersistedValidationData {
		parent_head: HeadData(Vec::new()),
		relay_parent_number: leaf_n,
		relay_parent_storage_root: polkadot_primitives::Hash::zero(),
		max_pov_size: 5 * 1024 * 1024,
	};

	let commitments = CandidateCommitments {
		head_data: HeadData(vec![1]),
		horizontal_messages: Default::default(),
		upward_messages: Default::default(),
		new_validation_code: None,
		processed_downward_messages: 0,
		hrmp_watermark: 0,
	};

	let descriptor: CandidateDescriptor = CandidateDescriptor {
		para_id: PARA,
		relay_parent: leaf,
		collator: CollatorId::from(sp_core::sr25519::Public::from_raw(collator_bytes)),
		persisted_validation_data_hash: pvd.hash(),
		pov_hash: polkadot_primitives::Hash::zero(),
		erasure_root: polkadot_primitives::Hash::zero(),
		signature: CollatorSignature::from(sp_core::sr25519::Signature::from_raw(signature_bytes)),
		para_head: commitments.head_data.hash(),
		validation_code_hash: polkadot_primitives_test_helpers::dummy_validation_code().hash(),
	};
	let receipt_v2: polkadot_primitives::CandidateReceiptV2 = polkadot_primitives::CandidateReceiptV2 {
		descriptor: descriptor.into(),
		commitments_hash: commitments.hash(),
	};
	let candidate = Candidate::from_receipt(receipt_v2.clone());

	let peer = w.declared_peer(PARA, V1);
	w.base.sim.send(peer.advertise(leaf, None, None));
	let request_id = w.fetch_request(&candidate);
	w.respond_fetch_v1(request_id, receipt_v2, PoV { block_data: BlockData(vec![1]) });
	w.expect_second(&candidate);
}
