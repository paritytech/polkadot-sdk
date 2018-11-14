// Copyright 2017-2018 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Substrate service components.

use std::{sync::Arc, net::SocketAddr, marker::PhantomData, ops::Deref};
use serde::{Serialize, de::DeserializeOwned};
use tokio::runtime::TaskExecutor;
use chain_spec::{ChainSpec, Properties};
use client_db;
use client::{self, Client, runtime_api::{TaggedTransactionQueue, Metadata}};
use {error, Service, RpcConfig, maybe_start_server, TransactionPoolAdapter};
use network::{self, OnDemand, import_queue::ImportQueue};
use substrate_executor::{NativeExecutor, NativeExecutionDispatch};
use transaction_pool::txpool::{self, Options as TransactionPoolOptions, Pool as TransactionPool};
use runtime_primitives::{traits::Block as BlockT, traits::Header as HeaderT, BuildStorage, generic::SignedBlock};
use config::Configuration;
use primitives::{Blake2Hasher, H256};
use rpc;

// Type aliases.
// These exist mainly to avoid typing `<F as Factory>::Foo` all over the code.
/// Network service type for a factory.
pub type NetworkService<F> = network::Service<
	<F as ServiceFactory>::Block,
	<F as ServiceFactory>::NetworkProtocol,
	<<F as ServiceFactory>::Block as BlockT>::Hash,
>;

/// Code executor type for a factory.
pub type CodeExecutor<F> = NativeExecutor<<F as ServiceFactory>::RuntimeDispatch>;

/// Full client backend type for a factory.
pub type FullBackend<F> = client_db::Backend<<F as ServiceFactory>::Block>;

/// Full client executor type for a factory.
pub type FullExecutor<F> = client::LocalCallExecutor<
	client_db::Backend<<F as ServiceFactory>::Block>,
	CodeExecutor<F>,
>;

/// Light client backend type for a factory.
pub type LightBackend<F> = client::light::backend::Backend<
	client_db::light::LightStorage<<F as ServiceFactory>::Block>,
	network::OnDemand<<F as ServiceFactory>::Block, NetworkService<F>>,
>;

/// Light client executor type for a factory.
pub type LightExecutor<F> = client::light::call_executor::RemoteCallExecutor<
	client::light::blockchain::Blockchain<
		client_db::light::LightStorage<<F as ServiceFactory>::Block>,
		network::OnDemand<<F as ServiceFactory>::Block, NetworkService<F>>
	>,
	network::OnDemand<<F as ServiceFactory>::Block, NetworkService<F>>,
	Blake2Hasher,
>;

/// Full client type for a factory.
pub type FullClient<F> = Client<FullBackend<F>, FullExecutor<F>, <F as ServiceFactory>::Block, <F as ServiceFactory>::RuntimeApi>;

/// Light client type for a factory.
pub type LightClient<F> = Client<LightBackend<F>, LightExecutor<F>, <F as ServiceFactory>::Block, <F as ServiceFactory>::RuntimeApi>;

/// `ChainSpec` specialization for a factory.
pub type FactoryChainSpec<F> = ChainSpec<<F as ServiceFactory>::Genesis>;

/// `Genesis` specialization for a factory.
pub type FactoryGenesis<F> = <F as ServiceFactory>::Genesis;

/// `Block` type for a factory.
pub type FactoryBlock<F> = <F as ServiceFactory>::Block;

/// `Extrinsic` type for a factory.
pub type FactoryExtrinsic<F> = <<F as ServiceFactory>::Block as BlockT>::Extrinsic;

/// `Number` type for a factory.
pub type FactoryBlockNumber<F> = <<FactoryBlock<F> as BlockT>::Header as HeaderT>::Number;

/// Full `Configuration` type for a factory.
pub type FactoryFullConfiguration<F> = Configuration<<F as ServiceFactory>::Configuration, FactoryGenesis<F>>;

/// Client type for `Components`.
pub type ComponentClient<C> = Client<
	<C as Components>::Backend,
	<C as Components>::Executor,
	FactoryBlock<<C as Components>::Factory>,
	<C as Components>::RuntimeApi,
