// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Logic for working with storage proofs.

use frame_support::{
	sp_runtime::{SaturatedConversion, StateVersion},
	traits::TrackedStorageKey,
	PalletError,
};
use sp_core::RuntimeDebug;
use sp_std::{default::Default, vec, vec::Vec};
use sp_trie::{
	accessed_nodes_tracker::AccessedNodesTracker, generate_trie_proof, read_trie_value,
	verify_trie_proof, LayoutV0, LayoutV1, MemoryDB, StorageProof, Trie, TrieDBBuilder, TrieHash,
};

use codec::{Decode, Encode};
use hash_db::{HashDB, Hasher, EMPTY_PREFIX};
use scale_info::TypeInfo;
#[cfg(feature = "test-helpers")]
use sp_trie::{recorder_ext::RecorderExt, Recorder, TrieError};
use trie_db::DBValue;
#[cfg(feature = "test-helpers")]
use trie_db::{TrieConfiguration, TrieDBMut};

use crate::Size;

/// Errors that can occur when interacting with `UnverifiedStorageProof` and `VerifiedStorageProof`.
#[derive(Clone, Encode, Decode, RuntimeDebug, PartialEq, Eq, PalletError, TypeInfo)]
pub enum StorageProofError {
	/// Call to `generate_trie_proof()` failed.
	UnableToGenerateTrieProof,
	/// Call to `verify_trie_proof()` failed.
	InvalidProof,
	/// The `Vec` entries weren't sorted as expected.
	UnsortedEntries,
	/// The provided key wasn't found.
	UnavailableKey,
	/// The value associated to the provided key is `None`.
	EmptyVal,
	/// Error decoding value associated to a provided key.
	DecodeError,
	/// At least one key or node wasn't read.
	UnusedKey,

	/// Expected storage root is missing from the proof. (for non-compact proofs)
	StorageRootMismatch,
	/// Unable to reach expected storage value using provided trie nodes. (for non-compact proofs)
	StorageValueUnavailable,
	/// The proof contains duplicate nodes. (for non-compact proofs)
	DuplicateNodes,
}

impl From<sp_trie::StorageProofError> for StorageProofError {
	fn from(e: sp_trie::StorageProofError) -> Self {
		match e {
			sp_trie::StorageProofError::DuplicateNodes => StorageProofError::DuplicateNodes,
		}
	}
}

impl From<sp_trie::accessed_nodes_tracker::Error> for StorageProofError {
	fn from(e: sp_trie::accessed_nodes_tracker::Error) -> Self {
		match e {
			sp_trie::accessed_nodes_tracker::Error::UnusedNodes => StorageProofError::UnusedKey,
		}
	}
}

/// Raw storage proof type (just raw trie nodes).
pub type RawStorageProof = sp_trie::RawStorageProof;

/// Calculates size for `RawStorageProof`.
pub fn raw_storage_proof_size(raw_storage_proof: &RawStorageProof) -> usize {
	raw_storage_proof
		.iter()
		.fold(0usize, |sum, node| sum.saturating_add(node.len()))
}

/// Storage values size requirements.
///
/// This is currently used by benchmarks when generating storage proofs.
#[cfg(feature = "test-helpers")]
#[derive(Clone, Copy, Debug, Default)]
pub struct UnverifiedStorageProofParams {
	/// Expected storage proof size in bytes.
	pub db_size: Option<u32>,
}

#[cfg(feature = "test-helpers")]
impl UnverifiedStorageProofParams {
	/// Make storage proof parameters that require proof of at least `db_size` bytes.
	pub fn from_db_size(db_size: u32) -> Self {
		Self { db_size: Some(db_size) }
	}
}

/// This struct is used to read storage values from a subset of a Merklized database. The "proof"
/// is a subset of the nodes in the Merkle structure of the database, so that it provides
/// authentication against a known Merkle root as well as the values in the
/// database themselves.
pub struct StorageProofChecker<H>
where
	H: Hasher,
{
	root: H::Out,
	db: MemoryDB<H>,
	accessed_nodes_tracker: AccessedNodesTracker<H::Out>,
}

