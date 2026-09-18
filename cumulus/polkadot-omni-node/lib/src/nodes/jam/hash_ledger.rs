// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Durable ledger mapping a parachain block hash to the JAM `WorkPackageHash` this node
//! submitted for it, backed by the client auxiliary store.
//!
//! sr25519 signing is non-deterministic (randomised nonce via `rand::thread_rng()`), so a
//! work-package hash can never be recomputed after signing — it can only be remembered. This
//! module is that memory.

use jam_interface::WorkPackageHash;
use sc_client_api::backend::AuxStore;
use std::sync::Arc;

/// Aux-store key prefix: `b"jam_wp_hash_"` ++ 32 block-hash bytes.
const KEY_PREFIX: &[u8] = b"jam_wp_hash_";

fn aux_key(block_hash: &[u8; 32]) -> Vec<u8> {
	let mut key = Vec::with_capacity(KEY_PREFIX.len() + 32);
	key.extend_from_slice(KEY_PREFIX);
	key.extend_from_slice(block_hash);
	key
}

/// Durable map from a parachain block hash to the JAM `WorkPackageHash` this node submitted
/// for that block. Backed by the client aux store so entries survive restarts.
pub(crate) struct WpHashLedger<C> {
	client: Arc<C>,
}

impl<C: AuxStore> WpHashLedger<C> {
	pub(crate) fn new(client: Arc<C>) -> Self {
		Self { client }
	}

	/// Record that this node submitted `wp_hash` for the block at `block_hash`.
	pub(crate) fn insert(
		&self,
		block_hash: &[u8; 32],
		wp_hash: WorkPackageHash,
	) -> sp_blockchain::Result<()> {
		let key = aux_key(block_hash);
		self.client.insert_aux(&[(&key[..], &wp_hash.0[..])], &[])
	}

	/// Look up the `WorkPackageHash` this node submitted for `block_hash`, if any.
	pub(crate) fn get(
		&self,
		block_hash: &[u8; 32],
	) -> sp_blockchain::Result<Option<WorkPackageHash>> {
		let key = aux_key(block_hash);
		self.client.get_aux(&key).map(|opt| {
			opt.and_then(|bytes| match <[u8; 32]>::try_from(bytes) {
				Ok(arr) => Some(WorkPackageHash(arr)),
				Err(v) => {
					tracing::warn!(
						target: super::LOG_TARGET,
						key_len = v.len(),
						"Malformed aux-store entry for WP hash ledger (expected 32 bytes); treating as absent.",
					);
					None
				},
			})
		})
	}

	/// Remove the entry for `block_hash` (call when a package is forgotten to bound map size).
	pub(crate) fn remove(&self, block_hash: &[u8; 32]) -> sp_blockchain::Result<()> {
		let key = aux_key(block_hash);
		self.client.insert_aux(&[], &[&key[..]])
	}
}

/// Test doubles for the ledger, shared with the collation-task tests so both exercise the same
/// store.
#[cfg(test)]
pub(crate) mod test_support {
	use super::WpHashLedger;
	use sc_client_api::backend::AuxStore;
	use std::{
		collections::HashMap,
		sync::{Arc, Mutex},
	};

	/// In-memory fake for `AuxStore` backed by a mutex-protected `HashMap`.
	pub(crate) struct MockAuxStore {
		data: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
	}

	impl MockAuxStore {
		pub(crate) fn new() -> Self {
			Self { data: Mutex::new(HashMap::new()) }
		}
	}

	impl AuxStore for MockAuxStore {
		fn insert_aux<'a, 'b: 'a, 'c: 'a, I, D>(
			&self,
			insert: I,
			delete: D,
		) -> sp_blockchain::Result<()>
		where
			I: IntoIterator<Item = &'a (&'c [u8], &'c [u8])>,
			D: IntoIterator<Item = &'a &'b [u8]>,
		{
			let mut data = self.data.lock().expect("mutex not poisoned; qed");
			for (k, v) in insert {
				data.insert(k.to_vec(), v.to_vec());
			}
			for k in delete {
				data.remove(*k);
			}
			Ok(())
		}

		fn get_aux(&self, key: &[u8]) -> sp_blockchain::Result<Option<Vec<u8>>> {
			let data = self.data.lock().expect("mutex not poisoned; qed");
			Ok(data.get(key).cloned())
		}
	}

	/// A ledger over a fresh in-memory store.
	pub(crate) fn ledger() -> WpHashLedger<MockAuxStore> {
		WpHashLedger::new(Arc::new(MockAuxStore::new()))
	}
}

#[cfg(test)]
mod tests {
	use super::{
		test_support::{ledger, MockAuxStore},
		WpHashLedger,
	};
	use jam_interface::WorkPackageHash;
	use sc_client_api::backend::AuxStore;
	use std::sync::Arc;

	fn block_hash(byte: u8) -> [u8; 32] {
		[byte; 32]
	}

	fn wp_hash(byte: u8) -> WorkPackageHash {
		WorkPackageHash([byte; 32])
	}

	#[test]
	fn insert_then_get_returns_same_hash() {
		let ledger = ledger();
		let bh = block_hash(1);
		let wph = wp_hash(0xab);
		ledger.insert(&bh, wph).expect("insert ok; qed");
		assert_eq!(ledger.get(&bh).expect("get ok; qed"), Some(wph));
	}

	#[test]
	fn get_missing_returns_none() {
		let ledger = ledger();
		assert_eq!(ledger.get(&block_hash(2)).expect("get ok; qed"), None);
	}

	#[test]
	fn remove_then_get_returns_none() {
		let ledger = ledger();
		let bh = block_hash(3);
		ledger.insert(&bh, wp_hash(0xcd)).expect("insert ok; qed");
		ledger.remove(&bh).expect("remove ok; qed");
		assert_eq!(ledger.get(&bh).expect("get ok; qed"), None);
	}

	/// A corrupt aux-store entry (e.g. from a version-skewed node) must never panic the
	/// collator. A value whose length is not 32 bytes is treated as absent.
	#[test]
	fn get_returns_none_for_malformed_value() {
		let store = Arc::new(MockAuxStore::new());
		let ledger = WpHashLedger::new(Arc::clone(&store));
		let bh = block_hash(4);
		let mut raw_key = b"jam_wp_hash_".to_vec();
		raw_key.extend_from_slice(&bh);
		let bad_val = [0xffu8; 5];
		store
			.insert_aux(&[(&raw_key[..], &bad_val[..])], &[])
			.expect("direct aux insert ok; qed");
		assert_eq!(ledger.get(&bh).expect("no error for malformed entry; qed"), None);
	}
}