>;

/// Block type for `Components`
pub type ComponentBlock<C> = <<C as Components>::Factory as ServiceFactory>::Block;

/// Extrinsic hash type for `Components`
pub type ComponentExHash<C> = <<C as Components>::TransactionPoolApi as txpool::ChainApi>::Hash;

/// Extrinsic type.
pub type ComponentExtrinsic<C> = <ComponentBlock<C> as BlockT>::Extrinsic;

/// Extrinsic pool API type for `Components`.
pub type PoolApi<C> = <C as Components>::TransactionPoolApi;

/// A set of traits for the runtime genesis config.
pub trait RuntimeGenesis: Serialize + DeserializeOwned + BuildStorage {}
impl<T: Serialize + DeserializeOwned + BuildStorage> RuntimeGenesis for T {}

/// Something that can start the RPC service.
pub trait StartRPC<C: Components> {
	fn start_rpc(
		client: Arc<Client<C::Backend, C::Executor, ComponentBlock<C>, C::RuntimeApi>>,
		chain_name: String,
		impl_name: &'static str,
		impl_version: &'static str,
		rpc_http: Option<SocketAddr>,
		rpc_ws: Option<SocketAddr>,
		properties: Properties,
		task_executor: TaskExecutor,
		transaction_pool: Arc<TransactionPool<C::TransactionPoolApi>>,
	) -> Result<(Option<rpc::HttpServer>, Option<rpc::WsServer>), error::Error>;
}

impl<T: Components> StartRPC<Self> for T where
	T::RuntimeApi: Metadata<ComponentBlock<T>>,
	for<'de> SignedBlock<ComponentBlock<T>>: ::serde::Deserialize<'de>,
{
	fn start_rpc(
		client: Arc<Client<T::Backend, T::Executor, ComponentBlock<T>, T::RuntimeApi>>,
		chain_name: String,
		impl_name: &'static str,
		impl_version: &'static str,
		rpc_http: Option<SocketAddr>,
		rpc_ws: Option<SocketAddr>,
		properties: Properties,
		task_executor: TaskExecutor,
		transaction_pool: Arc<TransactionPool<T::TransactionPoolApi>>,
	) -> Result<(Option<rpc::HttpServer>, Option<rpc::WsServer>), error::Error> {
		let rpc_config = RpcConfig { properties, chain_name, impl_name, impl_version };

		let handler = || {
			let client = client.clone();
			let subscriptions = rpc::apis::Subscriptions::new(task_executor.clone());
			let chain = rpc::apis::chain::Chain::new(client.clone(), subscriptions.clone());
			let state = rpc::apis::state::State::new(client.clone(), subscriptions.clone());
			let author = rpc::apis::author::Author::new(
				client.clone(), transaction_pool.clone(), subscriptions
			);
			rpc::rpc_handler::<ComponentBlock<T>, ComponentExHash<T>, _, _, _, _>(
				state,
				chain,
				author,
				rpc_config.clone(),
			)
		};

		Ok((
			maybe_start_server(rpc_http, |address| rpc::start_http(address, handler()))?,
			maybe_start_server(rpc_ws, |address| rpc::start_ws(address, handler()))?,
		))
	}
}

/// Something that can create an instance of `network::Params`.
pub trait CreateNetworkParams<C: Components> {
	fn create_network_params<S>(
		client: Arc<Client<C::Backend, C::Executor, ComponentBlock<C>, C::RuntimeApi>>,
		roles: network::config::Roles,
		network_config: network::config::NetworkConfiguration,
		on_demand: Option<Arc<OnDemand<FactoryBlock<C::Factory>, NetworkService<C::Factory>>>>,
		transaction_pool_adapter: TransactionPoolAdapter<C>,
		specialization: S,
	) -> network::config::Params<ComponentBlock<C>, S, ComponentExHash<C>>;
}

