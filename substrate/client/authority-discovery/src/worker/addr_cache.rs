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
use core::fmt;
use futures::{channel::mpsc, executor::block_on, StreamExt};
use log::{debug, error, warn};
use sc_network::{
	multiaddr::{ParseError, Protocol},
	Multiaddr,
};
use sc_network_types::PeerId;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sp_authority_discovery::AuthorityId;
use sp_runtime::DeserializeOwned;
use std::{
	collections::{hash_map::Entry, HashMap, HashSet},
	fs::File,
	io::{self, BufReader, Write},
	path::PathBuf,
	sync::Arc,
	thread,
};

/// Cache for [`AuthorityId`] -> [`HashSet<Multiaddr>`] and [`PeerId`] -> [`HashSet<AuthorityId>`]
/// mappings.
pub(super) struct AddrCache {
	/// The addresses found in `authority_id_to_addresses` are guaranteed to always match
	/// the peerids found in `peer_id_to_authority_ids`. In other words, these two hashmaps
	/// are similar to a bi-directional map.
	///
	/// Since we may store the mapping across several sessions, a single
	/// `PeerId` might correspond to multiple `AuthorityId`s. However,
	/// it's not expected that a single `AuthorityId` can have multiple `PeerId`s.
	authority_id_to_addresses: HashMap<AuthorityId, HashSet<Multiaddr>>,
	peer_id_to_authority_ids: HashMap<PeerId, HashSet<AuthorityId>>,

	on_change: Option<OnAddrCacheChange>,
}
impl AddrCache {
	/// Clones all but the `on_change` handler (if any).
	fn clone_content(&self) -> Self {
		AddrCache {
			authority_id_to_addresses: self.authority_id_to_addresses.clone(),
			peer_id_to_authority_ids: self.peer_id_to_authority_ids.clone(),
			on_change: None,
		}
	}
}
impl std::fmt::Debug for AddrCache {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("AddrCache")
			.field("authority_id_to_addresses", &self.authority_id_to_addresses)
			.field("peer_id_to_authority_ids", &self.peer_id_to_authority_ids)
			.finish()
	}
}
impl PartialEq for AddrCache {
	fn eq(&self, other: &Self) -> bool {
		self.authority_id_to_addresses == other.authority_id_to_addresses &&
			self.peer_id_to_authority_ids == other.peer_id_to_authority_ids
	}
}
pub(super) type OnAddrCacheChange =
	Box<dyn Fn(AddrCache) -> Result<(), Box<dyn fmt::Debug>> + 'static>;

impl AddrCache {
	pub fn new() -> Self {
		AddrCache {
			authority_id_to_addresses: HashMap::new(),
			peer_id_to_authority_ids: HashMap::new(),
			on_change: None,
		}
	}

	pub fn install_on_change_callback<F>(&mut self, on_change: F)
	where
		F: Fn(AddrCache) -> Result<(), Box<dyn fmt::Debug>> + 'static,
	{
		self.on_change = Some(Box::new(on_change));
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

		self.notify_change_if_needed()
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

		self.notify_change_if_needed()
	}

