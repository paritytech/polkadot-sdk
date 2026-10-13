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
use std::future::Future;

const LOG_TARGET: &str = "consensus::common::parent_search";

#[derive(Clone, Debug)]
pub enum ParentSearchParams {
	/// Candidate version V2
	V2 {
		/// The scheduling-parent that is intended to be used.
		/// For V2, the scheduling parent is equal to the relay parent.
		scheduling_parent: RelayHash,
	},
	/// Candidate version V3
	V3 {
		/// The scheduling-parent that is intended to be used.
		scheduling_parent: RelayHash,
	},
}

impl ParentSearchParams {
	/// The params for the scheduling version: V3 anchors at the scheduling parent, V2 at the
	/// relay parent.
	pub fn new(v3_enabled: bool, scheduling_parent: RelayHash, relay_parent: RelayHash) -> Self {
		if v3_enabled {
			Self::V3 { scheduling_parent }
		} else {
			Self::V2 { scheduling_parent: relay_parent }
		}
	}

	/// The relay block the search is anchored at: the scheduling parent for V3, the relay parent
	/// for V2.
	pub fn scheduling_parent(&self) -> &RelayHash {
		match self {
			ParentSearchParams::V2 { scheduling_parent } => scheduling_parent,
			ParentSearchParams::V3 { scheduling_parent } => scheduling_parent,
		}
	}
}

/// A potential parent block returned from [`find_parent_for_building`]
#[derive(PartialEq, Clone)]
pub struct ParentSearchResult<Block: BlockT> {
	/// The header of the included block (confirmed on relay chain) at the scheduling parent.
	pub included_at_scheduling: Block::Header,
	/// The header of the best parent block to build on.
	pub best_parent_header: Block::Header,
}

impl<B: BlockT> std::fmt::Debug for ParentSearchResult<B> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("ParentSearchResult")
			.field("included_at_scheduling_number", &self.included_at_scheduling.number())
			.field("best_parent_hash", &self.best_parent_header.hash())
			.field("best_parent_number", &self.best_parent_header.number())
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