impl<T: Components> CreateNetworkParams<Self> for T where
	T::RuntimeApi: TaggedTransactionQueue<ComponentBlock<T>>
{
	fn create_network_params<S>(
		client: Arc<Client<T::Backend, T::Executor, ComponentBlock<T>, T::RuntimeApi>>,
		roles: network::config::Roles,
		network_config: network::config::NetworkConfiguration,
		on_demand: Option<Arc<OnDemand<FactoryBlock<T::Factory>, NetworkService<T::Factory>>>>,
		transaction_pool_adapter: TransactionPoolAdapter<T>,
		specialization: S,
	) -> network::config::Params<ComponentBlock<T>, S, ComponentExHash<T>> {
		network::config::Params {
			config: network::config::ProtocolConfig { roles },
			network_config,
			chain: client,
			on_demand: on_demand.map(|d| d as Arc<network::OnDemandService<ComponentBlock<T>>>),
			transaction_pool: Arc::new(transaction_pool_adapter),
			specialization,
		}
	}
}

/// The super trait that combines all required traits a `Service` needs to implement.
pub trait ServiceTrait<C: Components>:
	Deref<Target = Service<C>>
	+ Send
	+ Sync
	+ 'static
	+ StartRPC<C>
	+ CreateNetworkParams<C>
{}
impl<C: Components, T> ServiceTrait<C> for T where
	T: Deref<Target = Service<C>> + Send + Sync + 'static + StartRPC<C> + CreateNetworkParams<C>
{}

/// A collection of types and methods to build a service on top of the substrate service.
pub trait ServiceFactory: 'static + Sized {
	/// Block type.
	type Block: BlockT<Hash=H256>;
	/// The type that implements the runtime API.
	type RuntimeApi: Send + Sync;
	/// Network protocol extensions.
	type NetworkProtocol: network::specialization::NetworkSpecialization<Self::Block>;
	/// Chain runtime.
	type RuntimeDispatch: NativeExecutionDispatch + Send + Sync + 'static;
	/// Extrinsic pool backend type for the full client.
	type FullTransactionPoolApi: txpool::ChainApi<Hash = <Self::Block as BlockT>::Hash, Block = Self::Block> + Send + 'static;
	/// Extrinsic pool backend type for the light client.
	type LightTransactionPoolApi: txpool::ChainApi<Hash = <Self::Block as BlockT>::Hash, Block = Self::Block> + 'static;
	/// Genesis configuration for the runtime.
	type Genesis: RuntimeGenesis;
	/// Other configuration for service members.
	type Configuration: Default;
	/// Extended full service type.
	type FullService: ServiceTrait<FullComponents<Self>>;
	/// Extended light service type.
	type LightService: ServiceTrait<LightComponents<Self>>;
	/// ImportQueue for full client
	type FullImportQueue: network::import_queue::ImportQueue<Self::Block> + 'static;
	/// ImportQueue for light clients
	type LightImportQueue: network::import_queue::ImportQueue<Self::Block> + 'static;

	//TODO: replace these with a constructor trait. that TransactionPool implements.
	/// Extrinsic pool constructor for the full client.
	fn build_full_transaction_pool(config: TransactionPoolOptions, client: Arc<FullClient<Self>>)
		-> Result<TransactionPool<Self::FullTransactionPoolApi>, error::Error>;
	/// Extrinsic pool constructor for the light client.
	fn build_light_transaction_pool(config: TransactionPoolOptions, client: Arc<LightClient<Self>>)
		-> Result<TransactionPool<Self::LightTransactionPoolApi>, error::Error>;

	/// Build network protocol.
	fn build_network_protocol(config: &FactoryFullConfiguration<Self>)
		-> Result<Self::NetworkProtocol, error::Error>;

	/// Build full service.
	fn new_full(config: FactoryFullConfiguration<Self>, executor: TaskExecutor)
		-> Result<Self::FullService, error::Error>;
	/// Build light service.
	fn new_light(config: FactoryFullConfiguration<Self>, executor: TaskExecutor)
		-> Result<Self::LightService, error::Error>;

	/// ImportQueue for a full client
	fn build_full_import_queue(
		config: &FactoryFullConfiguration<Self>,
		_client: Arc<FullClient<Self>>
	) -> Result<Self::FullImportQueue, error::Error> {
		if let Some(name) = config.chain_spec.consensus_engine() {
			match name {
				_ => Err(format!("Chain Specification defines unknown consensus engine '{}'", name).into())
			}

		} else {
			Err("Chain Specification doesn't contain any consensus_engine name".into())
		}
	}

	/// ImportQueue for a light client
	fn build_light_import_queue(
		config: &FactoryFullConfiguration<Self>,
		_client: Arc<LightClient<Self>>
	) -> Result<Self::LightImportQueue, error::Error> {
		if let Some(name) = config.chain_spec.consensus_engine() {
			match name {
				_ => Err(format!("Chain Specification defines unknown consensus engine '{}'", name).into())
			}

		} else {
			Err("Chain Specification doesn't contain any consensus_engine name".into())
		}
	}
}

