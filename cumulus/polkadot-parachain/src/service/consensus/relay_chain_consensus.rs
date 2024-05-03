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

use crate::service::{ParachainBackend, ParachainBlockImport, ParachainClient};
use codec::Codec;
use cumulus_client_consensus_common::{ParachainCandidate, ParachainConsensus};
use cumulus_client_consensus_relay_chain::Verifier as RelayChainVerifier;
use cumulus_client_service::old_consensus;
use cumulus_primitives_core::ParaId;
use cumulus_relay_chain_interface::{OverseerHandle, PHash, RelayChainInterface};
use futures::lock::Mutex;
use parachains_common::{Block, Hash, Header};
use polkadot_primitives::{CollatorPair, PersistedValidationData};
use sc_consensus::{
	import_queue::{BasicQueue, Verifier as VerifierT},
	BlockImportParams,
};
use sc_network_sync::SyncingService;
use sc_service::{Configuration, TaskManager};
use sc_telemetry::TelemetryHandle;
use sp_api::{ApiExt, ConstructRuntimeApi};
use sp_consensus_aura::AuraApi;
use sp_core::Pair;
use sp_keystore::KeystorePtr;
use sp_runtime::{app_crypto::AppCrypto, traits::Header as HeaderT};
use std::{marker::PhantomData, sync::Arc, time::Duration};
use substrate_prometheus_endpoint::Registry;

/// Start relay-chain consensus that is free for all. Everyone can submit a block, the relay-chain
/// decides what is backed and included.
pub fn start_relay_chain_consensus(
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
	_sync_oracle: Arc<SyncingService<Block>>,
	_keystore: KeystorePtr,
	_relay_chain_slot_duration: Duration,
	para_id: ParaId,
	collator_key: CollatorPair,
	overseer_handle: OverseerHandle,
	announce_block: Arc<dyn Fn(Hash, Option<Vec<u8>>) + Send + Sync>,
	_backend: Arc<ParachainBackend>,
) -> Result<(), sc_service::Error> {
	let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
		task_manager.spawn_handle(),
		client.clone(),
		transaction_pool,
		prometheus_registry,
		telemetry,
	);

	let free_for_all = cumulus_client_consensus_relay_chain::build_relay_chain_consensus(
		cumulus_client_consensus_relay_chain::BuildRelayChainConsensusParams {
			para_id,
			proposer_factory,
			block_import,
			relay_chain_interface: relay_chain_interface.clone(),
			create_inherent_data_providers: move |_, (relay_parent, validation_data)| {
				let relay_chain_interface = relay_chain_interface.clone();
				async move {
					let parachain_inherent =
                        cumulus_client_parachain_inherent::ParachainInherentDataProvider::create_at(
                            relay_parent,
                            &relay_chain_interface,
                            &validation_data,
                            para_id,
                        ).await;
					let parachain_inherent = parachain_inherent.ok_or_else(|| {
						Box::<dyn std::error::Error + Send + Sync>::from(
							"Failed to create parachain inherent",
						)
					})?;
					Ok(parachain_inherent)
				}
			},
		},
	);

	let spawner = task_manager.spawn_handle();

	// Required for free-for-all consensus
	#[allow(deprecated)]
	old_consensus::start_collator_sync(old_consensus::StartCollatorParams {
		para_id,
		block_status: client.clone(),
		announce_block,
		overseer_handle,
		spawner,
		key: collator_key,
		parachain_consensus: free_for_all,
		runtime_api: client.clone(),
	});

	Ok(())
}

enum BuildOnAccess<R> {
	Uninitialized(Option<Box<dyn FnOnce() -> R + Send + Sync>>),
	Initialized(R),
}

impl<R> BuildOnAccess<R> {
	fn get_mut(&mut self) -> &mut R {
		loop {
			match self {
				Self::Uninitialized(f) => {
					*self = Self::Initialized((f.take().unwrap())());
				},
				Self::Initialized(ref mut r) => return r,
			}
		}
	}
}

/// Special [`ParachainConsensus`] implementation that waits for the upgrade from
/// shell to a parachain runtime that implements Aura.
struct WaitForAuraConsensus<Client, AuraId> {
	client: Arc<Client>,
	aura_consensus: Arc<Mutex<BuildOnAccess<Box<dyn ParachainConsensus<Block>>>>>,
	relay_chain_consensus: Arc<Mutex<Box<dyn ParachainConsensus<Block>>>>,
	_phantom: PhantomData<AuraId>,
}

