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

//! The Cumulus [`CollatorService`] is a utility struct for performing common
//! operations used in parachain consensus/authoring.

use cumulus_client_network::WaitToAnnounce;
use cumulus_primitives_core::{
	BlockWeightApi, CollationInfo, CollectCollationInfo, ParachainBlockData,
};

use crate::metrics::CollatorMetrics;
use sc_client_api::BlockBackend;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_consensus::BlockStatus;
use sp_core::traits::SpawnNamed;
use sp_runtime::traits::{Block as BlockT, HashingFor, Header as HeaderT, Zero};

use cumulus_client_consensus_common::ParachainCandidate;
use polkadot_node_primitives::{
	BlockData, Collation, CollationSecondedSignal, MaybeCompressedPoV, PoV,
};

use codec::Encode;
use futures::channel::oneshot;
use parking_lot::Mutex;
use std::sync::Arc;

/// The logging target.
const LOG_TARGET: &str = "cumulus-collator";

/// Utility functions generally applicable to writing collators for Cumulus.
pub trait ServiceInterface<Block: BlockT> {
	/// Checks the status of the given block hash in the Parachain.
	///
	/// Returns `true` if the block could be found and is good to be build on.
	fn check_block_status(&self, hash: Block::Hash, header: &Block::Header) -> bool;

	/// Build a full [`Collation`] from a given [`ParachainCandidate`]. This requires
	/// that the underlying block has been fully imported into the underlying client,
	/// as implementations will fetch underlying runtime API data.
	///
	/// This also returns the unencoded parachain block data, in case that is desired.
	fn build_collation(
		&self,
		parent_header: &Block::Header,
		block_hash: Block::Hash,
		candidate: ParachainCandidate<Block>,
	) -> Option<(Collation, ParachainBlockData<Block>)>;

	/// Inform networking systems that the block should be announced after a signal has
	/// been received to indicate the block has been seconded by a relay-chain validator.
	///
	/// This sets up the barrier and returns the sending side of a channel, for the signal
	/// to be passed through.
	fn announce_with_barrier(
		&self,
		block_hash: Block::Hash,
	) -> oneshot::Sender<CollationSecondedSignal>;

	/// Directly announce a block on the network.
	fn announce_block(&self, block_hash: Block::Hash, data: Option<Vec<u8>>);

	/// Check the deviation between runtime-reported proof size and actual proof size.
	///
	/// This is a sanity check to detect discrepancies between what the runtime reports
	/// as the consumed proof size in `BlockWeight` vs the actual proof size in the PoV.
	///
	/// Default implementation does nothing. Implementations that have access to runtimes
	/// supporting [`BlockWeightApi`] should override this.
	fn check_proof_size_deviation(
		&self,
		_block_hash: Block::Hash,
		_block_data: &ParachainBlockData<Block>,
	) {
		// Default: no-op for runtimes that don't support BlockWeightApi
	}
}

/// Callback type for proof size deviation checking.
///
/// This callback is invoked after building a collation to compare the runtime-reported
/// proof size against the actual proof size.
pub type ProofSizeCheckFn<Block> =
	Arc<dyn Fn(<Block as BlockT>::Hash, &ParachainBlockData<Block>) + Send + Sync>;

/// The [`CollatorService`] provides common utilities for parachain consensus and authoring.
///
/// This includes logic for checking the block status of arbitrary parachain headers
/// gathered from the relay chain state, creating full [`Collation`]s to be shared with validators,
/// and distributing new parachain blocks along the network.
pub struct CollatorService<Block: BlockT, BS, RA> {
	block_status: Arc<BS>,
	wait_to_announce: Arc<Mutex<WaitToAnnounce<Block>>>,
	announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	runtime_api: Arc<RA>,
	metrics: Option<CollatorMetrics>,
	proof_size_check: Option<ProofSizeCheckFn<Block>>,
}

