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
	bundle_provides, ump_signal_tail, CollationInfo, CollectCollationInfo, ParachainBlockData,
	SchedulingProof, SchedulingSignalError, SchedulingSignals, SignedSchedulingInfo,
	SpecMessagingError, SpecMessagingSignals,
};

use cumulus_primitives_spec_messaging::{ConsumptionRecord, LiftsBySource};
use polkadot_primitives::StreamsRoot;
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

/// What the speculative-messaging node side supplies for a collation: the blocks' consumption
/// records, in bundle order, and the lifts binding them to committed source roots.
#[derive(Debug, Default)]
pub struct SpecMsgInputs {
	/// One record per block, in bundle order.
	pub records: Vec<ConsumptionRecord>,
	/// The lifts the PoV carries; empty when the blocks consumed nothing.
	pub lifts: LiftsBySource,
}

/// Supplies [`SpecMsgInputs`] for a collation's blocks. Nodes that run speculative messaging set
/// one with [`CollatorService::with_spec_msg_assembler`]; without one, a collation carries no
/// records and no lifts, which is correct for blocks that consumed nothing.
pub type SpecMsgAssembler<Block> =
	Arc<dyn Fn(&[Block]) -> Result<SpecMsgInputs, String> + Send + Sync>;

/// Why a collation's UMP signal tail cannot be built: `validate_block` would panic on it.
#[derive(Debug, PartialEq, Eq)]
enum SignalTailError {
	Scheduling(SchedulingSignalError),
	SpecMessaging(SpecMessagingError),
}

impl core::fmt::Display for SignalTailError {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		match self {
			Self::Scheduling(error) => error.fmt(f),
			Self::SpecMessaging(error) => error.fmt(f),
		}
	}
}

/// The candidate's UMP signal tail, byte-identical to `validate_block`'s from the same records and
/// lifts, so a rejected candidate is never submitted. `provides` comes from the blocks' digests
/// ([`bundle_provides`]).
fn collation_ump_signals(
	signed_scheduling_info: Option<&SignedSchedulingInfo>,
	provides: Option<StreamsRoot>,
	upward_message_signals: Vec<Vec<u8>>,
	inputs: &SpecMsgInputs,
) -> Result<Vec<Vec<u8>>, SignalTailError> {
	let spec_messaging = SpecMessagingSignals::build(
		provides,
		&upward_message_signals,
		&inputs.records,
		Some(&inputs.lifts),
	)
	.map_err(SignalTailError::SpecMessaging)?;
	// The same parse the PVF applies: a signed scheduling info (resubmission) replaces the
	// block's *scheduling* signals wholesale; otherwise they are re-encoded in canonical order.
	let scheduling = match signed_scheduling_info {
		Some(signed_info) => SchedulingSignals::from_scheduling_info(signed_info),
		None => SchedulingSignals::try_from_block_signals(&upward_message_signals)
			.map_err(SignalTailError::Scheduling)?,
	};
	Ok(ump_signal_tail(scheduling, spec_messaging))
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
	spec_msg_assembler: Option<SpecMsgAssembler<Block>>,
}

