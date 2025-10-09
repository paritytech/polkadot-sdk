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

//! Parachain informant service.
//!
//! Provides a service that logs information about parachain candidate events
//! and related metrics.

use std::{pin::Pin, sync::Arc, time::Instant};

use cumulus_primitives_core::{relay_chain::Header as RelayHeader, ParaId};
use cumulus_relay_chain_interface::{RelayChainInterface, RelayChainResult};
use futures::{FutureExt, StreamExt};
use polkadot_primitives::CandidateEvent;
use prometheus::{linear_buckets, Histogram, HistogramOpts, Registry};
use sc_service::TransactionMonitorEvent;
use sc_telemetry::log;
use schnellru::{ByLength, LruMap};
use sp_blockchain::HeaderBackend;
use sp_core::Decode;
use sp_runtime::{
	traits::{Block as BlockT, Header},
	SaturatedConversion, Saturating,
};

const LOG_TARGET: &str = "parachain_informant";

/// The maximum number of entries to keep in the LRU caches.
const LRU_LENGTH: u32 = 64;

/// Type alias for the transaction monitor event stream.
type TransactionMonitorEventStream<Hash> =
	Pin<Box<dyn futures::Stream<Item = TransactionMonitorEvent<Hash>> + Unpin + Send>>;

/// The parachain informant service.
pub struct ParachainInformant<Block: BlockT> {
	/// Relay chain interface to interact with the relay chain.
	relay_chain_interface: Arc<dyn RelayChainInterface>,

	/// Client to access the blockchain headers.
	client: Arc<dyn HeaderBackend<Block>>,

	/// Optional metrics for the parachain informant.
	metrics: Option<ParachainInformantMetrics>,

	/// Parachain ID of the parachain this informant is running for.
	para_id: ParaId,

	/// Last time a block was backed.
	last_backed_block_time: Option<Instant>,

	/// Cache for storing the last backed blocks.
	backed_blocks: LruMap<sp_core::H256, ()>,

	/// Cache for storing transactions not yet backed.
	unresolved_tx: LruMap<sp_core::H256, Vec<Instant>>,

	/// Stream of transaction events from RPC transaction v2 handles.
	transaction_v2_handle: TransactionMonitorEventStream<Block::Hash>,
}

impl<Block: BlockT> ParachainInformant<Block> {
	pub fn new(
		relay_chain_interface: Arc<dyn RelayChainInterface>,
		client: Arc<dyn HeaderBackend<Block>>,
		registry: Option<&Registry>,
		para_id: ParaId,
		rpc_transaction_v2_handles: Vec<sc_service::TransactionMonitorHandle<Block::Hash>>,
	) -> sc_service::error::Result<Self> {
		let metrics = registry.map(|r| ParachainInformantMetrics::new(r)).transpose()?;

		let transaction_v2_handle: Pin<
			Box<dyn futures::Stream<Item = TransactionMonitorEvent<Block::Hash>> + Unpin + Send>,
		> = if rpc_transaction_v2_handles.is_empty() {
			Box::pin(futures::stream::pending())
		} else {
			Box::pin(futures::stream::select_all(rpc_transaction_v2_handles))
		};

		Ok(Self {
			relay_chain_interface,
			client,
			metrics,
			para_id,
			last_backed_block_time: None,
			backed_blocks: LruMap::new(ByLength::new(LRU_LENGTH)),
			unresolved_tx: LruMap::new(ByLength::new(LRU_LENGTH)),
			transaction_v2_handle,
		})
	}

	/// Run the parachain informant service.
	pub async fn run(mut self) -> RelayChainResult<()> {
		let mut import_notifications =
			self.relay_chain_interface.import_notification_stream().await.inspect_err(|e| {
				log::error!(
					target: LOG_TARGET,
					"Failed to get import notification stream: {e:?}. Parachain informant will not run!"
				);
			})?;

		loop {
			futures::select! {
				notification = import_notifications.next().fuse() => {
					let Some(notification) = notification else { return Ok(()) };

					self.handle_import_notification(notification).await;
				},

				tx_event = self.transaction_v2_handle.next().fuse() => {
					let Some(tx_event) = tx_event else { continue };

					 log::debug!(target: LOG_TARGET, "Received transaction event: {:?}", tx_event);

					self.handle_rpc_monitor_event(tx_event);
				},
			}
		}
	}

