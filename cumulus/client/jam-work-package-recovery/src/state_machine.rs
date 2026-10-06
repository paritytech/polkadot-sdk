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

//! JAM work-package recovery state machine.
//!
//! Follows pov-recovery's architecture adapted for JAM: keyed by `WorkReportHash` rather than
//! a relay-chain candidate hash. On every JAM best block the engine reads the availability
//! assignments (state key C(10)), picks the reports refined for our para and schedules the
//! recovery of every one whose block is not known locally.

use crate::{
	bundle_decode::{bundle_work_package_hash, decode_bundle},
	JamBundleRecovery, RecoveryDelayRange, RecoveryQueue,
};
use codec::{Decode, DecodeAll};
use cumulus_jam_interface::{
	EpochIndex, HeaderHash, JamChainSource, JamStateSource, ServiceId, Slot, VersionedParameters,
	WorkPackageHash, WorkReport, WorkReportHash,
};
use futures::{FutureExt, Stream, StreamExt};
use sp_additional_data::AdditionalData;
use sp_consensus::BlockStatus;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use std::collections::{HashMap, HashSet, VecDeque};

const LOG_TARGET: &str = "cumulus-jam-work-package-recovery";

/// Decoded and demultiplexed block ready to be forwarded to the node's import pipeline.
///
/// Equivalent to `sc_consensus::IncomingBlock` without the heavy sc-network dependency.
/// Todo 9 converts these into full `IncomingBlock` structs for the node import queue.
pub struct RecoveredBlock<Block: BlockT> {
	pub hash: Block::Hash,
	pub header: Block::Header,
	pub body: Vec<Block::Extrinsic>,
	pub additional_data: Option<AdditionalData>,
}

/// Lightweight sink for recovered blocks.
///
/// Implemented by the node glue (todo 9) using `sc_consensus::ImportQueueService`. The
/// test mock stores hashes for assertion.
pub trait ImportBlocksSink<Block: BlockT>: Send {
	fn import_blocks(&mut self, blocks: Vec<RecoveredBlock<Block>>);
}

/// Reports the authentic hash of a recovered bundle, keyed by each block it carries.
///
/// The node wires this to its durable hash ledger. Recovery is the only legitimate source of
/// another collator's hash: the bundle holds the author's own signed bytes, so the hash over
/// them is the one JAM recognises. `Err` means the ledger write failed; the engine logs it and
/// carries on, because losing one entry only restarts a chain while aborting loses a block.
pub type RecoveredHashFn<Block> =
	Box<dyn Fn(<Block as BlockT>::Hash, WorkPackageHash) -> Result<(), String> + Send + Sync>;

pub(crate) struct PendingReport<Block: BlockT> {
	pub(crate) assurance_epoch: EpochIndex,
	/// The para block the report carries, read from its refine output.
	pub(crate) block_hash: Block::Hash,
}

/// The para id and head (the block's header hash) a parachain-service refine output carries.
///
/// The output is the SCALE-encoded `ParachainWorkDigest`; only its `Ok` variant (index 0) names
/// a head: `para_id`, the validation code hash, the parent head hash, then the head data.
pub(crate) fn decode_reported_head<Hash: Decode>(output: &[u8]) -> Option<(u32, Hash)> {
	let mut input = output;
	if u8::decode(&mut input).ok()? != 0 {
		return None;
	}
	let para_id = u32::decode(&mut input).ok()?;
	let _validation_code = <[u8; 32]>::decode(&mut input).ok()?;
	let _parent_head_hash = <[u8; 32]>::decode(&mut input).ok()?;
	let head_data = Vec::<u8>::decode(&mut input).ok()?;
	Some((para_id, Hash::decode_all(&mut &head_data[..]).ok()?))
}

/// JAM work-package recovery state machine.
///
/// Reads the availability assignments of every new JAM best block to discover the work reports
/// of our para. For each one whose block is not yet local, the engine schedules randomised
/// DA-layer bundle recovery, decodes the recovered bundle into parachain blocks, and imports
/// them in parent-first order. A single retry is attempted before a candidate is discarded.
pub struct JamWorkPackageRecovery<Block: BlockT> {
	pub(crate) outstanding: HashMap<WorkReportHash, PendingReport<Block>>,
	pub(crate) recovery_queue: RecoveryQueue,
	/// Blocks that arrived but whose parent is not yet known; keyed by the missing parent hash.
	pub(crate) waiting_for_parent: HashMap<Block::Hash, Vec<(Block, Option<AdditionalData>)>>,
	pub(crate) reports_in_retry: HashSet<WorkReportHash>,
	import_sink: Box<dyn ImportBlocksSink<Block>>,
	on_recovered: RecoveredHashFn<Block>,
	jam_chain: Box<dyn JamChainSource>,
	jam_state: Box<dyn JamStateSource>,
	/// The parachain service whose reports carry our blocks.
	service_id: ServiceId,
	/// Our para; the service refines every para hosted by it.
	para_id: u32,
	/// The JAM epoch length, read once from the chain parameters.
	epoch_period: Option<Slot>,
}

