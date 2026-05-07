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

//! Validator-side sanity checks on a fetched `CollationFetchingResponse`.
//!
//! Each scenario advertises legitimately, lets the validator fetch, then delivers a
//! response that violates one of the response-side invariants. The validator must
//! reject and report the peer Malicious.
//!
//! * [`response_with_mismatched_candidate_hash_reports_malicious`] — receipt's hash differs
//!   from the advertised hash.
//! * [`response_with_wrong_parent_head_data_reports_malicious`] — `parent_head_data` in a
//!   `CollationWithParentHeadData` response hashes to something different from what was
//!   advertised.
//! * [`v2_descriptor_with_invalid_core_index_reports_malicious`] — descriptor's core_index
//!   is out of range for the validator's assignment.
//! * [`v3_candidate_via_v2_protocol_reports_malicious`] — receipt is a V3 descriptor
//!   delivered over a V2 protocol connection.
//!
//! All four KNOWN-FAIL on experimental: per
//! `project_collator_experimental_no_invalid_reputation_event.md`, experimental updates
//! its persistent reputation store directly rather than emitting a `Reputation::Malicious`
//! bus event. The validation does happen and the candidate is rejected — only the
//! observable Effect is missing.

use crate::{
	builders::{Candidate, ProtocolVersion::V2},
	contract::RepBucket,
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_node_primitives::{BlockData, PoV};
use polkadot_primitives::{
	CandidateHash, CandidateReceiptV2, CoreIndex, HeadData, Hash, Id as ParaId,
	MutateDescriptorV2, PersistedValidationData,
};
use polkadot_primitives_test_helpers::{
	dummy_committed_candidate_receipt_v2, dummy_committed_candidate_receipt_v3,
};

const PARA_A: ParaId = ParaId::new(2000);

/// Build a PVD whose `parent_head` is empty (the framework's default fixture). Most
/// sanity-check scenarios pin the candidate's `persisted_validation_data_hash` to this
/// shape; deviations that don't match get rejected by real backing's PVD lookup.
fn empty_parent_pvd(relay_parent_number: u32) -> PersistedValidationData {
	PersistedValidationData {
		parent_head: HeadData(Vec::new()),
		relay_parent_number,
		relay_parent_storage_root: Hash::zero(),
		max_pov_size: 5 * 1024 * 1024,
	}
}

#[crate::sim_test]
fn response_with_mismatched_candidate_hash_reports_malicious<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA_A)]);
	let pvd = empty_parent_pvd(w.leaf_number());
	let mut actual = Candidate::for_para_at(PARA_A, w.leaf());
	actual.receipt.descriptor.set_persisted_validation_data_hash(pvd.hash());

	let peer = w.declared_peer(PARA_A, V2);

	// Advertise with a hash that is NOT the actual fetched candidate's hash.
	let advertised_hash = CandidateHash(Hash::repeat_byte(0xFE));
	assert_ne!(advertised_hash, actual.hash(), "advertised hash must differ from actual");
	w.advertise_with_parent_head(&peer, w.leaf(), advertised_hash, HeadData(Vec::new()).hash());

	// Validator fires a fetch for the advertised hash. We don't have a Candidate that
	// hashes to `advertised_hash`, so we skip world.fetch_request() and call the lower
	// level matcher directly.
	let send_request = w.sim.expect(
		|e| matches!(
			e,
			crate::contract::Effect::SendRequest {
				kind: crate::contract::ReqKind::CollationFetchingV2,
				candidate_hash: Some(c),
				..
			} if c == &advertised_hash
		),
		std::time::Duration::from_millis(500),
		"Effect::SendRequest CollationFetchingV2 for the advertised hash",
	);
	let request_id = send_request.request_id().expect("SendRequest carries a RequestId");

	w.respond_fetch_v2(request_id, actual.receipt.clone(), PoV { block_data: BlockData(vec![1]) });

	w.expect_rep(&peer, RepBucket::Malicious);
}

#[crate::sim_test]
fn response_with_wrong_parent_head_data_reports_malicious<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA_A)]);
	let pvd = empty_parent_pvd(w.leaf_number());
	let mut candidate = Candidate::for_para_at(PARA_A, w.leaf());
	candidate.receipt.descriptor.set_persisted_validation_data_hash(pvd.hash());

	let peer = w.declared_peer(PARA_A, V2);

	let advertised_parent_head_hash = HeadData(Vec::new()).hash();
	w.advertise_with_parent_head(
		&peer,
		w.leaf(),
		candidate.hash(),
		advertised_parent_head_hash,
	);
	let request_id = w.fetch_request(&candidate);

	let wrong_parent_head = HeadData(vec![0xDE, 0xAD, 0xBE, 0xEF]);
	assert_ne!(wrong_parent_head.hash(), advertised_parent_head_hash);
	w.respond_fetch_v2_with_parent_head(
		request_id,
		candidate.receipt.clone(),
		PoV { block_data: BlockData(vec![1]) },
		wrong_parent_head,
	);

	w.expect_rep(&peer, RepBucket::Malicious);
}

#[crate::sim_test]
fn v2_descriptor_with_invalid_core_index_reports_malicious<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA_A)]);
	let pvd = empty_parent_pvd(w.leaf_number());

	// Build a V2 committed candidate with an out-of-range core_index. Para is assigned to
	// core 0; we set core_index = 10.
	let mut committed = dummy_committed_candidate_receipt_v2(w.leaf());
	committed.descriptor.set_para_id(PARA_A);
	committed.descriptor.set_persisted_validation_data_hash(pvd.hash());
	committed.descriptor.set_core_index(CoreIndex(10));
	committed.descriptor.set_session_index(0);
	let receipt: CandidateReceiptV2 = committed.to_plain();
	let candidate = Candidate::from_receipt(receipt.clone());

	let peer = w.declared_peer(PARA_A, V2);

	w.advertise_with_parent_head(&peer, w.leaf(), candidate.hash(), HeadData(Vec::new()).hash());
	let request_id = w.fetch_request(&candidate);
	w.respond_fetch_v2(request_id, receipt, PoV { block_data: BlockData(vec![1]) });

	w.expect_rep(&peer, RepBucket::Malicious);
}

#[crate::sim_test]
fn v3_candidate_via_v2_protocol_reports_malicious<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA_A)]);
	let pvd = empty_parent_pvd(w.leaf_number());

	// V3 descriptor (set_version(1)) on a V2 protocol peer.
	let mut committed = dummy_committed_candidate_receipt_v3(w.leaf(), w.leaf());
	committed.descriptor.set_para_id(PARA_A);
	committed.descriptor.set_persisted_validation_data_hash(pvd.hash());
	committed.descriptor.set_core_index(CoreIndex(0));
	committed.descriptor.set_session_index(0);
	let receipt: CandidateReceiptV2 = committed.to_plain();
	let candidate = Candidate::from_receipt(receipt.clone());

	let peer = w.declared_peer(PARA_A, V2);

	w.advertise_with_parent_head(&peer, w.leaf(), candidate.hash(), HeadData(Vec::new()).hash());
	let request_id = w.fetch_request(&candidate);
	w.respond_fetch_v2(request_id, receipt, PoV { block_data: BlockData(vec![1]) });

	w.expect_rep(&peer, RepBucket::Malicious);
}
