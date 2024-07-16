// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Cumulus parachain collator primitives.

#![warn(missing_docs)]

pub mod aura;
pub mod command;
pub mod parachain;
mod solochain;

use cumulus_primitives_core::CollectCollationInfo;
use sc_client_api::{BlockBackend, UsageProvider};
use sc_client_db::{Backend, DbHash};
use sc_consensus::DefaultImportQueue;
use sc_executor::{HostFunctions, WasmExecutor};
use sc_rpc::DenyUnsafe;
pub use sc_service::{error::Result as ServiceResult, Error as ServiceError};
use sc_service::{Configuration, PartialComponents, TFullClient, TaskManager};
use sc_telemetry::{Telemetry, TelemetryHandle, TelemetryWorker, TelemetryWorkerHandle};
use sc_transaction_pool::FullPool;
use sp_api::{ApiExt, CallApiAt, ConstructRuntimeApi, Metadata, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder;
use sp_blockchain::{HeaderBackend, HeaderMetadata};
use sp_runtime::{
	traits::{Block as BlockT, BlockIdTo},
	OpaqueExtrinsic,
};
use sp_session::SessionKeys;
use sp_transaction_pool::runtime_api::TaggedTransactionQueue;
use std::sync::Arc;

pub(crate) type NodeBackend<Block> = Backend<Block>;

pub(crate) type FullNodeClient<Spec> = TFullClient<
	<Spec as NodeSpec>::Block,
	<Spec as NodeSpec>::RuntimeApi,
	WasmExecutor<<Spec as NodeSpec>::HostFunctions>,
>;

/// Assembly of PartialComponents (enough to run chain ops subcommands)
pub(crate) type NodeService<Spec> =
PartialComponents<
	FullNodeClient<Spec>,
	Backend<<Spec as NodeSpec>::Block>,
	<<Spec as NodeSpec>::BuildSelectChain as BuildSelectChain<
		<Spec as NodeSpec>::Block,
	>>::SelectChain,
	DefaultImportQueue<<Spec as NodeSpec>::Block>,
	FullPool<<Spec as NodeSpec>::Block, FullNodeClient<Spec>>,
	(
		<<Spec as NodeSpec>::BuildImportQueue as BuildImportQueue<
			<Spec as NodeSpec>::Block,
			FullNodeClient<Spec>,
		>>::BlockImport,
		Option<Telemetry>,
		Option<TelemetryWorkerHandle>,
	),
>;

/// A type representing all RPC extensions.
pub(crate) type RpcModule = jsonrpsee::RpcModule<()>;

/// Convenience trait that defines the basic bounds for the `RuntimeApi` of a parachain node.
pub trait NodeRuntimeApi<Block: BlockT>:
	ApiExt<Block>
	+ Metadata<Block>
	+ SessionKeys<Block>
	+ BlockBuilder<Block>
	+ TaggedTransactionQueue<Block>
	+ CollectCollationInfo<Block>
	+ Sized
{
}

impl<T, Block: BlockT> NodeRuntimeApi<Block> for T where
	T: ApiExt<Block>
		+ Metadata<Block>
		+ SessionKeys<Block>
		+ BlockBuilder<Block>
		+ TaggedTransactionQueue<Block>
		+ CollectCollationInfo<Block>
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

pub trait NodeClient<Block: BlockT>:
	ProvideRuntimeApi<Block, Api = Self::BoundedRuntimeApi>
	+ BlockBackend<Block>
	+ BlockIdTo<Block>
	+ HeaderBackend<Block>
	+ HeaderMetadata<Block, Error = sp_blockchain::Error>
	+ UsageProvider<Block>
{
	type BoundedRuntimeApi: NodeRuntimeApi<Block>;
}

impl<T, Block: BlockT> NodeClient<Block> for T
where
	T: ProvideRuntimeApi<Block>
		+ BlockBackend<Block>
		+ BlockIdTo<Block>
		+ HeaderBackend<Block>
		+ HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ UsageProvider<Block>,
	T::Api: NodeRuntimeApi<Block>,
{
	type BoundedRuntimeApi = T::Api;
}

/// Extra args that are passed when creating a new node spec.
pub struct NodeExtraArgs {
	pub use_slot_based_consensus: bool,
}

pub(crate) trait BuildImportQueue<Block: BlockT, Client> {
	type BlockImport;

	fn build_import_queue(
		client: Arc<Client>,
		backend: Arc<NodeBackend<Block>>,
		config: &Configuration,
		telemetry_handle: Option<TelemetryHandle>,
		task_manager: &TaskManager,
	) -> ServiceResult<(Self::BlockImport, DefaultImportQueue<Block>)>;
}

pub(crate) trait BuildSelectChain<Block: BlockT> {
	type SelectChain;

	fn build_select_chain(backend: Arc<NodeBackend<Block>>) -> Self::SelectChain;
}

pub(crate) trait BuildRpcExtensions<Block: BlockT, Client: NodeClient<Block> + 'static, Backend> {
	fn build_rpc_extensions(
		deny_unsafe: DenyUnsafe,
		client: Arc<Client>,
		backend: Arc<Backend>,
		pool: Arc<FullPool<Block, Client>>,
	) -> ServiceResult<RpcModule>;
}

pub(crate) trait NodeSpec {
	type Block: BlockT<Extrinsic = OpaqueExtrinsic, Hash = DbHash>
		+ for<'de> serde::Deserialize<'de>;
	type RuntimeApi: ConstructNodeRuntimeApi<Self::Block, FullNodeClient<Self>>;
	type HostFunctions: HostFunctions;

	type BuildImportQueue: BuildImportQueue<Self::Block, FullNodeClient<Self>>;
	type BuildSelectChain: BuildSelectChain<Self::Block>;

	fn new_partial(config: &Configuration) -> ServiceResult<NodeService<Self>> {
		let telemetry = config
			.telemetry_endpoints
			.clone()
			.filter(|x| !x.is_empty())
			.map(|endpoints| -> Result<_, sc_telemetry::Error> {
				let worker = TelemetryWorker::new(16)?;
				let telemetry = worker.handle().new_telemetry(endpoints);
				Ok((worker, telemetry))
			})
			.transpose()?;

		let executor = sc_service::new_wasm_executor(config);

		let (client, backend, keystore_container, task_manager) =
			sc_service::new_full_parts_record_import::<Self::Block, Self::RuntimeApi, _>(
				config,
				telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
				executor,
				true,
			)?;
		let client = Arc::new(client);

		let telemetry_worker_handle = telemetry.as_ref().map(|(worker, _)| worker.handle());
		let telemetry = telemetry.map(|(worker, telemetry)| {
			task_manager.spawn_handle().spawn("telemetry", None, worker.run());
			telemetry
		});

		let transaction_pool = sc_transaction_pool::BasicPool::new_full(
			config.transaction_pool.clone(),
			config.role.is_authority().into(),
			config.prometheus_registry(),
			task_manager.spawn_essential_handle(),
			client.clone(),
		);

		let (block_import, import_queue) = Self::BuildImportQueue::build_import_queue(
			client.clone(),
			backend.clone(),
			config,
			telemetry.as_ref().map(|telemetry| telemetry.handle()),
			&task_manager,
		)?;

		Ok(PartialComponents {
			client,
			backend: backend.clone(),
			task_manager,
			keystore_container,
			select_chain: Self::BuildSelectChain::build_select_chain(backend),
			import_queue,
			transaction_pool,
			other: (block_import, telemetry, telemetry_worker_handle),
		})
	}
}

pub(crate) trait NodeSpecProvider {
	type NodeSpec: NodeSpec;
}
