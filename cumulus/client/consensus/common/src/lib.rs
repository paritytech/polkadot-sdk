// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use codec::Decode;
use polkadot_primitives::{
	Block as PBlock, Hash as PHash, Header as PHeader, PersistedValidationData, ValidationCodeHash,
};

use cumulus_primitives_core::{
	relay_chain,
	AbridgedHostConfiguration,
};
use cumulus_relay_chain_interface::{RelayChainError, RelayChainInterface};

use sc_client_api::Backend;
use sc_consensus::{shared_data::SharedData, BlockImport, ImportResult};
use sp_consensus_slots::Slot;

use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use sp_timestamp::Timestamp;

use std::{sync::Arc, time::Duration};

mod level_monitor;
mod parachain_consensus;
mod parent_search;
#[cfg(test)]
mod tests;

pub use parent_search::*;

pub use parachain_consensus::run_parachain_consensus;

use level_monitor::LevelMonitor;
pub use level_monitor::{LevelLimit, MAX_LEAVES_PER_LEVEL_SENSIBLE_DEFAULT};

pub mod import_queue;

/// Provides the hash of validation code used for authoring/execution of blocks at a given
/// hash.
pub trait ValidationCodeHashProvider<Hash> {
	fn code_hash_at(&self, at: Hash) -> Option<ValidationCodeHash>;
}

impl<F, Hash> ValidationCodeHashProvider<Hash> for F
where
	F: Fn(Hash) -> Option<ValidationCodeHash>,
{
	fn code_hash_at(&self, at: Hash) -> Option<ValidationCodeHash> {
		(self)(at)
	}
}

/// The result of [`ParachainConsensus::produce_candidate`].
pub struct ParachainCandidate<B> {
	/// The block that was built for this candidate.
	pub block: B,
	/// The proof that was recorded while building the block.
	pub proof: sp_trie::StorageProof,
}

/// A specific parachain consensus implementation that can be used by a collator to produce
/// candidates.
///
/// The collator will call [`Self::produce_candidate`] every time there is a free core for the
/// parachain this collator is collating for. It is the job of the consensus implementation to
/// decide if this specific collator should build a candidate for the given relay chain block. The
/// consensus implementation could, for example, check whether this specific collator is part of a
/// staked set.
#[async_trait::async_trait]
pub trait ParachainConsensus<B: BlockT>: Send + Sync + dyn_clone::DynClone {
	/// Produce a new candidate at the given parent block and relay-parent blocks.
	///
	/// Should return `None` if the consensus implementation decided that it shouldn't build a
	/// candidate or if there occurred any error.
	///
	/// # NOTE
	///
	/// It is expected that the block is already imported when the future resolves.
	async fn produce_candidate(
		&mut self,
		parent: &B::Header,
		relay_parent: PHash,
		validation_data: &PersistedValidationData,
	) -> Option<ParachainCandidate<B>>;
}

dyn_clone::clone_trait_object!(<B> ParachainConsensus<B> where B: BlockT);

#[async_trait::async_trait]
impl<B: BlockT> ParachainConsensus<B> for Box<dyn ParachainConsensus<B> + Send + Sync> {
	async fn produce_candidate(
		&mut self,
		parent: &B::Header,
		relay_parent: PHash,
		validation_data: &PersistedValidationData,
	) -> Option<ParachainCandidate<B>> {
		(*self).produce_candidate(parent, relay_parent, validation_data).await
	}
}

/// Parachain specific block import.
///
/// Specialized block import for parachains. It supports to delay setting the best block until the
/// relay chain has included a candidate in its best block. By default the delayed best block
/// setting is disabled. The block import also monitors the imported blocks and prunes by default if
/// there are too many blocks at the same height. Too many blocks at the same height can for example
/// happen if the relay chain is rejecting the parachain blocks in the validation.
pub struct ParachainBlockImport<Block: BlockT, BI, BE> {
	inner: BI,
	monitor: Option<SharedData<LevelMonitor<Block, BE>>>,
	delayed_best_block: bool,
}

impl<Block: BlockT, BI, BE: Backend<Block>> ParachainBlockImport<Block, BI, BE> {
	/// Create a new instance.
	///
	/// The number of leaves per level limit is set to `LevelLimit::Default`.
	pub fn new(inner: BI, backend: Arc<BE>) -> Self {
		Self::new_with_limit(inner, backend, LevelLimit::Default)
	}

