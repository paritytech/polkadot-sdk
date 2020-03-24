// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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

//! An opt-in utility for tracking historical sessions in FRAME-session.
//!
//! This is generally useful when implementing blockchains that require accountable
//! safety where validators from some amount f prior sessions must remain slashable.
//!
//! Rather than store the full session data for any given session, we instead commit
//! to the roots of merkle tries containing the session data.
//!
//! These roots and proofs of inclusion can be generated at any time during the current session.
//! Afterwards, the proofs can be fed to a consensus module when reporting misbehavior.

use sp_std::prelude::*;
use codec::{Encode, Decode};
use sp_runtime::{KeyTypeId, RuntimeDebug};
use sp_runtime::traits::{Convert, OpaqueKeys};
use frame_support::{decl_module, decl_storage};
use frame_support::{Parameter, print};
use sp_trie::{MemoryDB, Trie, TrieMut, Recorder, EMPTY_PREFIX};
use sp_trie::trie_types::{TrieDBMut, TrieDB};
use super::{SessionIndex, Module as SessionModule};

type ValidatorCount = u32;

/// Trait necessary for the historical module.
pub trait Trait: super::Trait {
	/// Full identification of the validator.
	type FullIdentification: Parameter;

	/// A conversion from validator ID to full identification.
	///
	/// This should contain any references to economic actors associated with the
	/// validator, since they may be outdated by the time this is queried from a
	/// historical trie.
	///
	/// It must return the identification for the current session index.
	type FullIdentificationOf: Convert<Self::ValidatorId, Option<Self::FullIdentification>>;
}

decl_storage! {
	trait Store for Module<T: Trait> as Session {
		/// Mapping from historical session indices to session-data root hash and validator count.
		HistoricalSessions get(fn historical_root):
			map hasher(twox_64_concat) SessionIndex => Option<(T::Hash, ValidatorCount)>;
		/// The range of historical sessions we store. [first, last)
		StoredRange: Option<(SessionIndex, SessionIndex)>;
		/// Deprecated.
		CachedObsolete:
			map hasher(twox_64_concat) SessionIndex
			=> Option<Vec<(T::ValidatorId, T::FullIdentification)>>;
	}
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {}
}

impl<T: Trait> Module<T> {
	/// Prune historical stored session roots up to (but not including)
	/// `up_to`.
	pub fn prune_up_to(up_to: SessionIndex) {
		<Self as Store>::StoredRange::mutate(|range| {
			let (start, end) = match *range {
				Some(range) => range,
				None => return, // nothing to prune.
			};

			let up_to = sp_std::cmp::min(up_to, end);

			if up_to < start {
				return // out of bounds. harmless.
			}

			(start..up_to).for_each(<Self as Store>::HistoricalSessions::remove);

			let new_start = up_to;
			*range = if new_start == end {
				None // nothing is stored.
			} else {
				Some((new_start, end))
			}
		})
	}
}

/// Specialization of the crate-level `SessionManager` which returns the set of full identification
/// when creating a new session.
pub trait SessionManager<ValidatorId, FullIdentification>: crate::SessionManager<ValidatorId> {
	/// If there was a validator set change, its returns the set of new validators along with their
	/// full identifications.
	fn new_session(new_index: SessionIndex) -> Option<Vec<(ValidatorId, FullIdentification)>>;
	fn start_session(start_index: SessionIndex);
	fn end_session(end_index: SessionIndex);
}

/// An `SessionManager` implementation that wraps an inner `I` and also
/// sets the historical trie root of the ending session.
pub struct NoteHistoricalRoot<T, I>(sp_std::marker::PhantomData<(T, I)>);

impl<T: Trait, I> crate::SessionManager<T::ValidatorId> for NoteHistoricalRoot<T, I>
	where I: SessionManager<T::ValidatorId, T::FullIdentification>
{
	fn new_session(new_index: SessionIndex) -> Option<Vec<T::ValidatorId>> {
		StoredRange::mutate(|range| {
			range.get_or_insert_with(|| (new_index, new_index)).1 = new_index + 1;
		});

		let new_validators_and_id = <I as SessionManager<_, _>>::new_session(new_index);
		let new_validators = new_validators_and_id.as_ref().map(|new_validators| {
			new_validators.iter().map(|(v, _id)| v.clone()).collect()
		});

		if let Some(new_validators) = new_validators_and_id {
			let count = new_validators.len() as u32;
			match ProvingTrie::<T>::generate_for(new_validators) {
				Ok(trie) => <HistoricalSessions<T>>::insert(new_index, &(trie.root, count)),
				Err(reason) => {
					print("Failed to generate historical ancestry-inclusion proof.");
					print(reason);
				}
			};
		} else {
			let previous_index = new_index.saturating_sub(1);
			if let Some(previous_session) = <HistoricalSessions<T>>::get(previous_index) {
				<HistoricalSessions<T>>::insert(new_index, previous_session);
			}
		}

		new_validators
	}
	fn start_session(start_index: SessionIndex) {
		<I as SessionManager<_, _>>::start_session(start_index)
	}
	fn end_session(end_index: SessionIndex) {
		<I as SessionManager<_, _>>::end_session(end_index)
	}
}

