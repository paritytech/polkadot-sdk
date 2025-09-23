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

use crate::error::Error;
use log::{info, warn};
use sc_network::{multiaddr::Protocol, Multiaddr};
use sc_network_types::PeerId;
use serde::{Deserialize, Serialize};
use sp_authority_discovery::AuthorityId;
use sp_runtime::DeserializeOwned;
use std::{
	collections::{hash_map::Entry, HashMap, HashSet},
	fs::File,
	io::{self, BufReader, Write},
	path::Path,
};

/// Cache for [`AuthorityId`] -> [`HashSet<Multiaddr>`] and [`PeerId`] -> [`HashSet<AuthorityId>`]
/// mappings.
#[derive(Default, Clone, PartialEq, Debug)]
pub(crate) struct AddrCache {
	/// The addresses found in `authority_id_to_addresses` are guaranteed to always match
	/// the peerids found in `peer_id_to_authority_ids`. In other words, these two hashmaps
	/// are similar to a bi-directional map.
	///
	/// Since we may store the mapping across several sessions, a single
	/// `PeerId` might correspond to multiple `AuthorityId`s. However,
	/// it's not expected that a single `AuthorityId` can have multiple `PeerId`s.
	authority_id_to_addresses: HashMap<AuthorityId, HashSet<Multiaddr>>,
	peer_id_to_authority_ids: HashMap<PeerId, HashSet<AuthorityId>>,
}

impl Serialize for AddrCache {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		SerializeAddrCache::from(self.clone()).serialize(serializer)
	}
}

impl<'de> Deserialize<'de> for AddrCache {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		SerializeAddrCache::deserialize(deserializer).map(Into::into)
	}
}

/// A storage and serialization time optimized version of `AddrCache`
/// which contains the bare minimum info to reconstruct the AddrCache. We
/// rely on the fact that the `peer_id_to_authority_ids` can be reconstructed from
/// the `authority_id_to_addresses` field.
///
/// Benchmarks show that this is about 2x faster to serialize and about 4x faster to deserialize
/// compared to the full `AddrCache`.
///
/// Storage wise it is about half the size of the full `AddrCache`.
///
/// This is used to persist the `AddrCache` to disk and load it back.
///
/// AddrCache impl of Serialize and Deserialize "piggybacks" on this struct.
#[derive(Serialize, Deserialize)]
struct SerializeAddrCache {
	authority_id_to_addresses: HashMap<AuthorityId, HashSet<Multiaddr>>,
}

impl From<SerializeAddrCache> for AddrCache {
	fn from(value: SerializeAddrCache) -> Self {
		let mut peer_id_to_authority_ids: HashMap<PeerId, HashSet<AuthorityId>> = HashMap::new();

		for (authority_id, addresses) in &value.authority_id_to_addresses {
			for peer_id in addresses_to_peer_ids(addresses) {
				peer_id_to_authority_ids
					.entry(peer_id)
					.or_insert_with(HashSet::new)
					.insert(authority_id.clone());
			}
		}

		AddrCache {
			authority_id_to_addresses: value.authority_id_to_addresses,
			peer_id_to_authority_ids,
		}
	}
}
impl From<AddrCache> for SerializeAddrCache {
	fn from(value: AddrCache) -> Self {
		Self { authority_id_to_addresses: value.authority_id_to_addresses }
	}
}

fn write_to_file(path: impl AsRef<Path>, contents: &str) -> io::Result<()> {
	let path = path.as_ref();
	let mut file = File::create(path)?;
	file.write_all(contents.as_bytes())?;
	file.flush()?;
	Ok(())
}

impl TryFrom<&Path> for AddrCache {
	type Error = Error;

	fn try_from(path: &Path) -> Result<Self, Self::Error> {
		// Try to load from the cache file if it exists and is valid.
		load_from_file::<AddrCache>(&path).map_err(|e| {
			Error::EncodingDecodingAddrCache(format!(
				"Failed to load AddrCache from file: {}, error: {:?}",
				path.display(),
				e
			))
		})
	}
}
impl AddrCache {
	pub fn new() -> Self {
		AddrCache::default()
	}

	fn serialize(&self) -> Option<String> {
		serde_json::to_string_pretty(self).inspect_err(|e| {
			warn!(target: super::LOG_TARGET, "Failed to serialize AddrCache to JSON: {} => skip persisting it.", e);
		}).ok()
	}