impl<Client, AuraId> Clone for WaitForAuraConsensus<Client, AuraId> {
	fn clone(&self) -> Self {
		Self {
			client: self.client.clone(),
			aura_consensus: self.aura_consensus.clone(),
			relay_chain_consensus: self.relay_chain_consensus.clone(),
			_phantom: PhantomData,
		}
	}
}

#[async_trait::async_trait]
impl<Client, AuraId> ParachainConsensus<Block> for WaitForAuraConsensus<Client, AuraId>
where
	Client: sp_api::ProvideRuntimeApi<Block> + Send + Sync,
	Client::Api: AuraApi<Block, AuraId>,
	AuraId: Send + Codec + Sync,
{
	async fn produce_candidate(
		&mut self,
		parent: &Header,
		relay_parent: PHash,
		validation_data: &PersistedValidationData,
	) -> Option<ParachainCandidate<Block>> {
		if self
			.client
			.runtime_api()
			.has_api::<dyn AuraApi<Block, AuraId>>(parent.hash())
			.unwrap_or(false)
		{
			self.aura_consensus
				.lock()
				.await
				.get_mut()
				.produce_candidate(parent, relay_parent, validation_data)
				.await
		} else {
			self.relay_chain_consensus
				.lock()
				.await
				.produce_candidate(parent, relay_parent, validation_data)
				.await
		}
	}
}

struct Verifier<Client, AuraId> {
	client: Arc<Client>,
	aura_verifier: BuildOnAccess<Box<dyn VerifierT<Block>>>,
	relay_chain_verifier: Box<dyn VerifierT<Block>>,
	_phantom: PhantomData<AuraId>,
}

#[async_trait::async_trait]
impl<Client, AuraId> VerifierT<Block> for Verifier<Client, AuraId>
where
	Client: sp_api::ProvideRuntimeApi<Block> + Send + Sync,
	Client::Api: AuraApi<Block, AuraId>,
	AuraId: Send + Sync + Codec,
{
	async fn verify(
		&mut self,
		block_import: BlockImportParams<Block>,
	) -> Result<BlockImportParams<Block>, String> {
		if self
			.client
			.runtime_api()
			.has_api::<dyn AuraApi<Block, AuraId>>(*block_import.header.parent_hash())
			.unwrap_or(false)
		{
			self.aura_verifier.get_mut().verify(block_import).await
		} else {
			self.relay_chain_verifier.verify(block_import).await
		}
	}
}

/// Build the import queue for parachain runtimes that started with relay chain consensus and
/// switched to aura.
pub fn build_relay_to_aura_import_queue<RuntimeApi, AuraId: AppCrypto>(
	client: Arc<ParachainClient<RuntimeApi>>,
	block_import: ParachainBlockImport<RuntimeApi>,
	config: &Configuration,
	telemetry_handle: Option<TelemetryHandle>,
	task_manager: &TaskManager,
) -> Result<sc_consensus::DefaultImportQueue<Block>, sc_service::Error>
where
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<Block>
		+ sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ sp_consensus_aura::AuraApi<Block, <<AuraId as AppCrypto>::Pair as Pair>::Public>,
	<<AuraId as AppCrypto>::Pair as Pair>::Signature:
		TryFrom<Vec<u8>> + std::hash::Hash + sp_runtime::traits::Member + Codec,
{
	let verifier_client = client.clone();

	let aura_verifier = move || {
		Box::new(cumulus_client_consensus_aura::build_verifier::<
			<AuraId as AppCrypto>::Pair,
			_,
			_,
			_,
		>(cumulus_client_consensus_aura::BuildVerifierParams {
			client: verifier_client.clone(),
			create_inherent_data_providers: move |parent_hash, _| {
				let cidp_client = verifier_client.clone();
				async move {
					let slot_duration = cumulus_client_consensus_aura::slot_duration_at(
						&*cidp_client,
						parent_hash,
					)?;
					let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

					let slot =
                        sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
                            *timestamp,
                            slot_duration,
                        );

					Ok((slot, timestamp))
				}
			},
			telemetry: telemetry_handle,
		})) as Box<_>
	};

	let relay_chain_verifier =
		Box::new(RelayChainVerifier::new(client.clone(), |_, _| async { Ok(()) })) as Box<_>;

	let verifier = Verifier {
		client,
		relay_chain_verifier,
		aura_verifier: BuildOnAccess::Uninitialized(Some(Box::new(aura_verifier))),
		_phantom: PhantomData,
	};

	let registry = config.prometheus_registry();
	let spawner = task_manager.spawn_essential_handle();

	Ok(BasicQueue::new(verifier, Box::new(block_import), None, &spawner, registry))
}
