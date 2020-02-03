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

pub use runtime;
use runtime::{
	genesismap::{additional_storage_with_genesis, GenesisConfig},
	Block,
};
use sp_core::{sr25519, storage::Storage, ChangesTrieConfiguration};
use sp_keyring::{AccountKeyring, Sr25519Keyring};
use sp_runtime::traits::{Block as BlockT, Hash as HashT, Header as HeaderT};
pub use test_client::*;

mod local_executor {
	use test_client::sc_executor::native_executor_instance;
	native_executor_instance!(
		pub LocalExecutor,
		runtime::api::dispatch,
		runtime::native_version,
	);
}

/// Native executor used for tests.
pub use local_executor::LocalExecutor;

/// Test client database backend.
pub type Backend = test_client::Backend<Block>;

/// Test client executor.
pub type Executor =
	sc_client::LocalCallExecutor<Backend, sc_executor::NativeExecutor<LocalExecutor>>;

/// Test client builder for Cumulus
pub type TestClientBuilder = test_client::TestClientBuilder<Block, Executor, Backend, GenesisParameters>;

/// LongestChain type for the test runtime/client.
pub type LongestChain = test_client::sc_client::LongestChain<Backend, Block>;

/// Test client type with `LocalExecutor` and generic Backend.
pub type Client = sc_client::Client<Backend, Executor, Block, runtime::RuntimeApi>;

/// Parameters of test-client builder with test-runtime.
#[derive(Default)]
pub struct GenesisParameters {
	support_changes_trie: bool,
}

impl test_client::GenesisInit for GenesisParameters {
	fn genesis_storage(&self) -> Storage {
		use codec::Encode;

		let changes_trie_config: Option<ChangesTrieConfiguration> = if self.support_changes_trie {
			Some(sp_test_primitives::changes_trie_config())
		} else {
			None
		};
		let mut storage = genesis_config(changes_trie_config).genesis_map();

		let child_roots = storage.children.iter().map(|(sk, child_content)| {
			let state_root =
				<<<runtime::Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
					child_content.data.clone().into_iter().collect(),
				);
			(sk.clone(), state_root.encode())
		});
		let state_root =
			<<<runtime::Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
				storage.top.clone().into_iter().chain(child_roots).collect(),
			);
		let block: runtime::Block = sc_client::genesis::construct_genesis_block(state_root);
		storage.top.extend(additional_storage_with_genesis(&block));

		storage
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

fn genesis_config(changes_trie_config: Option<ChangesTrieConfiguration>) -> GenesisConfig {
	GenesisConfig::new(
		changes_trie_config,
		vec![
			sr25519::Public::from(Sr25519Keyring::Alice).into(),
			sr25519::Public::from(Sr25519Keyring::Bob).into(),
			sr25519::Public::from(Sr25519Keyring::Charlie).into(),
		],
		vec![
			AccountKeyring::Alice.into(),
			AccountKeyring::Bob.into(),
			AccountKeyring::Charlie.into(),
		],
		1000,
		Default::default(),
		Default::default(),
	)
}
