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

//! Parachain blocks whose work packages never reach JAM must still land, through new packages a
//! collator builds and signs itself.
//!
//! Both tests pin the JAM ordinary node's RPC port, pre-allocate a second port for
//! [`ProxyServer`], and start the proxy before the network so a collator's `--jam-rpc-urls` can
//! name it before `spawn_fn` builds the whole network in one call. The proxy swallows
//! `submitWorkPackage` calls as its [`DropPolicy`] decides and forwards the rest byte-exact; its
//! ledger records which parachain blocks every dropped and every forwarded package carried.
//!
//! A lost block is recovered by whichever collator owns the current round-robin turn: it puts
//! the same block into a fresh work package, one block per package, signed by itself and carrying
//! a chain of JAM headers that proves the anchors the block was built against. Nothing is resent
//! byte-identically and no collator asks another for its signature. To the proxy a resubmission
//! is therefore a *forwarded* package carrying a block that an earlier, *dropped* package carried,
//! and it announces each one through [`ProxyServer::subscribe_resubmissions`].
//!
//! [`resends_make_the_head_advance`] drops a seeded random share of one collator's submissions
//! and only ends once [`MIN_RESUBMISSIONS`] resubmissions were observed and the blocks they
//! carried are in the collator's **finalized** chain. A parachain block only finalizes after JAM
//! accumulated the work package that carried it, so a finalized resubmitted block is the proof
//! that the new package reached JAM and was accepted; a log line cannot prove that, the node
//! emits it on its own schedule.
//!
//! [`another_collator_resends_a_lost_package`] has two collators authoring, but only alice behind
//! a proxy running [`DropPolicy::Everything`]: she builds and gossips her blocks, so bob imports
//! them, but none of her packages is ever delivered. There is no package sync between collators,
//! so only bob's own new packages, signed by bob and carrying a JAM header chain proving the old
//! anchors, can land her blocks. Its completion is bob's finalized height while the proxy
//! forwarded nothing, plus a finalized block authored by alice.

use anyhow::{anyhow, Context};
use codec::Decode;
use cumulus_jam_zombienet_tests::{
	network::free_port,
	para::{Para, TINY_CORES},
	proxy::{DropPolicy, ProxyServer, Resubmission},
	spawn::{spawn, CollatorOptions, JamNetwork, SpawnOptions},
};
use cumulus_zombienet_sdk_helpers::{
	jam::{DigestItem, JamHeader},
	ParaConfig,
};
use std::{collections::HashMap, env::VarError, path::PathBuf, time::Duration};
use tokio::time::Instant;
use zombienet_sdk::subxt::{blocks::Block, utils::H256, OnlineClient};

/// A parachain block as a collator's subxt client hands it out.
type ParaBlock = Block<ParaConfig, OnlineClient<ParaConfig>>;

/// The share of alice's submissions [`resends_make_the_head_advance`] drops.
const DROP_PROBABILITY: f64 = 0.3;
/// Names the seed of the random drops so a run can be replayed; unset draws a fresh one.
const DROP_SEED_ENV: &str = "JAM_TEST_DROP_SEED";
/// How many resubmissions [`resends_make_the_head_advance`] has to observe, and how many of the
/// blocks they carried have to end up finalized.
const MIN_RESUBMISSIONS: usize = 10;
/// How long observing [`MIN_RESUBMISSIONS`] resubmissions and finalizing their blocks may take.
/// Loose, but a collator that never rebuilds a lost block has to time out here rather than pass.
const RESUBMISSION_BUDGET: Duration = Duration::from_secs(20 * 60);

/// The finalized parachain height bob has to reach in [`another_collator_resends_a_lost_package`].
/// Alice authors every other block and none of her packages arrives, so bob's chain cannot get
/// this far unless he lands her blocks himself.
const HEAD_TARGET: u64 = 5;
/// Loose, but a collator that never rebuilds another collator's lost blocks has to time out here
/// rather than pass.
const HEAD_BUDGET: Duration = Duration::from_secs(10 * 60);

/// One collator behind a proxy that drops a random share of its submissions. The blocks a dropped
/// package carried only land once the collator rebuilds them into new packages, and the run only
/// ends after [`MIN_RESUBMISSIONS`] of them are finalized.
#[tokio::test(flavor = "multi_thread")]
async fn resends_make_the_head_advance() -> Result<(), anyhow::Error> {
	const TEST: &str = "resends_make_the_head_advance";
	init_logging();
	let seed = drop_seed()?;
	log::info!(
		"{TEST}: dropping submissions with probability {DROP_PROBABILITY}, seed {seed}; replay \
		 with {DROP_SEED_ENV}={seed}"
	);
	let (jam, proxy) = spawn_proxied_network(
		TEST,
		&["alice"],
		&["alice"],
		DropPolicy::Random { probability: DROP_PROBABILITY, seed },
		None,
	)
	.await?;
	let result = resubmissions_then_finalize(&jam, &proxy).await;
	let result = result.map_err(|error| anyhow!("{error}\n\n{}", proxy.describe()));
	proxy.shutdown().await;
	jam.finish(result).await
}

