// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! What happens to a parachain when the cores under it are handed out, taken away and moved.
//!
//! These three tests are about the boundary between the two things a JAM collator does. Authoring
//! is local and unconditional: a collator with no core keeps building blocks. Accumulation is not:
//! a head only moves once a work package has been authorized on a core, guaranteed, reported and
//! accumulated. So every assertion here reads *both* — the collator's own height, and the head
//! parasim has stored for the para — because it is the gap between them that says what the core
//! layer is doing.
//!
//! A tiny network has exactly two cores, which is why two paras is the widest test there is and
//! why a single para's spare core is core 1.
//!
//! Freeing a core parks it rather than emptying it: the same authorizer code stays on it under a
//! config naming no para, so it stops carrying parachain work but keeps taking control packages.
//! That is what lets the stall test heal on the very core it took away, and it is the difference
//! between the two single-para tests here — one puts the para back where it was, the other moves
//! it somewhere else.

use super::{
	collators::Para,
	env::parasim_tool_or_skip,
	harness::{finish, setup, Run},
	rpc::CollatorRpc,
};
use anyhow::Context;
use std::{path::Path, time::Duration};

/// The para the single-para tests run, and the core it starts on.
const PARA: u32 = 0;
const CORE: u32 = 0;

/// The other core of a tiny network. In a single-para run genesis names only the para's own core,
/// so this one keeps the null authorizer and service 0 as its assigner — which is what leaves the
/// bootstrap lane open to it, and is how the reassignment test moves a para onto it.
const SPARE_CORE: u32 = 1;

/// The accumulated height a para has to reach before a test starts interfering with its cores.
///
/// It is deliberately a JAM-side number rather than a local one: a collator builds blocks whether
/// or not anything works, so only a head parasim has stored proves the whole pipeline is running.
const HEALTHY_HEAD: u64 = 5;

/// How long a para is given to get there, and to recover afterwards. A JAM slot is six seconds and
/// a zombienet-spawned network is lumpy, so these are minutes rather than seconds.
const WARM_UP: Duration = Duration::from_secs(8 * 60);
const HEAL_BUDGET: Duration = Duration::from_secs(8 * 60);

/// Everything after the warm-up phase is wall-clock bound, so these runs need more than the single
/// phase [`super::harness::DEADLINE`] allows.
const EXTRA_TIME: Duration = Duration::from_secs(20 * 60);

/// Two paras, one core each, disjoint collator sets: the full width of a tiny JAM network.
///
/// The point is that nothing but JAM itself is shared. The paras have different ids, so their
/// authorizer hashes differ and each core authorizes exactly one of them; they have different
/// collator sets, so no key signs for both; and parasim keeps their heads in separate storage. If
/// any of that leaked, one para's head would stop tracking its own chain — which is what the
/// second half of this test checks, and what a plain "both are producing blocks" would miss.
#[tokio::test(flavor = "multi_thread")]
async fn two_paras_on_two_cores_build_blocks() -> Result<(), anyhow::Error> {
	const TEST: &str = "two_paras_on_two_cores_build_blocks";
	/// Long enough that a stall lasting a few slots cannot be mistaken for progress; the figure
	/// the single-para progress tests use, so the two are comparable.
	const BLOCKS: u64 = 30;
	const FINALIZED: u64 = 25;

	let Some(binaries) = setup(TEST) else { return Ok(()) };
	let paras = vec![
		Para { id: 0, core: 0, collators: vec![0, 1] },
		Para { id: 1, core: 1, collators: vec![2, 3] },
	];

	let mut run = Run::start(TEST, &binaries, paras).await?;
	let result = async {
		let heights = run.wait_for_blocks(BLOCKS, FINALIZED).await?;
		log::info!("both paras are at full cadence: {}", run.describe(&heights));
		heads_belong_to_their_own_para(&mut run).await
	}
	.await;
	finish(run, result).await
}

