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
	collation::{
		anchor_chain::header_chain_until,
		in_flight::{
			appeared_in, due_for_sending, forget_package, plan_on_jam_block, InFlight,
			InFlightPackages, StatusSubscriptions,
		},
		policy::{
			original_package_expired, resubmission_due, ForgetWhenAnchorExpires, Observed,
			PolicyAction, ResubmissionPolicy, MAX_RESUBMIT_BLOCKS, REPORT_DEADLINE_SLOTS,
		},
		resubmit::{chain_above, rebuildable, resubmission_package},
	},
	package::{
		build_pov,
		tests::{anchored, header, package_source, signed, wp_hash},
		work_package_hash,
	},
	store::{
		hash_ledger::test_support::ledger,
		pov_store::{BlockPov, BlockPovs},
	},
};
use codec::DecodeAll;
use cumulus_jam_primitives::anchor_proof::{
	jam_header_hash, JamHeaderPrefix, MAX_ANCHOR_CHAIN_LEN,
};
use cumulus_primitives_core::{CumulusDigestItem, ParachainBlockData};
use cumulus_test_runtime::Block as TestBlock;
use futures::{stream::SelectAll, StreamExt};
use jam_interface::{
	BlockDesc, BoxStream, HeaderHash, Slot as JamSlot, WorkPackageHash, WorkPackageStatus,
};
use jam_std_common::{BlockInfo, Mmr, RecentBlocks};
use jam_types::{
	BoundedVec, CoreIndex, RecentBlockCount, SegmentTreeRoot, StateRootHash, VecMap, WorkReportHash,
};
use sp_core::H256;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use std::collections::{HashMap, HashSet};

/// `count` packages for a parent/child line of blocks, package `k` hashed as `[k; 32]`, all
/// anchored at slot `k`. The blocks form a chain because that is what a collator authors;
/// the packages themselves are independent, which is the point.
fn in_flight(count: u8) -> InFlightPackages<TestBlock> {
	let mut packages = InFlightPackages::new();
	let mut parent_hash = H256::repeat_byte(200);
	for index in 0..count {
		let block_header = header(u32::from(index) + 1, parent_hash);
		let block_hash = block_header.hash();
		let package = signed(&package_source(), &anchored(JamSlot::from(index)));
		packages.entries.push_back(InFlight {
			block_hash,
			block_number: *block_header.number(),
			parent_hash,
			wp_hash: wp_hash(index),
			package,
			anchored: anchored(JamSlot::from(index)),
			reported: false,
			resubmission: false,
			sends: 1,
			unsent: None,
		});
		parent_hash = block_hash;
	}
	packages
}

/// `count` held packages on `cores` in turn, package `k` naming package `k - 1`.
fn held_chain(count: u8, cores: &[CoreIndex]) -> InFlightPackages<TestBlock> {
	let mut packages = in_flight(count);
	for (index, entry) in packages.entries.iter_mut().enumerate() {
		entry.unsent = Some(vec![index as u8]);
		entry.anchored.submit_target = Some(cores[index % cores.len()]);
		if index > 0 {
			entry.package.context.prerequisites = vec![wp_hash(index as u8 - 1)].into();
		}
	}
	packages
}

/// Sends what `due_for_sending` releases at `tip`, the way `send_due` does.
fn send_at(
	packages: &mut InFlightPackages<TestBlock>,
	sent_at: &mut HashMap<CoreIndex, JamSlot>,
	tip: JamSlot,
) -> Vec<WorkPackageHash> {
	let due = due_for_sending(packages, tip, sent_at);
	for wp_hash in &due {
		let index = packages.position_of_package(*wp_hash).expect("due is in flight; qed");
		packages.entries[index].unsent = None;
		if let Some(core) = packages.entries[index].anchored.submit_target {
			sent_at.insert(core, tip);
		}
	}
	due
}

/// A resubmission round's burst on one core leaves one package per JAM slot, in order.
#[test]
fn a_burst_for_one_core_goes_out_one_package_per_slot() {
	let mut packages = held_chain(4, &[0]);
	let mut sent_at = HashMap::new();

	assert_eq!(send_at(&mut packages, &mut sent_at, 10), vec![wp_hash(0)]);
	assert!(send_at(&mut packages, &mut sent_at, 10).is_empty(), "the slot is spent");
	assert_eq!(send_at(&mut packages, &mut sent_at, 11), vec![wp_hash(1)]);
	assert_eq!(send_at(&mut packages, &mut sent_at, 12), vec![wp_hash(2)]);
	assert_eq!(send_at(&mut packages, &mut sent_at, 13), vec![wp_hash(3)]);
	assert!(send_at(&mut packages, &mut sent_at, 14).is_empty());
}