/// Find the deepest valid parent block starting from `start`.
///
/// The `start` block (pending or included) is always valid by construction.
/// This function explores its descendants via DFS, returning the deepest block
/// whose relay-parent is within the allowed ancestry.
async fn find_deepest_valid_parent<Block: BlockT, Fut: Future<Output = bool>>(
	backend: &impl Backend<Block>,
	start_header: Block::Header,
	start_hash: Block::Hash,
	is_valid: impl Fn(&Block::Header) -> Fut,
) -> Block::Header {
	let mut best = start_header;

	let mut frontier: Vec<Block::Hash> =
		backend.blockchain().children(start_hash).ok().into_iter().flatten().collect();

	tracing::trace!(
		target: LOG_TARGET,
		?start_hash,
		num_children = frontier.len(),
		"Searching for deepest valid parent."
	);

	while let Some(hash) = frontier.pop() {
		let Ok(Some(header)) = backend.blockchain().header(hash) else { continue };

		if !is_valid(&header).await {
			continue;
		}

		// This block is valid - update best if it's deeper.
		if header.number() > best.number() {
			best = header;
		}

		frontier.extend(backend.blockchain().children(hash).ok().into_iter().flatten());
	}

	best
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

/// The para state a parent search starts from at its relay-chain anchor.
pub struct SearchStart<Block: BlockT> {
	/// The included header at the search's relay-chain anchor.
	pub included_header: Block::Header,
	/// The pending-availability block, when one exists. Always locally known: an unknown pending
	/// block aborts the search instead.
	pub pending_header: Option<Block::Header>,
}

impl<Block: BlockT> SearchStart<Block> {
	/// The block the search starts from: the pending block when present, the included one
	/// otherwise. Valid by construction — included is on-chain state and pending is already
	/// backed — so only descendants need validity checks.
	pub fn start_header(&self) -> &Block::Header {
		self.pending_header.as_ref().unwrap_or(&self.included_header)
	}
}

/// Resolve the block a parent search anchored at `scheduling_parent` starts from: the pending
/// block when one exists and is locally known, the included block otherwise.
///
/// `None` when the included block is unknown, or a pending block exists but is not locally known.
pub async fn search_start_block<Block: BlockT>(
	relay_client: &impl RelayChainInterface,
	backend: &impl Backend<Block>,
	scheduling_parent: RelayHash,
	para_id: ParaId,
) -> RelayChainResult<Option<SearchStart<Block>>> {
	let Some((included_header, included_hash)) =
		fetch_included_from_relay_chain(relay_client, backend, scheduling_parent, para_id).await?
	else {
		return Ok(None);
	};

	// Fetch the pending block if one exists. `OccupiedCoreAssumption::Included` enacts the
	// candidate pending availability before it is returned to us.
	let maybe_pending = fetch_pvd_header::<Block>(
		relay_client,
		scheduling_parent,
		para_id,
		OccupiedCoreAssumption::Included,
	)
	.await?
	.filter(|header| header.hash() != included_hash);

	let pending_header = match maybe_pending {
		Some(header) => {
			// If the pending block is not locally known, we can't proceed.
			let Some(header) = get_para_header(backend, header.hash()) else {
				return Ok(None);
			};
			Some(header)
		},
		None => None,
	};

	Ok(Some(SearchStart { included_header, pending_header }))
}

/// The per-block validity rule of a parent search, with the context it needs prepared once from
/// the search's [`ParentSearchParams`].
enum ParentValidityCheck {
	/// V2 requires a block's relay parent within the relay parent's allowed ancestry.
	V2 { rp_ancestry: Vec<(RelayHash, RelayHash)> },
	/// V3 requires a block to be backable at the scheduling parent.
	V3 { scheduling_parent: RelayHash },
}

impl ParentValidityCheck {
	async fn new(
		relay_client: &impl RelayChainInterface,
		params: &ParentSearchParams,
	) -> RelayChainResult<Self> {
		Ok(match params {
			ParentSearchParams::V2 { scheduling_parent: relay_parent } => {
				let ancestry_lookback = relay_client
					.scheduling_lookahead(*relay_parent)
					.await
					.unwrap_or(DEFAULT_SCHEDULING_LOOKAHEAD)
					.saturating_sub(1) as usize;
				let rp_ancestry =
					build_relay_parent_ancestry(relay_client, *relay_parent, ancestry_lookback)
						.await?;

				Self::V2 { rp_ancestry }
			},
			ParentSearchParams::V3 { scheduling_parent } => {
				Self::V3 { scheduling_parent: *scheduling_parent }
			},
		})
	}

	async fn is_valid<Block: BlockT>(
		&self,
		relay_client: &impl RelayChainInterface,
		header: &Block::Header,
	) -> RelayChainResult<bool> {
		match self {
			Self::V2 { rp_ancestry } => {
				Ok(is_relay_parent_in_ancestry::<Block>(header, rp_ancestry))
			},
			Self::V3 { scheduling_parent } => {
				has_ancestor_relay_parent_info::<Block>(relay_client, *scheduling_parent, header)
					.await
			},
		}
	}
}

/// Check one block as a build parent under `params`' context: V2 requires its relay parent within
/// the scheduling parent's allowed ancestry, V3 requires it to be backable at the scheduling
/// parent.
///
/// [`find_parent_for_building`] applies the same rule while searching; this is for re-checking an
/// already-found parent after the context it was found under changed.
pub async fn is_parent_valid_for_params<Block: BlockT>(
	relay_client: &impl RelayChainInterface,
	params: &ParentSearchParams,
	header: &Block::Header,
) -> RelayChainResult<bool> {
	ParentValidityCheck::new(relay_client, params)
		.await?
		.is_valid::<Block>(relay_client, header)
		.await
}

/// Find the best parent block to build on.
///
/// This accepts a relay-chain block to be used as an anchor and searches for the best
/// parachain block to use as a parent for a new block.
///
/// The search starts from either the pending block (if one exists) or the included block,
/// and finds the deepest descendant whose relay-parent is within the allowed ancestry.
///
/// Returns `None` if no suitable parent can be found (e.g., included block unknown locally).
pub async fn find_parent_for_building<Block: BlockT>(
	relay_client: &impl RelayChainInterface,
	backend: &impl Backend<Block>,
	para_id: ParaId,
	params: ParentSearchParams,
) -> RelayChainResult<Option<ParentSearchResult<Block>>> {
	tracing::trace!(
		target: LOG_TARGET,
		?para_id,
		?params,
		"Parent search"
	);

	let scheduling_parent = *params.scheduling_parent();
	let Some(start) = search_start_block(relay_client, backend, scheduling_parent, para_id).await?
	else {
		return Ok(None);
	};
	let start_header = start.start_header().clone();
	let start_hash = start_header.hash();
	let included_header = start.included_header;

	let check = ParentValidityCheck::new(relay_client, &params).await?;

	// Search for the deepest valid parent starting from the pending/included block.
	let best_parent_header =
		find_deepest_valid_parent(backend, start_header, start_hash, |header| {
			let header = header.clone();
			let check = &check;
			async move { check.is_valid::<Block>(relay_client, &header).await.unwrap_or(false) }
		})
		.await;

	Ok(Some(ParentSearchResult { included_at_scheduling: included_header, best_parent_header }))
}
