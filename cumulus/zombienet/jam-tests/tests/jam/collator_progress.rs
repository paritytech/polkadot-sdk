// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The parachain must keep producing and finalizing blocks whatever the size of the collator set.
//!
//! Two collators is the smallest set that exercises handover between collators; six is the whole
//! set of dev accounts the template's genesis endows. The authority set is sized to the collator
//! count in every case, so an unfilled aura slot — which costs a whole slot of block production —
//! shows up as a timeout rather than passing unnoticed.

use super::harness::assert_collators_build_blocks;

/// Long enough that a stall lasting a few slots cannot be mistaken for progress.
const BLOCKS: u64 = 30;

/// Finality trails the best block by about four, so this is the highest number that is certain to
/// be reached once `BLOCKS` is.
const FINALIZED: u64 = 25;

#[tokio::test(flavor = "multi_thread")]
async fn one_jam_collator_builds_blocks() -> Result<(), anyhow::Error> {
	assert_collators_build_blocks("one_jam_collator_builds_blocks", 1, BLOCKS, FINALIZED).await
}

#[tokio::test(flavor = "multi_thread")]
async fn two_jam_collators_build_blocks() -> Result<(), anyhow::Error> {
	assert_collators_build_blocks("two_jam_collators_build_blocks", 2, BLOCKS, FINALIZED).await
}

#[tokio::test(flavor = "multi_thread")]
async fn three_jam_collators_build_blocks() -> Result<(), anyhow::Error> {
	assert_collators_build_blocks("three_jam_collators_build_blocks", 3, BLOCKS, FINALIZED).await
}

#[tokio::test(flavor = "multi_thread")]
async fn six_jam_collators_build_blocks() -> Result<(), anyhow::Error> {
	assert_collators_build_blocks("six_jam_collators_build_blocks", 6, BLOCKS, FINALIZED).await
}
