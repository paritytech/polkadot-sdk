// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

use crate::cli::Consensus;
use crate::service::parachains_common::Hash;
use futures::{FutureExt, SinkExt};
use polkadot_sdk::{
	sc_client_api::backend::Backend,
	sc_consensus_manual_seal::{seal_block, SealBlockParams},
	sc_executor::WasmExecutor,
	sc_service::{error::Error as ServiceError, Configuration, TaskManager},
	sc_telemetry::{Telemetry, TelemetryWorker},
	sc_transaction_pool_api::OffchainTransactionPoolFactory,
	sp_runtime::traits::Block as BlockT,
	*,
};
use revive_dev_runtime::{OpaqueBlock as Block, RuntimeApi};
use std::sync::{
	atomic::{AtomicU64, Ordering},
	Arc, LazyLock, Mutex,
};

type HostFunctions = sp_io::SubstrateHostFunctions;

#[docify::export]
pub(crate) type FullClient =
	sc_service::TFullClient<Block, RuntimeApi, WasmExecutor<HostFunctions>>;

pub type FullBackend = sc_service::TFullBackend<Block>;
type FullSelectChain = sc_consensus::LongestChain<FullBackend, Block>;

/// Assembly of PartialComponents (enough to run chain ops subcommands)
pub type Service = sc_service::PartialComponents<
	FullClient,
	FullBackend,
	FullSelectChain,
	sc_consensus::DefaultImportQueue<Block>,
	sc_transaction_pool::TransactionPoolHandle<Block, FullClient>,
	Option<Telemetry>,
>;

pub type SharedDelta = Arc<Mutex<Option<u64>>>;

pub static NEXT_TIMESTAMP: LazyLock<Arc<AtomicU64>> = LazyLock::new(|| {
	// Initialize with current system time to ensure proper starting point
	let now = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap_or_default()
		.as_millis() as u64;
	Arc::new(AtomicU64::new(now))
});

fn create_timestamp_provider(
	delta_for_inherent: SharedDelta,
	next_timestamp_ref: Arc<AtomicU64>,
) -> impl Fn(
	Hash,
	(),
) -> std::pin::Pin<
	Box<
		dyn std::future::Future<
				Output = Result<
					sp_timestamp::InherentDataProvider,
					Box<dyn std::error::Error + Send + Sync>,
				>,
			> + Send,
	>,
> + Send
       + Clone {
	move |_parent_hash, ()| {
		let delta_for_inherent = delta_for_inherent.clone();
		let next_timestamp_ref = next_timestamp_ref.clone();

		Box::pin(async move {
			// Priority 1: Check if timestamp_delta was provided via engine_createBlock
			let delta_ms_guard = delta_for_inherent.lock().unwrap();
			if let Some(override_delta_ms) = *delta_ms_guard {
				// Use the immediate timestamp delta (already in milliseconds)
				drop(delta_ms_guard);
				*delta_for_inherent.lock().unwrap() = None; // Clear after use
				NEXT_TIMESTAMP.store(override_delta_ms, Ordering::SeqCst);
				return Ok(sp_timestamp::InherentDataProvider::new(override_delta_ms.into()));
			}
			drop(delta_ms_guard);

			// Priority 2: Check if a specific timestamp was set via evm_setNextBlockTimestamp RPC
			let explicit_timestamp_ms = next_timestamp_ref.load(Ordering::SeqCst);
			if explicit_timestamp_ms > 0 {
				// Use the explicitly set timestamp and reset it to prevent reuse
				next_timestamp_ref.store(0, Ordering::SeqCst);
				// Update the global timestamp counter to match the explicit timestamp
				// so subsequent auto-increments start from the correct base
				NEXT_TIMESTAMP.store(explicit_timestamp_ms, Ordering::SeqCst);
				return Ok(sp_timestamp::InherentDataProvider::new(explicit_timestamp_ms.into()));
			}

			// Priority 3: Fall back to auto-increment logic
			let default_delta = 1000; // Default to 6 second
			let next_timestamp =
				NEXT_TIMESTAMP.fetch_add(default_delta, Ordering::SeqCst) + default_delta;
			Ok(sp_timestamp::InherentDataProvider::new(next_timestamp.into()))
		})
	}
}

pub fn new_partial(config: &Configuration) -> Result<Service, ServiceError> {
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

	let executor = sc_service::new_wasm_executor(&config.executor);

	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts::<Block, RuntimeApi, _>(
			config,
			telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
			executor,
		)?;
	let client = Arc::new(client);

	let telemetry = telemetry.map(|(worker, telemetry)| {
		task_manager.spawn_handle().spawn("telemetry", None, worker.run());
		telemetry
	});

	let select_chain = sc_consensus::LongestChain::new(backend.clone());

	let transaction_pool = Arc::from(
		sc_transaction_pool::Builder::new(
			task_manager.spawn_essential_handle(),
			client.clone(),
			config.role.is_authority().into(),
		)
		.with_options(config.transaction_pool.clone())
		.build(),
	);

	let import_queue = sc_consensus_manual_seal::import_queue(
		Box::new(client.clone()),
		&task_manager.spawn_essential_handle(),
		None,
	);

	Ok(sc_service::PartialComponents {
		client,
		backend,
		task_manager,
		import_queue,
		keystore_container,
		select_chain,
		transaction_pool,
		other: (telemetry),
	})
}

