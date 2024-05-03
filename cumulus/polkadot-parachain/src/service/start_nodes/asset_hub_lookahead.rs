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

use crate::service::{
	aura, build_parachain_rpc_extensions,
	build_relay_to_aura_import_queue, start_node_impl, AuraParams,
	ParachainClient,
	common_types::{AccountId, Balance, Block, Hash, Nonce}
};
use codec::{Codec, Decode};
use cumulus_client_cli::CollatorOptions;
use cumulus_client_collator::service::CollatorService;
use cumulus_client_consensus_proposer::Proposer;
use cumulus_client_service::CollatorSybilResistance;
use cumulus_primitives_core::{relay_chain::ValidationCode, BlockT, ParaId};
use futures::StreamExt;
use sc_network::NetworkBackend;
use sc_service::{Configuration, TaskManager};
use sp_api::{ApiExt, ConstructRuntimeApi, ProvideRuntimeApi};
use sp_consensus_aura::AuraApi;
use sp_core::{traits::SpawnEssentialNamed, Pair};
use sp_runtime::app_crypto::AppCrypto;
use std::{sync::Arc, time::Duration};

/// Start a shell node which should later transition into an Aura powered parachain node. Asset Hub
/// uses this because at genesis, Asset Hub was on the `shell` runtime which didn't have Aura and
/// needs to sync and upgrade before it can run `AuraApi` functions.
///
/// Uses the lookahead collator to support async backing.
#[sc_tracing::logging::prefix_logs_with("Parachain")]
pub async fn start_asset_hub_lookahead_node<
	RuntimeApi,
	AuraId: AppCrypto + Send + Codec + Sync,
	Net,
>(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	para_id: ParaId,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient<RuntimeApi>>)>
where
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<Block>
		+ sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ cumulus_primitives_core::CollectCollationInfo<Block>
		+ sp_consensus_aura::AuraApi<Block, <<AuraId as AppCrypto>::Pair as Pair>::Public>
		+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>
		+ cumulus_primitives_aura::AuraUnincludedSegmentApi<Block>,
	<<AuraId as AppCrypto>::Pair as Pair>::Signature:
		TryFrom<Vec<u8>> + std::hash::Hash + sp_runtime::traits::Member + Codec,
	Net: NetworkBackend<Block, Hash>,
{
	start_node_impl::<RuntimeApi, _, _, _, Net>(
		parachain_config,
		polkadot_config,
		collator_options,
		CollatorSybilResistance::Resistant, // Aura
		para_id,
		build_parachain_rpc_extensions::<RuntimeApi>,
		build_relay_to_aura_import_queue::<_, AuraId>,
		|client,
		 block_import,
		 prometheus_registry,
		 telemetry,
		 task_manager,
		 relay_chain_interface,
		 transaction_pool,
		 sync_oracle,
		 keystore,
		 relay_chain_slot_duration,
		 para_id,
		 collator_key,
		 overseer_handle,
		 announce_block,
		 backend| {
			let relay_chain_interface2 = relay_chain_interface.clone();

			let collator_service = CollatorService::new(
				client.clone(),
				Arc::new(task_manager.spawn_handle()),
				announce_block,
				client.clone(),
			);

			let spawner = task_manager.spawn_handle();

			let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
				spawner,
				client.clone(),
				transaction_pool,
				prometheus_registry,
				telemetry.clone(),
			);

			let collation_future = Box::pin(async move {
				// Start collating with the `shell` runtime while waiting for an upgrade to an Aura
				// compatible runtime.
				let mut request_stream = cumulus_client_collator::relay_chain_driven::init(
					collator_key.clone(),
					para_id,
					overseer_handle.clone(),
				)
				.await;
				while let Some(request) = request_stream.next().await {
					let pvd = request.persisted_validation_data().clone();
					let last_head_hash =
						match <Block as BlockT>::Header::decode(&mut &pvd.parent_head.0[..]) {
							Ok(header) => header.hash(),
							Err(e) => {
								log::error!("Could not decode the head data: {e}");
								request.complete(None);
								continue
							},
						};

					// Check if we have upgraded to an Aura compatible runtime and transition if
					// necessary.
					if client
						.runtime_api()
						.has_api::<dyn AuraApi<Block, AuraId>>(last_head_hash)
						.unwrap_or(false)
					{
						// Respond to this request before transitioning to Aura.
						request.complete(None);
						break
					}
				}

				// Move to Aura consensus.
				let proposer = Proposer::new(proposer_factory);

				let params = AuraParams {
					create_inherent_data_providers: move |_, ()| async move { Ok(()) },
					block_import,
					para_client: client.clone(),
					para_backend: backend,
					relay_client: relay_chain_interface2,
					code_hash_provider: move |block_hash| {
						client.code_at(block_hash).ok().map(|c| ValidationCode::from(c).hash())
					},
					sync_oracle,
					keystore,
					collator_key,
					para_id,
					overseer_handle,
					relay_chain_slot_duration,
					proposer,
					collator_service,
					authoring_duration: Duration::from_millis(1500),
					reinitialize: true, /* we need to always re-initialize for asset-hub moving
					                     * to aura */
				};

				aura::run::<Block, <AuraId as AppCrypto>::Pair, _, _, _, _, _, _, _, _, _>(params)
					.await
			});

			let spawner = task_manager.spawn_essential_handle();
			spawner.spawn_essential("cumulus-asset-hub-collator", None, collation_future);

			Ok(())
		},
		hwbench,
	)
	.await
}