impl<Block: BlockT> JamWorkPackageRecovery<Block> {
	/// Create a new instance.
	pub fn new(
		recovery_delay_range: RecoveryDelayRange,
		import_sink: Box<dyn ImportBlocksSink<Block>>,
		on_recovered: RecoveredHashFn<Block>,
		jam_chain: Box<dyn JamChainSource>,
		jam_state: Box<dyn JamStateSource>,
		service_id: ServiceId,
		para_id: u32,
	) -> Self {
		Self {
			outstanding: HashMap::new(),
			recovery_queue: RecoveryQueue::new(recovery_delay_range),
			waiting_for_parent: HashMap::new(),
			reports_in_retry: HashSet::new(),
			import_sink,
			on_recovered,
			jam_chain,
			jam_state,
			service_id,
			para_id,
			epoch_period: None,
		}
	}

	async fn epoch_period(&mut self) -> cumulus_jam_interface::Result<Slot> {
		if let Some(epoch_period) = self.epoch_period {
			return Ok(epoch_period);
		}
		let epoch_period = match self.jam_chain.parameters().await? {
			VersionedParameters::V1(parameters) => parameters.epoch_period,
		};
		self.epoch_period = Some(epoch_period);
		Ok(epoch_period)
	}

	/// Schedule recovery for the reports of our para JAM holds in availability at `at`.
	pub(crate) async fn discover_work_reports(
		&mut self,
		at: HeaderHash,
		block_status: &impl Fn(Block::Hash) -> BlockStatus,
	) -> cumulus_jam_interface::Result<()> {
		let epoch_period = self.epoch_period().await?;
		let availability = self.jam_state.availability(at).await?;
		for assignment in availability.iter().flatten() {
			let assurance_epoch = assignment.guarantee.slot / epoch_period;
			self.observe_report(assignment.report(), assurance_epoch, block_status);
		}
		Ok(())
	}

	/// Schedule the recovery of `report` if it carries a block of our para that is not known.
	pub(crate) fn observe_report(
		&mut self,
		report: &WorkReport,
		assurance_epoch: EpochIndex,
		block_status: &impl Fn(Block::Hash) -> BlockStatus,
	) {
		let Some(output) = report
			.results
			.iter()
			.find(|digest| digest.service == self.service_id)
			.and_then(|digest| digest.result.as_ref().ok())
		else {
			return;
		};
		let Some((para_id, block_hash)) = decode_reported_head::<Block::Hash>(&output.0) else {
			return;
		};
		if para_id != self.para_id {
			return;
		}
		let report_hash = report.hash();
		if self.outstanding.contains_key(&report_hash) ||
			block_status(block_hash) != BlockStatus::Unknown
		{
			return;
		}

		tracing::debug!(
			target: LOG_TARGET,
			?report_hash,
			?block_hash,
			assurance_epoch,
			"Scheduling recovery of a work-report whose block is not known locally",
		);
		self.outstanding
			.insert(report_hash, PendingReport { assurance_epoch, block_hash });
		self.recovery_queue.push_recovery(report_hash);
	}

