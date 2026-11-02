// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! Persistence for the archive: exact-key reads and atomic write batches.
//!
//! The archive is node-local disk, not consensus state (design § Archive Pruning). It lives in the
//! client's auxiliary database, which is fork-unaware; the archive handles forks itself.

use parking_lot::Mutex;
use sc_client_api::AuxStore;
use std::{collections::BTreeMap, sync::Arc};

/// One atomic write: inserts, then deletes. A key in both is deleted, as in [`AuxStore`].
#[derive(Default, Debug)]
pub struct Batch {
	/// Keys to write.
	pub insert: Vec<(Vec<u8>, Vec<u8>)>,
	/// Keys to remove, after the inserts.
	pub delete: Vec<Vec<u8>>,
}

/// A key-value store with exact-key reads.
pub trait ArchiveStore: Send + Sync {
	/// The value at `key`. A read error counts as absent; the archive then refuses to serve.
	fn get(&self, key: &[u8]) -> Option<Vec<u8>>;

	/// Apply `batch` atomically.
	fn commit(&self, batch: Batch) -> Result<(), String>;
}

/// [`ArchiveStore`] over a client's auxiliary database.
pub struct AuxArchiveStore<C>(Arc<C>);

impl<C> AuxArchiveStore<C> {
	/// Wrap `client`.
	pub fn new(client: Arc<C>) -> Self {
		Self(client)
	}
}

impl<C: AuxStore + Send + Sync> ArchiveStore for AuxArchiveStore<C> {
	fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
		self.0
			.get_aux(key)
			.map_err(|e| log::warn!(target: crate::LOG_TARGET, "Archive read failed: {e}"))
			.ok()
			.flatten()
	}

	fn commit(&self, batch: Batch) -> Result<(), String> {
		let insert: Vec<(&[u8], &[u8])> =
			batch.insert.iter().map(|(k, v)| (k.as_slice(), v.as_slice())).collect();
		let delete: Vec<&[u8]> = batch.delete.iter().map(Vec::as_slice).collect();
		self.0.insert_aux(insert.iter(), delete.iter()).map_err(|e| e.to_string())
	}
}

/// In-memory [`ArchiveStore`], for tests.
#[derive(Default)]
pub struct MemoryStore(Mutex<BTreeMap<Vec<u8>, Vec<u8>>>);

impl MemoryStore {
	/// Number of stored keys.
	pub fn len(&self) -> usize {
		self.0.lock().len()
	}

	/// Whether the store is empty.
	pub fn is_empty(&self) -> bool {
		self.0.lock().is_empty()
	}
}

impl ArchiveStore for MemoryStore {
	fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
		self.0.lock().get(key).cloned()
	}

	fn commit(&self, batch: Batch) -> Result<(), String> {
		let mut map = self.0.lock();
		for (key, value) in batch.insert {
			map.insert(key, value);
		}
		for key in batch.delete {
			map.remove(&key);
		}
		Ok(())
	}
}

impl<S: ArchiveStore + ?Sized> ArchiveStore for Arc<S> {
	fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
		(**self).get(key)
	}

	fn commit(&self, batch: Batch) -> Result<(), String> {
		(**self).commit(batch)
	}
}
