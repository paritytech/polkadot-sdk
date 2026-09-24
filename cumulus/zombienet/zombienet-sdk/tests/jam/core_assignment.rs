// This file is part of Cumulus.

// Copyright (C) Parity Technologies (UK) Ltd.
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

#![cfg(feature = "jam")]
// allow: SIZE_OK — one cohesive test module: the three core-assignment tests plus the native
// orchestration helpers they share. The old `jam-tests` harness kept that orchestration in a
// library; the native harness deliberately keeps none (see `tests/jam/mod.rs`), so it lives here.

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

use crate::jam::{jam_rpc_url, para, setup_with_cores, JamSetup, Para};
use anyhow::{anyhow, Context};
use cumulus_jam_zombienet_tests::{
	control,
	env::binaries_or_err,
	para::{DEADLINE, PARACHAIN_SERVICE_ID, TINY_CORES},
	para_head::{read_para_head, ParaHead},
	rpc::{CollatorRpc, Height, JamRpc},
};
use std::{path::Path, time::Duration};
use tokio::time::Instant;
use zombienet_sdk::{LocalFileSystem, Network};

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

/// Long enough that a stall lasting a few slots cannot be mistaken for progress; the figure
/// the single-para progress tests use, so the two are comparable.
const BLOCKS: u64 = 30;
const FINALIZED: u64 = 25;

/// The longest a single head is allowed to take. A healthy para accumulates one every slot or
/// two; this tolerates an order of magnitude worse and still fails a para that has stopped.
const GAP_TOLERANCE: Duration = Duration::from_secs(2 * 60);

/// The best-block metric every zombienet node reports.
const PARA_BLOCK_METRIC: &str = "block_height{status=\"best\"}";

/// The finalized-block metric every zombienet node reports.
const PARA_FINALIZED_METRIC: &str = "block_height{status=\"finalized\"}";

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
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let paras = vec![para(0, 0, &["alice", "bob"]), para(1, 1, &["charlie", "dave"])];
	let jam = setup_with_cores(TEST, &paras, TINY_CORES)?;
	let network = spawn_network(&jam, paras.len()).await?;

	let result = async {
		let jam_url = jam_rpc_url(&network)?;
		let jam_rpc = JamRpc::wait_ready(&jam_url, Instant::now() + DEADLINE).await?;
		wait_for_every_collator(&network, &paras).await?;
		heads_belong_to_their_own_para(&network, &jam_rpc, &paras).await
	}
	.await;

	destroy(network).await;
	result
}

