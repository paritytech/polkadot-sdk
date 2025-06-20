// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

use crate::{
	cli::AuthoringPolicy,
	common::{
		aura::{AuraIdT, AuraRuntimeApi},
		rpc::BuildParachainRpcExtensions,
		spec::{
			BaseNodeSpec, BuildImportQueue, ClientBlockImport, InitBlockImport, NodeSpec,
			StartConsensus,
		},
		types::{
			AccountId, Balance, Hash, Nonce, ParachainBackend, ParachainBlockImport,
			ParachainClient,
		},
		ConstructNodeRuntimeApi, NodeBlock, NodeExtraArgs,
	},
	nodes::DynNodeSpecExt,
};
use cumulus_client_collator::service::{
	CollatorService, ServiceInterface as CollatorServiceInterface,
};
#[docify::export(slot_based_colator_import)]
use cumulus_client_consensus_aura::collators::slot_based::{
	self as slot_based, Params as SlotBasedParams,
};
use cumulus_client_consensus_aura::{
	collators::{
		lookahead::{self as aura, Params as AuraParams},
		slot_based::{SlotBasedBlockImport, SlotBasedBlockImportHandle},
	},
	equivocation_import_queue::Verifier as EquivocationVerifier,
};
use cumulus_client_consensus_proposer::{Proposer, ProposerInterface};
use cumulus_client_consensus_relay_chain::Verifier as RelayChainVerifier;
#[allow(deprecated)]
use cumulus_client_service::CollatorSybilResistance;
use cumulus_primitives_core::{relay_chain::ValidationCode, ParaId};
use cumulus_relay_chain_interface::{OverseerHandle, RelayChainInterface};
use futures::prelude::*;
use polkadot_primitives::CollatorPair;
use prometheus_endpoint::Registry;
use sc_client_api::BlockchainEvents;
use sc_client_db::DbHash;
use sc_consensus::{
	import_queue::{BasicQueue, Verifier as VerifierT},
	BlockCheckParams, BlockImport, BlockImportParams, DefaultImportQueue, ImportResult,
};
use sc_consensus_aura::find_pre_digest;
use sc_service::{Configuration, Error, TaskManager};
use sc_telemetry::TelemetryHandle;
use sc_transaction_pool::TransactionPoolHandle;
use schnellru::{ByLength, LruMap};
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_consensus::BlockOrigin;
use sp_consensus_aura::{inherents::AuraCreateInherentDataProviders, AuraApi, Slot};
use sp_core::{traits::SpawnNamed, Pair};
use sp_inherents::{CreateInherentDataProviders, InherentDataProvider};
use sp_keystore::KeystorePtr;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, NumberFor};
use std::{
	marker::PhantomData,
	sync::{Arc, Mutex},
	time::Duration,
};

struct Verifier<Block, Client, AuraId> {
	client: Arc<Client>,
	aura_verifier: Box<dyn VerifierT<Block>>,
	relay_chain_verifier: Box<dyn VerifierT<Block>>,
	_phantom: PhantomData<AuraId>,
}

#[async_trait::async_trait]
impl<Block: BlockT, Client, AuraId> VerifierT<Block> for Verifier<Block, Client, AuraId>
where
	Client: ProvideRuntimeApi<Block> + Send + Sync,
	Client::Api: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
{
	async fn verify(
		&self,
		block_import: BlockImportParams<Block>,
	) -> Result<BlockImportParams<Block>, String> {
		if self.client.runtime_api().has_aura_api(*block_import.header.parent_hash()) {
			self.aura_verifier.verify(block_import).await
		} else {
			self.relay_chain_verifier.verify(block_import).await
		}
	}
}

/// Build the import queue for parachain runtimes that started with relay chain consensus and
/// switched to aura.
pub(crate) struct BuildRelayToAuraImportQueue<Block, RuntimeApi, AuraId, BlockImport>(
	PhantomData<(Block, RuntimeApi, AuraId, BlockImport)>,
);