	fn persist(path: impl AsRef<Path>, serialized_cache: String) {
		match write_to_file(path.as_ref(), &serialized_cache) {
			Err(err) => {
				warn!(target: super::LOG_TARGET, "Failed to persist AddrCache on disk at path: {}, error: {}", path.as_ref().display(), err);
			},
			Ok(_) => {
				info!(target: super::LOG_TARGET, "Successfully persisted AddrCache on disk");
			},
		}
	}

	pub fn serialize_and_persist(&self, path: impl AsRef<Path>) {
		let Some(serialized) = self.serialize() else { return };
		Self::persist(path, serialized);
	}

	/// Inserts the given [`AuthorityId`] and [`Vec<Multiaddr>`] pair for future lookups by
	/// [`AuthorityId`] or [`PeerId`].
	pub fn insert(&mut self, authority_id: AuthorityId, addresses: Vec<Multiaddr>) {
		let addresses = addresses.into_iter().collect::<HashSet<_>>();
		let peer_ids = addresses_to_peer_ids(&addresses);

		if peer_ids.is_empty() {
			log::debug!(
				target: super::LOG_TARGET,
				"Authority({:?}) provides no addresses or addresses without peer ids. Adresses: {:?}",
				authority_id,
				addresses,
			);
			return
		} else if peer_ids.len() > 1 {
			log::warn!(
				target: super::LOG_TARGET,
				"Authority({:?}) can be reached through multiple peer ids: {:?}",
				authority_id,
				peer_ids
			);
		}

		log::debug!(
			target: super::LOG_TARGET,
			"Found addresses for authority {authority_id:?}: {addresses:?}",
		);

		let old_addresses = self.authority_id_to_addresses.insert(authority_id.clone(), addresses);
		let old_peer_ids = addresses_to_peer_ids(&old_addresses.unwrap_or_default());

		// Add the new peer ids
		peer_ids.difference(&old_peer_ids).for_each(|new_peer_id| {
			self.peer_id_to_authority_ids
				.entry(*new_peer_id)
				.or_default()
				.insert(authority_id.clone());
		});

		// Remove the old peer ids
		self.remove_authority_id_from_peer_ids(&authority_id, old_peer_ids.difference(&peer_ids));
	}

	/// Remove the given `authority_id` from the `peer_id` to `authority_ids` mapping.
	///
	/// If a `peer_id` doesn't have any `authority_id` assigned anymore, it is removed.
	fn remove_authority_id_from_peer_ids<'a>(
		&mut self,
		authority_id: &AuthorityId,
		peer_ids: impl Iterator<Item = &'a PeerId>,
	) {
		peer_ids.for_each(|peer_id| {
			if let Entry::Occupied(mut e) = self.peer_id_to_authority_ids.entry(*peer_id) {
				e.get_mut().remove(authority_id);

				// If there are no more entries, remove the peer id.
				if e.get().is_empty() {
					e.remove();
				}
			}
		})
	}

	/// Returns the number of authority IDs in the cache.
	pub fn num_authority_ids(&self) -> usize {
		self.authority_id_to_addresses.len()
	}

	/// Returns the addresses for the given [`AuthorityId`].
	pub fn get_addresses_by_authority_id(
		&self,
		authority_id: &AuthorityId,
	) -> Option<&HashSet<Multiaddr>> {
		self.authority_id_to_addresses.get(authority_id)
	}

	/// Returns the [`AuthorityId`]s for the given [`PeerId`].
	///
	/// As the authority id can change between sessions, one [`PeerId`] can be mapped to
	/// multiple authority ids.
	pub fn get_authority_ids_by_peer_id(&self, peer_id: &PeerId) -> Option<&HashSet<AuthorityId>> {
		self.peer_id_to_authority_ids.get(peer_id)
	}

	/// Removes all [`PeerId`]s and [`Multiaddr`]s from the cache that are not related to the given
	/// [`AuthorityId`]s.
	pub fn retain_ids(&mut self, authority_ids: &[AuthorityId]) {
		// The below logic could be replaced by `BtreeMap::drain_filter` once it stabilized.
		let authority_ids_to_remove = self
			.authority_id_to_addresses
			.iter()
			.filter(|(id, _addresses)| !authority_ids.contains(id))
			.map(|entry| entry.0)
			.cloned()
			.collect::<Vec<AuthorityId>>();

		for authority_id_to_remove in authority_ids_to_remove {
			// Remove other entries from `self.authority_id_to_addresses`.
			let addresses = if let Some(addresses) =
				self.authority_id_to_addresses.remove(&authority_id_to_remove)
			{
				addresses
			} else {
				continue
			};

			self.remove_authority_id_from_peer_ids(
				&authority_id_to_remove,
				addresses_to_peer_ids(&addresses).iter(),
			);
		}
	}
}