/// Assert each para's accumulated head is a block of that para's chain, and of no other.
///
/// Two parachains have disjoint block hashes, so the collator that has never heard of a hash is
/// the proof: para 0's collator knowing a head parasim filed under para 1 would mean the two
/// chains had converged, and para 1's collator knowing one filed under para 0 would mean parasim
/// had mixed their state up.
async fn heads_belong_to_their_own_para(run: &mut Run) -> anyhow::Result<()> {
	let rpcs = run.rpcs().await?;
	let ids: Vec<u32> = run.paras.iter().map(|para| para.para.id).collect();

	let mut heads = Vec::new();
	for (index, rpc) in rpcs.iter().enumerate() {
		let progress = run.sample(index, rpc).await?;
		let head = progress.jam_head.clone().with_context(|| {
			format!(
				"para {} built {} blocks but JAM accumulated no head for it at all",
				ids[index], progress.height.best
			)
		})?;
		log::info!("para {}: {progress}", ids[index]);
		heads.push(head);
	}

	for (index, rpc) in rpcs.iter().enumerate() {
		for (owner, head) in ids.iter().zip(&heads) {
			let known = rpc.height_of(&head.hash).await?;
			match (*owner == ids[index], known) {
				(true, Some(number)) => anyhow::ensure!(
					number == head.number,
					"para {owner}'s collator has {} at height {number}, but JAM accumulated it as \
					 {head}",
					head.hash,
				),
				(true, None) => anyhow::bail!(
					"JAM accumulated {head} for para {owner}, but para {owner}'s own collator has \
					 never seen that block"
				),
				(false, Some(number)) => anyhow::bail!(
					"para {}'s collator knows {head}, which JAM accumulated for para {owner}, at \
					 height {number} — the two paras are not running separate chains",
					ids[index],
				),
				(false, None) => {},
			}
		}
	}
	log::info!("each para's accumulated head is a block of its own chain and of no other");
	Ok(())
}

/// Taking a para's core away freezes its head on JAM while it keeps authoring, and giving the
/// same core back brings the head back.
///
/// This is the failure mode the whole core layer has to survive: nothing tells a collator that its
/// core is gone. Packages keep being submitted for as long as the old authorizer lasts in the
/// pool, then they are simply never reported, the soft resubmit spends its attempts on silence,
/// the manager forgets them, and the only visible consequence is a head that has stopped. What
/// must *not* happen is the collator stopping too, so the assertion is deliberately two-sided:
/// the JAM head stands still and the local chain does not.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "mid-run core control is out of scope: the target setup preconfigures cores in genesis"]
async fn freeing_the_core_freezes_the_para_head_until_it_is_assigned_again(
) -> Result<(), anyhow::Error> {
	const TEST: &str = "freeing_the_core_freezes_the_para_head_until_it_is_assigned_again";

	let Some(binaries) = setup(TEST) else { return Ok(()) };
	// Before the network is spawned: whether the tool is there decides whether this test can run
	// at all, and eight minutes of warm-up is a long way to go to find that out.
	let Some(tool) = parasim_tool_or_skip(TEST, &binaries) else { return Ok(()) };

	let mut run = Run::start(TEST, &binaries, vec![Para::single(1)]).await?;
	run.extend_deadline(EXTRA_TIME);

	let result = stall_then_heal(&mut run, &tool).await;
	finish(run, result).await
}