impl<Block: BlockT, RuntimeApi, AuraId, BlockImport>
	BuildImportQueue<Block, RuntimeApi, BlockImport>
	for BuildRelayToAuraImportQueue<Block, RuntimeApi, AuraId, BlockImport>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
	BlockImport:
		sc_consensus::BlockImport<Block, Error = sp_consensus::Error> + Send + Sync + 'static,
{
	fn build_import_queue(
		client: Arc<ParachainClient<Block, RuntimeApi>>,
		block_import: ParachainBlockImport<Block, BlockImport>,
		config: &Configuration,
		telemetry_handle: Option<TelemetryHandle>,
		task_manager: &TaskManager,
	) -> sc_service::error::Result<DefaultImportQueue<Block>> {
		let create_inherent_data_providers =
			move |_, _| async move { Ok(sp_timestamp::InherentDataProvider::from_system_time()) };
		let registry = config.prometheus_registry();
		let spawner = task_manager.spawn_essential_handle();

		let relay_chain_verifier =
			Box::new(RelayChainVerifier::new(client.clone(), |_, _| async { Ok(()) }));

		let equivocation_aura_verifier = EquivocationVerifier::<AuraId::BoundedPair, _, _>::new(
			client.clone(),
			telemetry_handle,
		);

		let verifier = Verifier {
			client: client.clone(),
			aura_verifier: Box::new(equivocation_aura_verifier),
			relay_chain_verifier,
			_phantom: Default::default(),
		};

		let block_import =
			AuraBlockImport::new(block_import, client, create_inherent_data_providers);

		Ok(BasicQueue::new(verifier, Box::new(block_import), None, &spawner, registry))
	}
}

struct AuraBlockImport<Block: BlockT, BI, Client, CIDP, AuraId> {
	inner: BI,
	client: Arc<Client>,
	create_inherent_data_providers: CIDP,
	defender: Mutex<NaiveEquivocationDefender<NumberFor<Block>>>,
	_phantom: PhantomData<AuraId>,
}

impl<Block: BlockT, BI, Client, CIDP, AuraId> AuraBlockImport<Block, BI, Client, CIDP, AuraId> {
	fn new(inner: BI, client: Arc<Client>, create_inherent_data_providers: CIDP) -> Self {
		Self {
			inner,
			client,
			create_inherent_data_providers,
			defender: Default::default(),
			_phantom: Default::default(),
		}
	}
}

#[async_trait::async_trait]
impl<Block: BlockT, BI, Client, CIDP, AuraId> BlockImport<Block>
	for AuraBlockImport<Block, BI, Client, CIDP, AuraId>
