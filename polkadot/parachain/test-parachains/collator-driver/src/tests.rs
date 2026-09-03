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

use super::*;
use codec::Encode;
use polkadot_primitives::{ClaimQueueOffset, CoreSelector, UMPSignal, UMP_SEPARATOR};
use rstest::rstest;
use std::collections::{BTreeMap, VecDeque};

const PARA_ID: ParaId = ParaId::new(5);

/// The `SelectCore` signal a parachain commits to, if any.
struct CoreSelectorData {
	/// The core selector index of the first collation of a leaf.
	index: u8,
	/// How much the index grows between chained collations. `0` or a value larger than one
	/// models a parachain that reuses core indexes.
	increment_index_by: u8,
	/// The committed claim queue offset.
	cq_offset: u8,
}

fn upward_messages(selector: &Option<CoreSelectorData>) -> UpwardMessages {
	let mut messages = UpwardMessages::default();
	if let Some(selector) = selector {
		messages.force_push(UMP_SEPARATOR);
		messages.force_push(
			UMPSignal::SelectCore(
				CoreSelector(selector.index),
				ClaimQueueOffset(selector.cq_offset),
			)
			.encode(),
		);
	}
	messages
}

/// Replay the core selection of one leaf, returning the cores that would be collated on, in
/// order. Mirrors the loop in `collate_on_assigned_cores`.
fn selected_cores(
	claim_queue: &ClaimQueueSnapshot,
	mut selector: Option<CoreSelectorData>,
) -> Vec<u32> {
	let n_assigned_cores = claim_queue
		.iter_all_claims()
		.filter(|(_, paras)| paras.contains(&PARA_ID))
		.count();

	let mut used_cores = HashSet::new();
	let mut cores = Vec::new();

	for index in 0..n_assigned_cores {
		let Ok(core_index) = select_core_index(
			claim_queue,
			PARA_ID,
			&upward_messages(&selector),
			index,
			&used_cores,
		) else {
			break;
		};

		used_cores.insert(core_index);
		cores.push(core_index.0);

		if let Some(selector) = selector.as_mut() {
			selector.index += selector.increment_index_by;
		}
	}

	cores
}

fn claim_queue(claims: impl IntoIterator<Item = (u32, Vec<ParaId>)>) -> ClaimQueueSnapshot {
	ClaimQueueSnapshot(
		claims
			.into_iter()
			.map(|(core, paras)| (CoreIndex(core), VecDeque::from(paras)))
			.collect::<BTreeMap<_, _>>(),
	)
}

#[test]
// Cores assigned to the para only at a deeper claim queue position are not collated on.
fn selects_only_cores_assigned_at_offset_0() {
	// Every core is assigned to some other para at depth 0 except core 5, and to our para at
	// depths 1 and 2. That shouldn't matter.
	let claim_queue =
		claim_queue((0..=5).map(|core| (core, vec![ParaId::from(core), PARA_ID, PARA_ID])));

	assert_eq!(selected_cores(&claim_queue, None), vec![5]);
}

#[rstest]
#[case(0)]
#[case(1)]
#[case(2)]
#[case(3)]
// A collation is built for every core assigned to the para at depth 0.
fn collates_on_every_assigned_core(#[case] total_cores: u32) {
	let claim_queue = claim_queue((0..total_cores).map(|core| (core, vec![PARA_ID])));

	assert_eq!(selected_cores(&claim_queue, None), (0..total_cores).collect::<Vec<_>>());
}

#[rstest]
#[case(1, 0, 0)]
#[case(2, 0, 1)]
// The committed core selector index may start above the number of cores — the remainder selects
// the core. The committed claim queue offset decides at which depth the assigned cores are
// looked up.
fn honours_committed_core_selector_and_claim_queue_offset(
	#[case] total_cores: u32,
	#[case] init_cs_index: u8,
	#[case] cq_offset: u8,
) {
	let other_para_id = ParaId::from(10);
	let claim_queue = claim_queue((0..total_cores).map(|core| {
		let mut paras = vec![other_para_id; cq_offset as usize];
		paras.push(PARA_ID);
		(core, paras)
	}));

	let mut expected = (0..total_cores).collect::<Vec<_>>();
	if total_cores > 1 && init_cs_index > 0 {
		// A non-zero first core selector index changes the order of submissions, but collations
		// are still submitted on all cores.
		expected.rotate_left((init_cs_index as u32 % total_cores) as usize);
	}

	assert_eq!(
		selected_cores(
			&claim_queue,
			Some(CoreSelectorData { index: init_cs_index, increment_index_by: 1, cq_offset }),
		),
		expected,
	);
}

#[rstest]
#[case(3, 0, vec![0])]
#[case(3, 1, vec![0, 1, 2])]
#[case(3, 2, vec![0, 2, 1])]
#[case(3, 3, vec![0])]
#[case(3, 4, vec![0, 1, 2])]
// A parachain that selects a core index it already used stops the chain for this leaf.
fn stops_when_a_core_is_selected_twice(
	#[case] total_cores: u32,
	#[case] increment_cs_index_by: u8,
	#[case] expected_cores: Vec<u32>,
) {
	let claim_queue = claim_queue((0..total_cores).map(|core| (core, vec![PARA_ID])));

	assert_eq!(
		selected_cores(
			&claim_queue,
			Some(CoreSelectorData {
				index: 0,
				increment_index_by: increment_cs_index_by,
				cq_offset: 0
			}),
		),
		expected_cores,
	);
}

#[test]
// Nothing is collated when the para has no assignment at the committed claim queue offset.
fn no_core_at_committed_offset() {
	let claim_queue = claim_queue([(0, vec![PARA_ID])]);

	assert_matches::assert_matches!(
		select_core_index(
			&claim_queue,
			PARA_ID,
			&upward_messages(&Some(CoreSelectorData {
				index: 0,
				increment_index_by: 1,
				cq_offset: 1,
			})),
			0,
			&HashSet::new(),
		),
		Err(CoreSelectionError::NoAssignment(1))
	);
}
