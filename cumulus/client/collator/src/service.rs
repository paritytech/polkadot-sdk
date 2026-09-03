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

use cumulus_primitives_core::{
	CollationInfo, CollectCollationInfo, ParachainBlockData, SchedulingProof,
};

use polkadot_primitives::UMP_SEPARATOR;
use sc_client_api::BlockBackend;
use sp_api::{ApiExt, ProvideRuntimeApi, StorageProof};
use sp_consensus::BlockStatus;
use sp_runtime::traits::{Block as BlockT, HashingFor, Header as HeaderT, Zero};

use cumulus_client_consensus_common::ParachainCandidate;
use polkadot_node_primitives::{BlockData, Collation, MaybeCompressedPoV, PoV};

use codec::Encode;
use sp_additional_data::AdditionalData;
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
	/// `scheduling_proof` is `Some` for V3 candidates (produces [`ParachainBlockData::V2`], or
	/// [`ParachainBlockData::V3`] when additional data is carried) and `None` for legacy candidates
	/// (produces [`ParachainBlockData::V1`]).
	///
	/// `additional_data` is a per-block vec of optional maps; a non-empty `Some` entry produces
	/// [`ParachainBlockData::V3`]. It can only ride in a V3 candidate (a V3 extension of V2), so it
	/// is carried only when `scheduling_proof` is `Some`; otherwise it is dropped.
	///
	/// This also returns the unencoded parachain block data, in case that is desired.
	fn build_collation(
		&self,
		parent_header: &Block::Header,
		block_hash: Block::Hash,
		candidate: ParachainCandidate<Block>,
		scheduling_proof: Option<SchedulingProof>,
		additional_data: Vec<Option<AdditionalData>>,
	) -> Option<(Collation, ParachainBlockData<Block>)>;

	/// Build a multi-block collation.
	///
	/// Does the same as [`Self::build_collation`], but includes multiple blocks into one collation.
	/// The given `parent_header` should be the header from the parent of the first block.
	///
	/// `scheduling_proof` is `Some` for V3 candidates (produces [`ParachainBlockData::V2`], or
	/// [`ParachainBlockData::V3`] when additional data is carried) and `None` for legacy candidates
	/// (produces [`ParachainBlockData::V1`]).
	///
	/// `additional_data` is a per-block vec of optional maps; a non-empty `Some` entry produces
	/// [`ParachainBlockData::V3`]. It can only ride in a V3 candidate (a V3 extension of V2), so it
	/// is carried only when `scheduling_proof` is `Some`; otherwise it is dropped.
	fn build_multi_block_collation(
		&self,
		parent_header: &Block::Header,
		blocks: Vec<Block>,
		proof: StorageProof,
		scheduling_proof: Option<SchedulingProof>,
		additional_data: Vec<Option<AdditionalData>>,
	) -> Option<(Collation, ParachainBlockData<Block>)>;

	/// Directly announce a block on the network.
	fn announce_block(&self, block_hash: Block::Hash, data: Option<Vec<u8>>);
}

/// The [`CollatorService`] provides common utilities for parachain consensus and authoring.
///
/// This includes logic for checking the block status of arbitrary parachain headers
/// gathered from the relay chain state, creating full [`Collation`]s to be shared with validators,
/// and distributing new parachain blocks along the network.
pub struct CollatorService<Block: BlockT, BS, RA> {
	block_status: Arc<BS>,
	announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	runtime_api: Arc<RA>,
}

