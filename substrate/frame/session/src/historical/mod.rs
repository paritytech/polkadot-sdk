// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

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

pub mod offchain;
pub mod onchain;
mod shared;

use alloc::vec::Vec;
use codec::{Decode, Encode};
use core::fmt::Debug;
use sp_runtime::{
	traits::{Convert, OpaqueKeys},
	KeyTypeId,
};
use sp_session::{MembershipProof, ValidatorCount};
use sp_staking::SessionIndex;
use sp_trie::{
	trie_types::{TrieDBBuilder, TrieDBMutBuilderV0},
	LayoutV0, MemoryDB, RandomState, Recorder, StorageProof, Trie, TrieMut, TrieRecorder,
};

use frame_support::{
	print,
	traits::{KeyOwnerProofSystem, ValidatorSet, ValidatorSetWithIdentification},
	Parameter,
};

const LOG_TARGET: &'static str = "runtime::historical";

use crate::{self as pallet_session, Pallet as Session};

pub use pallet::*;
use sp_trie::{accessed_nodes_tracker::AccessedNodesTracker, recorder_ext::RecorderExt};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	/// Config necessary for the historical pallet.
	#[pallet::config]
	pub trait Config: pallet_session::Config + frame_system::Config {
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

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

	/// Mapping from historical session indices to session-data root hash and validator count.
	#[pallet::storage]
	#[pallet::getter(fn historical_root)]
	pub type HistoricalSessions<T: Config> =
		StorageMap<_, Twox64Concat, SessionIndex, (T::Hash, ValidatorCount), OptionQuery>;

	/// The range of historical sessions we store. [first, last)
	#[pallet::storage]
	pub type StoredRange<T> = StorageValue<_, (SessionIndex, SessionIndex), OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T> {
		/// The merkle root of the validators of the said session were stored
		RootStored { index: SessionIndex },
		/// The merkle roots of up to this session index were pruned
		RootsPruned { up_to: SessionIndex },
	}
}

impl<T: Config> Pallet<T> {
	/// Prune historical stored session roots up to (but not including)
	/// `up_to`.
	pub fn prune_up_to(up_to: SessionIndex) {
		StoredRange::<T>::mutate(|range| {
			let (start, end) = match *range {
				Some(range) => range,
				None => return, // nothing to prune.
			};

			let up_to = core::cmp::min(up_to, end);

			if up_to < start {
				return // out of bounds. harmless.
			}

			(start..up_to).for_each(HistoricalSessions::<T>::remove);

			let new_start = up_to;
			*range = if new_start == end {
				None // nothing is stored.
			} else {
				Some((new_start, end))
			}
		});

		Self::deposit_event(Event::<T>::RootsPruned { up_to });
	}

	fn full_id_validators() -> Vec<(T::ValidatorId, T::FullIdentification)> {
		<Session<T>>::validators()
			.into_iter()
			.filter_map(|validator| {
				T::FullIdentificationOf::convert(validator.clone())
					.map(|full_id| (validator, full_id))
			})
			.collect::<Vec<_>>()
	}
}

impl<T: Config> ValidatorSet<T::AccountId> for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type ValidatorIdOf = T::ValidatorIdOf;

	fn session_index() -> sp_staking::SessionIndex {
		super::Pallet::<T>::current_index()
	}

	fn validators() -> Vec<Self::ValidatorId> {
		super::Pallet::<T>::validators()
	}
}

impl<T: Config> ValidatorSetWithIdentification<T::AccountId> for Pallet<T> {
	type Identification = T::FullIdentification;
	type IdentificationOf = T::FullIdentificationOf;
}

/// Specialization of the crate-level `SessionManager` which returns the set of full identification
/// when creating a new session.
pub trait SessionManager<ValidatorId, FullIdentification>:
	pallet_session::SessionManager<ValidatorId>
{
	/// If there was a validator set change, its returns the set of new validators along with their
	/// full identifications.
	fn new_session(new_index: SessionIndex) -> Option<Vec<(ValidatorId, FullIdentification)>>;
	fn new_session_genesis(
		new_index: SessionIndex,
	) -> Option<Vec<(ValidatorId, FullIdentification)>> {
		<Self as SessionManager<_, _>>::new_session(new_index)
	}
	fn start_session(start_index: SessionIndex);
	fn end_session(end_index: SessionIndex);
}

/// An `SessionManager` implementation that wraps an inner `I` and also
/// sets the historical trie root of the ending session.
pub struct NoteHistoricalRoot<T, I>(core::marker::PhantomData<(T, I)>);