fn peer_id_from_multiaddr(addr: &Multiaddr) -> Option<PeerId> {
	addr.iter().last().and_then(|protocol| {
		if let Protocol::P2p(multihash) = protocol {
			PeerId::from_multihash(multihash).ok()
		} else {
			None
		}
	})
}

fn addresses_to_peer_ids(addresses: &HashSet<Multiaddr>) -> HashSet<PeerId> {
	addresses.iter().filter_map(peer_id_from_multiaddr).collect::<HashSet<_>>()
}

fn load_from_file<T: DeserializeOwned>(path: impl AsRef<Path>) -> io::Result<T> {
	let file = File::open(path)?;
	let reader = BufReader::new(file);

	serde_json::from_reader(reader).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {

	use std::{
		thread::sleep,
		time::{Duration, Instant},
	};

	use super::*;

	use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};
	use sc_network_types::multihash::{Code, Multihash};

	use sp_authority_discovery::{AuthorityId, AuthorityPair};
	use sp_core::crypto::Pair;

	#[derive(Clone, Debug)]
	struct TestAuthorityId(AuthorityId);

	impl Arbitrary for TestAuthorityId {
		fn arbitrary(g: &mut Gen) -> Self {
			let seed = (0..32).map(|_| u8::arbitrary(g)).collect::<Vec<_>>();
			TestAuthorityId(AuthorityPair::from_seed_slice(&seed).unwrap().public())
		}
	}

	#[derive(Clone, Debug)]
	struct TestMultiaddr(Multiaddr);

	impl Arbitrary for TestMultiaddr {
		fn arbitrary(g: &mut Gen) -> Self {
			let seed = (0..32).map(|_| u8::arbitrary(g)).collect::<Vec<_>>();
			let peer_id =
				PeerId::from_multihash(Multihash::wrap(Code::Sha2_256.into(), &seed).unwrap())
					.unwrap();
			let multiaddr = "/ip6/2001:db8:0:0:0:0:0:2/tcp/30333"
				.parse::<Multiaddr>()
				.unwrap()
				.with(Protocol::P2p(peer_id.into()));

			TestMultiaddr(multiaddr)
		}
	}

	#[derive(Clone, Debug)]
	struct TestMultiaddrsSamePeerCombo(Multiaddr, Multiaddr);

	impl Arbitrary for TestMultiaddrsSamePeerCombo {
		fn arbitrary(g: &mut Gen) -> Self {
			let seed = (0..32).map(|_| u8::arbitrary(g)).collect::<Vec<_>>();
			let peer_id =
				PeerId::from_multihash(Multihash::wrap(Code::Sha2_256.into(), &seed).unwrap())
					.unwrap();
			let multiaddr1 = "/ip6/2001:db8:0:0:0:0:0:2/tcp/30333"
				.parse::<Multiaddr>()
				.unwrap()
				.with(Protocol::P2p(peer_id.into()));
			let multiaddr2 = "/ip6/2002:db8:0:0:0:0:0:2/tcp/30133"
				.parse::<Multiaddr>()
				.unwrap()
				.with(Protocol::P2p(peer_id.into()));
			TestMultiaddrsSamePeerCombo(multiaddr1, multiaddr2)
		}
	}

	#[test]
	fn retains_only_entries_of_provided_authority_ids() {
		fn property(
			first: (TestAuthorityId, TestMultiaddr),
			second: (TestAuthorityId, TestMultiaddr),
			third: (TestAuthorityId, TestMultiaddr),
		) -> TestResult {
			let first: (AuthorityId, Multiaddr) = ((first.0).0, (first.1).0);
			let second: (AuthorityId, Multiaddr) = ((second.0).0, (second.1).0);
			let third: (AuthorityId, Multiaddr) = ((third.0).0, (third.1).0);

			let mut cache = AddrCache::new();

			cache.insert(first.0.clone(), vec![first.1.clone()]);
			cache.insert(second.0.clone(), vec![second.1.clone()]);
			cache.insert(third.0.clone(), vec![third.1.clone()]);

			assert_eq!(
				Some(&HashSet::from([third.1.clone()])),
				cache.get_addresses_by_authority_id(&third.0),
				"Expect `get_addresses_by_authority_id` to return addresses of third authority.",
			);
			assert_eq!(
				Some(&HashSet::from([third.0.clone()])),
				cache.get_authority_ids_by_peer_id(&peer_id_from_multiaddr(&third.1).unwrap()),
				"Expect `get_authority_id_by_peer_id` to return `AuthorityId` of third authority.",
			);

			cache.retain_ids(&vec![first.0.clone(), second.0]);

			assert_eq!(
				None,
				cache.get_addresses_by_authority_id(&third.0),
				"Expect `get_addresses_by_authority_id` to not return `None` for third authority.",
			);
			assert_eq!(
				None,
				cache.get_authority_ids_by_peer_id(&peer_id_from_multiaddr(&third.1).unwrap()),
				"Expect `get_authority_id_by_peer_id` to return `None` for third authority.",
			);

			TestResult::passed()
		}

		QuickCheck::new()
			.max_tests(10)
			.quickcheck(property as fn(_, _, _) -> TestResult)
	}

	#[test]
	fn test_from_to_serializable() {
		let serializable = SerializeAddrCache::from(AddrCache::sample());
		let roundtripped = AddrCache::from(serializable);
		assert_eq!(roundtripped, AddrCache::sample())
	}
	#[test]
	fn keeps_consistency_between_authority_id_and_peer_id() {
		fn property(
			authority1: TestAuthorityId,
			authority2: TestAuthorityId,
			multiaddr1: TestMultiaddr,
			multiaddr2: TestMultiaddr,
			multiaddr3: TestMultiaddrsSamePeerCombo,
		) -> TestResult {
			let authority1 = authority1.0;
			let authority2 = authority2.0;
			let multiaddr1 = multiaddr1.0;
			let multiaddr2 = multiaddr2.0;
			let TestMultiaddrsSamePeerCombo(multiaddr3, multiaddr4) = multiaddr3;

			let mut cache = AddrCache::new();

			cache.insert(authority1.clone(), vec![multiaddr1.clone()]);
			cache.insert(
				authority1.clone(),
				vec![multiaddr2.clone(), multiaddr3.clone(), multiaddr4.clone()],
			);

			assert_eq!(
				None,
				cache.get_authority_ids_by_peer_id(&peer_id_from_multiaddr(&multiaddr1).unwrap())
			);
			assert_eq!(
				Some(&HashSet::from([authority1.clone()])),
				cache.get_authority_ids_by_peer_id(&peer_id_from_multiaddr(&multiaddr2).unwrap())
			);
			assert_eq!(
				Some(&HashSet::from([authority1.clone()])),
				cache.get_authority_ids_by_peer_id(&peer_id_from_multiaddr(&multiaddr3).unwrap())
			);
			assert_eq!(
				Some(&HashSet::from([authority1.clone()])),
				cache.get_authority_ids_by_peer_id(&peer_id_from_multiaddr(&multiaddr4).unwrap())
			);

			cache.insert(authority2.clone(), vec![multiaddr2.clone()]);

			assert_eq!(
				Some(&HashSet::from([authority2.clone(), authority1.clone()])),
				cache.get_authority_ids_by_peer_id(&peer_id_from_multiaddr(&multiaddr2).unwrap())
			);
			assert_eq!(
				Some(&HashSet::from([authority1.clone()])),
				cache.get_authority_ids_by_peer_id(&peer_id_from_multiaddr(&multiaddr3).unwrap())
			);
			assert_eq!(cache.get_addresses_by_authority_id(&authority1).unwrap().len(), 3);

			cache.insert(authority2.clone(), vec![multiaddr2.clone(), multiaddr3.clone()]);

			assert_eq!(
				Some(&HashSet::from([authority2.clone(), authority1.clone()])),
				cache.get_authority_ids_by_peer_id(&peer_id_from_multiaddr(&multiaddr2).unwrap())
			);
			assert_eq!(
				Some(&HashSet::from([authority2.clone(), authority1.clone()])),
				cache.get_authority_ids_by_peer_id(&peer_id_from_multiaddr(&multiaddr3).unwrap())
			);
			assert_eq!(
				&HashSet::from([multiaddr2.clone(), multiaddr3.clone(), multiaddr4.clone()]),
				cache.get_addresses_by_authority_id(&authority1).unwrap(),
			);

			TestResult::passed()
		}

		QuickCheck::new()
			.max_tests(10)
			.quickcheck(property as fn(_, _, _, _, _) -> TestResult)
	}

	/// As the runtime gives us the current + next authority ids, it can happen that some
	/// authority changed its session keys. Changing the sessions keys leads to having two
	/// authority ids that map to the same `PeerId` & addresses.
	#[test]
	fn adding_two_authority_ids_for_the_same_peer_id() {
		let mut addr_cache = AddrCache::new();

		let peer_id = PeerId::random();
		let addr = Multiaddr::empty().with(Protocol::P2p(peer_id.into()));

		let authority_id0 = AuthorityPair::generate().0.public();
		let authority_id1 = AuthorityPair::generate().0.public();

		addr_cache.insert(authority_id0.clone(), vec![addr.clone()]);
		addr_cache.insert(authority_id1.clone(), vec![addr.clone()]);

		assert_eq!(2, addr_cache.num_authority_ids());
		assert_eq!(
			&HashSet::from([addr.clone()]),
			addr_cache.get_addresses_by_authority_id(&authority_id0).unwrap()
		);
		assert_eq!(
			&HashSet::from([addr]),
			addr_cache.get_addresses_by_authority_id(&authority_id1).unwrap()
		);
	}

	impl AddrCache {
		pub fn sample() -> Self {
			let mut addr_cache = AddrCache::new();

			let peer_id = PeerId::from_multihash(
				Multihash::wrap(Code::Sha2_256.into(), &[0xab; 32]).unwrap(),
			)
			.unwrap();
			let addr = Multiaddr::empty().with(Protocol::P2p(peer_id.into()));
			let authority_id0 = AuthorityPair::from_seed(&[0xaa; 32]).public();
			let authority_id1 = AuthorityPair::from_seed(&[0xbb; 32]).public();

			addr_cache.insert(authority_id0.clone(), vec![addr.clone()]);
			addr_cache.insert(authority_id1.clone(), vec![addr.clone()]);
			addr_cache
		}
	}

	#[test]
	fn serde_json() {
		let sample = || AddrCache::sample();
		let serializable = AddrCache::from(sample());
		let json = serde_json::to_string(&serializable).expect("Serialization should not fail");
		let deserialized = serde_json::from_str::<AddrCache>(&json).unwrap();
		let from_serializable = AddrCache::try_from(deserialized).unwrap();
		assert_eq!(sample(), from_serializable);
	}

	#[test]
	fn deserialize_from_json() {
		let json = r#"
		{
		  "authority_id_to_addresses": {
		    "5FjfMGrqw9ck5XZaPVTKm2RE5cbwoVUfXvSGZY7KCUEFtdr7": [
		      "/p2p/QmZtnFaddFtzGNT8BxdHVbQrhSFdq1pWxud5z4fA4kxfDt"
		    ],
		    "5DiQDBQvjFkmUF3C8a7ape5rpRPoajmMj44Q9CTGPfVBaa6U": [
		      "/p2p/QmZtnFaddFtzGNT8BxdHVbQrhSFdq1pWxud5z4fA4kxfDt"
		    ]
		  }
		}
		"#;
		let deserialized = serde_json::from_str::<AddrCache>(json).unwrap();
		assert_eq!(deserialized, AddrCache::sample())
	}

	fn serialize_and_write_to_file<T: Serialize>(
		path: impl AsRef<Path>,
		contents: &T,
	) -> io::Result<()> {
		let serialized = serde_json::to_string_pretty(contents).unwrap();
		write_to_file(path, &serialized)
	}

	#[test]
	fn test_load_cache_from_disc() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("cache.json");
		let sample = AddrCache::sample();
		assert_eq!(sample.num_authority_ids(), 2);
		serialize_and_write_to_file(&path, &sample).unwrap();
		sleep(Duration::from_millis(10)); // Ensure file is written before loading
		let cache = AddrCache::try_from(path.as_path()).unwrap();
		assert_eq!(cache.num_authority_ids(), 2);
	}

	fn create_cache(authority_id_count: u64, multiaddr_per_authority_count: u64) -> AddrCache {
		let mut addr_cache = AddrCache::new();

		for i in 0..authority_id_count {
			let seed = &mut [0xab as u8; 32];
			let i_bytes = i.to_le_bytes();
			seed[0..8].copy_from_slice(&i_bytes);

			let authority_id = AuthorityPair::from_seed(seed).public();
			let multi_addresses = (0..multiaddr_per_authority_count)
				.map(|j| {
					let mut digest = [0xab; 32];
					let j_bytes = j.to_le_bytes();
					digest[0..8].copy_from_slice(&j_bytes);
					let peer_id = PeerId::from_multihash(
						Multihash::wrap(Code::Sha2_256.into(), &digest).unwrap(),
					)
					.unwrap();
					Multiaddr::empty().with(Protocol::P2p(peer_id.into()))
				})
				.collect::<Vec<_>>();

			assert_eq!(multi_addresses.len(), multiaddr_per_authority_count as usize);
			addr_cache.insert(authority_id.clone(), multi_addresses);
		}
		assert_eq!(addr_cache.authority_id_to_addresses.len(), authority_id_count as usize);

		addr_cache
	}

	/// This test is ignored by default as it takes a long time to run.
	#[test]
	#[ignore]
	fn addr_cache_measure_serde_performance() {
		let addr_cache = create_cache(1000, 5);

		/// A replica of `AddrCache` that is serializable and deserializable
		/// without any optimizations.
		#[derive(Default, Clone, PartialEq, Debug, Serialize, Deserialize)]
		pub(crate) struct NaiveSerdeAddrCache {
			authority_id_to_addresses: HashMap<AuthorityId, HashSet<Multiaddr>>,
			peer_id_to_authority_ids: HashMap<PeerId, HashSet<AuthorityId>>,
		}
		impl From<AddrCache> for NaiveSerdeAddrCache {
			fn from(value: AddrCache) -> Self {
				Self {
					authority_id_to_addresses: value.authority_id_to_addresses,
					peer_id_to_authority_ids: value.peer_id_to_authority_ids,
				}
			}
		}

		let naive = NaiveSerdeAddrCache::from(addr_cache.clone());
		let storage_optimized = addr_cache.clone();

		fn measure_clone<T: Clone>(data: &T) -> Duration {
			let start = Instant::now();
			let _ = data.clone();
			start.elapsed()
		}
		fn measure_serialize<T: Serialize>(data: &T) -> (Duration, String) {
			let start = Instant::now();
			let json = serde_json::to_string_pretty(data).unwrap();
			(start.elapsed(), json)
		}
		fn measure_deserialize<T: DeserializeOwned>(json: String) -> (Duration, T) {
			let start = Instant::now();
			let value = serde_json::from_str(&json).unwrap();
			(start.elapsed(), value)
		}

		let serialize_naive = measure_serialize(&naive);
		let serialize_storage_optimized = measure_serialize(&storage_optimized);
		println!("CLONE: Naive took: {} ms", measure_clone(&naive).as_millis());
		println!(
			"CLONE: Storage optimized took: {} ms",
			measure_clone(&storage_optimized).as_millis()
		);
		println!("SERIALIZE: Naive took: {} ms", serialize_naive.0.as_millis());
		println!(
			"SERIALIZE: Storage optimized took: {} ms",
			serialize_storage_optimized.0.as_millis()
		);
		let deserialize_naive = measure_deserialize::<NaiveSerdeAddrCache>(serialize_naive.1);
		let deserialize_storage_optimized =
			measure_deserialize::<AddrCache>(serialize_storage_optimized.1);
		println!("DESERIALIZE: Naive took: {} ms", deserialize_naive.0.as_millis());
		println!(
			"DESERIALIZE: Storage optimized took: {} ms",
			deserialize_storage_optimized.0.as_millis()
		);
		assert_eq!(deserialize_naive.1, naive);
		assert_eq!(deserialize_storage_optimized.1, storage_optimized);
	}
}
