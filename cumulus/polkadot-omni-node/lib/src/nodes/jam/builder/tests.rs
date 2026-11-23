// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::nodes::jam::{
	builder::{
		parent::{choose_parent, select_parent, Descendant, ParentSource, STALL_REROOT_SLOTS},
		state::{head_move, BuilderState, HeadMove, Reroot},
		turn::{
			anchor_slot_claimed, next_parent_source, package_fits_turn, time_until_next_para_slot,
			tip_is_fresh, turn_package_budget, MAX_TIP_LAG_SLOTS,
		},
	},
	chain::tests::{chain, TestHeader},
};
use cumulus_jam_aura_primitives::timestamp_for_anchor;
use sp_consensus_aura::{Slot, SlotDuration};
use sp_core::H256;
use sp_timestamp::Timestamp;
use std::{collections::VecDeque, time::Duration};

/// The descendants of the accumulated head, as
/// [`local_descendants`](super::parent::local_descendants) would hand them over: breadth-first, so
/// shallower blocks come first and siblings keep the order they were seen.
fn descendants(blocks: &[(&TestHeader, usize)]) -> Vec<Descendant<TestHeader>> {
	blocks
		.iter()
		.map(|(header, depth)| Descendant { header: (*header).clone(), depth: *depth })
		.collect()
}

fn ours(headers: &[&TestHeader]) -> VecDeque<H256> {
	headers.iter().map(|header| header.hash()).collect()
}

/// A sibling of `header`: same parent and number, different state root — the shape two
/// collators authoring in the same slot produce.
fn sibling(header: &TestHeader) -> TestHeader {
	let mut sibling = header.clone();
	sibling.state_root = H256::repeat_byte(0xaa);
	sibling
}

/// Pipelining in one line: the collator extends the deepest block it holds rather than
/// re-rooting on the accumulated head every slot.
#[test]
fn the_deepest_local_descendant_is_the_parent() {
	let chain = chain(3);
	let candidates = descendants(&[(&chain[0], 1), (&chain[1], 2), (&chain[2], 3)]);

	let parent = select_parent(&candidates, &VecDeque::new()).expect("the tree is not empty");

	assert_eq!(parent.header.hash(), chain[2].hash());
	assert_eq!(parent.depth, 3);
}

/// Nothing below the accumulated head means the next block is a direct child of it, which is
/// the root case and the state a collator restarts into.
#[test]
fn an_empty_local_tree_selects_no_parent() {
	assert!(select_parent::<TestHeader>(&[], &VecDeque::new()).is_none());
}

/// Two collators authored children of the same block. Ours is the one whose package this
/// collator already has in flight, so extending it keeps consecutive ticks on one line
/// instead of hopping between siblings every slot.
#[test]
fn our_own_block_wins_a_tie_at_the_same_depth() {
	let chain = chain(2);
	let theirs = sibling(&chain[1]);

	let theirs_first = descendants(&[(&chain[0], 1), (&theirs, 2), (&chain[1], 2)]);
	let parent = select_parent(&theirs_first, &ours(&[&chain[1]])).expect("not empty");

	assert_eq!(parent.header.hash(), chain[1].hash());
}

/// A deeper block of somebody else's still beats a shallower one of ours: depth is the first
/// rule, and preferring our own shallower block would throw away a slot of pipelining.
#[test]
fn depth_beats_ownership() {
	let chain = chain(3);
	let candidates = descendants(&[(&chain[0], 1), (&chain[1], 2), (&chain[2], 3)]);

	let parent = select_parent(&candidates, &ours(&[&chain[0]])).expect("not empty");

	assert_eq!(parent.header.hash(), chain[2].hash());
}

/// With neither block ours, the tie is broken by arrival: the first one the walk saw. The
/// database enumerates a block's children in import order, so this keeps a collator on the
/// same branch across ticks instead of flipping between two strangers' siblings.
#[test]
fn a_tie_between_two_foreign_blocks_goes_to_the_first_seen() {
	let chain = chain(2);
	let theirs = sibling(&chain[1]);
	let ours_none = VecDeque::new();

	let first = descendants(&[(&chain[1], 1), (&theirs, 1)]);
	let reversed = descendants(&[(&theirs, 1), (&chain[1], 1)]);

	assert_eq!(select_parent(&first, &ours_none).unwrap().header.hash(), chain[1].hash());
	assert_eq!(select_parent(&reversed, &ours_none).unwrap().header.hash(), theirs.hash());
}