/// Two collators authoring; only alice is behind a proxy that drops **every** submission it sees,
/// while bob talks to JAM directly. Alice's blocks reach bob by gossip but never reach JAM, so the
/// head can only advance if bob rebuilds them into packages of his own.
#[tokio::test(flavor = "multi_thread")]
async fn another_collator_resends_a_lost_package() -> Result<(), anyhow::Error> {
	const TEST: &str = "another_collator_resends_a_lost_package";
	let (jam, proxy) =
		spawn_proxied_network(TEST, &["alice", "bob"], &["alice"], DropPolicy::Everything, None)
			.await?;
	let result = foreign_resend_then_advance(&jam, &proxy).await;
	let result = result.map_err(|error| anyhow!("{error}\n\n{}", proxy.describe()));
	proxy.shutdown().await;
	jam.finish(result).await
}

/// Route `log` output to stderr at `info` unless `RUST_LOG` says otherwise; a repeated call is a
/// no-op.
fn init_logging() {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
}

/// The seed of [`DropPolicy::Random`]: `JAM_TEST_DROP_SEED` if set, else a fresh random one.
fn drop_seed() -> anyhow::Result<u64> {
	match std::env::var(DROP_SEED_ENV) {
		Ok(seed) => seed
			.parse()
			.with_context(|| format!("parsing {DROP_SEED_ENV}={seed:?} as a u64")),
		Err(VarError::NotPresent) => Ok(rand::random()),
		Err(error) => Err(error).with_context(|| format!("reading {DROP_SEED_ENV}")),
	}
}

/// Spawn the tiny JAM network with `collators` authoring para 0, the ones named in `proxied`
/// pointing their `--jam-rpc-urls` at a proxy applying `policy`; the others talk to JAM directly.
///
/// `runtime` is the PolkaVM blob para 0 validates with *and* its collators execute; `None` keeps
/// the run's default ([`Binaries::runtime_wasm`](cumulus_jam_zombienet_tests::env::Binaries)).
async fn spawn_proxied_network(
	test: &str,
	collators: &[&str],
	proxied: &[&str],
	policy: DropPolicy,
	runtime: Option<PathBuf>,
) -> anyhow::Result<(JamNetwork, ProxyServer)> {
	init_logging();

	let jam_or_port = free_port()?;
	let proxy_port = free_port()?;

	let mut para = Para::new(0, 0, collators);
	if let Some(runtime) = runtime {
		para = para.with_runtime(runtime);
	}
	let paras = vec![para];

	// Pin each collator's p2p port and announce it as a public address, so collators sharing
	// this host find each other over a known loopback socket: a lost block can only be rebuilt
	// by a collator that imported it.
	let collator_options: HashMap<String, CollatorOptions> = collators
		.iter()
		.map(|name| -> anyhow::Result<(String, CollatorOptions)> {
			Ok((
				(*name).to_string(),
				CollatorOptions {
					jam_rpc_url: proxied
						.contains(name)
						.then(|| format!("ws://127.0.0.1:{proxy_port}")),
					p2p_port: Some(free_port()?),
					args: Vec::new(),
				},
			))
		})
		.collect::<anyhow::Result<HashMap<_, _>>>()?;
	let options = SpawnOptions {
		ordinary_rpc_port: Some(jam_or_port),
		collators: collator_options,
		ready_timeout: Duration::from_secs(120),
		..SpawnOptions::new(TINY_CORES)
	};

	// The proxy has to be serving before the network comes up: the proxied collators'
	// `--jam-rpc-urls` already name it, and their `jam-init` retries for ~15s, so it only has to
	// be there by then.
	let proxy_task = tokio::spawn(async move {
		ProxyServer::serve(
			policy,
			&format!("ws://127.0.0.1:{jam_or_port}"),
			&format!("127.0.0.1:{proxy_port}"),
			Duration::from_secs(90),
		)
		.await
	});

	let jam = spawn(test, &paras, options).await?;
	let proxy = proxy_task.await??;

	Ok((jam, proxy))
}

