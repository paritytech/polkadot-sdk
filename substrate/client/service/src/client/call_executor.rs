// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::{code_provider::CodeProvider, ClientConfig};
use sc_client_api::{
	backend, call_executor::CallExecutor, execution_extensions::ExecutionExtensions, HeaderBackend,
};
use sc_executor::{RuntimeVersion, RuntimeVersionOf};
use sp_api::ProofRecorder;
use sp_core::traits::{CallContext, CodeExecutor};
use sp_externalities::Extensions;
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, HashingFor},
};
<<<<<<< HEAD
use sp_state_machine::{backend::AsTrieBackend, Ext, OverlayedChanges, StateMachine, StorageProof};
use std::sync::Arc;
||||||| 07e55006ad0
use sp_state_machine::{backend::AsTrieBackend, Ext, OverlayedChanges, StateMachine, StorageProof};
use std::{cell::RefCell, sync::Arc};
=======
use sp_state_machine::{backend::AsTrieBackend, OverlayedChanges, StateMachine, StorageProof};
use std::{cell::RefCell, sync::Arc};
>>>>>>> origin/master

/// Call executor that executes methods locally, querying all required
/// data from local backend.
pub struct LocalCallExecutor<Block: BlockT, B, E> {
	backend: Arc<B>,
	executor: E,
	code_provider: CodeProvider<Block, B, E>,
	execution_extensions: Arc<ExecutionExtensions<Block>>,
}

impl<Block: BlockT, B, E> LocalCallExecutor<Block, B, E>
where
	E: CodeExecutor + RuntimeVersionOf + Clone + 'static,
	B: backend::Backend<Block>,
{
	/// Creates new instance of local call executor.
	pub fn new(
		backend: Arc<B>,
		executor: E,
		client_config: ClientConfig<Block>,
		execution_extensions: ExecutionExtensions<Block>,
	) -> sp_blockchain::Result<Self> {
		let code_provider = CodeProvider::new(&client_config, executor.clone(), backend.clone())?;

		Ok(LocalCallExecutor {
			backend,
			executor,
			code_provider,
			execution_extensions: Arc::new(execution_extensions),
		})
	}
}

impl<Block: BlockT, B, E> Clone for LocalCallExecutor<Block, B, E>
where
	E: Clone,
{
	fn clone(&self) -> Self {
		LocalCallExecutor {
			backend: self.backend.clone(),
			executor: self.executor.clone(),
			code_provider: self.code_provider.clone(),
			execution_extensions: self.execution_extensions.clone(),
		}
	}
}

