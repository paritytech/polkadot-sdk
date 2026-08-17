// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Cumulus parachain collator primitives.

#![warn(missing_docs)]

pub(crate) mod aura;
pub mod chain_spec;
pub mod command;
pub mod rpc;
pub mod runtime;
pub mod spec;
pub(crate) mod statement_store;
pub mod types;

use crate::cli::AuthoringPolicy;

use cumulus_primitives_core::{
	CollectCollationInfo, GetParachainInfo, RelayParentOffsetApi, SchedulingV3EnabledApi,
};
use sc_client_db::DbHash;
use sc_network_sync::strategy::chain_sync::{GapSyncBodyPolicy, GapSyncBodyPolicyProvider};
use sc_offchain::OffchainWorkerApi;
use sc_service::BlocksPruning;
use serde::de::DeserializeOwned;
use sp_api::{ApiExt, CallApiAt, ConstructRuntimeApi, Metadata};
use sp_block_builder::BlockBuilder;
use sp_runtime::{
	traits::{Block as BlockT, BlockNumber, Header as HeaderT, NumberFor},
	OpaqueExtrinsic, SaturatedConversion,
};
use sp_session::SessionKeys;
use sp_transaction_pool::runtime_api::TaggedTransactionQueue;
use sp_transaction_storage_proof::runtime_api::TransactionStorageApi;
use std::{fmt::Debug, path::PathBuf, str::FromStr, sync::Arc};

pub trait NodeBlock:
	BlockT<Extrinsic = OpaqueExtrinsic, Header = Self::BoundedHeader, Hash = DbHash>
	+ DeserializeOwned
	+ Unpin
{
	type BoundedFromStrErr: Debug;
	type BoundedNumber: FromStr<Err = Self::BoundedFromStrErr> + BlockNumber;
	type BoundedHeader: HeaderT<Number = Self::BoundedNumber, Hash = DbHash> + Unpin;
}

impl<T> NodeBlock for T
where
	T: BlockT<Extrinsic = OpaqueExtrinsic, Hash = DbHash> + DeserializeOwned + Unpin,
	<T as BlockT>::Header: Unpin,
	<NumberFor<T> as FromStr>::Err: Debug,
{
	type BoundedFromStrErr = <NumberFor<T> as FromStr>::Err;
	type BoundedNumber = NumberFor<T>;
	type BoundedHeader = <T as BlockT>::Header;
}

/// Convenience trait that defines the basic bounds for the `RuntimeApi` of a parachain node.
pub trait NodeRuntimeApi<Block: BlockT>:
	ApiExt<Block>
	+ Metadata<Block>
	+ SessionKeys<Block>
	+ BlockBuilder<Block>
	+ TaggedTransactionQueue<Block>
	+ OffchainWorkerApi<Block>
	+ CollectCollationInfo<Block>
	+ GetParachainInfo<Block>
	+ TransactionStorageApi<Block>
	+ RelayParentOffsetApi<Block>
	+ sp_authority_discovery::AuthorityDiscoveryApi<Block>
	+ SchedulingV3EnabledApi<Block>
	+ Sized
{
}

impl<T, Block: BlockT> NodeRuntimeApi<Block> for T where
	T: ApiExt<Block>
		+ Metadata<Block>
		+ SessionKeys<Block>
		+ BlockBuilder<Block>
		+ TaggedTransactionQueue<Block>
		+ OffchainWorkerApi<Block>
		+ RelayParentOffsetApi<Block>
		+ CollectCollationInfo<Block>
		+ GetParachainInfo<Block>
		+ TransactionStorageApi<Block>
		+ sp_authority_discovery::AuthorityDiscoveryApi<Block>
		+ SchedulingV3EnabledApi<Block>
{
}

/// Convenience trait that defines the basic bounds for the `ConstructRuntimeApi` of a parachain
/// node.
pub trait ConstructNodeRuntimeApi<Block: BlockT, C: CallApiAt<Block>>:
	ConstructRuntimeApi<Block, C, RuntimeApi = Self::BoundedRuntimeApi> + Send + Sync + 'static
{
	/// Basic bounds for the `RuntimeApi` of a parachain node.
	type BoundedRuntimeApi: NodeRuntimeApi<Block>;
}