where
	Client: ProvideRuntimeApi<Block> + Send + Sync,
	Client::Api: AuraRuntimeApi<Block, AuraId>,
	<Client as ProvideRuntimeApi<Block>>::Api: BlockBuilderApi<Block>,
	AuraId: AuraIdT + Sync,
	BI: sc_consensus::BlockImport<Block, Error = sp_consensus::Error> + Send + Sync,
	CIDP: CreateInherentDataProviders<Block, ()> + Sync,
	AuraId::BoundedPair: Pair,
	<AuraId::BoundedPair as Pair>::Signature: codec::Codec,
{
	type Error = sp_consensus::Error;

	async fn check_block(
		&self,
		block: BlockCheckParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).await
	}

	async fn import_block(
		&self,
		mut block_params: BlockImportParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		// Check inherents.
		if let Some(inner_body) = block_params.body.take() {
			let inherent_data_providers = self
				.create_inherent_data_providers
				.create_inherent_data_providers(*block_params.header.parent_hash(), ())
				.await
				.map_err(sp_consensus::Error::Other)?;

			let inherent_data = inherent_data_providers
				.create_inherent_data()
				.await
				.map_err(|e| sp_consensus::Error::Other(e.into()))?;

			let block = Block::new(block_params.header.clone(), inner_body);

			let inherent_res = self
				.client
				.runtime_api()
				.check_inherents(*block.header().parent_hash(), block.clone(), inherent_data)
				.map_err(|e| sp_consensus::Error::Other(Box::new(e)))?;

			if !inherent_res.ok() {
				for (i, e) in inherent_res.into_errors() {
					match inherent_data_providers.try_handle_error(&i, &e).await {
						Some(res) => res.map_err(|e| sp_consensus::Error::InvalidInherents(e))?,
						None => return Err(sp_consensus::Error::InvalidInherentsUnhandled(i)),
					}
				}
			}

			let (_, inner_body) = block.deconstruct();
			block_params.body = Some(inner_body);
		}

		if self.client.runtime_api().has_aura_api(*block_params.header.parent_hash()) {
			let slot = find_pre_digest::<Block, <AuraId::BoundedPair as Pair>::Signature>(
				&block_params.header,
			)
			.map_err(|e| sp_consensus::Error::Other(e.into()))?;

			// We need some kind of identifier for the relay parent, in the worst case we
			// take the all `0` hash.
			let relay_parent =
				cumulus_primitives_core::rpsr_digest::extract_relay_parent_storage_root(
					block_params.header.digest(),
				)
				.map(|r| r.0)
				.unwrap_or_else(|| {
					cumulus_primitives_core::extract_relay_parent(block_params.header.digest())
						.unwrap_or_default()
				});

			// Check for and reject egregious amounts of equivocations.
			//
			// If the `origin` is `ConsensusBroadcast`, we ignore the result of the
			// equivocation check. This `origin` is for example used by pov-recovery.
			if self.defender.lock().unwrap().insert_and_check(
				slot,
				*block_params.header.number(),
				relay_parent,
			) && !matches!(block_params.origin, BlockOrigin::ConsensusBroadcast)
			{
				return Err(sp_consensus::Error::Other(
					format!("Rejecting block {slot:?} due to excessive equivocations at slot",)
						.as_str()
						.into(),
				))
			}
		}

		self.inner.import_block(block_params).await
	}
}

const LRU_WINDOW: u32 = 512;
const EQUIVOCATION_LIMIT: usize = 16;

struct NaiveEquivocationDefender<N> {
	/// We distinguish blocks by `(Slot, BlockNumber, RelayParent)`.
	cache: LruMap<(u64, N, polkadot_primitives::Hash), usize>,
}

impl<N: std::hash::Hash + PartialEq> Default for NaiveEquivocationDefender<N> {
	fn default() -> Self {
		NaiveEquivocationDefender { cache: LruMap::new(ByLength::new(LRU_WINDOW)) }
	}
}

impl<N: std::hash::Hash + PartialEq> NaiveEquivocationDefender<N> {
	// Returns `true` if equivocation is beyond the limit.
	fn insert_and_check(
		&mut self,
		slot: Slot,
		block_number: N,
		relay_chain_parent: polkadot_primitives::Hash,
	) -> bool {
		let val = self
			.cache
			.get_or_insert((*slot, block_number, relay_chain_parent), || 0)
			.expect("insertion with ByLength limiter always succeeds; qed");

		if *val == EQUIVOCATION_LIMIT {
			true
		} else {
			*val += 1;
			false
		}
	}
}

/// Uses the lookahead collator to support async backing.
///
/// Start an aura powered parachain node. Some system chains use this.
pub(crate) struct AuraNode<Block, RuntimeApi, AuraId, StartConsensus, InitBlockImport>(
	pub PhantomData<(Block, RuntimeApi, AuraId, StartConsensus, InitBlockImport)>,
);

impl<Block, RuntimeApi, AuraId, StartConsensus, InitBlockImport> Default
	for AuraNode<Block, RuntimeApi, AuraId, StartConsensus, InitBlockImport>
{
	fn default() -> Self {
		Self(Default::default())
	}
}

impl<Block, RuntimeApi, AuraId, StartConsensus, InitBlockImport> BaseNodeSpec
	for AuraNode<Block, RuntimeApi, AuraId, StartConsensus, InitBlockImport>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>
		+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
	AuraId: AuraIdT + Sync,
	InitBlockImport: self::InitBlockImport<Block, RuntimeApi> + Send,
	InitBlockImport::BlockImport:
		sc_consensus::BlockImport<Block, Error = sp_consensus::Error> + 'static,
{
	type Block = Block;
	type RuntimeApi = RuntimeApi;
	type BuildImportQueue =
		BuildRelayToAuraImportQueue<Block, RuntimeApi, AuraId, InitBlockImport::BlockImport>;
	type InitBlockImport = InitBlockImport;
}

