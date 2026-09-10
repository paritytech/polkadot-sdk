// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Rpc for state migration.

use jsonrpsee::{
	core::RpcResult,
	proc_macros::rpc,
	types::error::{ErrorCode, ErrorObject, ErrorObjectOwned},
	Extensions,
};
use sc_client_api::TrieCacheContext;
use sc_rpc_api::check_if_safe;
use serde::{Deserialize, Serialize};
use sp_runtime::traits::Block as BlockT;
use std::sync::Arc;

use sp_core::{
	storage::{ChildInfo, ChildType, PrefixedStorageKey},
	Hasher,
};
use sp_state_machine::backend::AsTrieBackend;
use sp_trie::{
	trie_types::{TrieDB, TrieDBBuilder},
	KeySpacedDB, Trie,
};
use trie_db::{
	node::{NodePlan, ValuePlan},
	TrieDBNodeIterator,
};

fn count_migrate<'a, H: Hasher>(
	storage: &'a dyn trie_db::HashDBRef<H, Vec<u8>>,
	root: &'a H::Out,
) -> std::result::Result<(u64, u64, TrieDB<'a, 'a, H>), String> {
	let mut nb = 0u64;
	let mut total_nb = 0u64;
	let trie = TrieDBBuilder::new(storage, root).build();
	let iter_node =
		TrieDBNodeIterator::new(&trie).map_err(|e| format!("TrieDB node iterator error: {}", e))?;
	for node in iter_node {
		let node = node.map_err(|e| format!("TrieDB node iterator error: {}", e))?;
		match node.2.node_plan() {
			NodePlan::Leaf { value, .. } | NodePlan::NibbledBranch { value: Some(value), .. } => {
				total_nb += 1;
				if let ValuePlan::Inline(range) = value {
					if (range.end - range.start) as u32 >=
						sp_core::storage::TRIE_VALUE_NODE_THRESHOLD
					{
						nb += 1;
					}
				}
			},
			_ => (),
		}
	}
	Ok((nb, total_nb, trie))
}

/// Check trie migration status.
pub fn migration_status<H, B>(backend: &B) -> std::result::Result<MigrationStatusResult, String>
where
	H: Hasher,
	H::Out: codec::Codec,
	B: AsTrieBackend<H>,
{
	let trie_backend = backend.as_trie_backend();
	let essence = trie_backend.essence();
	let (top_remaining_to_migrate, total_top, trie) = count_migrate(essence, essence.root())?;

	let mut child_remaining_to_migrate = 0;
	let mut total_child = 0;
	let mut child_roots: Vec<(ChildInfo, Vec<u8>)> = Vec::new();
	// get all child trie roots
	for key_value in trie.iter().map_err(|e| format!("TrieDB node iterator error: {}", e))? {
		let (key, value) = key_value.map_err(|e| format!("TrieDB node iterator error: {}", e))?;
		if key[..].starts_with(sp_core::storage::well_known_keys::DEFAULT_CHILD_STORAGE_KEY_PREFIX)
		{
			let prefixed_key = PrefixedStorageKey::new(key);
			let (_type, unprefixed) = ChildType::from_prefixed_key(&prefixed_key).unwrap();
			child_roots.push((ChildInfo::new_default(unprefixed), value));
		}
	}
	for (child_info, root) in child_roots {
		let mut child_root = H::Out::default();
		let storage = KeySpacedDB::new(essence, child_info.keyspace());

		if root.len() != H::LENGTH {
			return Err(format!(
				"Invalid child trie root length: expected {}, got {}",
				H::LENGTH,
				root.len()
			));
		}
		child_root.as_mut()[..].copy_from_slice(&root[..]);
		let (nb, total_top, _) = count_migrate(&storage, &child_root)?;
		child_remaining_to_migrate += nb;
		total_child += total_top;
	}

	Ok(MigrationStatusResult {
		top_remaining_to_migrate,
		child_remaining_to_migrate,
		total_top,
		total_child,
	})
}

