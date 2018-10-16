// Copyright 2018 Parity Technologies (UK) Ltd.
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

#![warn(unused_extern_crates)]

//! Substrate service. Specialized wrapper over substrate service.

extern crate node_primitives;
extern crate node_runtime;
extern crate node_executor;
extern crate node_network;
extern crate substrate_client as client;
extern crate substrate_network as network;
extern crate substrate_primitives as primitives;
extern crate substrate_service as service;
extern crate substrate_transaction_pool as transaction_pool;
extern crate tokio;
#[cfg(test)]
extern crate substrate_service_test as service_test;

#[macro_use]
extern crate hex_literal;
#[cfg(all(test, feature="rhd"))]
extern crate rhododendron as rhd;
extern crate sr_primitives as runtime_primitives;
pub mod chain_spec;

use std::sync::Arc;
use transaction_pool::txpool::{Pool as TransactionPool};
use node_primitives::{Block, Hash};
use node_runtime::GenesisConfig;
use client::Client;
use node_network::Protocol as DemoProtocol;
use tokio::runtime::TaskExecutor;
use service::FactoryFullConfiguration;
use network::import_queue::{BasicQueue, BlockOrigin, ImportBlock, Verifier};
use runtime_primitives::{traits::Block as BlockT};
use primitives::{Blake2Hasher, AuthorityId};

pub use service::{Roles, PruningMode, TransactionPoolOptions, ServiceFactory,
	ErrorKind, Error, ComponentBlock, LightComponents, FullComponents};
pub use client::ExecutionStrategy;

/// Specialised `ChainSpec`.
pub type ChainSpec = service::ChainSpec<GenesisConfig>;
/// Client type for specialised `Components`.
pub type ComponentClient<C> = Client<<C as Components>::Backend, <C as Components>::Executor, Block>;
pub type NetworkService = network::Service<Block, <Factory as service::ServiceFactory>::NetworkProtocol, Hash>;

/// A verifier that doesn't actually do any checks
pub struct NoneVerifier;
/// This Verifiyer accepts all data as valid
impl<B: BlockT> Verifier<B> for NoneVerifier {
	fn verify(
		&self,
		origin: BlockOrigin,
		header: B::Header,
		justification: Vec<u8>,
		body: Option<Vec<B::Extrinsic>>
	) -> Result<(ImportBlock<B>, Option<Vec<AuthorityId>>), String> {
		Ok((ImportBlock {
			origin,
			header,
			body,
			finalized: true,
			external_justification: justification,
			internal_justification: vec![],
			auxiliary: Vec::new(),
		}, None))
	}
}

/// A collection of type to generalise specific components over full / light client.
pub trait Components: service::Components {
	/// Demo API.
	type Api: 'static + Send + Sync;
	/// Client backend.
	type Backend: 'static + client::backend::Backend<Block, Blake2Hasher>;
	/// Client executor.
	type Executor: 'static + client::CallExecutor<Block, Blake2Hasher> + Send + Sync;
}

impl Components for service::LightComponents<Factory> {
	type Api = service::LightClient<Factory>;
	type Executor = service::LightExecutor<Factory>;
	type Backend = service::LightBackend<Factory>;
}

impl Components for service::FullComponents<Factory> {
	type Api = service::FullClient<Factory>;
	type Executor = service::FullExecutor<Factory>;
	type Backend = service::FullBackend<Factory>;
}

/// All configuration for the node.
pub type Configuration = FactoryFullConfiguration<Factory>;

/// Demo-specific configuration.
#[derive(Default)]
pub struct CustomConfiguration;

/// Config for the substrate service.
pub struct Factory;

impl service::ServiceFactory for Factory {
	type Block = Block;
	type ExtrinsicHash = Hash;
	type NetworkProtocol = DemoProtocol;
	type RuntimeDispatch = node_executor::Executor;
	type FullTransactionPoolApi = transaction_pool::ChainApi<service::FullBackend<Self>, service::FullExecutor<Self>, Block>;
	type LightTransactionPoolApi = transaction_pool::ChainApi<service::LightBackend<Self>, service::LightExecutor<Self>, Block>;
	type Genesis = GenesisConfig;
	type Configuration = CustomConfiguration;
	type FullService = Service<service::FullComponents<Self>>;
	type LightService = Service<service::LightComponents<Self>>;
	/// instance of import queue for clients
	type ImportQueue = BasicQueue<Block, NoneVerifier>;

	fn build_full_transaction_pool(config: TransactionPoolOptions, client: Arc<service::FullClient<Self>>)
		-> Result<TransactionPool<Self::FullTransactionPoolApi>, Error>
	{
		Ok(TransactionPool::new(config, transaction_pool::ChainApi::new(client)))
	}