	/// Handle a bundle recovery result.
	///
	/// `block_status_fn` is called with the parent hash of the first recovered block to decide
	/// whether to import immediately or defer to `waiting_for_parent`.
	pub(crate) fn handle_recovered_inner(
		&mut self,
		report_hash: WorkReportHash,
		result: Result<Option<Vec<u8>>, String>,
		block_status_fn: impl Fn(Block::Hash) -> BlockStatus,
	) {
		let bytes = match result {
			Ok(Some(b)) => b,
			Ok(None) | Err(_) => {
				if let Err(error) = &result {
					tracing::debug!(target: LOG_TARGET, ?report_hash, "Bundle recovery error: {error}");
				}
				if self.reports_in_retry.insert(report_hash) {
					tracing::debug!(target: LOG_TARGET, ?report_hash, "Bundle not available; retrying once");
					self.recovery_queue.push_recovery(report_hash);
				} else {
					tracing::warn!(target: LOG_TARGET, ?report_hash, "Bundle still unavailable; dropping");
					self.reports_in_retry.remove(&report_hash);
					self.outstanding.remove(&report_hash);
				}
				return;
			},
		};

		self.reports_in_retry.remove(&report_hash);

		let blocks_and_data = match decode_bundle::<Block>(&bytes) {
			Ok(v) => v,
			Err(e) => {
				tracing::warn!(target: LOG_TARGET, ?report_hash, "Bundle decode failed: {e}; dropping");
				self.outstanding.remove(&report_hash);
				return;
			},
		};

		// The bundle bytes are the author's own signed package, so the hash over them is
		// authentic; recovery is the only way another collator's hash can enter the ledger.
		// One call per carried block keeps the tip — the block a child is built on — recorded.
		match bundle_work_package_hash(&bytes) {
			Ok(wp_hash) => {
				for (block, _) in &blocks_and_data {
					let block_hash = block.hash();
					if let Err(error) = (self.on_recovered)(block_hash, wp_hash) {
						tracing::warn!(
							target: LOG_TARGET,
							?report_hash,
							?block_hash,
							"Recording the recovered work-package hash failed: {error}; continuing",
						);
					}
				}
			},
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					?report_hash,
					"Recovered bundle hash failed: {error}; continuing without recording",
				);
			},
		}

		let parent_hash = match blocks_and_data.first().map(|(b, _)| *b.header().parent_hash()) {
			Some(h) => h,
			None => {
				tracing::warn!(target: LOG_TARGET, ?report_hash, "No blocks in recovered bundle; dropping");
				self.outstanding.remove(&report_hash);
				return;
			},
		};

		self.outstanding.remove(&report_hash);

		match block_status_fn(parent_hash) {
			BlockStatus::Unknown => {
				tracing::debug!(
					target: LOG_TARGET,
					?report_hash,
					parent = ?parent_hash,
					"Parent unknown; deferring until parent arrives",
				);
				self.waiting_for_parent.entry(parent_hash).or_default().extend(blocks_and_data);
			},
			_ => self.import_blocks(blocks_and_data.into_iter()),
		}
	}

	/// Import blocks and recursively drain any blocks waiting for these hashes as their parent.
	pub(crate) fn import_blocks(
		&mut self,
		blocks: impl Iterator<Item = (Block, Option<AdditionalData>)>,
	) {
		let mut queue = VecDeque::from_iter(blocks);
		let mut recovered = Vec::new();

		while let Some((block, additional_data)) = queue.pop_front() {
			let hash = block.hash();
			let (header, body) = block.deconstruct();
			if let Some(waiting) = self.waiting_for_parent.remove(&hash) {
				queue.extend(waiting);
			}
			recovered.push(RecoveredBlock { hash, header, body, additional_data });
		}

		tracing::debug!(
			target: LOG_TARGET,
			blocks = ?recovered.iter().map(|b| b.hash).collect::<Vec<_>>(),
			"Importing blocks retrieved using JAM work-package recovery",
		);
		self.import_sink.import_blocks(recovered);
	}

	/// A block was imported; drain any children that were waiting for it.
	pub(crate) fn handle_imported(&mut self, block_hash: Block::Hash) {
		if let Some(waiting) = self.waiting_for_parent.remove(&block_hash) {
			tracing::debug!(
				target: LOG_TARGET,
				?block_hash,
				children = waiting.len(),
				"Parent imported; queuing waiting children",
			);
			self.import_blocks(waiting.into_iter());
		}
	}

	/// Run the recovery engine until an input stream ends.
	///
	/// Follows `best_block_stream` from the JAM chain source to discover new work reports, and
	/// the substrate import notifications to drain blocks waiting for their parent.
	pub async fn run(
		mut self,
		mut bundle_recovery: Box<dyn JamBundleRecovery>,
		block_status: impl Fn(Block::Hash) -> BlockStatus,
		import_notifications: impl Stream<Item = Block::Hash> + Unpin,
	) {
		let best_block_stream = match self.jam_chain.best_block_stream().await {
			Ok(s) => s.fuse(),
			Err(e) => {
				tracing::error!(
					target: LOG_TARGET,
					error = ?e,
					"Unable to open JAM best-block stream",
				);
				return;
			},
		};

		let mut import_notifs = import_notifications.fuse();
		futures::pin_mut!(best_block_stream);
		loop {
			let recover_hash = futures::select! {
				jam_block = best_block_stream.next() => {
					match jam_block {
						Some(b) => {
							if let Err(e) =
								self.discover_work_reports(b.header_hash, &block_status).await
							{
								tracing::warn!(
									target: LOG_TARGET,
									jam_hash = ?b.header_hash,
									"discover_work_reports failed: {e:?}",
								);
							}
							None
						},
						None => return,
					}
				},
				hash = self.recovery_queue.next_recovery().fuse() => Some(hash),
				hash = import_notifs.next() => {
					match hash {
						Some(h) => { self.handle_imported(h); None },
						None => return,
					}
				},
			};

			if let Some(hash) = recover_hash {
				let (epoch, block_hash) = match self.outstanding.get(&hash) {
					Some(r) => (r.assurance_epoch, r.block_hash),
					None => continue,
				};
				if block_status(block_hash) != BlockStatus::Unknown {
					tracing::debug!(
						target: LOG_TARGET,
						?hash,
						?block_hash,
						"Block already known; skipping recovery",
					);
					self.outstanding.remove(&hash);
					self.reports_in_retry.remove(&hash);
					continue;
				}
				tracing::debug!(target: LOG_TARGET, ?hash, "Issuing bundle recovery");
				let result = bundle_recovery.recover_bundle(hash, epoch).await;
				self.handle_recovered_inner(hash, result, &block_status);
			}
		}
	}
}
