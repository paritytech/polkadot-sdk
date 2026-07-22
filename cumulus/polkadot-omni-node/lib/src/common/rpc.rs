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

//! Parachain-specific RPCs implementation.

#![warn(missing_docs)]

use crate::common::{
	types::{AccountId, Balance, Nonce, ParachainBackend, ParachainClient},
	ConstructNodeRuntimeApi,
};
use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
use sc_hop::{HopApiServer, HopRpcServer};
use sc_rpc::{
	dev::{Dev, DevApiServer},
	statement::{StatementApiServer, StatementStore},
};
use sc_rpc_spec_v2::statement::{StatementSpec, StatementSpecApiServer};
use sp_runtime::traits::Block as BlockT;
use std::{marker::PhantomData, sync::Arc};
use substrate_frame_rpc_system::{System, SystemApiServer};
use substrate_state_trie_migration_rpc::{StateMigration, StateMigrationApiServer};

/// A type representing all RPC extensions.
pub type RpcExtension = jsonrpsee::RpcModule<()>;

pub(crate) use eth_rpc::BuildParachainRpcExtensions;

mod eth_rpc {
	use super::*;
	use pallet_revive_eth_rpc::{
		cli::{build_eth_rpc_module, build_native_inmemory_client},
		client::SubscriptionType,
		native_client::ReviveRuntimeApiT,
		NativeClientBlockInfoProvider, NativeSubstrateClient,
	};
	use sp_api::{Metadata as _, ProvideRuntimeApi};
	use sp_core::H256;

	/// The default number of recent blocks kept in the in-memory receipt cache.
	const DEFAULT_KEEP_LATEST_BLOCKS: usize = 256;

	/// Reads the `ChainId` constant from the `Revive` pallet via the runtime metadata.
	fn read_revive_chain_id<B, C>(
		client: &Arc<C>,
		best_hash: B::Hash,
	) -> Result<u64, Box<dyn std::error::Error + Send + Sync>>
	where
		B: BlockT,
		C: ProvideRuntimeApi<B>,
		C::Api: sp_api::Metadata<B>,
	{
		use codec::Decode as _;

		let opaque = client
			.runtime_api()
			.metadata(best_hash)
			.map_err(|e| format!("metadata API error: {e}"))?;

		let meta = subxt_metadata::Metadata::decode(&mut &opaque[..])
			.map_err(|e| format!("metadata decode error: {e}"))?;

		let value = meta
			.pallet_by_name("Revive")
			.and_then(|p| p.constant_by_name("ChainId"))
			.map(|c| c.value().to_vec())
			.ok_or("Revive pallet `ChainId` constant not found in runtime metadata")?;

		u64::decode(&mut &value[..]).map_err(|e| format!("ChainId decode error: {e}").into())
	}

	pub(crate) struct BuildParachainRpcExtensions<Block, RuntimeApi>(
		PhantomData<(Block, RuntimeApi)>,
	);