impl<H> StorageProofChecker<H>
where
	H: Hasher,
{
	/// Constructs a new storage proof checker.
	///
	/// This returns an error if the given proof is invalid with respect to the given root.
	pub fn new(root: H::Out, proof: RawStorageProof) -> Result<Self, StorageProofError> {
		let proof = StorageProof::new_with_duplicate_nodes_check(proof)?;

		let recorder = AccessedNodesTracker::new(proof.len());

		let db = proof.into_memory_db();
		if !db.contains(&root, EMPTY_PREFIX) {
			return Err(StorageProofError::StorageRootMismatch)
		}

		Ok(StorageProofChecker { root, db, accessed_nodes_tracker: recorder })
	}

	/// Returns error if the proof has some nodes that are left intact by previous `read_value`
	/// calls.
	pub fn ensure_no_unused_nodes(self) -> Result<(), StorageProofError> {
		self.accessed_nodes_tracker.ensure_no_unused_nodes().map_err(Into::into)
	}

	/// Reads a value from the available subset of storage. If the value cannot be read due to an
	/// incomplete or otherwise invalid proof, this function returns an error.
	pub fn read_value(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageProofError> {
		// LayoutV1 or LayoutV0 is identical for proof that only read values.
		read_trie_value::<LayoutV1<H>, _>(
			&self.db,
			&self.root,
			key,
			Some(&mut self.accessed_nodes_tracker),
			None,
		)
		.map_err(|_| StorageProofError::StorageValueUnavailable)
	}

	/// Reads and decodes a value from the available subset of storage. If the value cannot be read
	/// due to an incomplete or otherwise invalid proof, this function returns an error. If value is
	/// read, but decoding fails, this function returns an error.
	pub fn read_and_decode_value<T: Decode>(
		&mut self,
		key: &[u8],
	) -> Result<Option<T>, StorageProofError> {
		self.read_value(key).and_then(|v| {
			v.map(|v| {
				T::decode(&mut &v[..]).map_err(|e| {
					log::warn!(target: "bridge-storage-proofs", "read_and_decode_value error: {e:?}");
					StorageProofError::DecodeError
				})
			})
			.transpose()
		})
	}

	/// Reads and decodes a value from the available subset of storage. If the value cannot be read
	/// due to an incomplete or otherwise invalid proof, or if the value is `None`, this function
	/// returns an error. If value is read, but decoding fails, this function returns an error.
	pub fn read_and_decode_mandatory_value<T: Decode>(
		&mut self,
		key: &[u8],
	) -> Result<T, StorageProofError> {
		self.read_and_decode_value(key)?.ok_or(StorageProofError::EmptyVal)
	}

	/// Reads and decodes a value from the available subset of storage. If the value cannot be read
	/// due to an incomplete or otherwise invalid proof, this function returns `Ok(None)`.
	/// If value is read, but decoding fails, this function returns an error.
	pub fn read_and_decode_opt_value<T: Decode>(
		&mut self,
		key: &[u8],
	) -> Result<Option<T>, StorageProofError> {
		match self.read_and_decode_value(key) {
			Ok(outbound_lane_data) => Ok(outbound_lane_data),
			Err(StorageProofError::StorageValueUnavailable) => Ok(None),
			Err(e) => Err(e),
		}
	}
}

pub type RawStorageKey = Vec<u8>;

/// Structure representing a key-value database stored as a sorted `Vec` of tuples.
///
/// The structure also contains a proof of the fact that the key-value tuples are actually present
/// in the chain storage.
#[derive(Clone, Default, Decode, Encode, Eq, PartialEq, RuntimeDebug, TypeInfo)]
pub struct UnverifiedStorageProof {
	proof: RawStorageProof,
	db: Vec<(RawStorageKey, Option<DBValue>)>,
}

impl UnverifiedStorageProof {
	/// Creates a new instance of `UnverifiedStorageProof`.
	pub fn try_new<H: Hasher>(
		read_proof: StorageProof,
		root: TrieHash<LayoutV1<H>>,
		mut keys: Vec<impl AsRef<[u8]> + Ord>,
	) -> Result<Self, StorageProofError> {
		// It's ok to use `LayoutV1` in this function, no matter the actual underlying layout,
		// because we only perform read operations. When reading `LayoutV0` and `LayoutV1` lead to
		// the same result.
		let mem_db = read_proof.into_memory_db();
		let trie_db = TrieDBBuilder::<LayoutV1<H>>::new(&mem_db, &root).build();

		let trie_proof = generate_trie_proof::<LayoutV1<H>, _, _, _>(&mem_db, root, &keys)
			.map_err(|_| StorageProofError::UnableToGenerateTrieProof)?;

		let mut entries = Vec::with_capacity(keys.len());
		keys.sort();
		for key in keys {
			let val = trie_db.get(key.as_ref()).map_err(|_| StorageProofError::UnavailableKey)?;
			entries.push((key.as_ref().to_vec(), val));
		}

		Ok(Self { proof: trie_proof, db: entries })
	}

	/// Creates a new instance of `UnverifiedStorageProof` from the provided entries.
	///
	/// **This function is used only in tests and benchmarks.**
	#[cfg(any(all(feature = "std", feature = "test-helpers"), test))]
	pub fn try_from_entries<H: Hasher>(
		state_version: StateVersion,
		entries: &[(RawStorageKey, Option<DBValue>)],
	) -> Result<(H::Out, UnverifiedStorageProof), StorageProofError>
	where
		H::Out: codec::Codec,
	{
		let keys: Vec<_> = entries.iter().map(|(key, _)| key.clone()).collect();
		let entries: Vec<_> =
			entries.iter().cloned().map(|(key, val)| (None, vec![(key, val)])).collect();
		let backend = sp_state_machine::TrieBackend::<sp_trie::PrefixedMemoryDB<H>, H>::from((
			entries,
			state_version,
		));
		let root = *backend.root();

		Ok((root, UnverifiedStorageProof::try_from_db(backend.backend_storage(), root, keys)?))
	}

	/// Creates a new instance of `UnverifiedStorageProof` from the provided db.
	///
	/// **This function is used only in tests and benchmarks.**
	#[cfg(any(feature = "test-helpers", test))]
	pub fn try_from_db<H: Hasher, DB>(
		db: &DB,
		root: H::Out,
		keys: Vec<impl AsRef<[u8]> + Ord>,
	) -> Result<UnverifiedStorageProof, StorageProofError>
	where
		DB: hash_db::HashDBRef<H, DBValue>,
	{
		use sp_std::collections::btree_set::BTreeSet;

		let mut recorder = Recorder::<LayoutV1<H>>::new();
		let trie = TrieDBBuilder::<LayoutV1<H>>::new(db, &root)
			.with_recorder(&mut recorder)
			.build();
		for key in &keys {
			trie.get(key.as_ref()).map_err(|_| StorageProofError::UnavailableKey)?;
		}

		let raw_read_proof: Vec<_> = recorder
			.drain()
			.into_iter()
			.map(|n| n.data)
			// recorder may record the same trie node multiple times and we don't want duplicate
			// nodes in our proofs => let's deduplicate it by collecting to the BTreeSet first
			.collect::<BTreeSet<_>>()
			.into_iter()
			.collect();

		UnverifiedStorageProof::try_new::<H>(StorageProof::new(raw_read_proof), root, keys)
	}

	/// Validates the contained `db` against the contained proof. If the `db` is valid, converts it
	/// into a `VerifiedStorageProof`.
	pub fn verify<H: Hasher>(
		mut self,
		state_version: StateVersion,
		state_root: &TrieHash<LayoutV1<H>>,
	) -> Result<VerifiedStorageProof, StorageProofError> {
		// First we verify the proof for the `UnverifiedStorageProof`.
		// Note that `verify_trie_proof()` also checks for duplicate keys and unused nodes.
		match state_version {
			StateVersion::V0 =>
				verify_trie_proof::<LayoutV0<H>, _, _, _>(state_root, &self.proof, &self.db),
			StateVersion::V1 =>
				verify_trie_proof::<LayoutV1<H>, _, _, _>(state_root, &self.proof, &self.db),
		}
		.map_err(|e| {
			log::warn!(
				target:
				"bridge-storage-proofs", "UnverifiedStorageProof::verify error: {:?}",
				match e {
					sp_trie::VerifyError::DuplicateKey(_) => "DuplicateKey",
					sp_trie::VerifyError::ExtraneousNode => "ExtraneousNode",
					sp_trie::VerifyError::ExtraneousValue(_) => "ExtraneousValue",
					sp_trie::VerifyError::ExtraneousHashReference(_) => "ExtraneousHashReference",
					sp_trie::VerifyError::InvalidChildReference(_) => "InvalidChildReference",
					sp_trie::VerifyError::ValueMismatch(_) => "ValueMismatch",
					sp_trie::VerifyError::IncompleteProof => "IncompleteProof",
					sp_trie::VerifyError::RootMismatch(_) => "RootMismatch",
					sp_trie::VerifyError::DecodeError(_) => "DecodeError",
				}
			);
			StorageProofError::InvalidProof
		})?;

		// Fill the `VerifiedStorageProof`
		let mut trusted_db = Vec::with_capacity(self.db.len());
		let mut iter = self.db.drain(..).peekable();
		while let Some((key, val)) = iter.next() {
			// Let's also make sure that the db is actually sorted.
			if let Some((next_key, _)) = iter.peek() {
				if next_key <= &key {
					return Err(StorageProofError::UnsortedEntries)
				}
			}
			trusted_db.push((TrackedStorageKey::new(key), val))
		}
		Ok(VerifiedStorageProof { db: trusted_db })
	}

	/// Getter for proof
	pub fn proof(&self) -> &RawStorageProof {
		&self.proof
	}
}

impl Size for UnverifiedStorageProof {
	fn size(&self) -> u32 {
		let proof_size = raw_storage_proof_size(&self.proof);
		let entries_size = self.db.iter().fold(0usize, |sum, (key, value)| {
			sum.saturating_add(key.len())
				.saturating_add(value.as_ref().unwrap_or(&vec![]).len())
		});

		proof_size.saturating_add(entries_size).saturated_into()
	}
}

/// Structure representing a key-value database stored as a sorted `Vec` of tuples.
pub struct VerifiedStorageProof {
	db: Vec<(TrackedStorageKey, Option<DBValue>)>,
}

impl VerifiedStorageProof {
	/// Returns a reference to the value corresponding to the key.
	///
	/// Returns an error if the key doesn't exist.
	pub fn get(&mut self, key: &impl AsRef<[u8]>) -> Result<&Option<DBValue>, StorageProofError> {
		let idx = self
			.db
			.binary_search_by(|(db_key, _)| db_key.key.as_slice().cmp(key.as_ref()))
			.map_err(|_| StorageProofError::UnavailableKey)?;
		let (db_key, db_val) = self.db.get_mut(idx).ok_or(StorageProofError::UnavailableKey)?;
		db_key.add_read();
		Ok(db_val)
	}

	/// Returns a reference to the value corresponding to the key.
	///
	/// Returns an error if the key doesn't exist or if the value associated to it is `None`.
	pub fn get_and_decode_mandatory<D: Decode>(
		&mut self,
		key: &impl AsRef<[u8]>,
	) -> Result<D, StorageProofError> {
		let val = self.get(key)?.as_ref().ok_or(StorageProofError::EmptyVal)?;
		D::decode(&mut &val[..]).map_err(|e| {
			log::warn!(target: "bridge-storage-proofs", "get_and_decode_mandatory error: {e:?}");
			StorageProofError::DecodeError
		})
	}

	/// Returns a reference to the value corresponding to the key.
	///
	/// Returns `None` if the key doesn't exist or if the value associated to it is `None`.
	pub fn get_and_decode_optional<D: Decode>(
		&mut self,
		key: &impl AsRef<[u8]>,
	) -> Result<Option<D>, StorageProofError> {
		match self.get_and_decode_mandatory(key) {
			Ok(val) => Ok(Some(val)),
			Err(StorageProofError::UnavailableKey | StorageProofError::EmptyVal) => Ok(None),
			Err(e) => Err(e),
		}
	}

	/// Checks if each key was read.
	pub fn ensure_no_unused_keys(&self) -> Result<(), StorageProofError> {
		for (key, _) in &self.db {
			if !key.has_been_read() {
				return Err(StorageProofError::UnusedKey)
			}
		}

		Ok(())
	}
}

/// Add extra data to the storage value so that it'll be of given size.
#[cfg(feature = "test-helpers")]
pub fn grow_storage_value(mut value: Vec<u8>, params: &UnverifiedStorageProofParams) -> Vec<u8> {
	if let Some(db_size) = params.db_size {
		if db_size as usize > value.len() {
			value.extend(sp_std::iter::repeat(42u8).take(db_size as usize - value.len()));
		}
	}
	value
}

/// Insert values in the provided trie at common-prefix keys in order to inflate the resulting
/// storage proof.
///
/// This function can add at most 15 common-prefix keys per prefix nibble (4 bits).
/// Each such key adds about 33 bytes (a node) to the proof.
#[cfg(feature = "test-helpers")]
pub fn grow_storage_proof<L: TrieConfiguration>(
	trie: &mut TrieDBMut<L>,
	prefix: Vec<u8>,
	num_extra_nodes: usize,
) {
	use sp_trie::TrieMut;

	let mut added_nodes = 0;
	for i in 0..prefix.len() {
		let mut prefix = prefix[0..=i].to_vec();
		// 1 byte has 2 nibbles (4 bits each)
		let first_nibble = (prefix[i] & 0xf0) >> 4;
		let second_nibble = prefix[i] & 0x0f;

		// create branches at the 1st nibble
		for branch in 1..=15 {
			if added_nodes >= num_extra_nodes {
				return
			}

			// create branches at the 1st nibble
			prefix[i] = (first_nibble.wrapping_add(branch) % 16) << 4;
			trie.insert(&prefix, &[0; 32])
				.map_err(|_| "TrieMut::insert has failed")
				.expect("TrieMut::insert should not fail in benchmarks");
			added_nodes += 1;
		}

		// create branches at the 2nd nibble
		for branch in 1..=15 {
			if added_nodes >= num_extra_nodes {
				return
			}

			prefix[i] = (first_nibble << 4) | (second_nibble.wrapping_add(branch) % 16);
			trie.insert(&prefix, &[0; 32])
				.map_err(|_| "TrieMut::insert has failed")
				.expect("TrieMut::insert should not fail in benchmarks");
			added_nodes += 1;
		}
	}

	assert_eq!(added_nodes, num_extra_nodes)
}

/// Record all keys for a given root.
#[cfg(feature = "test-helpers")]
pub fn record_all_keys<L: TrieConfiguration, DB>(
	db: &DB,
	root: &TrieHash<L>,
) -> Result<RawStorageProof, sp_std::boxed::Box<TrieError<L>>>
where
	DB: hash_db::HashDBRef<L::Hash, trie_db::DBValue>,
{
	let mut recorder = Recorder::<L>::new();
	let trie = TrieDBBuilder::<L>::new(db, root).with_recorder(&mut recorder).build();
	for x in trie.iter()? {
		let (key, _) = x?;
		trie.get(&key)?;
	}

	Ok(recorder.into_raw_storage_proof())
}

/// Return valid storage proof and state root.
///
/// NOTE: This should only be used for **testing**.
#[cfg(feature = "std")]
pub fn craft_valid_storage_proof() -> (sp_core::H256, RawStorageProof) {
	use sp_state_machine::{backend::Backend, prove_read, InMemoryBackend};

	let state_version = sp_runtime::StateVersion::default();

	// construct storage proof
	let backend = <InMemoryBackend<sp_core::Blake2Hasher>>::from((
		sp_std::vec![
			(None, vec![(b"key1".to_vec(), Some(b"value1".to_vec()))]),
			(None, vec![(b"key2".to_vec(), Some(b"value2".to_vec()))]),
			(None, vec![(b"key3".to_vec(), Some(b"value3".to_vec()))]),
			(None, vec![(b"key4".to_vec(), Some((42u64, 42u32, 42u16, 42u8).encode()))]),
			// Value is too big to fit in a branch node
			(None, vec![(b"key11".to_vec(), Some(vec![0u8; 32]))]),
		],
		state_version,
	));
	let root = backend.storage_root(sp_std::iter::empty(), state_version).0;
	let proof =
		prove_read(backend, &[&b"key1"[..], &b"key2"[..], &b"key4"[..], &b"key22"[..]]).unwrap();

	(root, proof.into_nodes().into_iter().collect())
}

#[cfg(test)]
pub mod tests_for_storage_proof_checker {
	use super::*;
	use codec::Encode;

	#[test]
	fn storage_proof_check() {
		let (root, proof) = craft_valid_storage_proof();

		// check proof in runtime
		let mut checker =
			<StorageProofChecker<sp_core::Blake2Hasher>>::new(root, proof.clone()).unwrap();
		assert_eq!(checker.read_value(b"key1"), Ok(Some(b"value1".to_vec())));
		assert_eq!(checker.read_value(b"key2"), Ok(Some(b"value2".to_vec())));
		assert_eq!(checker.read_value(b"key4"), Ok(Some((42u64, 42u32, 42u16, 42u8).encode())));
		assert_eq!(
			checker.read_value(b"key11111"),
			Err(StorageProofError::StorageValueUnavailable)
		);
		assert_eq!(checker.read_value(b"key22"), Ok(None));
		assert_eq!(checker.read_and_decode_value(b"key4"), Ok(Some((42u64, 42u32, 42u16, 42u8))),);
		assert!(matches!(
			checker.read_and_decode_value::<[u8; 64]>(b"key4"),
			Err(StorageProofError::DecodeError),
		));

		// checking proof against invalid commitment fails
		assert_eq!(
			<StorageProofChecker<sp_core::Blake2Hasher>>::new(sp_core::H256::random(), proof).err(),
			Some(StorageProofError::StorageRootMismatch)
		);
	}

	#[test]
	fn proof_with_unused_items_is_rejected() {
		let (root, proof) = craft_valid_storage_proof();

		let mut checker =
			StorageProofChecker::<sp_core::Blake2Hasher>::new(root, proof.clone()).unwrap();
		checker.read_value(b"key1").unwrap().unwrap();
		checker.read_value(b"key2").unwrap();
		checker.read_value(b"key4").unwrap();
		checker.read_value(b"key22").unwrap();
		assert_eq!(checker.ensure_no_unused_nodes(), Ok(()));

		let checker = StorageProofChecker::<sp_core::Blake2Hasher>::new(root, proof).unwrap();
		assert_eq!(checker.ensure_no_unused_nodes(), Err(StorageProofError::UnusedKey));
	}
}

#[cfg(test)]
mod tests_for_unverified_storage_proof {
	use super::*;

	type Hasher = sp_core::Blake2Hasher;

	#[test]
	fn verify_succeeds_when_used_correctly() {
		let (root, db) = UnverifiedStorageProof::try_from_entries::<Hasher>(
			StateVersion::default(),
			&[(b"key1".to_vec(), None), (b"key2".to_vec(), Some(b"val2".to_vec()))],
		)
		.expect("UnverifiedStorageProof::try_from_entries() shouldn't fail in tests");

		assert!(db.verify::<Hasher>(StateVersion::V1, &root).is_ok());
	}

	#[test]
	fn verify_fails_when_proof_contains_unneeded_nodes() {
		let (root, mut db) = UnverifiedStorageProof::try_from_entries::<Hasher>(
			StateVersion::default(),
			&[
				(b"key1".to_vec(), Some(b"val1".to_vec().encode())),
				(b"key2".to_vec(), Some(b"val2".to_vec().encode())),
			],
		)
		.expect("UnverifiedStorageProof::try_from_entries() shouldn't fail in tests");
		assert!(db.db.pop().is_some());

		assert!(matches!(
			db.verify::<Hasher>(StateVersion::V1, &root),
			Err(StorageProofError::InvalidProof)
		));
	}

	#[test]
	fn verify_fails_when_db_contains_duplicate_nodes() {
		let (root, mut db) = UnverifiedStorageProof::try_from_entries::<Hasher>(
			StateVersion::default(),
			&[(b"key".to_vec(), None)],
		)
		.expect("UnverifiedStorageProof::try_from_entries() shouldn't fail in tests");
		db.db.push((b"key".to_vec(), None));

		assert!(matches!(
			db.verify::<Hasher>(StateVersion::V1, &root),
			Err(StorageProofError::InvalidProof)
		));
	}

	#[test]
	fn verify_fails_when_entries_are_not_sorted() {
		let (root, mut db) = UnverifiedStorageProof::try_from_entries::<Hasher>(
			StateVersion::default(),
			&[
				(b"key1".to_vec(), Some(b"val1".to_vec().encode())),
				(b"key2".to_vec(), Some(b"val2".to_vec().encode())),
			],
		)
		.expect("UnverifiedStorageProof::try_from_entries() shouldn't fail in tests");
		db.db.reverse();

		assert!(matches!(
			db.verify::<Hasher>(StateVersion::V1, &root),
			Err(StorageProofError::UnsortedEntries)
		));
	}

	#[test]
	fn get_and_decode_mandatory_works() {
		let (root, db) = UnverifiedStorageProof::try_from_entries::<Hasher>(
			StateVersion::default(),
			&[
				(b"key11".to_vec(), Some(b"val11".to_vec().encode())),
				(b"key2".to_vec(), Some(b"val2".to_vec().encode())),
				(b"key1".to_vec(), None),
				(b"key15".to_vec(), Some(b"val15".to_vec())),
			],
		)
		.expect("UnverifiedStorageProof::try_from_entries() shouldn't fail in tests");
		let mut trusted_db = db.verify::<Hasher>(StateVersion::V1, &root).unwrap();

		assert!(
			matches!(trusted_db.get_and_decode_mandatory::<Vec<u8>>(b"key11"), Ok(val) if val == b"val11".to_vec())
		);
		assert!(
			matches!(trusted_db.get_and_decode_mandatory::<Vec<u8>>(b"key2"), Ok(val) if val == b"val2".to_vec())
		);
		assert!(matches!(
			trusted_db.get_and_decode_mandatory::<Vec<u8>>(b"key1"),
			Err(StorageProofError::EmptyVal)
		));
		assert!(matches!(
			trusted_db.get_and_decode_mandatory::<Vec<u8>>(b"key15"),
			Err(StorageProofError::DecodeError)
		));
	}

	#[test]
	fn get_and_decode_optional_works() {
		let (root, db) = UnverifiedStorageProof::try_from_entries::<Hasher>(
			StateVersion::default(),
			&[
				(b"key11".to_vec(), Some(b"val11".to_vec().encode())),
				(b"key2".to_vec(), Some(b"val2".to_vec().encode())),
				(b"key1".to_vec(), None),
				(b"key15".to_vec(), Some(b"val15".to_vec())),
			],
		)
		.expect("UnverifiedStorageProof::try_from_entries() shouldn't fail in tests");
		let mut trusted_db = db.verify::<Hasher>(StateVersion::V1, &root).unwrap();

		assert!(
			matches!(trusted_db.get_and_decode_optional::<Vec<u8>>(b"key11"), Ok(Some(val)) if val ==
		b"val11".to_vec())
		);
		assert!(
			matches!(trusted_db.get_and_decode_optional::<Vec<u8>>(b"key2"), Ok(Some(val)) if val == b"val2".to_vec())
		);
		assert!(matches!(trusted_db.get_and_decode_optional::<Vec<u8>>(b"key1"), Ok(None)));
		assert!(matches!(
			trusted_db.get_and_decode_optional::<Vec<u8>>(b"key15"),
			Err(StorageProofError::DecodeError)
		));
	}

	#[test]
	fn ensure_no_unused_keys_works_correctly() {
		let (root, db) = UnverifiedStorageProof::try_from_entries::<Hasher>(
			StateVersion::default(),
			&[(b"key1".to_vec(), None), (b"key2".to_vec(), Some(b"val2".to_vec()))],
		)
		.expect("UnverifiedStorageProof::try_from_entries() shouldn't fail in tests");
		let mut trusted_db = db.verify::<Hasher>(StateVersion::V1, &root).unwrap();
		assert!(trusted_db.get(b"key1").is_ok());

		assert!(matches!(trusted_db.ensure_no_unused_keys(), Err(StorageProofError::UnusedKey)));
	}
}
