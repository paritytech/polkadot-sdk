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
	ump_signal_tail, CollationInfo, CollectCollationInfo, ParachainBlockData, SchedulingProof,
	SchedulingSignals, SignedSchedulingInfo, SpecMessagingError, SpecMessagingSignals,
};

use polkadot_primitives::UMP_SEPARATOR;
use sc_client_api::BlockBackend;
use sp_api::{ApiExt, ProvideRuntimeApi, StorageProof};
use sp_consensus::BlockStatus;
use sp_runtime::traits::{Block as BlockT, HashingFor, Header as HeaderT, Zero};

use cumulus_client_consensus_common::ParachainCandidate;
use polkadot_node_primitives::{BlockData, Collation, MaybeCompressedPoV, PoV};

use codec::Encode;
use std::sync::Arc;
/// The logging target.
const LOG_TARGET: &str = "cumulus-collator";

/// The candidate's UMP signal tail, byte-identical to what `validate_block` emits.
///
/// Runs the same speculative-messaging checks as `validate_block`, so a candidate it would reject
/// is never submitted. Consumption records and lifts are not sourced node-side yet, so no
/// `Requires` is synthesized here; `validate_block` rejects a consuming candidate either way.
fn collation_ump_signals(
	signed_scheduling_info: Option<&SignedSchedulingInfo>,
	upward_message_signals: Vec<Vec<u8>>,
) -> Result<Vec<Vec<u8>>, SpecMessagingError> {
	let spec_messaging = SpecMessagingSignals::build(&upward_message_signals, &[], None)?;
	Ok(match signed_scheduling_info {
		// A signed scheduling info (resubmission) replaces the block's *scheduling* signals
		// wholesale, via the same `SchedulingSignals::from_scheduling_info` the PVF applies. The
		// block's `Provides` is kept, as the PVF keeps it.
		Some(signed_info) => {
			ump_signal_tail(SchedulingSignals::from_scheduling_info(signed_info), spec_messaging)
		},
		// Otherwise the block's signals pass through unchanged: the runtime already emits them
		// in the canonical order (`pallet_parachain_system::send_ump_signals`).
		None if upward_message_signals.is_empty() => Vec::new(),
		None => core::iter::once(UMP_SEPARATOR).chain(upward_message_signals).collect(),
	})
}

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
	/// `scheduling_proof` is `Some` for V3 candidates (produces [`ParachainBlockData::V2`])
	/// and `None` for legacy candidates (produces [`ParachainBlockData::V1`]).
	///
	/// This also returns the unencoded parachain block data, in case that is desired.
	fn build_collation(
		&self,
		parent_header: &Block::Header,
		block_hash: Block::Hash,
		candidate: ParachainCandidate<Block>,
		scheduling_proof: Option<SchedulingProof>,
	) -> Option<(Collation, ParachainBlockData<Block>)>;

	/// Build a multi-block collation.
	///
	/// Does the same as [`Self::build_collation`], but includes multiple blocks into one collation.
	/// The given `parent_header` should be the header from the parent of the first block.
	///
	/// `scheduling_proof` is `Some` for V3 candidates (produces [`ParachainBlockData::V2`])
	/// and `None` for legacy candidates (produces [`ParachainBlockData::V1`]).
	fn build_multi_block_collation(
		&self,
		parent_header: &Block::Header,
		blocks: Vec<Block>,
		proof: StorageProof,
		scheduling_proof: Option<SchedulingProof>,
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

		// Capture the signed scheduling info before `scheduling_proof` is moved; the tail below is
		// rebuilt from it.
		let signed_scheduling_info =
			scheduling_proof.as_ref().and_then(|p| p.signed_scheduling_info.clone());

		let block_data = ParachainBlockData::<Block>::new(blocks, compact_proof, scheduling_proof);

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

		// Emit the UMP signal tail, as `validate_block` will.
		match collation_ump_signals(signed_scheduling_info.as_ref(), upward_message_signals) {
			Ok(tail) => upward_messages.extend(tail),
			Err(error) => {
				tracing::error!(
					target: LOG_TARGET,
					%error,
					"Not submitting a collation `validate_block` would reject.",
				);
				return None;
			},
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
	) -> Option<(Collation, ParachainBlockData<Block>)> {
		CollatorService::build_multi_block_collation(
			self,
			parent_header,
			vec![candidate.block],
			candidate.proof,
			scheduling_proof,
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
	) -> Option<(Collation, ParachainBlockData<Block>)> {
		CollatorService::build_multi_block_collation(
			self,
			parent_header,
			blocks,
			proof,
			scheduling_proof,
		)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use cumulus_primitives_core::SchedulingInfoPayload;
	use polkadot_primitives::{
		ApprovedPeerId, ClaimQueueOffset, CoreSelector, Hash, Id as ParaId, RequiresSet,
		StreamsRoot, UMPSignal,
	};

	fn peer(byte: u8) -> ApprovedPeerId {
		ApprovedPeerId::try_from(vec![byte; 4]).expect("4 bytes fits the bound; qed")
	}

	fn signed(core_selector: u8, peer_id: ApprovedPeerId) -> SignedSchedulingInfo {
		SignedSchedulingInfo {
			payload: SchedulingInfoPayload::new(
				CoreSelector(core_selector),
				1,
				peer_id,
				Default::default(),
			),
			signature: [0u8; 64],
		}
	}

	/// What `validate_block` emits for the same inputs, with no records and no lifts.
	fn pvf_tail(signed_info: Option<&SignedSchedulingInfo>, raw: &[Vec<u8>]) -> Vec<Vec<u8>> {
		let scheduling = match signed_info {
			Some(signed_info) => SchedulingSignals::from_scheduling_info(signed_info),
			None => SchedulingSignals::from_block_signals(raw),
		};
		ump_signal_tail(scheduling, SpecMessagingSignals::build(raw, &[], None).unwrap())
	}

	fn block_signals() -> Vec<Vec<u8>> {
		vec![
			UMPSignal::SelectCore(CoreSelector(0), ClaimQueueOffset(0)).encode(),
			UMPSignal::ApprovedPeer(peer(0xAA)).encode(),
			UMPSignal::Provides(StreamsRoot(Hash::repeat_byte(7))).encode(),
		]
	}

	#[test]
	fn plain_collation_matches_validate_block() {
		let raw = block_signals();
		assert_eq!(collation_ump_signals(None, raw.clone()).unwrap(), pvf_tail(None, &raw));
	}

	#[test]
	fn resubmission_keeps_provides_and_matches_validate_block() {
		let raw = block_signals();
		let signed_info = signed(5, peer(0xBB));

		let tail = collation_ump_signals(Some(&signed_info), raw.clone()).unwrap();
		assert_eq!(tail, pvf_tail(Some(&signed_info), &raw));
		// The signed scheduling signals replace the block's; its `Provides` stays.
		assert_eq!(
			tail,
			vec![
				UMP_SEPARATOR,
				UMPSignal::SelectCore(CoreSelector(5), ClaimQueueOffset(1)).encode(),
				UMPSignal::ApprovedPeer(peer(0xBB)).encode(),
				UMPSignal::Provides(StreamsRoot(Hash::repeat_byte(7))).encode(),
			]
		);
	}

	#[test]
	fn no_signals_emit_nothing() {
		assert!(collation_ump_signals(None, Vec::new()).unwrap().is_empty());
	}

	#[test]
	fn resubmission_without_block_signals_matches_validate_block() {
		let signed_info = signed(2, peer(0xCC));
		assert_eq!(
			collation_ump_signals(Some(&signed_info), Vec::new()).unwrap(),
			pvf_tail(Some(&signed_info), &[])
		);
	}

	#[test]
	fn rejected_signals_are_not_submitted() {
		// `validate_block` would reject a block-emitted `Requires`; the collator must not submit.
		let requires =
			RequiresSet::try_from_iter([(ParaId::from(1u32), StreamsRoot(Hash::repeat_byte(1)))])
				.unwrap();
		let raw = vec![UMPSignal::Requires(requires).encode()];
		assert_eq!(collation_ump_signals(None, raw), Err(SpecMessagingError::BlockEmittedRequires));
	}
}