	/// Create a new instance with an explicit limit to the number of leaves per level.
	///
	/// This function alone doesn't enforce the limit on levels for old imported blocks,
	/// the limit is eventually enforced only when new blocks are imported.
	pub fn new_with_limit(inner: BI, backend: Arc<BE>, level_leaves_max: LevelLimit) -> Self {
		let level_limit = match level_leaves_max {
			LevelLimit::None => None,
			LevelLimit::Some(limit) => Some(limit),
			LevelLimit::Default => Some(MAX_LEAVES_PER_LEVEL_SENSIBLE_DEFAULT),
		};

		let monitor =
			level_limit.map(|level_limit| SharedData::new(LevelMonitor::new(level_limit, backend)));

		Self { inner, monitor, delayed_best_block: false }
	}

	/// Create a new instance which delays setting the best block.
	///
	/// The number of leaves per level limit is set to `LevelLimit::Default`.
	pub fn new_with_delayed_best_block(inner: BI, backend: Arc<BE>) -> Self {
		Self {
			delayed_best_block: true,
			..Self::new_with_limit(inner, backend, LevelLimit::Default)
		}
	}
}

impl<Block: BlockT, I: Clone, BE> Clone for ParachainBlockImport<Block, I, BE> {
	fn clone(&self) -> Self {
		ParachainBlockImport {
			inner: self.inner.clone(),
			monitor: self.monitor.clone(),
			delayed_best_block: self.delayed_best_block,
		}
	}
}

#[async_trait::async_trait]
impl<Block, BI, BE> BlockImport<Block> for ParachainBlockImport<Block, BI, BE>
where
	Block: BlockT,
	BI: BlockImport<Block> + Send,
	BE: Backend<Block>,
{
	type Error = BI::Error;

	async fn check_block(
		&mut self,
		block: sc_consensus::BlockCheckParams<Block>,
	) -> Result<sc_consensus::ImportResult, Self::Error> {
		self.inner.check_block(block).await
	}

	async fn import_block(
		&mut self,
		mut params: sc_consensus::BlockImportParams<Block>,
	) -> Result<sc_consensus::ImportResult, Self::Error> {
		// Blocks are stored within the backend by using POST hash.
		let hash = params.post_hash();
		let number = *params.header.number();

		if params.with_state() {
			// Force imported state finality.
			// Required for warp sync. We assume that preconditions have been
			// checked properly and we are importing a finalized block with state.
			params.finalized = true;
		}

		if self.delayed_best_block {
			// Best block is determined by the relay chain, or if we are doing the initial sync
			// we import all blocks as new best.
			params.fork_choice = Some(sc_consensus::ForkChoiceStrategy::Custom(
				params.origin == sp_consensus::BlockOrigin::NetworkInitialSync,
			));
		}

		let maybe_lock = self.monitor.as_ref().map(|monitor_lock| {
			let mut monitor = monitor_lock.shared_data_locked();
			monitor.enforce_limit(number);
			monitor.release_mutex()
		});

		let res = self.inner.import_block(params).await?;

		if let (Some(mut monitor_lock), ImportResult::Imported(_)) = (maybe_lock, &res) {
			let mut monitor = monitor_lock.upgrade();
			monitor.block_imported(number, hash);
		}

		Ok(res)
	}
}

/// Marker trait denoting a block import type that fits the parachain requirements.
pub trait ParachainBlockImportMarker {}

impl<B: BlockT, BI, BE> ParachainBlockImportMarker for ParachainBlockImport<B, BI, BE> {}

/// Get the relay-parent slot and timestamp from a header.
pub fn relay_slot_and_timestamp(
	relay_parent_header: &PHeader,
	relay_chain_slot_duration: Duration,
) -> Option<(Slot, Timestamp)> {
	sc_consensus_babe::find_pre_digest::<PBlock>(relay_parent_header)
		.map(|babe_pre_digest| {
			let slot = babe_pre_digest.slot();
			let t = Timestamp::new(relay_chain_slot_duration.as_millis() as u64 * *slot);

			(slot, t)
		})
		.ok()
}

/// Reads abridged host configuration from the relay chain storage at the given relay parent.
pub async fn load_abridged_host_configuration(
	relay_parent: PHash,
	relay_client: &impl RelayChainInterface,
) -> Result<Option<AbridgedHostConfiguration>, RelayChainError> {
	relay_client
		.get_storage_by_key(relay_parent, relay_chain::well_known_keys::ACTIVE_CONFIG)
		.await?
		.map(|bytes| {
			AbridgedHostConfiguration::decode(&mut &bytes[..])
				.map_err(RelayChainError::DeserializationError)
		})
		.transpose()
}
