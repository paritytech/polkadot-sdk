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

use crate::common::{
	aura::{AuraIdT, AuraRuntimeApi},
	types::ParachainClient,
	ConstructNodeRuntimeApi,
};
use sc_client_db::DbHash;
use sc_network::Multiaddr;
use sc_price_oracle::ReportRecipients;
use sc_service::TaskManager;
use sp_api::ProvideRuntimeApi;
use sp_authority_discovery::{AuthorityDiscoveryApi, AuthorityId as AuthorityDiscoveryId};
use sp_consensus_aura::{AuraApi, Slot, AURA_ENGINE_ID};
use sp_keystore::KeystorePtr;
use sp_price_oracle::runtime_api::PriceOracleApi;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use std::{future::Future, marker::PhantomData, sync::Arc};

/// Log target for the price oracle.
const LOG_TARGET: &str = "price-oracle";
/// How many slots past the best block's the reports are sent to the authors of. With slots of
/// 6 s or more, this reaches at least 18 s ahead: time to open a substream and send a report
/// before an author's turn.
const LOOKAHEAD: u64 = 3;
/// Addresses used per author at most. Bounds the dial attempts an author's record can cause.
const MAX_ADDRESSES_PER_AUTHOR: usize = 4;

/// Network handles of the price oracle protocol.
///
/// Created by `start_node` when the protocol is registered with the network, and consumed by
/// `StartConsensus::start_consensus` to spawn the price oracle service.
pub struct PriceOracleNetwork<Block: BlockT> {
	/// Notification service of the protocol.
	pub notification_service: Box<dyn sc_network::service::traits::NotificationService>,
	/// Name of the protocol.
	pub protocol_name: sc_network::ProtocolName,
	/// The network service.
	pub network: Arc<dyn sc_network::service::traits::NetworkService>,
	/// The sync service.
	pub sync_service: Arc<sc_network_sync::SyncingService<Block>>,
	/// Authority discovery of the parachain, to find the authors the reports go to.
	pub authority_discovery: sc_authority_discovery::Service,
}

/// The collators authoring in the coming Aura slots, found through authority discovery.
///
/// The slot is taken from the best block, and the authors of that slot and the next
/// [`LOOKAHEAD`] slots from the Aura authorities at the best block. Their authority
/// discovery keys are the entries of `AuthorityDiscoveryApi::authorities` at the same positions, so
/// the runtime must return them in authoring order, as `cumulus-client-collator-discovery` requires
/// too.
pub struct AuraRecipients<Block: BlockT, RuntimeApi, AuraId> {
	client: Arc<ParachainClient<Block, RuntimeApi>>,
	authority_discovery: sc_authority_discovery::Service,
	_aura: PhantomData<AuraId>,
}

impl<Block, RuntimeApi, AuraId> AuraRecipients<Block, RuntimeApi, AuraId>
where
	Block: BlockT<Hash = DbHash>,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT,
{
	/// The authority discovery keys of the authors of the coming slots at `best`.
	fn coming_authors(&self, best: Block::Hash) -> Vec<AuthorityDiscoveryId> {
		let api = self.client.runtime_api();
		let aura_ids = match AuraApi::<Block, AuraId::BoundedPublic>::authorities(&*api, best) {
			Ok(aura_ids) => aura_ids,
			Err(e) => {
				log::warn!(target: LOG_TARGET, "Cannot read the Aura authorities at {best:?}: {e}");
				return Vec::new();
			},
		};
		let discovery_ids = match AuthorityDiscoveryApi::authorities(&*api, best) {
			Ok(discovery_ids) => discovery_ids,
			Err(e) => {
				log::warn!(
					target: LOG_TARGET,
					"Cannot read the authority discovery ids at {best:?}: {e}",
				);
				return Vec::new();
			},
		};
		if aura_ids.len() != discovery_ids.len() {
			log::warn!(
				target: LOG_TARGET,
				"{} Aura ids but {} authority discovery ids at {best:?}",
				aura_ids.len(),
				discovery_ids.len(),
			);
			return Vec::new();
		}
		let count = discovery_ids.len() as u64;
		if count == 0 {
			return Vec::new();
		}
		// The slot of the best block, from the Aura pre-digest of its header.
		let best_slot = match self.client.header(best) {
			Ok(Some(header)) => header
				.digest()
				.logs()
				.iter()
				.find_map(|log| log.pre_runtime_try_to::<Slot>(&AURA_ENGINE_ID)),
			_ => None,
		};
		let Some(best_slot) = best_slot else {
			log::warn!(target: LOG_TARGET, "No Aura slot in the header of {best:?}");
			return Vec::new();
		};
		// TODO: two cases are not covered.
		// - Session change: the new set authors before this node imports the change, so its first
		//   authors get no reports for about a tick and a dial.
		// - `best_slot` far behind: this node lags, or the chain stalls while the relay chain runs.
		//   Either way the authors resuming the chain are past the range. The wall clock's slot,
		//   minus the relay parent lag, would point at them.

		// Consecutive slots have consecutive authors, so at most `count` distinct ones.
		(0..(LOOKAHEAD + 1).min(count))
			.map(|ahead| discovery_ids[((*best_slot + ahead) % count) as usize].clone())
			.collect()
	}
}