/// An elastic turn, one package per core, goes out at once; a package never overtakes a
/// prerequisite still held for its own core.
#[test]
fn packages_on_different_cores_go_out_together_but_never_before_their_prerequisite() {
	let mut packages = held_chain(3, &[0, 1, 2]);
	let mut sent_at = HashMap::new();
	assert_eq!(send_at(&mut packages, &mut sent_at, 10), vec![wp_hash(0), wp_hash(1), wp_hash(2)]);

	let mut packages = held_chain(3, &[0, 1, 0]);
	let mut sent_at = HashMap::from([(0, 10)]);
	assert!(send_at(&mut packages, &mut sent_at, 10).is_empty(), "core 0 is spent");
	assert_eq!(
		send_at(&mut packages, &mut sent_at, 11),
		vec![wp_hash(0), wp_hash(1)],
		"core 1 follows its prerequisite in the same pass",
	);
	assert_eq!(send_at(&mut packages, &mut sent_at, 12), vec![wp_hash(2)]);
}

/// β naming each hash in `reported`, one block per hash, newest last.
fn recent_blocks(reported: &[WorkPackageHash]) -> RecentBlocks {
	let history: BoundedVec<BlockInfo, RecentBlockCount> = reported
		.iter()
		.enumerate()
		.map(|(index, wp_hash)| {
			let mut named = VecMap::new();
			named.insert(*wp_hash, SegmentTreeRoot([0u8; 32]));
			BlockInfo {
				hash: HeaderHash::from([index as u8; 32]),
				beefy_root: [0u8; 32].into(),
				state_root: [0u8; 32].into(),
				slot: index as JamSlot,
				reported: named,
			}
		})
		.collect::<Vec<_>>()
		.try_into()
		.expect("within recent block count");
	RecentBlocks { history, mmr: Mmr::default() }
}

/// The β read is what tells "reported" apart from "still missing": a hash named by some block
/// in the history comes back with that block, and an unknown hash comes back empty.
#[test]
fn appeared_in_finds_the_block_that_named_the_package() {
	let named = wp_hash(0x11);
	let history = recent_blocks(&[wp_hash(0x22), named]);

	let found = appeared_in(&history, named).expect("the second block named it");
	assert_eq!(found.slot, 1);
	assert!(appeared_in(&history, wp_hash(0x33)).is_none());
}

/// One JAM block sorts every unreported package: a package in β is reported, one whose anchor
/// is past its deadline is forgotten, and the rest wait — however long they have been missing,
/// since nothing is ever sent twice.
#[test]
fn a_jam_block_sorts_reported_expired_and_waiting() {
	let tip = 100;
	let mut packages = in_flight(4);
	// 0: named in β.
	packages.entries[0].anchored.anchor_slot = tip;
	// 1: missing for most of its window, but its anchor still lives.
	packages.entries[1].anchored.anchor_slot = tip - (REPORT_DEADLINE_SLOTS - 1);
	// 2: missing and its anchor has expired.
	packages.entries[2].anchored.anchor_slot = tip - REPORT_DEADLINE_SLOTS;
	// 3: missing, but only just anchored.
	packages.entries[3].anchored.anchor_slot = tip;

	let history = recent_blocks(&[wp_hash(0)]);
	let plan = plan_on_jam_block(&packages, tip, &history, &ForgetWhenAnchorExpires);
	let actions: Vec<_> = plan.iter().map(|(hash, action, _)| (*hash, *action)).collect();

	assert_eq!(
		actions,
		vec![
			(wp_hash(0), PolicyAction::Reported),
			(wp_hash(1), PolicyAction::Wait),
			(wp_hash(2), PolicyAction::Forget),
			(wp_hash(3), PolicyAction::Wait),
		],
	);
	assert_eq!(plan[0].2.map(|block| block.slot), Some(0), "the β block travels with it");
}

/// A package already known to be reported is filtered out before the clock is even
/// consulted: nothing forgets a package JAM has already accepted.
#[test]
fn a_reported_package_is_left_alone_by_the_clock() {
	let tip = 100;
	let mut packages = in_flight(2);
	packages.entries[0].reported = true;
	packages.entries[0].anchored.anchor_slot = tip - REPORT_DEADLINE_SLOTS;
	packages.entries[1].anchored.anchor_slot = tip;

	let plan = plan_on_jam_block(&packages, tip, &recent_blocks(&[]), &ForgetWhenAnchorExpires);

	assert_eq!(plan.len(), 1, "the reported package is not re-decided");
	assert_eq!(plan[0].0, wp_hash(1));
	assert_eq!(plan[0].1, PolicyAction::Wait);
}