/// Assert each para's accumulated head is a block of that para's chain, and of no other.
///
/// Two parachains have disjoint block hashes, so the collator that has never heard of a hash is
/// the proof: para 0's collator knowing a head parasim filed under para 1 would mean the two
/// chains had converged, and para 1's collator knowing one filed under para 0 would mean parasim
/// had mixed their state up.
async fn heads_belong_to_their_own_para(
	network: &Network<LocalFileSystem>,
	jam_rpc: &JamRpc,
	paras: &[Para],
) -> anyhow::Result<()> {
	let ids: Vec<u32> = paras.iter().map(|para| para.id).collect();
	let mut rpcs = Vec::with_capacity(paras.len());
	for para in paras {
		rpcs.push(first_rpc(network, para).await?);
	}

	let mut heads = Vec::new();
	for (index, rpc) in rpcs.iter().enumerate() {
		let progress = read_progress(jam_rpc, rpc, ids[index]).await?;
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
/// core is gone. The package sent just before the core was parked is not reported, so the
/// collator resends it at anchor+2 and holds the parachain slot — authoring nothing — until its
/// anchor expires at anchor+8, about 48 s later. Only then does authoring resume, on blocks whose
/// `submit_target` is `None`: never sent, so never held. The only visible consequence of the lost
/// core is a head that has stopped. What must *not* happen is the collator stopping too, so the
/// assertion is deliberately two-sided: the JAM head stands still and the local chain does not.
#[tokio::test(flavor = "multi_thread")]
async fn freeing_the_core_freezes_the_para_head_until_it_is_assigned_again(
) -> Result<(), anyhow::Error> {
	const TEST: &str = "freeing_the_core_freezes_the_para_head_until_it_is_assigned_again";
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let binaries = binaries_or_err()?;
	// Before the network is spawned: whether the tool is there decides whether this test can run
	// at all, and eight minutes of warm-up is a long way to go to find that out.
	let tool = binaries
		.parasim_tool
		.clone()
		.context("PARASIM_TOOL_BIN is required for the dynamic-core tests")?;

	let paras = vec![Para::single(1)];
	let jam = setup_with_cores(TEST, &paras, TINY_CORES)?;
	let network = spawn_network(&jam, paras.len()).await?;

	let result = async {
		let jam_url = jam_rpc_url(&network)?;
		let jam_rpc = JamRpc::wait_ready(&jam_url, Instant::now() + DEADLINE).await?;
		let run = Run {
			network: &network,
			jam_rpc: &jam_rpc,
			jam_url: &jam_url,
			tool: &tool,
			authorizer_blob: &binaries.authorizer_blob,
			para: &paras[0],
		};
		stall_then_heal(&run).await
	}
	.await;

	destroy(network).await;
	result
}

async fn stall_then_heal(run: &Run<'_>) -> anyhow::Result<()> {
	/// How long the head has to stand still before the stall is called. The package the para sent
	/// just before its core was parked holds authoring for its eight-slot report window (~48 s)
	/// before the builder gives up and re-roots, so this is comfortably more than that: a run that
	/// reaches this point has been through the whole hold-and-resend sequence, not just a slow
	/// block.
	const STILL_FOR: Duration = Duration::from_secs(90);
	const STALL_BUDGET: Duration = Duration::from_secs(10 * 60);
	/// Blocks the collator has to author while the head is frozen. The parked package holds
	/// authoring for its eight-slot report window (~48 s), and only after that can the builder
	/// re-root and author again, so a 90 s stall is no longer the ten blocks the old soft-resubmit
	/// behaviour produced: expect roughly half. Three is below even that, and still fails a
	/// collator that stopped, which would produce none.
	const LOCAL_BLOCKS_WHILE_STALLED: usize = 3;

	let rpc = run.rpc().await?;

	run.wait_for_jam_head(HEALTHY_HEAD, WARM_UP).await?;
	let healthy = run.sample(&rpc).await?;
	log::info!("para {PARA} is healthy on core {CORE}: {healthy}");

	run.host_authorizer()?;
	run.free_core(CORE)?;
	let freed = run.sample(&rpc).await?;
	// Counted from here, not from the start of the run: a slow patch early on can stall the head
	// for eight slots too, and a re-root that happened before the core was taken away would say
	// nothing about what taking it away does.
	let reroots_before = reroots(run).await?;
	let authored_before = authored(run).await?;
	log::info!("core {CORE} is parked; para {PARA} was at {freed} when it went");

	run.wait_for_frozen_jam_head(STILL_FOR, STALL_BUDGET).await?;
	let frozen = run.sample(&rpc).await?;
	// Counted rather than read off the chain's height on purpose: a builder that has re-rooted is
	// authoring siblings of the stuck block, so its blocks stop making the chain taller long
	// before they stop being authored, and a height that stands still would read as a dead
	// collator when it is a working one.
	let authored = authored(run).await?.saturating_sub(authored_before);
	anyhow::ensure!(
		authored >= LOCAL_BLOCKS_WHILE_STALLED,
		"with its core gone the para authored only {authored} blocks while its head stood still \
		 for {STILL_FOR:?} ({freed} -> {frozen}); losing a core must not stop block production"
	);
	log::info!("the head froze at {frozen} while the collator authored {authored} more blocks");

	// Nothing in JAM state can show that the builder gave up on the branch above the frozen head
	// and started authoring siblings of it instead, so this one comes out of the collator's log.
	let rerooted = reroots(run).await?.saturating_sub(reroots_before);
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
	run.assign_core(CORE)?;
	let frozen_at = frozen.jam_head.context("the head froze before anything accumulated")?;
	run.wait_for_jam_head(frozen_at.number + 1, HEAL_BUDGET).await?;
	let healed = run.sample(&rpc).await?;
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
async fn moving_the_para_to_the_other_core_keeps_its_head_moving() -> Result<(), anyhow::Error> {
	const TEST: &str = "moving_the_para_to_the_other_core_keeps_its_head_moving";
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let binaries = binaries_or_err()?;
	let tool = binaries
		.parasim_tool
		.clone()
		.context("PARASIM_TOOL_BIN is required for the dynamic-core tests")?;

	let paras = vec![Para::single(1)];
	let jam = setup_with_cores(TEST, &paras, TINY_CORES)?;
	let network = spawn_network(&jam, paras.len()).await?;

	let result = async {
		let jam_url = jam_rpc_url(&network)?;
		let jam_rpc = JamRpc::wait_ready(&jam_url, Instant::now() + DEADLINE).await?;
		let run = Run {
			network: &network,
			jam_rpc: &jam_rpc,
			jam_url: &jam_url,
			tool: &tool,
			authorizer_blob: &binaries.authorizer_blob,
			para: &paras[0],
		};
		move_to_the_other_core(&run).await
	}
	.await;

	destroy(network).await;
	result
}

async fn move_to_the_other_core(run: &Run<'_>) -> anyhow::Result<()> {
	/// Heads to walk while both cores hold the authorizer, and after the old one is taken away.
	/// The second is more than the eight blocks the old core's pool takes to drain, so it spans
	/// the handover and keeps going after it.
	const OVERLAP_HEADS: u64 = 3;
	const MOVED_HEADS: u64 = 12;

	let rpc = run.rpc().await?;

	run.wait_for_jam_head(HEALTHY_HEAD, WARM_UP).await?;
	let before = run.sample(&rpc).await?;
	log::info!("para {PARA} is healthy on core {CORE}: {before}");

	// Before core 1 is assigned, not after: a bootstrap instruction only rides a core still under
	// the null authorizer, and once this run has assigned core 1 there is no such core left.
	run.host_authorizer()?;

	// Genesis named only core 0 in a single-para run, so core 1 still holds the null authorizer and
	// service 0 as its assigner: this rides the bootstrap lane on core 1 itself, and needs no
	// carrier.
	run.assign_core(SPARE_CORE)?;
	let overlapping = walk_heads(run, &rpc, OVERLAP_HEADS).await?;
	log::info!("both cores hold the authorizer and the head kept moving: {overlapping}");

	// Which core a package was submitted to exists only in the collator's log. These two fields
	// together appear in one line, the pool scan's: it saw the authorizer on both cores and chose
	// the lower-numbered one, which is the policy this overlap depends on.
	let stayed = run.lines_with(&["core=0", "also_on=[1]"]).await?;
	anyhow::ensure!(
		!stayed.is_empty(),
		"core {SPARE_CORE} was assigned to para {PARA} as well, but the collator never recorded \
		 seeing both cores and staying on the lower-numbered one"
	);

	run.free_core(CORE)?;
	let moved = walk_heads(run, &rpc, MOVED_HEADS).await?;
	log::info!("core {CORE} is parked and the head kept moving: {moved}");

	// The same two fields as above, on the submission line this time (`package_len` is only on
	// that one): proof that packages are now going to the core the para was moved to.
	let submitted = run.lines_with(&["core=1", "package_len="]).await?;
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
async fn authored(run: &Run<'_>) -> anyhow::Result<usize> {
	Ok(run.lines_with(&["extrinsics="]).await?.len())
}

/// How many blocks the single para's collator has authored on a head it gave up waiting for.
///
/// `parent_source` is the field the builder records its choice of parent in: `Reroot` is the tick
/// that abandons the branch above a stuck head, and `Rerooted` every block authored after it.
async fn reroots(run: &Run<'_>) -> anyhow::Result<usize> {
	Ok(run.lines_with(&["parent_source=Reroot"]).await?.len())
}

/// Wait for the next `count` accumulated heads, each within [`GAP_TOLERANCE`] of the one before.
async fn walk_heads(run: &Run<'_>, rpc: &CollatorRpc, count: u64) -> anyhow::Result<Progress> {
	let mut progress = run.sample(rpc).await?;
	for step in 1..=count {
		let next = progress.jam_head.as_ref().map_or(1, |head| head.number + 1);
		log::info!("head {step} of {count} after the change: waiting for #{next}");
		run.wait_for_jam_head(next, GAP_TOLERANCE).await?;
		progress = run.sample(rpc).await?;
	}
	Ok(progress)
}

/// The RPC of the para's first collator, which is the one every assertion reads.
async fn first_rpc(network: &Network<LocalFileSystem>, para: &Para) -> anyhow::Result<CollatorRpc> {
	let name = para.collators.first().context("the para has no collator")?;
	let url = network.get_node(name.as_str())?.ws_uri();
	CollatorRpc::connect(&url, Instant::now() + DEADLINE).await
}

/// One reading of a para: where its own chain is, and where JAM thinks it is.
///
/// The two move independently, and every phase-6 assertion is about how: authoring is local and
/// carries on regardless, while the accumulated head only moves when a work package made it all
/// the way through JAM.
struct Progress {
	height: Height,
	jam_head: Option<ParaHead>,
}

impl std::fmt::Display for Progress {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			formatter,
			"best {} finalized {}, JAM head {}",
			self.height.best,
			self.height.finalized,
			match &self.jam_head {
				Some(head) => head.to_string(),
				None => "none".to_string(),
			}
		)
	}
}

/// Read the para's own height and the head JAM holds for it, in one reading.
async fn read_progress(jam_rpc: &JamRpc, rpc: &CollatorRpc, para: u32) -> anyhow::Result<Progress> {
	Ok(Progress {
		height: rpc.height().await?,
		jam_head: read_para_head(jam_rpc, PARACHAIN_SERVICE_ID, para).await?,
	})
}

/// The lines of collator `name`'s log that carry every one of `needles`.
async fn log_lines_with(
	network: &Network<LocalFileSystem>,
	name: &str,
	needles: &[&str],
) -> anyhow::Result<Vec<String>> {
	let log = network.get_node(name)?.logs().await?;
	Ok(log
		.lines()
		.filter(|line| needles.iter().all(|needle| line.contains(needle)))
		.map(str::to_string)
		.collect())
}

/// The running network plus the handles the dynamic-core tests change it with.
///
/// This mirrors what the old `jam-tests` harness kept in its `Run`: the spawned network, its JAM
/// RPC, the URL and authorizer blob the control tool is pointed at, and the single para of a
/// single-para run.
struct Run<'a> {
	network: &'a Network<LocalFileSystem>,
	jam_rpc: &'a JamRpc,
	jam_url: &'a str,
	tool: &'a Path,
	authorizer_blob: &'a Path,
	para: &'a Para,
}

impl Run<'_> {
	/// The RPC of the para's first collator.
	async fn rpc(&self) -> anyhow::Result<CollatorRpc> {
		first_rpc(self.network, self.para).await
	}

	/// One reading of the para.
	async fn sample(&self, rpc: &CollatorRpc) -> anyhow::Result<Progress> {
		read_progress(self.jam_rpc, rpc, self.para.id).await
	}

	/// Wait until JAM has accumulated a head of at least `target` for the para.
	async fn wait_for_jam_head(&self, target: u64, budget: Duration) -> anyhow::Result<()> {
		crate::jam::wait_for_jam_head(
			self.jam_rpc,
			PARACHAIN_SERVICE_ID,
			self.para.id,
			target,
			budget,
		)
		.await
	}

	/// Wait until JAM's head for the para has stood still for `still_for`.
	async fn wait_for_frozen_jam_head(
		&self,
		still_for: Duration,
		budget: Duration,
	) -> anyhow::Result<()> {
		crate::jam::wait_for_frozen_jam_head(
			self.jam_rpc,
			PARACHAIN_SERVICE_ID,
			self.para.id,
			still_for,
			budget,
		)
		.await
	}

	/// Host the authorizer blob in the bootstrap service, for `parasim-tool`'s control packages.
	fn host_authorizer(&self) -> anyhow::Result<()> {
		control::host_authorizer_for_control_packages(
			self.jam_url,
			PARACHAIN_SERVICE_ID,
			self.authorizer_blob,
			self.tool,
		)
	}

	/// Point `core` at this para through `parasim-tool`.
	fn assign_core(&self, core: u32) -> anyhow::Result<()> {
		control::assign_core(
			self.jam_url,
			PARACHAIN_SERVICE_ID,
			self.authorizer_blob,
			self.tool,
			self.para,
			core,
			None,
		)
	}

	/// Park `core` through `parasim-tool`.
	fn free_core(&self, core: u32) -> anyhow::Result<()> {
		control::free_core(
			self.jam_url,
			PARACHAIN_SERVICE_ID,
			self.authorizer_blob,
			self.tool,
			self.para,
			core,
		)
	}

	/// The para's first collator's log lines carrying every `needle`.
	async fn lines_with(&self, needles: &[&str]) -> anyhow::Result<Vec<String>> {
		log_lines_with(self.network, &self.para.collators[0], needles).await
	}
}

