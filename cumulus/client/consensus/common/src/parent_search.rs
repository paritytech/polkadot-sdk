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
use polkadot_primitives::Hash as RelayHash;

use cumulus_primitives_core::{
	relay_chain::{BlockId as RBlockId, OccupiedCoreAssumption},
	ParaId,
};
use cumulus_relay_chain_interface::{RelayChainError, RelayChainInterface};

use sc_client_api::{Backend, HeaderBackend};

use sp_blockchain::Backend as BlockchainBackend;

use sp_runtime::traits::{Block as BlockT, Header as HeaderT};

const LOG_TARGET: &str = "consensus::common::parent_search";

/// Parameters when searching for suitable parents to build on top of.
#[derive(Debug)]
pub struct ParentSearchParams {
	/// The relay-parent that is intended to be used.
	pub relay_parent: RelayHash,
	/// The ID of the parachain.
	pub para_id: ParaId,
	/// A limitation on the age of relay parents for parachain blocks that are being
	/// considered. This is relative to the `relay_parent` number.
	pub ancestry_lookback: usize,
}

/// Result of the parent search, containing the included block and the best parent to build on.
pub struct ParentSearchResult<B: BlockT> {
	/// The header of the included block (confirmed on relay chain).
	pub included_header: B::Header,
	/// The header of the best parent block to build on.
	pub best_parent_header: B::Header,
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
pub async fn find_parent_for_building<B: BlockT>(
	params: ParentSearchParams,
	backend: &impl Backend<B>,
	relay_client: &impl RelayChainInterface,
) -> Result<Option<ParentSearchResult<B>>, RelayChainError> {
	tracing::trace!(target: LOG_TARGET, "Parent search parameters: {params:?}");

	// Get the included block.
	let Some((included_header, included_hash)) =
		fetch_included_from_relay_chain(relay_client, backend, params.para_id, params.relay_parent)
			.await?
	else {
		return Ok(None);
	};

	// Fetch the pending block if one exists.
	let maybe_pending = {
		// Fetch the most recent pending header from the relay chain. We use
		// `OccupiedCoreAssumption::Included` so the candidate pending availability gets enacted
		// before being returned to us.
		let pending_header = relay_client
			.persisted_validation_data(
				params.relay_parent,
				params.para_id,
				OccupiedCoreAssumption::Included,
			)
			.await?
			.and_then(|p| B::Header::decode(&mut &p.parent_head.0[..]).ok())
			.filter(|x| x.hash() != included_hash);

		// If the pending block is not locally known, we can't proceed.
		if let Some(header) = pending_header {
			let pending_hash = header.hash();
			let Ok(Some(header)) = backend.blockchain().header(pending_hash) else {
				tracing::warn!(
					target: LOG_TARGET,
					%pending_hash,
					"Failed to get header for pending block.",
				);
				return Ok(None);
			};
			Some((header, pending_hash))
		} else {
			None
		}
	};

	// Determine the starting point for the search.
	let (start_hash, start_header) = match &maybe_pending {
		Some((pending_header, pending_hash)) => {
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

	// Build up the ancestry record of the relay chain to compare against.
	let rp_ancestry =
		build_relay_parent_ancestry(params.ancestry_lookback, params.relay_parent, relay_client)
			.await?;

	// Search for the deepest valid parent starting from the pending/included block.
	let best_parent_header =
		find_deepest_valid_parent(start_hash, start_header, backend, &rp_ancestry);

	Ok(Some(ParentSearchResult { included_header, best_parent_header }))
}

/// Fetch the included block from the relay chain.
async fn fetch_included_from_relay_chain<B: BlockT>(
	relay_client: &impl RelayChainInterface,
	backend: &impl Backend<B>,
	para_id: ParaId,
	relay_parent: RelayHash,
) -> Result<Option<(B::Header, B::Hash)>, RelayChainError> {
	// Fetch the pending header from the relay chain. We use `OccupiedCoreAssumption::TimedOut`
	// so that even if there is a pending candidate, we assume it is timed out and we get the
	// included head.
	let included_header = relay_client
		.persisted_validation_data(relay_parent, para_id, OccupiedCoreAssumption::TimedOut)
		.await?;
	let included_header = match included_header {
		Some(pvd) => pvd.parent_head,
		None => return Ok(None), // this implies the para doesn't exist.
	};

	let included_header = match B::Header::decode(&mut &included_header.0[..]).ok() {
		None => return Ok(None),
		Some(x) => x,
	};

	let included_hash = included_header.hash();
	// If the included block is not locally known, we can't do anything.
	match backend.blockchain().header(included_hash) {
		Ok(None) | Err(_) => {
			tracing::warn!(
				target: LOG_TARGET,
				%included_hash,
				"Failed to get header for included block.",
			);
			return Ok(None);
		},
		_ => {},
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
	ancestry_lookback: usize,
	relay_parent: RelayHash,
	relay_client: &impl RelayChainInterface,
) -> Result<Vec<(RelayHash, RelayHash)>, RelayChainError> {
	let mut ancestry = Vec::with_capacity(ancestry_lookback + 1);
	let mut current_rp = relay_parent;
	let mut required_session = None;
	while ancestry.len() <= ancestry_lookback {
		let Some(header) = relay_client.header(RBlockId::hash(current_rp)).await? else { break };

		let session = relay_client.session_index_for_child(current_rp).await?;
		if required_session.get_or_insert(session) != &session {
			// Respect the relay-chain rule not to cross session boundaries.
			break;
		}

		ancestry.push((current_rp, *header.state_root()));
		current_rp = *header.parent_hash();

		// don't iterate back into the genesis block.
		if header.number == 1 {
			break;
		}
	}
	Ok(ancestry)
}

/// Find the deepest valid parent block starting from `start`.
///
/// The `start` block (pending or included) is always valid by construction.
/// This function explores its descendants via DFS, returning the deepest block
/// whose relay-parent is within the allowed ancestry.
fn find_deepest_valid_parent<Block: BlockT>(
	start_hash: Block::Hash,
	start_header: Block::Header,
	backend: &impl Backend<Block>,
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

/// Check if a block's relay parent is within the allowed ancestry.
fn is_relay_parent_in_ancestry<Block: BlockT>(
	header: &Block::Header,
	rp_ancestry: &[(RelayHash, RelayHash)],
) -> bool {
	let digest = header.digest();
	let relay_parent = cumulus_primitives_core::extract_relay_parent(digest);
	let storage_root =
		cumulus_primitives_core::rpsr_digest::extract_relay_parent_storage_root(digest)
			.map(|(r, _)| r);

	rp_ancestry.iter().any(|(rp_hash, rp_storage_root)| {
		relay_parent.map_or(false, |rp| *rp_hash == rp) ||
			storage_root.map_or(false, |sr| *rp_storage_root == sr)
	})
}
