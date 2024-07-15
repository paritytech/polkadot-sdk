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

use crate::common::{
	BuildImportQueue, BuildSelectChain, ConstructNodeRuntimeApi, NodeBackend, NodeSpec,
	NodeSpecProvider, RpcModule, ServiceError, ServiceResult,
};
use futures::FutureExt;
use sc_client_api::Backend;
use sc_consensus::{DefaultImportQueue, LongestChain};
use sc_executor::WasmExecutor;
use sc_network::NetworkBackend;
use sc_service::{Configuration, PartialComponents, TFullClient, TaskManager};
use sc_telemetry::TelemetryHandle;
use sc_transaction_pool_api::OffchainTransactionPoolFactory;
use sp_core::crypto::AccountId32;
use sp_runtime::{generic, traits, traits::Block as BlockT, OpaqueExtrinsic};
use std::{marker::PhantomData, sync::Arc};
use substrate_frame_rpc_system::{System, SystemApiServer};

type BlockNumber = u32;
type AccountId = AccountId32;
type Nonce = u32;

/// Opaque block header type.
type Header = generic::Header<BlockNumber, traits::BlakeTwo256>;
/// Opaque block type.
type Block = generic::Block<Header, OpaqueExtrinsic>;

#[cfg(feature = "runtime-benchmarks")]
type HostFunctions =
	(sp_io::SubstrateHostFunctions, frame_benchmarking::benchmarking::HostFunctions);

#[cfg(not(feature = "runtime-benchmarks"))]
type HostFunctions = sp_io::SubstrateHostFunctions;

type SolochainClient<RuntimeApi> = TFullClient<Block, RuntimeApi, WasmExecutor<HostFunctions>>;

pub struct BuildManualSealImportQueue<RuntimeApi>(PhantomData<RuntimeApi>);

impl<RuntimeApi> BuildImportQueue<Block, SolochainClient<RuntimeApi>>
	for BuildManualSealImportQueue<RuntimeApi>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, SolochainClient<RuntimeApi>>,
{
	type BlockImport = ();

	fn build_import_queue(
		client: Arc<SolochainClient<RuntimeApi>>,
		_backend: Arc<NodeBackend<Block>>,
		config: &Configuration,
		_telemetry_handle: Option<TelemetryHandle>,
		task_manager: &TaskManager,
	) -> ServiceResult<(Self::BlockImport, DefaultImportQueue<Block>)> {
		Ok((
			(),
			sc_consensus_manual_seal::import_queue(
				Box::new(client),
				&task_manager.spawn_essential_handle(),
				config.prometheus_registry(),
			),
		))
	}
}

pub struct BuildSelectLongestChain;

impl BuildSelectChain<Block> for BuildSelectLongestChain {
	type SelectChain = LongestChain<NodeBackend<Block>, Block>;

	fn build_select_chain(backend: Arc<NodeBackend<Block>>) -> Self::SelectChain {
		LongestChain::new(backend.clone())
	}
}

pub struct SolochainNode<RuntimeApi>(PhantomData<RuntimeApi>);

impl<RuntimeApi> NodeSpec for SolochainNode<RuntimeApi>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, SolochainClient<RuntimeApi>>,
{
	type Block = Block;
	type RuntimeApi = RuntimeApi;
	type HostFunctions = HostFunctions;
	type BuildImportQueue = BuildManualSealImportQueue<RuntimeApi>;
	type BuildSelectChain = BuildSelectLongestChain;
}