	/// Handle an import notification from the relay chain.
	///
	/// Ensures the RPC backed blocks reflect into the metrics and
	/// performs the parachain logging.
	async fn handle_import_notification(&mut self, n: RelayHeader) {
		let candidate_events = match self.relay_chain_interface.candidate_events(n.hash()).await {
			Ok(candidate_events) => candidate_events,
			Err(e) => {
				log::warn!(target: LOG_TARGET, "Failed to get candidate events for block {}: {e:?}", n.hash());
				return;
			},
		};

		self.handle_rpc_backed_blocks(candidate_events.iter());

		self.handle_logging(candidate_events, &n);
	}

	/// Handle a transaction event from the RPC transaction monitor.
	///
	/// If the transaction is included in a backed block a metric is recorded.
	/// Otherwise, the transaction is stored in an unresolved transactions cache
	/// until the block is backed.
	fn handle_rpc_monitor_event(&mut self, event: TransactionMonitorEvent<Block::Hash>) {
		let (block_hash, submitted_at) = match event {
			sc_service::TransactionMonitorEvent::InBlock { block_hash, submitted_at } =>
				(sp_core::H256::from_slice(block_hash.as_ref()), submitted_at),
		};

		if self.backed_blocks.peek(&block_hash).is_some() {
			if let Some(metrics) = &self.metrics {
				metrics
					.transaction_backed_duration
					.observe(submitted_at.elapsed().as_secs_f64());
			}
		} else {
			// Received the transaction before the block is backed.
			self.unresolved_tx
				.get_or_insert(block_hash, || Vec::new())
				.map(|pending| pending.push(submitted_at));
		}
	}

