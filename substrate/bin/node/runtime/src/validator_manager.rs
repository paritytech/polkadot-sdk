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

//! A proof-of-authority validator set.
//!
//! A privileged origin registers and deregisters validators. A change is active two session
//! rotations after the call.
//!
//! The genesis validators come from the session keys in the `pallet_session` genesis config. From
//! session 2 on, every planned session gets the full validator set, so `pallet_session::historical`
//! stores a root for each session.

use alloc::vec::Vec;
use polkadot_sdk::*;
use sp_staking::SessionIndex;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: polkadot_sdk::frame_system::Config + pallet_session::Config {
		/// The origin that can register and deregister validators.
		type PrivilegedOrigin: EnsureOrigin<Self::RuntimeOrigin>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The validators join the set at the next planned session.
		ValidatorsRegistered { validators: Vec<T::ValidatorId> },
		/// The validators leave the set at the next planned session.
		ValidatorsDeregistered { validators: Vec<T::ValidatorId> },
	}

	/// Validators that join the set at the next planned session.
	#[pallet::storage]
	pub type ValidatorsToAdd<T: Config> = StorageValue<_, Vec<T::ValidatorId>, ValueQuery>;

	/// Validators that leave the set at the next planned session.
	#[pallet::storage]
	pub type ValidatorsToRemove<T: Config> = StorageValue<_, Vec<T::ValidatorId>, ValueQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Add `validators` to the set at the next planned session.
		///
		/// The planned session leaves out a validator that has no session keys.
		#[pallet::call_index(0)]
		#[pallet::weight(T::DbWeight::get().reads_writes(2, 2))]
		pub fn register_validators(
			origin: OriginFor<T>,
			validators: Vec<T::ValidatorId>,
		) -> DispatchResult {
			T::PrivilegedOrigin::ensure_origin(origin)?;
			ValidatorsToRemove::<T>::mutate(|to_remove| {
				to_remove.retain(|v| !validators.contains(v))
			});
			ValidatorsToAdd::<T>::mutate(|to_add| to_add.extend(validators.iter().cloned()));
			Self::deposit_event(Event::ValidatorsRegistered { validators });
			Ok(())
		}

		/// Remove `validators` from the set at the next planned session.
		#[pallet::call_index(1)]
		#[pallet::weight(T::DbWeight::get().reads_writes(2, 2))]
		pub fn deregister_validators(
			origin: OriginFor<T>,
			validators: Vec<T::ValidatorId>,
		) -> DispatchResult {
			T::PrivilegedOrigin::ensure_origin(origin)?;
			ValidatorsToAdd::<T>::mutate(|to_add| to_add.retain(|v| !validators.contains(v)));
			ValidatorsToRemove::<T>::mutate(|to_remove| {
				to_remove.extend(validators.iter().cloned())
			});
			Self::deposit_event(Event::ValidatorsDeregistered { validators });
			Ok(())
		}
	}
}

impl<T: Config> pallet_session::SessionManager<T::ValidatorId> for Pallet<T> {
	fn new_session(new_index: SessionIndex) -> Option<Vec<T::ValidatorId>> {
		if new_index <= 1 {
			return None;
		}

		let to_remove = ValidatorsToRemove::<T>::take();
		let mut validators = pallet_session::Pallet::<T>::validators();
		validators.retain(|v| !to_remove.contains(v));
		for validator in ValidatorsToAdd::<T>::take() {
			if !validators.contains(&validator) {
				validators.push(validator);
			}
		}
		// `pallet_session` leaves out validators without keys. The historical root must count the
		// same validators, or every key ownership proof for the session fails.
		validators.retain(|v| pallet_session::Pallet::<T>::load_keys(v).is_some());
		if validators.is_empty() {
			// An epoch without authorities stops the chain, because BABE cannot author a block.
			log::error!(
				target: "runtime::validator-manager",
				"The planned validator set for session {new_index} is empty, keeping the current set",
			);
			validators = pallet_session::Pallet::<T>::validators();
			validators.retain(|v| pallet_session::Pallet::<T>::load_keys(v).is_some());
		}
		Some(validators)
	}

	fn start_session(_: SessionIndex) {}

	fn end_session(_: SessionIndex) {}
}

impl<T: Config> pallet_session::historical::SessionManager<T::ValidatorId, ()> for Pallet<T> {
	fn new_session(new_index: SessionIndex) -> Option<Vec<(T::ValidatorId, ())>> {
		<Self as pallet_session::SessionManager<_>>::new_session(new_index)
			.map(|validators| validators.into_iter().map(|v| (v, ())).collect())
	}

