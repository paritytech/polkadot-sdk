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

use crate::chain::Chain;
use crate::client::Client;
use crate::error::Error;

use async_trait::async_trait;
use bp_messages::LaneId;
use bp_runtime::InstanceId;
use relay_utils::metrics::{register, Gauge, Metrics, Registry, StandaloneMetrics, U64};
use sp_core::storage::StorageKey;
use sp_runtime::traits::Header as HeaderT;
use sp_trie::StorageProof;
use std::time::Duration;

/// Storage proof overhead update interval (in blocks).
const UPDATE_INTERVAL_IN_BLOCKS: u32 = 100;

/// Metric that represents extra size of storage proof as unsigned integer gauge.
///
/// Regular Substrate node does not provide any RPC endpoints that return storage proofs.
/// So here we're using our own `pallet-bridge-messages-rpc` RPC API, which returns proof
/// of the inbound message lane state. Then we simply subtract size of this state from
/// the size of storage proof to compute metric value.
///
/// There are two things to keep in mind when using this metric:
///
/// 1) it'll only work on inbound lanes that have already accepted at least one message;
/// 2) the overhead may be slightly different for other values, but this metric gives a good estimation.
#[derive(Debug)]
pub struct StorageProofOverheadMetric<C: Chain> {
	client: Client<C>,
	inbound_lane: (InstanceId, LaneId),
	inbound_lane_data_key: StorageKey,
	metric: Gauge<U64>,
}

impl<C: Chain> Clone for StorageProofOverheadMetric<C> {
	fn clone(&self) -> Self {
		StorageProofOverheadMetric {
			client: self.client.clone(),
			inbound_lane: self.inbound_lane,
			inbound_lane_data_key: self.inbound_lane_data_key.clone(),
			metric: self.metric.clone(),
		}
	}
}

impl<C: Chain> StorageProofOverheadMetric<C> {
	/// Create new metric instance with given name and help.
	pub fn new(
		client: Client<C>,
		inbound_lane: (InstanceId, LaneId),
		inbound_lane_data_key: StorageKey,
		name: String,
		help: String,
	) -> Self {
		StorageProofOverheadMetric {
			client,
			inbound_lane,
			inbound_lane_data_key,
			metric: Gauge::new(name, help).expect(
				"only fails if gauge options are customized;\
					we use default options;\
					qed",
			),
		}
	}

	/// Returns approximate storage proof size overhead.
	///
	/// Returs `Ok(None)` if inbound lane we're watching for has no state. This shouldn't be treated as error.
	async fn compute_storage_proof_overhead(&self) -> Result<Option<usize>, Error> {
		let best_header_hash = self.client.best_finalized_header_hash().await?;
		let best_header = self.client.header_by_hash(best_header_hash).await?;

		let storage_proof = self
			.client
			.prove_messages_delivery(self.inbound_lane.0, self.inbound_lane.1, best_header_hash)
			.await?;
		let storage_proof_size: usize = storage_proof.iter().map(|n| n.len()).sum();

		let storage_value_reader = bp_runtime::StorageProofChecker::<C::Hasher>::new(
			*best_header.state_root(),
			StorageProof::new(storage_proof),
		)
		.map_err(Error::StorageProofError)?;
		let maybe_encoded_storage_value = storage_value_reader
			.read_value(&self.inbound_lane_data_key.0)
			.map_err(Error::StorageProofError)?;
		let encoded_storage_value_size = match maybe_encoded_storage_value {
			Some(encoded_storage_value) => encoded_storage_value.len(),
			None => return Ok(None),
		};

		Ok(Some(storage_proof_size - encoded_storage_value_size))
	}
}

impl<C: Chain> Metrics for StorageProofOverheadMetric<C> {
	fn register(&self, registry: &Registry) -> Result<(), String> {
		register(self.metric.clone(), registry).map_err(|e| e.to_string())?;
		Ok(())
	}
}

#[async_trait]
impl<C: Chain> StandaloneMetrics for StorageProofOverheadMetric<C> {
	fn update_interval(&self) -> Duration {
		C::AVERAGE_BLOCK_INTERVAL * UPDATE_INTERVAL_IN_BLOCKS
	}

	async fn update(&self) {
		relay_utils::metrics::set_gauge_value(
			&self.metric,
			self.compute_storage_proof_overhead()
				.await
				.map(|v| v.map(|overhead| overhead as u64)),
		);
	}
}
