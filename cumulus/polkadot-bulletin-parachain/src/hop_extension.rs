// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! HOP wiring as a [`polkadot_omni_node_lib::BuildRpcExtensions`] impl.

use std::{
	marker::PhantomData,
	path::Path,
	sync::{Arc, OnceLock},
};

use polkadot_omni_node_lib::{
	common::types::{ParachainBackend, ParachainClient},
	BuildParachainRpcExtensions, BuildRpcExtensions, ConstructNodeRuntimeApi, NodeBlock,
};
use sc_hop::{HopApiServer, HopDataPool, HopParams, HopRpcServer};
use sc_transaction_pool::TransactionPoolHandle;

/// Wraps the default parachain builder and merges HOP. Lazily builds the pool
/// and spawns the maintenance task on first invocation.
pub struct HopBuildRpcExtensions<Block, RuntimeApi> {
	inner: BuildParachainRpcExtensions<Block, RuntimeApi>,
	params: HopParams,
	pool: OnceLock<Arc<HopDataPool>>,
	_phantom: PhantomData<(Block, RuntimeApi)>,
}

impl<Block, RuntimeApi> HopBuildRpcExtensions<Block, RuntimeApi> {
	pub fn new(params: HopParams) -> Self {
		Self {
			inner: BuildParachainRpcExtensions::default(),
			params,
			pool: OnceLock::new(),
			_phantom: PhantomData,
		}
	}
}

impl<Block, RuntimeApi>
	BuildRpcExtensions<
		ParachainClient<Block, RuntimeApi>,
		ParachainBackend<Block>,
		TransactionPoolHandle<Block, ParachainClient<Block, RuntimeApi>>,
		sc_statement_store::Store,
	> for HopBuildRpcExtensions<Block, RuntimeApi>
where
	Block: NodeBlock,
	RuntimeApi:
		ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<
			Block,
			parachains_common_types::Balance,
		> + substrate_frame_rpc_system::AccountNonceApi<
			Block,
			parachains_common_types::AccountId,
			parachains_common_types::Nonce,
		>,
{
	fn build_rpc_extensions(
		&self,
		client: Arc<ParachainClient<Block, RuntimeApi>>,
		backend: Arc<ParachainBackend<Block>>,
		pool: Arc<TransactionPoolHandle<Block, ParachainClient<Block, RuntimeApi>>>,
		statement_store: Option<Arc<sc_statement_store::Store>>,
		spawn_handle: Arc<dyn sp_core::traits::SpawnNamed>,
		database_path: Option<&Path>,
	) -> sc_service::error::Result<jsonrpsee::RpcModule<()>> {
		let hop_pool = match self.pool.get() {
			Some(p) => p.clone(),
			None => {
				let p = self
					.params
					.build_pool(database_path.map(|p| p.to_path_buf()))
					.map_err(|e| sc_service::Error::Application(Box::new(e)))?;
				let task = sc_hop::build_maintenance_task::<Block, _, _>(
					&client,
					&pool,
					p.clone(),
					self.params.promotion_buffer_blocks,
					self.params.check_interval,
				);
				spawn_handle.spawn("hop-maintenance", None, Box::pin(task.run()));
				let _ = self.pool.set(p.clone());
				p
			},
		};

		let mut module = self.inner.build_rpc_extensions(
			client.clone(),
			backend,
			pool,
			statement_store,
			spawn_handle,
			database_path,
		)?;
		module
			.merge(HopRpcServer::new(hop_pool, client).into_rpc())
			.map_err(|e| sc_service::Error::Other(e.to_string()))?;
		Ok(module)
	}
}