impl<Block: BlockT, BS, RA> Clone for CollatorService<Block, BS, RA> {
	fn clone(&self) -> Self {
		Self {
			block_status: self.block_status.clone(),
			wait_to_announce: self.wait_to_announce.clone(),
			announce_block: self.announce_block.clone(),
			runtime_api: self.runtime_api.clone(),
			metrics: self.metrics.clone(),
			proof_size_check: self.proof_size_check.clone(),
		}
	}
}

impl<Block, BS, RA> CollatorService<Block, BS, RA>
where
	Block: BlockT,
	BS: BlockBackend<Block>,
	RA: ProvideRuntimeApi<Block>,
	RA::Api: CollectCollationInfo<Block>,
{
	/// Create a new instance.
	pub fn new(
		block_status: Arc<BS>,
		spawner: Arc<dyn SpawnNamed + Send + Sync>,
		announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
		runtime_api: Arc<RA>,
		metrics: Option<CollatorMetrics>,
	) -> Self {
		let wait_to_announce =
			Arc::new(Mutex::new(WaitToAnnounce::new(spawner, announce_block.clone())));

		Self {
			block_status,
			wait_to_announce,
			announce_block,
			runtime_api,
			metrics,
			proof_size_check: None,
		}
	}

	/// Checks the status of the given block hash in the Parachain.
	///
	/// Returns `true` if the block could be found and is good to be build on.
	pub fn check_block_status(&self, hash: Block::Hash, header: &Block::Header) -> bool {
		match self.block_status.block_status(hash) {
			Ok(BlockStatus::Queued) => {
				tracing::debug!(
					target: LOG_TARGET,
					block_hash = ?hash,
					"Skipping candidate production, because block is still queued for import.",
				);
				false
			},
			Ok(BlockStatus::InChainWithState) => true,
			Ok(BlockStatus::InChainPruned) => {
				tracing::error!(
					target: LOG_TARGET,
					"Skipping candidate production, because block `{:?}` is already pruned!",
					hash,
				);
				false
			},
			Ok(BlockStatus::KnownBad) => {
				tracing::error!(
					target: LOG_TARGET,
					block_hash = ?hash,
					"Block is tagged as known bad and is included in the relay chain! Skipping candidate production!",
				);
				false
			},
			Ok(BlockStatus::Unknown) => {
				if header.number().is_zero() {
					tracing::error!(
						target: LOG_TARGET,
						block_hash = ?hash,
						"Could not find the header of the genesis block in the database!",
					);
				} else {
					tracing::debug!(
						target: LOG_TARGET,
						block_hash = ?hash,
						"Skipping candidate production, because block is unknown.",
					);
				}
				false
			},
			Err(e) => {
				tracing::error!(
					target: LOG_TARGET,
					block_hash = ?hash,
					error = ?e,
					"Failed to get block status.",
				);
				false
			},
		}
	}

	/// Fetch the collation info from the runtime.
	///
	/// Returns `Ok(Some((CollationInfo, ApiVersion)))` on success, `Err(_)` on error or `Ok(None)`
	/// if the runtime api isn't implemented by the runtime. `ApiVersion` being the version of the
	/// [`CollectCollationInfo`] runtime api.
	pub fn fetch_collation_info(
		&self,
		block_hash: Block::Hash,
		header: &Block::Header,
	) -> Result<Option<(CollationInfo, u32)>, sp_api::ApiError> {
		let runtime_api = self.runtime_api.runtime_api();

		let api_version =
			match runtime_api.api_version::<dyn CollectCollationInfo<Block>>(block_hash)? {
				Some(version) => version,
				None => {
					tracing::error!(
						target: LOG_TARGET,
						"Could not fetch `CollectCollationInfo` runtime api version."
					);
					return Ok(None)
				},
			};

		let collation_info = if api_version < 2 {
			#[allow(deprecated)]
			runtime_api
				.collect_collation_info_before_version_2(block_hash)?
				.into_latest(header.encode().into())
		} else {
			runtime_api.collect_collation_info(block_hash, header)?
		};

		Ok(Some((collation_info, api_version)))
	}

	/// Build a full [`Collation`] from a given [`ParachainCandidate`]. This requires
	/// that the underlying block has been fully imported into the underlying client,
	/// as it fetches underlying runtime API data.
	///
	/// This also returns the unencoded parachain block data, in case that is desired.
	pub fn build_collation(
		&self,
		parent_header: &Block::Header,
		block_hash: Block::Hash,
		candidate: ParachainCandidate<Block>,
	) -> Option<(Collation, ParachainBlockData<Block>)> {
		let block = candidate.block;

		let compact_proof = match candidate
			.proof
			.into_compact_proof::<HashingFor<Block>>(*parent_header.state_root())
		{
			Ok(proof) => proof,
			Err(e) => {
				tracing::error!(target: "cumulus-collator", "Failed to compact proof: {:?}", e);
				return None
			},
		};

		// Create the parachain block data for the validators.
		let (collation_info, _api_version) = self
			.fetch_collation_info(block_hash, block.header())
			.map_err(|e| {
				tracing::error!(
					target: LOG_TARGET,
					error = ?e,
					"Failed to collect collation info.",
				)
			})
			.ok()
			.flatten()?;

		// Workaround for: https://github.com/paritytech/polkadot-sdk/issues/64
		//
		// We are always using the `api_version` of the parent block. The `api_version` can only
		// change with a runtime upgrade and this is when we want to observe the old `api_version`.
		// Because this old `api_version` is the one used to validate this block. Otherwise we
		// already assume the `api_version` is higher than what the relay chain will use and this
		// will lead to validation errors.
		let api_version = self
			.runtime_api
			.runtime_api()
			.api_version::<dyn CollectCollationInfo<Block>>(parent_header.hash())
			.ok()
			.flatten()?;

		let block_data = ParachainBlockData::<Block>::new(vec![block], compact_proof);

		let pov = polkadot_node_primitives::maybe_compress_pov(PoV {
			block_data: BlockData(if api_version >= 3 {
				block_data.encode()
			} else {
				let block_data = block_data.as_v0();

				if block_data.is_none() {
					tracing::error!(
						target: LOG_TARGET,
						"Trying to submit a collation with multiple blocks is not supported by the current runtime."
					);
				}

				block_data?.encode()
			}),
		});

		let upward_messages = collation_info
			.upward_messages
			.try_into()
			.map_err(|e| {
				tracing::error!(
					target: LOG_TARGET,
					error = ?e,
					"Number of upward messages should not be greater than `MAX_UPWARD_MESSAGE_NUM`",
				)
			})
			.ok()?;
		let horizontal_messages = collation_info
			.horizontal_messages
			.try_into()
			.map_err(|e| {
				tracing::error!(
					target: LOG_TARGET,
					error = ?e,
					"Number of horizontal messages should not be greater than `MAX_HORIZONTAL_MESSAGE_NUM`",
				)
			})
			.ok()?;

		let collation = Collation {
			upward_messages,
			new_validation_code: collation_info.new_validation_code,
			processed_downward_messages: collation_info.processed_downward_messages,
			horizontal_messages,
			hrmp_watermark: collation_info.hrmp_watermark,
			head_data: collation_info.head_data,
			proof_of_validity: MaybeCompressedPoV::Compressed(pov),
		};

		Some((collation, block_data))
	}

	/// Inform the networking systems that the block should be announced after an appropriate
	/// signal has been received. This returns the sending half of the signal.
	pub fn announce_with_barrier(
		&self,
		block_hash: Block::Hash,
	) -> oneshot::Sender<CollationSecondedSignal> {
		let (result_sender, signed_stmt_recv) = oneshot::channel();
		self.wait_to_announce.lock().wait_to_announce(block_hash, signed_stmt_recv);
		result_sender
	}
}