/// A tuple of the validator's ID and their full identification.
pub type IdentificationTuple<T> = (<T as crate::Trait>::ValidatorId, <T as Trait>::FullIdentification);

/// a trie instance for checking and generating proofs.
pub struct ProvingTrie<T: Trait> {
	db: MemoryDB<T::Hashing>,
	root: T::Hash,
}

impl<T: Trait> ProvingTrie<T> {
	fn generate_for<I>(validators: I) -> Result<Self, &'static str>
		where I: IntoIterator<Item=(T::ValidatorId, T::FullIdentification)>
	{
		let mut db = MemoryDB::default();
		let mut root = Default::default();

		{
			let mut trie = TrieDBMut::new(&mut db, &mut root);
			for (i, (validator, full_id)) in validators.into_iter().enumerate() {
				let i = i as u32;
				let keys = match <SessionModule<T>>::load_keys(&validator) {
					None => continue,
					Some(k) => k,
				};

				let full_id = (validator, full_id);

				// map each key to the owner index.
				for key_id in T::Keys::key_ids() {
					let key = keys.get_raw(*key_id);
					let res = (key_id, key).using_encoded(|k|
						i.using_encoded(|v| trie.insert(k, v))
					);

					let _ = res.map_err(|_| "failed to insert into trie")?;
				}

				// map each owner index to the full identification.
				let _ = i.using_encoded(|k| full_id.using_encoded(|v| trie.insert(k, v)))
					.map_err(|_| "failed to insert into trie")?;
			}
		}

		Ok(ProvingTrie {
			db,
			root,
		})
	}

	fn from_nodes(root: T::Hash, nodes: &[Vec<u8>]) -> Self {
		use sp_trie::HashDBT;

		let mut memory_db = MemoryDB::default();
		for node in nodes {
			HashDBT::insert(&mut memory_db, EMPTY_PREFIX, &node[..]);
		}

		ProvingTrie {
			db: memory_db,
			root,
		}
	}

	/// Prove the full verification data for a given key and key ID.
	pub fn prove(&self, key_id: KeyTypeId, key_data: &[u8]) -> Option<Vec<Vec<u8>>> {
		let trie = TrieDB::new(&self.db, &self.root).ok()?;
		let mut recorder = Recorder::new();
		let val_idx = (key_id, key_data).using_encoded(|s| {
			trie.get_with(s, &mut recorder)
				.ok()?
				.and_then(|raw| u32::decode(&mut &*raw).ok())
		})?;

		val_idx.using_encoded(|s| {
			trie.get_with(s, &mut recorder)
				.ok()?
				.and_then(|raw| <IdentificationTuple<T>>::decode(&mut &*raw).ok())
		})?;

		Some(recorder.drain().into_iter().map(|r| r.data).collect())
	}

	/// Access the underlying trie root.
	pub fn root(&self) -> &T::Hash {
		&self.root
	}

	// Check a proof contained within the current memory-db. Returns `None` if the
	// nodes within the current `MemoryDB` are insufficient to query the item.
	fn query(&self, key_id: KeyTypeId, key_data: &[u8]) -> Option<IdentificationTuple<T>> {
		let trie = TrieDB::new(&self.db, &self.root).ok()?;
		let val_idx = (key_id, key_data).using_encoded(|s| trie.get(s))
			.ok()?
			.and_then(|raw| u32::decode(&mut &*raw).ok())?;

		val_idx.using_encoded(|s| trie.get(s))
			.ok()?
			.and_then(|raw| <IdentificationTuple<T>>::decode(&mut &*raw).ok())
	}

}

/// Proof of ownership of a specific key.
#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug)]
pub struct Proof {
	session: SessionIndex,
	trie_nodes: Vec<Vec<u8>>,
}

impl Proof {
	/// Returns a session this proof was generated for.
	pub fn session(&self) -> SessionIndex {
		self.session
	}
}