	fn notify_change_if_needed(&self) {
		if let Some(on_change) = &self.on_change {
			match (on_change)(self.clone_content()) {
				Ok(()) => {},
				Err(err) => {
					log::error!(target: super::LOG_TARGET, "Error while notifying change: {:?}", err);
				},
			}
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

/// A (de)serializable version of the [`AddrCache`] that can be used for serialization,
/// implements Serialize and Deserialize traits, by holding variants of `Multiaddr` and `PeerId`
/// that can be encoded and decoded.
/// This is used for storing the cache in the database.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct SerializableAddrCache {
	authority_id_to_addresses: HashMap<AuthorityId, HashSet<SerializableMultiaddr>>,
	peer_id_to_authority_ids: HashMap<SerializablePeerId, HashSet<AuthorityId>>,
}
impl From<AddrCache> for SerializableAddrCache {
	fn from(addr_cache: AddrCache) -> Self {
		let authority_id_to_addresses = addr_cache
			.authority_id_to_addresses
			.into_iter()
			.map(|(authority_id, addresses)| {
				let addresses =
					addresses.into_iter().map(SerializableMultiaddr::from).collect::<HashSet<_>>();
				(authority_id, addresses)
			})
			.collect::<HashMap<_, _>>();

		let peer_id_to_authority_ids = addr_cache
			.peer_id_to_authority_ids
			.into_iter()
			.map(|(peer_id, authority_ids)| (SerializablePeerId::from(peer_id), authority_ids))
			.collect::<HashMap<_, _>>();

		SerializableAddrCache { authority_id_to_addresses, peer_id_to_authority_ids }
	}
}

impl TryFrom<SerializableAddrCache> for AddrCache {
	type Error = crate::Error;

	fn try_from(value: SerializableAddrCache) -> Result<Self, Self::Error> {
		let authority_id_to_addresses = value
			.authority_id_to_addresses
			.into_iter()
			.map(|(authority_id, addresses)| {
				let addresses = addresses
					.into_iter()
					.map(|ma| {
						Multiaddr::try_from(ma)
							.map_err(|e| Error::EncodingDecodingAddrCache(e.to_string()))
					})
					.collect::<Result<HashSet<Multiaddr>, Self::Error>>()?;
				Ok((authority_id, addresses))
			})
			.collect::<Result<HashMap<AuthorityId, HashSet<Multiaddr>>, Self::Error>>()?;

		let peer_id_to_authority_ids = value
			.peer_id_to_authority_ids
			.into_iter()
			.map(|(peer_id, authority_ids)| {
				let peer_id = PeerId::try_from(peer_id)?;
				Ok((peer_id, authority_ids.into_iter().collect::<HashSet<AuthorityId>>()))
			})
			.collect::<Result<HashMap<PeerId, HashSet<AuthorityId>>, Self::Error>>()?;

		Ok(AddrCache { authority_id_to_addresses, peer_id_to_authority_ids, on_change: None })
	}
}

/// A (de)serializable version of [`PeerId`] that can be used for serialization,
#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
struct SerializablePeerId {
	#[serde_as(as = "serde_with::hex::Hex")]
	bytes: Vec<u8>,
}

impl From<PeerId> for SerializablePeerId {
	fn from(peer_id: PeerId) -> Self {
		Self { bytes: peer_id.to_bytes() }
	}
}
impl TryFrom<SerializablePeerId> for PeerId {
	type Error = Error;

	fn try_from(value: SerializablePeerId) -> Result<Self, Self::Error> {
		PeerId::from_bytes(&value.bytes)
			.map_err(|e| Error::EncodingDecodingAddrCache(e.to_string()))
	}
}

/// A (de)serializable version of [`Multiaddr`] that can be used for serialization,
#[serde_as]
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
struct SerializableMultiaddr {
	/// `Multiaddr` holds a single `LiteP2pMultiaddr`, which holds `Arc<Vec<u8>>`.
	#[serde_as(as = "serde_with::hex::Hex")]
	bytes: Vec<u8>,
}
impl From<Multiaddr> for SerializableMultiaddr {
	fn from(multiaddr: Multiaddr) -> Self {
		Self { bytes: multiaddr.to_vec() }
	}
}
impl TryFrom<SerializableMultiaddr> for Multiaddr {
	type Error = ParseError;

	fn try_from(value: SerializableMultiaddr) -> Result<Self, Self::Error> {
		Self::try_from(value.bytes)
	}
}

/// Writes the content to a file at the specified path.
fn write_to_file(path: &PathBuf, content: &str) -> io::Result<()> {
	let mut file = File::create(path)?;
	file.write_all(content.as_bytes())?;
	file.flush()
}

/// Reads content from a file at the specified path and tries to JSON deserializes
/// it into the specified type.
fn load_from_file<T: DeserializeOwned>(path: &PathBuf) -> io::Result<T> {
	let file = File::open(path)?;
	let reader = BufReader::new(file);

	serde_json::from_reader(reader).map_err(|e| {
		error!(target: super::LOG_TARGET, "Failed to load from file: {}", e);
		io::Error::new(io::ErrorKind::InvalidData, e)
	})
}

/// Asynchronously writes content to a file using a background thread.
/// Multiple consecutive writes in quick succession (before the current write is completed)
/// will be **throttled**, and only the last write will be performed.
#[derive(Clone)]
pub struct ThrottlingAsyncFileWriter {
	/// Each request to write content will send a message to this sender.
	///
	/// N.B. this is not passed in as an argument, it is an implementation
	/// detail.
	sender: mpsc::UnboundedSender<String>,
}

impl ThrottlingAsyncFileWriter {
	/// Creates a new `ThrottlingAsyncFileWriter` for the specified file path,
	/// the label is used for logging purposes.
	pub fn new(purpose_label: String, path: impl Into<PathBuf>) -> Self {
		let path = Arc::new(path.into());
		let (sender, mut receiver) = mpsc::unbounded();

		let path_clone = Arc::clone(&path);
		thread::spawn(move || {
			let mut latest: Option<String>;

			while let Some(msg) = block_on(receiver.next()) {
				latest = Some(msg);
				while let Ok(Some(msg)) = receiver.try_next() {
					latest = Some(msg);
				}

				if let Some(ref content) = latest {
					if let Err(err) = write_to_file(&path_clone, content) {
						error!(target: super::LOG_TARGET, "Failed to write to file for {}, error: {}", purpose_label, err);
					}
				}
			}
		});

		Self { sender }
	}

	/// Write content to the file asynchronously, subsequent calls in quick succession
	/// will be throttled, and only the last write will be performed.
	///
	/// The content is written to the file specified by the path in the constructor.
	pub fn write(&self, content: impl Into<String>) {
		let _ = self.sender.unbounded_send(content.into());
	}

	/// Serialize the value and write it to the file, asynchronously and throttled.
	///
	/// This calls `write` after serializing the value to a pretty JSON string.
	pub fn write_serde<T: Serialize>(
		&self,
		value: &T,
	) -> std::result::Result<(), serde_json::Error> {
		let json = serde_json::to_string_pretty(value)?;
		self.write(json);
		Ok(())
	}
}

/// Load contents of persisted cache from file, if it exists, and is valid. Create a new one
/// otherwise, and install a callback to persist it on change.
pub(crate) fn create_addr_cache(persistence_path: PathBuf) -> AddrCache {
	// Try to load from cache on file it it exists and is valid.
	let mut addr_cache: AddrCache = load_from_file::<SerializableAddrCache>(&persistence_path)
		.map_err(|_|Error::EncodingDecodingAddrCache(format!("Failed to load AddrCache from file: {}", persistence_path.display())))
		.and_then(AddrCache::try_from).unwrap_or_else(|e| {
			warn!(target: super::LOG_TARGET, "Failed to load AddrCache from file, using empty instead, error: {}", e);
			AddrCache::new()
		});

	let async_file_writer =
		ThrottlingAsyncFileWriter::new("Persisted-AddrCache".to_owned(), &persistence_path);

	addr_cache.install_on_change_callback(move |cache| {
		let serializable = SerializableAddrCache::from(cache);
		debug!(target: super::LOG_TARGET, "Persisting AddrCache to file: {}", persistence_path.display());
		async_file_writer
			.write_serde(&serializable)
			.map_err(|e| Box::new(e) as Box<dyn std::fmt::Debug>)
	});

	addr_cache
}

#[cfg(test)]
mod tests {

	use std::{thread::sleep, time::Duration};

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

	/// Tests From<AddrCache> and TryFrom<SerializableAddrCache> implementations
	#[test]
	fn roundtrip_serializable_variant() {
		let sample = || AddrCache::sample();
		let serializable = SerializableAddrCache::from(sample());
		let from_serializable = AddrCache::try_from(serializable).unwrap();
		assert_eq!(sample(), from_serializable);
	}

	/// Tests JSON roundtrip is stable.
	#[test]
	fn serde_json() {
		let sample = || AddrCache::sample();
		let serializable = SerializableAddrCache::from(sample());
		let json = serde_json::to_string(&serializable).expect("Serialization should not fail");
		let deserialized = serde_json::from_str::<SerializableAddrCache>(&json).unwrap();
		let from_serializable = AddrCache::try_from(deserialized).unwrap();
		assert_eq!(sample(), from_serializable);
	}

	impl AddrCache {
		fn contains_authority_id(&self, id: &AuthorityId) -> bool {
			self.authority_id_to_addresses.contains_key(id)
		}
	}

	#[test]
	fn test_on_change_callback_on_insert() {
		use std::{cell::RefCell, rc::Rc};

		let called = Rc::new(RefCell::new(false));
		let mut sut = AddrCache::new();

		let new_authority = AuthorityPair::from_seed(&[0xbb; 32]).public();
		let called_clone = Rc::clone(&called);
		let new_authority_clone = new_authority.clone();
		sut.install_on_change_callback(move |changed| {
			*called_clone.borrow_mut() = true;
			assert!(changed.contains_authority_id(&new_authority_clone));
			Ok(())
		});
		assert!(!sut.contains_authority_id(&new_authority));

		sut.insert(
			new_authority.clone(),
			vec![Multiaddr::empty().with(Protocol::P2p(PeerId::random().into()))],
		);

		assert!(*called.borrow(), "on_change callback should be called after insert");
	}

	#[test]
	fn test_on_change_callback_on_retain() {
		use std::{cell::RefCell, rc::Rc};

		let called = Rc::new(RefCell::new(false));
		let mut sut = AddrCache::new();

		let authority_id = AuthorityPair::from_seed(&[0xbb; 32]).public();
		let called_clone = Rc::clone(&called);
		let authority_id_clone = authority_id.clone();
		sut.insert(
			authority_id.clone(),
			vec![Multiaddr::empty().with(Protocol::P2p(PeerId::random().into()))],
		);

		sut.install_on_change_callback(move |changed| {
			*called_clone.borrow_mut() = true;
			assert!(!changed.contains_authority_id(&authority_id_clone));
			Ok(())
		});
		assert!(sut.contains_authority_id(&authority_id));
		sut.retain_ids(&[]); // remove value keyed by `authority_id`

		assert!(*called.borrow(), "on_change callback should be called after insert");
	}

	#[test]
	fn deserialize_from_json() {
		let json = r#"
		{
			"authority_id_to_addresses": {
				"5GKfaFiY4UoCegBEw8ppnKL8kKv4X6jTq5CNfbYuxynrTsmA": [
					"a503220020d4968f78e5dd380759ef0532529367aae2e2040adb3b5bfba4e2dcd0f66005af"
				],
				"5F2Q58Tg8YKdg9YHUXwnWFBzq8ksuD1eBqY8szWSoPBgjT2J": [
					"a503220020d4968f78e5dd380759ef0532529367aae2e2040adb3b5bfba4e2dcd0f66005af"
				]
			},
			"peer_id_to_authority_ids": {
				"0020d4968f78e5dd380759ef0532529367aae2e2040adb3b5bfba4e2dcd0f66005af": [
					"5F2Q58Tg8YKdg9YHUXwnWFBzq8ksuD1eBqY8szWSoPBgjT2J",
					"5GKfaFiY4UoCegBEw8ppnKL8kKv4X6jTq5CNfbYuxynrTsmA"
				]
			}
		}
		"#;
		let deserialized = serde_json::from_str::<SerializableAddrCache>(json)
			.expect("Should be able to deserialize valid JSON into SerializableAddrCache");
		assert_eq!(deserialized.authority_id_to_addresses.len(), 2);
	}

	#[test]
	fn cache_is_persisted_on_change() {
		// ARRANGE
		let dir = tempfile::tempdir().expect("tempfile should create tmp dir");
		let path = dir.path().join("cache.json");
		let read_from_disk = || {
			sleep(Duration::from_millis(10)); // sleep short period to let `fs` complete writing to file.
			let read_from_path = load_from_file::<SerializableAddrCache>(&path).unwrap();
			AddrCache::try_from(read_from_path).unwrap()
		};

		let mut addr_cache = create_addr_cache(path.clone());
		let authority_id0 = AuthorityPair::generate().0.public();
		let authority_id1 = AuthorityPair::generate().0.public();

		// Test Insert
		{
			let peer_id = PeerId::random();
			let addr = Multiaddr::empty().with(Protocol::P2p(peer_id.into()));

			// ACT
			addr_cache.insert(authority_id0.clone(), vec![addr.clone()]);
			addr_cache.insert(authority_id1.clone(), vec![addr.clone()]);

			// ASSERT
			assert_eq!(2, read_from_disk().num_authority_ids());
		}

		// Test Insert
		{
			// ACT
			addr_cache.retain_ids(&[authority_id1]);
			addr_cache.retain_ids(&[]);

			// ASSERT
			assert_eq!(0, read_from_disk().num_authority_ids());
		}
	}

	#[test]
	fn test_load_cache_from_disc() {
		let dir = tempfile::tempdir().expect("tempfile should create tmp dir");
		let path = dir.path().join("cache.json");
		let sample = AddrCache::sample();
		assert_eq!(sample.num_authority_ids(), 2);
		let existing = serde_json::to_string(&SerializableAddrCache::from(sample)).unwrap();
		write_to_file(&path, &existing).unwrap();

		let cache = create_addr_cache(path);
		assert_eq!(cache.num_authority_ids(), 2);
	}
}