impl<T: Config, I: SessionManager<T::ValidatorId, T::FullIdentification>> NoteHistoricalRoot<T, I> {
	fn do_new_session(new_index: SessionIndex, is_genesis: bool) -> Option<Vec<T::ValidatorId>> {
		<StoredRange<T>>::mutate(|range| {
			range.get_or_insert_with(|| (new_index, new_index)).1 = new_index + 1;
		});

		let new_validators_and_id = if is_genesis {
			<I as SessionManager<_, _>>::new_session_genesis(new_index)
		} else {
			<I as SessionManager<_, _>>::new_session(new_index)
		};
		let new_validators_opt = new_validators_and_id
			.as_ref()
			.map(|new_validators| new_validators.iter().map(|(v, _id)| v.clone()).collect());

		if let Some(new_validators) = new_validators_and_id {
			let count = new_validators.len() as ValidatorCount;
			match ProvingTrie::<T>::generate_for(new_validators) {
				Ok(trie) => {
					<HistoricalSessions<T>>::insert(new_index, &(trie.root, count));
					Pallet::<T>::deposit_event(Event::RootStored { index: new_index });
				},
				Err(reason) => {
					print("Failed to generate historical ancestry-inclusion proof.");
					print(reason);
				},
			};
		} else {
			let previous_index = new_index.saturating_sub(1);
			if let Some(previous_session) = <HistoricalSessions<T>>::get(previous_index) {
				<HistoricalSessions<T>>::insert(new_index, previous_session);
				Pallet::<T>::deposit_event(Event::RootStored { index: new_index });
			}
		}

		new_validators_opt
	}
}

impl<T: Config, I> pallet_session::SessionManager<T::ValidatorId> for NoteHistoricalRoot<T, I>
where
	I: SessionManager<T::ValidatorId, T::FullIdentification>,
{
	fn new_session(new_index: SessionIndex) -> Option<Vec<T::ValidatorId>> {
		Self::do_new_session(new_index, false)
	}

	fn new_session_genesis(new_index: SessionIndex) -> Option<Vec<T::ValidatorId>> {
		Self::do_new_session(new_index, true)
	}

	fn start_session(start_index: SessionIndex) {
		<I as SessionManager<_, _>>::start_session(start_index)
	}

	fn end_session(end_index: SessionIndex) {
		onchain::store_session_validator_set_to_offchain::<T>(end_index);
		<I as SessionManager<_, _>>::end_session(end_index)
	}
}

/// A tuple of the validator's ID and their full identification.
pub type IdentificationTuple<T> =
	(<T as pallet_session::Config>::ValidatorId, <T as Config>::FullIdentification);

/// A trie instance for checking and generating proofs.
pub struct ProvingTrie<T: Config> {
	db: MemoryDB<T::Hashing>,
	root: T::Hash,
}

impl<T: Config> ProvingTrie<T> {
	fn generate_for<I>(validators: I) -> Result<Self, &'static str>
	where
		I: IntoIterator<Item = (T::ValidatorId, T::FullIdentification)>,
	{
		let mut db = MemoryDB::with_hasher(RandomState::default());
		let mut root = Default::default();

		{
			let mut trie = TrieDBMutBuilderV0::new(&mut db, &mut root).build();
			for (i, (validator, full_id)) in validators.into_iter().enumerate() {
				let i = i as u32;
				let keys = match <Session<T>>::load_keys(&validator) {
					None => continue,
					Some(k) => k,
				};

				let id_tuple = (validator, full_id);

				// map each key to the owner index.
				for key_id in T::Keys::key_ids() {
					let key = keys.get_raw(*key_id);
					let res =
						(key_id, key).using_encoded(|k| i.using_encoded(|v| trie.insert(k, v)));

					res.map_err(|_| "failed to insert into trie")?;
				}

				// map each owner index to the full identification.
				i.using_encoded(|k| id_tuple.using_encoded(|v| trie.insert(k, v)))
					.map_err(|_| "failed to insert into trie")?;
			}
		}

		Ok(ProvingTrie { db, root })
	}

	fn from_proof(root: T::Hash, proof: StorageProof) -> Self {
		ProvingTrie { db: proof.into_memory_db(), root }
	}

	/// Prove the full verification data for a given key and key ID.
	pub fn prove(&self, key_id: KeyTypeId, key_data: &[u8]) -> Option<Vec<Vec<u8>>> {
		let mut recorder = Recorder::<LayoutV0<T::Hashing>>::new();
		self.query(key_id, key_data, Some(&mut recorder));

		Some(recorder.into_raw_storage_proof())
	}

	/// Access the underlying trie root.
	pub fn root(&self) -> &T::Hash {
		&self.root
	}

