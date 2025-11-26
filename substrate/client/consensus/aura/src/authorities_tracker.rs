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
use sp_consensus_aura::{digests::CompatibleDigestItem, AuraApi};
use sp_core::Pair;
use sp_runtime::{
	traits::{Block, Header, NumberFor},
	DigestItem,
};

use crate::{fetch_authorities_from_runtime, AuthorityId, CompatibilityMode};

const LOG_TARGET: &str = "aura::authorities_tracker";

/// AURA authorities tracker. Updates authorities based on the AURA authorities change
/// digest in the block header.
pub struct AuthoritiesTracker<P: Pair, B: Block, C> {
	authorities: RwLock<ForkTree<B::Hash, NumberFor<B>, Vec<AuthorityId<P>>>>,
	client: Arc<C>,
}

impl<P: Pair, B: Block, C> AuthoritiesTracker<P, B, C>
where
	C: HeaderBackend<B> + HeaderMetadata<B, Error = sp_blockchain::Error> + ProvideRuntimeApi<B>,
	P::Public: Codec + Debug,
	C::Api: AuraApi<B, AuthorityId<P>>,
{
	/// Create a new `AuthoritiesTracker`.
	pub(crate) fn new(
		client: Arc<C>,
		compatibility_mode: &CompatibilityMode<NumberFor<B>>,
	) -> Result<Self, String> {
		let finalized_hash = client.info().finalized_hash;
		let mut authorities_cache = ForkTree::new();
		for mut hash in
			client.leaves().map_err(|e| format!("Could not get leaf hashes: {e}"))?
		{
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
				let existing_node =
					authorities_cache
						.find_node_where(&hash, &number, &is_descendent_of, &|_| true)
						.map_err(|e| {
							format!("Could not find authorities for block {hash:?} at number {number}: {e}")
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
			let mut last_imported_authorities = None;
			for (number, hash) in chain.into_iter().rev() {
				let authorities =
					fetch_authorities_from_runtime(&*client, hash, number, compatibility_mode)
						.map_err(|e| format!("Could not fetch authorities at {hash:?}: {e}"))?;
				if Some(&authorities) != last_imported_authorities.as_ref() {
					last_imported_authorities = Some(authorities.clone());
					Self::import_authorities(
						&mut authorities_cache,
						&client,
						None,
						hash,
						number,
						authorities,
					)?;
				}
			}
		}
		Ok(Self { authorities: RwLock::new(authorities_cache), client })
	}

	/// Create a new empty [`AuthoritiesTracker`]. Usually you should _not_ use this method,
	/// as it will not have any initial authorities imported. Use [`AuthoritiesTracker::new`]
	/// instead.
	pub(crate) fn new_empty(client: Arc<C>) -> Self {
		Self { authorities: RwLock::new(ForkTree::new()), client }
	}

	fn import_authorities(
		cache: &mut ForkTree<B::Hash, NumberFor<B>, Vec<AuthorityId<P>>>,
		client: &C,
		current: Option<(B::Hash, B::Hash)>,
		hash: B::Hash,
		number: NumberFor<B>,
		authorities: Vec<AuthorityId<P>>,
	) -> Result<(), String> {
		let is_descendent_of = sc_client_api::utils::is_descendent_of(client, current);
		cache.import(hash, number, authorities, &is_descendent_of).map_err(|e| {
			format!("Could not import authorities for block {hash:?} at number {number}: {e}")
		})?;
		Ok(())
	}
}

impl<P, B, C> AuthoritiesTracker<P, B, C>
where
	P: Pair,
	B: Block,
	C: HeaderBackend<B> + HeaderMetadata<B, Error = sp_blockchain::Error> + ProvideRuntimeApi<B>,
	P::Public: Codec + Debug,
	P::Signature: Codec,
	C::Api: AuraApi<B, AuthorityId<P>>,
{
	/// Fetch authorities from the tracker, if available. If not available, return an error.
	pub fn fetch(&self, header: &B::Header) -> Result<Vec<AuthorityId<P>>, String> {
		let hash = header.hash();
		let number = *header.number();
		let parent_hash = *header.parent_hash();
		let is_descendent_of =
			sc_client_api::utils::is_descendent_of(&*self.client, Some((hash, parent_hash)));
		let authorities_cache = self.authorities.read();
		let node = authorities_cache
			.find_node_where(&hash, &number, &is_descendent_of, &|_| true)
			.map_err(|e| {
				format!("Could not find authorities for block {hash:?} at number {number}: {e}")
			})?
			.ok_or_else(|| {
				format!("Authorities for block {hash:?} at number {number} not found in",)
			})?;
		Ok(node.data.clone())
	}

	/// If there is an authorities change digest in the header, import it into the tracker.
	pub fn import_from_block(&self, post_header: &B::Header) -> Result<(), String> {
		if let Some(authorities_change) = find_authorities_change_digest::<B, P>(&post_header) {
			let hash = post_header.hash();
			let parent_hash = *post_header.parent_hash();
			let number = *post_header.number();
			log::debug!(
				target: LOG_TARGET,
				"Importing authorities change for block {:?} at number {} found in header digest",
				hash,
				number,
			);
			self.prune_finalized()?;
			let mut authorities_cache = self.authorities.write();
			Self::import_authorities(
				&mut authorities_cache,
				&self.client,
				Some((hash, parent_hash)),
				hash,
				number,
				authorities_change,
			)?;
		}
		Ok(())
	}

	/// Import the authorities change for the given header from the runtime.
	pub fn import_from_runtime(&self, post_header: &B::Header) -> Result<(), String> {
		let hash = post_header.hash();
		let number = *post_header.number();

		let authorities =
			fetch_authorities_from_runtime(&*self.client, hash, number, &CompatibilityMode::None)
				.map_err(|e| format!("Could not fetch authorities: {e:?}"))?;

		self.authorities
			.write()
			.import(hash, number, authorities, &|_, _| {
				Ok::<_, fork_tree::Error<sp_blockchain::Error>>(true)
			})
			.map_err(|e| {
				format!("Could not import authorities for block {hash:?} at number {number}: {e}")
			})?;

		Ok(())
	}

	/// Returns true if there are no authorities stored in the tracker.
	pub fn is_empty(&self) -> bool {
		self.authorities.read().is_empty()
	}

	fn prune_finalized(&self) -> Result<(), String> {
		let is_descendent_of = sc_client_api::utils::is_descendent_of(&*self.client, None);
		let info = self.client.info();
		let mut authorities_cache = self.authorities.write();
		let _pruned = authorities_cache
			.prune(&info.finalized_hash, &info.finalized_number, &is_descendent_of, &|_| true)
			.map_err(|e| e.to_string())?;
		Ok(())
	}
}

/// Extract the AURA authorities change digest from the given header, if it exists.
fn find_authorities_change_digest<B, P>(header: &B::Header) -> Option<Vec<AuthorityId<P>>>
where
	B: Block,
	P: Pair,
	P::Public: Codec,
	P::Signature: Codec,
{
	header.digest().convert_first(|log| -> Option<Vec<AuthorityId<P>>> {
		log::trace!(target: LOG_TARGET, "Checking log {:?}, looking for authorities change digest.", log);
		<DigestItem as CompatibleDigestItem<P::Signature>>::as_authorities_change::<AuthorityId<P>>(
			log,
		)
	})
}
