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

use std::sync::Arc;

use kvdb::KeyValueDB;
use node_primitives::Hash;
use sp_core::H256;
use sp_trie::{DBLocation, DBValue, MemoryDB};
use trie_db::node_db::{NodeDB, Prefix};

pub type Hasher = sp_core::Blake2Hasher;

/// Immutable generated trie database with root.
pub struct SimpleTrie<'a> {
	pub db: Arc<dyn KeyValueDB>,
	pub overlay: &'a mut MemoryDB<Hasher>,
}

impl<'a> NodeDB<Hasher, DBValue, DBLocation> for SimpleTrie<'a> {
	fn get(
		&self,
		key: &H256,
		prefix: Prefix,
		_locaton: DBLocation,
	) -> Option<(DBValue, Vec<DBLocation>)> {
		if let Some(value) = self.overlay.get(&key, prefix) {
			return Some((value.clone(), vec![]));
		}
		self.db
			.get(0, key.as_ref())
			.expect("Database backend error")
			.map(|v| (v, vec![]))
	}

	fn contains(&self, hash: &Hash, prefix: Prefix, location: DBLocation) -> bool {
		self.get(hash, prefix, location).is_some()
	}
}