impl<Block, RuntimeApi, AuraId, StartConsensus, InitBlockImport> NodeSpec
	for AuraNode<Block, RuntimeApi, AuraId, StartConsensus, InitBlockImport>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>
		+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
	AuraId: AuraIdT + Sync,
	StartConsensus: self::StartConsensus<
			Block,
			RuntimeApi,
			InitBlockImport::BlockImport,
			InitBlockImport::BlockImportAuxiliaryData,
		> + 'static,
	InitBlockImport: self::InitBlockImport<Block, RuntimeApi> + Send,
	InitBlockImport::BlockImport:
		sc_consensus::BlockImport<Block, Error = sp_consensus::Error> + 'static,
{
	type BuildRpcExtensions = BuildParachainRpcExtensions<Block, RuntimeApi>;
	type StartConsensus = StartConsensus;
	const SYBIL_RESISTANCE: CollatorSybilResistance = CollatorSybilResistance::Resistant;
}

pub fn new_aura_node_spec<Block, RuntimeApi, AuraId>(
	extra_args: &NodeExtraArgs,
) -> Box<dyn DynNodeSpecExt>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>
		+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
	AuraId: AuraIdT + Sync,
	AuraId::BoundedPair: Send + Sync,
{
	if extra_args.authoring_policy == AuthoringPolicy::SlotBased {
		Box::new(AuraNode::<
			Block,
			RuntimeApi,
			AuraId,
			StartSlotBasedAuraConsensus<Block, RuntimeApi, AuraId>,
			StartSlotBasedAuraConsensus<Block, RuntimeApi, AuraId>,
		>::default())
	} else {
		Box::new(AuraNode::<
			Block,
			RuntimeApi,
			AuraId,
			StartLookaheadAuraConsensus<Block, RuntimeApi, AuraId>,
			ClientBlockImport,
		>::default())
	}
}

/// Start consensus using the lookahead aura collator.
pub(crate) struct StartSlotBasedAuraConsensus<Block, RuntimeApi, AuraId>(
	PhantomData<(Block, RuntimeApi, AuraId)>,
);

impl<Block: BlockT<Hash = DbHash>, RuntimeApi, AuraId>
	StartSlotBasedAuraConsensus<Block, RuntimeApi, AuraId>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
	AuraId::BoundedPair: Send + Sync,
{
	#[docify::export_content]
	fn launch_slot_based_collator<CIDP, CHP, Proposer, CS, Spawner>(
		params_with_export: SlotBasedParams<
			Block,
			ParachainBlockImport<
				Block,
				SlotBasedBlockImport<
					Block,
					Arc<ParachainClient<Block, RuntimeApi>>,
					ParachainClient<Block, RuntimeApi>,
					AuraCreateInherentDataProviders<Block>,
					AuraId::BoundedPair,
					NumberFor<Block>,
				>,
			>,
			CIDP,
			ParachainClient<Block, RuntimeApi>,
			ParachainBackend<Block>,
			Arc<dyn RelayChainInterface>,
			CHP,
			Proposer,
			CS,
			Spawner,
		>,
	) where
		CIDP: CreateInherentDataProviders<Block, ()> + 'static,
		CIDP::InherentDataProviders: Send,
		CHP: cumulus_client_consensus_common::ValidationCodeHashProvider<Hash> + Send + 'static,
		Proposer: ProposerInterface<Block> + Send + Sync + 'static,
		CS: CollatorServiceInterface<Block> + Send + Sync + Clone + 'static,
		Spawner: SpawnNamed,
	{
		slot_based::run::<Block, AuraId::BoundedPair, _, _, _, _, _, _, _, _, _>(
			params_with_export,
		);
	}
}