impl<Block: BlockT, BS, RA> Clone for CollatorService<Block, BS, RA> {
	fn clone(&self) -> Self {
		Self {
			block_status: self.block_status.clone(),
			announce_block: self.announce_block.clone(),
			runtime_api: self.runtime_api.clone(),
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
	fn split_at_separator(messages: Vec<Vec<u8>>) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
		let mut parts = messages.splitn(2, |m: &Vec<u8>| m.is_empty());
		(parts.next().unwrap_or(&[]).to_vec(), parts.next().unwrap_or(&[]).to_vec())
	}

	/// Create a new instance.
	pub fn new(
		block_status: Arc<BS>,
		announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
		runtime_api: Arc<RA>,
	) -> Self {
		Self { block_status, announce_block, runtime_api }
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
					return Ok(None);
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
	fn build_multi_block_collation(
		&self,
		parent_header: &Block::Header,
		blocks: Vec<Block>,
		proof: StorageProof,
		scheduling_proof: Option<SchedulingProof>,
		additional_data: Vec<Option<AdditionalData>>,
	) -> Option<(Collation, ParachainBlockData<Block>)> {
		let compact_proof =
			match proof.into_compact_proof::<HashingFor<Block>>(*parent_header.state_root()) {
				Ok(proof) => proof,
				Err(e) => {
					tracing::error!(target: "cumulus-collator", "Failed to compact proof: {:?}", e);
					return None;
				},
			};

		// We are always using the `api_version` of the parent block. The `api_version` can only
		// change with a runtime upgrade and this is when we want to observe the old
		// `api_version`. Because this old `api_version` is the one used to validate this
		// block. Otherwise, we already assume the `api_version` is higher than what the relay
		// chain will use and this will lead to validation errors.
		let api_version = self
			.runtime_api
			.runtime_api()
			.api_version::<dyn CollectCollationInfo<Block>>(parent_header.hash())
			.ok()
			.flatten()?;
		let mut upward_messages = Vec::new();
		let mut upward_message_signals = Vec::<Vec<u8>>::with_capacity(4);
		let mut horizontal_messages = Vec::new();
		let mut new_validation_code = None;
		let mut processed_downward_messages = 0;
		let mut hrmp_watermark = None;
		let mut head_data = None;

		for block in &blocks {
			// Create the parachain block data for the validators.
			let (collation_info, _api_version) = self
				.fetch_collation_info(block.hash(), block.header())
				.map_err(|e| {
					tracing::error!(
						target: LOG_TARGET,
						error = ?e,
						"Failed to collect collation info.",
					)
				})
				.ok()
				.flatten()?;

			let (messages, signals) = Self::split_at_separator(collation_info.upward_messages);

			upward_messages.extend(messages);
			upward_message_signals.extend(signals);
			horizontal_messages.extend(collation_info.horizontal_messages);

			if let Some(new_code) = collation_info.new_validation_code {
				if new_validation_code.replace(new_code).is_some() {
					tracing::warn!(
						target: LOG_TARGET,
						block = ?block.hash(),
						"Overwriting validation code from an earlier block in the bundle.",
					);
				}
			}
			processed_downward_messages += collation_info.processed_downward_messages;
			hrmp_watermark = Some(collation_info.hrmp_watermark);
			head_data = Some(collation_info.head_data);
		}

		// Sort by recipient as required by the relay chain rules.
		horizontal_messages.sort_by(|a, b| a.recipient.cmp(&b.recipient));

		let block_data = match scheduling_proof {
			Some(scheduling_proof) => ParachainBlockData::<Block>::new_with_additional_data(
				blocks,
				compact_proof,
				scheduling_proof,
				additional_data,
			),
			// No scheduling proof (V3 scheduling disabled) → legacy V1 candidate. Additional data
			// can only ride in a V3 candidate (a V3 extension of V2), so it is not carried here.
			None => ParachainBlockData::new(blocks, compact_proof, None),
		};

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

		// If we got some signals, push them now.
		if !upward_message_signals.is_empty() {
			upward_messages.push(UMP_SEPARATOR);
			upward_messages.extend(upward_message_signals.into_iter());
		}

		let upward_messages = upward_messages
			.try_into()
			.map_err(|e| {
				tracing::error!(
					target: LOG_TARGET,
					error = ?e,
					"Number of upward messages should not be greater than `MAX_UPWARD_MESSAGE_NUM`",
				)
			})
			.ok()?;
		let horizontal_messages = horizontal_messages
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
			new_validation_code,
			processed_downward_messages,
			horizontal_messages,
			// If these are `None`, there was no block.
			hrmp_watermark: hrmp_watermark?,
			head_data: head_data?,
			proof_of_validity: MaybeCompressedPoV::Compressed(pov),
		};

		Some((collation, block_data))
	}
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
		_: Block::Hash,
		candidate: ParachainCandidate<Block>,
		scheduling_proof: Option<SchedulingProof>,
		additional_data: Vec<Option<AdditionalData>>,
	) -> Option<(Collation, ParachainBlockData<Block>)> {
		CollatorService::build_multi_block_collation(
			self,
			parent_header,
			vec![candidate.block],
			candidate.proof,
			scheduling_proof,
			additional_data,
		)
	}

	fn announce_block(&self, block_hash: Block::Hash, data: Option<Vec<u8>>) {
		(self.announce_block)(block_hash, data)
	}