impl<Block: BlockT, BS, RA> Clone for CollatorService<Block, BS, RA> {
	fn clone(&self) -> Self {
		Self {
			block_status: self.block_status.clone(),
			announce_block: self.announce_block.clone(),
			runtime_api: self.runtime_api.clone(),
			spec_msg_assembler: self.spec_msg_assembler.clone(),
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
		Self { block_status, announce_block, runtime_api, spec_msg_assembler: None }
	}

	/// Supply speculative-messaging records and lifts to every collation. Only nodes that run
	/// speculative messaging need this.
	pub fn with_spec_msg_assembler(mut self, assembler: SpecMsgAssembler<Block>) -> Self {
		self.spec_msg_assembler = Some(assembler);
		self
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

		let provides = bundle_provides(blocks.iter().map(|block| block.header().digest()));

		// The speculative-messaging inputs for these blocks. A collation `validate_block` would
		// reject is not worth submitting, so a failure skips it.
		let spec_msg_inputs = match &self.spec_msg_assembler {
			Some(assemble) => match assemble(&blocks) {
				Ok(inputs) => inputs,
				Err(error) => {
					tracing::error!(
						target: LOG_TARGET,
						%error,
						"Failed to assemble speculative-messaging lifts; not submitting the collation.",
					);
					return None;
				},
			},
			None => SpecMsgInputs::default(),
		};

		// Lifts ride in the PoV, which needs the V3 format; without them, keep the older ones.
		let block_data = if spec_msg_inputs.lifts.is_empty() {
			ParachainBlockData::<Block>::new(blocks, compact_proof, scheduling_proof)
		} else {
			ParachainBlockData::<Block>::new_with_lifts(
				blocks,
				compact_proof,
				scheduling_proof,
				spec_msg_inputs.lifts.clone(),
			)
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

		// Emit the UMP signal tail, as `validate_block` will.
		match collation_ump_signals(
			signed_scheduling_info.as_ref(),
			provides,
			upward_message_signals,
			&spec_msg_inputs,
		) {
			Ok(tail) => {
				let consuming =
					spec_msg_inputs.records.iter().filter(|r| !r.entries.is_empty()).count();
				if consuming > 0 {
					tracing::debug!(
						target: LOG_TARGET,
						consuming,
						signals = tail.len(),
						"Collation carries a `Requires` over the consumption of {consuming} blocks.",
					);
				}
				upward_messages.extend(tail)
			},
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
		StreamsRoot, UMPSignal, UMP_SEPARATOR,
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

	/// `validate_block`'s verdict on the same signals, records and PoV lifts (`None` for a V1/V2
	/// PoV): its tail, or `None` where it panics. Mirrors the tail assembly in
	/// `validate_block::implementation`.
	fn pvf_verdict(
		signed_info: Option<&SignedSchedulingInfo>,
		provides: Option<StreamsRoot>,
		raw: &[Vec<u8>],
		records: &[ConsumptionRecord],
		pov_lifts: Option<&LiftsBySource>,
	) -> Option<Vec<Vec<u8>>> {
		quiet_pvf_panics();
		IN_PVF.with(|in_pvf| in_pvf.set(true));
		let verdict = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			let scheduling = match signed_info {
				Some(signed_info) => SchedulingSignals::from_scheduling_info(signed_info),
				None => SchedulingSignals::from_block_signals(raw),
			};
			let spec_messaging = SpecMessagingSignals::build(provides, raw, records, pov_lifts)
				.unwrap_or_else(|error| panic!("{}", error));
			ump_signal_tail(scheduling, spec_messaging)
		}));
		IN_PVF.with(|in_pvf| in_pvf.set(false));
		verdict.ok()
	}

	/// What `validate_block` emits for the same signals, records and lifts.
	fn pvf_tail(
		signed_info: Option<&SignedSchedulingInfo>,
		provides: Option<StreamsRoot>,
		raw: &[Vec<u8>],
		inputs: &SpecMsgInputs,
	) -> Vec<Vec<u8>> {
		pvf_verdict(signed_info, provides, raw, &inputs.records, Some(&inputs.lifts))
			.expect("validate_block accepts these inputs")
	}

	std::thread_local! {
		/// Whether this thread is inside `pvf_verdict`, whose panics are verdicts, not failures.
		static IN_PVF: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
	}

	/// Silence the panics `pvf_verdict` catches, and only those.
	fn quiet_pvf_panics() {
		static ONCE: std::sync::Once = std::sync::Once::new();
		ONCE.call_once(|| {
			let previous = std::panic::take_hook();
			std::panic::set_hook(Box::new(move |info| {
				if !IN_PVF.with(|in_pvf| in_pvf.get()) {
					previous(info)
				}
			}));
		});
	}

	fn no_inputs() -> SpecMsgInputs {
		SpecMsgInputs::default()
	}

	/// A block that consumed `n` messages of one channel stream from source 2000, and the identity
	/// lift binding them to the source's committed `StreamsRoot`.
	fn consumed(n: u64) -> (SpecMsgInputs, StreamsRoot) {
		use cumulus_primitives_spec_messaging::{
			streams_root::{gen_stream_proof, streams_root},
			Interval, MMRExtensionProof, MmrFrontier, MmrRoot, RequiresLift, StreamId,
		};
		use std::collections::BTreeMap;

		let source = ParaId::from(2000u32);
		let stream = StreamId::Channel { recipient: ParaId::from(1000u32), domain: 0, num: 0 };
		let mut frontier = MmrFrontier::new();
		for i in 0..n {
			frontier.append(Hash::from_low_u64_be(i + 1));
		}
		let entries = BTreeMap::from([(stream, frontier.root().0)]);
		let committed = streams_root(&entries).unwrap();
		let (_, tree_proof) = gen_stream_proof(&entries, stream).unwrap();
		let record = ConsumptionRecord {
			entries: BTreeMap::from([(
				source,
				vec![(stream, Interval { start: MmrRoot(Hash::zero()), end: frontier.clone() })],
			)]),
		};
		let lift = RequiresLift {
			advances: Vec::new(),
			extension: MMRExtensionProof::identity(&frontier),
			tree_proof,
		};
		let lifts = LiftsBySource::try_from(BTreeMap::from([(source, vec![lift])])).unwrap();
		(SpecMsgInputs { records: vec![record], lifts }, committed)
	}

	fn block_signals() -> Vec<Vec<u8>> {
		vec![
			UMPSignal::SelectCore(CoreSelector(0), ClaimQueueOffset(0)).encode(),
			UMPSignal::ApprovedPeer(peer(0xAA)).encode(),
		]
	}

	fn root() -> Option<StreamsRoot> {
		Some(StreamsRoot(Hash::repeat_byte(7)))
	}

	#[test]
	fn plain_collation_matches_validate_block() {
		let raw = block_signals();
		for provides in [None, root()] {
			assert_eq!(
				collation_ump_signals(None, provides, raw.clone(), &no_inputs()).unwrap(),
				pvf_tail(None, provides, &raw, &no_inputs())
			);
		}
		// `Provides` alone still gets its separator.
		assert_eq!(
			collation_ump_signals(None, root(), Vec::new(), &no_inputs()).unwrap(),
			pvf_tail(None, root(), &[], &no_inputs())
		);
	}

	#[test]
	fn resubmission_keeps_provides_and_matches_validate_block() {
		let raw = block_signals();
		let signed_info = signed(5, peer(0xBB));

		let tail =
			collation_ump_signals(Some(&signed_info), root(), raw.clone(), &no_inputs()).unwrap();
		assert_eq!(tail, pvf_tail(Some(&signed_info), root(), &raw, &no_inputs()));
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
		assert!(collation_ump_signals(None, None, Vec::new(), &no_inputs()).unwrap().is_empty());
	}

	#[test]
	fn resubmission_without_block_signals_matches_validate_block() {
		let signed_info = signed(2, peer(0xCC));
		assert_eq!(
			collation_ump_signals(Some(&signed_info), None, Vec::new(), &no_inputs()).unwrap(),
			pvf_tail(Some(&signed_info), None, &[], &no_inputs())
		);
	}

	#[test]
	fn rejected_signals_are_not_submitted() {
		// `validate_block` would reject a block-emitted `Requires`; the collator must not submit.
		let requires =
			RequiresSet::try_from_iter([(ParaId::from(1u32), StreamsRoot(Hash::repeat_byte(1)))])
				.unwrap();
		let raw = vec![UMPSignal::Requires(requires).encode()];
		let signed_info = signed(2, peer(0xCC));
		assert_eq!(
			collation_ump_signals(Some(&signed_info), None, raw, &no_inputs()),
			Err(SignalTailError::SpecMessaging(SpecMessagingError::BlockEmittedSpecMsgSignal))
		);
	}

	#[test]
	fn a_consuming_collation_appends_requires_like_validate_block() {
		let raw = block_signals();
		let (inputs, committed) = consumed(5);
		let tail = collation_ump_signals(None, None, raw.clone(), &inputs).unwrap();
		assert_eq!(tail, pvf_tail(None, None, &raw, &inputs));
		// `Requires` comes last and names the source's committed root.
		let requires = RequiresSet::try_from_iter([(ParaId::from(2000u32), committed)]).unwrap();
		assert_eq!(tail.last(), Some(&UMPSignal::Requires(requires).encode()));
	}

	#[test]
	fn a_consuming_resubmission_matches_validate_block() {
		let raw = block_signals();
		let signed_info = signed(3, peer(0xDD));
		let (inputs, _) = consumed(2);
		assert_eq!(
			collation_ump_signals(Some(&signed_info), None, raw.clone(), &inputs).unwrap(),
			pvf_tail(Some(&signed_info), None, &raw, &inputs)
		);
	}

	#[test]
	fn block_signals_out_of_runtime_order_match_validate_block() {
		// `validate_block` re-emits the block's scheduling signals as `SelectCore`, `ApprovedPeer`
		// whatever order the block appended them in.
		let raw: Vec<_> = block_signals().into_iter().rev().collect();
		assert_eq!(
			collation_ump_signals(None, root(), raw.clone(), &no_inputs()).ok(),
			pvf_verdict(None, root(), &raw, &[], None)
		);
	}

	#[test]
	fn trailing_bytes_in_a_block_signal_match_validate_block() {
		// `validate_block` decodes the signal, ignoring what follows it, and re-encodes it.
		let mut raw = block_signals();
		raw[0].push(0xEE);
		assert_eq!(
			collation_ump_signals(None, None, raw.clone(), &no_inputs()).ok(),
			pvf_verdict(None, None, &raw, &[], None)
		);
	}

	#[test]
	fn duplicate_block_signals_are_not_submitted() {
		let mut raw = block_signals();
		raw.push(raw[0].clone());
		assert_eq!(pvf_verdict(None, None, &raw, &[], None), None);
		assert_eq!(
			collation_ump_signals(None, None, raw.clone(), &no_inputs()),
			Err(SignalTailError::Scheduling(SchedulingSignalError::DuplicateSelectCore))
		);
		// A resubmission ignores the block's scheduling signals, on both sides.
		let signed_info = signed(2, peer(0xCC));
		assert_eq!(
			collation_ump_signals(Some(&signed_info), None, raw.clone(), &no_inputs()).ok(),
			pvf_verdict(Some(&signed_info), None, &raw, &[], None)
		);
	}

	/// Every combination of scheduling override, block signals (none, either, both, out of
	/// order, duplicated, with trailing bytes, spec-messaging, undecodable), `Provides` and
	/// records/lifts: the collator submits exactly the tail `validate_block` emits, and refuses
	/// whatever `validate_block` rejects. Every accepted tail passes the relay's decoder.
	#[test]
	fn every_input_matches_validate_block() {
		let select_core = |n| UMPSignal::SelectCore(CoreSelector(n), ClaimQueueOffset(0)).encode();
		let mut trailing = select_core(0);
		trailing.push(0xEE);
		let alphabet = [
			select_core(0),
			select_core(1),
			UMPSignal::ApprovedPeer(peer(0xAA)).encode(),
			trailing,
			UMPSignal::Provides(StreamsRoot(Hash::repeat_byte(3))).encode(),
			vec![0xFF],
		];
		let mut raws = vec![Vec::<Vec<u8>>::new()];
		let mut layer = raws.clone();
		for _ in 0..3 {
			layer = layer
				.iter()
				.flat_map(|prefix| {
					alphabet.iter().map(move |signal| {
						let mut raw = prefix.clone();
						raw.push(signal.clone());
						raw
					})
				})
				.collect();
			raws.extend(layer.iter().cloned());
		}

		let (consuming, _) = consumed(3);
		let input_sets = [
			("nothing", no_inputs()),
			("consuming", consumed(3).0),
			(
				"records without lifts",
				SpecMsgInputs { records: consuming.records.clone(), lifts: Default::default() },
			),
			(
				"lifts without records",
				SpecMsgInputs { records: Vec::new(), lifts: consuming.lifts },
			),
			(
				"idle records",
				SpecMsgInputs { records: vec![Default::default(); 2], lifts: Default::default() },
			),
		];

		let mut cases = 0;
		let mut mismatches = Vec::new();
		for signed_info in [None, Some(signed(4, peer(0xEE)))] {
			for provides in [None, root()] {
				for (name, inputs) in &input_sets {
					for raw in &raws {
						cases += 1;
						let collator = collation_ump_signals(
							signed_info.as_ref(),
							provides,
							raw.clone(),
							inputs,
						)
						.ok();
						// The collator ships empty lifts as a V1/V2 PoV; a V3 PoV with empty lifts
						// must not make a difference.
						let pov_lifts = (!inputs.lifts.is_empty()).then_some(&inputs.lifts);
						let pvf = pvf_verdict(
							signed_info.as_ref(),
							provides,
							raw,
							&inputs.records,
							pov_lifts,
						);
						let pvf_v3 = pvf_verdict(
							signed_info.as_ref(),
							provides,
							raw,
							&inputs.records,
							Some(&inputs.lifts),
						);
						assert_eq!(pvf, pvf_v3, "{name}: {raw:?}");
						if collator != pvf {
							mismatches.push((name, signed_info.is_some(), provides, raw.clone()));
							continue;
						}
						let Some(tail) = pvf else { continue };
						let commitments = polkadot_primitives::CandidateCommitments {
							upward_messages: tail.clone().try_into().unwrap(),
							..Default::default()
						};
						commitments.ump_signals().unwrap_or_else(|error| {
							panic!("relay rejects {tail:?} ({name}, {raw:?}): {error:?}")
						});
						assert!(
							tail.len() <= 1 + polkadot_primitives::v9::MAX_UMP_SIGNALS,
							"{name}: {raw:?}"
						);
					}
				}
			}
		}
		assert_eq!(cases, 2 * 2 * input_sets.len() * raws.len());
		assert!(
			mismatches.is_empty(),
			"{} of {cases} cases diverge from validate_block, e.g. {:?}",
			mismatches.len(),
			&mismatches[..mismatches.len().min(3)]
		);
	}

	#[test]
	fn records_without_lifts_are_not_submitted() {
		let (mut inputs, _) = consumed(1);
		inputs.lifts = LiftsBySource::default();
		assert!(matches!(
			collation_ump_signals(None, None, block_signals(), &inputs),
			Err(SignalTailError::SpecMessaging(SpecMessagingError::RequiresSynthesis(_)))
		));
	}
}
