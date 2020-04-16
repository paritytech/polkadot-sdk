// Copyright 2020 Parity Technologies (UK) Ltd.
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

//! Trie benchmark (integrated).

use std::{borrow::Cow, sync::Arc};
use kvdb::KeyValueDB;
use lazy_static::lazy_static;
use rand::Rng;
use hash_db::Prefix;
use sp_state_machine::Backend as _;

use node_primitives::Hash;

use crate::{
	core::{self, Mode, Path},
	generator::generate_trie,
	tempdb::TempDatabase,
};

pub const SAMPLE_SIZE: usize = 100;

pub type KeyValues = Vec<(Vec<u8>, Vec<u8>)>;

#[derive(Clone, Copy, Debug, derive_more::Display)]
pub enum DatabaseSize {
	#[display(fmt = "empty")]
	Empty,
	#[display(fmt = "smallest")]
	Smallest,
	#[display(fmt = "small")]
	Small,
	#[display(fmt = "medium")]
	Medium,
	#[display(fmt = "large")]
	Large,
	#[display(fmt = "largest")]
	Largest,
}

lazy_static! {
	static ref KUSAMA_STATE_DISTRIBUTION: SizePool =
		SizePool::from_histogram(crate::state_sizes::KUSAMA_STATE_DISTRIBUTION);
}

impl DatabaseSize {
	/// Should be multiple of SAMPLE_SIZE!
	fn keys(&self) -> usize {
		let val = match *self {
			Self::Empty => 200, // still need some keys to query
			Self::Smallest => 1_000,
			Self::Small => 10_000,
			Self::Medium => 100_000,
			Self::Large => 200_000,
			Self::Largest => 1_000_000,
		};

		assert_eq!(val % SAMPLE_SIZE, 0);

		val
	}
}

pub struct TrieBenchmarkDescription {
	pub database_size: DatabaseSize,
}

pub struct TrieBenchmark {
	database: TempDatabase,
	root: Hash,
	warmup_keys: KeyValues,
	query_keys: KeyValues,
}

impl core::BenchmarkDescription for TrieBenchmarkDescription {
	fn path(&self) -> Path {
		let mut path = Path::new(&["trie"]);
		path.push(&format!("{}", self.database_size));
		path
	}

	fn setup(self: Box<Self>) -> Box<dyn core::Benchmark> {
		let mut database = TempDatabase::new();

		// TODO: make seedable
		let mut rng = rand::thread_rng();
		let warmup_prefix = KUSAMA_STATE_DISTRIBUTION.key(&mut rng);

		let mut key_values = KeyValues::new();
		let mut warmup_keys = KeyValues::new();
		let mut query_keys = KeyValues::new();
		let every_x_key = self.database_size.keys() / SAMPLE_SIZE;
		for idx in 0..self.database_size.keys() {
			let kv = (
				KUSAMA_STATE_DISTRIBUTION.key(&mut rng).to_vec(),
				KUSAMA_STATE_DISTRIBUTION.value(&mut rng),
			);
			if idx % every_x_key == 0 {
				// warmup keys go to separate tree with high prob
				let mut actual_warmup_key = warmup_prefix.clone();
				actual_warmup_key[16..].copy_from_slice(&kv.0[16..]);
				warmup_keys.push((actual_warmup_key.clone(), kv.1.clone()));
				key_values.push((actual_warmup_key.clone(), kv.1.clone()));
			} else if idx % every_x_key == 1 {
				query_keys.push(kv.clone());
			}

			key_values.push(kv)
		}

		assert_eq!(warmup_keys.len(), SAMPLE_SIZE);
		assert_eq!(query_keys.len(), SAMPLE_SIZE);

		let root = generate_trie(
			database.open(),
			key_values,
		);

		Box::new(TrieBenchmark {
			database,
			root,
			warmup_keys,
			query_keys,
		})
	}

	fn name(&self) -> Cow<'static, str> {

		fn pretty_print(v: usize) -> String {
			let mut print = String::new();
			for (idx, val) in v.to_string().chars().rev().enumerate() {
				if idx != 0 && idx % 3 == 0 {
					print.insert(0, ',');
				}
				print.insert(0, val);
			}
			print
		}

		format!(
			"Trie benchmark({} database ({} keys))",
			self.database_size,
			pretty_print(self.database_size.keys()),
		).into()
	}
}

struct Storage(Arc<dyn KeyValueDB>);

impl sp_state_machine::Storage<sp_core::Blake2Hasher> for Storage {
	fn get(&self, key: &Hash, prefix: Prefix) -> Result<Option<Vec<u8>>, String> {
		let key = sp_trie::prefixed_key::<sp_core::Blake2Hasher>(key, prefix);
		self.0.get(0, &key).map_err(|e| format!("Database backend error: {:?}", e))
	}
}

impl core::Benchmark for TrieBenchmark {
	fn run(&mut self, mode: Mode) -> std::time::Duration {
		let mut db = self.database.clone();
		let storage: Arc<dyn sp_state_machine::Storage<sp_core::Blake2Hasher>> =
		Arc::new(Storage(db.open()));

		let trie_backend = sp_state_machine::TrieBackend::new(
			storage,
			self.root,
		);
		for (warmup_key, warmup_value) in self.warmup_keys.iter() {
			let value = trie_backend.storage(&warmup_key[..])
				.expect("Failed to get key: db error")
				.expect("Warmup key should exist");

			// sanity for warmup keys
			assert_eq!(&value, warmup_value);
		}

		if mode == Mode::Profile {
			std::thread::park_timeout(std::time::Duration::from_secs(3));
		}

		let started = std::time::Instant::now();
		for (key, _) in self.query_keys.iter() {
			let _ = trie_backend.storage(&key[..]);
		}
		let elapsed = started.elapsed();

		if mode == Mode::Profile {
			std::thread::park_timeout(std::time::Duration::from_secs(1));
		}

		elapsed / (SAMPLE_SIZE as u32)
	}
}

struct SizePool {
	distribution: std::collections::BTreeMap<u32, u32>,
	total: u32,
}

impl SizePool {
	fn from_histogram(h: &[(u32, u32)]) -> SizePool {
		let mut distribution = std::collections::BTreeMap::default();
		let mut total = 0;
		for (size, count) in h {
			total += count;
			distribution.insert(total, *size);
		}
		SizePool { distribution, total }
	}

	fn value<R: Rng>(&self, rng: &mut R) -> Vec<u8> {
		let sr = (rng.next_u64() % self.total as u64) as u32;
		let mut range = self.distribution.range((std::ops::Bound::Included(sr), std::ops::Bound::Unbounded));
		let size = *range.next().unwrap().1 as usize;
		let mut v = Vec::new();
		v.resize(size, 0);
		rng.fill_bytes(&mut v);
		v
	}

	fn key<R: Rng>(&self, rng: &mut R) -> Vec<u8> {
		let mut key = [0u8; 32];
		rng.fill_bytes(&mut key[..]);
		key.to_vec()
	}
}