/// The para head advancing settles everything at or below its height: the block that became
/// the head accumulated, and a package for another block of that height lost the fork — the
/// service sweeps it out of its reorder buffer by exactly this rule.
#[test]
fn the_new_head_settles_every_package_at_or_below_its_height() {
	let mut packages = in_flight(4);
	let third = packages.entries[2].block_number;

	let settled = packages.remove_up_to(third);

	assert_eq!(settled.len(), 3);
	assert_eq!(packages.len(), 1);
	assert_eq!(packages.entries[0].wp_hash, wp_hash(3));
}

/// A head deeper than anything this collator has in flight settles the lot; a head below
/// them all settles nothing.
#[test]
fn a_head_past_or_behind_everything_settles_accordingly() {
	let mut packages = in_flight(3);
	assert_eq!(packages.remove_up_to(0).len(), 0, "nothing is at or below height zero");
	assert_eq!(packages.remove_up_to(99).len(), 3);
	assert_eq!(packages.len(), 0);
}

/// A package that is neither the head nor below it stays in flight even though its block is
/// nothing this collator can prove yet: under 5a the service buffers a block whose parent has
/// not arrived, so a package one height ahead of the head is not lost, it is early.
#[test]
fn a_package_above_the_head_is_early_rather_than_lost() {
	let mut packages = in_flight(2);
	let first = packages.entries[0].block_number;

	packages.remove_up_to(first);

	assert_eq!(packages.len(), 1);
	assert_eq!(packages.entries[0].wp_hash, wp_hash(1));
}

/// Forgetting one package must leave every other one exactly where it was: packages are
/// independent now, and dropping a "tail" would throw away blocks that can still accumulate.
#[test]
fn forgetting_one_package_keeps_the_others() {
	let mut packages = in_flight(4);
	let index = packages.position_of_package(wp_hash(1)).expect("the package is in flight");

	let forgotten = packages.remove(index);

	assert_eq!(forgotten.wp_hash, wp_hash(1));
	assert_eq!(packages.len(), 3);
	assert_eq!(
		packages.entries.iter().map(|entry| entry.wp_hash).collect::<Vec<_>>(),
		vec![wp_hash(0), wp_hash(2), wp_hash(3)],
	);
}

/// A forgotten package's child goes with it: the child names the parent's package as its
/// prerequisite, and a prerequisite nothing will ever report blocks the child for good. Both
/// entries come back so `forget` closes both subscriptions.
#[test]
fn cascade_forget_drops_child_package() {
	let mut packages = in_flight(2);
	let parent_block_hash = packages.entries[0].block_hash;
	let parent = packages.entries[0].wp_hash;
	let child = packages.entries[1].wp_hash;
	packages.entries[1].anchored.context.prerequisites = vec![parent].into();
	packages.entries[1].package.context.prerequisites = vec![parent].into();
	let ledger = ledger();
	ledger.insert(&parent_block_hash.into(), parent).expect("insert ok; qed");

	let forgotten = forget_package(&mut packages, &ledger, 0);

	assert_eq!(forgotten.len(), 2, "the child is forgotten with the parent it named");
	assert!(packages.position_of_package(parent).is_none());
	assert!(packages.position_of_package(child).is_none(), "no child outlives its parent");
}

/// A forgotten package leaves no ledger entry behind: that entry is what a later child would
/// name as its prerequisite, and keeping it would chain the child to a package nothing will
/// ever report.
#[test]
fn forget_removes_the_ledger_entry() {
	let mut packages = in_flight(1);
	let block_hash = packages.entries[0].block_hash;
	let submitted = packages.entries[0].wp_hash;
	let ledger = ledger();
	ledger.insert(&block_hash.into(), submitted).expect("insert ok; qed");

	forget_package(&mut packages, &ledger, 0);

	assert_eq!(
		ledger.get(&block_hash.into()).expect("read ok; qed"),
		None,
		"a forgotten package leaves no entry for a child to name",
	);
}

/// A status stream that never ends on its own — which is what a real subscription is, and
/// the reason nothing leaves the select unless it is ended deliberately.
fn endless_statuses() -> BoxStream<'static, (WorkPackageHash, WorkPackageStatus)> {
	futures::stream::pending().boxed()
}

