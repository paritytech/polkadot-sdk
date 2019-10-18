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

use std::sync::Arc;

use parachain_runtime::{self, GenesisConfig, opaque::Block};

use inherents::InherentDataProviders;
use substrate_service::{AbstractService, Configuration};
use network::construct_simple_protocol;
use substrate_executor::native_executor_instance;

use futures::prelude::*;

use futures03::FutureExt;

use log::error;

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

/// Run the collator with the given `config`.
pub fn run_collator<C: Send + Default + 'static, E: crate::cli::IntoExit + Send + 'static>(
	config: Configuration<C, GenesisConfig>,
	exit: E,
	key: Arc<polkadot_primitives::parachain::CollatorPair>,
	version: crate::cli::VersionInfo,
) -> crate::cli::Result<()> {
	let (builder, inherent_data_providers) = new_full_start!(config);
	inherent_data_providers.register_provider(srml_timestamp::InherentDataProvider).unwrap();

	let service = builder.with_network_protocol(|_| Ok(NodeProtocol::new()))?.build()?;
	let proposer_factory = basic_authorship::ProposerFactory {
		client: service.client(),
		transaction_pool: service.transaction_pool(),
	};

	let on_exit = service.on_exit();

	let setup_parachain = SetupParachain { service, inherent_data_providers, proposer_factory, exit };

	cumulus_collator::run_collator(setup_parachain, crate::PARA_ID, on_exit, key, version)
}

struct SetupParachain<S, PF, E> {
	service: S,
	proposer_factory: PF,
	exit: E,
	inherent_data_providers: InherentDataProviders,
}

impl<S, PF, E> cumulus_collator::SetupParachain<Block> for SetupParachain<S, PF, E>
	where
		S: AbstractService,
		E: Send + crate::cli::IntoExit,
		PF: consensus_common::Environment<Block> + Send + 'static,
		<PF::Proposer as consensus_common::Proposer<Block>>::Create: Send + Unpin,
		PF::Error: std::fmt::Debug,
{
	type ProposerFactory = PF;

	fn setup_parachain<P: cumulus_consensus::PolkadotClient>(
		self,
		polkadot_client: P,
		task_executor: polkadot_collator::TaskExecutor,
	) -> Result<(Self::ProposerFactory, InherentDataProviders), String> {
		let client = self.service.client();

		let follow = match cumulus_consensus::follow_polkadot(crate::PARA_ID, client, polkadot_client) {
			Ok(follow) => follow,
			Err(e) => {
				return Err(format!("Could not start following polkadot: {:?}", e));
			}
		};

		task_executor.execute(
			Box::new(
				self.service
					.map_err(|e| error!("Parachain service error: {:?}", e))
					.select(futures03::compat::Compat::new(follow.map(|_| Ok::<(), ()>(()))))
					.map(|_| ())
					.map_err(|_| ())
					.select(self.exit.into_exit())
					.map(|_| ())
					.map_err(|_| ())
			),
		).map_err(|_| "Could not spawn parachain server!")?;

		Ok((self.proposer_factory, self.inherent_data_providers))
	}
}
