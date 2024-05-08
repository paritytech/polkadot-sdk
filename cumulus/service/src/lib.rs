#![allow(unused_imports, unused)]

use std::sync::Arc;

pub mod aura;
pub mod aura_async_backing;
pub mod relay;
pub mod relay_to_aura;

// TODO: IDEALLY, nothing should be generic over Block and RuntimeApi here, we should be able to
// always get the block from RuntimeApi as it relies on Block as well, as they should always be the
// same anyways. Possibly requires a big refactor to move Block to an associated type instead of
// generic in a few places.

pub type ParachainClient<Block, RuntimeApi, HostFns> =
	sc_service::TFullClient<Block, RuntimeApi, sc_executor::WasmExecutor<HostFns>>;
pub type ParachainBackend<Block> = sc_service::TFullBackend<Block>;
pub type ParachainBlockImport<Block, RuntimeApi, HostFns> =
	cumulus_client_consensus_common::ParachainBlockImport<
		Block,
		Arc<ParachainClient<Block, RuntimeApi, HostFns>>,
		ParachainBackend<Block>,
	>;

pub type ParachainService<Block, RuntimeApi, HostFns> = sc_service::PartialComponents<
	ParachainClient<Block, RuntimeApi, HostFns>,
	ParachainBackend<Block>,
	(),
	sc_consensus::DefaultImportQueue<Block>,
	sc_transaction_pool::FullPool<Block, ParachainClient<Block, RuntimeApi, HostFns>>,
	(
		ParachainBlockImport<Block, RuntimeApi, HostFns>,
		Option<sc_telemetry::Telemetry>,
		Option<sc_telemetry::TelemetryWorkerHandle>,
	),
>;

pub trait BuildImportQueue<
	Block: sp_runtime::traits::Block,
	RuntimeApi: sp_api::ConstructRuntimeApi<Block, ParachainClient<Block, RuntimeApi, HostFns>>
		+ Send
		+ Sync
		+ 'static,
	HostFns: sp_wasm_interface::HostFunctions,
>:
	FnOnce(
	Arc<ParachainClient<Block, RuntimeApi, HostFns>>,
	ParachainBlockImport<Block, RuntimeApi, HostFns>,
	&sc_service::Configuration,
	Option<sc_telemetry::TelemetryHandle>,
	&sc_service::TaskManager,
) -> Result<sc_consensus::DefaultImportQueue<Block>, sc_service::Error>
{
}
impl<
		RuntimeApi: sp_api::ConstructRuntimeApi<Block, ParachainClient<Block, RuntimeApi, HostFns>>
			+ Send
			+ Sync
			+ 'static,
		Block: sp_runtime::traits::Block,
		HostFns: sp_wasm_interface::HostFunctions,
		T: FnOnce(
			Arc<ParachainClient<Block, RuntimeApi, HostFns>>,
			ParachainBlockImport<Block, RuntimeApi, HostFns>,
			&sc_service::Configuration,
			Option<sc_telemetry::TelemetryHandle>,
			&sc_service::TaskManager,
		) -> Result<sc_consensus::DefaultImportQueue<Block>, sc_service::Error>,
	> BuildImportQueue<Block, RuntimeApi, HostFns> for T
{
}

pub trait BuildRpcExtension<
	Block: sp_runtime::traits::Block,
	RuntimeApi: sp_api::ConstructRuntimeApi<Block, ParachainClient<Block, RuntimeApi, HostFns>>
		+ Send
		+ Sync
		+ 'static,
	HostFns: sp_wasm_interface::HostFunctions,
>:
	Fn(
		sc_rpc::DenyUnsafe,
		Arc<ParachainClient<Block, RuntimeApi, HostFns>>,
		Arc<ParachainBackend<Block>>,
		Arc<sc_transaction_pool::FullPool<Block, ParachainClient<Block, RuntimeApi, HostFns>>>,
	) -> Result<jsonrpsee::RpcModule<()>, sc_service::Error>
	+ 'static where
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>,
{
}
impl<
		RuntimeApi: sp_api::ConstructRuntimeApi<Block, ParachainClient<Block, RuntimeApi, HostFns>>
			+ Send
			+ Sync
			+ 'static,
		Block: sp_runtime::traits::Block,
		HostFns: sp_wasm_interface::HostFunctions,
		T: Fn(
				sc_rpc::DenyUnsafe,
				Arc<ParachainClient<Block, RuntimeApi, HostFns>>,
				Arc<ParachainBackend<Block>>,
				Arc<
					sc_transaction_pool::FullPool<
						Block,
						ParachainClient<Block, RuntimeApi, HostFns>,
					>,
				>,
			) -> Result<jsonrpsee::RpcModule<()>, sc_service::Error>
			+ 'static,
	> BuildRpcExtension<Block, RuntimeApi, HostFns> for T
where
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>,
{
}