impl<T, Block: BlockT, C: CallApiAt<Block>> ConstructNodeRuntimeApi<Block, C> for T
where
	T: ConstructRuntimeApi<Block, C> + Send + Sync + 'static,
	T::RuntimeApi: NodeRuntimeApi<Block>,
{
	type BoundedRuntimeApi = T::RuntimeApi;
}

/// Extra args that are passed when creating a new node spec.
pub struct NodeExtraArgs {
	/// The authoring policy to use.
	///
	/// Can be used to influence details of block production.
	pub authoring_policy: AuthoringPolicy,

	/// If set, each `PoV` build by the node will be exported to this folder.
	pub export_pov: Option<PathBuf>,

	/// The maximum percentage of the maximum PoV size that the collator can use.
	/// It will be removed once <https://github.com/paritytech/polkadot-sdk/issues/6020> is fixed.
	pub max_pov_percentage: Option<u32>,

	/// Statement store and network handler configuration.
	/// `None` disables the statement store.
	pub statement_store_config: Option<sc_statement_store::Config>,

	/// Parameters for storage monitoring.
	pub storage_monitor: sc_storage_monitor::StorageMonitorParams,

	/// Upper bound on collator reserved-peer slots.
	pub collator_reserved_slots: usize,

	/// HOP (Hand-Off Protocol) configuration parameters.
	/// `None` disables HOP.
	pub hop: Option<sc_hop::HopParams>,
}

/// Safety margin, in blocks, subtracted from the runtime's transaction-storage retention
/// period when deriving the gap sync body download window.
///
/// Bodies are only required for blocks above `finalized - (retention - margin)`. The
/// margin covers the finality lead of serving peers over the local node plus request,
/// retry and import-queue delays, so that every conforming peer still retains the bodies
/// we require. It is small compared to real retention periods, which span days or weeks.
pub(crate) const GAP_SYNC_BODY_SAFETY_MARGIN: u32 = 128;

/// Returns the [`GapSyncBodyPolicyProvider`] for this node.
///
/// The provider is evaluated when a `ChainSync` instance is created — on a warp-syncing
/// node right after state sync completes — so it queries the runtime at the best block
/// (the warp target) instead of genesis. A runtime API failure or an invalid
/// configuration fails `ChainSync` creation and thereby the node, instead of silently
/// degrading a storage chain to header-only gap sync.
pub(crate) fn gap_sync_body_policy_provider<Block, Client>(
	client: Arc<Client>,
	blocks_pruning: BlocksPruning,
) -> GapSyncBodyPolicyProvider
where
	Block: BlockT,
	Client: sp_api::ProvideRuntimeApi<Block> + sp_blockchain::HeaderBackend<Block> + 'static,
	Client::Api: TransactionStorageApi<Block>,
{
	Arc::new(move || {
		let at = client.info().best_hash;
		let api = client.runtime_api();
		let has_storage_api = api
			.has_api_with::<dyn TransactionStorageApi<Block>, _>(at, |version| version >= 2)
			.map_err(sp_blockchain::Error::RuntimeApiError)?;
		let storage_chain_retention = has_storage_api
			.then(|| {
				api.retention_period(at)
					.map(|retention| retention.saturated_into::<u32>())
					.map_err(sp_blockchain::Error::RuntimeApiError)
			})
			.transpose()?;

		let policy = resolve_gap_sync_body_policy(
			storage_chain_retention,
			blocks_pruning,
			GAP_SYNC_BODY_SAFETY_MARGIN,
		)
		.map_err(|error| sp_blockchain::Error::Application(error.into()))?;
		log::info!(
			"Resolved gap sync body policy {policy:?} (runtime retention period: \
			 {storage_chain_retention:?}, blocks pruning: {blocks_pruning:?}, safety margin: \
			 {GAP_SYNC_BODY_SAFETY_MARGIN})",
		);
		Ok(policy)
	})
}

