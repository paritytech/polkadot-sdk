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
use polkadot_primitives::{
	CandidateHash, CandidateReceiptV2, CoreIndex, HeadData, Hash, Id as ParaId,
	MutateDescriptorV2, PersistedValidationData,
};
use polkadot_primitives_test_helpers::{
	dummy_committed_candidate_receipt_v2, dummy_committed_candidate_receipt_v3,
};

const PARA_A: ParaId = ParaId::new(2000);

/// PVD whose `parent_head` is empty (the framework's default fixture). All four sanity
/// scenarios pin `persisted_validation_data_hash` to this shape so real backing's PVD
/// lookup proceeds; the rejection happens later, in the response-side check.
fn empty_parent_pvd(relay_parent_number: u32) -> PersistedValidationData {
	PersistedValidationData {
		parent_head: HeadData(Vec::new()),
		relay_parent_number,
		relay_parent_storage_root: Hash::zero(),
		max_pov_size: 5 * 1024 * 1024,
	}
}

/// Build a `CandidateReceiptV2` (with the supplied closure) wrapped in a [`Candidate`]
/// for advertise/fetch convenience. Used by the V2/V3 invalid-descriptor scenarios.
fn build_descriptor_with<F>(
	w: &crate::scenarios::shared::World<impl CollatorSut>,
	mut f: F,
) -> (CandidateReceiptV2, Candidate)
where
	F: FnMut(&mut polkadot_primitives::CommittedCandidateReceiptV2),
{
	let pvd = empty_parent_pvd(w.leaf_number());
	let mut committed = dummy_committed_candidate_receipt_v2(w.leaf());
	committed.descriptor.set_para_id(PARA_A);
	committed.descriptor.set_persisted_validation_data_hash(pvd.hash());
	committed.descriptor.set_core_index(CoreIndex(0));
	committed.descriptor.set_session_index(0);
	f(&mut committed);
	let receipt: CandidateReceiptV2 = committed.to_plain();
	let candidate = Candidate::from_receipt(receipt.clone());
	(receipt, candidate)
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

	let request_id = w.expect_fetch_for_hash(advertised_hash);
	w.respond_fetch_v2(request_id, actual.receipt.clone(), Candidate::empty_pov());
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
	w.advertise_with_parent_head(&peer, w.leaf(), candidate.hash(), advertised_parent_head_hash);
	let request_id = w.fetch_request(&candidate);

	let wrong_parent_head = HeadData(vec![0xDE, 0xAD, 0xBE, 0xEF]);
	assert_ne!(wrong_parent_head.hash(), advertised_parent_head_hash);
	w.respond_fetch_v2_with_parent_head(
		request_id,
		candidate.receipt.clone(),
		Candidate::empty_pov(),
		wrong_parent_head,
	);

	w.expect_rep(&peer, RepBucket::Malicious);
}

#[crate::sim_test]
fn v2_descriptor_with_invalid_core_index_reports_malicious<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA_A)]);
	// Para is assigned to core 0; out-of-range core 10 → rejected as malicious.
	let (receipt, candidate) = build_descriptor_with(&w, |c| {
		c.descriptor.set_core_index(CoreIndex(10));
	});
	let peer = w.declared_peer(PARA_A, V2);
	w.advertise_with_parent_head(&peer, w.leaf(), candidate.hash(), HeadData(Vec::new()).hash());
	let request_id = w.fetch_request(&candidate);
	w.respond_fetch_v2(request_id, receipt, Candidate::empty_pov());
	w.expect_rep(&peer, RepBucket::Malicious);
}