	fn build_light_transaction_pool(config: TransactionPoolOptions, client: Arc<service::LightClient<Self>>)
		-> Result<TransactionPool<Self::LightTransactionPoolApi>, Error>
	{
		Ok(TransactionPool::new(config, transaction_pool::ChainApi::new(client)))
	}

	fn build_network_protocol(_config: &Configuration)
		-> Result<DemoProtocol, Error>
	{
		Ok(DemoProtocol::new())
	}

	fn build_full_import_queue(
		_config: &FactoryFullConfiguration<Self>,
		_client: Arc<service::FullClient<Self>>,
	) -> Result<BasicQueue<Block, NoneVerifier>, service::Error> {
		Ok(BasicQueue::new(Arc::new(NoneVerifier {})))
	}

	fn build_light_import_queue(
		_config: &FactoryFullConfiguration<Self>,
		_client: Arc<service::LightClient<Self>>,
	) -> Result<BasicQueue<Block, NoneVerifier>, service::Error> {
		Ok(BasicQueue::new(Arc::new(NoneVerifier {})))
	}

	fn new_light(config: Configuration, executor: TaskExecutor)
		-> Result<Service<LightComponents<Factory>>, Error>
	{
		let service = service::Service::<LightComponents<Factory>>::new(config, executor.clone())?;
		Ok(Service {
			inner: service,
			_consensus: None,
		})
	}

	fn new_full(config: Configuration, executor: TaskExecutor)
		-> Result<Service<FullComponents<Factory>>, Error>
	{
		let service = service::Service::<FullComponents<Factory>>::new(config, executor.clone())?;
		// FIXME: Spin consensus service if configured
		let consensus = None;
		Ok(Service {
			inner: service,
			_consensus: consensus,
		})
	}
}
/// Demo service.
pub struct Service<C: Components> {
	inner: service::Service<C>,
	_consensus: Option<bool>,  // FIXME: add actual consensus engine
}

impl<C: Components> ::std::ops::Deref for Service<C> {
	type Target = service::Service<C>;
	fn deref(&self) -> &Self::Target {
		&self.inner
	}
}


/// Creates bare client without any networking.
pub fn new_client(config: Configuration)
	-> Result<Arc<service::ComponentClient<FullComponents<Factory>>>, Error>
{
	service::new_client::<Factory>(&config)
}

#[cfg(test)]
mod tests {
	use {service_test, Factory, chain_spec};

	#[test]
	fn test_connectivity() {
		service_test::connectivity::<Factory>(chain_spec::integration_test_config());
	}

	#[test]
	#[cfg(feature = "rhd")]
	fn test_sync() {
		use client::{ImportBlock, BlockOrigin};

		let alice: Arc<ed25519::Pair> = Arc::new(Keyring::Alice.into());
		let bob: Arc<ed25519::Pair> = Arc::new(Keyring::Bob.into());
		let validators = vec![alice.public().0.into(), bob.public().0.into()];
		let keys: Vec<&ed25519::Pair> = vec![&*alice, &*bob];
		let offline = Arc::new(RwLock::new(OfflineTracker::new()));
		let dummy_runtime = ::tokio::runtime::Runtime::new().unwrap();
		let block_factory = |service: &<Factory as service::ServiceFactory>::FullService| {
			let block_id = BlockId::number(service.client().info().unwrap().chain.best_number);
			let parent_header = service.client().header(&block_id).unwrap().unwrap();
			let consensus_net = ConsensusNetwork::new(service.network(), service.client().clone());
			let proposer_factory = consensus::ProposerFactory {
				client: service.client().clone(),
				transaction_pool: service.transaction_pool().clone(),
				network: consensus_net,
				offline: offline.clone(),
				force_delay: 0,
				handle: dummy_runtime.executor(),
			};
			let (proposer, _, _) = proposer_factory.init(&parent_header, &validators, alice.clone()).unwrap();
			let block = proposer.propose().expect("Error making test block");
			ImportBlock {
				origin: BlockOrigin::File,
				external_justification: Vec::new(),
				internal_justification: Vec::new(),
				finalized: true,
				body: Some(block.extrinsics),
				header: block.header,
				auxiliary: Vec::new(),
			}
		};
		let extrinsic_factory = |service: &<Factory as service::ServiceFactory>::FullService| {
			let payload = (0, Call::Balances(BalancesCall::transfer(RawAddress::Id(bob.public().0.into()), 69)), Era::immortal(), service.client().genesis_hash());
			let signature = alice.sign(&payload.encode()).into();
			let id = alice.public().0.into();
			let xt = UncheckedExtrinsic {
				signature: Some((RawAddress::Id(id), signature, payload.0, Era::immortal())),
				function: payload.1,
			}.encode();
			let v: Vec<u8> = Decode::decode(&mut xt.as_slice()).unwrap();
			OpaqueExtrinsic(v)
		};
		service_test::sync::<Factory, _, _>(chain_spec::integration_test_config(), block_factory, extrinsic_factory);
	}

}