/// A fresh builder, as `run_builder_task` starts one.
fn builder_state() -> BuilderState<TestHeader> {
	BuilderState {
		last_claimed_anchor: None,
		included: None,
		own_recent: VecDeque::new(),
		head_advanced_at: None,
		rerooted: None,
		last_pool_scan: None,
	}
}

/// The stall boundary decides how long a lost work package holds the parachain up, so it has
/// to be exact: one slot short of the bound the collator keeps waiting, at the bound it acts.
#[test]
fn the_stall_bound_is_where_re_rooting_starts() {
	let chain = chain(1);
	let above_the_head = descendants(&[(&chain[0], 1)]);
	let ours = VecDeque::new();

	assert_eq!(
		choose_parent(&above_the_head, &ours, None, STALL_REROOT_SLOTS - 1).1,
		ParentSource::Deepest,
		"one slot short of the bound the branch is still given the benefit of the doubt",
	);
	assert_eq!(
		choose_parent(&above_the_head, &ours, None, STALL_REROOT_SLOTS).1,
		ParentSource::Reroot,
	);
}

/// Re-rooting authors on the stuck head itself — a sibling of the block whose package is
/// lost — which is the only thing that can move the head on.
#[test]
fn re_rooting_authors_on_the_stuck_head_itself() {
	let chain = chain(3);
	let branch = descendants(&[(&chain[0], 1), (&chain[1], 2), (&chain[2], 3)]);

	let mine = ours(&[&chain[0]]);

	let (parent, source) = choose_parent(&branch, &mine, None, STALL_REROOT_SLOTS);

	assert_eq!(source, ParentSource::Reroot);
	assert!(parent.is_none(), "no descendant at all: the parent is the accumulated head");
}

/// With nothing above the head there is no branch to abandon — the next block is already its
/// child — so a stall must not turn into a decision at all, however long it has lasted.
#[test]
fn a_head_with_nothing_above_it_is_never_re_rooted() {
	let (parent, source) =
		choose_parent::<TestHeader>(&[], &VecDeque::new(), None, STALL_REROOT_SLOTS * 10);

	assert_eq!(source, ParentSource::AccumulatedHead);
	assert!(parent.is_none());
}

/// Having re-rooted, the collator has to *extend* the branch it started, not start another
/// one every slot: the abandoned branch is still the deeper of the two, so the plain
/// deepest-block rule would author a fresh sibling of the stuck block every single slot and
/// never get anywhere.
#[test]
fn a_re_rooted_branch_is_extended_rather_than_started_again() {
	let chain = chain(3);
	let abandoned = descendants(&[(&chain[0], 1), (&chain[1], 2), (&chain[2], 3)]);
	let sibling = sibling(&chain[0]);
	let committed = Descendant { header: sibling.clone(), depth: 1 };

	let (parent, source) = choose_parent(&abandoned, &VecDeque::new(), Some(&committed), 0);

	assert_eq!(source, ParentSource::Rerooted);
	assert_eq!(parent.map(|parent| parent.header.hash()), Some(sibling.hash()));
}

/// ...but a branch that goes nowhere either must not be clung to forever: once the stall
/// clock — restarted when the re-root was authored — runs out again, the new branch is
/// abandoned in its turn.
#[test]
fn a_re_rooted_branch_that_also_stalls_is_abandoned_in_turn() {
	let chain = chain(2);
	let above_the_head = descendants(&[(&chain[0], 1), (&chain[1], 2)]);
	let committed = Descendant { header: sibling(&chain[0]), depth: 1 };

	let (parent, source) =
		choose_parent(&above_the_head, &VecDeque::new(), Some(&committed), STALL_REROOT_SLOTS);

	assert_eq!(source, ParentSource::Reroot);
	assert!(parent.is_none());
}