/// Builder methods for configuring optional features on [`CollatorService`].
impl<Block, BS, RA> CollatorService<Block, BS, RA>
where
	Block: BlockT,
	BS: BlockBackend<Block>,
	RA: ProvideRuntimeApi<Block>,
	RA::Api: CollectCollationInfo<Block>,
{
	/// Enable proof size deviation checking for this collator service.
	///
	/// This method should only be called when the runtime implements [`BlockWeightApi`].
	/// Use [`create_proof_size_check`] to create the callback.
	pub fn with_proof_size_check(mut self, check: ProofSizeCheckFn<Block>) -> Self {
		self.proof_size_check = Some(check);
		self
	}
}

/// Create a proof size check callback for runtimes that implement [`BlockWeightApi`].
///
/// This function creates a callback that can be passed to
/// [`CollatorService::with_proof_size_check`] to enable proof size deviation checking.
pub fn create_proof_size_check<Block, RA>(
	runtime_api: Arc<RA>,
	metrics: Option<CollatorMetrics>,
) -> ProofSizeCheckFn<Block>
where
	Block: BlockT,
	RA: ProvideRuntimeApi<Block> + Send + Sync + 'static,
	RA::Api: BlockWeightApi<Block>,
{
	Arc::new(move |block_hash, block_data| {
		let api = runtime_api.runtime_api();

		let reported_weight = match api.block_weight(block_hash) {
			Ok(weight) => weight,
			Err(e) => {
				tracing::debug!(
					target: LOG_TARGET,
					block_hash = ?block_hash,
					error = ?e,
					"Failed to fetch block weight from runtime API",
				);
				return;
			},
		};

		let reported_proof_size = reported_weight.proof_size();
		let actual_proof_size = block_data.proof().encoded_size() as u64;
		let deviation = actual_proof_size as i64 - reported_proof_size as i64;

		tracing::debug!(
			target: LOG_TARGET,
			block_hash = ?block_hash,
			%reported_proof_size,
			%actual_proof_size,
			%deviation,
			"BlockWeight proof size sanity check",
		);

		// Warning threshold: 10KB deviation
		const WARNING_THRESHOLD: i64 = 10_000;
		if deviation.abs() > WARNING_THRESHOLD {
			tracing::warn!(
				target: LOG_TARGET,
				block_hash = ?block_hash,
				%reported_proof_size,
				%actual_proof_size,
				%deviation,
				"Large BlockWeight proof size deviation detected",
			);
		}

		if let Some(ref m) = metrics {
			m.proof_size_deviation.observe(deviation as f64);
		}
	})
}