async fn stall_then_heal(run: &mut Run, tool: &Path) -> anyhow::Result<()> {
	/// How long the head has to stand still before the stall is called. The builder re-roots after
	/// eight para slots, so this is comfortably more than that: a run that reaches this point has
	/// been through the whole soft-resubmit and re-root sequence, not just a slow block.
	const STILL_FOR: Duration = Duration::from_secs(90);
	const STALL_BUDGET: Duration = Duration::from_secs(10 * 60);
	/// Blocks the collator has to author while the head is frozen. A stalled builder authors more
	/// slowly than a healthy one — it fills its buffer above the stuck head, then re-roots and
	/// starts again — and a measured stall of this length produced ten. Five is half of that,
	/// against a collator that stopped, which would produce none.
	const LOCAL_BLOCKS_WHILE_STALLED: usize = 5;

	let para = run.paras[0].para.clone();
	let rpc = first_rpc(run).await?;

	let healthy = run.wait_for_jam_head(0, &rpc, HEALTHY_HEAD, WARM_UP).await?;
	log::info!("para {PARA} is healthy on core {CORE}: {healthy}");

	run.network.host_authorizer_for_control_packages(tool)?;
	run.network.free_core(tool, &para, CORE)?;
	let freed = run.sample(0, &rpc).await?;
	// Counted from here, not from the start of the run: a slow patch early on can stall the head
	// for eight slots too, and a re-root that happened before the core was taken away would say
	// nothing about what taking it away does.
	let reroots_before = reroots(run);
	let authored_before = authored(run);
	log::info!("core {CORE} is parked; para {PARA} was at {freed} when it went");

	let frozen = run.wait_for_frozen_jam_head(0, &rpc, STILL_FOR, STALL_BUDGET).await?;
	// Counted rather than read off the chain's height on purpose: a builder that has re-rooted is
	// authoring siblings of the stuck block, so its blocks stop making the chain taller long
	// before they stop being authored, and a height that stands still would read as a dead
	// collator when it is a working one.
	let authored = authored(run).saturating_sub(authored_before);
	anyhow::ensure!(
		authored >= LOCAL_BLOCKS_WHILE_STALLED,
		"with its core gone the para authored only {authored} blocks while its head stood still \
		 for {STILL_FOR:?} ({freed} -> {frozen}); losing a core must not stop block production"
	);
	log::info!("the head froze at {frozen} while the collator authored {authored} more blocks");

	// Nothing in JAM state can show that the builder gave up on the branch above the frozen head
	// and started authoring siblings of it instead, so this one comes out of the collator's log.
	let rerooted = reroots(run).saturating_sub(reroots_before);
	anyhow::ensure!(
		rerooted > 0,
		"the head stood still at {frozen} for {STILL_FOR:?}, which is longer than the builder's \
		 stall threshold, but it never re-rooted onto the stuck head"
	);
	log::info!("the builder authored {rerooted} blocks re-rooted onto the stuck head");

	// The heal goes back to the *same* core, which is the point. Parking left core 0 running the
	// same authorizer code under a config naming no para, so it still takes a control package
	// even though it carries no parachain work — and parasim, holding its assigner privilege from
	// genesis, can act on one. No spare core is involved, so what this asserts is that losing a
	// core is recoverable on a network with nothing else to fall back on.
	run.network.assign_core(tool, &para, CORE, None)?;
	let frozen_at = frozen.jam_head.context("the head froze before anything accumulated")?;
	let healed = run.wait_for_jam_head(0, &rpc, frozen_at.number + 1, HEAL_BUDGET).await?;
	log::info!("para {PARA} healed on core {CORE}, the core it lost: {frozen_at} -> {healed}");
	Ok(())
}

/// Moving a para from one core to the other does not cost it a single block.
///
/// The move is two steps and the overlap between them is the whole point. Assigning the spare core
/// leaves the para's authorizer in *both* pools, and the collator's lowest-index policy keeps it
/// submitting to the core it is already on. Parking that core does not cut anything off either:
/// its pool drains over the following blocks and the packages already under it are still reported
/// and accumulated. So the head is never allowed to pause, which is what the walk below asserts —
/// one head at a time, each within a bounded wait, so that a stall anywhere across the handover
/// fails here instead of being averaged out by a generous overall budget.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "mid-run core control is out of scope: the target setup preconfigures cores in genesis"]
async fn moving_the_para_to_the_other_core_keeps_its_head_moving() -> Result<(), anyhow::Error> {
	const TEST: &str = "moving_the_para_to_the_other_core_keeps_its_head_moving";

	let Some(binaries) = setup(TEST) else { return Ok(()) };
	let Some(tool) = parasim_tool_or_skip(TEST, &binaries) else { return Ok(()) };

	let mut run = Run::start(TEST, &binaries, vec![Para::single(1)]).await?;
	run.extend_deadline(EXTRA_TIME);

	let result = move_to_the_other_core(&mut run, &tool).await;
	finish(run, result).await
}

