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

//! Mirrors `v2_sanity_check_session_index_unchanged` and
//! `v3_sanity_check_uses_scheduling_session_not_relay_parent_session`.
//!
//! These upstream tests directly exercise `descriptor_version_sanity_check_with_params`,
//! a private function. The contract-level shape is the validator's outbound effect when
//! a candidate with mismatched session_index is delivered. We exercise that by sending a
//! V2 candidate with `session_index != session at relay_parent` → validator rejects.
//!
//! Both impls reject the candidate. The rep *signal* diverges (legacy emits Malicious;
//! experimental silent) — see [`crate::scenarios::divergent::reputation_emission`]. Here
//! we assert only the shared invariant: no `SecondCandidate`.

use crate::scenarios::shared::WorldExt as _;
use crate::{
	builders::{Candidate, ProtocolVersion::V2},
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_node_primitives::{BlockData, PoV};
use polkadot_primitives::{
	CandidateReceiptV2, CoreIndex, HeadData, Id as ParaId, MutateDescriptorV2,
	PersistedValidationData,
};
use polkadot_primitives_test_helpers::dummy_committed_candidate_receipt_v2;
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

/// V2 candidate with a session_index that doesn't match the relay parent's session is
/// rejected. (Our chain has session 0; we set descriptor.session_index=999.)
#[crate::sim_test]
fn v2_descriptor_with_wrong_session_index_rejects<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);

	let pvd = PersistedValidationData {
		parent_head: HeadData(Vec::new()),
		relay_parent_number: w.leaf_number(),
		relay_parent_storage_root: polkadot_primitives::Hash::zero(),
		max_pov_size: 5 * 1024 * 1024,
	};
	let mut committed = dummy_committed_candidate_receipt_v2(w.leaf());
	committed.descriptor.set_para_id(PARA);
	committed.descriptor.set_persisted_validation_data_hash(pvd.hash());
	committed.descriptor.set_core_index(CoreIndex(0));
	committed.descriptor.set_session_index(999); // wrong session
	let receipt: CandidateReceiptV2 = committed.to_plain();
	let candidate = Candidate::from_receipt(receipt.clone());

	let peer = w.declared_peer(PARA, V2);
	w.advertise_with_parent_head(&peer, w.leaf(), candidate.hash(), HeadData(Vec::new()).hash());
	let request_id = w.fetch_request(&candidate);
	w.respond_fetch_v2(request_id, receipt, PoV { block_data: BlockData(vec![1]) });
	w.expect_no_second(&candidate, Duration::from_millis(500));
}
