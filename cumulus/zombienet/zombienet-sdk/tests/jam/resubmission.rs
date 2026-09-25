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

//! A collator whose first work-package submission never reaches JAM must resend the identical
//! package and still move the head.
//!
//! The test pins the JAM ordinary node's RPC port, pre-allocates a second port for
//! [`ProxyServer`], and starts the proxy before the network so `alice`'s `--jam-rpc-urls` can name
//! it before `spawn_fn` builds the whole network in one call. The proxy swallows the first
//! `submitWorkPackage` of every distinct work-package hash and forwards every later one
//! byte-exact. If the collator gives up instead of resending, or re-signs the package so its hash
//! changes, the head never reaches the target; if it resends identically, it does. Completion is
//! the collator's **finalized** parachain height, not a log line: a parachain block only finalizes
//! after JAM accumulated the work package that produced it, which is exactly the proof that a
//! package was resubmitted to JAM. The proxy's own ledger is the witness that the first submission
//! was really dropped.
//!
//! [`another_collator_resends_a_lost_package`] has two collators authoring, but only one behind a
//! proxy running [`DropPolicy::Everything`]: the other has to learn the proxied collator's
//! packages over the collator sync protocol and resubmit them. Its completion is `bob`'s finalized
//! height while the proxy ledger stays empty, and it additionally requires a finalized block
//! authored by the *other* collator — with alice's submissions all dropped, only a resubmission
//! through `bob` can put one there.

use anyhow::{anyhow, Context};
use codec::Decode;
use cumulus_jam_zombienet_tests::{
	env::binaries_or_err,
	network::free_port,
	para::{Para, JAM_SLOT, TINY_CORES},
	proxy::{DropPolicy, ProxyServer},
	spawn::{spawn, CollatorOptions, JamNetwork, SpawnOptions},
};
use cumulus_zombienet_sdk_helpers::{
	jam::{DigestItem, JamHeader},
	ParaConfig,
};
use std::{collections::HashMap, path::PathBuf, time::Duration};
use tokio::time::Instant;
use zombienet_sdk::subxt::OnlineClient;

/// The finalized parachain height the run has to reach. Every block needs its package reported to
/// JAM, and every reported package had its first submission dropped, so reaching this is only
/// possible through resends.
const HEAD_TARGET: u64 = 5;
/// Loose, but a collator that never resends has to time out here rather than pass.
const HEAD_BUDGET: Duration = Duration::from_secs(10 * 60);

/// One collator behind a proxy that drops the first submission of every package; the head can
/// only reach the target by resending.
#[tokio::test(flavor = "multi_thread")]
async fn resends_make_the_head_advance() -> Result<(), anyhow::Error> {
	const TEST: &str = "resends_make_the_head_advance";
	let (jam, proxy) =
		spawn_proxied_network(TEST, &["alice"], &["alice"], DropPolicy::FirstSubmission, None)
			.await?;
	let result = resend_then_advance(&jam, &proxy).await;
	let result = result.map_err(|error| anyhow!("{error}\n\n{}", proxy.describe()));
	proxy.shutdown().await;
	jam.finish(result).await
}

/// Two collators authoring; only alice is behind a proxy that drops **every** submission it sees,
/// while bob talks to JAM directly. The head can only advance if bob learns alice's packages over
/// the collator sync protocol and resubmits them.
#[tokio::test(flavor = "multi_thread")]
async fn another_collator_resends_a_lost_package() -> Result<(), anyhow::Error> {
	const TEST: &str = "another_collator_resends_a_lost_package";
	let binaries = binaries_or_err()?;
	let runtime = binaries.runtime_wasm.clone();
	let (jam, proxy) = spawn_proxied_network(
		TEST,
		&["alice", "bob"],
		&["alice"],
		DropPolicy::Everything,
		Some(runtime),
	)
	.await?;
	let result = foreign_resend_then_advance(&jam, &proxy).await;
	let result = result.map_err(|error| anyhow!("{error}\n\n{}", proxy.describe()));
	proxy.shutdown().await;
	jam.finish(result).await
}

