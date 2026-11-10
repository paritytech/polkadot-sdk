// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

use codec::Decode;
use cumulus_primitives_core::{
	relay_chain::{BlockId as RelayBlockId, OccupiedCoreAssumption},
	ParaId,
};
use cumulus_relay_chain_interface::{RelayChainError, RelayChainInterface, RelayChainResult};
use polkadot_primitives::{Block as RelayBlock, Hash as RelayHash, DEFAULT_SCHEDULING_LOOKAHEAD};
use sc_client_api::{Backend, HeaderBackend};
use sc_consensus_babe::contains_epoch_change;
use sp_blockchain::Backend as BlockchainBackend;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use std::collections::HashSet;

const LOG_TARGET: &str = "consensus::common::parent_search";

#[derive(Clone, Debug)]
pub enum ParentSearchParams {
	/// Candidate version V2
	V2 {
		/// The relay parent: anchors the included/pending lookup and the V2 lookahead window.
		relay_parent: RelayHash,
		/// The relay chain tip the relay parent trails; V3-executed blocks are judged from it.
		scheduling_parent: RelayHash,
	},
	/// Candidate version V3
	V3 {
		/// The scheduling-parent that is intended to be used.
		scheduling_parent: RelayHash,
	},
}

impl ParentSearchParams {
	/// The params for the scheduling version.
	pub fn new(v3_enabled: bool, scheduling_parent: RelayHash, relay_parent: RelayHash) -> Self {
		if v3_enabled {
			Self::V3 { scheduling_parent }
		} else {
			Self::V2 { relay_parent, scheduling_parent }
		}
	}

	/// Anchor for the included/pending lookup and the V2 window: V2 uses the relay parent. V3
	/// uses the tip, where a V2-executed block's original submission could still land.
	fn anchor(&self) -> RelayHash {
		match self {
			ParentSearchParams::V2 { relay_parent, .. } => *relay_parent,
			ParentSearchParams::V3 { scheduling_parent } => *scheduling_parent,
		}
	}

	/// Anchor V3-executed blocks are judged from.
	fn scheduling_parent(&self) -> RelayHash {
		match self {
			ParentSearchParams::V2 { scheduling_parent, .. } |
			ParentSearchParams::V3 { scheduling_parent } => *scheduling_parent,
		}
	}
}

/// A potential parent block returned from [`find_parent_for_building`].
#[derive(PartialEq, Clone)]
pub struct ParentSearchResult<Block: BlockT> {
	/// Included head at the scheduling parent.
	pub included_at_scheduling: Block::Header,
	/// Block to build on.
	pub best_parent_header: Block::Header,
	/// V3 resubmittable segment, oldest first; empty for V2 and free of V2-executed blocks.
	pub resubmittable_segment: Vec<Block::Header>,
}

/// Trim the best parent (and segment) back to the first ancestor passing `filter_parent`, or to
/// the included head. Walks the in-memory segment first, then follows storage via `get_header` past
/// the segment base to an older bundle-ender when the segment itself holds none.
pub(crate) fn trim_best_parent_to_filter<Block: BlockT>(
	mut result: ParentSearchResult<Block>,
	get_header: impl Fn(Block::Hash) -> Option<Block::Header>,
	filter_parent: impl Fn(&Block::Header) -> bool,
) -> ParentSearchResult<Block> {
	while !filter_parent(&result.best_parent_header) {
		let parent_hash = *result.best_parent_header.parent_hash();
		result.resubmittable_segment.pop();
		// Inside the segment the popped tail is the parent, read from memory; once it empties, the
		// parent sits below the segment base and is read from storage.
		let parent =
			result.resubmittable_segment.last().cloned().or_else(|| get_header(parent_hash));
		match parent {
			Some(header) => {
				result.best_parent_header = header;
				if parent_hash == result.included_at_scheduling.hash() {
					break;
				}
			},
			None => {
				result.best_parent_header = result.included_at_scheduling.clone();
				result.resubmittable_segment.clear();
				break;
			},
		}
	}
	result
}

impl<B: BlockT> std::fmt::Debug for ParentSearchResult<B> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("ParentSearchResult")
			.field("included_at_scheduling_number", &self.included_at_scheduling.number())
			.field("best_parent_hash", &self.best_parent_header.hash())
			.field("best_parent_number", &self.best_parent_header.number())
			.field("resubmittable_segment_len", &self.resubmittable_segment.len())
			.finish()
	}
}

fn get_para_header<Block: BlockT>(
	backend: &impl Backend<Block>,
	hash: Block::Hash,
) -> Option<Block::Header> {
	let Ok(Some(header)) = backend.blockchain().header(hash) else {
		tracing::warn!(
			target: LOG_TARGET,
			%hash,
			"Failed to get header for para block.",
		);
		return None;
	};

	Some(header)
}