/// Maps the runtime's transaction-storage retention period (`None` when the runtime does
/// not expose `TransactionStorageApi` v2) and the local pruning configuration onto a
/// [`GapSyncBodyPolicy`], validating that the configuration can actually serve the
/// storage chain.
fn resolve_gap_sync_body_policy(
	storage_chain_retention: Option<u32>,
	blocks_pruning: BlocksPruning,
	safety_margin: u32,
) -> Result<GapSyncBodyPolicy, String> {
	let Some(retention_period) = storage_chain_retention else {
		// Not a storage chain: archive nodes backfill the whole gap with bodies, pruned
		// nodes backfill headers and justifications only.
		return Ok(match blocks_pruning {
			BlocksPruning::KeepAll | BlocksPruning::KeepFinalized => GapSyncBodyPolicy::All,
			BlocksPruning::Some(_) => GapSyncBodyPolicy::HeadersOnly,
		});
	};

	match blocks_pruning {
		// Archive nodes retain every body anyway; no safety cutoff is necessary.
		BlocksPruning::KeepAll | BlocksPruning::KeepFinalized => Ok(GapSyncBodyPolicy::All),
		BlocksPruning::Some(window) => {
			if safety_margin >= retention_period {
				return Err(format!(
					"the gap sync body safety margin ({safety_margin}) must be smaller than \
					 the runtime's transaction storage retention period ({retention_period}), \
					 otherwise no gap bodies would be downloaded",
				));
			}
			if window < retention_period {
				return Err(format!(
					"the blocks pruning window ({window}) must be at least the runtime's \
					 transaction storage retention period ({retention_period}) on a storage \
					 chain; increase `--blocks-pruning`",
				));
			}
			Ok(GapSyncBodyPolicy::DownloadFinalized(retention_period - safety_margin))
		},
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn no_storage_chain_maps_pruning_to_default_policy() {
		for (blocks_pruning, expected) in [
			(BlocksPruning::KeepAll, GapSyncBodyPolicy::All),
			(BlocksPruning::KeepFinalized, GapSyncBodyPolicy::All),
			(BlocksPruning::Some(256), GapSyncBodyPolicy::HeadersOnly),
		] {
			assert_eq!(
				resolve_gap_sync_body_policy(None, blocks_pruning, GAP_SYNC_BODY_SAFETY_MARGIN),
				Ok(expected),
			);
		}
	}

	#[test]
	fn storage_chain_enables_required_within_with_pre_shrunk_window() {
		assert_eq!(
			resolve_gap_sync_body_policy(Some(100_800), BlocksPruning::Some(200_000), 128),
			Ok(GapSyncBodyPolicy::DownloadFinalized(100_800 - 128)),
		);
	}

	#[test]
	fn storage_chain_archive_nodes_download_all_bodies() {
		for blocks_pruning in [BlocksPruning::KeepAll, BlocksPruning::KeepFinalized] {
			assert_eq!(
				resolve_gap_sync_body_policy(Some(100_800), blocks_pruning, 128),
				Ok(GapSyncBodyPolicy::All),
			);
		}
	}

	#[test]
	fn safety_margin_must_be_smaller_than_retention() {
		for margin in [100, 101] {
			assert!(
				resolve_gap_sync_body_policy(Some(100), BlocksPruning::Some(1000), margin).is_err()
			);
		}
		assert!(resolve_gap_sync_body_policy(Some(100), BlocksPruning::Some(1000), 99).is_ok());
	}

	#[test]
	fn pruning_window_must_cover_retention() {
		assert!(resolve_gap_sync_body_policy(Some(1000), BlocksPruning::Some(999), 128).is_err());
		assert_eq!(
			resolve_gap_sync_body_policy(Some(1000), BlocksPruning::Some(1000), 128),
			Ok(GapSyncBodyPolicy::DownloadFinalized(872)),
		);
	}
}