/// Wait, per collator, for the best metric to pass [`BLOCKS`] and then the finalized metric to
/// pass [`FINALIZED`]. Every collator is waited on, because a set where only one collator authors
/// still has to keep the chain producing and finalizing.
async fn wait_for_every_collator(
	network: &Network<LocalFileSystem>,
	paras: &[Para],
) -> anyhow::Result<()> {
	let timeout = DEADLINE.as_secs();
	for para in paras {
		for name in &para.collators {
			let node = network.get_node(name.as_str())?;

			log::info!("Waiting for collator {name} to reach best block #{BLOCKS}");
			node.wait_metric_with_timeout(PARA_BLOCK_METRIC, |best| best >= BLOCKS as f64, timeout)
				.await
				.map_err(|error| {
					anyhow!(
						"collator {name} did not reach best block #{BLOCKS} in {timeout}s: {error}"
					)
				})?;

			log::info!("Waiting for collator {name} to finalize block #{FINALIZED}");
			node.wait_metric_with_timeout(
				PARA_FINALIZED_METRIC,
				|finalized| finalized >= FINALIZED as f64,
				timeout,
			)
			.await
			.map_err(|error| {
				anyhow!(
					"collator {name} did not finalize block #{FINALIZED} in {timeout}s: {error}"
				)
			})?;
		}
	}
	Ok(())
}

/// Build the run's zombienet config — a JAM chain plus the first `parachains` paras — and spawn
/// it. The JAM genesis itself was already generated by [`setup_with_cores`]; this only turns it
/// into a running network.
async fn spawn_network(
	jam: &JamSetup,
	parachains: usize,
) -> anyhow::Result<Network<LocalFileSystem>> {
	let mut config = jam.jamchain();
	for index in 0..parachains {
		config = config.with_parachain(|p| jam.parachain(p, index));
	}
	let config = config
		.with_global_settings(|g| g.with_base_dir(jam.base_dir()))
		.build()
		.map_err(|errors| {
			anyhow!(
				"config errs: {}",
				errors.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ")
			)
		})?;
	Ok(zombienet_sdk::environment::get_spawn_fn()(config).await?)
}

/// Tear the network down, logging rather than propagating a teardown failure: the assertions have
/// already run, and a dropped network cleans up after a panic anyway.
async fn destroy(network: Network<LocalFileSystem>) {
	if let Err(error) = network.destroy().await {
		log::warn!("tearing down the JAM network failed: {error}");
	}
}
