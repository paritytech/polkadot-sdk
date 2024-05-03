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

use crate::service::{aura, AuraParams, ParachainBackend, ParachainBlockImport, ParachainClient};
use cumulus_client_collator::service::CollatorService;
use cumulus_client_consensus_proposer::Proposer;
use cumulus_primitives_core::{relay_chain::ValidationCode, ParaId};
use cumulus_relay_chain_interface::{OverseerHandle, RelayChainInterface};
use parachains_common::{AuraId, Block, Hash};
use polkadot_primitives::CollatorPair;
use sc_network_sync::SyncingService;
use sc_service::{Configuration, TaskManager};
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

/// Build the import queue for Aura-based runtimes.
pub fn build_aura_import_queue(
	client: Arc<ParachainClient<crate::fake_runtime_api::aura::RuntimeApi>>,
	block_import: ParachainBlockImport<crate::fake_runtime_api::aura::RuntimeApi>,
	config: &Configuration,
	telemetry: Option<TelemetryHandle>,
	task_manager: &TaskManager,
) -> Result<sc_consensus::DefaultImportQueue<Block>, sc_service::Error> {
	let slot_duration = cumulus_client_consensus_aura::slot_duration(&*client)?;

	cumulus_client_consensus_aura::import_queue::<
		sp_consensus_aura::sr25519::AuthorityPair,
		_,
		_,
		_,
		_,
		_,
	>(cumulus_client_consensus_aura::ImportQueueParams {
		block_import,
		client,
		create_inherent_data_providers: move |_, _| async move {
			let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

			let slot =
				sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
					*timestamp,
					slot_duration,
				);

			Ok((slot, timestamp))
		},
		registry: config.prometheus_registry(),
		spawner: &task_manager.spawn_essential_handle(),
		telemetry,
	})
		.map_err(Into::into)
}