async fn fetch_pvd_header<Block: BlockT>(
	relay_client: &impl RelayChainInterface,
	at: RelayHash,
	para_id: ParaId,
	occupied_core_assumption: OccupiedCoreAssumption,
) -> RelayChainResult<Option<Block::Header>> {
	let maybe_header = relay_client
		.persisted_validation_data(at, para_id, occupied_core_assumption)
		.await?
		.and_then(|pvd| Block::Header::decode(&mut &pvd.parent_head.0[..]).ok());

	Ok(maybe_header)
}

/// Fetch the included block from the relay chain.
pub async fn fetch_included_from_relay_chain<B: BlockT>(
	relay_client: &impl RelayChainInterface,
	backend: &impl Backend<B>,
	at: RelayHash,
	para_id: ParaId,
) -> Result<Option<(B::Header, B::Hash)>, RelayChainError> {
	// Fetch the pending header from the relay chain. We use `OccupiedCoreAssumption::TimedOut`
	// so that even if there is a pending candidate, we assume it is timed out, and we get the
	// included head.
	let Some(included_header) =
		fetch_pvd_header::<B>(relay_client, at, para_id, OccupiedCoreAssumption::TimedOut).await?
	else {
		return Ok(None);
	};

	let included_hash = included_header.hash();
	// If the included block is not locally known, we can't do anything.
	let Some(included_header) = get_para_header(backend, included_hash) else {
		return Ok(None);
	};
	Ok(Some((included_header, included_hash)))
}

/// Build an ancestry of relay parents that are acceptable.
///
/// An acceptable relay parent is one that is no more than `ancestry_lookback` + 1 blocks below the
/// relay parent we want to build on. Parachain blocks anchored on relay parents older than that can
/// not be considered potential parents for block building. They have no chance of still getting
/// included, so our newly build parachain block would also not get included.
///
/// On success, returns a vector of `(header_hash, state_root)` of the relevant relay chain
/// ancestry blocks.
async fn build_relay_parent_ancestry(
	relay_client: &impl RelayChainInterface,
	relay_parent: RelayHash,
	ancestry_lookback: usize,
) -> Result<Vec<(RelayHash, RelayHash)>, RelayChainError> {
	let mut ancestry = Vec::with_capacity(ancestry_lookback + 1);
	let mut current_rp = relay_parent;
	while ancestry.len() <= ancestry_lookback {
		let Some(header) = relay_client.header(RelayBlockId::hash(current_rp)).await? else {
			tracing::warn!(
				target: LOG_TARGET,
				?current_rp,
				"Relay chain header missing while walking the allowed ancestry.",
			);
			break;
		};

		ancestry.push((current_rp, *header.state_root()));
		current_rp = *header.parent_hash();

		// Respect the relay-chain rule not to cross session boundaries.
		if contains_epoch_change::<RelayBlock>(&header) {
			break;
		}

		// don't iterate back into the genesis block.
		if header.number == 1 {
			break;
		}
	}
	Ok(ancestry)
}

/// Check if a block's relay parent is within the allowed ancestry.
fn is_relay_parent_in_ancestry<Block: BlockT>(
	header: &Block::Header,
	rp_ancestry: &[(RelayHash, RelayHash)],
) -> bool {
	let digest = header.digest();
	let relay_parent = cumulus_primitives_core::extract_relay_parent(digest);
	let storage_root =
		cumulus_primitives_core::rpsr_digest::extract_relay_parent_storage_root(digest)
			.map(|(storage_root, _)| storage_root);
	if relay_parent.is_none() && storage_root.is_none() {
		return false;
	}

	rp_ancestry.iter().any(|(rp_hash, rp_storage_root)| {
		Some(*rp_hash) == relay_parent || Some(*rp_storage_root) == storage_root
	})
}

/// The relay parents a V2-executed block may use: the lookahead window ending at `anchor`.
async fn v2_window(
	relay_client: &impl RelayChainInterface,
	anchor: RelayHash,
) -> RelayChainResult<Vec<(RelayHash, RelayHash)>> {
	let lookback = relay_client
		.scheduling_lookahead(anchor)
		.await
		.unwrap_or(DEFAULT_SCHEDULING_LOOKAHEAD)
		.saturating_sub(1) as usize;
	build_relay_parent_ancestry(relay_client, anchor, lookback).await
}

/// Whether `header` carries the runtime upgrade digest.
pub fn has_runtime_upgrade_digest<H: HeaderT>(header: &H) -> bool {
	header
		.digest()
		.logs
		.iter()
		.any(|it| matches!(it, sp_runtime::DigestItem::RuntimeEnvironmentUpdated))
}

