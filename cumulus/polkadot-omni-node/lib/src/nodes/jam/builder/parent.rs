// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Which block a tick authors on, out of the blocks the local database holds below the
//! accumulated head.

use crate::{
	common::{types::ParachainBackend, NodeBlock},
	nodes::jam::LOG_TARGET,
};
use sc_client_api::Backend as _;
use sp_blockchain::{Backend as BlockchainBackend, HeaderBackend};
use sp_runtime::traits::Header as HeaderT;
use std::collections::VecDeque;

/// How long the accumulated head may stand still before the builder authors a sibling of it
/// instead of extending its own stuck branch.
///
/// A package this collator gave up on leaves a block nothing will ever accumulate, and every
/// block authored on top of it inherits that: the parachain service buffers the descendants, the
/// head never moves, and the runtime's capacity gate stops authoring a few blocks later. Losing a
/// package is benign and regular, so this has to fire; the bound sits comfortably above the
/// normal inclusion trail (about two slots) plus the collation task's resend window and the
/// authoring hold it can impose, so it only ever fires on a package that is genuinely lost.
pub(crate) const STALL_REROOT_SLOTS: u64 = 8;

/// One block the local database holds below the accumulated head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Descendant<Header> {
	pub(crate) header: Header,
	/// Generations below the accumulated head; `1` is a direct child of it.
	pub(crate) depth: usize,
}

/// The blocks the local database holds below `included`, breadth-first and depth-capped.
///
/// Everything the next block could possibly be built on is in here: the blocks this collator
/// authored (it imports them before submitting their packages) and the blocks other collators
/// announced and this node imported. Sibling forks are kept — several collators may have
/// authored children of the same block, and choosing between them is [`select_parent`]'s job.
///
/// The walk ends where the tree does. It needs no cap of its own: the runtime refuses to execute a
/// block that does not fit its unincluded segment, at authoring and at import alike, so no branch
/// below the accumulated head is deeper than the runtime's capacity.
pub(crate) fn local_descendants<Block: NodeBlock>(
	backend: &ParachainBackend<Block>,
	included: Block::Hash,
) -> Vec<Descendant<Block::Header>> {
	let blockchain = backend.blockchain();
	let mut frontier = vec![included];
	let mut descendants = Vec::new();
	for depth in 1.. {
		let mut next = Vec::new();
		for hash in frontier {
			let children = match blockchain.children(hash) {
				Ok(children) => children,
				Err(error) => {
					tracing::warn!(
						target: LOG_TARGET,
						?hash,
						?error,
						"Cannot list a block's children; the parent search stops on this branch.",
					);
					continue;
				},
			};
			for child in children {
				if let Ok(Some(header)) = blockchain.header(child) {
					descendants.push(Descendant { header, depth });
					next.push(child);
				}
			}
		}
		if next.is_empty() {
			break;
		}
		frontier = next;
	}
	descendants
}

/// The block to author on, out of what the local database holds below the accumulated head.
///
/// This is the whole of the builder's parent choice, and the order it applies is deterministic:
///
/// 1. **The deepest block wins.** That is what pipelining is — the collator keeps extending its own
///    unaccumulated tip instead of waiting for JAM to accept it.
/// 2. **At equal depth, a block this collator authored beats one imported from a peer.** Its
///    package is already in flight from here, and preferring it keeps consecutive ticks on one line
///    rather than hopping between siblings every slot.
/// 3. **Otherwise the first one seen**, which is the database's own order of children within a
///    generation, walked breadth-first by [`local_descendants`] — stable across ticks for as long
///    as the database does not change.
///
/// `None` means the local database holds nothing below the accumulated head, so the next block is
/// a direct child of it. What the tick actually authors on is [`choose_parent`], which overrides
/// this answer when the accumulated head has stood still for too long.
pub(crate) fn select_parent<'a, Header: HeaderT>(
	descendants: &'a [Descendant<Header>],
	ours: &VecDeque<Header::Hash>,
) -> Option<&'a Descendant<Header>> {
	let mut best: Option<&Descendant<Header>> = None;
	for candidate in descendants {
		let better = match best {
			None => true,
			Some(best) if candidate.depth != best.depth => candidate.depth > best.depth,
			Some(best) => {
				ours.contains(&candidate.header.hash()) && !ours.contains(&best.header.hash())
			},
		};
		if better {
			best = Some(candidate);
		}
	}
	best
}

/// Where the block being authored hangs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParentSource {
	/// The head JAM has accumulated, or genesis while there is none: a root block, because the
	/// local database holds nothing below that head.
	AccumulatedHead,
	/// The deepest block the local database holds below the accumulated head — the ordinary,
	/// pipelined case.
	Deepest,
	/// The accumulated head again, deliberately, because it has stood still too long: this
	/// authors a *sibling* of the block that is stuck, abandoning the branch above it.
	Reroot,
	/// Our own block on the branch an earlier tick re-rooted onto.
	Rerooted,
}

/// The block one tick authors on. `None` is the accumulated head itself.
///
/// Ordinarily this is [`select_parent`]'s answer. The two other cases are the way out of a stall:
///
/// - the head has not moved for [`STALL_REROOT_SLOTS`] para slots and there is a branch above it,
///   so the branch is abandoned and a sibling of the stuck block authored instead. Nothing else in
///   the builder gets past a package nobody will resubmit; accumulate's freshness check referees
///   between the two branches, and any of our newer blocks that can still apply wait in the
///   service's reorder buffer meanwhile.
/// - a previous tick already re-rooted, and this one stays on the branch it started (`committed`).
///   Without that the abandoned branch — still the deeper of the two — would be re-selected every
///   slot and a fresh sibling authored every slot, which extends nothing.
///
/// The stall check comes first on purpose: the clock is restarted when a re-root is authored, so
/// a branch that itself goes nowhere is abandoned in turn after another full window.
pub(crate) fn choose_parent<'a, Header: HeaderT>(
	descendants: &'a [Descendant<Header>],
	ours: &VecDeque<Header::Hash>,
	committed: Option<&'a Descendant<Header>>,
	stalled_for: u64,
) -> (Option<&'a Descendant<Header>>, ParentSource) {
	let selected = select_parent(descendants, ours);
	let depth = selected.map_or(0, |parent| parent.depth);
	// Nothing above the head means there is nothing to abandon: the next block is already its
	// child, and re-rooting would be a no-op.
	if stalled_for >= STALL_REROOT_SLOTS && depth > 0 {
		return (None, ParentSource::Reroot);
	}
	match (committed, selected) {
		(Some(parent), _) => (Some(parent), ParentSource::Rerooted),
		(None, Some(parent)) => (Some(parent), ParentSource::Deepest),
		(None, None) => (None, ParentSource::AccumulatedHead),
	}
}
