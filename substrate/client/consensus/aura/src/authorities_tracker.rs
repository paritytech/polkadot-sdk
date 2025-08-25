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
use sp_consensus_aura::{AuraApi, ConsensusLog, AURA_ENGINE_ID};
use sp_core::Pair;
use sp_runtime::{
	generic::OpaqueDigestItemId,
	traits::{Block, Header, NumberFor},
};

use crate::{fetch_authorities_from_runtime, AuthorityId, CompatibilityMode};

const LOG_TARGET: &str = "aura::authorities_tracker";

/// AURA authorities tracker. Updates authorities based on the AURA authorities change
/// digest in the block header.
pub struct AuthoritiesTracker<P: Pair, B: Block, C> {
	authorities: RwLock<ForkTree<B::Hash, NumberFor<B>, Vec<AuthorityId<P>>>>,
	client: Arc<C>,
}

impl<P: Pair, B: Block, C> AuthoritiesTracker<P, B, C> {
	/// Create a new `AuthoritiesTracker`.
	pub fn new(client: Arc<C>) -> Self {
		Self { authorities: RwLock::new(ForkTree::new()), client }
	}
}

impl<P, B, C> AuthoritiesTracker<P, B, C>
where
	P: Pair,
	B: Block,
	C: HeaderBackend<B> + HeaderMetadata<B, Error = sp_blockchain::Error> + ProvideRuntimeApi<B>,
	P::Public: Codec + Debug,
	C::Api: AuraApi<B, AuthorityId<P>>,
{
	/// Fetch authorities from the tracker, if available. If not available, fetch from the client
	/// and update the tracker.
	pub fn fetch_or_update(
		&self,
		header: &B::Header,
		compatibility_mode: &CompatibilityMode<NumberFor<B>>,
	) -> Result<Vec<AuthorityId<P>>, String> {
		let hash = header.hash();
		let number = *header.number();
		let parent_hash = *header.parent_hash();

		// Fetch authorities from cache, if available.
		let authorities = {
			let is_descendent_of =
				sc_client_api::utils::is_descendent_of(&*self.client, Some((hash, parent_hash)));
			let authorities_cache = self.authorities.read();
			authorities_cache
				.find_node_where(&hash, &number, &is_descendent_of, &|_| true)
				.map_err(|e| {
					format!("Could not find authorities for block {hash:?} at number {number}: {e}")
				})?
				.map(|node| node.data.clone())
		};

		match authorities {
			Some(authorities) => {
				log::debug!(
					target: LOG_TARGET,
					"Authorities for block {:?} at number {} found in cache",
					hash,
					number,
				);
				Ok(authorities)
			},
			None => {
				// Authorities are missing from the cache. Fetch them from the runtime and cache
				// them.
				log::debug!(
					target: LOG_TARGET,
					"Authorities for block {:?} at number {} not found in cache, fetching from runtime",
					hash,
					number
				);
				let authorities = fetch_authorities_from_runtime(
					&*self.client,
					parent_hash,
					number,
					compatibility_mode,
				)
				.map_err(|e| format!("Could not fetch authorities at {:?}: {}", parent_hash, e))?;
				let is_descendent_of = sc_client_api::utils::is_descendent_of(&*self.client, None);
				let mut authorities_cache = self.authorities.write();
				authorities_cache
					.import(
						parent_hash,
						number - 1u32.into(),
						authorities.clone(),
						&is_descendent_of,
					)
					.map_err(|e| {
						format!("Could not import authorities for block {parent_hash:?} at number {}: {e}", number - 1u32.into())
					})?;
				Ok(authorities)
			},
		}
	}

	/// If there is an authorities change digest in the header, import it into the tracker.
	pub fn import(&self, header: &B::Header) -> Result<(), String> {
		if let Some(authorities_change) = find_authorities_change_digest::<B, P>(header) {
			let hash = header.hash();
			let number = *header.number();
			log::debug!(
				target: LOG_TARGET,
				"Importing authorities change for block {:?} at number {} found in header digest",
				hash,
				number,
			);
			self.prune_finalized()?;
			let is_descendent_of = sc_client_api::utils::is_descendent_of(&*self.client, None);
			let mut authorities_cache = self.authorities.write();
			authorities_cache
				.import(hash, number, authorities_change, &is_descendent_of)
				.map_err(|e| {
					format!(
						"Could not import authorities for block {hash:?} at number {number}: {e}"
					)
				})?;
		}
		Ok(())
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
{
	for log in header.digest().logs() {
		log::trace!(target: LOG_TARGET, "Checking log {:?}, looking for authorities change digest.", log);
		let log = log
			.try_to::<ConsensusLog<AuthorityId<P>>>(OpaqueDigestItemId::Consensus(&AURA_ENGINE_ID));
		if let Some(ConsensusLog::AuthoritiesChange(authorities)) = log {
			return Some(authorities);
		}
	}
	None
}
