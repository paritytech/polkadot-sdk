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

//! The parachain must keep producing and finalizing blocks whatever the size of the collator set.
//!
//! Two collators is the smallest set that exercises handover between collators; six is the whole
//! set of dev accounts the template's genesis endows. The authority set is sized to the collator
//! count in every case, so an unfilled aura slot — which costs a whole slot of block production —
//! shows up as a timeout rather than passing unnoticed.
//!
//! The assertions are the ones the old `assert_collators_build_blocks` made: every collator
//! reaches best [`BLOCKS`] and finalized [`FINALIZED`], JAM has accumulated the para head to
//! [`JAM_HEAD_TARGET`], and the finalized header carries a non-zero `JamParent` digest. The
//! height waits alone are a false positive when the JAM pipeline is dead, so the last two are the
//! guards the old harness added to kill exactly that.

use crate::jam::{jam_rpc_url, wait_for_jam_head, Para};
use anyhow::{anyhow, Context};
use cumulus_jam_zombienet_tests::{
	env::binaries_or_err,
	genesis_build::{build_jam_genesis, polkavm_env},
	network::{
		base_dir, collator_args, copy_para_specs, path_str, work_dir, ORDINARY_NODE,
		VALIDATORS_PER_CORE,
	},
	para::{DEADLINE, PARACHAIN_SERVICE_ID, TINY_CORES},
	rpc::{CollatorRpc, JamRpc},
};
use cumulus_primitives_core::{relay_chain, CumulusDigestItem};
use sp_runtime::generic::Digest as SubstrateDigest;
use std::time::Duration;
use tokio::time::Instant;
use zombienet_sdk::{LocalFileSystem, Network};

/// Long enough that a stall lasting a few slots cannot be mistaken for progress.
const BLOCKS: u64 = 30;

/// Finality trails the best block by about four, so this is the highest number that is certain to
/// be reached once `BLOCKS` is.
const FINALIZED: u64 = 25;

/// The para head number JAM's own storage must reach before a progress test passes. A collator
/// authors blocks whether or not JAM accepts them, so the height waits alone are a false positive
/// when the pipeline is dead; this requires sustained accumulation across most of the run.
const JAM_HEAD_TARGET: u64 = 20;

/// Budget for the JAM-head assertion. On a healthy network the head already exceeds
/// [`JAM_HEAD_TARGET`] by the time the height waits finish.
const JAM_HEAD_BUDGET: Duration = Duration::from_secs(8 * 60);

/// The best-block metric every zombienet node reports.
const PARA_BLOCK_METRIC: &str = "block_height{status=\"best\"}";

/// The finalized-block metric every zombienet node reports.
const PARA_FINALIZED_METRIC: &str = "block_height{status=\"finalized\"}";

#[tokio::test(flavor = "multi_thread")]
async fn one_jam_collator_builds_blocks() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	assert_collators_build_blocks("one_jam_collator_builds_blocks", 1).await
}

#[tokio::test(flavor = "multi_thread")]
async fn two_jam_collators_build_blocks() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	assert_collators_build_blocks("two_jam_collators_build_blocks", 2).await
}

#[tokio::test(flavor = "multi_thread")]
async fn three_jam_collators_build_blocks() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	assert_collators_build_blocks("three_jam_collators_build_blocks", 3).await
}

#[tokio::test(flavor = "multi_thread")]
async fn six_jam_collators_build_blocks() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	assert_collators_build_blocks("six_jam_collators_build_blocks", 6).await
}

/// Run the para [`Para::single`] describes with `collators` collators and assert every one of
/// them reaches best [`BLOCKS`] and finalized [`FINALIZED`].
async fn assert_collators_build_blocks(test: &str, collators: usize) -> anyhow::Result<()> {
	let para = Para::single(collators);
	let binaries = binaries_or_err()?;
	let (work_dir, _temp) = work_dir(test)?;
	let paras = std::slice::from_ref(&para);
	let genesis = build_jam_genesis(&binaries, &work_dir, paras, TINY_CORES)?;
	let para_specs = copy_para_specs(&work_dir, &genesis.para_specs, paras)?;
	let base_dir = base_dir(&work_dir)?;
	let jam_node = path_str(&binaries.jam_node)?;
	let genspec_node = binaries.genspec_node.as_deref().map(path_str).transpose()?;
	let omni_node = path_str(&binaries.omni_node)?;
	let authorizer_blob = path_str(&genesis.authorizer_blob)?;
	let overrides = genesis.overrides.clone();
	let no_overrides = std::collections::HashMap::new();
	let validators = TINY_CORES as usize * VALIDATORS_PER_CORE;

	let config =
		zombienet_sdk::NetworkConfigBuilder::new()
			.with_jamchain(|jam| {
				let jam = jam.with_id("jam").with_default_command(jam_node.as_str());
				let jam = match genspec_node.as_deref() {
					Some(command) if command != jam_node.as_str() => {
						jam.with_chain_spec_command(command)
					},
					_ => jam,
				};
				let jam = jam.with_genesis_overrides(overrides);
				let jam = jam.with_validator(|node| node.with_name("jam0").with_env(polkavm_env()));
				let jam = (1..validators).fold(jam, |jam, index| {
					jam.with_validator(|node| {
						node.with_name(&format!("jam{index}")).with_env(polkavm_env())
					})
				});
				jam.with_ordinary(|node| node.with_name(ORDINARY_NODE).with_env(polkavm_env()))
			})
			.with_parachain(|p| {
				let p = p
					.with_id(para.id)
					.with_registration_strategy(zombienet_sdk::RegistrationStrategy::Manual)
					.with_chain_spec_path(para_specs[0].clone())
					.with_default_command(omni_node.as_str());
				let p = p.with_collator(|node| {
					node.with_name(para.collators[0].as_str()).with_env(polkavm_env()).with_args(
						collator_args(&authorizer_blob, &no_overrides, &para.collators[0], true),
					)
				});
				para.collators[1..].iter().fold(p, |p, name| {
					p.with_collator(|node| {
						node.with_name(name.as_str())
							.with_env(polkavm_env())
							.with_args(collator_args(&authorizer_blob, &no_overrides, name, true))
					})
				})
			})
			.with_global_settings(|g| g.with_base_dir(base_dir))
			.build()
			.map_err(|errors| {
				anyhow!(
					"config errs: {}",
					errors.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ")
				)
			})?;

	let network = zombienet_sdk::environment::get_spawn_fn()(config).await?;
	let result = async {
		assert_every_collator_reaches(&network, &para).await?;
		assert_jam_heads_advance(&network, &para).await?;
		assert_jam_parent_in_blocks(&network, &para).await
	}
	.await;

	if let Err(error) = network.destroy().await {
		log::warn!("tearing down the JAM network failed: {error}");
	}
	result
}