/// Spawn the tiny JAM network with `collators` authoring para 0, the ones named in `proxied`
/// pointing their `--jam-rpc-urls` at a proxy applying `policy`.
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
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let jam_or_port = free_port()?;
	let proxy_port = free_port()?;

	let mut para = Para::new(0, 0, collators);
	if let Some(runtime) = runtime {
		para = para.with_runtime(runtime);
	}
	let paras = vec![para];

	// Authority discovery publishes only addresses the node considers external; a bare listening
	// socket is not one, so with no `--public-addr` the AD worker publishes nothing and the
	// collators can never resolve each other. Pin each collator's p2p port and name it as a
	// public address, so the loopback socket the other collator dials is what AD hands back.
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
				},
			))
		})
		.collect::<anyhow::Result<HashMap<_, _>>>()?;
	let options = SpawnOptions {
		cores: TINY_CORES,
		ordinary_rpc_port: Some(jam_or_port),
		collators: collator_options,
		ready_timeout: Duration::from_secs(120),
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

/// Wait until `client`'s collator has **finalized** block number `target`.
///
/// A parachain block only finalizes after JAM has accumulated and reported the work package that
/// produced it, so a finalized height is the objective proof that a package was resubmitted to
/// JAM. A log line cannot prove that: the node emits it on its own schedule, independent of
/// whether the package ever reached a guarantor. The error names the last finalized height, so a
/// stuck run says whether the chain stalled or merely fell behind.
async fn wait_for_finalized_height(
	client: &OnlineClient<ParaConfig>,
	target: u64,
	budget: Duration,
) -> anyhow::Result<()> {
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
			return Ok(());
		}
	}
}

/// Wait for alice's own finalized parachain height to prove the resend reached JAM, then check the
/// proxy ledger shows every package had to be submitted at least twice.
async fn resend_then_advance(jam: &JamNetwork, proxy: &ProxyServer) -> anyhow::Result<()> {
	// How long after its first submission a package may still have been submitted only once. A
	// collator resends at anchor+2, one JAM_SLOT at a time, so anything older than this that was
	// never submitted twice is a package the collator abandoned instead of resending.
	let resend_grace = JAM_SLOT * 4;

	let alice_client = jam.collator_client("alice").await?;
	wait_for_finalized_height(&alice_client, HEAD_TARGET, HEAD_BUDGET).await?;

	let attempts = proxy.snapshot();
	anyhow::ensure!(
		!attempts.is_empty(),
		"the head reached {HEAD_TARGET}, but the proxy saw no submitWorkPackage at all: alice is \
		 talking to the network directly, not through the proxy"
	);

	let forwarded: Vec<String> = attempts
		.iter()
		.filter(|attempt| attempt.count >= 2)
		.map(|attempt| hex::encode(&attempt.hash[..8]))
		.collect();
	anyhow::ensure!(
		forwarded.len() as u64 >= HEAD_TARGET,
		"the head reached {HEAD_TARGET} through the proxy, but the proxy forwarded only {} \
		 distinct work package(s); every accumulated head's package must have been submitted at \
		 least twice",
		forwarded.len()
	);

	for attempt in &attempts {
		if attempt.count == 1 {
			let age = attempt.first_seen.elapsed();
			anyhow::ensure!(
				age < resend_grace,
				"the proxy saw work package 0x{}… once and never again {age:?} after its first \
				 submission; the collator must resend the identical bytes, not re-sign a new \
				 package",
				hex::encode(&attempt.hash[..8]),
			);
		}
	}

	anyhow::ensure!(
		proxy.upstream_errors() == 0,
		"the proxy's upstream rejected {} forwarded submission(s)",
		proxy.upstream_errors()
	);

	Ok(())
}

/// Wait for `bob`'s finalized parachain height while alice's submissions all stay dropped: only
/// bob resubmitting alice's packages can finalize a block, and the proxy ledger proves alice never
/// delivered.
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
/// block can only be there because bob resubmitted its package; bob's own re-rooted blocks are all
/// his. Two distinct authors therefore prove a foreign resubmission happened, with no log line.
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
