use crate::{ParachainBackend, ParachainBlockImport, ParachainClient};
use codec::{Codec, Decode};
use cumulus_client_cli::CollatorOptions;
use cumulus_client_collator::service::CollatorService;
use cumulus_client_consensus_aura::collators::{
	basic::{self as basic_aura, Params as BasicAuraParams},
	lookahead::{self as aura, Params as AuraParams},
};
use cumulus_client_consensus_common::{
	ParachainBlockImport as TParachainBlockImport, ParachainCandidate, ParachainConsensus,
};
use cumulus_client_consensus_proposer::Proposer;
use cumulus_client_consensus_relay_chain::Verifier as RelayChainVerifier;
#[allow(deprecated)]
use cumulus_client_service::old_consensus;
use cumulus_client_service::{
	build_network, build_relay_chain_interface, prepare_node_config, start_relay_chain_tasks,
	BuildNetworkParams, CollatorSybilResistance, DARecoveryProfile, StartRelayChainTasksParams,
};
use cumulus_primitives_core::{
	relay_chain::{Hash as PHash, PersistedValidationData, ValidationCode},
	ParaId,
};
use cumulus_relay_chain_interface::{OverseerHandle, RelayChainInterface};
use futures::{lock::Mutex, prelude::*};
use jsonrpsee::RpcModule;
use polkadot_primitives::CollatorPair;
use sc_consensus::{
	import_queue::{BasicQueue, Verifier as VerifierT},
	BlockImportParams, ImportQueue,
};
use sc_executor::{HeapAllocStrategy, WasmExecutor, DEFAULT_HEAP_ALLOC_STRATEGY};
use sc_network::{config::FullNetworkConfiguration, NetworkBlock};
use sc_network_sync::SyncingService;
use sc_rpc::DenyUnsafe;
use sc_service::{Configuration, PartialComponents, TFullBackend, TFullClient, TaskManager};
use sc_telemetry::{Telemetry, TelemetryHandle, TelemetryWorker, TelemetryWorkerHandle};
use sp_api::{ApiExt, ConstructRuntimeApi, ProvideRuntimeApi};
use sp_consensus_aura::AuraApi;
use sp_core::{traits::SpawnEssentialNamed, Pair};
use sp_keystore::KeystorePtr;
use sp_runtime::{
	app_crypto::AppCrypto,
	traits::{Block as BlockT, Header as HeaderT},
};
use std::{marker::PhantomData, sync::Arc, time::Duration};
use substrate_prometheus_endpoint::Registry;

pub fn start_consensus<Block, RuntimeApi, HostFns>(
	client: Arc<ParachainClient<Block, RuntimeApi, HostFns>>,
	block_import: ParachainBlockImport<Block, RuntimeApi, HostFns>,
	prometheus_registry: Option<&Registry>,
	telemetry: Option<TelemetryHandle>,
	task_manager: &TaskManager,
	relay_chain_interface: Arc<dyn RelayChainInterface>,
	transaction_pool: Arc<
		sc_transaction_pool::FullPool<Block, ParachainClient<Block, RuntimeApi, HostFns>>,
	>,
	sync_oracle: Arc<SyncingService<Block>>,
	keystore: KeystorePtr,
	relay_chain_slot_duration: Duration,
	para_id: ParaId,
	collator_key: CollatorPair,
	overseer_handle: OverseerHandle,
	announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	_backend: Arc<ParachainBackend<Block>>,
) -> Result<(), sc_service::Error>
where
	HostFns: sp_wasm_interface::HostFunctions,
	Block: BlockT,
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<Block, RuntimeApi, HostFns>>
		+ Send
		+ Sync
		+ 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<Block>
		+ sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ cumulus_primitives_core::CollectCollationInfo<Block>
		+ sp_consensus_aura::AuraApi<Block, sp_consensus_aura::sr25519::AuthorityId>, /* TODO: double check if needs to be generic. */
{
	let slot_duration = cumulus_client_consensus_aura::slot_duration(&*client)?;

	let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
		task_manager.spawn_handle(),
		client.clone(),
		transaction_pool,
		prometheus_registry,
		telemetry.clone(),
	);
	let proposer = Proposer::new(proposer_factory);

	let collator_service = CollatorService::new(
		client.clone(),
		Arc::new(task_manager.spawn_handle()),
		announce_block,
		client.clone(),
	);

	let params = BasicAuraParams {
		create_inherent_data_providers: move |_, ()| async move { Ok(()) },
		block_import,
		para_client: client,
		relay_client: relay_chain_interface,
		sync_oracle,
		keystore,
		collator_key,
		para_id,
		overseer_handle,
		slot_duration,
		relay_chain_slot_duration,
		proposer,
		collator_service,
		// Very limited proposal time.
		authoring_duration: Duration::from_millis(500), /* TODO: probably also needs ot be
		                                                 * parameterized. */
		collation_request_receiver: None,
	};

	let fut = basic_aura::run::<
		Block,
		<sp_consensus_aura::sr25519::AuthorityId as AppCrypto>::Pair,
		_,
		_,
		_,
		_,
		_,
		_,
		_,
	>(params);
	task_manager.spawn_essential_handle().spawn("aura", None, fut);

	Ok(())
}

/// Build the import queue for the shell runtime.
pub fn build_import_queue<Block: BlockT, RuntimeApi, HostFns>(
	client: Arc<ParachainClient<Block, RuntimeApi, HostFns>>,
	block_import: ParachainBlockImport<Block, RuntimeApi, HostFns>,
	config: &Configuration,
	_: Option<TelemetryHandle>,
	task_manager: &TaskManager,
) -> Result<sc_consensus::DefaultImportQueue<Block>, sc_service::Error>
where
	HostFns: sp_wasm_interface::HostFunctions,
	Block: BlockT,
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<Block, RuntimeApi, HostFns>>
		+ Send
		+ Sync
		+ 'static,
	RuntimeApi::RuntimeApi: sp_block_builder::BlockBuilder<Block>,
{
	todo!();
}
