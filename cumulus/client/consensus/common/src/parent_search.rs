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

use sp_blockchain::{Backend as BlockchainBackend, TreeRoute};

use sp_runtime::traits::{Block as BlockT, Header as HeaderT};

const PARENT_SEARCH_LOG_TARGET: &str = "consensus::common::find_potential_parents";

/// Parameters when searching for suitable parents to build on top of.
#[derive(Debug)]
pub struct ParentSearchParams {
	/// The best known relay chain block. Must be a descendant of the intended relay parent.
	pub relay_best_block: RelayHash,
	/// The ID of the parachain.
	pub para_id: ParaId,
	/// A limitation on the age of relay parents for parachain blocks that are being
	/// considered. This is relative to the `relay_parent` number.
	pub ancestry_lookback: usize,
	/// How "deep" parents can be relative to the included parachain block at the relay-parent.
	/// The included block has depth 0.
	pub max_depth: usize,
}

/// A potential parent block returned from [`find_potential_parents`]
#[derive(PartialEq)]
pub struct PotentialParent<B: BlockT> {
	/// The hash of the block.
	pub hash: B::Hash,
	/// The header of the block.
	pub header: B::Header,
	/// The depth of the block with respect to the included block.
	pub depth: usize,
}

impl<B: BlockT> std::fmt::Debug for PotentialParent<B> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("PotentialParent")
			.field("hash", &self.hash)
			.field("depth", &self.depth)
			.field("number", &self.header.number())
			.finish()
	}
}

/// Perform a recursive search through blocks to find potential
/// parent blocks for a new block.
///
/// This accepts a relay-chain block to be used as an anchor and a maximum search depth,
/// along with some arguments for filtering parachain blocks and performs a recursive search
/// for parachain blocks. The search begins at the last included parachain block and returns
/// a set of [`PotentialParent`]s which could be potential parents of a new block with this
/// relay-parent according to the search parameters.
///
/// A parachain block is a potential parent if it is either the last included parachain block, the
/// pending parachain block (when `max_depth` >= 1), or all of the following hold:
///   * its parent is a potential parent
///   * its relay-parent is within `ancestry_lookback` of the targeted relay-parent.
///   * its relay-parent is within the same session as the targeted relay-parent.
///   * the block number is within `max_depth` blocks of the included block
pub async fn find_potential_parents<B: BlockT>(
	params: ParentSearchParams,
	backend: &impl Backend<B>,
	relay_client: &impl RelayChainInterface,
) -> Result<Vec<PotentialParent<B>>, RelayChainError> {
	tracing::trace!("Parent search parameters: {params:?}");
	// Get the included block.
	let Some((included_header, included_hash)) = fetch_included_from_relay_chain(
		relay_client,
		backend,
		params.para_id,
		params.relay_best_block,
	)
	.await?
	else {
		return Ok(Default::default())
	};

	let only_included = vec![PotentialParent {
		hash: included_hash,
		header: included_header.clone(),
		depth: 0,
	}];

	if params.max_depth == 0 {
		return Ok(only_included)
	};

	// Pending header and hash.
	let maybe_pending = {
		// Fetch the most recent pending header from the relay chain. We use
		// `OccupiedCoreAssumption::Included` so the candidate pending availability gets enacted
		// before being returned to us.
		let pending_header = relay_client
			.persisted_validation_data(
				params.relay_best_block,
				params.para_id,
				OccupiedCoreAssumption::Included,
			)
			.await?
			.and_then(|p| B::Header::decode(&mut &p.parent_head.0[..]).ok())
			.filter(|x| x.hash() != included_hash);

		// If the pending block is not locally known, we can't do anything.
		if let Some(header) = pending_header {
			let pending_hash = header.hash();
			match backend.blockchain().header(pending_hash) {
				// We only respect branches that contain the pending block, but we
				// do not know the pending block locally.
				Ok(None) | Err(_) => {
					tracing::warn!(
						target: PARENT_SEARCH_LOG_TARGET,
						%pending_hash,
						"Failed to get header for pending block.",
					);
					return Ok(Default::default())
				},
				Ok(Some(_)) => Some((header, pending_hash)),
			}
		} else {
			None
		}
	};

	let maybe_route_to_last_pending = maybe_pending
		.as_ref()
		.map(|(_, pending)| {
			sp_blockchain::tree_route(backend.blockchain(), included_hash, *pending)
		})
		.transpose()?;

	// Since we only respect branches that contain the pending block, there is no reason to start
	// the parent search at the included block when a pending block exists. We can add the included
	// block and the path to the pending block to the potential parents directly (limited by
	// max_depth).
	let (frontier, potential_parents) = match (&maybe_pending, &maybe_route_to_last_pending) {
		(Some((pending_header, pending_hash)), Some(ref route_to_pending)) => {
			let mut potential_parents = only_included;

			// This is a defensive check, should never happen.
			if !route_to_pending.retracted().is_empty() {
				tracing::warn!(target: PARENT_SEARCH_LOG_TARGET, "Included block not an ancestor of pending block. This should not happen.");
				return Ok(Default::default())
			}

			// Add all items on the path included -> pending - 1 to the potential parents, but
			// not more than `max_depth`.
			let pending_depth = route_to_pending.enacted().len();
			let num_parents_on_path = pending_depth.saturating_sub(1).min(params.max_depth);

			for (num, block) in
				route_to_pending.enacted().iter().take(num_parents_on_path).enumerate()
			{
				let Ok(Some(header)) = backend.blockchain().header(block.hash) else { continue };

				potential_parents.push(PotentialParent {
					hash: block.hash,
					header,
					depth: 1 + num,
				});
			}

			// The frontier contains blocks whose children we want to explore.
			// We put the pending block in the frontier so search_child_branches_for_parents
			// will validate it and explore its children.
			let frontier = if pending_depth <= params.max_depth {
				vec![PotentialParent {
					hash: *pending_hash,
					header: pending_header.clone(),
					depth: pending_depth,
				}]
			} else {
				vec![]
			};

			(frontier, potential_parents)
		},
		_ => (only_included, Default::default()),
	};

	if potential_parents.len() > params.max_depth {
		return Ok(potential_parents);
	}

	// Build up the ancestry record of the relay chain to compare against.
	let rp_ancestry = build_relay_parent_ancestry(
		params.ancestry_lookback,
		params.relay_best_block,
		relay_client,
	)
	.await?;

	Ok(search_child_branches_for_parents(
		frontier,
		maybe_route_to_last_pending,
		included_header,
		maybe_pending.map(|(_, hash)| hash),
		backend,
		params.max_depth,
		rp_ancestry,
		potential_parents,
	))
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
		Ok(None) => {
			tracing::warn!(
				target: PARENT_SEARCH_LOG_TARGET,
				%included_hash,
				"Failed to get header for included block.",
			);
			return Ok(None)
		},
		Err(e) => {
			tracing::warn!(
				target: PARENT_SEARCH_LOG_TARGET,
				%included_hash,
				%e,
				"Failed to get header for included block.",
			);
			return Ok(None)
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
			break
		}
	}
	Ok(ancestry)
}