/// Find the deepest valid parent block starting from `start`.
///
/// The `start` block (pending or included) is always valid by construction. Descendants are
/// explored via DFS and judged by the rules of the runtime that executed them: V3-executed blocks
/// need a relay parent the relay chain still knows from the scheduling parent, V2-executed blocks
/// need one in the V2 window. Also returns the valid V2-executed blocks.
async fn find_deepest_valid_parent<Block: BlockT>(
	relay_client: &impl RelayChainInterface,
	backend: &impl Backend<Block>,
	params: &ParentSearchParams,
	start_header: Block::Header,
	start_hash: Block::Hash,
	v3_enabled_at: &impl Fn(Block::Hash) -> bool,
) -> RelayChainResult<(Block::Header, HashSet<Block::Hash>)> {
	// Pure V2 needs the window anyway and keeps propagating its fetch error; V3 fetches it lazily.
	let mut v2_ancestry = match params {
		ParentSearchParams::V2 { relay_parent, .. } => {
			Some(v2_window(relay_client, *relay_parent).await?)
		},
		ParentSearchParams::V3 { .. } => None,
	};
	let mut v2_executed = HashSet::new();

	// Each entry carries whether its block was executed under V3. The flag only changes across a
	// code change, which always carries the upgrade digest, so the runtime is only queried at the
	// start block and at digest-carrying parents.
	let mut frontier: Vec<(Block::Hash, bool)> = Vec::new();
	let push_children = |frontier: &mut Vec<_>, parent: Block::Hash, v3: Option<bool>| {
		let children = backend.blockchain().children(parent).unwrap_or_default();
		if !children.is_empty() {
			let v3 = v3.unwrap_or_else(|| v3_enabled_at(parent));
			frontier.extend(children.into_iter().map(|child| (child, v3)));
		}
	};
	push_children(&mut frontier, start_hash, None);

	tracing::trace!(
		target: LOG_TARGET,
		?start_hash,
		num_children = frontier.len(),
		"Searching for deepest valid parent."
	);

	let mut best = start_header;
	while let Some((hash, executed_by_v3)) = frontier.pop() {
		let Ok(Some(header)) = backend.blockchain().header(hash) else { continue };

		let is_valid = if executed_by_v3 {
			has_ancestor_relay_parent_info::<Block>(
				relay_client,
				params.scheduling_parent(),
				&header,
			)
			.await
			.unwrap_or(false)
		} else {
			if v2_ancestry.is_none() {
				let ancestry =
					v2_window(relay_client, params.anchor()).await.unwrap_or_else(|error| {
						tracing::debug!(
							target: LOG_TARGET,
							?error,
							"Failed to fetch the V2 window; V2-executed blocks are invalid.",
						);
						Vec::new()
					});
				v2_ancestry = Some(ancestry);
			}
			v2_ancestry
				.as_deref()
				.is_some_and(|ancestry| is_relay_parent_in_ancestry::<Block>(&header, ancestry))
		};
		if !is_valid {
			continue;
		}
		if !executed_by_v3 {
			v2_executed.insert(hash);
		}

		let v3 = (!has_runtime_upgrade_digest(&header)).then_some(executed_by_v3);
		push_children(&mut frontier, hash, v3);

		// This block is valid - update best if it's deeper.
		if header.number() > best.number() {
			best = header;
		}
	}

	Ok((best, v2_executed))
}

/// The chain from `best` back to `start_hash` (exclusive), oldest first.
fn reconstruct_resubmittable_segment<Block: BlockT>(
	backend: &impl Backend<Block>,
	best: &Block::Header,
	start_hash: Block::Hash,
) -> Vec<Block::Header> {
	let mut ancestry = Vec::new();
	let mut current = best.clone();
	while current.hash() != start_hash {
		let parent_hash = *current.parent_hash();
		ancestry.push(current);
		match backend.blockchain().header(parent_hash) {
			Ok(Some(parent)) => current = parent,
			_ => break,
		}
	}
	ancestry.reverse();
	ancestry
}

async fn get_relay_parent<Block: BlockT>(
	relay_client: &impl RelayChainInterface,
	header: &Block::Header,
) -> RelayChainResult<Option<RelayHash>> {
	let digest = header.digest();

	if let Some(relay_parent) = cumulus_primitives_core::extract_relay_parent(digest) {
		return Ok(Some(relay_parent));
	}

	if let Some((storage_root, number)) =
		cumulus_primitives_core::rpsr_digest::extract_relay_parent_storage_root(digest)
	{
		let Some(relay_parent_header) = relay_client.header(RelayBlockId::Number(number)).await?
		else {
			return Ok(None);
		};
		if relay_parent_header.state_root != storage_root {
			return Ok(None);
		}
		return Ok(Some(relay_parent_header.hash()));
	}

	Ok(None)
}