impl<RuntimeApi> SolochainNode<RuntimeApi>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, SolochainClient<RuntimeApi>>,
	RuntimeApi::RuntimeApi: substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>
		+ sc_offchain::OffchainWorkerApi<Block>,
{
	fn do_start_node<Net>(
		config: Configuration,
		block_time: u64,
	) -> Result<TaskManager, ServiceError>
	where
		Net: NetworkBackend<Block, <Block as BlockT>::Hash>,
	{
		let PartialComponents {
			client,
			backend,
			mut task_manager,
			import_queue,
			keystore_container,
			select_chain,
			transaction_pool,
			other: (_, mut telemetry, _),
		} = Self::new_partial(&config)?;

		let net_config = sc_network::config::FullNetworkConfiguration::<
			Block,
			<Block as BlockT>::Hash,
			Net,
		>::new(&config.network);
		let metrics = Net::register_notification_metrics(
			config.prometheus_config.as_ref().map(|cfg| &cfg.registry),
		);

		let (network, system_rpc_tx, tx_handler_controller, network_starter, sync_service) =
			sc_service::build_network(sc_service::BuildNetworkParams {
				config: &config,
				client: client.clone(),
				transaction_pool: transaction_pool.clone(),
				spawn_handle: task_manager.spawn_handle(),
				import_queue,
				net_config,
				block_announce_validator_builder: None,
				warp_sync_params: None,
				block_relay: None,
				metrics,
			})?;

		if config.offchain_worker.enabled {
			task_manager.spawn_handle().spawn(
				"offchain-workers-runner",
				"offchain-worker",
				sc_offchain::OffchainWorkers::new(sc_offchain::OffchainWorkerOptions {
					runtime_api_provider: client.clone(),
					is_validator: config.role.is_authority(),
					keystore: Some(keystore_container.keystore()),
					offchain_db: backend.offchain_storage(),
					transaction_pool: Some(OffchainTransactionPoolFactory::new(
						transaction_pool.clone(),
					)),
					network_provider: Arc::new(network.clone()),
					enable_http_requests: true,
					custom_extensions: |_| vec![],
				})
				.run(client.clone(), task_manager.spawn_handle())
				.boxed(),
			);
		}

		let rpc_extensions_builder = {
			let client = client.clone();
			let pool = transaction_pool.clone();

			Box::new(move |deny_unsafe, _| {
				let mut module = RpcModule::new(());
				module
					.merge(System::new(client.clone(), pool.clone(), deny_unsafe).into_rpc())
					.map_err(|e| ServiceError::Application(e.into()))?;
				Ok(module)
			})
		};

		let prometheus_registry = config.prometheus_registry().cloned();

		sc_service::spawn_tasks(sc_service::SpawnTasksParams {
			config,
			client: client.clone(),
			backend,
			task_manager: &mut task_manager,
			keystore: keystore_container.keystore(),
			transaction_pool: transaction_pool.clone(),
			rpc_builder: rpc_extensions_builder,
			network,
			system_rpc_tx,
			tx_handler_controller,
			sync_service,
			telemetry: telemetry.as_mut(),
		})?;

		let proposer = sc_basic_authorship::ProposerFactory::new(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool.clone(),
			prometheus_registry.as_ref(),
			telemetry.as_ref().map(|x| x.handle()),
		);

		// Start ManualSeal consensus
		let (mut sink, commands_stream) = futures::channel::mpsc::channel(1024);
		task_manager.spawn_handle().spawn("block_authoring", None, async move {
			loop {
				futures_timer::Delay::new(std::time::Duration::from_millis(block_time)).await;
				sink.try_send(sc_consensus_manual_seal::EngineCommand::SealNewBlock {
					create_empty: true,
					finalize: true,
					parent_hash: None,
					sender: None,
				})
				.unwrap();
			}
		});

		let params = sc_consensus_manual_seal::ManualSealParams {
			block_import: client.clone(),
			env: proposer,
			client,
			pool: transaction_pool,
			select_chain,
			commands_stream: Box::pin(commands_stream),
			consensus_data_provider: None,
			create_inherent_data_providers: move |_, ()| async move {
				Ok(sp_timestamp::InherentDataProvider::from_system_time())
			},
		};
		let authorship_future = sc_consensus_manual_seal::run_manual_seal(params);

		task_manager.spawn_essential_handle().spawn_blocking(
			"manual-seal",
			None,
			authorship_future,
		);

		network_starter.start_network();
		Ok(task_manager)
	}

	#[allow(dead_code)]
	pub fn start_node(config: Configuration, block_time: u64) -> Result<TaskManager, ServiceError> {
		match config.network.network_backend {
			sc_network::config::NetworkBackendType::Libp2p =>
				Self::do_start_node::<sc_network::NetworkWorker<_, _>>(config, block_time),
			sc_network::config::NetworkBackendType::Litep2p =>
				Self::do_start_node::<sc_network::Litep2pNetworkBackend>(config, block_time),
		}
	}
}

impl<RuntimeApi> NodeSpecProvider for SolochainNode<RuntimeApi>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, SolochainClient<RuntimeApi>>,
{
	type NodeSpec = Self;
}