/// Start search for child blocks that can be used as parents.
///
/// This function only respects branches that contain the pending block.
///
/// The frontier is initialized with either the pending block (if it exists and is within max_depth)
/// or the included block (if there's no pending block). This function validates blocks from the
/// frontier and explores their children, ensuring all blocks are aligned with the pending block.
pub fn search_child_branches_for_parents<Block: BlockT>(
	mut frontier: Vec<PotentialParent<Block>>,
	maybe_route_to_last_pending: Option<TreeRoute<Block>>,
	included_header: Block::Header,
	pending_hash: Option<Block::Hash>,
	backend: &impl Backend<Block>,
	max_depth: usize,
	rp_ancestry: Vec<(RelayHash, RelayHash)>,
	mut potential_parents: Vec<PotentialParent<Block>>,
) -> Vec<PotentialParent<Block>> {
	let included_hash = included_header.hash();
	let is_hash_in_ancestry = |hash| rp_ancestry.iter().any(|x| x.0 == hash);
	let is_root_in_ancestry = |root| rp_ancestry.iter().any(|x| x.1 == root);

	// The distance between pending and included block. Is later used to check if a child
	// is aligned with pending when it is between pending and included block.
	let pending_distance = maybe_route_to_last_pending.as_ref().map(|route| route.enacted().len());

	// If a block is on the path included -> pending, we consider it `aligned_with_pending`.
	let is_child_pending = |hash| {
		maybe_route_to_last_pending
			.as_ref()
			.map_or(true, |route| route.enacted().iter().any(|x| x.hash == hash))
	};

	tracing::trace!(
		target: PARENT_SEARCH_LOG_TARGET,
		?included_hash,
		included_num = ?included_header.number(),
		?pending_hash ,
		?rp_ancestry,
		"Searching relay chain ancestry."
	);
	while let Some(entry) = frontier.pop() {
		let is_pending = pending_hash.as_ref().map_or(false, |h| &entry.hash == h);
		let is_included = included_hash == entry.hash;

		// The frontier is initialized with either the included block (no pending) or the pending
		// block. Both are always potential parents because they're already posted on chain.
		// For other blocks, check if their relay parent is in the acceptable ancestry.
		let is_potential = is_pending || is_included || {
			let digest = entry.header.digest();
			let is_hash_in_ancestry_check = cumulus_primitives_core::extract_relay_parent(digest)
				.map_or(false, is_hash_in_ancestry);
			let is_root_in_ancestry_check =
				cumulus_primitives_core::rpsr_digest::extract_relay_parent_storage_root(digest)
					.map(|(r, _n)| r)
					.map_or(false, is_root_in_ancestry);

			is_hash_in_ancestry_check || is_root_in_ancestry_check
		};

		let child_depth = entry.depth + 1;
		let hash = entry.hash;

		tracing::trace!(
			target: PARENT_SEARCH_LOG_TARGET,
			?hash,
			is_potential,
			is_pending,
			is_included,
			"Checking potential parent."
		);

		if is_potential {
			potential_parents.push(entry);
		}

		if !is_potential || child_depth > max_depth {
			continue
		}

		// push children onto search frontier.
		for child in backend.blockchain().children(hash).ok().into_iter().flatten() {
			tracing::trace!(target: PARENT_SEARCH_LOG_TARGET, ?child, child_depth, ?pending_distance, "Looking at child.");

			let aligned_with_pending =
				pending_distance.map_or(true, |dist| child_depth > dist) ||
					is_child_pending(child);

			// We only respect branches that contain the pending block.
			if !aligned_with_pending {
				tracing::trace!(target: PARENT_SEARCH_LOG_TARGET, ?child, "Child is not aligned with pending block.");
				continue
			}

			let Ok(Some(header)) = backend.blockchain().header(child) else { continue };

			frontier.push(PotentialParent {
				hash: child,
				header,
				depth: child_depth,
			});
		}
	}

	potential_parents
}