/// The node closes a status subscription only when the client drops its stream, and it caps a
/// connection at 1024 of them. A handle that outlives its entry therefore leaks a
/// subscription the node holds open for good: live, after ~1050 packages, every new
/// submission failed with "Too many subscriptions on the connection". Whichever way a package
/// stops being tracked — accumulated, superseded, forgotten — its subscription has to go
/// with it, and following it again must not add a second one.
#[test]
fn a_status_subscription_never_outlives_the_package_it_follows() {
	let mut packages = in_flight(4);
	let mut subscriptions = StatusSubscriptions::new();
	let mut statuses = SelectAll::new();
	for entry in &packages.entries {
		statuses.push(subscriptions.follow(entry.wp_hash, endless_statuses()));
	}
	assert_eq!(subscriptions.len(), 4);

	let followed_twice = packages.entries[3].wp_hash;
	statuses.push(subscriptions.follow(followed_twice, endless_statuses()));
	assert_eq!(subscriptions.len(), 4, "following again replaces a package's subscription");

	let settled = packages.entries[1].block_number;
	for entry in packages.remove_up_to(settled) {
		assert!(subscriptions.close(entry.wp_hash), "the settled packages are closed");
	}
	assert_eq!(subscriptions.len(), 2);

	for entry in packages.remove_up_to(99) {
		assert!(subscriptions.close(entry.wp_hash), "and so is a forgotten one");
	}
	assert_eq!(subscriptions.len(), 0, "no handle outlives the packages");
	assert!(!subscriptions.close(followed_twice), "closing twice is not a leak either");
}

/// ...and closing has to actually end the stream, because that — and only that — is what
/// drops it out of the select and unsubscribes on the node side.
#[test]
fn closing_a_subscription_ends_its_stream() {
	let mut subscriptions = StatusSubscriptions::new();
	let mut statuses = SelectAll::new();
	statuses.push(subscriptions.follow(wp_hash(1), endless_statuses()));
	assert!(subscriptions.close(wp_hash(1)));

	assert!(
		futures::executor::block_on(statuses.next()).is_none(),
		"the select is empty again, so the subscription was dropped",
	);
}

/// Block `number` on top of `parent_hash`, built against a JAM anchor at `anchor_slot` and
/// kept in the PoV store the way the authoring and import paths keep one.
fn kept_block(number: u32, parent_hash: H256, anchor_slot: JamSlot) -> BlockPov<TestBlock> {
	let mut block_header = header(number, parent_hash);
	block_header.digest.push(
		CumulusDigestItem::JamParent {
			anchor: H256::repeat_byte(anchor_slot as u8),
			anchor_slot,
			lookup_anchor: H256::repeat_byte(0x80 | anchor_slot as u8),
			lookup_anchor_slot: anchor_slot.saturating_sub(1),
		}
		.to_digest_item(),
	);
	let mut source = package_source();
	source.blocks = vec![TestBlock::new(block_header, vec![])];
	source.parent_header = header(number - 1, H256::repeat_byte(6));
	let (package, _pov) = source.package(&anchored(11));
	let spec = package.items[0].extrinsics[0].clone();
	source.into_block_pov(spec).expect("the block carries `JamParent`")
}

/// The chain a round rebuilds runs from the head's child to the local best block, oldest
/// first — and only while the head is an ancestor of best: a head on another fork has nothing
/// above it on the best chain, so the walk yields nothing rather than the other fork's blocks.
#[test]
fn the_chain_above_the_head_is_the_best_chains_blocks_past_it() {
	let h0 = header(0, H256::repeat_byte(0));
	let h1 = header(1, h0.hash());
	let h2 = header(2, h1.hash());
	let h3 = header(3, h2.hash());
	let mut fork = header(1, h0.hash());
	fork.state_root = H256::repeat_byte(0xF0);
	let headers = [h0.clone(), h1.clone(), h2.clone(), h3.clone(), fork.clone()];
	let lookup = |hash: H256| headers.iter().find(|h| h.hash() == hash).cloned();

	assert_eq!(
		chain_above(h3.hash(), (h1.hash(), 1), lookup),
		Some(vec![h2.hash(), h3.hash()]),
		"the head's child first, best last",
	);
	assert_eq!(chain_above(h3.hash(), (h3.hash(), 3), lookup), Some(vec![]), "best is the head");
	assert_eq!(chain_above(h3.hash(), (fork.hash(), 1), lookup), None, "head on another fork");
	assert_eq!(chain_above(h1.hash(), (h3.hash(), 3), lookup), None, "best behind the head");
	assert_eq!(chain_above(H256::repeat_byte(0xEE), (h1.hash(), 1), lookup), None, "unknown");
}