	fn start_session(_: SessionIndex) {}

	fn end_session(_: SessionIndex) {}
}

#[cfg(test)]
mod tests {
	use crate::{
		genesis_config_presets::{authority_keys_from_seed, ENDOWMENT},
		AccountId, BalancesConfig, BuildStorage, Historical, Runtime, RuntimeGenesisConfig,
		RuntimeOrigin, Session, SessionConfig, System, ValidatorManager,
	};
	use polkadot_sdk::{
		frame_support::{assert_ok, traits::KeyOwnerProofSystem},
		pallet_babe,
		sp_consensus_babe::{digests::PreDigest, KEY_TYPE as BABE},
		sp_core::crypto::ByteArray,
		sp_io::TestExternalities,
		sp_keyring::Sr25519Keyring,
	};

	/// Alice is the only genesis validator. Bob has session keys but is not a validator.
	fn new_test_ext() -> (TestExternalities, AccountId, AccountId) {
		let (alice, _, alice_keys) = authority_keys_from_seed("Alice");
		let (bob, _, bob_keys) = authority_keys_from_seed("Bob");
		let ext = RuntimeGenesisConfig {
			balances: BalancesConfig {
				balances: vec![(alice.clone(), ENDOWMENT), (bob.clone(), ENDOWMENT)],
				..Default::default()
			},
			session: SessionConfig {
				keys: vec![(alice.clone(), alice.clone(), alice_keys)],
				non_authority_keys: vec![(bob.clone(), bob.clone(), bob_keys)],
			},
			..Default::default()
		}
		.build_storage()
		.unwrap()
		.into();
		(ext, alice, bob)
	}

	fn rotate_session() {
		System::set_block_number(System::block_number() + 1);
		// BABE asserts that its per-block initialization ran before an epoch change.
		pallet_babe::Initialized::<Runtime>::put(None::<PreDigest>);
		Session::rotate_session();
	}

	#[test]
	fn changes_apply_two_rotations_later() {
		let (mut ext, alice, bob) = new_test_ext();
		ext.execute_with(|| {
			rotate_session();
			rotate_session();

			assert_ok!(ValidatorManager::register_validators(
				RuntimeOrigin::root(),
				vec![bob.clone()]
			));
			rotate_session();
			assert_eq!(Session::validators(), vec![alice.clone()]);
			rotate_session();
			assert_eq!(Session::validators(), vec![alice.clone(), bob.clone()]);

			assert_ok!(ValidatorManager::deregister_validators(
				RuntimeOrigin::root(),
				vec![bob.clone()]
			));
			rotate_session();
			assert_eq!(Session::validators(), vec![alice.clone(), bob]);
			rotate_session();
			assert_eq!(Session::validators(), vec![alice]);
		});
	}

	#[test]
	fn deregistering_cancels_a_pending_registration() {
		let (mut ext, alice, bob) = new_test_ext();
		ext.execute_with(|| {
			rotate_session();
			rotate_session();

			assert_ok!(ValidatorManager::register_validators(
				RuntimeOrigin::root(),
				vec![bob.clone()]
			));
			assert_ok!(ValidatorManager::deregister_validators(RuntimeOrigin::root(), vec![bob]));
			rotate_session();
			rotate_session();
			assert_eq!(Session::validators(), vec![alice]);
		});
	}

	#[test]
	fn an_empty_set_keeps_the_current_validators() {
		let (mut ext, alice, _) = new_test_ext();
		ext.execute_with(|| {
			rotate_session();
			rotate_session();

			assert_ok!(ValidatorManager::deregister_validators(
				RuntimeOrigin::root(),
				vec![alice.clone()]
			));
			rotate_session();
			rotate_session();
			assert_eq!(Session::validators(), vec![alice]);
		});
	}

	#[test]
	fn validator_without_keys_keeps_proofs_valid() {
		let (mut ext, alice, _) = new_test_ext();
		ext.execute_with(|| {
			rotate_session();
			rotate_session();

			let charlie = Sr25519Keyring::Charlie.to_account_id();
			assert_ok!(ValidatorManager::register_validators(RuntimeOrigin::root(), vec![charlie]));
			rotate_session();
			rotate_session();
			assert_eq!(Session::validators(), vec![alice.clone()]);

			let alice_babe = Session::load_keys(&alice).unwrap().babe.to_raw_vec();
			let proof = Historical::prove((BABE, alice_babe.clone())).unwrap();
			rotate_session();
			assert!(Historical::check_proof((BABE, alice_babe), proof).is_some());
		});
	}
}
