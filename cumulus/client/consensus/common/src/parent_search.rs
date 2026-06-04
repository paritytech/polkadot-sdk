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
use polkadot_primitives::{Block as RelayBlock, Hash as RelayHash, DEFAULT_SCHEDULING_LOOKAHEAD};
use std::ops::Add;

use cumulus_primitives_core::{
	relay_chain::{BlockId as RelayBlockId, OccupiedCoreAssumption},
	ParaId,
};
use cumulus_relay_chain_interface::{RelayChainError, RelayChainInterface, RelayChainResult};

use sc_client_api::{Backend, HeaderBackend};
use sc_consensus_babe::contains_epoch_change;
use sp_blockchain::Backend as BlockchainBackend;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, One};

const LOG_TARGET: &str = "consensus::common::parent_search";

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

/// Fetch the included block from the relay chain.
pub async fn fetch_included_from_relay_chain<B: BlockT>(
	relay_client: &impl RelayChainInterface,
	backend: &impl Backend<B>,
	para_id: ParaId,
	relay_parent: RelayHash,
) -> Result<Option<(B::Hash, B::Header)>, RelayChainError> {
	// Fetch the pending header from the relay chain. We use `OccupiedCoreAssumption::TimedOut`
	// so that even if there is a pending candidate, we assume it is timed out, and we get the
	// included head.
	let maybe_included_header = relay_client
		.persisted_validation_data(relay_parent, para_id, OccupiedCoreAssumption::TimedOut)
		.await?
		.and_then(|pvd| B::Header::decode(&mut &pvd.parent_head.0[..]).ok());
	let Some(included_header) = maybe_included_header else {
		return Ok(None);
	};

	let included_hash = included_header.hash();
	// If the included block is not locally known, we can't do anything.
	let Some(_) = get_para_header(backend, included_hash) else {
		return Ok(None);
	};

	Ok(Some((included_hash, included_header)))
}

struct ParentSearchStart<Block: BlockT> {
	/// The header of the included block (confirmed on relay chain) at the scheduling context.
	included: Block::Header,
	/// The hash and header of the block where the parent search can start.
	start: (Block::Hash, Block::Header),
}

async fn get_parent_search_start<Block: BlockT>(
	relay_client: &impl RelayChainInterface,
	backend: &impl Backend<Block>,
	para_id: ParaId,
	scheduling_parent: RelayHash,
) -> RelayChainResult<Option<ParentSearchStart<Block>>> {
	let Some((included_hash, included_header)) =
		fetch_included_from_relay_chain(relay_client, backend, para_id, scheduling_parent).await?
	else {
		return Ok(None);
	};

	// Fetch the pending block if one exists.
	let maybe_pending = {
		// Fetch the most recent pending header from the relay chain. We use
		// `OccupiedCoreAssumption::Included` so the candidate pending availability gets enacted
		// before being returned to us.
		let pending_header = relay_client
			.persisted_validation_data(scheduling_parent, para_id, OccupiedCoreAssumption::Included)
			.await?
			.and_then(|p| Block::Header::decode(&mut &p.parent_head.0[..]).ok())
			.filter(|x| x.hash() != included_hash);

		// If the pending block is not locally known, we can't proceed.
		match pending_header {
			Some(header) => {
				let pending_hash = header.hash();
				let Some(_) = get_para_header(backend, pending_hash) else {
					return Ok(None);
				};
				Some((pending_hash, header))
			},
			None => None,
		}
	};

	// Determine the starting point for the search.
	let (start_hash, start_header) = match &maybe_pending {
		Some((pending_hash, pending_header)) => {
			// Verify pending is a descendant of included.
			let route =
				sp_blockchain::tree_route(backend.blockchain(), included_hash, *pending_hash)?;
			if !route.retracted().is_empty() {
				tracing::warn!(
					target: LOG_TARGET,
					"Included block not an ancestor of pending block. This should not happen."
				);
				return Ok(None);
			}
			(*pending_hash, pending_header.clone())
		},
		None => (included_hash, included_header.clone()),
	};

	Ok(Some(ParentSearchStart { included: included_header, start: (start_hash, start_header) }))
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
fn find_deepest_valid_parent<Block: BlockT>(
	backend: &impl Backend<Block>,
	start_header: Block::Header,
	start_hash: Block::Hash,
	rp_ancestry: &[(RelayHash, RelayHash)],
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

		if !is_relay_parent_in_ancestry::<Block>(&header, rp_ancestry) {
			continue;
		}

		// This block is valid - update best if it's deeper.
		if header.number() > best.number() {
			best = header.clone();
		}

		frontier.extend(backend.blockchain().children(hash).ok().into_iter().flatten());
	}

	best
}

