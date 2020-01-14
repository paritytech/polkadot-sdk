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

use parachain_runtime::{self, opaque::Block, GenesisConfig};

use sc_executor::native_executor_instance;
use sc_network::construct_simple_protocol;
use sc_service::{AbstractService, Configuration};
use sp_consensus::{BlockImport, Environment, Proposer};
use sp_inherents::InherentDataProviders;

use futures::{compat::Future01CompatExt, future, task::Spawn, FutureExt, TryFutureExt};

use log::error;

pub use sc_executor::NativeExecutor;

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
		let inherent_data_providers = sp_inherents::InherentDataProviders::new();

		let builder = sc_service::ServiceBuilder::new_full::<
			parachain_runtime::opaque::Block,
			parachain_runtime::RuntimeApi,
			crate::service::Executor,
		>($config)?
		.with_select_chain(|_config, backend| Ok(sc_client::LongestChain::new(backend.clone())))?
		.with_transaction_pool(|config, client, _| {
			let pool_api = sc_transaction_pool::FullChainApi::new(client.clone());
			let pool = sc_transaction_pool::BasicPool::new(config, pool_api);
			let maintainer =
				sc_transaction_pool::FullBasicPoolMaintainer::new(pool.pool().clone(), client);
			let maintainable_pool =
				sp_transaction_pool::MaintainableTransactionPool::new(pool, maintainer);
			Ok(maintainable_pool)
		})?
		.with_import_queue(|_config, client, _, _| {
			let import_queue = cumulus_consensus::import_queue::import_queue(
				client.clone(),
				client,
				inherent_data_providers.clone(),
			)?;

			Ok(import_queue)
		})?;

		(builder, inherent_data_providers)
		}};
}

/// Run the collator with the given `config`.
pub fn run_collator<C: Send + Default + 'static, E: crate::cli::IntoExit + Send + 'static>(
	config: Configuration<C, GenesisConfig>,
	exit: E,
	key: Arc<polkadot_primitives::parachain::CollatorPair>,
	polkadot_config: polkadot_collator::Configuration,
) -> crate::cli::Result<()> {
	let (builder, inherent_data_providers) = new_full_start!(config);
	inherent_data_providers
		.register_provider(sp_timestamp::InherentDataProvider)
		.unwrap();

	let service = builder
		.with_network_protocol(|_| Ok(NodeProtocol::new()))?
		.build()?;
	let proposer_factory = sc_basic_authority::ProposerFactory {
		client: service.client(),
		transaction_pool: service.transaction_pool(),
	};

	let on_exit = service.on_exit();
	let block_import = service.client();

	let setup_parachain = SetupParachain {
		service,
		inherent_data_providers,
		proposer_factory,
		exit,
		block_import,
	};

	cumulus_collator::run_collator(
		setup_parachain,
		crate::PARA_ID,
		on_exit,
		key,
		polkadot_config,
	)
}

struct SetupParachain<S, PF, E, BI> {
	service: S,
	proposer_factory: PF,
	exit: E,
	inherent_data_providers: InherentDataProviders,
	block_import: BI,
}

type TransactionFor<E, Block> =
	<<E as Environment<Block>>::Proposer as Proposer<Block>>::Transaction;

impl<S, PF, E, BI> cumulus_collator::SetupParachain<Block> for SetupParachain<S, PF, E, BI>
where
	S: AbstractService,
	E: Send + crate::cli::IntoExit,
	PF: Environment<Block> + Send + 'static,
	BI: BlockImport<Block, Error = sp_consensus::Error, Transaction = TransactionFor<PF, Block>>
		+ Send
		+ Sync
		+ 'static,
{
	type ProposerFactory = PF;
	type BlockImport = BI;

	fn setup_parachain<P: cumulus_consensus::PolkadotClient, Spawner>(
		self,
		polkadot_client: P,
		spawner: Spawner,
	) -> Result<
		(
			Self::ProposerFactory,
			Self::BlockImport,
			InherentDataProviders,
		),
		String,
	>
	where
		Spawner: Spawn + Clone + Send + Sync + 'static,
	{
		let client = self.service.client();

		let follow =
			match cumulus_consensus::follow_polkadot(crate::PARA_ID, client, polkadot_client) {
				Ok(follow) => follow,
				Err(e) => {
					return Err(format!("Could not start following polkadot: {:?}", e));
				}
			};

		spawner
			.spawn_obj(
				Box::new(
					future::select(
						self.service
							.compat()
							.map_err(|e| error!("Parachain service error: {:?}", e)),
						future::select(follow, self.exit.into_exit()),
					)
					.map(|_| ()),
				)
				.into(),
			)
			.map_err(|_| "Could not spawn parachain server!")?;

		Ok((
			self.proposer_factory,
			self.block_import,
			self.inherent_data_providers,
		))
	}
}