/// A round rebuilds the leading run of blocks whose PoVs are stored and whose original
/// packages have expired, oldest first and at most `limit` long; a block failing either test
/// ends the run, and the blocks behind it wait for a later round.
#[test]
fn only_the_leading_run_of_expired_stored_blocks_is_rebuilt() {
	let anchor_slot = 10;
	let a = kept_block(1, H256::repeat_byte(7), anchor_slot);
	let b = kept_block(2, a.block.hash(), anchor_slot + 1);
	let c = kept_block(3, b.block.hash(), anchor_slot + 1);
	let (ha, hb, hc) = (a.block.hash(), b.block.hash(), c.block.hash());
	let store = BlockPovs::<TestBlock>::new(8);
	for kept in [&a, &b, &c] {
		store.insert(kept.block.hash(), kept.clone());
	}
	let above = vec![ha, hb, hc];
	let hashes = |blocks: Vec<(H256, BlockPov<TestBlock>)>| {
		blocks.into_iter().map(|(hash, _)| hash).collect::<Vec<_>>()
	};

	// However long the head has stalled, a block whose first package can still be reported
	// is never rebuilt.
	let still_live = anchor_slot + REPORT_DEADLINE_SLOTS - 1;
	assert!(rebuildable(&above, still_live, |h| store.get(h), MAX_RESUBMIT_BLOCKS).is_empty());
	// A's package has expired, B's and C's have not: the run is A alone.
	let a_expired = anchor_slot + REPORT_DEADLINE_SLOTS;
	assert_eq!(
		hashes(rebuildable(&above, a_expired, |h| store.get(h), MAX_RESUBMIT_BLOCKS)),
		vec![ha],
	);
	// All expired: the whole run, oldest first, under the cap.
	let all_expired = anchor_slot + 1 + REPORT_DEADLINE_SLOTS;
	assert_eq!(
		hashes(rebuildable(&above, all_expired, |h| store.get(h), MAX_RESUBMIT_BLOCKS)),
		vec![ha, hb, hc],
	);
	assert_eq!(hashes(rebuildable(&above, all_expired, |h| store.get(h), 2)), vec![ha, hb]);
	// A gap in the store ends the run: C is not rebuilt past a missing B.
	let gapped = BlockPovs::<TestBlock>::new(8);
	gapped.insert(ha, a.clone());
	gapped.insert(hc, c.clone());
	assert_eq!(
		hashes(rebuildable(&above, all_expired, |h| gapped.get(h), MAX_RESUBMIT_BLOCKS)),
		vec![ha],
	);
	// And a missing first block means nothing is rebuilt at all.
	let empty = BlockPovs::<TestBlock>::new(8);
	assert!(rebuildable(&above, all_expired, |h| empty.get(h), MAX_RESUBMIT_BLOCKS).is_empty());
}

/// A resubmission package carries exactly one extrinsic — the rebuilt PoV, whose
/// `ParachainBlockData` carries the anchor chain — whose spec names the blob it is submitted
/// with; package k > 0 of a round names package k − 1 as its single prerequisite, and package
/// 0 names nothing.
#[test]
fn resubmission_packages_chain_and_carry_the_anchor_chain_in_the_pov() {
	let first = kept_block(1, H256::repeat_byte(7), 5);
	let second = kept_block(2, first.block.hash(), 6);
	let headers = vec![vec![1u8; 120], vec![2u8; 120]];
	let source = package_source();
	let context = anchored(30).context;
	let assemble = |block_pov: &BlockPov<TestBlock>, prerequisite| {
		resubmission_package(
			block_pov,
			&headers,
			prerequisite,
			source.validation_code_hash,
			&source.params,
			source.authorizer.clone(),
			&context,
		)
	};

	let (package_0, pov_0) = assemble(&first, None);
	let (package_1, pov_1) = assemble(&second, Some(work_package_hash(&package_0)));

	assert!(package_0.context.prerequisites.as_ref().is_empty(), "package 0 starts the chain");
	assert_eq!(
		package_1.context.prerequisites.as_ref(),
		&[work_package_hash(&package_0)],
		"package 1 names exactly package 0",
	);
	for (package, pov, kept) in [(&package_0, &pov_0, &first), (&package_1, &pov_1, &second)] {
		assert_eq!(package.items[0].extrinsics.len(), 1, "the PoV is the one extrinsic");
		let spec = &package.items[0].extrinsics[0];
		assert_eq!(
			(spec.hash, spec.len),
			(jam_types::ExtrinsicHash::from(jam_std_common::hash_raw(pov)), pov.len() as u32),
			"the spec names the PoV's hash and length",
		);
		assert_eq!(
			*pov,
			build_pov(
				std::slice::from_ref(&kept.block),
				&kept.compact_proof,
				&kept.parent_header,
				&kept.additional_data,
				headers.clone(),
			),
			"the extrinsic is the block's rebuilt PoV",
		);
		let decoded = ParachainBlockData::<TestBlock>::decode_all(&mut &pov[..])
			.expect("PoV decodes as ParachainBlockData");
		assert_eq!(decoded.anchor_chain(), &headers[..], "the PoV carries the anchor chain");
		assert_eq!(package.context.anchor, context.anchor, "anchored where the round is");
	}
}