impl<Block: BlockT<Hash = DbHash>, RuntimeApi, AuraId>
	StartConsensus<
		Block,
		RuntimeApi,
		SlotBasedBlockImport<
			Block,
			Arc<ParachainClient<Block, RuntimeApi>>,
			ParachainClient<Block, RuntimeApi>,
			AuraCreateInherentDataProviders<Block>,
			AuraId::BoundedPair,
			NumberFor<Block>,
		>,
		SlotBasedBlockImportHandle<Block>,
	> for StartSlotBasedAuraConsensus<Block, RuntimeApi, AuraId>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
	AuraId::BoundedPair: Send + Sync,
{
	fn start_consensus(
		client: Arc<ParachainClient<Block, RuntimeApi>>,
		block_import: ParachainBlockImport<
			Block,
			SlotBasedBlockImport<
				Block,
				Arc<ParachainClient<Block, RuntimeApi>>,
				ParachainClient<Block, RuntimeApi>,
				AuraCreateInherentDataProviders<Block>,
				AuraId::BoundedPair,
				NumberFor<Block>,
			>,
		>,
		prometheus_registry: Option<&Registry>,
		telemetry: Option<TelemetryHandle>,
		task_manager: &TaskManager,
		relay_chain_interface: Arc<dyn RelayChainInterface>,
		transaction_pool: Arc<TransactionPoolHandle<Block, ParachainClient<Block, RuntimeApi>>>,
		keystore: KeystorePtr,
		relay_chain_slot_duration: Duration,
		para_id: ParaId,
		collator_key: CollatorPair,
		_overseer_handle: OverseerHandle,
		announce_block: Arc<dyn Fn(Hash, Option<Vec<u8>>) + Send + Sync>,
		backend: Arc<ParachainBackend<Block>>,
		node_extra_args: NodeExtraArgs,
		block_import_handle: SlotBasedBlockImportHandle<Block>,
	) -> Result<(), Error> {
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

		let client_for_aura = client.clone();
		let client_for_closure = client.clone();
		let params = SlotBasedParams {
			create_inherent_data_providers: Arc::new(move |parent, _| {
				let slot_duration = sc_consensus_aura::standalone::slot_duration_at(
					client_for_closure.as_ref(),
					parent,
				);
				let timestamp = sp_timestamp::InherentDataProvider::from_system_time();
				let slot = slot_duration.map(|slot_duration| {
					sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
						*timestamp,
						slot_duration,
					)
				});
				async move { Ok((slot?, timestamp)) }
			}) as AuraCreateInherentDataProviders<Block>,
			block_import,
			para_client: client.clone(),
			para_backend: backend.clone(),
			relay_client: relay_chain_interface,
			relay_chain_slot_duration,
			code_hash_provider: move |block_hash| {
				client_for_aura.code_at(block_hash).ok().map(|c| ValidationCode::from(c).hash())
			},
			keystore,
			collator_key,
			para_id,
			proposer,
			collator_service,
			authoring_duration: Duration::from_millis(2000),
			reinitialize: false,
			slot_offset: Duration::from_secs(1),
			block_import_handle,
			spawner: task_manager.spawn_handle(),
			export_pov: node_extra_args.export_pov,
			max_pov_percentage: node_extra_args.max_pov_percentage,
		};

		// We have a separate function only to be able to use `docify::export` on this piece of
		// code.

		Self::launch_slot_based_collator(params);

		Ok(())
	}
}

impl<Block: BlockT<Hash = DbHash>, RuntimeApi, AuraId> InitBlockImport<Block, RuntimeApi>
	for StartSlotBasedAuraConsensus<Block, RuntimeApi, AuraId>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	RuntimeApi::BoundedRuntimeApi: AuraApi<Block, <AuraId::BoundedPair as Pair>::Public>,
	AuraId: AuraIdT + Sync,
	AuraId::BoundedPair: Send + Sync,
{
	type BlockImport = SlotBasedBlockImport<
		Block,
		Arc<ParachainClient<Block, RuntimeApi>>,
		ParachainClient<Block, RuntimeApi>,
		AuraCreateInherentDataProviders<Block>,
		AuraId::BoundedPair,
		NumberFor<Block>,
	>;
	type BlockImportAuxiliaryData = SlotBasedBlockImportHandle<Block>;

	fn init_block_import(
		client: Arc<ParachainClient<Block, RuntimeApi>>,
	) -> sc_service::error::Result<(Self::BlockImport, Self::BlockImportAuxiliaryData)> {
		Ok(SlotBasedBlockImport::new(
			client.clone(),
			client.clone(),
			Arc::new(move |parent, _| {
				let slot_duration =
					sc_consensus_aura::standalone::slot_duration_at(client.as_ref(), parent);
				let timestamp = sp_timestamp::InherentDataProvider::from_system_time();
				let slot = slot_duration.map(|slot_duration| {
					sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
						*timestamp,
						slot_duration,
					)
				});
				async move { Ok((slot?, timestamp)) }
			}),
			true,
			Default::default(),
		))
	}
}

