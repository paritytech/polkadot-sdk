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

use anyhow::{anyhow, Context};
use cumulus_jam_zombienet_tests::{
	network::wait_for_collators,
	para::{Para, DEADLINE, PARACHAIN_SERVICE_ID, TINY_CORES},
	para_head::wait_for_jam_head,
	spawn::{spawn, JamNetwork, SpawnOptions},
};
use cumulus_primitives_core::relay_chain;
use cumulus_zombienet_sdk_helpers::find_jam_parent;
use std::time::Duration;

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

#[tokio::test(flavor = "multi_thread")]
async fn one_jam_collator_builds_blocks() -> Result<(), anyhow::Error> {
	assert_collators_build_blocks("one_jam_collator_builds_blocks", 1).await
}

#[tokio::test(flavor = "multi_thread")]
async fn two_jam_collators_build_blocks() -> Result<(), anyhow::Error> {
	assert_collators_build_blocks("two_jam_collators_build_blocks", 2).await
}

#[tokio::test(flavor = "multi_thread")]
async fn three_jam_collators_build_blocks() -> Result<(), anyhow::Error> {
	assert_collators_build_blocks("three_jam_collators_build_blocks", 3).await
}

#[tokio::test(flavor = "multi_thread")]
async fn six_jam_collators_build_blocks() -> Result<(), anyhow::Error> {
	assert_collators_build_blocks("six_jam_collators_build_blocks", 6).await
}

/// Run the para [`Para::single`] describes with `collators` collators and assert every one of
/// them reaches best [`BLOCKS`] and finalized [`FINALIZED`].
async fn assert_collators_build_blocks(test: &str, collators: usize) -> anyhow::Result<()> {
	let jam = spawn(test, &[Para::single(collators)], SpawnOptions::new(TINY_CORES)).await?;
	let para = Para::single(collators);
	let result = async {
		wait_for_collators(
			&jam.network,
			&[Para::single(collators)],
			BLOCKS,
			FINALIZED,
			DEADLINE.as_secs(),
		)
		.await?;
		assert_jam_heads_advance(&jam, &para).await?;
		assert_jam_parent_in_blocks(&jam, &para).await
	}
	.await;

	jam.destroy().await;
	result
}

/// Assert JAM accumulated `para`'s head to [`JAM_HEAD_TARGET`], not just that the collators
/// authored blocks. A wrong PVF format, an unregistered para, or a missing code preimage leaves
/// the collator authoring while JAM accepts nothing, so the height waits above cannot see it.
async fn assert_jam_heads_advance(jam: &JamNetwork, para: &Para) -> anyhow::Result<()> {
	wait_for_jam_head(&jam.jam_rpc, PARACHAIN_SERVICE_ID, para.id, JAM_HEAD_TARGET, JAM_HEAD_BUDGET)
		.await
}

/// Assert the finalized header carries a `JamParent` digest whose anchors are non-zero — proof the
/// JAM host served real anchors in the refine context.
async fn assert_jam_parent_in_blocks(jam: &JamNetwork, para: &Para) -> anyhow::Result<()> {
	let name = para.collators.first().context("the para has no collator")?;
	let client = jam.collator_client(name).await?;

	let mut finalized = client.blocks().subscribe_finalized().await?;
	let block = finalized
		.next()
		.await
		.ok_or_else(|| anyhow!("the finalized block stream ended"))??;

	let height = block.number();
	if height <= 2 {
		log::warn!(
			"assert_jam_parent_in_blocks: finalized head at height {height}, skipping (no prior \
			 anchor at height <= 2)"
		);
		return Ok(());
	}

	let (anchor, lookup_anchor) = find_jam_parent(&block).ok_or_else(|| {
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