fn reportable(remaining_blocks: u16) -> WorkPackageStatus {
	WorkPackageStatus::Reportable { remaining_blocks }
}

fn reported() -> WorkPackageStatus {
	WorkPackageStatus::Reported {
		reported_in: BlockDesc { header_hash: HeaderHash::from([1u8; 32]), slot: 1 },
		core: CoreIndex::default(),
		report_hash: WorkReportHash::from([2u8; 32]),
	}
}

fn observed(tip_slot: JamSlot, anchor_slot: JamSlot, on_chain: bool) -> Observed {
	Observed { tip_slot, anchor_slot, on_chain }
}

#[test]
fn waits_while_reportable_and_finishes_on_reported() {
	let policy = ForgetWhenAnchorExpires;
	assert_eq!(policy.on_status(&reportable(8)), PolicyAction::Wait);
	assert_eq!(policy.on_status(&reportable(3)), PolicyAction::Wait);
	assert_eq!(policy.on_status(&reported()), PolicyAction::Reported);
}

/// `Ready` is still in flight — queued for accumulation, not accumulated — so it is treated
/// exactly like `Reported`: the deadline stops applying and the para head decides completion.
#[test]
fn a_ready_package_is_treated_as_reported() {
	let policy = ForgetWhenAnchorExpires;
	let ready = WorkPackageStatus::Ready {
		reported_in: BlockDesc { header_hash: HeaderHash::from([1u8; 32]), slot: 1 },
		core: CoreIndex::default(),
		report_hash: WorkReportHash::from([2u8; 32]),
		ready_in: BlockDesc { header_hash: HeaderHash::from([3u8; 32]), slot: 2 },
	};
	assert_eq!(policy.on_status(&ready), PolicyAction::Reported);
}

/// A failure — "Not reported in time" or a spent anchor — is terminal: the package is
/// forgotten, and its block waits for a resubmission round to rebuild it into a new package.
#[test]
fn a_failure_is_forgotten() {
	let policy = ForgetWhenAnchorExpires;
	let failed = WorkPackageStatus::Failed("anchor expired".into());
	assert_eq!(policy.on_status(&failed), PolicyAction::Forget);
}

/// β not naming the package is no reason to act while its anchor can still be reported:
/// nothing is ever sent twice, so the silence is waited out, however long it lasts.
#[test]
fn silence_is_waited_out_while_the_anchor_lives() {
	let policy = ForgetWhenAnchorExpires;
	assert_eq!(policy.on_jam_block(observed(100, 100, false)), PolicyAction::Wait);
	assert_eq!(
		policy.on_jam_block(observed(100 + REPORT_DEADLINE_SLOTS - 1, 100, false)),
		PolicyAction::Wait,
	);
}

/// At `anchor + REPORT_DEADLINE_SLOTS` the package can never be reported, so it is forgotten;
/// one slot earlier it still waits.
#[test]
fn an_expired_anchor_is_forgotten() {
	let policy = ForgetWhenAnchorExpires;
	let at = |tip: JamSlot| observed(tip, 0, false);

	assert_eq!(policy.on_jam_block(at(REPORT_DEADLINE_SLOTS - 1)), PolicyAction::Wait);
	assert_eq!(policy.on_jam_block(at(REPORT_DEADLINE_SLOTS)), PolicyAction::Forget);
}

/// Appearing in β is the truth, whatever the deadline says: a reported package is reported
/// even when its anchor has expired.
#[test]
fn appearing_on_chain_wins_over_the_deadline() {
	let policy = ForgetWhenAnchorExpires;
	let expired = observed(100, 0, true);
	assert_eq!(policy.on_jam_block(expired), PolicyAction::Reported);
}

// ---- resubmission_due tests ----

#[test]
fn resubmission_fires_when_all_conditions_hold() {
	assert!(resubmission_due(
		10,    // tip_slot
		7,     // head_changed_at
		None,  // last_round_at
		true,  // has_unaccumulated_child
		true,  // our_turn
		false, // round_in_flight
	));
}

#[test]
fn resubmission_blocked_when_not_our_turn() {
	assert!(!resubmission_due(10, 7, None, true, false, false));
}

#[test]
fn resubmission_blocked_when_round_in_flight() {
	assert!(!resubmission_due(10, 7, None, true, true, true));
}

#[test]
fn resubmission_blocked_when_no_unaccumulated_child() {
	assert!(!resubmission_due(10, 7, None, false, true, false));
}

#[test]
fn resubmission_blocked_when_stall_too_short() {
	assert!(!resubmission_due(9, 7, None, true, true, false));
}

#[test]
fn last_round_at_extends_the_stall_clock() {
	// head changed at 5, last round at 8 → stall measured from 8
	assert!(!resubmission_due(10, 5, Some(8), true, true, false));
	assert!(resubmission_due(11, 5, Some(8), true, true, false));
}

