// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! HOP wiring as a [`polkadot_omni_node_lib::NodeExtension`].

use std::{
	marker::PhantomData,
	path::Path,
	sync::{Arc, OnceLock},
};

use jsonrpsee::RpcModule;
use polkadot_omni_node_lib::{
	common::types::ParachainClient, ConstructNodeRuntimeApi, NodeBlock, NodeExtension,
	NodeExtensionFactory,
};
use sc_hop::{HopApiServer, HopDataPool, HopParams, HopRpcServer};
use sc_service::TaskManager;
use sc_transaction_pool::TransactionPoolHandle;
use sp_runtime::AccountId32;

/// `OnceLock` carries the pool from `on_start` to `build_rpc_extension`; both
/// trait methods take `&self`.
pub struct HopExtension<Block, RuntimeApi> {
	params: HopParams,
	pool: Arc<OnceLock<Arc<HopDataPool>>>,
	_phantom: PhantomData<(Block, RuntimeApi)>,
}

impl<Block, RuntimeApi> HopExtension<Block, RuntimeApi> {
	pub fn new(params: HopParams) -> Self {
		Self { params, pool: Arc::new(OnceLock::new()), _phantom: PhantomData }
	}
}

impl<Block, RuntimeApi> NodeExtension<Block, RuntimeApi> for HopExtension<Block, RuntimeApi>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: sp_hop::HopRuntimeApi<Block, AccountId32>,
{
	fn on_start(
		&self,
		client: Arc<ParachainClient<Block, RuntimeApi>>,
		transaction_pool: Arc<TransactionPoolHandle<Block, ParachainClient<Block, RuntimeApi>>>,
		task_manager: &TaskManager,
		database_path: Option<&Path>,
	) -> sc_service::error::Result<()> {
		let pool = self
			.params
			.build_pool(database_path.map(|p| p.to_path_buf()))
			.map_err(|e| sc_service::Error::Application(Box::new(e)))?;

		let task = sc_hop::build_maintenance_task::<Block, _, _>(
			&client,
			&transaction_pool,
			pool.clone(),
			self.params.promotion_buffer_blocks,
			self.params.check_interval,
		);
		task_manager.spawn_handle().spawn("hop-maintenance", None, task.run());

		let _ = self.pool.set(pool);
		Ok(())
	}

	fn build_rpc_extension(
		&self,
		client: Arc<ParachainClient<Block, RuntimeApi>>,
	) -> sc_service::error::Result<RpcModule<()>> {
		let mut module = RpcModule::new(());
		if let Some(pool) = self.pool.get() {
			module
				.merge(HopRpcServer::new(pool.clone(), client).into_rpc())
				.map_err(|e| sc_service::Error::Other(e.to_string()))?;
		}
		Ok(module)
	}
}

/// Wires HOP for the Bulletin runtime's `(Block<u32>, aura_sr25519)` combo.
pub struct HopExtensionFactory {
	params: HopParams,
}

impl HopExtensionFactory {
	pub fn new(params: HopParams) -> Self {
		Self { params }
	}
}

impl NodeExtensionFactory for HopExtensionFactory {
	fn create_aura_sr25519_u32(
		&self,
	) -> Option<
		Box<
			dyn NodeExtension<
				polkadot_omni_node_lib::common::BlockU32,
				polkadot_omni_node_lib::fake_runtime_api::aura_sr25519::RuntimeApi,
			>,
		>,
	> {
		Some(Box::new(HopExtension::<
			polkadot_omni_node_lib::common::BlockU32,
			polkadot_omni_node_lib::fake_runtime_api::aura_sr25519::RuntimeApi,
		>::new(self.params.clone())))
	}
}