async fn move_to_the_other_core(run: &mut Run, tool: &Path) -> anyhow::Result<()> {
	/// The longest a single head is allowed to take. A healthy para accumulates one every slot or
	/// two; this tolerates an order of magnitude worse and still fails a para that has stopped.
	const GAP_TOLERANCE: Duration = Duration::from_secs(2 * 60);
	/// Heads to walk while both cores hold the authorizer, and after the old one is taken away.
	/// The second is more than the eight blocks the old core's pool takes to drain, so it spans
	/// the handover and keeps going after it.
	const OVERLAP_HEADS: u64 = 3;
	const MOVED_HEADS: u64 = 12;

	let para = run.paras[0].para.clone();
	let rpc = first_rpc(run).await?;

	let before = run.wait_for_jam_head(0, &rpc, HEALTHY_HEAD, WARM_UP).await?;
	log::info!("para {PARA} is healthy on core {CORE}: {before}");

	// Before core 1 is assigned, not after: a bootstrap instruction only rides a core still under
	// the null authorizer, and once this run has assigned core 1 there is no such core left.
	run.network.host_authorizer_for_control_packages(tool)?;

	// Genesis named only core 0 in a single-para run, so core 1 still holds the null authorizer and
	// service 0 as its assigner: this rides the bootstrap lane on core 1 itself, and needs no
	// carrier.
	run.network.assign_core(tool, &para, SPARE_CORE, None)?;
	let overlapping = walk_heads(run, &rpc, OVERLAP_HEADS, GAP_TOLERANCE).await?;
	log::info!("both cores hold the authorizer and the head kept moving: {overlapping}");

	// Which core a package was submitted to exists only in the collator's log. These two fields
	// together appear in one line, the pool scan's: it saw the authorizer on both cores and chose
	// the lower-numbered one, which is the policy this overlap depends on.
	let stayed = run.paras[0].collators.log_lines_with(&["core=0", "also_on=[1]"]);
	anyhow::ensure!(
		!stayed.is_empty(),
		"core {SPARE_CORE} was assigned to para {PARA} as well, but the collator never recorded \
		 seeing both cores and staying on the lower-numbered one"
	);

	run.network.free_core(tool, &para, CORE)?;
	let moved = walk_heads(run, &rpc, MOVED_HEADS, GAP_TOLERANCE).await?;
	log::info!("core {CORE} is parked and the head kept moving: {moved}");

	// The same two fields as above, on the submission line this time (`package_len` is only on
	// that one): proof that packages are now going to the core the para was moved to.
	let submitted = run.paras[0].collators.log_lines_with(&["core=1", "package_len="]);
	anyhow::ensure!(
		!submitted.is_empty(),
		"para {PARA}'s head reached {moved} after the move, but the collator never submitted a \
		 work package to core {SPARE_CORE}"
	);
	log::info!("the collator submitted {} work packages to core {SPARE_CORE}", submitted.len());

	// Finality is the fork check. A handover that left the collator building on two parents at
	// once would show up as a best block that keeps climbing while finality falls behind.
	anyhow::ensure!(
		moved.height.finalized > before.height.finalized,
		"the para's head moved across the handover but its own chain finalized nothing new \
		 ({before} -> {moved})"
	);
	Ok(())
}

/// How many parachain blocks the single para's collator has authored so far.
///
/// `extrinsics` is a field only the "built and imported a block" line carries, which is what makes
/// this a count of blocks authored rather than of anything else the builder logged.
fn authored(run: &Run) -> usize {
	run.paras[0].collators.log_lines_with(&["extrinsics="]).len()
}

/// How many blocks the single para's collator has authored on a head it gave up waiting for.
///
/// `parent_source` is the field the builder records its choice of parent in: `Reroot` is the tick
/// that abandons the branch above a stuck head, and `Rerooted` every block authored after it.
fn reroots(run: &Run) -> usize {
	run.paras[0].collators.log_lines_with(&["parent_source=Reroot"]).len()
}

/// Wait for the next `count` accumulated heads, each within `tolerance` of the one before it.
async fn walk_heads(
	run: &mut Run,
	rpc: &CollatorRpc,
	count: u64,
	tolerance: Duration,
) -> anyhow::Result<super::harness::ParaProgress> {
	let mut progress = run.sample(0, rpc).await?;
	for step in 1..=count {
		let next = progress.jam_head.as_ref().map_or(1, |head| head.number + 1);
		log::info!("head {step} of {count} after the change: waiting for #{next}");
		progress = run.wait_for_jam_head(0, rpc, next, tolerance).await?;
	}
	Ok(progress)
}

/// The RPC of the single para's first collator, which is the one every assertion reads.
async fn first_rpc(run: &Run) -> anyhow::Result<CollatorRpc> {
	run.rpcs().await?.into_iter().next().context("the run started no paras")
}