impl<Block, BS, RA> ServiceInterface<Block> for CollatorService<Block, BS, RA>
where
	Block: BlockT,
	BS: BlockBackend<Block>,
	RA: ProvideRuntimeApi<Block>,
	RA::Api: CollectCollationInfo<Block>,
{
	fn check_block_status(&self, hash: Block::Hash, header: &Block::Header) -> bool {
		CollatorService::check_block_status(self, hash, header)
	}

	fn build_collation(
		&self,
		parent_header: &Block::Header,
		block_hash: Block::Hash,
		candidate: ParachainCandidate<Block>,
	) -> Option<(Collation, ParachainBlockData<Block>)> {
		CollatorService::build_collation(self, parent_header, block_hash, candidate)
	}

	fn announce_with_barrier(
		&self,
		block_hash: Block::Hash,
	) -> oneshot::Sender<CollationSecondedSignal> {
		CollatorService::announce_with_barrier(self, block_hash)
	}

	fn announce_block(&self, block_hash: Block::Hash, data: Option<Vec<u8>>) {
		(self.announce_block)(block_hash, data)
	}

	fn check_proof_size_deviation(
		&self,
		block_hash: Block::Hash,
		block_data: &ParachainBlockData<Block>,
	) {
		if let Some(ref check) = self.proof_size_check {
			check(block_hash, block_data);
		}
	}
}
