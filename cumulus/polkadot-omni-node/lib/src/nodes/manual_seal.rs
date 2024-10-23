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
	rpc::BuildRpcExtensions as BuildRpcExtensionsT,
	spec::{BaseNodeSpec, BuildImportQueue, NodeSpec as NodeSpecT},
	types::{Hash, ParachainBlockImport, ParachainClient},
};
use codec::Encode;
use cumulus_client_parachain_inherent::{MockValidationDataInherentDataProvider, MockXcmConfig};
use cumulus_primitives_core::ParaId;
use sc_consensus::{DefaultImportQueue, LongestChain};
use sc_consensus_manual_seal::rpc::{ManualSeal, ManualSealApiServer};
use sc_network::NetworkBackend;
use sc_service::{build_polkadot_syncing_strategy, Configuration, PartialComponents, TaskManager};
use sc_telemetry::TelemetryHandle;
use sp_runtime::traits::Header;
use std::{marker::PhantomData, sync::Arc};

pub struct ManualSealNode<NodeSpec>(PhantomData<NodeSpec>);

impl<NodeSpec: NodeSpecT> BuildImportQueue<NodeSpec::Block, NodeSpec::RuntimeApi>
	for ManualSealNode<NodeSpec>
{
	fn build_import_queue(
		client: Arc<ParachainClient<NodeSpec::Block, NodeSpec::RuntimeApi>>,
		_block_import: ParachainBlockImport<NodeSpec::Block, NodeSpec::RuntimeApi>,
		config: &Configuration,
		_telemetry_handle: Option<TelemetryHandle>,
		task_manager: &TaskManager,
	) -> sc_service::error::Result<DefaultImportQueue<NodeSpec::Block>> {
		Ok(sc_consensus_manual_seal::import_queue(
			Box::new(client.clone()),
			&task_manager.spawn_essential_handle(),
			config.prometheus_registry(),
		))
	}
}

impl<NodeSpec: NodeSpecT> BaseNodeSpec for ManualSealNode<NodeSpec> {
	type Block = NodeSpec::Block;
	type RuntimeApi = NodeSpec::RuntimeApi;
	type BuildImportQueue = Self;
}

impl<NodeSpec: NodeSpecT> ManualSealNode<NodeSpec> {
	pub fn new() -> Self {
		Self(Default::default())
	}

	pub fn start_node<Net>(
		&self,
		mut config: Configuration,
		para_id: ParaId,
		block_time: u64,
	) -> sc_service::error::Result<TaskManager>
	where
		Net: NetworkBackend<NodeSpec::Block, Hash>,
	{
		let PartialComponents {
			client,
			backend,
			mut task_manager,
			import_queue,
			keystore_container,
			select_chain: _,
			transaction_pool,
			other: (_, mut telemetry, _),
		} = Self::new_partial(&config)?;
		let select_chain = LongestChain::new(backend.clone());

		// Since this is a dev node, prevent it from connecting to peers.
		config.network.default_peers_set.in_peers = 0;
		config.network.default_peers_set.out_peers = 0;
		let mut net_config = sc_network::config::FullNetworkConfiguration::<_, _, Net>::new(
			&config.network,
			config.prometheus_config.as_ref().map(|cfg| cfg.registry.clone()),
		);
		let metrics = Net::register_notification_metrics(
			config.prometheus_config.as_ref().map(|cfg| &cfg.registry),
		);

		let syncing_strategy = build_polkadot_syncing_strategy(
			config.protocol_id(),
			config.chain_spec.fork_id(),
			&mut net_config,
			None,
			client.clone(),
			&task_manager.spawn_handle(),
			config.prometheus_config.as_ref().map(|config| &config.registry),
		)?;

		let (network, system_rpc_tx, tx_handler_controller, start_network, sync_service) =
			sc_service::build_network(sc_service::BuildNetworkParams {
				config: &config,
				client: client.clone(),
				transaction_pool: transaction_pool.clone(),
				spawn_handle: task_manager.spawn_handle(),
				import_queue,
				net_config,
				block_announce_validator_builder: None,
				syncing_strategy,
				block_relay: None,
				metrics,
			})?;

		let proposer = sc_basic_authorship::ProposerFactory::new(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool.clone(),
			None,
			None,
		);

		let (manual_seal_sink, manual_seal_stream) = futures::channel::mpsc::channel(1024);
		let mut manual_seal_sink_clone = manual_seal_sink.clone();
		task_manager
			.spawn_essential_handle()
			.spawn("block_authoring", None, async move {
				loop {
					futures_timer::Delay::new(std::time::Duration::from_millis(block_time)).await;
					manual_seal_sink_clone
						.try_send(sc_consensus_manual_seal::EngineCommand::SealNewBlock {
							create_empty: true,
							finalize: true,
							parent_hash: None,
							sender: None,
						})
						.unwrap();
				}
			});

		let client_for_cidp = client.clone();
		let params = sc_consensus_manual_seal::ManualSealParams {
			block_import: client.clone(),
			env: proposer,
			client: client.clone(),
			pool: transaction_pool.clone(),
			select_chain,
			commands_stream: Box::pin(manual_seal_stream),
			consensus_data_provider: None,
			create_inherent_data_providers: move |block: Hash, ()| {
				let current_para_head = client_for_cidp
					.header(block)
					.expect("Header lookup should succeed")
					.expect("Header passed in as parent should be present in backend.");
				let current_para_block_head =
					Some(polkadot_primitives::HeadData(current_para_head.encode()));
				let client_for_xcm = client_for_cidp.clone();
				async move {
					use sp_runtime::traits::UniqueSaturatedInto;

					let mocked_parachain = MockValidationDataInherentDataProvider {
						// When using manual seal we start from block 0, and it's very unlikely to
						// reach a block number > u32::MAX.
						current_para_block: UniqueSaturatedInto::<u32>::unique_saturated_into(
							*current_para_head.number(),
						),
						para_id,
						current_para_block_head,
						relay_offset: 1000,
						relay_blocks_per_para_block: 1,
						para_blocks_per_relay_epoch: 10,
						relay_randomness_config: (),
						xcm_config: MockXcmConfig::new(&*client_for_xcm, block, Default::default()),
						raw_downward_messages: vec![],
						raw_horizontal_messages: vec![],
						additional_key_values: None,
					};
					Ok((
						// This is intentional, as the runtime that we expect to run against this
						// will never receive the aura-related inherents/digests, and providing
						// real timestamps would cause aura <> timestamp checking to fail.
						sp_timestamp::InherentDataProvider::new(sp_timestamp::Timestamp::new(0)),
						mocked_parachain,
					))
				}
			},
		};
		let authorship_future = sc_consensus_manual_seal::run_manual_seal(params);
		task_manager.spawn_essential_handle().spawn_blocking(
			"manual-seal",
			None,
			authorship_future,
		);
		let rpc_extensions_builder = {
			let client = client.clone();
			let transaction_pool = transaction_pool.clone();
			let backend_for_rpc = backend.clone();

			Box::new(move |_| {
				let mut module = NodeSpec::BuildRpcExtensions::build_rpc_extensions(
					client.clone(),
					backend_for_rpc.clone(),
					transaction_pool.clone(),
				)?;
				module
					.merge(ManualSeal::new(manual_seal_sink.clone()).into_rpc())
					.map_err(|e| sc_service::Error::Application(e.into()))?;
				Ok(module)
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
		})?;

		start_network.start_network();
		Ok(task_manager)
	}
}