impl<B, E, Block> CallExecutor<Block> for LocalCallExecutor<Block, B, E>
where
	B: backend::Backend<Block>,
	E: CodeExecutor + RuntimeVersionOf + Clone + 'static,
	Block: BlockT,
{
	type Error = E::Error;

	type Backend = B;

	fn execution_extensions(&self) -> &ExecutionExtensions<Block> {
		&self.execution_extensions
	}

	fn call(
		&self,
		at_hash: Block::Hash,
		method: &str,
		call_data: &[u8],
		context: CallContext,
	) -> sp_blockchain::Result<Vec<u8>> {
		let mut changes = OverlayedChanges::default();
		let at_number =
			self.backend.blockchain().expect_block_number_from_id(&BlockId::Hash(at_hash))?;
		let state = self.backend.state_at(at_hash)?;

		let state_runtime_code = sp_state_machine::backend::BackendRuntimeCode::new(&state);
		let runtime_code =
			state_runtime_code.runtime_code().map_err(sp_blockchain::Error::RuntimeCode)?;

		let runtime_code = self.code_provider.maybe_override_code(runtime_code, &state, at_hash)?.0;

		let mut extensions = self.execution_extensions.extensions(at_hash, at_number);

		let mut sm = StateMachine::new(
			&state,
			&mut changes,
			&self.executor,
			method,
			call_data,
			&mut extensions,
			&runtime_code,
			context,
		)
		.set_parent_hash(at_hash);

		sm.execute().map_err(Into::into)
	}

	fn contextual_call(
		&self,
		at_hash: Block::Hash,
		method: &str,
		call_data: &[u8],
		changes: &mut OverlayedChanges<HashingFor<Block>>,
		recorder: Option<&ProofRecorder<Block>>,
		call_context: CallContext,
		extensions: &mut Extensions,
	) -> Result<Vec<u8>, sp_blockchain::Error> {
		let state = self.backend.state_at(at_hash)?;

		// It is important to extract the runtime code here before we create the proof
		// recorder to not record it. We also need to fetch the runtime code from `state` to
		// make sure we use the caching layers.
		let state_runtime_code = sp_state_machine::backend::BackendRuntimeCode::new(&state);

		let runtime_code =
			state_runtime_code.runtime_code().map_err(sp_blockchain::Error::RuntimeCode)?;
<<<<<<< HEAD
		let runtime_code = self.check_override(runtime_code, &state, at_hash)?.0;
||||||| 07e55006ad0
		let runtime_code = self.check_override(runtime_code, &state, at_hash)?.0;
		let mut extensions = extensions.borrow_mut();
=======
		let runtime_code = self.code_provider.maybe_override_code(runtime_code, &state, at_hash)?.0;
		let mut extensions = extensions.borrow_mut();
>>>>>>> origin/master

		match recorder {
			Some(recorder) => {
				let trie_state = state.as_trie_backend();

				let backend = sp_state_machine::TrieBackendBuilder::wrap(&trie_state)
					.with_recorder(recorder.clone())
					.build();

				let mut state_machine = StateMachine::new(
					&backend,
					changes,
					&self.executor,
					method,
					call_data,
					extensions,
					&runtime_code,
					call_context,
				)
				.set_parent_hash(at_hash);
				state_machine.execute()
			},
			None => {
				let mut state_machine = StateMachine::new(
					&state,
					changes,
					&self.executor,
					method,
					call_data,
					extensions,
					&runtime_code,
					call_context,
				)
				.set_parent_hash(at_hash);
				state_machine.execute()
			},
		}
		.map_err(Into::into)
	}

	fn runtime_version(&self, at_hash: Block::Hash) -> sp_blockchain::Result<RuntimeVersion> {
		let state = self.backend.state_at(at_hash)?;
		let state_runtime_code = sp_state_machine::backend::BackendRuntimeCode::new(&state);

		let runtime_code =
			state_runtime_code.runtime_code().map_err(sp_blockchain::Error::RuntimeCode)?;
		self.code_provider
			.maybe_override_code(runtime_code, &state, at_hash)
			.map(|(_, v)| v)
	}

	fn prove_execution(
		&self,
		at_hash: Block::Hash,
		method: &str,
		call_data: &[u8],
	) -> sp_blockchain::Result<(Vec<u8>, StorageProof)> {
		let at_number =
			self.backend.blockchain().expect_block_number_from_id(&BlockId::Hash(at_hash))?;
		let state = self.backend.state_at(at_hash)?;

		let trie_backend = state.as_trie_backend();

		let state_runtime_code = sp_state_machine::backend::BackendRuntimeCode::new(trie_backend);
		let runtime_code =
			state_runtime_code.runtime_code().map_err(sp_blockchain::Error::RuntimeCode)?;
		let runtime_code = self.code_provider.maybe_override_code(runtime_code, &state, at_hash)?.0;

		sp_state_machine::prove_execution_on_trie_backend(
			trie_backend,
			&mut Default::default(),
			&self.executor,
			method,
			call_data,
			&runtime_code,
			&mut self.execution_extensions.extensions(at_hash, at_number),
		)
		.map_err(Into::into)
	}
}

impl<B, E, Block> RuntimeVersionOf for LocalCallExecutor<Block, B, E>
where
	E: RuntimeVersionOf,
	Block: BlockT,
{
	fn runtime_version(
		&self,
		ext: &mut dyn sp_externalities::Externalities,
		runtime_code: &sp_core::traits::RuntimeCode,
	) -> Result<sp_version::RuntimeVersion, sc_executor::error::Error> {
		RuntimeVersionOf::runtime_version(&self.executor, ext, runtime_code)
	}
}

impl<Block, B, E> sp_version::GetRuntimeVersionAt<Block> for LocalCallExecutor<Block, B, E>
where
	B: backend::Backend<Block>,
	E: CodeExecutor + RuntimeVersionOf + Clone + 'static,
	Block: BlockT,
{
	fn runtime_version(&self, at: Block::Hash) -> Result<sp_version::RuntimeVersion, String> {
		CallExecutor::runtime_version(self, at).map_err(|e| e.to_string())
	}
}