#[test]
fn head_change_resets_the_stall_clock() {
	// head changed at 9, no prior round → stall measured from 9
	assert!(!resubmission_due(11, 9, None, true, true, false));
	assert!(resubmission_due(12, 9, None, true, true, false));
}

// ---- original_package_expired tests ----

/// A block is rebuildable exactly from the slot its original package is forgotten at: one slot
/// before the deadline the package may still be reported, however long the head has stalled.
#[test]
fn a_block_is_rebuildable_once_its_original_package_has_expired() {
	let anchor_slot = 100;
	assert!(!original_package_expired(anchor_slot, anchor_slot));
	assert!(!original_package_expired(anchor_slot + REPORT_DEADLINE_SLOTS - 1, anchor_slot));
	assert!(original_package_expired(anchor_slot + REPORT_DEADLINE_SLOTS, anchor_slot));
	assert!(original_package_expired(anchor_slot + REPORT_DEADLINE_SLOTS + 50, anchor_slot));
}

/// The deadline the gate applies is the one the status policy forgets the package by, so the
/// two can never disagree about a block of this collator's own.
#[test]
fn the_rebuild_gate_agrees_with_the_forget_deadline() {
	let policy = ForgetWhenAnchorExpires;
	for tip in 0..3 * REPORT_DEADLINE_SLOTS {
		let forgotten = policy.on_jam_block(observed(tip, REPORT_DEADLINE_SLOTS, false)) ==
			PolicyAction::Forget;
		assert_eq!(original_package_expired(tip, REPORT_DEADLINE_SLOTS), forgotten, "tip {tip}");
	}
}

// ---- header_chain_until tests ----

/// Build a synthetic encoded header: 100-byte prefix + 200-byte tail.
fn synthetic_header(parent: HeaderHash, state_root: H256, slot: u32) -> Vec<u8> {
	let mut bytes = vec![0u8; 300];
	bytes[0..32].copy_from_slice(&parent.0);
	bytes[32..64].copy_from_slice(state_root.as_bytes());
	// 64..96 = extrinsic hash (zeros)
	bytes[96..100].copy_from_slice(&slot.to_le_bytes());
	bytes
}

/// Build a chain of `len` synthetic headers, newest first. Returns (headers, hashes).
fn build_chain(len: usize, start_slot: u32) -> (Vec<Vec<u8>>, Vec<HeaderHash>) {
	let mut headers = Vec::new();
	let mut hashes = Vec::new();
	// Build oldest first, then reverse.
	let mut parent = HeaderHash::from([0xFFu8; 32]);
	let mut chain: Vec<(Vec<u8>, HeaderHash)> = Vec::new();
	for i in 0..len {
		let slot = start_slot + i as u32;
		let encoded = synthetic_header(parent, H256::repeat_byte((i + 1) as u8), slot);
		let hash = HeaderHash(jam_header_hash(&encoded).0);
		chain.push((encoded, hash));
		parent = hash;
	}
	chain.reverse();
	for (enc, hash) in chain {
		headers.push(enc);
		hashes.push(hash);
	}
	(headers, hashes)
}

#[tokio::test]
async fn header_chain_finds_all_needed_hashes() {
	let (headers, hashes) = build_chain(5, 10);
	// Map: hash -> encoded header
	let map: std::collections::HashMap<HeaderHash, Vec<u8>> =
		hashes.iter().cloned().zip(headers.iter().cloned()).collect();

	let needed: HashSet<HeaderHash> = [hashes[0], hashes[2], hashes[4]].into();
	let result = header_chain_until(hashes[0], needed, |h| {
		let m = map.clone();
		async move { m.get(&h).cloned().ok_or_else(|| format!("not found: {h:?}")) }
	})
	.await
	.expect("the walk succeeds");

	assert!(result.missing.is_empty(), "all needed hashes found");
	assert_eq!(result.headers.len(), 5, "walked the full chain to the oldest needed");
	assert_eq!(result.hashes, hashes, "each header is filed under the hash it was fetched for");
}

