use crate::service::{aura, AuraParams, ParachainBackend, ParachainBlockImport, ParachainClient};
use cumulus_client_collator::service::CollatorService;
use cumulus_client_consensus_proposer::Proposer;
use cumulus_primitives_core::{relay_chain::ValidationCode, ParaId};
use cumulus_relay_chain_interface::{OverseerHandle, RelayChainInterface};
use parachains_common::{AuraId, Block, Hash};
use polkadot_primitives::CollatorPair;
use sc_network_sync::SyncingService;
use sc_service::TaskManager;
use sc_telemetry::TelemetryHandle;
use sp_keystore::KeystorePtr;
use sp_runtime::app_crypto::AppCrypto;
use std::{sync::Arc, time::Duration};
use substrate_prometheus_endpoint::Registry;

/// Start consensus using the lookahead aura collator.
pub fn start_lookahead_aura_consensus(
	client: Arc<ParachainClient<crate::fake_runtime_api::aura::RuntimeApi>>,
	block_import: ParachainBlockImport<crate::fake_runtime_api::aura::RuntimeApi>,
	prometheus_registry: Option<&Registry>,
	telemetry: Option<TelemetryHandle>,
	task_manager: &TaskManager,
	relay_chain_interface: Arc<dyn RelayChainInterface>,
	transaction_pool: Arc<
		sc_transaction_pool::FullPool<
			Block,
			ParachainClient<crate::fake_runtime_api::aura::RuntimeApi>,
		>,
	>,
	sync_oracle: Arc<SyncingService<Block>>,
	keystore: KeystorePtr,
	relay_chain_slot_duration: Duration,
	para_id: ParaId,
	collator_key: CollatorPair,
	overseer_handle: OverseerHandle,
	announce_block: Arc<dyn Fn(Hash, Option<Vec<u8>>) + Send + Sync>,
	backend: Arc<ParachainBackend>,
) -> Result<(), sc_service::Error> {
	let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
		task_manager.spawn_handle(),
		client.clone(),
		transaction_pool,
		prometheus_registry,
		telemetry.clone(),
	);

	let collator_service = CollatorService::new(
		client.clone(),
		Arc::new(task_manager.spawn_handle()),
		announce_block,
		client.clone(),
	);

	let params = AuraParams {
		create_inherent_data_providers: move |_, ()| async move { Ok(()) },
		block_import,
		para_client: client.clone(),
		para_backend: backend,
		relay_client: relay_chain_interface,
		code_hash_provider: move |block_hash| {
			client.code_at(block_hash).ok().map(|c| ValidationCode::from(c).hash())
		},
		sync_oracle,
		keystore,
		collator_key,
		para_id,
		overseer_handle,
		relay_chain_slot_duration,
		proposer: Proposer::new(proposer_factory),
		collator_service,
		authoring_duration: Duration::from_millis(1500),
		reinitialize: false,
	};

	let fut = aura::run::<Block, <AuraId as AppCrypto>::Pair, _, _, _, _, _, _, _, _, _>(params);
	task_manager.spawn_essential_handle().spawn("aura", None, fut);

	Ok(())
}
