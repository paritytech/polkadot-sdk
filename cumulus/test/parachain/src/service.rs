//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

use std::sync::Arc;
use std::time::Duration;
use substrate_client::LongestChain;
use futures::prelude::*;
use parachain_runtime::{self, GenesisConfig, opaque::Block, RuntimeApi};
use substrate_service::{error::{Error as ServiceError}, AbstractService, Configuration, ServiceBuilder};
use transaction_pool::{self, txpool::{Pool as TransactionPool}};
use inherents::InherentDataProviders;
use network::construct_simple_protocol;
use substrate_executor::native_executor_instance;
pub use substrate_executor::NativeExecutor;

// Our native executor instance.
native_executor_instance!(
	pub Executor,
	parachain_runtime::api::dispatch,
	parachain_runtime::native_version,
);

construct_simple_protocol! {
	/// Demo protocol attachment for substrate.
	pub struct NodeProtocol where Block = Block { }
}

/// Starts a `ServiceBuilder` for a full service.
///
/// Use this macro if you don't actually need the full service, but just the builder in order to
/// be able to perform chain operations.
macro_rules! new_full_start {
	($config:expr) => {{
		let inherent_data_providers = inherents::InherentDataProviders::new();

		let builder = substrate_service::ServiceBuilder::new_full::<
			parachain_runtime::opaque::Block, parachain_runtime::RuntimeApi, crate::service::Executor,
		>($config)?
			.with_select_chain(|_config, backend| {
				Ok(substrate_client::LongestChain::new(backend.clone()))
			})?
			.with_transaction_pool(|config, client|
				Ok(transaction_pool::txpool::Pool::new(config, transaction_pool::FullChainApi::new(client)))
			)?
			.with_import_queue(|_config, client, _, _| {
				let import_queue = cumulus_consensus::import_queue::import_queue(
					client.clone(),
					client,
					inherent_data_providers.clone(),
				)?;

				Ok(import_queue)
			})?;

		(builder, inherent_data_providers)
	}}
}

/// Builds a new service for a full client.
pub fn new_full<C: Send + Default + 'static>(config: Configuration<C, GenesisConfig>)
	-> Result<impl AbstractService, ServiceError>
{
	let is_authority = config.roles.is_authority();
	let name = config.name.clone();
	let disable_grandpa = config.disable_grandpa;
	let force_authoring = config.force_authoring;

	let (builder, inherent_data_providers) = new_full_start!(config);

	let service = builder.with_network_protocol(|_| Ok(NodeProtocol::new()))?.build()?;

	Ok(service)
}