/// Current state migration status.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct MigrationStatusResult {
	/// Number of top items that should migrate.
	pub top_remaining_to_migrate: u64,
	/// Number of child items that should migrate.
	pub child_remaining_to_migrate: u64,
	/// Number of top items that we will iterate on.
	pub total_top: u64,
	/// Number of child items that we will iterate on.
	pub total_child: u64,
}

/// Migration RPC methods.
#[rpc(server)]
pub trait StateMigrationApi<BlockHash> {
	/// Check current migration state.
	///
	/// This call is performed locally without submitting any transactions. Thus executing this
	/// won't change any state. Nonetheless it is a VERY costly call that should be
	/// only exposed to trusted peers.
	#[method(name = "state_trieMigrationStatus", with_extensions)]
	fn call(&self, at: Option<BlockHash>) -> RpcResult<MigrationStatusResult>;
}

/// An implementation of state migration specific RPC methods.
pub struct StateMigration<C, B, BA> {
	client: Arc<C>,
	backend: Arc<BA>,
	_marker: std::marker::PhantomData<(B, BA)>,
}

impl<C, B, BA> StateMigration<C, B, BA> {
	/// Create new state migration rpc for the given reference to the client.
	pub fn new(client: Arc<C>, backend: Arc<BA>) -> Self {
		StateMigration { client, backend, _marker: Default::default() }
	}
}

impl<C, B, BA> StateMigrationApiServer<<B as BlockT>::Hash> for StateMigration<C, B, BA>
where
	B: BlockT,
	C: Send + Sync + 'static + sc_client_api::HeaderBackend<B>,
	BA: 'static + sc_client_api::backend::Backend<B>,
{
	fn call(
		&self,
		ext: &Extensions,
		at: Option<<B as BlockT>::Hash>,
	) -> RpcResult<MigrationStatusResult> {
		check_if_safe(ext)?;

		let hash = at.unwrap_or_else(|| self.client.info().best_hash);
		let state = self
			.backend
			.state_at(hash, TrieCacheContext::Untrusted)
			.map_err(error_into_rpc_err)?;
		migration_status(&state).map_err(error_into_rpc_err)
	}
}

fn error_into_rpc_err(err: impl std::fmt::Display) -> ErrorObjectOwned {
	ErrorObject::owned(
		ErrorCode::InternalError.code(),
		"Error while checking migration state",
		Some(err.to_string()),
	)
}

#[cfg(test)]
mod tests {
	use super::*;

	use sp_core::{
		storage::{well_known_keys, StateVersion},
		Blake2Hasher,
	};
	use sp_state_machine::new_in_mem;

	fn assert_migration_status_panic_safe<H, B>(
		backend: &B,
	) -> std::result::Result<MigrationStatusResult, String>
	where
		H: Hasher,
		H::Out: codec::Codec,
		B: AsTrieBackend<H>,
	{
		let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			migration_status::<H, B>(backend)
		}));
		match result {
			Ok(inner) => inner,
			Err(payload) => {
				let msg = payload
					.downcast_ref::<String>()
					.map(|s| s.clone())
					.or_else(|| payload.downcast_ref::<&'static str>().map(|s| s.to_string()))
					.unwrap_or_else(|| "unknown panic".to_string());
				panic!("migration_status panicked instead of returning Err: {}", msg);
			},
		}
	}

	fn assert_child_root_wrong_length_panic_safe(child_root_len: usize) {
		let mut child_key = well_known_keys::DEFAULT_CHILD_STORAGE_KEY_PREFIX.to_vec();
		child_key.extend_from_slice(b"bogus_child");
		let backend = new_in_mem::<Blake2Hasher>().update(
			vec![(None, vec![(child_key, Some(vec![0u8; child_root_len]))])],
			StateVersion::V1,
		);
		let _ = assert_migration_status_panic_safe::<Blake2Hasher, _>(&backend);
	}

	#[test]
	fn test_risk_264qyrx2_short_child_root_value_must_be_panic_safe() {
		assert_child_root_wrong_length_panic_safe(1);
	}
}
