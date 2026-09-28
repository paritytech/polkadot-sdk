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

//! Price oracle bounds and handles for cumulus parachain collators.

use super::{aura::AuraIdT, types::ParachainClient, ConstructNodeRuntimeApi};
use sc_client_db::DbHash;
use sc_service::TaskManager;
use sp_keystore::KeystorePtr;
use sp_price_oracle::runtime_api::{PriceOracleApi, PriceOracleMarketApi};
use sp_runtime::traits::Block as BlockT;
use std::{marker::PhantomData, sync::Arc};

/// Convenience trait for defining the bounds of a parachain runtime whose collators run the
/// price oracle. The signer key is the Aura key.
pub trait PriceOracleRuntimeApi<Block: BlockT, AuraId: AuraIdT>:
	PriceOracleApi<Block, AuraId::BoundedPublic> + PriceOracleMarketApi<Block>
{
}

impl<T, Block: BlockT, AuraId: AuraIdT> PriceOracleRuntimeApi<Block, AuraId> for T where
	T: PriceOracleApi<Block, AuraId::BoundedPublic> + PriceOracleMarketApi<Block>
{
}

/// Network handles of the price oracle gossip protocol.
///
/// Created by `start_node` when the protocol is registered with the network, and consumed by
/// `StartConsensus::start_consensus` to spawn the price oracle service.
// Never constructed without the feature: `start_node` passes `None`.
#[cfg_attr(not(feature = "price-oracle"), allow(dead_code))]
pub struct PriceOracleNetwork<Block: BlockT> {
	/// Notification service of the protocol.
	pub notification_service: Box<dyn sc_network::service::traits::NotificationService>,
	/// Name of the protocol.
	pub protocol_name: sc_network::ProtocolName,
	/// The network service.
	pub network: Arc<dyn sc_network::service::traits::NetworkService>,
	/// The sync service.
	pub sync_service: Arc<sc_network_sync::SyncingService<Block>>,
	/// Prometheus registry of the node.
	pub prometheus_registry: Option<prometheus_endpoint::Registry>,
}

/// Signer key type of the price oracle: the Aura authority key.
pub type OracleId<AuraId> = <AuraId as AuraIdT>::BoundedPublic;
/// Signature type of the price oracle: the Aura signature.
pub type OracleSignature<AuraId> = <AuraId as AuraIdT>::BoundedSignature;
/// Inherent data provider of the price oracle.
pub type OracleInherentDataProvider<AuraId> =
	sp_price_oracle::inherents::InherentDataProvider<OracleId<AuraId>, OracleSignature<AuraId>>;

/// The price oracle of a collator: a handle to the report pool of the running service, or
/// nothing when the service does not run.
///
/// Built without the `price-oracle` feature, the service never runs and the inherent data
/// provider is always empty.
pub struct PriceOracle<AuraId: AuraIdT> {
	#[cfg(feature = "price-oracle")]
	pool: Option<sc_price_oracle::ReportPool<OracleId<AuraId>, OracleSignature<AuraId>>>,
	_id: PhantomData<fn() -> AuraId>,
}

impl<AuraId: AuraIdT> Clone for PriceOracle<AuraId> {
	fn clone(&self) -> Self {
		Self {
			#[cfg(feature = "price-oracle")]
			pool: self.pool.clone(),
			_id: PhantomData,
		}
	}
}

impl<AuraId: AuraIdT + Send + Sync> PriceOracle<AuraId> {
	/// Spawn the price oracle service on `task_manager` when its network handles are given.
	pub fn start<Block, RuntimeApi>(
		network: Option<PriceOracleNetwork<Block>>,
		client: Arc<ParachainClient<Block, RuntimeApi>>,
		keystore: KeystorePtr,
		task_manager: &TaskManager,
	) -> Self
	where
		Block: BlockT<Hash = DbHash>,
		RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
		RuntimeApi::RuntimeApi: PriceOracleRuntimeApi<Block, AuraId>,
	{
		#[cfg(feature = "price-oracle")]
		let pool = network.map(|network| {
			let pool = sc_price_oracle::ReportPool::new();
			let service =
				sc_price_oracle::run::<Block, _, _, _, OracleId<AuraId>, OracleSignature<AuraId>>(
					sc_price_oracle::Params {
						client,
						network: network.network,
						sync: network.sync_service,
						notification_service: network.notification_service,
						protocol_name: network.protocol_name,
						keystore,
						pool: pool.clone(),
						prometheus_registry: network.prometheus_registry,
					},
				);
			task_manager.spawn_handle().spawn("price-oracle", None, service);
			pool
		});
		#[cfg(not(feature = "price-oracle"))]
		let _ = (network, client, keystore, task_manager);

		Self {
			#[cfg(feature = "price-oracle")]
			pool,
			_id: PhantomData,
		}
	}

	/// The inherent data provider for the block built on `parent`: the pooled reports when the
	/// service runs, none otherwise.
	pub fn inherent_data_provider<Block, RuntimeApi>(
		&self,
		client: &ParachainClient<Block, RuntimeApi>,
		parent: Block::Hash,
	) -> OracleInherentDataProvider<AuraId>
	where
		Block: BlockT<Hash = DbHash>,
		RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
		RuntimeApi::RuntimeApi: PriceOracleRuntimeApi<Block, AuraId>,
	{
		#[cfg(feature = "price-oracle")]
		if let Some(pool) = &self.pool {
			return sc_price_oracle::PriceOracleInherentDataProvider::create::<Block, _, _, _>(
				client, pool, parent,
			);
		}
		#[cfg(not(feature = "price-oracle"))]
		let _ = (client, parent);
		OracleInherentDataProvider::<AuraId>::new(Vec::new())
	}
}
