// Copyright 2018-2019 Parity Technologies (UK) Ltd.
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

//! Client testing utilities.

#![warn(missing_docs)]

pub mod trait_tests;

mod block_builder_ext;

use std::sync::Arc;
use std::collections::HashMap;
pub use substrate_test_client::*;
pub use substrate_test_runtime as runtime;

pub use self::block_builder_ext::BlockBuilderExt;

use sp_core::sr25519;
use sp_core::storage::{ChildInfo, Storage, StorageChild};
use substrate_test_runtime::genesismap::{GenesisConfig, additional_storage_with_genesis};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, Hash as HashT, NumberFor};
use sc_client::{
	light::fetcher::{
		Fetcher,
		RemoteHeaderRequest, RemoteReadRequest, RemoteReadChildRequest,
		RemoteCallRequest, RemoteChangesRequest, RemoteBodyRequest,
	},
};


/// A prelude to import in tests.
pub mod prelude {
	// Trait extensions
	pub use super::{BlockBuilderExt, DefaultTestClientBuilderExt, TestClientBuilderExt, ClientExt};
	// Client structs
	pub use super::{
		TestClient, TestClientBuilder, Backend, LightBackend,
		Executor, LightExecutor, LocalExecutor, NativeExecutor, WasmExecutionMethod,
	};
	// Keyring
	pub use super::{AccountKeyring, Sr25519Keyring};
}

mod local_executor {
	#![allow(missing_docs)]
	use substrate_test_runtime;
	use crate::sc_executor::native_executor_instance;
	// FIXME #1576 change the macro and pass in the `BlakeHasher` that dispatch needs from here instead
	native_executor_instance!(
		pub LocalExecutor,
		substrate_test_runtime::api::dispatch,
		substrate_test_runtime::native_version
	);
}

/// Native executor used for tests.
pub use self::local_executor::LocalExecutor;

/// Test client database backend.
pub type Backend = substrate_test_client::Backend<substrate_test_runtime::Block>;

/// Test client executor.
pub type Executor = sc_client::LocalCallExecutor<
	Backend,
	NativeExecutor<LocalExecutor>,
>;

/// Test client light database backend.
pub type LightBackend = substrate_test_client::LightBackend<substrate_test_runtime::Block>;

/// Test client light executor.
pub type LightExecutor = sc_client::light::call_executor::GenesisCallExecutor<
	LightBackend,
	sc_client::LocalCallExecutor<
		sc_client::light::backend::Backend<
			sc_client_db::light::LightStorage<substrate_test_runtime::Block>,
			Blake2Hasher,
		>,
		NativeExecutor<LocalExecutor>
	>
>;

/// Parameters of test-client builder with test-runtime.
#[derive(Default)]
pub struct GenesisParameters {
	support_changes_trie: bool,
	heap_pages_override: Option<u64>,
	extra_storage: Storage,
}

impl GenesisParameters {
	fn genesis_config(&self) -> GenesisConfig {
		GenesisConfig::new(
			self.support_changes_trie,
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
			self.heap_pages_override,
			self.extra_storage.clone(),
		)
	}
}

impl substrate_test_client::GenesisInit for GenesisParameters {
	fn genesis_storage(&self) -> Storage {
		use codec::Encode;

		let mut storage = self.genesis_config().genesis_map();

		let child_roots = storage.children.iter().map(|(sk, child_content)| {
			let state_root = <<<runtime::Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
				child_content.data.clone().into_iter().collect()
			);
			(sk.clone(), state_root.encode())
		});
		let state_root = <<<runtime::Block as BlockT>::Header as HeaderT>::Hashing as HashT>::trie_root(
			storage.top.clone().into_iter().chain(child_roots).collect()
		);
		let block: runtime::Block = sc_client::genesis::construct_genesis_block(state_root);
		storage.top.extend(additional_storage_with_genesis(&block));

		storage
	}
}

/// A `TestClient` with `test-runtime` builder.
pub type TestClientBuilder<E, B> = substrate_test_client::TestClientBuilder<E, B, GenesisParameters>;

