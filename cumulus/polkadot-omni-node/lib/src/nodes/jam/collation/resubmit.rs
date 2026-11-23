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

//! Resubmission rounds: lost blocks rebuilt into new work packages.

use crate::{
	common::{types::ParachainClient, ConstructNodeRuntimeApi, NodeBlock},
	nodes::jam::{
		chain::anchor::{read_anchor, AnchorReads},
		collation::{
			anchor_chain::header_chain_until,
			in_flight::InFlight,
			manager::Manager,
			policy::{original_package_expired, resubmission_due, MAX_RESUBMIT_BLOCKS},
		},
		package::{
			build_pov, extrinsic_spec,
			source::{record_submitted_hash, Anchored},
			work_package, PackageParams,
		},
		store::pov_store::BlockPov,
		LOG_TARGET,
	},
};
use cumulus_jam_primitives::anchor_proof::MAX_ANCHOR_CHAIN_LEN;
use jam_interface::{
	BlockDesc, HeaderHash, JamChainSource, JamStateSource, JamWorkPackageSubmission,
	Slot as JamSlot, WorkPackage, WorkPackageHash,
};
use jam_types::RefineContext;
use parachain_service_core::authorizer::Authorizer;
use sc_client_api::HeaderBackend;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, NumberFor};
use std::{collections::HashSet, sync::Arc};

/// The local best chain above `head`, oldest first: every block from the head's child up to
/// `best`. `None` when `head` is not an ancestor of `best` — nothing above the head is on the
/// best chain then, so a resubmission round has nothing to rebuild — or when `lookup` cannot
/// resolve a hash on the way.
pub(crate) fn chain_above<Header: HeaderT>(
	best: Header::Hash,
	(head_hash, head_number): (Header::Hash, Header::Number),
	lookup: impl Fn(Header::Hash) -> Option<Header>,
) -> Option<Vec<Header::Hash>> {
	let mut above = Vec::new();
	let mut current = best;
	loop {
		let header = lookup(current)?;
		if *header.number() <= head_number {
			break;
		}
		above.push(current);
		current = *header.parent_hash();
	}
	if current != head_hash {
		return None;
	}
	above.reverse();
	Some(above)
}

/// The blocks a round at `tip_slot` rebuilds, oldest first: the leading run of `above` — the
/// chain above the accumulated head — whose PoVs the store holds and whose original packages can
/// no longer be reported, at most `limit` long.
///
/// The run stops at the first block failing either test rather than skipping it: the packages
/// of a round chain by prerequisite, so a block after a gap would name a package for a block
/// the service cannot apply yet — and a block whose first package may still land must not be
/// rebuilt beside it.
pub(crate) fn rebuildable<Block: BlockT>(
	above: &[Block::Hash],
	tip_slot: JamSlot,
	pov_of: impl Fn(&Block::Hash) -> Option<BlockPov<Block>>,
	limit: usize,
) -> Vec<(Block::Hash, BlockPov<Block>)> {
	above
		.iter()
		.map_while(|hash| {
			let block_pov = pov_of(hash)?;
			original_package_expired(tip_slot, block_pov.jam_parent.anchor_slot)
				.then_some((*hash, block_pov))
		})
		.take(limit)
		.collect()
}

/// Assemble the package that rebuilds `block_pov` against `context`, unsigned, together with the
/// rebuilt PoV it is submitted with. The PoV carries the anchor chain `headers` the PVF verifies
/// the block's `JamParent` digest against.
///
/// `prerequisite` is the package the round submitted just before this one — package *k* > 0
/// names package *k* − 1 as its only prerequisite, so the guarantor processes the round in
/// order; package 0 names nothing. The token cannot be built here: it signs the finished
/// package, prerequisite included (`AuraAuthorizer::authorize` is the step after this one).
pub(crate) fn resubmission_package<Block: BlockT>(
	block_pov: &BlockPov<Block>,
	headers: &[Vec<u8>],
	prerequisite: Option<WorkPackageHash>,
	validation_code_hash: [u8; 32],
	params: &PackageParams,
	authorizer: Authorizer,
	context: &RefineContext,
) -> (WorkPackage, Vec<u8>) {
	let pov = build_pov(
		std::slice::from_ref(&block_pov.block),
		&block_pov.compact_proof,
		&block_pov.parent_header,
		&block_pov.additional_data,
		headers.to_vec(),
	);
	let context = RefineContext {
		prerequisites: prerequisite.into_iter().collect::<Vec<_>>().into(),
		..context.clone()
	};
	let package =
		work_package(vec![extrinsic_spec(&pov)], validation_code_hash, params, authorizer, context);
	(package, pov)
}