/// Builds a new service for a full client.
pub fn new_full<Network: sc_network::NetworkBackend<Block, <Block as BlockT>::Hash>>(
	config: Configuration,
	consensus: Consensus,
) -> Result<TaskManager, ServiceError> {
	let sc_service::PartialComponents {
		client,
		backend,
		mut task_manager,
		import_queue,
		keystore_container,
		select_chain,
		transaction_pool,
		other: mut telemetry,
	} = new_partial(&config)?;

	let net_config = sc_network::config::FullNetworkConfiguration::<
		Block,
		<Block as BlockT>::Hash,
		Network,
	>::new(&config.network, None);
	let metrics = Network::register_notification_metrics(None);

	let (network, system_rpc_tx, tx_handler_controller, sync_service) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &config,
			net_config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue,
			block_announce_validator_builder: None,
			warp_sync_config: None,
			block_relay: None,
			metrics,
		})?;

	if config.offchain_worker.enabled {
		let offchain_workers =
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
			})?;
		task_manager.spawn_handle().spawn(
			"offchain-workers-runner",
			"offchain-worker",
			offchain_workers.run(client.clone(), task_manager.spawn_handle()).boxed(),
		);
	}

	let proposer = sc_basic_authorship::ProposerFactory::new(
		task_manager.spawn_handle(),
		client.clone(),
		transaction_pool.clone(),
		None,
		telemetry.as_ref().map(|x| x.handle()),
	);

	let mut consensus_type: Consensus = Consensus::None;

	let (sink, manual_trigger_stream) =
		futures::channel::mpsc::channel::<sc_consensus_manual_seal::EngineCommand<Hash>>(1024);

	let timestamp_delta_override: SharedDelta = Arc::new(Mutex::new(None));
	let delta_for_inherent = timestamp_delta_override.clone();

	// Shared timestamp state between RPC and consensus for evm_setNextBlockTimestamp
	let next_timestamp = Arc::new(AtomicU64::new(0));

	match consensus {
		Consensus::InstantSeal => {
			consensus_type = Consensus::InstantSeal;

			let create_inherent_data_providers =
				create_timestamp_provider(delta_for_inherent.clone(), next_timestamp.clone());

			let params = sc_consensus_manual_seal::InstantSealParams {
				block_import: client.clone(),
				env: proposer,
				client: client.clone(),
				pool: transaction_pool.clone(),
				select_chain,
				consensus_data_provider: None,
				create_inherent_data_providers,
				manual_trigger_stream,
			};

			let authorship_future = sc_consensus_manual_seal::run_instant_seal(params);

			task_manager.spawn_essential_handle().spawn_blocking(
				"instant-seal",
				None,
				authorship_future,
			);
		},
		Consensus::ManualSeal(Some(rate)) => {
			consensus_type = Consensus::ManualSeal(Some(rate));

			let mut new_sink = sink.clone();

			task_manager.spawn_handle().spawn("block_authoring", None, async move {
				loop {
					futures_timer::Delay::new(std::time::Duration::from_millis(rate)).await;
					new_sink.try_send(sc_consensus_manual_seal::EngineCommand::SealNewBlock {
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
				env: proposer.clone(),
				client: client.clone(),
				pool: transaction_pool.clone(),
				select_chain: select_chain.clone(),
				commands_stream: Box::pin(manual_trigger_stream),
				consensus_data_provider: None,
				create_inherent_data_providers: create_timestamp_provider(
					delta_for_inherent.clone(),
					next_timestamp.clone(),
				),
			};

			task_manager.spawn_essential_handle().spawn_blocking(
				"manual-seal",
				None,
				sc_consensus_manual_seal::run_manual_seal(params),
			);
		},
		Consensus::ManualSeal(None) => {
			consensus_type = Consensus::ManualSeal(None);

			let params = sc_consensus_manual_seal::ManualSealParams {
				block_import: client.clone(),
				env: proposer.clone(),
				client: client.clone(),
				pool: transaction_pool.clone(),
				select_chain: select_chain.clone(),
				commands_stream: Box::pin(manual_trigger_stream),
				consensus_data_provider: None,
				create_inherent_data_providers: create_timestamp_provider(
					delta_for_inherent.clone(),
					next_timestamp.clone(),
				),
			};

			task_manager.spawn_essential_handle().spawn_blocking(
				"manual-seal",
				None,
				sc_consensus_manual_seal::run_manual_seal(params),
			);
		},
		_ => {},
	}

	// Set up RPC
	let rpc_extensions_builder = {
		let client = client.clone();
		let backend = backend.clone();
		let pool = transaction_pool.clone();
		let sink = sink.clone();
		let timestamp_delta = timestamp_delta_override.clone();
		let next_timestamp_for_rpc = next_timestamp.clone();

		Box::new(move |_| {
			let deps = crate::rpc::FullDeps {
				client: client.clone(),
				backend: backend.clone(),
				pool: pool.clone(),
				manual_seal_sink: sink.clone(),
				consensus_type: consensus_type.clone(),
				timestamp_delta: timestamp_delta.clone(),
				next_timestamp: next_timestamp_for_rpc.clone(),
			};
			crate::rpc::create_full(deps).map_err(Into::into)
		})
	};

	let _rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		network,
		client: client.clone(),
		keystore: keystore_container.keystore(),
		task_manager: &mut task_manager,
		transaction_pool: transaction_pool.clone(),
		rpc_builder: rpc_extensions_builder,
		backend,
		system_rpc_tx,
		tx_handler_controller,
		sync_service,
		config,
		telemetry: telemetry.as_mut(),
		tracing_execute_block: None,
	})?;

	Ok(task_manager)
}