/// The stall clock counts para slots, not ticks, so a skipped tick cannot buy the stuck
/// branch extra time — and a head that moved has to reset it, or a parachain that is running
/// perfectly well would re-root every `STALL_REROOT_SLOTS` slots.
#[test]
fn the_stall_clock_counts_slots_and_a_moving_head_resets_it() {
	let mut state = builder_state();

	assert_eq!(state.note_head(HeadMove::Unchanged, Slot::from(100)), 0, "the first tick");
	assert_eq!(state.note_head(HeadMove::Unchanged, Slot::from(101)), 1);
	// Slots 102..104 are skipped ticks: the clock keeps counting them all the same.
	assert_eq!(state.note_head(HeadMove::Unchanged, Slot::from(105)), 5);

	assert_eq!(
		state.note_head(HeadMove::Ours, Slot::from(106)),
		0,
		"the tick that sees the head move reports no stall: the branch above the new head is \
		 fresh, and re-rooting it would abandon a perfectly good block the moment the \
		 parachain recovered",
	);
	assert_eq!(state.note_head(HeadMove::Unchanged, Slot::from(107)), 1, "counted from 106");
}

/// A head that moved also releases the branch commitment: whatever the re-root was for is
/// over, and the next tick is free to pick the deepest block again.
#[test]
fn a_moving_head_releases_the_re_rooted_branch() {
	let mut state = builder_state();
	state.note_head(HeadMove::Unchanged, Slot::from(100));
	state.note_authored(ParentSource::Reroot, H256::repeat_byte(1), Slot::from(108));

	assert_eq!(
		state.rerooted,
		Some(Reroot { tip: H256::repeat_byte(1), depth: 1 }),
		"authoring the re-root starts the branch",
	);
	assert_eq!(
		state.note_head(HeadMove::Unchanged, Slot::from(109)),
		1,
		"and restarts the clock, so the new branch gets a full window of its own",
	);

	state.note_head(HeadMove::Foreign, Slot::from(110));

	assert_eq!(state.rerooted, None);
}

/// Each further block on the re-rooted branch extends it, and its depth has to keep counting
/// from the stuck head — that depth is what the capacity tripwire and the runtime's own gate
/// are told about.
#[test]
fn blocks_on_the_re_rooted_branch_deepen_it() {
	let mut state = builder_state();
	state.note_authored(ParentSource::Reroot, H256::repeat_byte(1), Slot::from(100));
	state.note_authored(ParentSource::Rerooted, H256::repeat_byte(2), Slot::from(101));

	assert_eq!(state.rerooted, Some(Reroot { tip: H256::repeat_byte(2), depth: 2 }));
}

/// The head standing still is the common tick and must stay quiet in the log; the two ways it
/// moves are worth a line each, and only the log distinguishes them.
#[test]
fn a_head_that_moved_says_whether_it_was_ours() {
	let chain = chain(2);
	let mine = ours(&[&chain[1]]);

	let (a, b) = (chain[0].hash(), chain[1].hash());
	assert_eq!(head_move(Some(&a), Some(&a), &mine), HeadMove::Unchanged);
	assert_eq!(head_move(Some(&a), Some(&b), &mine), HeadMove::Ours);
	assert_eq!(head_move(Some(&a), Some(&sibling(&chain[1]).hash()), &mine), HeadMove::Foreign);
	assert_eq!(
		head_move(Some(&a), None, &mine),
		HeadMove::Foreign,
		"a head that vanished is nothing we authored — it cannot happen, and if it did the \
		 log is where it should show up",
	);
}

/// The para's very first block accumulating: there was no head before, and it is ours.
#[test]
fn the_first_head_of_all_is_a_move() {
	let chain = chain(1);
	assert_eq!(head_move(None, Some(&chain[0].hash()), &ours(&[&chain[0]])), HeadMove::Ours);
	assert_eq!(head_move::<H256>(None, None, &VecDeque::new()), HeadMove::Unchanged);
}

/// The remembered blocks are a tie-breaker, not a ledger: once the accumulated head moves past
/// a block, that block can never be a parent again and is forgotten.
#[test]
fn blocks_below_the_accumulated_head_are_forgotten() {
	let chain = chain(4);
	let mut state = builder_state();
	for header in &chain {
		state.remember_authored(header.hash());
	}

	// The head moved to `chain[1]`: only `chain[2]` and `chain[3]` are still below it.
	state.forget_unreachable(&descendants(&[(&chain[2], 1), (&chain[3], 2)]));

	assert_eq!(state.own_recent, ours(&[&chain[2], &chain[3]]));
}

/// The turn claims an anchor slot once and may author several packages under that claim, so
/// the guard is per *turn*, not per block: only a second turn on the same anchor slot is
/// equivocation, and the next anchor always starts fresh.
#[test]
fn one_turn_per_anchor_slot() {
	assert!(!anchor_slot_claimed(None, 7));
	assert!(anchor_slot_claimed(Some(7), 7));
	assert!(anchor_slot_claimed(Some(7), 6));
	assert!(!anchor_slot_claimed(Some(7), 8));
}

