use std::fmt::Debug;

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
pub struct AuthoritiesTracker<P: Pair, B: Block>(
	RwLock<ForkTree<B::Hash, NumberFor<B>, Vec<AuthorityId<P>>>>,
);

impl<P: Pair, B: Block> AuthoritiesTracker<P, B> {
	/// Fetch authorities from the tracker, if available. If not available, fetch from the client
	/// and update the tracker.
	pub fn fetch_or_update<C>(
		&self,
		header: &B::Header,
		client: &C,
		compatibility_mode: &CompatibilityMode<NumberFor<B>>,
	) -> Result<Vec<AuthorityId<P>>, String>
	where
		C: HeaderBackend<B>
			+ HeaderMetadata<B, Error = sp_blockchain::Error>
			+ ProvideRuntimeApi<B>,
		P::Public: Codec + Debug,
		C::Api: AuraApi<B, AuthorityId<P>>,
	{
		let hash = header.hash();
		let number = *header.number();
		let parent_hash = *header.parent_hash();

		// Fetch authorities from cache, if available.
		let authorities = {
			let is_descendent_of =
				sc_client_api::utils::is_descendent_of(client, Some((hash, parent_hash)));
			let authorities_cache = self.0.read();
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
					"Authorities for block {:?} at number {} not found in cache, fetching from runtime.",
					hash,
					number
				);
				let authorities =
					fetch_authorities_from_runtime(client, parent_hash, number, compatibility_mode)
						.map_err(|e| {
							format!("Could not fetch authorities at {:?}: {}", parent_hash, e)
						})?;
				let is_descendent_of = sc_client_api::utils::is_descendent_of(client, None);
				let mut authorities_cache = self.0.write();
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
	pub fn import<C>(&self, header: &B::Header, client: &C) -> Result<(), String>
	where
		C: HeaderBackend<B> + HeaderMetadata<B, Error = sp_blockchain::Error>,
		P::Public: Codec,
	{
		if let Some(authorities_change) = find_authorities_change_digest::<B, P>(header) {
			let hash = header.hash();
			let number = *header.number();
			log::debug!(
				target: LOG_TARGET,
				"Importing authorities change for block {:?} at number {} found in header digest",
				hash,
				number,
			);
			self.prune_finalized(client)?;
			let is_descendent_of = sc_client_api::utils::is_descendent_of(client, None);
			let mut authorities_cache = self.0.write();
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

	fn prune_finalized<C>(&self, client: &C) -> Result<(), String>
	where
		C: HeaderBackend<B> + HeaderMetadata<B, Error = sp_blockchain::Error>,
	{
		let is_descendent_of = sc_client_api::utils::is_descendent_of(client, None);
		let info = client.info();
		let mut authorities_cache = self.0.write();
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
	let mut authorities_change_digest: Option<_> = None;
	for log in header.digest().logs() {
		log::trace!(target: LOG_TARGET, "Checking log {:?}, looking for authorities change digest.", log);
		let log = log
			.try_to::<ConsensusLog<AuthorityId<P>>>(OpaqueDigestItemId::Consensus(&AURA_ENGINE_ID));
		if let Some(ConsensusLog::AuthoritiesChange(authorities)) = log {
			authorities_change_digest = Some(authorities);
		}
	}
	authorities_change_digest
}