/// A collection of types and function to generalise over full / light client type.
pub trait Components: Sized + 'static {
	/// Associated service factory.
	type Factory: ServiceFactory;
	/// Client backend.
	type Backend: 'static + client::backend::Backend<FactoryBlock<Self::Factory>, Blake2Hasher>;
	/// Client executor.
	type Executor: 'static + client::CallExecutor<FactoryBlock<Self::Factory>, Blake2Hasher> + Send + Sync + Clone;
	/// Extrinsic pool type.
	type TransactionPoolApi: 'static + txpool::ChainApi<
		Hash = <<Self::Factory as ServiceFactory>::Block as BlockT>::Hash,
		Block = FactoryBlock<Self::Factory>
	>;
	/// The type that implements the runtime API.
	type RuntimeApi: Send + Sync;
	/// A type that can start the RPC.
	type RPC: StartRPC<Self>;
	/// A type that can create the network params.
	type CreateNetworkParams: CreateNetworkParams<Self>;

	/// Our Import Queue
	type ImportQueue: ImportQueue<FactoryBlock<Self::Factory>> + 'static;

	/// Create client.
	fn build_client(
		config: &FactoryFullConfiguration<Self::Factory>,
		executor: CodeExecutor<Self::Factory>,
	) -> Result<
		(
			Arc<ComponentClient<Self>>,
			Option<Arc<OnDemand<FactoryBlock<Self::Factory>, NetworkService<Self::Factory>>>>
		),
		error::Error
	>;

	/// Create extrinsic pool.
	fn build_transaction_pool(config: TransactionPoolOptions, client: Arc<ComponentClient<Self>>)
		-> Result<TransactionPool<Self::TransactionPoolApi>, error::Error>;

	/// instance of import queue for clients
	fn build_import_queue(
		config: &FactoryFullConfiguration<Self::Factory>,
		client: Arc<ComponentClient<Self>>
	) -> Result<Self::ImportQueue, error::Error>;
}

/// A struct that implement `Components` for the full client.
pub struct FullComponents<Factory: ServiceFactory> {
	_factory: PhantomData<Factory>,
	service: Service<FullComponents<Factory>>,
}

impl<Factory: ServiceFactory> FullComponents<Factory> {
	pub fn new(
		config: FactoryFullConfiguration<Factory>,
		task_executor: TaskExecutor
	) -> Result<Self, error::Error> {
		Ok(
			Self {
				_factory: Default::default(),
				service: Service::new(config, task_executor)?,
			}
		)
	}
}

impl<Factory: ServiceFactory> Deref for FullComponents<Factory> {
	type Target = Service<Self>;

	fn deref(&self) -> &Self::Target {
		&self.service
	}
}

impl<Factory: ServiceFactory> Components for FullComponents<Factory> {
	type Factory = Factory;
	type Executor = FullExecutor<Factory>;
	type Backend = FullBackend<Factory>;
	type TransactionPoolApi = <Factory as ServiceFactory>::FullTransactionPoolApi;
	type ImportQueue = Factory::FullImportQueue;
	type RuntimeApi = Factory::RuntimeApi;
	type RPC = Factory::FullService;
	type CreateNetworkParams = Factory::FullService;