impl<Block, RuntimeApi, Jam> Manager<Block, RuntimeApi, Jam>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	Jam: JamChainSource + JamStateSource + JamWorkPackageSubmission + 'static,
{
	/// Check the resubmission predicate and, when it fires, run a resubmission round.
	///
	/// `last_round_at` moves only when a round actually starts — when the chain above the head
	/// begins with a block that can be rebuilt. While it does not (the first block's original
	/// package may still be reported, or the store has no PoV for it), the check is cheap enough
	/// to re-ask on every JAM block, and a block must not wait out a second stall window after
	/// the slot it becomes rebuildable in.
	pub(crate) async fn resubmit_if_due(&mut self, tip: BlockDesc) {
		let Some(head) = self.accumulated_head else { return };
		let has_unaccumulated_child = self.para_client.info().best_number > head.1;
		// The turn at the tip belongs to whoever the set JAM authorizes at the tip's slot names;
		// the set is the tip's, not the accumulated head's.
		let authorizer = match self.authorizer_for(self.para_client.info().best_hash, tip.slot) {
			Ok(authorizer) => authorizer,
			Err(error) => {
				tracing::debug!(
					target: LOG_TARGET,
					?tip,
					error,
					"Resubmission is not this node's turn at the tip's anchor slot.",
				);
				return;
			},
		};
		let our_turn = authorizer.names_us(tip.slot);
		let round_in_flight = self.resubmissions_pending() > 0;

		if !resubmission_due(
			tip.slot,
			self.head_changed_at,
			self.last_round_at,
			has_unaccumulated_child,
			our_turn,
			round_in_flight,
		) {
			return;
		}

		let above = self.chain_above_head(head);
		let blocks =
			rebuildable(&above, tip.slot, |hash| self.block_povs.get(hash), MAX_RESUBMIT_BLOCKS);
		if blocks.is_empty() {
			tracing::debug!(
				target: LOG_TARGET,
				?tip,
				head = ?head.0,
				head_number = %head.1,
				above_head = above.len(),
				"Resubmission is due, but nothing above the accumulated head is rebuildable yet: \
				 the first block there has no stored PoV, or its original package can still be \
				 reported.",
			);
			return;
		}
		self.last_round_at = Some(tip.slot);
		self.resubmit_round(tip, blocks).await;
	}