	fn build_multi_block_collation(
		&self,
		parent_header: &<Block as BlockT>::Header,
		blocks: Vec<Block>,
		proof: StorageProof,
		scheduling_proof: Option<SchedulingProof>,
		additional_data: Vec<Option<AdditionalData>>,
	) -> Option<(Collation, ParachainBlockData<Block>)> {
		CollatorService::build_multi_block_collation(
			self,
			parent_header,
			blocks,
			proof,
			scheduling_proof,
			additional_data,
		)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use cumulus_test_client::{
		BuildBlockBuilder, DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
	};
	use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
	use polkadot_primitives::HeadData;
	use sc_consensus::{BlockImport, BlockImportParams, ForkChoiceStrategy, StateAction};
	use sp_consensus::BlockOrigin;
	use sp_core::{traits::SpawnNamed, H256};
	use std::sync::Arc;

	type Block = cumulus_test_client::runtime::Block;
	type Client = cumulus_test_client::Client;

	#[derive(Clone)]
	struct NoopSpawner;
	impl SpawnNamed for NoopSpawner {
		fn spawn_blocking(
			&self,
			_: &'static str,
			_: Option<&'static str>,
			future: futures::future::BoxFuture<'static, ()>,
		) {
			drop(future);
		}

		fn spawn(
			&self,
			_: &'static str,
			_: Option<&'static str>,
			future: futures::future::BoxFuture<'static, ()>,
		) {
			drop(future);
		}
	}

	fn make_scheduling_proof() -> SchedulingProof {
		let make_header = |n: u32| {
			polkadot_primitives::Header::new(
				n,
				H256::repeat_byte(1),
				H256::repeat_byte(2),
				H256::repeat_byte(3),
				Default::default(),
			)
		};
		SchedulingProof {
			header_chain: vec![make_header(2)],
			internal_scheduling_parent_header: make_header(1),
			signed_scheduling_info: None,
		}
	}

	async fn build_and_import_block(client: &Arc<Client>) -> (Block, StorageProof) {
		use cumulus_test_client::BlockBuilderAndSupportData;
		let genesis_hash = client.chain_info().genesis_hash;
		let genesis_header = client.header(genesis_hash).unwrap().unwrap();
		let mut sproof = RelayStateSproofBuilder::default();
		sproof.para_id = cumulus_test_client::runtime::PARACHAIN_ID.into();
		sproof.included_para_head = Some(HeadData(genesis_header.encode()));
		let BlockBuilderAndSupportData {
			block_builder,
			proof_recorder,
			additional_data_recorder,
			..
		} = client.init_block_builder_builder().with_relay_sproof_builder(sproof).build();
		let built = block_builder.build().expect("block built");
		let block = built.block.clone();
		let proof = proof_recorder.drain_storage_proof();

		let mut params = BlockImportParams::new(BlockOrigin::Own, block.header.clone());
		params.body = Some(block.extrinsics.clone());
		params.state_action = StateAction::Execute;
		params.fork_choice = Some(ForkChoiceStrategy::LongestChain);
		// The runtime read relay state while building, so the header carries an `AdditionalData`
		// digest; the executing import path requires the matching recorded map.
		params.additional_data = additional_data_recorder();
		client.import_block(params).await.expect("block imported");

		(block, proof)
	}

	fn make_service(client: Arc<Client>) -> CollatorService<Block, Client, Client> {
		CollatorService::new(
			client.clone(),
			Arc::new(NoopSpawner) as Arc<dyn SpawnNamed + Send + Sync>,
			Arc::new(|_, _| {}),
			client,
		)
	}

	#[tokio::test]
	async fn v3_packing_when_additional_data_provided() {
		let client = Arc::new(TestClientBuilder::new().build());
		let genesis_hash = client.chain_info().genesis_hash;
		let genesis_header = client.header(genesis_hash).unwrap().unwrap();
		let (block, proof) = build_and_import_block(&client).await;

		let blob: AdditionalData = [("test".to_string(), vec![7u8, 8, 9])].into();
		let result = make_service(client).build_multi_block_collation(
			&genesis_header,
			vec![block],
			proof,
			Some(make_scheduling_proof()),
			vec![Some(blob.clone())],
		);

		let (_, block_data) = result.expect("collation produced");
		assert!(matches!(block_data, ParachainBlockData::V3 { .. }), "expected V3");
		assert_eq!(block_data.additional_data()[0], Some(blob));
	}

	#[tokio::test]
	async fn v2_fallback_when_no_additional_data() {
		let client = Arc::new(TestClientBuilder::new().build());
		let genesis_hash = client.chain_info().genesis_hash;
		let genesis_header = client.header(genesis_hash).unwrap().unwrap();
		let (block, proof) = build_and_import_block(&client).await;

		let result = make_service(client).build_multi_block_collation(
			&genesis_header,
			vec![block],
			proof,
			Some(make_scheduling_proof()),
			vec![None],
		);

		let (_, block_data) = result.expect("collation produced");
		assert!(matches!(block_data, ParachainBlockData::V2 { .. }), "expected V2");
		assert!(block_data.additional_data().is_empty());
	}
}
