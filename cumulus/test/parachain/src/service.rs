// Copyright 2019 Parity Technologies (UK) Ltd.
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

use ansi_term::Color;
use cumulus_collator::{prepare_collator_config, CollatorBuilder};
use cumulus_network::DelayedBlockAnnounceValidator;
use futures::{future::ready, FutureExt};
use polkadot_primitives::v0::CollatorPair;
use sc_executor::native_executor_instance;
pub use sc_executor::NativeExecutor;
use sc_informant::OutputFormat;
use sc_service::{Configuration, PartialComponents, TaskManager, TFullBackend, TFullClient, Role};
use std::sync::Arc;
use sp_core::crypto::Pair;
use sp_trie::PrefixedMemoryDB;
use sp_runtime::traits::BlakeTwo256;

// Our native executor instance.
native_executor_instance!(
	pub Executor,
	parachain_runtime::api::dispatch,
	parachain_runtime::native_version,
);

/// Starts a `ServiceBuilder` for a full service.
///
/// Use this macro if you don't actually need the full service, but just the builder in order to
/// be able to perform chain operations.
pub fn new_partial(config: &mut Configuration) -> Result<
	PartialComponents<
		TFullClient<parachain_runtime::opaque::Block, parachain_runtime::RuntimeApi, crate::service::Executor>,
		TFullBackend<parachain_runtime::opaque::Block>,
		(),
		sp_consensus::import_queue::BasicQueue<parachain_runtime::opaque::Block, PrefixedMemoryDB<BlakeTwo256>>,
		sc_transaction_pool::FullPool<parachain_runtime::opaque::Block, TFullClient<parachain_runtime::opaque::Block, parachain_runtime::RuntimeApi, crate::service::Executor>>,
		(),
	>,
	sc_service::Error,
>
{
	let inherent_data_providers = sp_inherents::InherentDataProviders::new();

	let (client, backend, keystore, task_manager) =
		sc_service::new_full_parts::<
			parachain_runtime::opaque::Block,
			parachain_runtime::RuntimeApi,
			crate::service::Executor,
		>(&config)?;
	let client = Arc::new(client);
	//let select_chain = sc_consensus::LongestChain::new(backend.clone());

	let registry = config.prometheus_registry();

	let transaction_pool = sc_transaction_pool::BasicPool::new_full(
		config.transaction_pool.clone(),
		//std::sync::Arc::new(pool_api),
		config.prometheus_registry(),
		task_manager.spawn_handle(),
		client.clone(),
	);

	let import_queue = cumulus_consensus::import_queue::import_queue(
		client.clone(),
		client.clone(),
		inherent_data_providers.clone(),
		&task_manager.spawn_handle(),
		registry.clone(),
	)?;

	let params = PartialComponents {
		backend,
		client,
		import_queue,
		keystore,
		task_manager,
		transaction_pool,
		inherent_data_providers,
		select_chain: (),
		other: (),
	};

	Ok(params)
}

/// Run a collator node with the given parachain `Configuration` and relaychain `Configuration`
///
/// This function blocks until done.
pub fn run_collator(
	parachain_config: Configuration,
	key: Arc<CollatorPair>,
	mut polkadot_config: polkadot_collator::Configuration,
	id: polkadot_primitives::v0::Id,
	validator: bool,
) -> sc_service::error::Result<(
	TaskManager,
	Arc<TFullClient<parachain_runtime::opaque::Block, parachain_runtime::RuntimeApi, crate::service::Executor>>,
)> {
	if matches!(parachain_config.role, Role::Light) {
		return Err("Light client not supported!".into());
	}
	if matches!(polkadot_config.role, Role::Light) {
		return Err("Light client not supported!".into());
	}

	let mut parachain_config = prepare_collator_config(parachain_config);

	parachain_config.informant_output_format = OutputFormat {
		enable_color: true,
		prefix: format!("[{}] ", Color::Yellow.bold().paint("Parachain")),
	};
	polkadot_config.informant_output_format = OutputFormat {
		enable_color: true,
		prefix: format!("[{}] ", Color::Blue.bold().paint("Relaychain")),
	};

	let params = new_partial(&mut parachain_config)?;
	params.inherent_data_providers
		.register_provider(sp_timestamp::InherentDataProvider)
		.unwrap();

	let client = params.client.clone();
	let backend = params.backend.clone();
	let block_announce_validator = DelayedBlockAnnounceValidator::new();
	let block_announce_validator_builder = {
		let block_announce_validator = block_announce_validator.clone();
		move |_| Box::new(block_announce_validator) as Box<_>
	};

	let prometheus_registry = parachain_config.prometheus_registry().cloned();
	let transaction_pool = params.transaction_pool.clone();
	let mut task_manager = params.task_manager;
	let import_queue = params.import_queue;
	let (network, network_status_sinks, system_rpc_tx) =
		sc_service::build_network(sc_service::BuildNetworkParams {
				config: &parachain_config,
				client: client.clone(),
				transaction_pool: transaction_pool.clone(),
				spawn_handle: task_manager.spawn_handle(),
				import_queue,
				on_demand: None,
				block_announce_validator_builder: Some(Box::new(block_announce_validator_builder)),
				finality_proof_request_builder: None,
				finality_proof_provider: None,
		})?;

	let _rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
			on_demand: None,
			remote_blockchain: None,
			rpc_extensions_builder: Box::new(|_| ()),
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			task_manager: &mut task_manager,
			telemetry_connection_sinks: Default::default(),
			config: parachain_config,
			keystore: params.keystore,
			backend,
			network: network.clone(),
			network_status_sinks,
			system_rpc_tx,
	})?;

	if validator {
		let proposer_factory = sc_basic_authorship::ProposerFactory::new(
			client.clone(),
			transaction_pool,
			prometheus_registry.as_ref(),
		);

		let block_import = client.clone();
		let announce_block = Arc::new(move |hash, data| network.announce_block(hash, data));
		let builder = CollatorBuilder::new(
			proposer_factory,
			params.inherent_data_providers,
			block_import,
			client.clone(),
			id,
			client.clone(),
			announce_block,
			block_announce_validator,
		);

		let (polkadot_future, polkadpt_task_manager) =
			polkadot_collator::start_collator(builder, id, key, polkadot_config)?;

		// Make sure the polkadot task manager survives as long as the service.
		let polkadot_future = polkadot_future.then(move |_| {
			let _ = polkadpt_task_manager;
			ready(())
		});

		task_manager
			.spawn_essential_handle()
			.spawn("polkadot", polkadot_future);
	} else {
		let is_light = matches!(polkadot_config.role, Role::Light);
		let builder = polkadot_service::NodeBuilder::new(polkadot_config);
		let mut polkadot_task_manager = if is_light {
			return Err("Light client not supported.".into());
		} else {
			builder.build_full(
				Some((key.public(), id)),
				None,
				false,
				6000,
				None,
			)
		}?;
		let polkadot_future = async move {
			polkadot_task_manager.future().await.expect("polkadot essential task failed");
		};

		task_manager
			.spawn_essential_handle()
			.spawn("polkadot", polkadot_future);
	}

	Ok((task_manager, client))
}