/// Wait for the Aura runtime API to appear on chain.
/// This is useful for chains that started out without Aura. Components that
/// are depending on Aura functionality will wait until Aura appears in the runtime.
async fn wait_for_aura<Block: BlockT, RuntimeApi, AuraId>(
	client: Arc<ParachainClient<Block, RuntimeApi>>,
) where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
{
	let finalized_hash = client.chain_info().finalized_hash;
	if client.runtime_api().has_aura_api(finalized_hash) {
		return;
	};

	let mut stream = client.finality_notification_stream();
	while let Some(notification) = stream.next().await {
		if client.runtime_api().has_aura_api(notification.hash) {
			return;
		}
	}
}

/// Start consensus using the lookahead aura collator.
pub(crate) struct StartLookaheadAuraConsensus<Block, RuntimeApi, AuraId>(
	PhantomData<(Block, RuntimeApi, AuraId)>,
);

impl<Block: BlockT<Hash = DbHash>, RuntimeApi, AuraId>
	StartConsensus<Block, RuntimeApi, Arc<ParachainClient<Block, RuntimeApi>>, ()>
	for StartLookaheadAuraConsensus<Block, RuntimeApi, AuraId>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
	AuraId::BoundedPair: Send + Sync + 'static,
{
	fn start_consensus(
		client: Arc<ParachainClient<Block, RuntimeApi>>,
		block_import: ParachainBlockImport<Block, Arc<ParachainClient<Block, RuntimeApi>>>,
		prometheus_registry: Option<&Registry>,
		telemetry: Option<TelemetryHandle>,
		task_manager: &TaskManager,
		relay_chain_interface: Arc<dyn RelayChainInterface>,
		transaction_pool: Arc<TransactionPoolHandle<Block, ParachainClient<Block, RuntimeApi>>>,
		keystore: KeystorePtr,
		relay_chain_slot_duration: Duration,
		para_id: ParaId,
		collator_key: CollatorPair,
		overseer_handle: OverseerHandle,
		announce_block: Arc<dyn Fn(Hash, Option<Vec<u8>>) + Send + Sync>,
		backend: Arc<ParachainBackend<Block>>,
		node_extra_args: NodeExtraArgs,
		_: (),
	) -> Result<(), Error> {
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

		let client_for_closure = client.clone();
		let params = aura::ParamsWithExport {
			export_pov: node_extra_args.export_pov,
			params: AuraParams {
				create_inherent_data_providers: Arc::new(move |parent, _| {
					let slot_duration = sc_consensus_aura::standalone::slot_duration_at(
						client_for_closure.as_ref(),
						parent,
					);
					let timestamp = sp_timestamp::InherentDataProvider::from_system_time();
					let slot = slot_duration.map(|slot_duration| {
						sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
							*timestamp,
							slot_duration,
						)
					});
					async move { Ok((slot?, timestamp)) }
				}) as AuraCreateInherentDataProviders<Block>,
				block_import,
				para_client: client.clone(),
				para_backend: backend,
				relay_client: relay_chain_interface,
				code_hash_provider: {
					let client = client.clone();
					move |block_hash| {
						client.code_at(block_hash).ok().map(|c| ValidationCode::from(c).hash())
					}
				},
				keystore,
				collator_key,
				para_id,
				overseer_handle,
				relay_chain_slot_duration,
				proposer: Proposer::new(proposer_factory),
				collator_service,
				authoring_duration: Duration::from_millis(2000),
				reinitialize: false,
				max_pov_percentage: node_extra_args.max_pov_percentage,
			},
		};

		let fut = async move {
			wait_for_aura(client).await;
			aura::run_with_export::<Block, AuraId::BoundedPair, _, _, _, _, _, _, _, _>(params)
				.await;
		};
		task_manager.spawn_essential_handle().spawn("aura", None, fut);

		Ok(())
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use codec::Encode;
	use cumulus_test_client::{
		seal_block, InitBlockBuilder, TestClientBuilder, TestClientBuilderExt,
	};
	use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
	use cumulus_test_runtime::AuraId;
	use futures::FutureExt;
	use polkadot_primitives::{HeadData, PersistedValidationData};
	use sc_client_api::HeaderBackend;
	use std::{collections::HashSet, sync::Arc};

	struct TestBlockImport;

	#[async_trait::async_trait]
	impl<Block: BlockT> BlockImport<Block> for TestBlockImport {
		type Error = sp_consensus::Error;

		async fn check_block(
			&self,
			_: BlockCheckParams<Block>,
		) -> Result<ImportResult, Self::Error> {
			Ok(ImportResult::Imported(Default::default()))
		}

		async fn import_block(
			&self,
			_: BlockImportParams<Block>,
		) -> Result<ImportResult, Self::Error> {
			Ok(ImportResult::Imported(Default::default()))
		}
	}

	#[test]
	fn import_equivocated_blocks_from_recovery() {
		let client = Arc::new(TestClientBuilder::default().build());

		let block_import: AuraBlockImport<_, _, _, _, AuraId> =
			AuraBlockImport::new(TestBlockImport, client.clone(), |_, _| async move {
				Ok(sp_timestamp::InherentDataProvider::from_system_time())
			});

		let genesis = client.info().best_hash;
		let mut sproof = RelayStateSproofBuilder::default();
		sproof.included_para_head = Some(HeadData(client.header(genesis).unwrap().encode()));
		sproof.para_id = cumulus_test_client::runtime::PARACHAIN_ID.into();

		let validation_data = PersistedValidationData {
			relay_parent_number: 1,
			parent_head: client.header(genesis).unwrap().encode().into(),
			..Default::default()
		};

		let block_builder = client.init_block_builder(Some(validation_data), sproof);
		let block = block_builder.block_builder.build().unwrap();

		let mut blocks = Vec::new();
		for _ in 0..EQUIVOCATION_LIMIT + 1 {
			blocks.push(seal_block(block.block.clone(), &client))
		}

		// sr25519 should generate a different signature every time you sign something and thus, all
		// blocks get a different hash (even if they are the same block).
		assert_eq!(blocks.iter().map(|b| b.hash()).collect::<HashSet<_>>().len(), blocks.len());

		blocks.iter().take(EQUIVOCATION_LIMIT).for_each(|block| {
			let mut params =
				BlockImportParams::new(BlockOrigin::NetworkBroadcast, block.header().clone());
			params.body = Some(block.extrinsics().to_vec());
			block_import.import_block(params).now_or_never().unwrap().unwrap();
		});

		// Now let's try some previously verified block and a block we have not verified yet.
		//
		// Verify should fail, because we are above the limit. However, when we change the origin to
		// `ConsensusBroadcast`, it should work.
		let extra_blocks =
			vec![blocks[EQUIVOCATION_LIMIT / 2].clone(), blocks.last().unwrap().clone()];

		extra_blocks.into_iter().for_each(|block| {
			let mut params =
				BlockImportParams::new(BlockOrigin::NetworkBroadcast, block.header().clone());
			params.body = Some(block.extrinsics().to_vec());
			assert!(block_import
				.import_block(params)
				.now_or_never()
				.unwrap()
				.map(drop)
				.unwrap_err()
				.to_string()
				.contains("excessive equivocations at slot"));

			// When it comes from `pov-recovery`, we will accept it
			let mut params =
				BlockImportParams::new(BlockOrigin::ConsensusBroadcast, block.header().clone());
			params.body = Some(block.extrinsics().to_vec());
			assert!(block_import.import_block(params).now_or_never().unwrap().is_ok());
		});
	}
}
