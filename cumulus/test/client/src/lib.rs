// Copyright 2019 Parity Technologies (UK) Ltd.
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
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! A Cumulus test client.

pub use test_client::*;
pub use runtime;
use runtime::{Block, genesismap::{GenesisConfig, additional_storage_with_genesis}};
use runtime_primitives::traits::{Hash as HashT, Header as HeaderT, Block as BlockT};
use primitives::storage::well_known_keys;

mod local_executor {
	use test_client::executor::native_executor_instance;
	native_executor_instance!(
		pub LocalExecutor,
		runtime::api::dispatch,
		runtime::native_version,
		runtime::WASM_BINARY
	);
}

/// Native executor used for tests.
pub use local_executor::LocalExecutor;

/// Test client database backend.
pub type Backend = test_client::Backend<Block>;

/// Test client executor.
pub type Executor = client::LocalCallExecutor<Backend, executor::NativeExecutor<LocalExecutor>>;

/// Test client builder for Cumulus
pub type TestClientBuilder = test_client::TestClientBuilder<Executor, Backend, GenesisParameters>;

/// LongestChain type for the test runtime/client.
pub type LongestChain = test_client::client::LongestChain<Backend, Block>;

/// Test client type with `LocalExecutor` and generic Backend.
pub type Client = client::Client<Backend, Executor, Block, runtime::RuntimeApi>;

/// Parameters of test-client builder with test-runtime.
#[derive(Default)]
pub struct GenesisParameters {
	support_changes_trie: bool,
}

impl test_client::GenesisInit for GenesisParameters {
	fn genesis_storage(&self) -> (StorageOverlay, ChildrenStorageOverlay) {
		let mut storage = genesis_config(self.support_changes_trie).genesis_map();
		storage.insert(well_known_keys::CODE.to_vec(), runtime::WASM_BINARY.to_vec());

		let state_root = <<<Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
			storage.clone().into_iter()
		);
		let block: runtime::Block = client::genesis::construct_genesis_block(state_root);
		storage.extend(additional_storage_with_genesis(&block));

		(storage, Default::default())
	}
}

/// A `test-runtime` extensions to `TestClientBuilder`.
pub trait TestClientBuilderExt: Sized {
	/// Enable or disable support for changes trie in genesis.
	fn set_support_changes_trie(self, support_changes_trie: bool) -> Self;

	/// Build the test client.
	fn build(self) -> Client {
		self.build_with_longest_chain().0
	}

	/// Build the test client and longest chain selector.
	fn build_with_longest_chain(self) -> (Client, LongestChain);
}

impl TestClientBuilderExt for TestClientBuilder {
	fn set_support_changes_trie(mut self, support_changes_trie: bool) -> Self {
		self.genesis_init_mut().support_changes_trie = support_changes_trie;
		self
	}

	fn build_with_longest_chain(self) -> (Client, LongestChain) {
		self.build_with_native_executor(None)
	}
}

/// A `TestClientBuilder` with default backend and executor.
pub trait DefaultTestClientBuilderExt: Sized {
	/// Create new `TestClientBuilder`
	fn new() -> Self;
}

impl DefaultTestClientBuilderExt for TestClientBuilder {
	fn new() -> Self {
		Self::with_default_backend()
	}
}

fn genesis_config(support_changes_trie: bool) -> GenesisConfig {
	GenesisConfig::new(support_changes_trie, vec![
		AuthorityKeyring::Alice.into(),
		AuthorityKeyring::Bob.into(),
		AuthorityKeyring::Charlie.into(),
	], vec![
		AccountKeyring::Alice.into(),
		AccountKeyring::Bob.into(),
		AccountKeyring::Charlie.into(),
	],
		1000
	)
}