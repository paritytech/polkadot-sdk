// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Module implementing the logic for verifying and importing AuRa blocks.

use std::{fmt::Debug, sync::Arc};

use codec::Codec;
use fork_tree::ForkTree;
use parking_lot::RwLock;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::{HeaderBackend, HeaderMetadata};
use sp_consensus_aura::{AuraApi, SlotDuration};
use sp_core::Pair;
use sp_runtime::traits::{Block, Header, NumberFor};

use crate::AuthorityId;

/// AURA slot duration tracker. Updates slot duration based on information from the runtime.
pub struct SlotDurationTracker<P, B: Block, C> {
	slot_durations: RwLock<ForkTree<B::Hash, NumberFor<B>, SlotDuration>>,
	client: Arc<C>,
	_phantom: std::marker::PhantomData<P>,
}

impl<P: Pair, B: Block, C> SlotDurationTracker<P, B, C>
where
	C: HeaderBackend<B> + HeaderMetadata<B, Error = sp_blockchain::Error> + ProvideRuntimeApi<B>,
	P::Public: Codec + Debug,
	C::Api: AuraApi<B, AuthorityId<P>>,
{
	// TODO: AURA API might be missing, so this will require the same lazy initialization mechanism
	// like the authorities tracker.
	// TODO: Implement that once Basti approves the previous PR, since I'm not sure if he's going to
	// like how the lazy initialization works or if he will want it changed.
	/// Create a new `SlotDurationTracker`.
	pub(super) fn new(client: Arc<C>) -> Result<Self, String> {
		let finalized_hash = client.info().finalized_hash;
		let mut slot_durations = ForkTree::new();
		for mut hash in client.leaves().map_err(|e| format!("Could not get leaf hashes: {e}"))? {
			// Import the entire chain back to the first imported ancestor, or to the last finalized
			// block if there is no imported ancestor. The chain must be imported in order, from
			// first block to last.
			let mut chain = Vec::new();
			// Limit the backtracking to 100 blocks, which should always be sufficient.
			while chain.len() < 100 {
				let header = client
					.header(hash)
					.map_err(|e| format!("Could not get header for {hash:?}: {e}"))?
					.ok_or_else(|| format!("Header for {hash:?} not found"))?;
				let number = *header.number();
				let is_descendent_of = sc_client_api::utils::is_descendent_of(&*client, None);
				let existing_node = slot_durations
					.find_node_where(&hash, &number, &is_descendent_of, &|_| true)
					.map_err(|e| {
						format!("Could not find slot duration for block {hash:?} at number {number}: {e}")
					})?;
				if existing_node.is_some() {
					// We have already imported this part of the chain.
					break;
				}
				chain.push((number, hash));
				if hash == finalized_hash {
					break;
				}
				hash = *header.parent_hash();
			}
			let mut last_imported_slot_duration = None;
			for (number, hash) in chain.into_iter().rev() {
				let slot_duration = client.runtime_api().slot_duration(hash).map_err(|e| {
					format!("Could not get slot duration from runtime at {hash:?}: {e}")
				})?;
				if Some(&slot_duration) != last_imported_slot_duration.as_ref() {
					last_imported_slot_duration = Some(slot_duration);
					let is_descendent_of = sc_client_api::utils::is_descendent_of(&*client, None);
					slot_durations.import(hash, number, slot_duration, &is_descendent_of).map_err(
						|e| {
							format!("Could not import slot duration for block {hash:?} at number {number}: {e}")
						},
					)?;
				}
			}
		}
		Ok(Self {
			slot_durations: RwLock::new(slot_durations),
			client,
			_phantom: std::marker::PhantomData,
		})
	}

	pub(super) fn new_empty(client: Arc<C>) -> Self {
		Self { slot_durations: RwLock::new(ForkTree::new()), client, _phantom: Default::default() }
	}
}

impl<P, B, C> SlotDurationTracker<P, B, C>
where
	P: Pair,
	B: Block,
	C: HeaderBackend<B> + HeaderMetadata<B, Error = sp_blockchain::Error> + ProvideRuntimeApi<B>,
	P::Public: Codec + Debug,
	C::Api: AuraApi<B, AuthorityId<P>>,
{
	/// Fetch the slot duration from the tracker, if available. If not available, return an error.
	pub fn fetch(&self, header: &B::Header) -> Result<Option<SlotDuration>, String> {
		let hash = header.hash();
		let number = *header.number();
		let parent_hash = *header.parent_hash();
		let is_descendent_of =
			sc_client_api::utils::is_descendent_of(&*self.client, Some((hash, parent_hash)));
		Ok(self
			.slot_durations
			.read()
			.find_node_where(&hash, &number, &is_descendent_of, &|_| true)
			.map_err(|e| {
				format!("Could not find slot duration for block {hash:?} at number {number}: {e}")
			})?
			.map(|n| n.data))
	}

	/// Import the slot duration from the runtime for the given header.
	pub fn import(
		&self,
		post_header: &B::Header,
		import: SlotDurationImport,
	) -> Result<(), String> {
		let hash = post_header.hash();
		let number = *post_header.number();
		let parent_hash = *post_header.parent_hash();
		let new_slot_duration = self
			.client
			.runtime_api()
			.slot_duration(hash)
			.map_err(|e| format!("Could not get slot duration from runtime at {hash}: {e}"))?;
		let result = match import {
			SlotDurationImport::WithParent => {
				self.prune_finalized()?;
				let current_slot_duration = self.fetch(post_header)?;
				if Some(&new_slot_duration) == current_slot_duration.as_ref() {
					// No change.
					return Ok(());
				}
				log::info!(
					"Slot duration changed from {} to {}ms",
					current_slot_duration
						.map(|d| format!("{}ms", d.as_millis()))
						.unwrap_or_else(|| "unknown".to_string()),
					new_slot_duration.as_millis(),
				);
				let is_descendent_of = sc_client_api::utils::is_descendent_of(
					&*self.client,
					Some((hash, parent_hash)),
				);
				self.slot_durations.write().import(
					hash,
					number,
					new_slot_duration,
					&is_descendent_of,
				)
			},
			SlotDurationImport::WithoutParent =>
				self.slot_durations.write().import(hash, number, new_slot_duration, &|_, _| {
					Ok::<_, sp_blockchain::Error>(true)
				}),
		};
		result.map_err(|e| {
			format!("Could not import slot duration for block {hash:?} at number {number}: {e}")
		})?;
		Ok(())
	}

	fn prune_finalized(&self) -> Result<(), String> {
		let is_descendent_of = sc_client_api::utils::is_descendent_of(&*self.client, None);
		let info = self.client.info();
		let _pruned = self
			.slot_durations
			.write()
			.prune(&info.finalized_hash, &info.finalized_number, &is_descendent_of, &|_| true)
			.map_err(|e| format!("Failed to prune finalized in slot duration tracker: {e:?}"))?;
		Ok(())
	}
}

/// How to import slot durations.
#[derive(Debug, Clone, Copy)]
pub enum SlotDurationImport {
	/// Assume that the parent of the current block is already imported.
	WithParent,
	/// Do not assume that the parent of the current block is already imported.
	WithoutParent,
}