/// True if `header`'s relay parent is a known ancestor of `scheduling_parent` on the relay chain,
/// i.e. one the relay chain still accepts candidates on.
async fn has_ancestor_relay_parent_info<Block: BlockT>(
	relay_client: &impl RelayChainInterface,
	scheduling_parent: RelayHash,
	header: &Block::Header,
) -> RelayChainResult<bool> {
	let Some(relay_parent) = get_relay_parent::<Block>(relay_client, header).await? else {
		return Ok(false);
	};

	if relay_parent == scheduling_parent {
		return Ok(true);
	}

	let relay_parent_session = relay_client.session_index_for_child(relay_parent).await?;
	let maybe_info = relay_client
		.ancestor_relay_parent_info(scheduling_parent, relay_parent_session, relay_parent)
		.await?;
	Ok(maybe_info.is_some())
}

/// Find the best parent block to build on.
///
/// This accepts a relay-chain block to be used as an anchor and searches for the best
/// parachain block to use as a parent for a new block.
///
/// The search starts from either the pending block (if one exists) or the included block,
/// and finds the deepest descendant whose relay-parent is within the allowed ancestry.
///
/// `v3_enabled_at` tells whether the runtime at a block has scheduling V3 enabled, i.e. whether
/// its children are executed under V3. Each unincluded block is judged by the rules of the runtime
/// that executed it, and V2-executed blocks never enter the V3 resubmittable segment.
///
/// Returns `None` if no suitable parent can be found (e.g., included block unknown locally).
pub async fn find_parent_for_building<Block: BlockT>(
	relay_client: &impl RelayChainInterface,
	backend: &impl Backend<Block>,
	para_id: ParaId,
	params: ParentSearchParams,
	v3_enabled_at: impl Fn(Block::Hash) -> bool,
	filter_parent: impl Fn(&Block::Header) -> bool,
) -> RelayChainResult<Option<ParentSearchResult<Block>>> {
	tracing::trace!(
		target: LOG_TARGET,
		?para_id,
		?params,
		"Parent search"
	);

	let anchor = params.anchor();
	let Some((included_header, included_hash)) =
		fetch_included_from_relay_chain(relay_client, backend, anchor, para_id).await?
	else {
		return Ok(None);
	};

	// Fetch the pending block if one exists.
	let maybe_pending = {
		// Fetch the most recent pending header from the relay chain. We use
		// `OccupiedCoreAssumption::Included` so the candidate pending availability gets enacted
		// before being returned to us.
		let maybe_header = fetch_pvd_header::<Block>(
			relay_client,
			anchor,
			para_id,
			OccupiedCoreAssumption::Included,
		)
		.await?
		.filter(|header| header.hash() != included_hash);

		// If the pending block is not locally known, we can't proceed.
		if let Some(header) = maybe_header {
			let hash = header.hash();
			let Some(header) = get_para_header(backend, hash) else {
				return Ok(None);
			};
			Some((header, hash))
		} else {
			None
		}
	};
	// Determine the starting point for the search.
	let (start_header, start_hash) =
		maybe_pending.unwrap_or((included_header.clone(), included_hash));

	let (best_parent_header, v2_executed) = find_deepest_valid_parent(
		relay_client,
		backend,
		&params,
		start_header,
		start_hash,
		&v3_enabled_at,
	)
	.await?;

	// Only V3 resubmits, and only the chain above the last V2-executed block. A lost V2 submission
	// of that block is never resubmitted: the chain extends it until its relay parent leaves the V2
	// window (at most `scheduling_lookahead - 1` relay blocks), then the search forks below it.
	let resubmittable_segment = if matches!(params, ParentSearchParams::V3 { .. }) {
		let mut segment =
			reconstruct_resubmittable_segment(backend, &best_parent_header, start_hash);
		if let Some(last_v2) = segment.iter().rposition(|h| v2_executed.contains(&h.hash())) {
			segment.drain(..=last_v2);
		}
		segment
	} else {
		Vec::new()
	};

	let result = ParentSearchResult {
		included_at_scheduling: included_header,
		best_parent_header,
		resubmittable_segment,
	};

	// Trim the best parent (and its segment) back to the first ancestor passing the filter, so the
	// returned result is the final parent to build on.
	Ok(Some(trim_best_parent_to_filter(
		result,
		|hash| backend.blockchain().header(hash).ok().flatten(),
		filter_parent,
	)))
}
