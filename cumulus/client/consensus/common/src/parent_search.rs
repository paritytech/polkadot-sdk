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

use crate::relay_chain_data_cache::RelayChainDataCache;
use codec::Decode;
use cumulus_primitives_core::relay_chain::{BlockId as RelayBlockId, OccupiedCoreAssumption};
use cumulus_relay_chain_interface::{RelayChainError, RelayChainInterface, RelayChainResult};
use polkadot_primitives::{Block as RelayBlock, Hash as RelayHash, DEFAULT_SCHEDULING_LOOKAHEAD};
use sc_client_api::{Backend, HeaderBackend};
use sc_consensus_babe::contains_epoch_change;
use sp_blockchain::Backend as BlockchainBackend;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};

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
	fn scheduling_parent(&self) -> &RelayHash {
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

async fn fetch_pvd_header<Block: BlockT, RI: RelayChainInterface + 'static>(
	relay_chain_data: &mut RelayChainDataCache<RI>,
	at: RelayHash,
	occupied_core_assumption: OccupiedCoreAssumption,
) -> RelayChainResult<Option<Block::Header>> {
	let maybe_header = relay_chain_data
		.persisted_validation_data(at, occupied_core_assumption)
		.await?
		.and_then(|pvd| Block::Header::decode(&mut &pvd.parent_head.0[..]).ok());

	Ok(maybe_header)
}

/// Fetch the included block from the relay chain.
pub async fn fetch_included_from_relay_chain<B: BlockT, RI: RelayChainInterface + 'static>(
	relay_chain_data: &mut RelayChainDataCache<RI>,
	backend: &impl Backend<B>,
	at: RelayHash,
) -> Result<Option<(B::Header, B::Hash)>, RelayChainError> {
	// Fetch the pending header from the relay chain. We use `OccupiedCoreAssumption::TimedOut`
	// so that even if there is a pending candidate, we assume it is timed out, and we get the
	// included head.
	let Some(included_header) =
		fetch_pvd_header::<B, RI>(relay_chain_data, at, OccupiedCoreAssumption::TimedOut).await?
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
async fn build_relay_parent_ancestry<RI: RelayChainInterface + 'static>(
	relay_chain_data: &mut RelayChainDataCache<RI>,
	relay_parent: RelayHash,
	ancestry_lookback: usize,
) -> Result<Vec<(RelayHash, RelayHash)>, RelayChainError> {
	let mut ancestry = Vec::with_capacity(ancestry_lookback + 1);
	let mut current_rp = relay_parent;
	while ancestry.len() <= ancestry_lookback {
		let header = match relay_chain_data.header(current_rp).await {
			Ok(Some(h)) => h,
			Ok(None) => break,
			Err(e) => return Err(e),
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

/// How to decide whether a candidate parent block is valid to build on.
enum ParentValidity {
	/// V2: the block's relay parent must be within the precomputed relay-parent ancestry.
	WithinAncestry(Vec<(RelayHash, RelayHash)>),
	/// V3: the block's relay parent must be an allowed relay parent of `scheduling_parent`,
	/// resolved (and cached) via the relay-chain `ancestor_relay_parent_info` API.
	AllowedRelayParent { scheduling_parent: RelayHash },
}

impl ParentValidity {
	/// Whether `header` is a valid parent to build on under this validity rule.
	async fn is_valid<Block: BlockT, RI: RelayChainInterface + 'static>(
		&self,
		relay_chain_data: &mut RelayChainDataCache<RI>,
		header: &Block::Header,
	) -> bool {
		match self {
			ParentValidity::WithinAncestry(rp_ancestry) => {
				is_relay_parent_in_ancestry::<Block>(header, rp_ancestry)
			},
			ParentValidity::AllowedRelayParent { scheduling_parent } => {
				has_ancestor_relay_parent_info::<Block, RI>(
					relay_chain_data,
					*scheduling_parent,
					header,
				)
				.await
				.unwrap_or(false)
			},
		}
	}
}

/// Find the deepest valid parent block starting from `start`.
///
/// The `start` block (pending or included) is always valid by construction.
/// This function explores its descendants via DFS, returning the deepest block whose relay parent
/// is valid according to `validity`.
async fn find_deepest_valid_parent<Block: BlockT, RI: RelayChainInterface + 'static>(
	backend: &impl Backend<Block>,
	relay_chain_data: &mut RelayChainDataCache<RI>,
	start_header: Block::Header,
	start_hash: Block::Hash,
	validity: ParentValidity,
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

		if !validity.is_valid::<Block, RI>(relay_chain_data, &header).await {
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

async fn has_ancestor_relay_parent_info<Block: BlockT, RI: RelayChainInterface + 'static>(
	relay_chain_data: &mut RelayChainDataCache<RI>,
	scheduling_parent: RelayHash,
	header: &Block::Header,
) -> RelayChainResult<bool> {
	let Some(relay_parent) =
		get_relay_parent::<Block>(relay_chain_data.relay_client(), header).await?
	else {
		return Ok(false);
	};

	relay_chain_data.is_allowed_relay_parent(scheduling_parent, relay_parent).await
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
pub async fn find_parent_for_building<Block: BlockT, RI: RelayChainInterface + 'static>(
	relay_chain_data: &mut RelayChainDataCache<RI>,
	backend: &impl Backend<Block>,
	params: ParentSearchParams,
) -> RelayChainResult<Option<ParentSearchResult<Block>>> {
	tracing::trace!(
		target: LOG_TARGET,
		?params,
		"Parent search"
	);

	let scheduling_parent = *params.scheduling_parent();
	let Some((included_header, included_hash)) =
		fetch_included_from_relay_chain(relay_chain_data, backend, scheduling_parent).await?
	else {
		return Ok(None);
	};

	// Fetch the pending block if one exists.
	let maybe_pending = {
		// Fetch the most recent pending header from the relay chain. We use
		// `OccupiedCoreAssumption::Included` so the candidate pending availability gets enacted
		// before being returned to us.
		let maybe_header = fetch_pvd_header::<Block, RI>(
			relay_chain_data,
			scheduling_parent,
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

	let best_parent_header = match params {
		ParentSearchParams::V2 { scheduling_parent: relay_parent } => {
			let ancestry_lookback = relay_chain_data
				.scheduling_lookahead(relay_parent)
				.await
				.unwrap_or(DEFAULT_SCHEDULING_LOOKAHEAD)
				.saturating_sub(1) as usize;
			// Build up the ancestry record of the relay chain to compare against.
			let rp_ancestry =
				build_relay_parent_ancestry(relay_chain_data, relay_parent, ancestry_lookback)
					.await?;

			find_deepest_valid_parent(
				backend,
				relay_chain_data,
				start_header,
				start_hash,
				ParentValidity::WithinAncestry(rp_ancestry),
			)
			.await
		},
		ParentSearchParams::V3 { scheduling_parent } => {
			find_deepest_valid_parent(
				backend,
				relay_chain_data,
				start_header,
				start_hash,
				ParentValidity::AllowedRelayParent { scheduling_parent },
			)
			.await
		},
	};

	Ok(Some(ParentSearchResult { included_at_scheduling: included_header, best_parent_header }))
}
