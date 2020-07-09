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
use polkadot_primitives::parachain::CollatorPair;
use sc_executor::native_executor_instance;
pub use sc_executor::NativeExecutor;
use sc_finality_grandpa::{
	FinalityProofProvider as GrandpaFinalityProofProvider, StorageAndProofProvider,
};
use sc_informant::OutputFormat;
use sc_service::{Configuration, TaskManager};
use std::sync::Arc;

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
macro_rules! new_full_start {
	($config:expr) => {{
		let inherent_data_providers = sp_inherents::InherentDataProviders::new();

		let builder = sc_service::ServiceBuilder::new_full::<
			parachain_runtime::opaque::Block,
			parachain_runtime::RuntimeApi,
			crate::service::Executor,
		>($config)?
		.with_select_chain(|_config, backend| Ok(sc_consensus::LongestChain::new(backend.clone())))?
		.with_transaction_pool(|builder| {
			let client = builder.client();
			let pool_api = Arc::new(sc_transaction_pool::FullChainApi::new(
				client.clone(),
				builder.prometheus_registry(),
			));
			let pool = sc_transaction_pool::BasicPool::new(
				builder.config().transaction_pool.clone(),
				pool_api,
				builder.prometheus_registry(),
			);
			Ok(pool)
		})?
		.with_import_queue(|_config, client, _, _, spawner, registry| {
			let import_queue = cumulus_consensus::import_queue::import_queue(
				client.clone(),
				client,
				inherent_data_providers.clone(),
				spawner,
				registry,
			)?;

			Ok(import_queue)
		})?;

		(builder, inherent_data_providers)
		}};
}

/// Run a collator node with the given parachain `Configuration` and relaychain `Configuration`
///
/// This function blocks until done.
pub fn run_collator(
	parachain_config: Configuration,
	key: Arc<CollatorPair>,
	mut polkadot_config: polkadot_collator::Configuration,
	id: polkadot_primitives::parachain::Id,
) -> sc_service::error::Result<TaskManager> {
	let mut parachain_config = prepare_collator_config(parachain_config);

	parachain_config.informant_output_format = OutputFormat {
		enable_color: true,
		prefix: format!("[{}] ", Color::Yellow.bold().paint("Parachain")),
	};

	let (builder, inherent_data_providers) = new_full_start!(parachain_config);
	inherent_data_providers
		.register_provider(sp_timestamp::InherentDataProvider)
		.unwrap();

	let block_announce_validator = DelayedBlockAnnounceValidator::new();
	let block_announce_validator_copy = block_announce_validator.clone();
	let service = builder
		.with_finality_proof_provider(|client, backend| {
			// GenesisAuthoritySetProvider is implemented for StorageAndProofProvider
			let provider = client as Arc<dyn StorageAndProofProvider<_, _>>;
			Ok(Arc::new(GrandpaFinalityProofProvider::new(backend, provider)) as _)
		})?
		.with_block_announce_validator(|_client| Box::new(block_announce_validator_copy))?
		.build_full()?;

	let registry = service.prometheus_registry.clone();

	let proposer_factory = sc_basic_authorship::ProposerFactory::new(
		service.client.clone(),
		service.transaction_pool.clone(),
		registry.as_ref(),
	);

	let block_import = service.client.clone();
	let client = service.client.clone();
	let network = service.network.clone();
	let announce_block = Arc::new(move |hash, data| network.announce_block(hash, data));
	let builder = CollatorBuilder::new(
		proposer_factory,
		inherent_data_providers,
		block_import,
		client.clone(),
		id,
		client,
		announce_block,
		block_announce_validator,
	);

	polkadot_config.informant_output_format = OutputFormat {
		enable_color: true,
		prefix: format!("[{}] ", Color::Blue.bold().paint("Relaychain")),
	};

	let (polkadot_future, task_manager) =
		polkadot_collator::start_collator(builder, id, key, polkadot_config)?;

	// Make sure the polkadot task manager survives as long as the service.
	let polkadot_future = polkadot_future.then(move |_| {
		let _ = task_manager;
		ready(())
	});

	service
		.task_manager
		.spawn_essential_handle()
		.spawn("polkadot", polkadot_future);

	Ok(service.task_manager)
}
