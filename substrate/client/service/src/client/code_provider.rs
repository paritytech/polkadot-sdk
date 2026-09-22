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

use super::{client::ClientConfig, wasm_override::WasmOverride, wasm_substitutes::WasmSubstitutes};
use sc_client_api::{backend, backend::StateBackend as _, TrieCacheContext};
use sc_executor::{RuntimeVersion, RuntimeVersionOf};
use sp_core::{
	storage::well_known_keys,
	traits::{FetchRuntimeCode, RuntimeCode},
};
use sp_runtime::traits::Block as BlockT;
use sp_state_machine::{backend::TryPendingCode, Ext, OverlayedChanges};
use std::sync::Arc;

/// Provider for fetching `:code` of a block.
///
/// As a node can run with code overrides or substitutes, this will ensure that these are taken into
/// account before returning the actual `code` for a block.
pub struct CodeProvider<Block: BlockT, Backend, Executor> {
	backend: Arc<Backend>,
	executor: Arc<Executor>,
	wasm_override: Arc<Option<WasmOverride>>,
	wasm_substitutes: WasmSubstitutes<Block, Executor, Backend>,
}

impl<Block: BlockT, Backend, Executor: Clone> Clone for CodeProvider<Block, Backend, Executor> {
	fn clone(&self) -> Self {
		Self {
			backend: self.backend.clone(),
			executor: self.executor.clone(),
			wasm_override: self.wasm_override.clone(),
			wasm_substitutes: self.wasm_substitutes.clone(),
		}
	}
}