/// Mirrors the second arm of upstream `invalid_v2_descriptor`: core_index=0 is fine but
/// the descriptor's session_index is wrong → rejected as malicious. (Distinct from
/// `v3_session_index_checks::v2_descriptor_with_wrong_session_index_reports_malicious`
/// only by which leg of the upstream rstest it tracks; both probe the same gate.)
#[crate::sim_test]
fn v2_descriptor_with_invalid_session_index_reports_malicious<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA_A)]);
	let (receipt, candidate) = build_descriptor_with(&w, |c| {
		c.descriptor.set_session_index(10); // chain has session 0
	});
	let peer = w.declared_peer(PARA_A, V2);
	w.advertise_with_parent_head(&peer, w.leaf(), candidate.hash(), HeadData(Vec::new()).hash());
	let request_id = w.fetch_request(&candidate);
	w.respond_fetch_v2(request_id, receipt, Candidate::empty_pov());
	w.expect_rep(&peer, RepBucket::Malicious);
}

#[crate::sim_test]
fn v3_candidate_via_v2_protocol_reports_malicious<S: CollatorSut>() {
	v3_descriptor_rejected_on_wrong_protocol_helper::<S>(
		ProtocolKind::V2,
		/* crafted_unknown */ false,
	);
}

#[crate::sim_test]
fn v3_candidate_via_v1_protocol_reports_malicious<S: CollatorSut>() {
	v3_descriptor_rejected_on_wrong_protocol_helper::<S>(
		ProtocolKind::V1,
		/* crafted_unknown */ false,
	);
}

#[crate::sim_test]
fn crafted_unknown_descriptor_via_v2_protocol_reports_malicious<S: CollatorSut>() {
	v3_descriptor_rejected_on_wrong_protocol_helper::<S>(
		ProtocolKind::V2,
		/* crafted_unknown */ true,
	);
}

#[crate::sim_test]
fn crafted_unknown_descriptor_via_v1_protocol_reports_malicious<S: CollatorSut>() {
	v3_descriptor_rejected_on_wrong_protocol_helper::<S>(
		ProtocolKind::V1,
		/* crafted_unknown */ true,
	);
}

#[derive(Clone, Copy)]
enum ProtocolKind {
	V1,
	V2,
}

/// Helper for the 4-case rstest above. Builds a V3 (or crafted-unknown via
/// `set_version(2)`) candidate, advertises over V1 or V2, responds with the matching
/// fetch flavour. Validator must report Malicious in all cases.
fn v3_descriptor_rejected_on_wrong_protocol_helper<S: CollatorSut>(
	wire: ProtocolKind,
	crafted_unknown: bool,
) {
	use crate::builders::ProtocolVersion;
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA_A)]);
	let pvd = empty_parent_pvd(w.leaf_number());

	let mut committed = dummy_committed_candidate_receipt_v3(w.leaf(), w.leaf());
	committed.descriptor.set_para_id(PARA_A);
	committed.descriptor.set_persisted_validation_data_hash(pvd.hash());
	committed.descriptor.set_core_index(CoreIndex(0));
	committed.descriptor.set_session_index(0);
	if crafted_unknown {
		// version=0 → V2, version=1 → V3, anything else → Unknown.
		committed.descriptor.set_version(2);
	}
	let receipt: CandidateReceiptV2 = committed.to_plain();
	let candidate = Candidate::from_receipt(receipt.clone());

	let proto = match wire {
		ProtocolKind::V1 => ProtocolVersion::V1,
		ProtocolKind::V2 => ProtocolVersion::V2,
	};
	let peer = w.declared_peer(PARA_A, proto);
	let leaf = w.leaf();
	match wire {
		ProtocolKind::V1 => {
			// V1 advertisement carries no candidate_hash on the wire.
			w.sim.send(peer.advertise(leaf, None, None));
			let (_, request_id, _) = w.expect_any_fetch();
			w.respond_fetch_v1(request_id, receipt, Candidate::empty_pov());
		},
		ProtocolKind::V2 => {
			w.advertise_with_parent_head(&peer, leaf, candidate.hash(), HeadData(Vec::new()).hash());
			let request_id = w.fetch_request(&candidate);
			w.respond_fetch_v2(request_id, receipt, Candidate::empty_pov());
		},
	}
	w.expect_rep(&peer, RepBucket::Malicious);
}
