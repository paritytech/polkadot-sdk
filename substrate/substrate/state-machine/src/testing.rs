// Copyright 2017 Parity Technologies (UK) Ltd.
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

//! Test implementation for Externalities.

use std::collections::HashMap;
use super::Externalities;
use triehash::trie_root;

/// Simple HashMap based Externalities impl.
pub type TestExternalities = HashMap<Vec<u8>, Vec<u8>>;

impl Externalities for TestExternalities {
	fn storage(&self, key: &[u8]) -> Option<Vec<u8>> {
		self.get(key).map(|x| x.to_vec())
	}

	fn place_storage(&mut self, key: Vec<u8>, maybe_value: Option<Vec<u8>>) {
		match maybe_value {
			Some(value) => { self.insert(key, value); }
			None => { self.remove(&key); }
		}
	}

	fn clear_prefix(&mut self, prefix: &[u8]) {
		self.retain(|key, _|
			!key.starts_with(prefix)
		)
	}

	fn chain_id(&self) -> u64 { 42 }

	fn storage_root(&mut self) -> [u8; 32] {
		trie_root(self.clone()).0
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn commit_should_work() {
		let mut ext = TestExternalities::new();
		ext.set_storage(b"doe".to_vec(), b"reindeer".to_vec());
		ext.set_storage(b"dog".to_vec(), b"puppy".to_vec());
		ext.set_storage(b"dogglesworth".to_vec(), b"cat".to_vec());
		const ROOT: [u8; 32] = hex!("8aad789dff2f538bca5d8ea56e8abe10f4c7ba3a5dea95fea4cd6e7c3a1168d3");
		assert_eq!(ext.storage_root(), ROOT);
	}
}