async fn has_ancestor_relay_parent_info<Block: BlockT>(
	relay_client: &impl RelayChainInterface,
	scheduling_parent: RelayHash,
	header: &Block::Header,
) -> RelayChainResult<bool> {
	let relay_parent = 'get_relay_parent: {
		let digest = header.digest();

		if let Some(relay_parent) = cumulus_primitives_core::extract_relay_parent(digest) {
			break 'get_relay_parent relay_parent;
		}

		if let Some((storage_root, number)) =
			cumulus_primitives_core::rpsr_digest::extract_relay_parent_storage_root(digest)
		{
			let Some(relay_parent_header) =
				relay_client.header(RelayBlockId::Number(number)).await?
			else {
				return Ok(false);
			};
			if relay_parent_header.state_root != storage_root {
				return Ok(false);
			}
			break 'get_relay_parent relay_parent_header.hash();
		}

		return Ok(false);
	};

	if relay_parent == scheduling_parent {
		return Ok(true);
	}

	let relay_parent_session = relay_client.session_index_for_child(relay_parent).await?;
	let Some(_info) = relay_client
		.ancestor_relay_parent_info(scheduling_parent, relay_parent_session, relay_parent)
		.await?
	else {
		return Ok(false);
	};

	Ok(true)
}

#[derive(Clone, Debug)]
pub enum ParentSearchParams<Block: BlockT> {
	V2 {
		/// The scheduling-parent that is intended to be used.
		/// For V2, the scheduling parent is equal to the relay parent.
		scheduling_parent: RelayHash,
	},
	V3 {
		/// The scheduling-parent that is intended to be used.
		scheduling_parent: RelayHash,
		para_best_hash: Block::Hash,
	},
}

impl<Block: BlockT> ParentSearchParams<Block> {
	fn scheduling_parent(&self) -> &RelayHash {
		match self {
			ParentSearchParams::V2 { scheduling_parent } => scheduling_parent,
			ParentSearchParams::V3 { scheduling_parent, .. } => scheduling_parent,
		}
	}
}

/// A potential parent block returned from [`find_parent_for_building`]
#[derive(PartialEq, Clone)]
pub struct ParentSearchResult<Block: BlockT> {
	/// The header of the included block (confirmed on relay chain).
	pub included_header: Block::Header,
	/// The header of the best parent block to build on.
	pub best_parent_header: Block::Header,
}

impl<B: BlockT> std::fmt::Debug for ParentSearchResult<B> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("ParentSearchResult")
			.field("included_number", &self.included_header.number())
			.field("best_parent_hash", &self.best_parent_header.hash())
			.field("best_parent_number", &self.best_parent_header.number())
			.finish()
	}
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
	params: ParentSearchParams<Block>,
) -> RelayChainResult<Option<ParentSearchResult<Block>>> {
	tracing::trace!(
		target: LOG_TARGET,
		?para_id,
		?params,
		"Parent search"
	);

	let scheduling_parent = *params.scheduling_parent();

	let Some(ParentSearchStart { included: included_header, start: (start_hash, start_header) }) =
		get_parent_search_start(relay_client, backend, para_id, scheduling_parent).await?
	else {
		return Ok(None);
	};

	match params {
		ParentSearchParams::V2 { scheduling_parent: relay_parent } => {
			let ancestry_lookback = relay_client
				.scheduling_lookahead(relay_parent)
				.await
				.unwrap_or(DEFAULT_SCHEDULING_LOOKAHEAD)
				.saturating_sub(1) as usize;
			// Build up the ancestry record of the relay chain to compare against.
			let rp_ancestry =
				build_relay_parent_ancestry(relay_client, relay_parent, ancestry_lookback).await?;

			// Search for the deepest valid parent starting from the pending/included block.
			let best_parent_header =
				find_deepest_valid_parent(backend, start_header, start_hash, &rp_ancestry);

			Ok(Some(ParentSearchResult { included_header, best_parent_header }))
		},
		ParentSearchParams::V3 { scheduling_parent, para_best_hash } => {
			let mut para_best_header = None;
			let mut current_hash = para_best_hash;
			let best_parent_header = loop {
				let Some(current_header) = get_para_header(backend, current_hash) else {
					break None;
				};
				if current_hash == para_best_hash {
					para_best_header = Some(current_header.clone());
				}

				if current_hash == para_best_hash ||
					*current_header.number() == start_header.number().add(One::one())
				{
					if !has_ancestor_relay_parent_info::<Block>(
						relay_client,
						scheduling_parent,
						&current_header,
					)
					.await?
					{
						break None;
					}
				}

				if current_hash == start_hash {
					break para_best_header;
				}

				if current_header.number() <= start_header.number() {
					break None;
				}

				current_hash = *current_header.parent_hash();
			};

			Ok(Some(ParentSearchResult {
				included_header,
				best_parent_header: best_parent_header.unwrap_or(start_header),
			}))
		},
	}
}