/// Wait, per collator, for the best metric to pass [`BLOCKS`] and then the finalized metric to
/// pass [`FINALIZED`]. Every collator is waited on, because a set where only one collator authors
/// still has to keep the chain producing and finalizing.
async fn assert_every_collator_reaches(
	network: &Network<LocalFileSystem>,
	para: &Para,
) -> anyhow::Result<()> {
	let timeout = DEADLINE.as_secs();
	for name in &para.collators {
		let node = network.get_node(name.as_str())?;

		log::info!("Waiting for collator {name} to reach best block #{BLOCKS}");
		node.wait_metric_with_timeout(PARA_BLOCK_METRIC, |best| best >= BLOCKS as f64, timeout)
			.await
			.map_err(|error| {
				anyhow!("collator {name} did not reach best block #{BLOCKS} in {timeout}s: {error}")
			})?;

		log::info!("Waiting for collator {name} to finalize block #{FINALIZED}");
		node.wait_metric_with_timeout(
			PARA_FINALIZED_METRIC,
			|finalized| finalized >= FINALIZED as f64,
			timeout,
		)
		.await
		.map_err(|error| {
			anyhow!("collator {name} did not finalize block #{FINALIZED} in {timeout}s: {error}")
		})?;
	}
	Ok(())
}

/// Assert JAM accumulated `para`'s head to [`JAM_HEAD_TARGET`], not just that the collators
/// authored blocks. A wrong PVF format, an unregistered para, or a missing code preimage leaves
/// the collator authoring while JAM accepts nothing, so the height waits above cannot see it.
async fn assert_jam_heads_advance(
	network: &Network<LocalFileSystem>,
	para: &Para,
) -> anyhow::Result<()> {
	let jam_url = jam_rpc_url(network)?;
	let jam_rpc = JamRpc::wait_ready(&jam_url, Instant::now() + DEADLINE).await?;
	wait_for_jam_head(&jam_rpc, PARACHAIN_SERVICE_ID, para.id, JAM_HEAD_TARGET, JAM_HEAD_BUDGET)
		.await
}

/// Assert the finalized header carries a `JamParent` digest whose anchors are non-zero — proof the
/// JAM host served real anchors in the refine context.
async fn assert_jam_parent_in_blocks(
	network: &Network<LocalFileSystem>,
	para: &Para,
) -> anyhow::Result<()> {
	let name = para.collators.first().context("the para has no collator")?;
	let rpc =
		CollatorRpc::connect(&network.get_node(name.as_str())?.ws_uri(), Instant::now() + DEADLINE)
			.await?;

	let header = rpc.finalized_header().await?;
	let raw = header["number"].as_str().context("header.number is not a string")?;
	let height = u64::from_str_radix(raw.trim_start_matches("0x"), 16)
		.with_context(|| format!("header.number {raw:?} is not valid hex"))?;

	if height <= 2 {
		log::warn!(
			"assert_jam_parent_in_blocks: finalized head at height {height}, skipping (no prior \
			 anchor at height <= 2)"
		);
		return Ok(());
	}

	let digest: SubstrateDigest = serde_json::from_value(header["digest"].clone())
		.context("decoding finalized header digest")?;

	let (anchor, lookup_anchor) = CumulusDigestItem::find_jam_parent(&digest).ok_or_else(|| {
		anyhow!(
			"finalized block at height {height} has no JamParent digest — \
			 rebuild RUNTIME_WASM with --cfg jam"
		)
	})?;

	anyhow::ensure!(
		anchor != relay_chain::Hash::default(),
		"finalized block at height {height}: JamParent anchor is all-zero — the JAM host served a \
		 zero anchor in the refine context"
	);
	anyhow::ensure!(
		lookup_anchor != relay_chain::Hash::default(),
		"finalized block at height {height}: JamParent lookup_anchor is all-zero — the JAM host \
		 served a zero lookup_anchor in the refine context"
	);

	log::info!(
		"assert_jam_parent_in_blocks: height {height}: anchor={anchor:?} \
		 lookup_anchor={lookup_anchor:?}"
	);
	Ok(())
}