	/// Handle the RPC metrics for backed blocks.
	fn handle_rpc_backed_blocks<'a>(&mut self, events: impl Iterator<Item = &'a CandidateEvent>) {
		let blocks = events.filter_map(|event| match event {
			CandidateEvent::CandidateBacked(receipt, ..)
				if receipt.descriptor.para_id() == self.para_id =>
				Some(receipt.descriptor.para_head()),
			_ => None,
		});

		for block in blocks {
			if self.backed_blocks.insert(block, ()) {
				log::trace!(target: LOG_TARGET, "New backed block: {:?}", block);
			}

			if let Some(tx_times) = self.unresolved_tx.remove(&block) {
				for submitted_at in tx_times {
					if let Some(metrics) = &self.metrics {
						metrics
							.transaction_backed_duration
							.observe(submitted_at.elapsed().as_secs_f64());
					}
				}
			}
		}
	}

	/// Handle candidate events and log the results.
	fn handle_logging(&mut self, candidate_events: Vec<CandidateEvent>, n: &RelayHeader) {
		let mut backed_candidates = Vec::new();
		let mut included_candidates = Vec::new();
		let mut timed_out_candidates = Vec::new();

		for event in candidate_events {
			match event {
				CandidateEvent::CandidateBacked(receipt, head, _, _) => {
					if receipt.descriptor.para_id() != self.para_id {
						continue;
					}

					let backed_block = match Block::Header::decode(&mut &head.0[..]) {
						Ok(header) => header,
						Err(e) => {
							log::warn!(
								target: LOG_TARGET,
								"Failed to decode parachain header from backed block: {e:?}"
							);
							continue
						},
					};
					let backed_block_time = Instant::now();
					if let Some(last_backed_block_time) = &self.last_backed_block_time {
						let duration = backed_block_time.duration_since(*last_backed_block_time);
						if let Some(metrics) = &self.metrics {
							metrics.parachain_block_backed_duration.observe(duration.as_secs_f64());
						}
					}
					self.last_backed_block_time = Some(backed_block_time);
					backed_candidates.push(backed_block);
				},
				CandidateEvent::CandidateIncluded(receipt, head, _, _) => {
					if receipt.descriptor.para_id() != self.para_id {
						continue;
					}

					let included_block = match Block::Header::decode(&mut &head.0[..]) {
						Ok(header) => header,
						Err(e) => {
							log::warn!(
								target: LOG_TARGET,
								"Failed to decode parachain header from included block: {e:?}"
							);
							continue
						},
					};
					let unincluded_segment_size =
						self.client.info().best_number.saturating_sub(*included_block.number());
					let unincluded_segment_size: u32 = unincluded_segment_size.saturated_into();
					if let Some(metrics) = &self.metrics {
						metrics.unincluded_segment_size.observe(unincluded_segment_size.into());
					}
					included_candidates.push(included_block);
				},
				CandidateEvent::CandidateTimedOut(receipt, head, _) => {
					if receipt.descriptor.para_id() != self.para_id {
						continue;
					}

					let timed_out_block = match Block::Header::decode(&mut &head.0[..]) {
						Ok(header) => header,
						Err(e) => {
							log::warn!(
								target: LOG_TARGET,
								"Failed to decode parachain header from timed out block: {e:?}"
							);
							continue
						},
					};
					timed_out_candidates.push(timed_out_block);
				},
			}
		}
		let mut log_parts = Vec::new();
		if !backed_candidates.is_empty() {
			let backed_candidates = backed_candidates
				.into_iter()
				.map(|c| format!("#{} ({})", c.number(), c.hash()))
				.collect::<Vec<_>>()
				.join(", ");
			log_parts.push(format!("backed: {}", backed_candidates));
		};
		if !included_candidates.is_empty() {
			let included_candidates = included_candidates
				.into_iter()
				.map(|c| format!("#{} ({})", c.number(), c.hash()))
				.collect::<Vec<_>>()
				.join(", ");
			log_parts.push(format!("included: {}", included_candidates));
		};
		if !timed_out_candidates.is_empty() {
			let timed_out_candidates = timed_out_candidates
				.into_iter()
				.map(|c| format!("#{} ({})", c.number(), c.hash()))
				.collect::<Vec<_>>()
				.join(", ");
			log_parts.push(format!("timed out: {}", timed_out_candidates));
		};
		if !log_parts.is_empty() {
			log::info!(
				target: LOG_TARGET,
				"Update at relay chain block #{} ({}) - {}",
				n.number(),
				n.hash(),
				log_parts.join(", ")
			);
		}
	}
}

/// Metrics for the parachain informant service.
pub struct ParachainInformantMetrics {
	/// Time between parachain blocks getting backed by the relaychain.
	parachain_block_backed_duration: Histogram,
	/// Number of blocks between best block and last included block.
	unincluded_segment_size: Histogram,
	/// Time between the submission of a transaction and its inclusion in a backed block.
	transaction_backed_duration: Histogram,
}

impl ParachainInformantMetrics {
	pub fn new(prometheus_registry: &Registry) -> prometheus::Result<Self> {
		let parachain_block_authorship_duration = Histogram::with_opts(HistogramOpts::new(
			"parachain_block_backed_duration",
			"Time between parachain blocks getting backed by the relaychain",
		))?;
		prometheus_registry.register(Box::new(parachain_block_authorship_duration.clone()))?;

		let unincluded_segment_size = Histogram::with_opts(
			HistogramOpts::new(
				"parachain_unincluded_segment_size",
				"Number of blocks between best block and last included block",
			)
			.buckets((0..=24).into_iter().map(|i| i as f64).collect()),
		)?;
		prometheus_registry.register(Box::new(unincluded_segment_size.clone()))?;

		let transaction_backed_duration = Histogram::with_opts(
			HistogramOpts::new(
				"parachain_transaction_backed_duration",
				"Time between the submission of a transaction and its inclusion in a backed block",
			)
			.buckets(linear_buckets(0.01, 40.0, 20).expect("Valid buckets; qed")),
		)?;
		prometheus_registry.register(Box::new(transaction_backed_duration.clone()))?;

		Ok(Self {
			parachain_block_backed_duration: parachain_block_authorship_duration,
			unincluded_segment_size,
			transaction_backed_duration,
		})
	}
}