impl<Block, Backend, Executor> CodeProvider<Block, Backend, Executor>
where
	Block: BlockT,
	Backend: backend::Backend<Block>,
	Executor: RuntimeVersionOf,
{
	/// Create a new instance.
	pub fn new(
		client_config: &ClientConfig<Block>,
		executor: Executor,
		backend: Arc<Backend>,
	) -> sp_blockchain::Result<Self> {
		let wasm_override = client_config
			.wasm_runtime_overrides
			.as_ref()
			.map(|p| WasmOverride::new(p.clone(), &executor))
			.transpose()?;

		let executor = Arc::new(executor);

		let wasm_substitutes = WasmSubstitutes::new(
			client_config.wasm_runtime_substitutes.clone(),
			executor.clone(),
			backend.clone(),
		)?;

		Ok(Self { backend, executor, wasm_override: Arc::new(wasm_override), wasm_substitutes })
	}

	/// Returns the `:code` (or `:pending_code`) for the given `block`.
	///
	/// This takes into account potential substitutes, but ignores overrides.
	pub fn code_at_ignoring_overrides(&self, block: Block::Hash) -> sp_blockchain::Result<Vec<u8>> {
		let state = self.backend.state_at(block, TrieCacheContext::Untrusted)?;

		let state_runtime_code =
			sp_state_machine::backend::BackendRuntimeCode::new(&state, TryPendingCode::Yes);
		let runtime_code =
			state_runtime_code.runtime_code().map_err(sp_blockchain::Error::RuntimeCode)?;

		self.maybe_override_code_internal(runtime_code, &state, block, true)
			.and_then(|r| {
				if let Some(code) = r.0.fetch_runtime_code() {
					return Ok(code.into_owned());
				}
				// :code (or :pending_code) holds an offchain-code marker; resolve via CODE column.
				let raw = state
					.storage(well_known_keys::PENDING_CODE)
					.ok()
					.flatten()
					.or_else(|| state.storage(well_known_keys::CODE).ok().flatten());
				let bytes = raw.ok_or_else(|| {
					sp_blockchain::Error::Backend(":code absent from state".into())
				})?;
				let code_hash = sp_code_marker::decode_marker(&bytes).ok_or_else(|| {
					sp_blockchain::Error::Backend(
						"fetch_runtime_code returned None for non-marker :code".into(),
					)
				})?;
				self.backend.code_blob(&code_hash)?.ok_or_else(|| {
					sp_blockchain::Error::Backend(format!(
						"Code blob not found for marker hash {:?}",
						code_hash,
					))
				})
			})
	}

	/// Maybe override the given `onchain_code`.
	///
	/// This takes into account potential overrides/substitutes.
	pub fn maybe_override_code<'a>(
		&'a self,
		onchain_code: RuntimeCode<'a>,
		state: &Backend::State,
		hash: Block::Hash,
	) -> sp_blockchain::Result<(RuntimeCode<'a>, RuntimeVersion)> {
		self.maybe_override_code_internal(onchain_code, state, hash, false)
	}

	/// Maybe override the given `onchain_code`.
	///
	/// This takes into account potential overrides(depending on `ignore_overrides`)/substitutes.
	fn maybe_override_code_internal<'a>(
		&'a self,
		onchain_code: RuntimeCode<'a>,
		state: &Backend::State,
		hash: Block::Hash,
		ignore_overrides: bool,
	) -> sp_blockchain::Result<(RuntimeCode<'a>, RuntimeVersion)> {
		let on_chain_version = self.on_chain_runtime_version(&onchain_code, state)?;
		let code_and_version = if let Some(d) = self.wasm_override.as_ref().as_ref().and_then(|o| {
			if ignore_overrides {
				return None;
			}

			o.get(
				&on_chain_version.spec_version,
				onchain_code.heap_pages,
				&on_chain_version.spec_name,
			)
		}) {
			tracing::debug!(target: "code-provider::overrides", block = ?hash, "using WASM override");
			d
		} else if let Some(s) =
			self.wasm_substitutes
				.get(on_chain_version.spec_version, onchain_code.heap_pages, hash)
		{
			tracing::debug!(target: "code-provider::substitutes", block = ?hash, "Using WASM substitute");
			s
		} else {
			tracing::debug!(
				target: "code-provider",
				block = ?hash,
				"Neither WASM override nor substitute available, using onchain code",
			);
			(onchain_code, on_chain_version)
		};

		Ok(code_and_version)
	}

	/// Returns the on chain runtime version.
	fn on_chain_runtime_version(
		&self,
		code: &RuntimeCode,
		state: &Backend::State,
	) -> sp_blockchain::Result<RuntimeVersion> {
		let mut overlay = OverlayedChanges::default();

		let mut ext = Ext::new(&mut overlay, state, None);

		self.executor
			.runtime_version(&mut ext, code)
			.map_err(|e| sp_blockchain::Error::VersionInvalid(e.to_string()))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use backend::Backend;
	use sc_client_api::{in_mem, HeaderBackend};
	use sc_executor::WasmExecutor;
	use sp_core::{
		testing::TaskExecutor,
		traits::{FetchRuntimeCode, WrappedRuntimeCode},
	};
	use std::collections::HashMap;
	use substrate_test_runtime_client::{runtime, GenesisInit};

	#[test]
	fn marker_in_code_resolves_from_code_column() {
		use sc_client_api::backend::{BlockImportOperation as _, StateBackend as _};
		use sp_core::storage::well_known_keys;
		use sp_runtime::{
			traits::{BlakeTwo256, Hash as HashT},
			StateVersion,
		};

		let executor = WasmExecutor::default();
		let backend =
			Arc::new(substrate_test_runtime_client::Backend::new_test(u32::MAX, u64::MAX));
		let client_config = ClientConfig::default();

		let genesis_block_builder = crate::GenesisBlockBuilder::new(
			&substrate_test_runtime_client::GenesisParameters::default().genesis_storage(),
			!client_config.no_genesis,
			backend.clone(),
			executor.clone(),
		)
		.expect("genesis block builder");

		// Warm the executor's WASM module cache so on_chain_runtime_version succeeds via cache.
		let _client =
			crate::client::new_with_backend::<_, _, runtime::Block, _, runtime::RuntimeApi>(
				backend.clone(),
				executor.clone(),
				genesis_block_builder,
				Box::new(TaskExecutor::new()),
				None,
				None,
				client_config.clone(),
			)
			.expect("client");

		let real_code: Vec<u8> = substrate_test_runtime::wasm_binary_unwrap().to_vec();
		let code_hash: [u8; 32] = <BlakeTwo256 as HashT>::hash(&real_code).0;
		let marker = sp_code_marker::encode_marker(&code_hash);

		backend.insert_code_blob_for_test(&code_hash, &real_code);

		// Build block 1 with :code replaced by the marker, inheriting all other genesis state.
		let genesis_hash = backend.blockchain().info().genesis_hash;
		let mut op = backend.begin_operation().unwrap();
		backend.begin_state_operation(&mut op, genesis_hash).unwrap();

		let state_root = {
			let parent = op.state().unwrap().expect("parent state");
			let (root, overlay) = parent.storage_root(
				std::iter::once((well_known_keys::CODE, Some(marker.as_ref()))),
				StateVersion::V1,
			);
			op.update_db_storage(overlay).unwrap();
			root
		};

		let header = runtime::Header {
			parent_hash: genesis_hash,
			number: 1u64,
			state_root,
			extrinsics_root: Default::default(),
			digest: Default::default(),
		};
		op.set_block_data(
			header.clone(),
			Some(vec![]),
			None,
			None,
			sc_client_api::backend::NewBlockState::Best,
			false,
		)
		.unwrap();
		backend.commit_operation(op).unwrap();

		let block1_hash = header.hash();
		let executor = Arc::new(executor);
		let code_provider = CodeProvider {
			backend: backend.clone(),
			executor: executor.clone(),
			wasm_override: Arc::new(None),
			wasm_substitutes: WasmSubstitutes::new(Default::default(), executor, backend.clone())
				.unwrap(),
		};

		let code = code_provider.code_at_ignoring_overrides(block1_hash).unwrap();
		assert_eq!(code, real_code);
	}

	#[test]
	fn marker_in_code_resolves_during_execution() {
		use crate::client::LocalCallExecutor;
		use sc_client_api::{
			backend::{BlockImportOperation as _, StateBackend as _},
			call_executor::CallExecutor,
			execution_extensions::ExecutionExtensions,
		};
		use sp_core::traits::CallContext;
		use sp_runtime::{
			traits::{BlakeTwo256, Hash as HashT},
			StateVersion,
		};

		let executor = WasmExecutor::default();
		let backend =
			Arc::new(substrate_test_runtime_client::Backend::new_test(u32::MAX, u64::MAX));
		let client_config = ClientConfig::default();

		let genesis_block_builder = crate::GenesisBlockBuilder::new(
			&substrate_test_runtime_client::GenesisParameters::default().genesis_storage(),
			!client_config.no_genesis,
			backend.clone(),
			executor.clone(),
		)
		.expect("genesis block builder");

		let _client =
			crate::client::new_with_backend::<_, _, runtime::Block, _, runtime::RuntimeApi>(
				backend.clone(),
				executor.clone(),
				genesis_block_builder,
				Box::new(TaskExecutor::new()),
				None,
				None,
				client_config.clone(),
			)
			.expect("client");

		let real_code: Vec<u8> = substrate_test_runtime::wasm_binary_unwrap().to_vec();
		let code_hash: [u8; 32] = <BlakeTwo256 as HashT>::hash(&real_code).0;
		let marker = sp_code_marker::encode_marker(&code_hash);

		backend.insert_code_blob_for_test(&code_hash, &real_code);

		let genesis_hash = backend.blockchain().info().genesis_hash;
		let mut op = backend.begin_operation().unwrap();
		backend.begin_state_operation(&mut op, genesis_hash).unwrap();

		let state_root = {
			let parent = op.state().unwrap().expect("parent state");
			let (root, overlay) = parent.storage_root(
				std::iter::once((well_known_keys::CODE, Some(marker.as_ref()))),
				StateVersion::V1,
			);
			op.update_db_storage(overlay).unwrap();
			root
		};

		let header = runtime::Header {
			parent_hash: genesis_hash,
			number: 1u64,
			state_root,
			extrinsics_root: Default::default(),
			digest: Default::default(),
		};
		op.set_block_data(
			header.clone(),
			Some(vec![]),
			None,
			None,
			sc_client_api::backend::NewBlockState::Best,
			false,
		)
		.unwrap();
		backend.commit_operation(op).unwrap();
		let block1_hash = header.hash();

		// Use a fresh executor (empty cache) to force code resolution from the CODE column.
		// Before the marker-resolution fix this call fails because fetch_runtime_code() = None.
		let exec = WasmExecutor::default();
		let call_executor = LocalCallExecutor::<runtime::Block, _, _>::new(
			backend.clone(),
			exec.clone(),
			client_config,
			ExecutionExtensions::new(None, Arc::new(exec)),
		)
		.unwrap();

		let version =
			CallExecutor::runtime_version(&call_executor, block1_hash, CallContext::Offchain)
				.expect(
					"runtime_version must succeed when :code is a marker with a CODE column entry",
				);
		assert!(!version.spec_name.is_empty(), "expected a real spec_name from the test runtime");
	}

	#[test]
	fn no_override_no_substitutes_work() {
		let executor = WasmExecutor::default();

		let code_fetcher = WrappedRuntimeCode(substrate_test_runtime::wasm_binary_unwrap().into());
		let onchain_code = RuntimeCode {
			code_fetcher: &code_fetcher,
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
				client_config.clone(),
			)
			.expect("Creates a client");

		let executor = Arc::new(executor);

		let code_provider = CodeProvider {
			backend: backend.clone(),
			executor: executor.clone(),
			wasm_override: Arc::new(None),
			wasm_substitutes: WasmSubstitutes::new(Default::default(), executor, backend.clone())
				.unwrap(),
		};

		let check = code_provider
			.maybe_override_code(
				onchain_code,
				&backend
					.state_at(backend.blockchain().info().genesis_hash, TrieCacheContext::Untrusted)
					.unwrap(),
				backend.blockchain().info().genesis_hash,
			)
			.expect("RuntimeCode override")
			.0;

		assert_eq!(code_fetcher.fetch_runtime_code(), check.fetch_runtime_code());
	}

	#[test]
	fn should_get_override_if_exists() {
		let executor = WasmExecutor::default();

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
				client_config.clone(),
			)
			.expect("Creates a client");

		let executor = Arc::new(executor);

		let code_provider = CodeProvider {
			backend: backend.clone(),
			executor: executor.clone(),
			wasm_override: Arc::new(Some(overrides)),
			wasm_substitutes: WasmSubstitutes::new(Default::default(), executor, backend.clone())
				.unwrap(),
		};

		let check = code_provider
			.maybe_override_code(
				onchain_code,
				&backend
					.state_at(backend.blockchain().info().genesis_hash, TrieCacheContext::Untrusted)
					.unwrap(),
				backend.blockchain().info().genesis_hash,
			)
			.expect("RuntimeCode override")
			.0;

		assert_eq!(Some(vec![2, 2, 2, 2, 2, 2, 2, 2]), check.fetch_runtime_code().map(Into::into));
	}

	#[test]
	fn returns_runtime_version_from_substitute() {
		const SUBSTITUTE_SPEC_NAME: &str = "substitute-spec-name-cool";

		let executor = WasmExecutor::default();

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

		let version = client
			.runtime_version_at(client.chain_info().genesis_hash, sp_api::CallContext::Offchain)
			.unwrap();

		assert_eq!(SUBSTITUTE_SPEC_NAME, &*version.spec_name);
	}
}