	/// Search for a key inside the proof.
	fn query(
		&self,
		key_id: KeyTypeId,
		key_data: &[u8],
		recorder: Option<&mut dyn TrieRecorder<T::Hash>>,
	) -> Option<IdentificationTuple<T>> {
		let trie = TrieDBBuilder::new(&self.db, &self.root)
			.with_optional_recorder(recorder)
			.build();

		let val_idx = (key_id, key_data)
			.using_encoded(|s| trie.get(s))
			.ok()?
			.and_then(|raw| u32::decode(&mut &*raw).ok())?;

		val_idx
			.using_encoded(|s| trie.get(s))
			.ok()?
			.and_then(|raw| <IdentificationTuple<T>>::decode(&mut &*raw).ok())
	}
}

impl<T: Config, D: AsRef<[u8]>> KeyOwnerProofSystem<(KeyTypeId, D)> for Pallet<T> {
	type Proof = MembershipProof;
	type IdentificationTuple = IdentificationTuple<T>;

	fn prove(key: (KeyTypeId, D)) -> Option<Self::Proof> {
		let session = <Session<T>>::current_index();
		let validators = Self::full_id_validators();

		let count = validators.len() as ValidatorCount;

		let trie = ProvingTrie::<T>::generate_for(validators).ok()?;

		let (id, data) = key;
		trie.prove(id, data.as_ref()).map(|trie_nodes| MembershipProof {
			session,
			trie_nodes,
			validator_count: count,
		})
	}

	fn check_proof(key: (KeyTypeId, D), proof: Self::Proof) -> Option<IdentificationTuple<T>> {
		fn print_error<E: Debug>(e: E) {
			log::error!(
				target: LOG_TARGET,
				"Rejecting equivocation report because of key ownership proof error: {:?}", e
			);
		}

		let (id, data) = key;
		let (root, count) = if proof.session == <Session<T>>::current_index() {
			let validators = Self::full_id_validators();
			let count = validators.len() as ValidatorCount;
			let trie = ProvingTrie::<T>::generate_for(validators).map_err(print_error).ok()?;
			(trie.root, count)
		} else {
			<HistoricalSessions<T>>::get(&proof.session)?
		};

		if count != proof.validator_count {
			print_error("InvalidCount");
			return None
		}

		let proof = StorageProof::new_with_duplicate_nodes_check(proof.trie_nodes)
			.map_err(print_error)
			.ok()?;
		let mut accessed_nodes_tracker = AccessedNodesTracker::<T::Hash>::new(proof.len());
		let trie = ProvingTrie::<T>::from_proof(root, proof);
		let res = trie.query(id, data.as_ref(), Some(&mut accessed_nodes_tracker))?;
		accessed_nodes_tracker.ensure_no_unused_nodes().map_err(print_error).ok()?;
		Some(res)
	}
}

#[cfg(test)]
pub(crate) mod tests {
	use super::*;
	use crate::mock::{
		force_new_session, set_next_validators, NextValidators, Session, System, Test,
	};
	use alloc::vec;

	use sp_runtime::{key_types::DUMMY, testing::UintAuthorityId, BuildStorage};
	use sp_state_machine::BasicExternalities;

	use frame_support::traits::{KeyOwnerProofSystem, OnInitialize};

	type Historical = Pallet<Test>;

	pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
		let keys: Vec<_> = NextValidators::get()
			.iter()
			.cloned()
			.map(|i| (i, i, UintAuthorityId(i).into()))
			.collect();
		BasicExternalities::execute_with_storage(&mut t, || {
			for (ref k, ..) in &keys {
				frame_system::Pallet::<Test>::inc_providers(k);
			}
		});
		pallet_session::GenesisConfig::<Test> { keys, ..Default::default() }
			.assimilate_storage(&mut t)
			.unwrap();
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

			assert_eq!(<StoredRange<Test>>::get(), Some((0, 100)));

			for i in 0..100 {
				assert!(Historical::historical_root(i).is_some())
			}

			Historical::prune_up_to(10);
			assert_eq!(<StoredRange<Test>>::get(), Some((10, 100)));

			Historical::prune_up_to(9);
			assert_eq!(<StoredRange<Test>>::get(), Some((10, 100)));

			for i in 10..100 {
				assert!(Historical::historical_root(i).is_some())
			}

			Historical::prune_up_to(99);
			assert_eq!(<StoredRange<Test>>::get(), Some((99, 100)));

			Historical::prune_up_to(100);
			assert_eq!(<StoredRange<Test>>::get(), None);

			for i in 99..199u64 {
				set_next_validators(vec![i]);
				force_new_session();

				System::set_block_number(i);
				Session::on_initialize(i);
			}

			assert_eq!(<StoredRange<Test>>::get(), Some((100, 200)));

			for i in 100..200 {
				assert!(Historical::historical_root(i).is_some())
			}

			Historical::prune_up_to(9999);
			assert_eq!(<StoredRange<Test>>::get(), None);

			for i in 100..200 {
				assert!(Historical::historical_root(i).is_none())
			}
		});
	}
}