impl<T: Trait, D: AsRef<[u8]>> frame_support::traits::KeyOwnerProofSystem<(KeyTypeId, D)>
	for Module<T>
{
	type Proof = Proof;
	type IdentificationTuple = IdentificationTuple<T>;

	fn prove(key: (KeyTypeId, D)) -> Option<Self::Proof> {
		let session = <SessionModule<T>>::current_index();
		let validators = <SessionModule<T>>::validators().into_iter()
			.filter_map(|validator| {
				T::FullIdentificationOf::convert(validator.clone())
					.map(|full_id| (validator, full_id))
			});
		let trie = ProvingTrie::<T>::generate_for(validators).ok()?;

		let (id, data) = key;

		trie.prove(id, data.as_ref()).map(|trie_nodes| Proof {
			session,
			trie_nodes,
		})
	}

	fn check_proof(key: (KeyTypeId, D), proof: Proof) -> Option<IdentificationTuple<T>> {
		let (id, data) = key;

		if proof.session == <SessionModule<T>>::current_index() {
			<SessionModule<T>>::key_owner(id, data.as_ref()).and_then(|owner|
				T::FullIdentificationOf::convert(owner.clone()).map(move |id| (owner, id))
			)
		} else {
			let (root, _) = <HistoricalSessions<T>>::get(&proof.session)?;
			let trie = ProvingTrie::<T>::from_nodes(root, &proof.trie_nodes);

			trie.query(id, data.as_ref())
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::crypto::key_types::DUMMY;
	use sp_runtime::testing::UintAuthorityId;
	use crate::mock::{
		NEXT_VALIDATORS, force_new_session,
		set_next_validators, Test, System, Session,
	};
	use frame_support::traits::{KeyOwnerProofSystem, OnInitialize};

	type Historical = Module<Test>;

	fn new_test_ext() -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
		crate::GenesisConfig::<Test> {
			keys: NEXT_VALIDATORS.with(|l|
				l.borrow().iter().cloned().map(|i| (i, i, UintAuthorityId(i).into())).collect()
			),
		}.assimilate_storage(&mut t).unwrap();
		sp_io::TestExternalities::new(t)
	}

	#[test]
	fn generated_proof_is_good() {
		new_test_ext().execute_with(|| {
			set_next_validators(vec![1, 2]);
			force_new_session();

			System::set_block_number(1);
			Session::on_initialize(1);

			let encoded_key_1 = UintAuthorityId(1).encode();
			let proof = Historical::prove((DUMMY, &encoded_key_1[..])).unwrap();

			// proof-checking in the same session is OK.
			assert!(Historical::check_proof((DUMMY, &encoded_key_1[..]), proof.clone()).is_some());

			set_next_validators(vec![1, 2, 4]);
			force_new_session();

			System::set_block_number(2);
			Session::on_initialize(2);

			assert!(Historical::historical_root(proof.session).is_some());
			assert!(Session::current_index() > proof.session);

			// proof-checking in the next session is also OK.
			assert!(Historical::check_proof((DUMMY, &encoded_key_1[..]), proof.clone()).is_some());

			set_next_validators(vec![1, 2, 5]);

			force_new_session();
			System::set_block_number(3);
			Session::on_initialize(3);
		});
	}

	#[test]
	fn prune_up_to_works() {
		new_test_ext().execute_with(|| {
			for i in 1..99u64 {
				set_next_validators(vec![i]);
				force_new_session();

				System::set_block_number(i);
				Session::on_initialize(i);

			}

			assert_eq!(StoredRange::get(), Some((0, 100)));

			for i in 0..100 {
				assert!(Historical::historical_root(i).is_some())
			}

			Historical::prune_up_to(10);
			assert_eq!(StoredRange::get(), Some((10, 100)));

			Historical::prune_up_to(9);
			assert_eq!(StoredRange::get(), Some((10, 100)));

			for i in 10..100 {
				assert!(Historical::historical_root(i).is_some())
			}

			Historical::prune_up_to(99);
			assert_eq!(StoredRange::get(), Some((99, 100)));

			Historical::prune_up_to(100);
			assert_eq!(StoredRange::get(), None);

			for i in 99..199u64 {
				set_next_validators(vec![i]);
				force_new_session();

				System::set_block_number(i);
				Session::on_initialize(i);

			}

			assert_eq!(StoredRange::get(), Some((100, 200)));

			for i in 100..200 {
				assert!(Historical::historical_root(i).is_some())
			}

			Historical::prune_up_to(9999);
			assert_eq!(StoredRange::get(), None);

			for i in 100..200 {
				assert!(Historical::historical_root(i).is_none())
			}
		});
	}
}