/// Wait until `client`'s collator has **finalized** block number `target` and return the first
/// finalized block at or above it.
///
/// A parachain block only finalizes after JAM has accumulated and reported the work package that
/// carried it, so a finalized height is the objective proof that a package reached JAM. A log
/// line cannot prove that: the node emits it on its own schedule, independent of whether the
/// package ever reached a guarantor. The error names the last finalized height, so a stuck run
/// says whether the chain stalled or merely fell behind.
async fn wait_for_finalized_height(
	client: &OnlineClient<ParaConfig>,
	target: u64,
	budget: Duration,
) -> anyhow::Result<ParaBlock> {
	let deadline = Instant::now() + budget;
	let mut finalized = client.blocks().subscribe_finalized().await?;
	let mut last = 0u64;
	loop {
		let remaining = deadline.saturating_duration_since(Instant::now());
		let block = tokio::time::timeout(remaining, finalized.next())
			.await
			.map_err(|_| {
				anyhow!(
					"the collator did not finalize block #{target} within {budget:?}; the last \
					 finalized height was {last}"
				)
			})?
			.ok_or_else(|| anyhow!("the collator's finalized-block stream ended"))??;
		last = u64::from(block.number());
		log::info!("collator finalized height: {last} (target #{target})");
		if last >= target {
			return Ok(block);
		}
	}
}

/// Wait until the proxy saw [`MIN_RESUBMISSIONS`] resubmissions, then until alice finalized the
/// highest block they carried, and require at least [`MIN_RESUBMISSIONS`] of the resubmitted
/// blocks in her finalized chain: a new package the proxy forwarded but JAM never accumulated
/// leaves its block unfinalized and fails here.
async fn resubmissions_then_finalize(jam: &JamNetwork, proxy: &ProxyServer) -> anyhow::Result<()> {
	let deadline = Instant::now() + RESUBMISSION_BUDGET;
	let alice_client = jam.collator_client("alice").await?;

	let resubmitted = await_resubmitted_blocks(proxy, deadline).await?;
	let (Some(low), Some(high)) =
		(resubmitted.values().min().copied(), resubmitted.values().max().copied())
	else {
		anyhow::bail!("the proxy announced resubmissions that carried no block");
	};
	log::info!(
		"the resubmitted blocks span #{low}..=#{high}; waiting for alice to finalize #{high}"
	);
	let head = wait_for_finalized_height(
		&alice_client,
		u64::from(high),
		deadline.saturating_duration_since(Instant::now()),
	)
	.await?;

	let landed = finalized_among(&alice_client, &head, &resubmitted, low).await?;
	let mut missing: Vec<(u32, H256)> = resubmitted
		.iter()
		.filter(|(hash, _)| !landed.contains(hash))
		.map(|(hash, number)| (*number, *hash))
		.collect();
	missing.sort();
	anyhow::ensure!(
		landed.len() >= MIN_RESUBMISSIONS,
		"only {} of the {} resubmitted block(s) landed in alice's finalized chain between #{low} \
		 and #{}; {MIN_RESUBMISSIONS} are required. Not in it: {missing:?}",
		landed.len(),
		resubmitted.len(),
		head.number(),
	);

	anyhow::ensure!(
		proxy.upstream_errors() == 0,
		"the proxy's upstream rejected {} forwarded submission(s)",
		proxy.upstream_errors()
	);

	Ok(())
}

/// Wait until the proxy's resubmissions carry at least [`MIN_RESUBMISSIONS`] distinct blocks and
/// return those blocks as hash to number.
///
/// The proxy announces every resubmission over a `watch` channel, so this awaits notifications
/// rather than polling. A block resubmitted twice is one block, so if the resubmissions seen so
/// far repeat one, the wait goes on until the next announcement.
async fn await_resubmitted_blocks(
	proxy: &ProxyServer,
	deadline: Instant,
) -> anyhow::Result<HashMap<H256, u32>> {
	let mut announced = proxy.subscribe_resubmissions();
	let mut needed = MIN_RESUBMISSIONS;
	loop {
		let count = tokio::time::timeout_at(deadline, announced.wait_for(|count| *count >= needed))
			.await
			.map_err(|_| {
				let seen = proxy.resubmissions();
				anyhow!(
					"within {RESUBMISSION_BUDGET:?} the proxy saw {} resubmission(s) carrying {} \
					 distinct block(s); {MIN_RESUBMISSIONS} distinct are required",
					seen.len(),
					resubmitted_blocks(&seen).len(),
				)
			})?
			.map(|count| *count)
			.map_err(|_| anyhow!("the proxy stopped counting resubmissions"))?;
		let blocks = resubmitted_blocks(&proxy.resubmissions());
		log::info!("{count} resubmission(s) carried {} distinct block(s)", blocks.len());
		if blocks.len() >= MIN_RESUBMISSIONS {
			return Ok(blocks);
		}
		needed = count + 1;
	}
}