	impl<Block, RuntimeApi>
		BuildRpcExtensions<
			ParachainClient<Block, RuntimeApi>,
			ParachainBackend<Block>,
			Block,
			sc_transaction_pool::TransactionPoolHandle<Block, ParachainClient<Block, RuntimeApi>>,
			sc_statement_store::Store,
		> for BuildParachainRpcExtensions<Block, RuntimeApi>
	where
		Block: BlockT<Hash = H256, Extrinsic = sp_runtime::OpaqueExtrinsic> + Send + Sync + 'static,
		Block::Header:
			sp_runtime::traits::Header<Number = u32, Hash = H256> + Unpin + Send + Sync + 'static,
		RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>
			+ Send
			+ Sync
			+ 'static,
		RuntimeApi::RuntimeApi: pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
			+ substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>
			+ ReviveRuntimeApiT<Block, u64>,
	{
		fn build_rpc_extensions(
			client: Arc<ParachainClient<Block, RuntimeApi>>,
			backend: Arc<ParachainBackend<Block>>,
			pool: Arc<
				sc_transaction_pool::TransactionPoolHandle<
					Block,
					ParachainClient<Block, RuntimeApi>,
				>,
			>,
			statement_store: Option<Arc<sc_statement_store::Store>>,
			hop_pool: Option<Arc<sc_hop::HopDataPool>>,
			spawn_handle: Arc<dyn sp_core::traits::SpawnNamed>,
			network: Arc<dyn sc_network::service::traits::NetworkService>,
			sync_service: Arc<sc_network_sync::SyncingService<Block>>,
		) -> sc_service::error::Result<RpcExtension> {
			let build = || -> Result<RpcExtension, Box<dyn std::error::Error + Send + Sync>> {
				let mut module = RpcExtension::new(());

				// Standard parachain RPCs.
				module.merge(System::new(client.clone(), pool.clone()).into_rpc())?;
				module.merge(TransactionPayment::new(client.clone()).into_rpc())?;
				module.merge(StateMigration::new(client.clone(), backend).into_rpc())?;
				if let Some(statement_store) = statement_store {
					module.merge(
						StatementStore::new(statement_store.clone(), spawn_handle.clone())
							.into_rpc(),
					)?;
					module.merge(
						StatementSpec::new(statement_store, spawn_handle.clone()).into_rpc(),
					)?;
				}
				if let Some(hop_pool) = hop_pool {
					module.merge(HopRpcServer::new(hop_pool, client.clone()).into_rpc())?;
				}
				module.merge(Dev::new(client.clone()).into_rpc())?;

				// ETH RPC server.
				let best_hash = client.chain_info().best_hash;
				match read_revive_chain_id::<Block, _>(&client, best_hash) {
					Ok(chain_id) => {
						let block_provider = NativeClientBlockInfoProvider::new(client.clone())
							.map_err(|e| format!("block info provider: {e}"))?;

						let net_status: Arc<dyn sc_network::NetworkStatusProvider + Send + Sync> =
							network.clone();
						let sync_oracle: Arc<dyn sp_consensus::SyncOracle + Send + Sync> =
							sync_service.clone();
						let native_client =
							NativeSubstrateClient::new(client.clone(), pool, chain_id, false)
								.map_err(|e| format!("native substrate client: {e}"))?
								.with_network(net_status, sync_oracle);

						let (bootstrap_tx, bootstrap_rx) = std::sync::mpsc::sync_channel(1);
						spawn_handle.spawn(
							"eth-rpc-bootstrap",
							Some("eth-rpc"),
							Box::pin(async move {
								let result = build_native_inmemory_client(
									native_client,
									block_provider,
									DEFAULT_KEEP_LATEST_BLOCKS,
									false,
								)
								.await
								.map_err(|e| format!("ETH RPC client init: {e}"));
								let _ = bootstrap_tx.send(result);
							}),
						);
						let eth_client = bootstrap_rx
							.recv()
							.map_err(|_| "eth-rpc bootstrap task did not return".to_string())??;

						let eth_best = eth_client.clone();
						spawn_handle.spawn(
							"eth-rpc-best-blocks",
							Some("eth-rpc"),
							Box::pin(async move {
								if let Err(e) = eth_best
									.subscribe_and_cache_new_blocks(SubscriptionType::BestBlocks)
									.await
								{
									log::error!(
										target: "eth-rpc",
										"Best-block subscription error: {e:?}"
									);
								}
							}),
						);
						let eth_finalized = eth_client.clone();
						spawn_handle.spawn(
							"eth-rpc-finalized-blocks",
							Some("eth-rpc"),
							Box::pin(async move {
								if let Err(e) = eth_finalized
									.subscribe_and_cache_new_blocks(
										SubscriptionType::FinalizedBlocks,
									)
									.await
								{
									log::error!(
										target: "eth-rpc",
										"Finalized-block subscription error: {e:?}"
									);
								}
							}),
						);

						let eth_module = build_eth_rpc_module(false, eth_client, false, false)?;
						module.merge(eth_module)?;
					},
					Err(e) => {
						log::debug!(
							target: "eth-rpc",
							"Revive pallet not found in runtime metadata, ETH RPC disabled: {e}"
						);
					},
				}

				Ok(module)
			};
			build().map_err(Into::into)
		}
	}
}

pub(crate) trait BuildRpcExtensions<Client, Backend, Block, Pool, StatementStore>
where
	Block: BlockT,
{
	fn build_rpc_extensions(
		client: Arc<Client>,
		backend: Arc<Backend>,
		pool: Arc<Pool>,
		statement_store: Option<Arc<StatementStore>>,
		hop_pool: Option<Arc<sc_hop::HopDataPool>>,
		spawn_handle: Arc<dyn sp_core::traits::SpawnNamed>,
		network: Arc<dyn sc_network::service::traits::NetworkService>,
		sync_service: Arc<sc_network_sync::SyncingService<Block>>,
	) -> sc_service::error::Result<RpcExtension>;
}
