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
use sc_network::{
	multiaddr::{ParseError, Protocol},
	Multiaddr,
};
use sc_network_types::PeerId;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sp_authority_discovery::AuthorityId;
use std::collections::{hash_map::Entry, HashMap, HashSet};

/// Cache for [`AuthorityId`] -> [`HashSet<Multiaddr>`] and [`PeerId`] -> [`HashSet<AuthorityId>`]
/// mappings.
#[derive(Clone, Debug, Default, PartialEq)]
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
}

impl AddrCache {
	pub fn new() -> Self {
		AddrCache::default()
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

/// A (de)serializable version of the [`AddrCache`] that can be used for serialization,
/// implements Serialize and Deserialize traits, by holding variants of `Multiaddr` and `PeerId`
/// that can be encoded and decoded.
/// This is used for storing the cache in the database.
#[derive(Clone, Debug, Serialize, Deserialize)]
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

		Ok(AddrCache { authority_id_to_addresses, peer_id_to_authority_ids })
	}
}

/// A (de)serializable version of [`PeerId`] that can be used for serialization,
#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
struct SerializablePeerId {
	#[serde_as(as = "serde_with::hex::Hex")]
	encoded_peer_id: Vec<u8>,
}

impl From<PeerId> for SerializablePeerId {
	fn from(peer_id: PeerId) -> Self {
		Self { encoded_peer_id: peer_id.to_bytes() }
	}
}
impl TryFrom<SerializablePeerId> for PeerId {
	type Error = Error;

	fn try_from(value: SerializablePeerId) -> Result<Self, Self::Error> {
		PeerId::from_bytes(&value.encoded_peer_id)
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

#[cfg(test)]
mod tests {
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

	fn roundtrip_serializable_variant_with_transform_reverse<
		T,
		E: std::fmt::Debug,
	>(
		transform: impl Fn(SerializableAddrCache) -> T,
		reverse: impl Fn(T) -> Result<SerializableAddrCache, E>,
	) {
		let cache = {
			let mut addr_cache = AddrCache::new();

			let peer_id = PeerId::random();
			let addr = Multiaddr::empty().with(Protocol::P2p(peer_id.into()));

			let authority_id0 = AuthorityPair::generate().0.public();
			let authority_id1 = AuthorityPair::generate().0.public();

			addr_cache.insert(authority_id0.clone(), vec![addr.clone()]);
			addr_cache.insert(authority_id1.clone(), vec![addr.clone()]);
			addr_cache
		};
		let serializable = SerializableAddrCache::from(cache.clone());
		let transformed = transform(serializable);
		let reversed = reverse(transformed).expect("Decoding should not fail");
		let from_serializable = AddrCache::try_from(reversed).expect("Decoding should not fail");
		assert_eq!(cache, from_serializable);
	}

	#[test]
	fn roundtrip_serializable_variant() {
		#[derive(Debug)]
		struct NoError;
		// This tests only that From/TryFrom of SerializableAddrCache works.
		roundtrip_serializable_variant_with_transform_reverse::<SerializableAddrCache, NoError>(
			|s| s,
			|t| Ok(t),
		);
	}

	#[test]
	fn serde_json() {
		// This tests Serde JSON
		roundtrip_serializable_variant_with_transform_reverse(
			|s| serde_json::to_string_pretty(&s).expect("Serialization should work"),
			|t| serde_json::from_str(&t),
		);
	}

	#[test]
	fn deserialize_from_str() {
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

}