/// Test client type with `LocalExecutor` and generic Backend.
pub type Client<B> = sc_client::Client<
	B,
	sc_client::LocalCallExecutor<B, sc_executor::NativeExecutor<LocalExecutor>>,
	substrate_test_runtime::Block,
	substrate_test_runtime::RuntimeApi,
>;

/// A test client with default backend.
pub type TestClient = Client<Backend>;

/// A `TestClientBuilder` with default backend and executor.
pub trait DefaultTestClientBuilderExt: Sized {
	/// Create new `TestClientBuilder`
	fn new() -> Self;
}

impl DefaultTestClientBuilderExt for TestClientBuilder<
	Executor,
	Backend,
> {
	fn new() -> Self {
		Self::with_default_backend()
	}
}

/// A `test-runtime` extensions to `TestClientBuilder`.
pub trait TestClientBuilderExt<B>: Sized {
	/// Enable or disable support for changes trie in genesis.
	fn set_support_changes_trie(self, support_changes_trie: bool) -> Self;

	/// Override the default value for Wasm heap pages.
	fn set_heap_pages(self, heap_pages: u64) -> Self;

	/// Add an extra value into the genesis storage.
	///
	/// # Panics
	///
	/// Panics if the key is empty.
	fn add_extra_child_storage<SK: Into<Vec<u8>>, K: Into<Vec<u8>>, V: Into<Vec<u8>>>(
		self,
		storage_key: SK,
		child_info: ChildInfo,
		key: K,
		value: V,
	) -> Self;

	/// Add an extra child value into the genesis storage.
	///
	/// # Panics
	///
	/// Panics if the key is empty.
	fn add_extra_storage<K: Into<Vec<u8>>, V: Into<Vec<u8>>>(self, key: K, value: V) -> Self;

	/// Build the test client.
	fn build(self) -> Client<B> {
		self.build_with_longest_chain().0
	}

	/// Build the test client and longest chain selector.
	fn build_with_longest_chain(self) -> (Client<B>, sc_client::LongestChain<B, substrate_test_runtime::Block>);
}

impl<B> TestClientBuilderExt<B> for TestClientBuilder<
	sc_client::LocalCallExecutor<B, sc_executor::NativeExecutor<LocalExecutor>>,
	B
> where
	B: sc_client_api::backend::Backend<substrate_test_runtime::Block, Blake2Hasher>,
{
	fn set_heap_pages(mut self, heap_pages: u64) -> Self {
		self.genesis_init_mut().heap_pages_override = Some(heap_pages);
		self
	}

	fn set_support_changes_trie(mut self, support_changes_trie: bool) -> Self {
		self.genesis_init_mut().support_changes_trie = support_changes_trie;
		self
	}

	fn add_extra_storage<K: Into<Vec<u8>>, V: Into<Vec<u8>>>(mut self, key: K, value: V) -> Self {
		let key = key.into();
		assert!(!key.is_empty());
		self.genesis_init_mut().extra_storage.top.insert(key, value.into());
		self
	}

	fn add_extra_child_storage<SK: Into<Vec<u8>>, K: Into<Vec<u8>>, V: Into<Vec<u8>>>(
		mut self,
		storage_key: SK,
		child_info: ChildInfo,
		key: K,
		value: V,
	) -> Self {
		let storage_key = storage_key.into();
		let key = key.into();
		assert!(!storage_key.is_empty());
		assert!(!key.is_empty());
		self.genesis_init_mut().extra_storage.children
			.entry(storage_key)
			.or_insert_with(|| StorageChild {
				data: Default::default(),
				child_info: child_info.to_owned(),
			}).data.insert(key, value.into());
		self
	}


	fn build_with_longest_chain(self) -> (Client<B>, sc_client::LongestChain<B, substrate_test_runtime::Block>) {
		self.build_with_native_executor(None)
	}
}

/// Type of optional fetch callback.
type MaybeFetcherCallback<Req, Resp> = Option<Box<dyn Fn(Req) -> Result<Resp, sp_blockchain::Error> + Send + Sync>>;