pub trait StartConsensus<Block: sp_runtime::traits::Block, RuntimeApi, HostFns>:
	FnOnce(
	Arc<ParachainClient<Block, RuntimeApi, HostFns>>,
	ParachainBlockImport<Block, RuntimeApi, HostFns>,
	Option<&substrate_prometheus_endpoint::Registry>,
	Option<sc_telemetry::TelemetryHandle>,
	&sc_service::TaskManager,
	Arc<dyn cumulus_relay_chain_interface::RelayChainInterface>,
	Arc<sc_transaction_pool::FullPool<Block, ParachainClient<Block, RuntimeApi, HostFns>>>,
	Arc<sc_network_sync::SyncingService<Block>>,
	sp_keystore::KeystorePtr,
	std::time::Duration,
	cumulus_primitives_core::ParaId,
	polkadot_primitives::CollatorPair,
	cumulus_relay_chain_interface::OverseerHandle,
	Arc<dyn Fn(<Block as sp_runtime::traits::Block>::Hash, Option<Vec<u8>>) + Send + Sync>,
	Arc<ParachainBackend<Block>>,
) -> Result<(), sc_service::Error>
{
}

impl<
		Block: sp_runtime::traits::Block,
		RuntimeApi,
		HostFns,
		T: FnOnce(
			Arc<ParachainClient<Block, RuntimeApi, HostFns>>,
			ParachainBlockImport<Block, RuntimeApi, HostFns>,
			Option<&substrate_prometheus_endpoint::Registry>,
			Option<sc_telemetry::TelemetryHandle>,
			&sc_service::TaskManager,
			Arc<dyn cumulus_relay_chain_interface::RelayChainInterface>,
			Arc<sc_transaction_pool::FullPool<Block, ParachainClient<Block, RuntimeApi, HostFns>>>,
			Arc<sc_network_sync::SyncingService<Block>>,
			sp_keystore::KeystorePtr,
			std::time::Duration,
			cumulus_primitives_core::ParaId,
			polkadot_primitives::CollatorPair,
			cumulus_relay_chain_interface::OverseerHandle,
			Arc<dyn Fn(<Block as sp_runtime::traits::Block>::Hash, Option<Vec<u8>>) + Send + Sync>,
			Arc<ParachainBackend<Block>>,
		) -> Result<(), sc_service::Error>,
	> StartConsensus<Block, RuntimeApi, HostFns> for T
{
}

/// Starts a `ServiceBuilder` for a full service.
///
/// Use this macro if you don't actually need the full service, but just the builder in order to
/// be able to perform chain operations.
pub fn new_partial<
	Block: sp_runtime::traits::Block,
	RuntimeApi: sp_api::ConstructRuntimeApi<Block, ParachainClient<Block, RuntimeApi, HostFns>>
		+ Send
		+ Sync
		+ 'static,
	HostFns: sp_wasm_interface::HostFunctions,
	BIQ: BuildImportQueue<Block, RuntimeApi, HostFns>,
>(
	config: &sc_service::Configuration,
	build_import_queue: BIQ,
) -> Result<ParachainService<Block, RuntimeApi, HostFns>, sc_service::Error>
where
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>,
{
	let telemetry = config
		.telemetry_endpoints
		.clone()
		.filter(|x| !x.is_empty())
		.map(|endpoints| -> Result<_, sc_telemetry::Error> {
			let worker = sc_telemetry::TelemetryWorker::new(16)?;
			let telemetry = worker.handle().new_telemetry(endpoints);
			Ok((worker, telemetry))
		})
		.transpose()?;

	let heap_pages =
		config.default_heap_pages.map_or(sc_executor::DEFAULT_HEAP_ALLOC_STRATEGY, |h| {
			sc_executor::HeapAllocStrategy::Static { extra_pages: h as _ }
		});

	let executor = sc_executor::WasmExecutor::<HostFns>::builder()
		.with_execution_method(config.wasm_method)
		.with_onchain_heap_alloc_strategy(heap_pages)
		.with_offchain_heap_alloc_strategy(heap_pages)
		.with_max_runtime_instances(config.max_runtime_instances)
		.with_runtime_cache_size(config.runtime_cache_size)
		.build();

	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts_record_import::<Block, RuntimeApi, _>(
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

	let block_import = ParachainBlockImport::new(client.clone(), backend.clone());

	let import_queue = build_import_queue(
		client.clone(),
		block_import.clone(),
		config,
		telemetry.as_ref().map(|telemetry| telemetry.handle()),
		&task_manager,
	)?;

	Ok(sc_service::PartialComponents {
		backend,
		client,
		import_queue,
		keystore_container,
		task_manager,
		transaction_pool,
		select_chain: (),
		other: (block_import, telemetry, telemetry_worker_handle),
	})
}

// TODO: test to ensure functions returned from this crate match the trait interfaces above.

/// Checks that the hardware meets the requirements and print a warning otherwise.
pub(crate) fn warn_if_slow_hardware(hwbench: &sc_sysinfo::HwBench) {
	// Polkadot para-chains should generally use these requirements to ensure that the relay-chain
	// will not take longer than expected to import its blocks.
	if let Err(err) = frame_benchmarking_cli::SUBSTRATE_REFERENCE_HARDWARE.check_hardware(hwbench) {
		log::warn!(
			"⚠️  The hardware does not meet the minimal requirements {} for role 'Authority' find out more at:\n\
			https://wiki.polkadot.network/docs/maintain-guides-how-to-validate-polkadot#reference-hardware",
			err
		);
	}
}