impl<Block, B, E> sp_version::GetNativeVersion for LocalCallExecutor<Block, B, E>
where
	B: backend::Backend<Block>,
	E: CodeExecutor + sp_version::GetNativeVersion + Clone + 'static,
	Block: BlockT,
{
	fn native_version(&self) -> &sp_version::NativeVersion {
		self.executor.native_version()
<<<<<<< HEAD
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use backend::Backend;
	use sc_client_api::in_mem;
	use sc_executor::{NativeElseWasmExecutor, WasmExecutor};
	use sp_core::{
		testing::TaskExecutor,
		traits::{FetchRuntimeCode, WrappedRuntimeCode},
	};
	use std::collections::HashMap;
	use substrate_test_runtime_client::{runtime, GenesisInit, LocalExecutorDispatch};

	fn executor() -> NativeElseWasmExecutor<LocalExecutorDispatch> {
		NativeElseWasmExecutor::new_with_wasm_executor(
			WasmExecutor::builder()
				.with_max_runtime_instances(1)
				.with_runtime_cache_size(2)
				.build(),
		)
	}

	#[test]
	fn should_get_override_if_exists() {
		let executor = executor();

		let overrides = crate::client::wasm_override::dummy_overrides();
		let onchain_code = WrappedRuntimeCode(substrate_test_runtime::wasm_binary_unwrap().into());
		let onchain_code = RuntimeCode {
			code_fetcher: &onchain_code,
			heap_pages: Some(128),
			hash: vec![0, 0, 0, 0],
		};

		let backend = Arc::new(in_mem::Backend::<runtime::Block>::new());

		// wasm_runtime_overrides is `None` here because we construct the
		// LocalCallExecutor directly later on
		let client_config = ClientConfig::default();

		let genesis_block_builder = crate::GenesisBlockBuilder::new(
			&substrate_test_runtime_client::GenesisParameters::default().genesis_storage(),
			!client_config.no_genesis,
			backend.clone(),
			executor.clone(),
		)
		.expect("Creates genesis block builder");

		// client is used for the convenience of creating and inserting the genesis block.
		let _client = crate::client::new_with_backend::<_, _, runtime::Block, _>(
			backend.clone(),
			executor.clone(),
			genesis_block_builder,
			Box::new(TaskExecutor::new()),
			None,
			None,
			client_config,
		)
		.expect("Creates a client");

		let call_executor = LocalCallExecutor {
			backend: backend.clone(),
			executor: executor.clone(),
			wasm_override: Arc::new(Some(overrides)),
			wasm_substitutes: WasmSubstitutes::new(
				Default::default(),
				executor.clone(),
				backend.clone(),
			)
			.unwrap(),
			execution_extensions: Arc::new(ExecutionExtensions::new(
				None,
				Arc::new(executor.clone()),
			)),
		};

		let check = call_executor
			.check_override(
				onchain_code,
				&backend.state_at(backend.blockchain().info().genesis_hash).unwrap(),
				backend.blockchain().info().genesis_hash,
			)
			.expect("RuntimeCode override")
			.0;

		assert_eq!(Some(vec![2, 2, 2, 2, 2, 2, 2, 2]), check.fetch_runtime_code().map(Into::into));
	}

	#[test]
	fn returns_runtime_version_from_substitute() {
		const SUBSTITUTE_SPEC_NAME: &str = "substitute-spec-name-cool";

		let executor = executor();

		let backend = Arc::new(in_mem::Backend::<runtime::Block>::new());

		// Let's only override the `spec_name` for our testing purposes.
		let substitute = sp_version::embed::embed_runtime_version(
			&substrate_test_runtime::WASM_BINARY_BLOATY.unwrap(),
			sp_version::RuntimeVersion {
				spec_name: SUBSTITUTE_SPEC_NAME.into(),
				..substrate_test_runtime::VERSION
			},
		)
		.unwrap();

		let client_config = crate::client::ClientConfig {
			wasm_runtime_substitutes: vec![(0, substitute)].into_iter().collect::<HashMap<_, _>>(),
			..Default::default()
		};

		let genesis_block_builder = crate::GenesisBlockBuilder::new(
			&substrate_test_runtime_client::GenesisParameters::default().genesis_storage(),
			!client_config.no_genesis,
			backend.clone(),
			executor.clone(),
		)
		.expect("Creates genesis block builder");

		// client is used for the convenience of creating and inserting the genesis block.
		let client = crate::client::new_with_backend::<_, _, runtime::Block, _>(
			backend.clone(),
			executor.clone(),
			genesis_block_builder,
			Box::new(TaskExecutor::new()),
			None,
			None,
			client_config,
		)
		.expect("Creates a client");

		let version = client.runtime_version_at(client.chain_info().genesis_hash).unwrap();

		assert_eq!(SUBSTITUTE_SPEC_NAME, &*version.spec_name);
||||||| 07e55006ad0
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use backend::Backend;
	use sc_client_api::in_mem;
	use sc_executor::{NativeElseWasmExecutor, WasmExecutor};
	use sp_core::{
		testing::TaskExecutor,
		traits::{FetchRuntimeCode, WrappedRuntimeCode},
	};
	use std::collections::HashMap;
	use substrate_test_runtime_client::{runtime, GenesisInit, LocalExecutorDispatch};

	fn executor() -> NativeElseWasmExecutor<LocalExecutorDispatch> {
		NativeElseWasmExecutor::new_with_wasm_executor(
			WasmExecutor::builder()
				.with_max_runtime_instances(1)
				.with_runtime_cache_size(2)
				.build(),
		)
	}

	#[test]
	fn should_get_override_if_exists() {
		let executor = executor();

		let overrides = crate::client::wasm_override::dummy_overrides();
		let onchain_code = WrappedRuntimeCode(substrate_test_runtime::wasm_binary_unwrap().into());
		let onchain_code = RuntimeCode {
			code_fetcher: &onchain_code,
			heap_pages: Some(128),
			hash: vec![0, 0, 0, 0],
		};

		let backend = Arc::new(in_mem::Backend::<runtime::Block>::new());

		// wasm_runtime_overrides is `None` here because we construct the
		// LocalCallExecutor directly later on
		let client_config = ClientConfig::default();

		let genesis_block_builder = crate::GenesisBlockBuilder::new(
			&substrate_test_runtime_client::GenesisParameters::default().genesis_storage(),
			!client_config.no_genesis,
			backend.clone(),
			executor.clone(),
		)
		.expect("Creates genesis block builder");

		// client is used for the convenience of creating and inserting the genesis block.
		let _client =
			crate::client::new_with_backend::<_, _, runtime::Block, _, runtime::RuntimeApi>(
				backend.clone(),
				executor.clone(),
				genesis_block_builder,
				Box::new(TaskExecutor::new()),
				None,
				None,
				client_config,
			)
			.expect("Creates a client");

		let call_executor = LocalCallExecutor {
			backend: backend.clone(),
			executor: executor.clone(),
			wasm_override: Arc::new(Some(overrides)),
			wasm_substitutes: WasmSubstitutes::new(
				Default::default(),
				executor.clone(),
				backend.clone(),
			)
			.unwrap(),
			execution_extensions: Arc::new(ExecutionExtensions::new(
				None,
				Arc::new(executor.clone()),
			)),
		};

		let check = call_executor
			.check_override(
				onchain_code,
				&backend.state_at(backend.blockchain().info().genesis_hash).unwrap(),
				backend.blockchain().info().genesis_hash,
			)
			.expect("RuntimeCode override")
			.0;

		assert_eq!(Some(vec![2, 2, 2, 2, 2, 2, 2, 2]), check.fetch_runtime_code().map(Into::into));
	}

	#[test]
	fn returns_runtime_version_from_substitute() {
		const SUBSTITUTE_SPEC_NAME: &str = "substitute-spec-name-cool";

		let executor = executor();

		let backend = Arc::new(in_mem::Backend::<runtime::Block>::new());

		// Let's only override the `spec_name` for our testing purposes.
		let substitute = sp_version::embed::embed_runtime_version(
			&substrate_test_runtime::WASM_BINARY_BLOATY.unwrap(),
			sp_version::RuntimeVersion {
				spec_name: SUBSTITUTE_SPEC_NAME.into(),
				..substrate_test_runtime::VERSION
			},
		)
		.unwrap();

		let client_config = crate::client::ClientConfig {
			wasm_runtime_substitutes: vec![(0, substitute)].into_iter().collect::<HashMap<_, _>>(),
			..Default::default()
		};

		let genesis_block_builder = crate::GenesisBlockBuilder::new(
			&substrate_test_runtime_client::GenesisParameters::default().genesis_storage(),
			!client_config.no_genesis,
			backend.clone(),
			executor.clone(),
		)
		.expect("Creates genesis block builder");

		// client is used for the convenience of creating and inserting the genesis block.
		let client =
			crate::client::new_with_backend::<_, _, runtime::Block, _, runtime::RuntimeApi>(
				backend.clone(),
				executor.clone(),
				genesis_block_builder,
				Box::new(TaskExecutor::new()),
				None,
				None,
				client_config,
			)
			.expect("Creates a client");

		let version = client.runtime_version_at(client.chain_info().genesis_hash).unwrap();

		assert_eq!(SUBSTITUTE_SPEC_NAME, &*version.spec_name);
=======
>>>>>>> origin/master
	}
}
