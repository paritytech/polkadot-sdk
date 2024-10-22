// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Metrics for message lane relay loop.

use crate::{
	message_lane::MessageLane,
	message_lane_loop::{SourceClientState, TargetClientState},
};

use bp_messages::{HashedLaneId, LegacyLaneId, MessageNonce};
use finality_relay::SyncLoopMetrics;
use relay_utils::metrics::{
	metric_name, register, GaugeVec, Metric, Opts, PrometheusError, Registry, U64,
};

/// Message lane relay metrics.
///
/// Cloning only clones references.
#[derive(Clone)]
pub struct MessageLaneLoopMetrics {
	/// Best finalized block numbers - "source", "source_at_target", "target_at_source".
	source_to_target_finality_metrics: SyncLoopMetrics,
	/// Best finalized block numbers - "source", "target", "source_at_target", "target_at_source".
	target_to_source_finality_metrics: SyncLoopMetrics,
	/// Lane state nonces: "source_latest_generated", "source_latest_confirmed",
	/// "target_latest_received", "target_latest_confirmed".
	lane_state_nonces: GaugeVec<U64>,
}

impl MessageLaneLoopMetrics {
	/// Create and register messages loop metrics.
	pub fn new(prefix: Option<&str>) -> Result<Self, PrometheusError> {
		Ok(MessageLaneLoopMetrics {
			source_to_target_finality_metrics: SyncLoopMetrics::new(
				prefix,
				"source",
				"source_at_target",
			)?,
			target_to_source_finality_metrics: SyncLoopMetrics::new(
				prefix,
				"target",
				"target_at_source",
			)?,
			lane_state_nonces: GaugeVec::new(
				Opts::new(metric_name(prefix, "lane_state_nonces"), "Nonces of the lane state"),
				&["type"],
			)?,
		})
	}

	/// Update source client state metrics.
	pub fn update_source_state<P: MessageLane>(&self, source_client_state: SourceClientState<P>) {
		self.source_to_target_finality_metrics
			.update_best_block_at_source(source_client_state.best_self.0);
		if let Some(best_finalized_peer_at_best_self) =
			source_client_state.best_finalized_peer_at_best_self
		{
			self.target_to_source_finality_metrics
				.update_best_block_at_target(best_finalized_peer_at_best_self.0);
			if let Some(actual_best_finalized_peer_at_best_self) =
				source_client_state.actual_best_finalized_peer_at_best_self
			{
				self.target_to_source_finality_metrics.update_using_same_fork(
					best_finalized_peer_at_best_self.1 == actual_best_finalized_peer_at_best_self.1,
				);
			}
		}
	}

	/// Update target client state metrics.
	pub fn update_target_state<P: MessageLane>(&self, target_client_state: TargetClientState<P>) {
		self.target_to_source_finality_metrics
			.update_best_block_at_source(target_client_state.best_self.0);
		if let Some(best_finalized_peer_at_best_self) =
			target_client_state.best_finalized_peer_at_best_self
		{
			self.source_to_target_finality_metrics
				.update_best_block_at_target(best_finalized_peer_at_best_self.0);
			if let Some(actual_best_finalized_peer_at_best_self) =
				target_client_state.actual_best_finalized_peer_at_best_self
			{
				self.source_to_target_finality_metrics.update_using_same_fork(
					best_finalized_peer_at_best_self.1 == actual_best_finalized_peer_at_best_self.1,
				);
			}
		}
	}

	/// Update latest generated nonce at source.
	pub fn update_source_latest_generated_nonce(
		&self,
		source_latest_generated_nonce: MessageNonce,
	) {
		self.lane_state_nonces
			.with_label_values(&["source_latest_generated"])
			.set(source_latest_generated_nonce);
	}

	/// Update the latest confirmed nonce at source.
	pub fn update_source_latest_confirmed_nonce(
		&self,
		source_latest_confirmed_nonce: MessageNonce,
	) {
		self.lane_state_nonces
			.with_label_values(&["source_latest_confirmed"])
			.set(source_latest_confirmed_nonce);
	}

	/// Update the latest received nonce at target.
	pub fn update_target_latest_received_nonce(&self, target_latest_generated_nonce: MessageNonce) {
		self.lane_state_nonces
			.with_label_values(&["target_latest_received"])
			.set(target_latest_generated_nonce);
	}

	/// Update the latest confirmed nonce at target.
	pub fn update_target_latest_confirmed_nonce(
		&self,
		target_latest_confirmed_nonce: MessageNonce,
	) {
		self.lane_state_nonces
			.with_label_values(&["target_latest_confirmed"])
			.set(target_latest_confirmed_nonce);
	}
}

impl Metric for MessageLaneLoopMetrics {
	fn register(&self, registry: &Registry) -> Result<(), PrometheusError> {
		self.source_to_target_finality_metrics.register(registry)?;
		self.target_to_source_finality_metrics.register(registry)?;
		register(self.lane_state_nonces.clone(), registry)?;
		Ok(())
	}
}

/// Provides a label for metrics.
pub trait Labeled {
	/// Returns a label.
	fn label(&self) -> String;
}

/// `Labeled` implementation for `LegacyLaneId`.
impl Labeled for LegacyLaneId {
	fn label(&self) -> String {
		hex::encode(self.0)
	}
}

/// `Labeled` implementation for `HashedLaneId`.
impl Labeled for HashedLaneId {
	fn label(&self) -> String {
		format!("{:?}", self.inner())
	}
}

#[test]
fn lane_to_label_works() {
	assert_eq!(
		"0x0101010101010101010101010101010101010101010101010101010101010101",
		HashedLaneId::from_inner(sp_core::H256::from([1u8; 32])).label(),
	);
	assert_eq!("00000001", LegacyLaneId([0, 0, 0, 1]).label());
}
