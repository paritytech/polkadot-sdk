// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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

//! Substrate offchain workers.
//!
//! The offchain workers is a special function of the runtime that
//! gets executed after block is imported. During execution
//! it's able to asynchronously submit extrinsics that will either
//! be propagated to other nodes added to the next block
//! produced by the node as unsigned transactions.
//!
//! Offchain workers can be used for computation-heavy tasks
//! that are not feasible for execution during regular block processing.
//! It can either be tasks that no consensus is required for,
//! or some form of consensus over the data can be built on-chain
//! for instance via:
//! 1. Challenge period for incorrect computations
//! 2. Majority voting for results
//! 3. etc

#![warn(missing_docs)]

use std::{fmt, marker::PhantomData, sync::Arc};

use parking_lot::Mutex;
use threadpool::ThreadPool;
use sp_api::ApiExt;
use futures::future::Future;
use log::{debug, warn};
use sc_network::NetworkStateInfo;
use sp_core::{offchain::{self, OffchainStorage}, ExecutionContext};
use sp_runtime::{generic::BlockId, traits::{self, ProvideRuntimeApi}};

mod api;

pub use sp_offchain::{OffchainWorkerApi, STORAGE_PREFIX};

/// An offchain workers manager.
pub struct OffchainWorkers<Client, Storage, Block: traits::Block> {
	client: Arc<Client>,
	db: Storage,
	_block: PhantomData<Block>,
	thread_pool: Mutex<ThreadPool>,
}

impl<Client, Storage, Block: traits::Block> OffchainWorkers<Client, Storage, Block> {
	/// Creates new `OffchainWorkers`.
	pub fn new(client: Arc<Client>, db: Storage) -> Self {
		Self {
			client,
			db,
			_block: PhantomData,
			thread_pool: Mutex::new(ThreadPool::new(num_cpus::get())),
		}
	}
}

impl<Client, Storage, Block: traits::Block> fmt::Debug for OffchainWorkers<
	Client,
	Storage,
	Block,
> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.debug_tuple("OffchainWorkers").finish()
	}
}

impl<Client, Storage, Block> OffchainWorkers<
	Client,
	Storage,
	Block,
> where
	Block: traits::Block,
	Client: ProvideRuntimeApi + Send + Sync + 'static,
	Client::Api: OffchainWorkerApi<Block>,
	Storage: OffchainStorage + 'static,
{
	/// Start the offchain workers after given block.
	#[must_use]
	pub fn on_block_imported(
		&self,
		number: &<Block::Header as traits::Header>::Number,
		network_state: Arc<dyn NetworkStateInfo + Send + Sync>,
		is_validator: bool,
	) -> impl Future<Output = ()> {
		let runtime = self.client.runtime_api();
		let at = BlockId::number(*number);
		let has_api = runtime.has_api::<dyn OffchainWorkerApi<Block, Error = ()>>(&at);
		debug!("Checking offchain workers at {:?}: {:?}", at, has_api);

		if has_api.unwrap_or(false) {
			let (api, runner) = api::AsyncApi::new(
				self.db.clone(),
				network_state.clone(),
				is_validator,
			);
			debug!("Spawning offchain workers at {:?}", at);
			let number = *number;
			let client = self.client.clone();
			self.spawn_worker(move || {
				let runtime = client.runtime_api();
				let api = Box::new(api);
				debug!("Running offchain workers at {:?}", at);
				let run = runtime.offchain_worker_with_context(
					&at,
					ExecutionContext::OffchainCall(Some((api, offchain::Capabilities::all()))),
					number,
				);
				if let Err(e) =	run {
					log::error!("Error running offchain workers at {:?}: {:?}", at, e);
				}
			});
			futures::future::Either::Left(runner.process())
		} else {
			futures::future::Either::Right(futures::future::ready(()))
		}
	}

	/// Spawns a new offchain worker.
	///
	/// We spawn offchain workers for each block in a separate thread,
	/// since they can run for a significant amount of time
	/// in a blocking fashion and we don't want to block the runtime.
	///
	/// Note that we should avoid that if we switch to future-based runtime in the future,
	/// alternatively:
	fn spawn_worker(&self, f: impl FnOnce() -> () + Send + 'static) {
		self.thread_pool.lock().execute(f);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::Arc;
	use sc_network::{Multiaddr, PeerId};
	use substrate_test_runtime_client::runtime::Block;
	use sc_transaction_pool::{BasicPool, FullChainApi};
	use sp_transaction_pool::{TransactionPool, InPoolTransaction};

	struct MockNetworkStateInfo();

	impl NetworkStateInfo for MockNetworkStateInfo {
		fn external_addresses(&self) -> Vec<Multiaddr> {
			Vec::new()
		}

		fn local_peer_id(&self) -> PeerId {
			PeerId::random()
		}
	}

	struct TestPool(BasicPool<FullChainApi<substrate_test_runtime_client::TestClient, Block>, Block>);

	impl sp_transaction_pool::OffchainSubmitTransaction<Block> for TestPool {
		fn submit_at(
			&self,
			at: &BlockId<Block>,
			extrinsic: <Block as sp_runtime::traits::Block>::Extrinsic,
		) -> Result<(), ()> {
			futures::executor::block_on(self.0.submit_one(&at, extrinsic))
				.map(|_| ())
				.map_err(|_| ())
		}
	}

	#[test]
	fn should_call_into_runtime_and_produce_extrinsic() {
		// given
		let _ = env_logger::try_init();
		let client = Arc::new(substrate_test_runtime_client::new());
		let pool = Arc::new(TestPool(BasicPool::new(Default::default(), FullChainApi::new(client.clone()))));
		client.execution_extensions()
			.register_transaction_pool(Arc::downgrade(&pool.clone()) as _);
		let db = sc_client_db::offchain::LocalStorage::new_test();
		let network_state = Arc::new(MockNetworkStateInfo());

		// when
		let offchain = OffchainWorkers::new(client, db);
		futures::executor::block_on(offchain.on_block_imported(&0u64, network_state, false));

		// then
		assert_eq!(pool.0.status().ready, 1);
		assert_eq!(pool.0.ready().next().unwrap().is_propagateable(), false);
	}
}