/// The budget is one package per core, and never zero: with no core at all the turn still
/// authors its first package, which is never handed to a guarantor and so never holds
/// authoring.
#[test]
fn the_turn_budget_is_one_package_per_core() {
	assert_eq!(turn_package_budget(0), 1, "the no-core turn still authors one package");
	assert_eq!(turn_package_budget(1), 1);
	assert_eq!(turn_package_budget(2), 2);
}

/// The first package of a turn is authored even with no core to send it to; every later one
/// needs a core of its own. The runtime's refusal ends the burst regardless, however many
/// cores are left.
#[test]
fn a_turn_needs_a_core_for_every_package_after_the_first() {
	assert!(package_fits_turn(0, None, true), "the first package goes out with no core");
	assert!(package_fits_turn(0, Some(0), true));
	assert!(!package_fits_turn(1, None, true), "the cores are used up");
	assert!(package_fits_turn(1, Some(2), true));
	assert!(!package_fits_turn(0, Some(0), false), "velocity or capacity refused");
	assert!(!package_fits_turn(1, Some(2), false));
}

/// Once the first package of a turn is authored, every later one extends the branch it
/// started — the ordinary one or a re-root's — so the re-root state keeps tracking the
/// turn's tip instead of snapping back to the first block.
#[test]
fn a_turns_later_packages_extend_its_branch() {
	assert_eq!(next_parent_source(ParentSource::Reroot), ParentSource::Rerooted);
	assert_eq!(next_parent_source(ParentSource::Rerooted), ParentSource::Rerooted);
	assert_eq!(next_parent_source(ParentSource::Deepest), ParentSource::Deepest);
	assert_eq!(next_parent_source(ParentSource::AccumulatedHead), ParentSource::Deepest);
}

/// The guard exists to catch a JAM chain that stalled, so the boundary matters: a tip that is
/// exactly `MAX_TIP_LAG_SLOTS` behind is still usable, one slot older is not.
#[test]
fn a_tip_lagging_more_than_the_bound_is_stale() {
	assert!(tip_is_fresh(100, 100));
	assert!(tip_is_fresh(100, 100 - MAX_TIP_LAG_SLOTS));
	assert!(!tip_is_fresh(100, 100 - MAX_TIP_LAG_SLOTS - 1));
}

/// The timer has to land on the next slot boundary; being early would make the tick derive
/// the previous slot's number and skip itself on the one-turn-per-slot guard.
#[test]
fn the_timer_waits_for_the_next_slot_boundary() {
	let slot = Duration::from_millis(6000);
	assert_eq!(time_until_next_para_slot(Duration::from_millis(0), slot).as_millis(), 6000);
	assert_eq!(time_until_next_para_slot(Duration::from_millis(1000), slot).as_millis(), 5000);
	assert_eq!(time_until_next_para_slot(Duration::from_millis(5999), slot).as_millis(), 1);
	assert_eq!(time_until_next_para_slot(Duration::from_millis(6000), slot).as_millis(), 6000);
}

/// The block's timestamp is the anchor's, and its Aura slot is that timestamp over the slot
/// duration: the parachain, the collator and the JAM authorizer read one clock, so a collator
/// cannot pick its own time. Both sides must agree at every anchor, including a para slot
/// several JAM timeslots long.
#[test]
fn the_block_timestamp_and_para_slot_derive_from_the_anchor_slot() {
	for slot_duration_ms in [6000u64, 12_000, 18_000] {
		let slot_duration = SlotDuration::from_millis(slot_duration_ms);
		for anchor_slot in [0, 1, 7, 123_456] {
			let timestamp = Timestamp::new(timestamp_for_anchor(anchor_slot));

			assert_eq!(
				timestamp.as_millis(),
				(jam_types::JAM_COMMON_ERA + u64::from(anchor_slot) * 6) * 1000,
				"the timestamp is the anchor's wall-clock time",
			);
			assert_eq!(
				*Slot::from_timestamp(timestamp, slot_duration),
				cumulus_jam_aura_primitives::aura_slot_for_anchor(anchor_slot, slot_duration_ms),
				"the para slot is the anchor's Aura slot",
			);
		}
	}
}