	/// One resubmission round: rebuild `blocks` — oldest first, each with a stored PoV and an
	/// expired original package — into new work packages anchored at `tip`, chained by
	/// prerequisites in that order.
	///
	/// Any failure ends the round instead of skipping the block it hit: the packages chain, so
	/// a block skipped in the middle would leave every later one naming a prerequisite for a
	/// block the service cannot apply yet. What was submitted before the failure stands, and the
	/// next round picks up where this one stopped.
	async fn resubmit_round(
		&mut self,
		tip: BlockDesc,
		blocks: Vec<(Block::Hash, BlockPov<Block>)>,
	) {
		// A fresh anchor context from the current tip; the round builds nothing on the para head,
		// so it is not read.
		let reads = read_anchor::<Block, _>(
			&*self.jam,
			tip,
			self.params.service_id,
			self.params.para_id.into(),
			&*self.authorizers,
			self.para_client.info().best_hash,
			false,
		)
		.await;
		let AnchorReads {
			anchor: BlockDesc { slot: anchor_slot, .. },
			context: anchor_context,
			pool_scan,
			authorizer,
			..
		} = match reads {
			Ok(Some(reads)) => reads,
			Ok(None) => {
				tracing::warn!(
					target: LOG_TARGET,
					?tip,
					"Resubmission round skipped: no suitable lookup anchor, or the anchor slot \
					 names another collator.",
				);
				return;
			},
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					?tip,
					error,
					"Resubmission round skipped: anchor reads failed.",
				);
				return;
			},
		};
		// A package nothing can be sent to would only hold the builder: it would count as a round
		// in flight until its anchor expired.
		let Some(core) = pool_scan.target else {
			tracing::warn!(
				target: LOG_TARGET,
				?tip,
				authorizer_hash = ?authorizer.hash(),
				blocks = blocks.len(),
				"Resubmission round skipped: no core holds this para's authorizer at the tip, so \
				 nothing could be submitted.",
			);
			return;
		};

		// One header walk for the whole round: the chain has to reach every block's anchor and
		// lookup anchor, and each package gets the prefix down to the oldest hash its block needs.
		let anchors_of = |block_pov: &BlockPov<Block>| {
			[
				HeaderHash(block_pov.jam_parent.anchor.0),
				HeaderHash(block_pov.jam_parent.lookup_anchor.0),
			]
		};
		let needed: HashSet<HeaderHash> =
			blocks.iter().flat_map(|(_, block_pov)| anchors_of(block_pov)).collect();
		let jam = Arc::clone(&self.jam);
		let walk = header_chain_until(tip.header_hash, needed, |hash| {
			let jam = Arc::clone(&jam);
			async move { jam.header(hash).await.map_err(|error| format!("header({hash:?}): {error}")) }
		})
		.await;
		let chain = match walk {
			Ok(chain) if chain.missing.is_empty() => chain,
			Ok(chain) => {
				tracing::warn!(
					target: LOG_TARGET,
					?tip,
					missing = ?chain.missing,
					headers_walked = chain.headers.len(),
					max_headers = MAX_ANCHOR_CHAIN_LEN,
					"Resubmission round skipped: the header chain from the tip did not reach every \
					 anchor the blocks were built against.",
				);
				return;
			},
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					?tip,
					error,
					"Resubmission round skipped: the header chain walk failed.",
				);
				return;
			},
		};

		let mut prerequisite: Option<WorkPackageHash> = None;
		let mut submitted = 0usize;

		for (block_hash, block_pov) in &blocks {
			let Some(headers) = chain.prefix_reaching(&anchors_of(block_pov)) else {
				// The walk above saw every hash in `needed`, so this cannot happen.
				tracing::warn!(
					target: LOG_TARGET,
					?block_hash,
					"Resubmission round stopped: the header chain does not reach the block's \
					 anchors.",
				);
				break;
			};

			// The parent's `:code`, hashed exactly as `on_new_block` hashes it for a fresh block,
			// a marker there resolved first.
			self.ensure_code_blob_at(block_pov.parent_hash, tip.header_hash).await;
			let validation_code_hash = match self.para_client.code_at(block_pov.parent_hash) {
				Ok(code) => sp_crypto_hashing::blake2_256(&code),
				Err(error) => {
					tracing::warn!(
						target: LOG_TARGET,
						?block_hash,
						parent_hash = ?block_pov.parent_hash,
						?error,
						"Resubmission round stopped: failed to read the validation code.",
					);
					break;
				},
			};

			let (mut package, pov) = resubmission_package(
				block_pov,
				headers,
				prerequisite,
				validation_code_hash,
				&self.params,
				authorizer.authorizer(),
				&anchor_context,
			);
			if let Err(error) = authorizer.authorize(&mut package) {
				tracing::warn!(
					target: LOG_TARGET,
					?block_hash,
					error,
					"Resubmission round stopped: failed to authorize the package.",
				);
				break;
			}

			let wp_hash = record_submitted_hash(&self.hash_ledger, (*block_hash).into(), &package);
			let anchored = Anchored {
				context: package.context.clone(),
				anchor_slot,
				submit_target: Some(core),
			};

			tracing::info!(
				target: LOG_TARGET,
				?block_hash,
				block_number = %block_pov.number,
				?wp_hash,
				?prerequisite,
				core,
				anchor = ?anchored.context.anchor,
				anchor_slot,
				lookup_anchor = ?anchored.context.lookup_anchor,
				lookup_anchor_slot = anchored.context.lookup_anchor_slot,
				original_anchor_slot = block_pov.jam_parent.anchor_slot,
				headers = headers.len(),
				"Resubmitting a parachain block in a new work package.",
			);

			self.packages.entries.push_back(InFlight {
				block_hash: *block_hash,
				block_number: block_pov.number,
				parent_hash: block_pov.parent_hash,
				wp_hash,
				package,
				anchored,
				reported: false,
				resubmission: true,
				sends: 0,
				unsent: Some(pov),
			});
			prerequisite = Some(wp_hash);
			submitted += 1;
		}

		self.send_due().await;
		tracing::info!(
			target: LOG_TARGET,
			submitted,
			total_blocks = blocks.len(),
			"Resubmission round complete; pacing releases its packages one per JAM slot.",
		);
		if submitted > 0 {
			self.log_state("a resubmission round completed");
		}
	}

	/// The local best chain above the accumulated head, oldest first; see [`chain_above`].
	fn chain_above_head(&self, head: (Block::Hash, NumberFor<Block>)) -> Vec<Block::Hash> {
		let best = self.para_client.info().best_hash;
		chain_above(best, head, |hash| match self.para_client.header(hash) {
			Ok(header) => header,
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					?hash,
					?error,
					"Unable to read a header while walking the chain above the accumulated head.",
				);
				None
			},
		})
		.unwrap_or_default()
	}
}