/// Type of fetcher future result.
type FetcherFutureResult<Resp> = futures::future::Ready<Result<Resp, sp_blockchain::Error>>;

/// Implementation of light client fetcher used in tests.
#[derive(Default)]
pub struct LightFetcher {
	call: MaybeFetcherCallback<RemoteCallRequest<substrate_test_runtime::Header>, Vec<u8>>,
	body: MaybeFetcherCallback<RemoteBodyRequest<substrate_test_runtime::Header>, Vec<substrate_test_runtime::Extrinsic>>,
}

impl LightFetcher {
	/// Sets remote call callback.
	pub fn with_remote_call(
		self,
		call: MaybeFetcherCallback<RemoteCallRequest<substrate_test_runtime::Header>, Vec<u8>>,
	) -> Self {
		LightFetcher {
			call,
			body: self.body,
		}
	}

	/// Sets remote body callback.
	pub fn with_remote_body(
		self,
		body: MaybeFetcherCallback<RemoteBodyRequest<substrate_test_runtime::Header>, Vec<substrate_test_runtime::Extrinsic>>,
	) -> Self {
		LightFetcher {
			call: self.call,
			body,
		}
	}
}

impl Fetcher<substrate_test_runtime::Block> for LightFetcher {
	type RemoteHeaderResult = FetcherFutureResult<substrate_test_runtime::Header>;
	type RemoteReadResult = FetcherFutureResult<HashMap<Vec<u8>, Option<Vec<u8>>>>;
	type RemoteCallResult = FetcherFutureResult<Vec<u8>>;
	type RemoteChangesResult = FetcherFutureResult<Vec<(NumberFor<substrate_test_runtime::Block>, u32)>>;
	type RemoteBodyResult = FetcherFutureResult<Vec<substrate_test_runtime::Extrinsic>>;

	fn remote_header(&self, _: RemoteHeaderRequest<substrate_test_runtime::Header>) -> Self::RemoteHeaderResult {
		unimplemented!()
	}

	fn remote_read(&self, _: RemoteReadRequest<substrate_test_runtime::Header>) -> Self::RemoteReadResult {
		unimplemented!()
	}

	fn remote_read_child(&self, _: RemoteReadChildRequest<substrate_test_runtime::Header>) -> Self::RemoteReadResult {
		unimplemented!()
	}

	fn remote_call(&self, req: RemoteCallRequest<substrate_test_runtime::Header>) -> Self::RemoteCallResult {
		match self.call {
			Some(ref call) => futures::future::ready(call(req)),
			None => unimplemented!(),
		}
	}

	fn remote_changes(&self, _: RemoteChangesRequest<substrate_test_runtime::Header>) -> Self::RemoteChangesResult {
		unimplemented!()
	}

	fn remote_body(&self, req: RemoteBodyRequest<substrate_test_runtime::Header>) -> Self::RemoteBodyResult {
		match self.body {
			Some(ref body) => futures::future::ready(body(req)),
			None => unimplemented!(),
		}
	}
}

/// Creates new client instance used for tests.
pub fn new() -> Client<Backend> {
	TestClientBuilder::new().build()
}

/// Creates new light client instance used for tests.
pub fn new_light() -> (
	sc_client::Client<LightBackend, LightExecutor, substrate_test_runtime::Block, substrate_test_runtime::RuntimeApi>,
	Arc<LightBackend>,
) {

	let storage = sc_client_db::light::LightStorage::new_test();
	let blockchain = Arc::new(sc_client::light::blockchain::Blockchain::new(storage));
	let backend = Arc::new(LightBackend::new(blockchain.clone()));
	let executor = NativeExecutor::new(WasmExecutionMethod::Interpreted, None);
	let local_call_executor = sc_client::LocalCallExecutor::new(backend.clone(), executor);
	let call_executor = LightExecutor::new(
		backend.clone(),
		local_call_executor,
	);

	(
		TestClientBuilder::with_backend(backend.clone())
			.build_with_executor(call_executor)
			.0,
		backend,
	)
}

/// Creates new light client fetcher used for tests.
pub fn new_light_fetcher() -> LightFetcher {
	LightFetcher::default()
}