/// The distinct blocks `resubmissions` carried, as hash to number.
fn resubmitted_blocks(resubmissions: &[Resubmission]) -> HashMap<H256, u32> {
	resubmissions
		.iter()
		.flat_map(|resubmission| resubmission.blocks.iter())
		.map(|(hash, number)| (H256::from(*hash), *number))
		.collect()
}

/// Walk `client`'s finalized chain back from `head` to block number `low` and return which of
/// `blocks` (hash to number) are in it.
async fn finalized_among(
	client: &OnlineClient<ParaConfig>,
	head: &ParaBlock,
	blocks: &HashMap<H256, u32>,
	low: u32,
) -> anyhow::Result<Vec<H256>> {
	let mut landed = Vec::new();
	let mut hash = head.hash();
	let mut header = head.header().clone();
	while header.number >= low {
		if blocks.contains_key(&hash) {
			landed.push(hash);
		}
		hash = header.parent_hash;
		header = client.blocks().at(hash).await?.header().clone();
	}
	Ok(landed)
}

/// Wait for bob's finalized parachain height while alice's submissions all stay dropped: only
/// bob's own new packages can land her blocks, and the proxy ledger proves she never delivered.
async fn foreign_resend_then_advance(jam: &JamNetwork, proxy: &ProxyServer) -> anyhow::Result<()> {
	let bob_client = jam.collator_client("bob").await?;
	wait_for_finalized_height(&bob_client, HEAD_TARGET, HEAD_BUDGET).await?;

	anyhow::ensure!(
		proxy.forwarded_count() == 0,
		"the proxy forwarded {} submission(s) although its policy drops everything",
		proxy.forwarded_count()
	);

	let attempts = proxy.snapshot();
	anyhow::ensure!(
		!attempts.is_empty(),
		"bob finalized block #{HEAD_TARGET}, but the proxy saw no submitWorkPackage at all: alice \
		 is talking to the network directly, not through the proxy"
	);

	anyhow::ensure!(
		proxy.upstream_errors() == 0,
		"the proxy's upstream rejected {} forwarded submission(s)",
		proxy.upstream_errors()
	);

	// Two collators author this chain; both must appear among the last finalized blocks.
	assert_foreign_author_finalized(&bob_client, 2, HEAD_TARGET + 2).await?;

	Ok(())
}

/// The Aura author index of `header`: the pre-runtime digest's slot modulo the authority count.
///
/// The runtime hands its authorities back in the chain spec's listed order, so `slot % count` is
/// the collator whose turn the slot was.
fn aura_author_index(header: &JamHeader, authorities: u64) -> anyhow::Result<u64> {
	let data = header
		.digest
		.logs
		.iter()
		.find_map(|item| match item {
			DigestItem::PreRuntime(engine, data) if engine == b"aura" => Some(data.clone()),
			_ => None,
		})
		.ok_or_else(|| anyhow!("the header carries no Aura pre-runtime digest"))?;
	let slot = u64::decode(&mut &data[..]).context("decoding the Aura slot")?;
	Ok(slot % authorities)
}

/// Walk back from `client`'s finalized head and require at least two distinct block authors.
///
/// In this two-collator test alice's submissions all stay dropped, so an alice-authored finalized
/// block can only be there because bob rebuilt it into a package of his own; bob's own blocks are
/// all his. Two distinct authors therefore prove a foreign resubmission happened, with no log line.
async fn assert_foreign_author_finalized(
	client: &OnlineClient<ParaConfig>,
	authorities: u64,
	depth: u64,
) -> anyhow::Result<()> {
	let mut finalized = client.blocks().subscribe_finalized().await?;
	let block = finalized
		.next()
		.await
		.ok_or_else(|| anyhow!("the collator's finalized-block stream ended"))??;
	let mut header = block.header().clone();
	let mut authors = std::collections::BTreeSet::new();
	for _ in 0..depth {
		// The walk can reach genesis, which has no digest; stop before decoding it.
		if header.number == 0 {
			break;
		}
		authors.insert(aura_author_index(&header, authorities)?);
		header = client.blocks().at(header.parent_hash).await?.header().clone();
	}
	anyhow::ensure!(
		authors.len() as u64 >= authorities,
		"the last {depth} finalized block(s) were all authored by one collator (authors \
		 {authors:?}); the head advanced without a foreign resubmission"
	);
	Ok(())
}