/// One walk serves a whole round: the chain reaching every block's anchors is cut per package
/// to the oldest hash that package needs, and a hash the walk never saw cuts nothing.
#[tokio::test]
async fn a_chain_is_cut_to_the_oldest_hash_a_package_needs() {
	let (headers, hashes) = build_chain(6, 1);
	let map: std::collections::HashMap<HeaderHash, Vec<u8>> =
		hashes.iter().cloned().zip(headers.iter().cloned()).collect();
	let needed: HashSet<HeaderHash> = [hashes[1], hashes[4]].into();
	let chain = header_chain_until(hashes[0], needed, |h| {
		let m = map.clone();
		async move { m.get(&h).cloned().ok_or_else(|| format!("not found: {h:?}")) }
	})
	.await
	.expect("the walk succeeds");
	assert_eq!(chain.headers.len(), 5, "the walk stopped at the oldest needed hash");

	assert_eq!(chain.prefix_reaching(&[hashes[1], hashes[0]]), Some(&headers[..=1]));
	assert_eq!(chain.prefix_reaching(&[hashes[2], hashes[4]]), Some(&headers[..=4]));
	assert_eq!(chain.prefix_reaching(&[hashes[4], hashes[1]]), Some(&headers[..=4]));
	assert_eq!(chain.prefix_reaching(&[hashes[1], hashes[5]]), None, "never walked to");
	assert_eq!(chain.prefix_reaching(&[]), None, "nothing to reach");
}

/// Bytes that do not hash to the header they were fetched as cannot prove anything: the walk
/// fails on them instead of handing the PVF a chain it will reject.
#[tokio::test]
async fn header_chain_rejects_bytes_that_do_not_hash_to_the_requested_header() {
	let (headers, hashes) = build_chain(2, 1);
	let needed: HashSet<HeaderHash> = [hashes[1]].into();
	let result = header_chain_until(hashes[0], needed, |_| {
		let wrong = headers[1].clone();
		async move { Ok::<_, String>(wrong) }
	})
	.await;

	let error = result.err().expect("the walk fails");
	assert!(error.contains(&format!("{:?}", hashes[0])), "{error}");
}

#[tokio::test]
async fn header_chain_reports_missing_when_beyond_cap() {
	// Build a chain longer than MAX_ANCHOR_CHAIN_LEN and request a hash past the cap.
	let (headers, hashes) = build_chain(MAX_ANCHOR_CHAIN_LEN + 5, 1);
	let map: std::collections::HashMap<HeaderHash, Vec<u8>> =
		hashes.iter().cloned().zip(headers.iter().cloned()).collect();

	let far_hash = hashes[MAX_ANCHOR_CHAIN_LEN + 2];
	let needed: HashSet<HeaderHash> = [hashes[0], far_hash].into();
	let result = header_chain_until(hashes[0], needed, |h| {
		let m = map.clone();
		async move { m.get(&h).cloned().ok_or_else(|| format!("not found: {h:?}")) }
	})
	.await
	.expect("the walk succeeds");

	assert!(result.missing.contains(&far_hash), "the distant hash is missing");
	assert_eq!(result.headers.len(), MAX_ANCHOR_CHAIN_LEN);
}

#[tokio::test]
async fn header_chain_stops_at_oldest_needed() {
	let (headers, hashes) = build_chain(10, 1);
	let map: std::collections::HashMap<HeaderHash, Vec<u8>> =
		hashes.iter().cloned().zip(headers.iter().cloned()).collect();

	// Only need the first and third header — should stop at the third.
	let needed: HashSet<HeaderHash> = [hashes[0], hashes[2]].into();
	let result = header_chain_until(hashes[0], needed, |h| {
		let m = map.clone();
		async move { m.get(&h).cloned().ok_or_else(|| format!("not found: {h:?}")) }
	})
	.await
	.expect("the walk succeeds");

	assert!(result.missing.is_empty());
	assert_eq!(result.headers.len(), 3, "stopped at the oldest needed hash");
}

#[tokio::test]
async fn header_chain_propagates_fetch_error() {
	let needed: HashSet<HeaderHash> = [HeaderHash::from([1u8; 32])].into();
	let result = header_chain_until(HeaderHash::from([1u8; 32]), needed, |_| async {
		Err::<Vec<u8>, String>("node unreachable".into())
	})
	.await;

	assert!(result.is_err());
}

/// Pins [`JamHeaderPrefix`] and [`jam_header_hash`] to polkajam's real header encoding, which
/// the synthetic headers above only imitate.
#[test]
fn header_prefix_and_hash_match_polkajam_header_encoding() {
	let mut header = jam_std_common::Header::genesis();
	header.parent = HeaderHash::from([0xABu8; 32]);
	header.parent_state_root = StateRootHash::from([0xCDu8; 32]);
	header.slot = 0x0403_0201;
	let encoded = jam_codec::Encode::encode(&header);

	let prefix = JamHeaderPrefix::parse(&encoded).expect("a real header is long enough");
	assert_eq!(prefix.parent.0, header.parent.0);
	assert_eq!(prefix.parent_state_root.0, header.parent_state_root.0);
	assert_eq!(prefix.slot, header.slot);
	assert_eq!(HeaderHash(jam_header_hash(&encoded).0), header.hash());
}
