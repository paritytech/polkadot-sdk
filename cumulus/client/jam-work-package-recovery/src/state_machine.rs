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
//! a relay-chain candidate hash, driven by a work-report notification channel.  The node wiring
//! (todo 9) connects the notification streams from `sc-client-api`; this module is standalone.

use crate::{bundle_decode::decode_bundle, JamBundleRecovery, RecoveryDelayRange, RecoveryQueue};
use cumulus_jam_interface::{EpochIndex, WorkReportHash};
use futures::{channel::mpsc::Receiver, FutureExt, Stream, StreamExt};
use sp_additional_data::AdditionalData;
use sp_consensus::BlockStatus;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, NumberFor};
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

/// A work-report seen on the JAM chain that this node does not yet have locally.
pub struct WorkReportNotification<Block: BlockT> {
	/// Hash identifying the work report in the JAM DA layer.
	pub report_hash: WorkReportHash,
	/// Epoch in which the report was assured. Required by `recover_bundle`.
	pub assurance_epoch: EpochIndex,
	/// Expected parachain block number — used for finalization cleanup.
	pub block_number: NumberFor<Block>,
}

pub(crate) struct PendingReport<Block: BlockT> {
	assurance_epoch: EpochIndex,
	block_number: NumberFor<Block>,
	waiting_recovery: bool,
}

/// JAM work-package recovery state machine.
///
/// For each work report observed on the JAM chain whose blocks are not yet local, this engine
/// schedules randomised DA-layer bundle recovery, decodes the recovered bundle into parachain
/// blocks, and imports them in parent-first order.  A single retry is attempted before a
/// candidate is discarded.
pub struct JamWorkPackageRecovery<Block: BlockT> {
	pub(crate) outstanding: HashMap<WorkReportHash, PendingReport<Block>>,
	recovery_queue: RecoveryQueue,
	/// Blocks that arrived but whose parent is not yet known; keyed by the missing parent hash.
	pub(crate) waiting_for_parent: HashMap<Block::Hash, Vec<(Block, Option<AdditionalData>)>>,
	pub(crate) reports_in_retry: HashSet<WorkReportHash>,
	import_sink: Box<dyn ImportBlocksSink<Block>>,
	work_report_rx: Receiver<WorkReportNotification<Block>>,
}

impl<Block: BlockT> JamWorkPackageRecovery<Block> {
	/// Create a new instance.
	pub fn new(
		recovery_delay_range: RecoveryDelayRange,
		import_sink: Box<dyn ImportBlocksSink<Block>>,
		work_report_rx: Receiver<WorkReportNotification<Block>>,
	) -> Self {
		Self {
			outstanding: HashMap::new(),
			recovery_queue: RecoveryQueue::new(recovery_delay_range),
			waiting_for_parent: HashMap::new(),
			reports_in_retry: HashSet::new(),
			import_sink,
			work_report_rx,
		}
	}

	/// Observe a new work report; schedule its bundle for randomised recovery.
	pub(crate) fn handle_work_report(&mut self, notif: WorkReportNotification<Block>) {
		if self.outstanding.contains_key(&notif.report_hash) {
			return;
		}
		tracing::debug!(
			target: LOG_TARGET,
			report_hash = ?notif.report_hash,
			block_number = ?notif.block_number,
			"Scheduling work-report for bundle recovery",
		);
		self.outstanding.insert(
			notif.report_hash,
			PendingReport {
				assurance_epoch: notif.assurance_epoch,
				block_number: notif.block_number,
				waiting_recovery: true,
			},
		);
		self.recovery_queue.push_recovery(notif.report_hash);
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
			Err(e) => {
				tracing::warn!(target: LOG_TARGET, ?report_hash, "Bundle recovery error: {e}; dropping");
				self.outstanding.remove(&report_hash);
				return;
			},
			Ok(None) => {
				if self.reports_in_retry.insert(report_hash) {
					tracing::debug!(target: LOG_TARGET, ?report_hash, "Bundle not available; retrying once");
					self.recovery_queue.push_recovery(report_hash);
					return;
				} else {
					tracing::warn!(target: LOG_TARGET, ?report_hash, "Bundle still unavailable; dropping");
					self.reports_in_retry.remove(&report_hash);
					self.outstanding.remove(&report_hash);
					return;
				}
			},
			Ok(Some(b)) => b,
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

		tracing::debug!(target: LOG_TARGET, count = recovered.len(), "Importing recovered blocks");
		self.import_sink.import_blocks(recovered);
	}

	/// Discard outstanding reports at or below the finalized parachain block number.
	pub(crate) fn handle_finalized(&mut self, finalized_number: NumberFor<Block>) {
		self.outstanding.retain(|_, r| r.block_number > finalized_number);
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
	/// The caller (todo 9) connects the substrate notification streams from `sc-client-api`
	/// and provides a `block_status` closure backed by `BlockBackend::block_status`.
	pub async fn run(
		mut self,
		mut bundle_recovery: Box<dyn JamBundleRecovery>,
		block_status: impl Fn(Block::Hash) -> BlockStatus,
		import_notifications: impl Stream<Item = Block::Hash> + Unpin,
		finality_notifications: impl Stream<Item = NumberFor<Block>> + Unpin,
	) {
		let mut import_notifs = import_notifications.fuse();
		let mut finality_notifs = finality_notifications.fuse();
		loop {
			let recover_hash = futures::select! {
				notif = self.work_report_rx.next() => {
					match notif {
						Some(n) => { self.handle_work_report(n); None },
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
				n = finality_notifs.next() => {
					match n {
						Some(n) => { self.handle_finalized(n); None },
						None => return,
					}
				},
			};

			if let Some(hash) = recover_hash {
				let epoch = match self.outstanding.get(&hash) {
					Some(r) if r.waiting_recovery => r.assurance_epoch,
					_ => continue,
				};
				tracing::debug!(target: LOG_TARGET, ?hash, "Issuing bundle recovery");
				let result = bundle_recovery.recover_bundle(hash, epoch).await;
				self.handle_recovered_inner(hash, result, &block_status);
			}
		}
	}
}
