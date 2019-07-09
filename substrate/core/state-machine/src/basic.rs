// Copyright 2017-2019 Parity Technologies (UK) Ltd.
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

//! Basic implementation for Externalities.

use std::collections::HashMap;
use std::iter::FromIterator;
use crate::backend::{Backend, InMemory};
use hash_db::Hasher;
use trie::trie_root;
use primitives::offchain;
use primitives::storage::well_known_keys::{HEAP_PAGES, is_child_storage_key};
use parity_codec::Encode;
use super::{ChildStorageKey, Externalities};
use log::warn;

/// Simple HashMap-based Externalities impl.
#[derive(Debug)]
pub struct BasicExternalities {
	top: HashMap<Vec<u8>, Vec<u8>>,
	children: HashMap<Vec<u8>, HashMap<Vec<u8>, Vec<u8>>>,
}

impl BasicExternalities {
	/// Create a new instance of `BasicExternalities`
	pub fn new(top: HashMap<Vec<u8>, Vec<u8>>) -> Self {
		Self::new_with_children(top, Default::default())
	}

	/// Create a new instance of `BasicExternalities` with children
	pub fn new_with_children(
		mut top: HashMap<Vec<u8>, Vec<u8>>,
		children: HashMap<Vec<u8>, HashMap<Vec<u8>, Vec<u8>>>,
	) -> Self {
		top.insert(HEAP_PAGES.to_vec(), 8u64.encode());
		BasicExternalities {
			top,
			children,
		}
	}

	/// Insert key/value
	pub fn insert(&mut self, k: Vec<u8>, v: Vec<u8>) -> Option<Vec<u8>> {
		self.top.insert(k, v)
	}

	/// Consume self and returns inner storages
	pub fn into_storages(self) -> (
		HashMap<Vec<u8>, Vec<u8>>,
		HashMap<Vec<u8>, HashMap<Vec<u8>, Vec<u8>>>,
	) {
		(self.top, self.children)
	}
}

impl PartialEq for BasicExternalities {
	fn eq(&self, other: &BasicExternalities) -> bool {
		self.top.eq(&other.top) && self.children.eq(&other.children)
	}
}

impl FromIterator<(Vec<u8>, Vec<u8>)> for BasicExternalities {
	fn from_iter<I: IntoIterator<Item=(Vec<u8>, Vec<u8>)>>(iter: I) -> Self {
		let mut t = Self::default();
		t.top.extend(iter);
		t
	}
}

impl Default for BasicExternalities {
	fn default() -> Self { Self::new(Default::default()) }
}

impl From<HashMap<Vec<u8>, Vec<u8>>> for BasicExternalities {
	fn from(hashmap: HashMap<Vec<u8>, Vec<u8>>) -> Self {
		BasicExternalities {
			top: hashmap,
			children: Default::default(),
		}
	}
}

impl<H: Hasher> Externalities<H> for BasicExternalities where H::Out: Ord {
	fn storage(&self, key: &[u8]) -> Option<Vec<u8>> {
		self.top.get(key).cloned()
	}

	fn original_storage(&self, key: &[u8]) -> Option<Vec<u8>> {
		Externalities::<H>::storage(self, key)
	}

	fn child_storage(&self, storage_key: ChildStorageKey<H>, key: &[u8]) -> Option<Vec<u8>> {
		self.children.get(storage_key.as_ref()).and_then(|child| child.get(key)).cloned()
	}

	fn place_storage(&mut self, key: Vec<u8>, maybe_value: Option<Vec<u8>>) {
		if is_child_storage_key(&key) {
			warn!(target: "trie", "Refuse to set child storage key via main storage");
			return;
		}

		match maybe_value {
			Some(value) => { self.top.insert(key, value); }
			None => { self.top.remove(&key); }
		}
	}

	fn place_child_storage(
		&mut self,
		storage_key: ChildStorageKey<H>,
		key: Vec<u8>,
		value: Option<Vec<u8>>
	) {
		let child_map = self.children.entry(storage_key.into_owned()).or_default();
		if let Some(value) = value {
			child_map.insert(key, value);
		} else {
			child_map.remove(&key);
		}
	}

	fn kill_child_storage(&mut self, storage_key: ChildStorageKey<H>) {
		self.children.remove(storage_key.as_ref());
	}

	fn clear_prefix(&mut self, prefix: &[u8]) {
		if is_child_storage_key(prefix) {
			warn!(
				target: "trie",
				"Refuse to clear prefix that is part of child storage key via main storage"
			);
			return;
		}

		self.top.retain(|key, _| !key.starts_with(prefix));
	}

	fn chain_id(&self) -> u64 { 42 }

	fn storage_root(&mut self) -> H::Out {
		trie_root::<H, _, _, _>(self.top.clone())
	}

	fn child_storage_root(&mut self, storage_key: ChildStorageKey<H>) -> Vec<u8> {
		if let Some(child) = self.children.get(storage_key.as_ref()) {
			let delta = child.clone().into_iter().map(|(k, v)| (k, Some(v)));

			InMemory::<H>::default().child_storage_root(storage_key.as_ref(), delta).0
		} else {
			vec![]
		}
	}

	fn storage_changes_root(&mut self, _parent: H::Out) -> Result<Option<H::Out>, ()> {
		Ok(None)
	}

	fn offchain(&mut self) -> Option<&mut dyn offchain::Externalities> {
		warn!("Call to non-existent out offchain externalities set.");
		None
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use primitives::{Blake2Hasher, H256, map};
	use primitives::storage::well_known_keys::CODE;
	use hex_literal::hex;

	#[test]
	fn commit_should_work() {
		let mut ext = BasicExternalities::default();
		let ext = &mut ext as &mut dyn Externalities<Blake2Hasher>;
		ext.set_storage(b"doe".to_vec(), b"reindeer".to_vec());
		ext.set_storage(b"dog".to_vec(), b"puppy".to_vec());
		ext.set_storage(b"dogglesworth".to_vec(), b"cat".to_vec());
		const ROOT: [u8; 32] = hex!("0b33ed94e74e0f8e92a55923bece1ed02d16cf424e124613ddebc53ac3eeeabe");
		assert_eq!(ext.storage_root(), H256::from(ROOT));
	}

	#[test]
	fn set_and_retrieve_code() {
		let mut ext = BasicExternalities::default();
		let ext = &mut ext as &mut dyn Externalities<Blake2Hasher>;

		let code = vec![1, 2, 3];
		ext.set_storage(CODE.to_vec(), code.clone());

		assert_eq!(&ext.storage(CODE).unwrap(), &code);
	}

	#[test]
	fn children_works() {
		let child_storage = b":child_storage:default:test".to_vec();

		let mut ext = BasicExternalities::new_with_children(
			Default::default(),
			map![
				child_storage.clone() => map![
					b"doe".to_vec() => b"reindeer".to_vec()
				]
			]
		);

		let ext = &mut ext as &mut dyn Externalities<Blake2Hasher>;

		let child = || ChildStorageKey::from_vec(child_storage.clone()).unwrap();

		assert_eq!(ext.child_storage(child(), b"doe"), Some(b"reindeer".to_vec()));

		ext.set_child_storage(child(), b"dog".to_vec(), b"puppy".to_vec());
		assert_eq!(ext.child_storage(child(), b"dog"), Some(b"puppy".to_vec()));

		ext.clear_child_storage(child(), b"dog");
		assert_eq!(ext.child_storage(child(), b"dog"), None);

		ext.kill_child_storage(child());
		assert_eq!(ext.child_storage(child(), b"doe"), None);
	}
}