impl<Block, RuntimeApi, AuraId> ReportRecipients<Block>
	for AuraRecipients<Block, RuntimeApi, AuraId>
where
	Block: BlockT<Hash = DbHash>,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
{
	fn recipients(&self, best: Block::Hash) -> impl Future<Output = Vec<Multiaddr>> + Send {
		let authors = self.coming_authors(best);
		let mut service = self.authority_discovery.clone();
		async move {
			let mut addresses = Vec::new();
			for author in authors {
				match service.get_addresses_by_authority_id(author.clone()).await {
					Some(found) => {
						addresses.extend(found.into_iter().take(MAX_ADDRESSES_PER_AUTHOR))
					},
					None => {
						log::debug!(target: LOG_TARGET, "No address known for author {author:?}")
					},
				}
			}
			addresses
		}
	}
}

/// Convenience trait for defining the bounds of a parachain runtime whose collators run the
/// price oracle. The signer key is the Aura key.
pub trait PriceOracleRuntimeApi<Block: BlockT, AuraId: AuraIdT>:
	PriceOracleApi<Block, AuraId::BoundedPublic>
{
}

impl<T, Block: BlockT, AuraId: AuraIdT> PriceOracleRuntimeApi<Block, AuraId> for T where
	T: PriceOracleApi<Block, AuraId::BoundedPublic>
{
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
pub struct PriceOracle<AuraId: AuraIdT> {
	pool: Option<sc_price_oracle::ReportPool<OracleId<AuraId>, OracleSignature<AuraId>>>,
}

impl<AuraId: AuraIdT> Clone for PriceOracle<AuraId> {
	fn clone(&self) -> Self {
		Self { pool: self.pool.clone() }
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
		RuntimeApi::RuntimeApi:
			PriceOracleRuntimeApi<Block, AuraId> + AuraRuntimeApi<Block, AuraId>,
	{
		let pool = network.map(|network| {
			let pool = sc_price_oracle::ReportPool::new();
			let recipients = AuraRecipients::<Block, RuntimeApi, AuraId> {
				client: client.clone(),
				authority_discovery: network.authority_discovery,
				_aura: PhantomData,
			};
			let service =
				sc_price_oracle::run::<Block, _, _, _, OracleId<AuraId>, OracleSignature<AuraId>>(
					sc_price_oracle::Params {
						client,
						network: network.network,
						sync: network.sync_service,
						notification_service: network.notification_service,
						protocol_name: network.protocol_name,
						recipients,
						keystore,
						pool: pool.clone(),
					},
				);
			task_manager.spawn_handle().spawn("price-oracle", None, service);
			pool
		});
		Self { pool }
	}

	/// The inherent data provider for the block built on `parent`: the pooled reports when
	/// the service runs, none otherwise.
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
		match &self.pool {
			Some(pool) => {
				sc_price_oracle::PriceOracleInherentDataProvider::create::<Block, _, _, _>(
					client, pool, parent,
				)
			},
			None => OracleInherentDataProvider::<AuraId>::new(Vec::new()),
		}
	}
}