	fn build_client(
		config: &FactoryFullConfiguration<Factory>,
		executor: CodeExecutor<Self::Factory>,
	)
		-> Result<(
			Arc<ComponentClient<Self>>,
			Option<Arc<OnDemand<FactoryBlock<Self::Factory>, NetworkService<Self::Factory>>>>
		), error::Error>
	{
		let db_settings = client_db::DatabaseSettings {
			cache_size: None,
			path: config.database_path.as_str().into(),
			pruning: config.pruning.clone(),
		};
		Ok((Arc::new(client_db::new_client(
			db_settings,
			executor,
			&config.chain_spec,
			config.block_execution_strategy,
			config.api_execution_strategy,
		)?), None))
	}

	fn build_transaction_pool(config: TransactionPoolOptions, client: Arc<ComponentClient<Self>>)
		-> Result<TransactionPool<Self::TransactionPoolApi>, error::Error>
	{
		Factory::build_full_transaction_pool(config, client)
	}

	fn build_import_queue(
		config: &FactoryFullConfiguration<Self::Factory>,
		client: Arc<ComponentClient<Self>>
	) -> Result<Self::ImportQueue, error::Error> {
		Factory::build_full_import_queue(config, client)
	}
}

/// A struct that implement `Components` for the light client.
pub struct LightComponents<Factory: ServiceFactory> {
	_factory: PhantomData<Factory>,
	service: Service<LightComponents<Factory>>,
}

impl<Factory: ServiceFactory> LightComponents<Factory> {
	pub fn new(
		config: FactoryFullConfiguration<Factory>,
		task_executor: TaskExecutor
	) -> Result<Self, error::Error> {
		Ok(
			Self {
				_factory: Default::default(),
				service: Service::new(config, task_executor)?,
			}
		)
	}
}

impl<Factory: ServiceFactory> Deref for LightComponents<Factory> {
	type Target = Service<Self>;

	fn deref(&self) -> &Self::Target {
		&self.service
	}
}

impl<Factory: ServiceFactory> Components for LightComponents<Factory> {
	type Factory = Factory;
	type Executor = LightExecutor<Factory>;
	type Backend = LightBackend<Factory>;
	type TransactionPoolApi = <Factory as ServiceFactory>::LightTransactionPoolApi;
	type ImportQueue = <Factory as ServiceFactory>::LightImportQueue;
	type RuntimeApi = Factory::RuntimeApi;
	type RPC = Factory::LightService;
	type CreateNetworkParams = Factory::LightService;

	fn build_client(
		config: &FactoryFullConfiguration<Factory>,
		executor: CodeExecutor<Self::Factory>,
	)
		-> Result<
			(
				Arc<ComponentClient<Self>>,
				Option<Arc<OnDemand<FactoryBlock<Self::Factory>, NetworkService<Self::Factory>>>>
			), error::Error>
	{
		let db_settings = client_db::DatabaseSettings {
			cache_size: None,
			path: config.database_path.as_str().into(),
			pruning: config.pruning.clone(),
		};
		let db_storage = client_db::light::LightStorage::new(db_settings)?;
		let light_blockchain = client::light::new_light_blockchain(db_storage);
		let fetch_checker = Arc::new(client::light::new_fetch_checker::<_, Blake2Hasher>(executor));
		let fetcher = Arc::new(network::OnDemand::new(fetch_checker));
		let client_backend = client::light::new_light_backend(light_blockchain, fetcher.clone());
		let client = client::light::new_light(client_backend, fetcher.clone(), &config.chain_spec)?;
		Ok((Arc::new(client), Some(fetcher)))
	}

	fn build_transaction_pool(config: TransactionPoolOptions, client: Arc<ComponentClient<Self>>)
		-> Result<TransactionPool<Self::TransactionPoolApi>, error::Error>
	{
		Factory::build_light_transaction_pool(config, client)
	}

	fn build_import_queue(
		config: &FactoryFullConfiguration<Self::Factory>,
		client: Arc<ComponentClient<Self>>
	) -> Result<Self::ImportQueue, error::Error> {
		Factory::build_light_import_queue(config, client)
	}
}
