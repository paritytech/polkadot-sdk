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

//! Validator Collators pallet.
//!
//! Stores the validator set announced by the chain that runs `pallet-staking-async`, typically
//! Asset Hub, and offers it as a session manager, so the relay-chain validators of the current era
//! collate on a system parachain.
//!
//! ## Overview
//!
//! The announcing chain sends the active validator set of each era, tagged with the era index of
//! `pallet-staking-async`.
//! This pallet stores the latest set, received through `set_validators` from [`Config::SetOrigin`]
//! or from another pallet through [`Pallet::receive_validator_set`]. It rejects a set whose era
//! is not newer than the stored one, or that has more validators than [`Config::MaxValidators`].
//!
//! The pallet is a [`pallet_session::SessionManager`]. At every session rotation it returns the
//! stored validators that have registered local session keys, checked with
//! [`Config::ValidatorRegistration`]. The runtime combines this pallet with
//! `pallet-collator-selection` through [`pallet_session::UnionSessionManager`], so invulnerables
//! and candidates keep collating next to the validators.
//!
//! [`MaxCollators`] optionally caps how many validators are returned. Governance can start with a
//! low cap and raise it in steps, or lower it again, to roll out validator collators gradually.
//! Under a cap the pallet draws the validators at random at the first forced rotation after a set
//! arrives or the cap changes, stores them in [`Collators`] and returns those with registered keys
//! until the next draw. The draw shuffles the whole set with a seed made of the genesis hash, the
//! era and the relay chain epoch randomness cached in [`EpochRandomness`], then keeps the first
//! validators with registered keys up to the cap. Every validator with keys has the same chance to
//! be drawn in each era. A cap change draws again from the stored set, so the drawn validators
//! can change. Every draw takes its seed from the randomness cached at the time of the draw. The
//! seed includes the genesis hash, so each chain draws independently from the same set. A
//! validator that registers keys after the draw waits for the next set or for a cap change. Each
//! set, so each era, gets a new draw, which is enacted at the second forced rotation with no
//! notice, so a validator with keys must already run a synced collator node to collate when drawn.
//!
//! The pallet implements [`cumulus_pallet_parachain_system::OnSystemEvent`]. The runtime must
//! include it in the `OnSystemEvent` of `cumulus-pallet-parachain-system`, alone or in a tuple
//! with other handlers, to keep [`EpochRandomness`] up to date from the relay chain state proof.
//! The runtime should also return the keys of [`Pallet::relay_keys_to_prove`] from
//! [`KeyToIncludeInRelayProof`](cumulus_primitives_core::KeyToIncludeInRelayProof), so collators
//! include the randomness key in the proof.
//! The draw runs in the session rotation, before the block's own proof is processed, so it uses
//! the value cached by the previous block.
//! A block whose proof lacks the trie nodes of the randomness key, or holds a value that does not
//! decode, is invalid. A proof that shows the key absent, as before the relay chain's first epoch
//! change, caches the zero value a draw without randomness uses.
//! Without the `OnSystemEvent` wiring, every draw uses a zero randomness value and logs an error.
//!
//! The stored set is the one staking elected for the era. The relay chain enacts only the elected
//! validators with relay-chain session keys, and each system chain returns only those with keys
//! registered there, so a returned collator is not necessarily an active relay-chain validator.
//!
//! The pallet is also a [`pallet_session::ShouldEndSession`]. Pallet-session queues a new set at
//! one rotation and enacts it at the next. When a set arrives, or the cap changes while a set is
//! stored, the pallet forces two rotations in the following two blocks, so the change is in force
//! without waiting for the regular period. The regular rotations given by
//! [`Config::PeriodicSession`] continue as before.
//!
//! The returned validators author blocks like any collator, so a `pallet-collator-selection` event
//! handler pays them from its pot and records them in `LastAuthoredBlock`.
//!
//! A runtime must configure `pallet-collator-selection`'s `KickThreshold` to exceed a full Aura
//! round of the merged list, [`Config::MaxValidators`] plus the invulnerables and candidates, times
//! the blocks the chain produces per Aura slot. Otherwise bonded candidates are kicked as stale
//! between their slots. [`Config::MaxValidators`] rather than the cap is the bound, since the cap
//! can be raised or removed without a runtime upgrade.
//!
//! ## TODO
//!
//! - Counting the blocks each validator authors and reporting era points to the chain where
//!   `pallet-staking-async` runs, typically Asset Hub.
//! - Dropping validators that author no blocks for a session, judged among the drawn validators
//!   under a cap.
//! - (Only if a need is established) Propagating relay-chain offences to the collator set.
//! - Under a cap, revisit the redraw period and notice once operators have run collators. A draw
//!   per era without notice is the first step, with operators expected to run a synced collator
//!   node all the time. Options: redraw every few eras or sessions, keep a drawn validator for
//!   several eras while it stays in the active set, and announce the drawn validators before they
//!   collate so they can sync a node. Open points: the time a node needs to sync, and refilling the
//!   place of a drawn validator that leaves the active set, which needs notice too.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(any(test, feature = "runtime-benchmarks"))]
mod relay_proof;
pub mod weights;

const LOG_TARGET: &str = "runtime::validator-collators";

#[frame_support::pallet]
pub mod pallet {
	pub use crate::weights::WeightInfo;
	use alloc::{collections::BTreeSet, vec, vec::Vec};
	use cumulus_pallet_parachain_system::{
		relay_state_snapshot::{Error as RelayProofError, ReadEntryErr},
		OnSystemEvent, RelayChainStateProof,
	};
	use cumulus_primitives_core::{
		relay_chain::well_known_keys, PersistedValidationData, RelayStorageKey,
	};
	use frame_support::{
		pallet_prelude::*,
		traits::{EnsureOrigin, ValidatorRegistration},
		BoundedBTreeSet,
	};
	use frame_system::pallet_prelude::*;
	use pallet_session::{SessionManager, ShouldEndSession};
	use rand::{seq::SliceRandom, SeedableRng};
	use rand_chacha::ChaCha20Rng;
	use sp_runtime::traits::{BlakeTwo256, Hash, Zero};
	use sp_staking::{EraIndex, SessionIndex};

	/// A validator set received for an era.
	#[frame_support::stored]
	pub struct EraValidatorSet<AccountId, MaxValidators: Get<u32>> {
		/// The era the set belongs to.
		pub era: EraIndex,
		/// The validator stashes of that era.
		pub validators: BoundedBTreeSet<AccountId, MaxValidators>,
	}

	/// The validators drawn under [`MaxCollators`], with the era of the set they were drawn from.
	#[frame_support::stored]
	pub struct DrawnCollators<AccountId, MaxValidators: Get<u32>> {
		/// The era of the set the validators were drawn from.
		pub era: EraIndex,
		/// The drawn validators, in drawn order.
		pub validators: BoundedVec<AccountId, MaxValidators>,
	}

	/// Progress of the two rotations that bring a received set into force.
	#[derive(
		Clone, Copy, Eq, PartialEq, Default, Encode, Decode, Debug, TypeInfo, MaxEncodedLen,
	)]
	pub enum RotationState {
		/// No forced rotation is pending.
		#[default]
		Idle,
		/// A set was received and the next rotation queues it.
		AwaitingQueue,
		/// The set is queued and the next rotation enacts it.
		AwaitingEnactment,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Configuration trait of this pallet.
	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Origin allowed to submit a validator set.
		type SetOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Origin allowed to change [`MaxCollators`].
		type UpdateOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Lookup of registered session keys.
		type ValidatorRegistration: ValidatorRegistration<Self::AccountId>;

		/// Maximum number of validators in a received set.
		#[pallet::constant]
		type MaxValidators: Get<u32>;

		/// The regular session rotation rule kept next to the forced rotations.
		type PeriodicSession: ShouldEndSession<BlockNumberFor<Self>>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	/// The latest received validator set.
	#[pallet::storage]
	pub type ValidatorSet<T: Config> =
		StorageValue<_, EraValidatorSet<T::AccountId, T::MaxValidators>, OptionQuery>;

	/// Progress of the forced rotations for the latest received set.
	#[pallet::storage]
	pub type PendingRotation<T: Config> = StorageValue<_, RotationState, ValueQuery>;

	/// Maximum number of validators returned as collators, `None` returns every validator with
	/// registered keys.
	///
	/// The cap applies before the union with the other session manager. A validator that the
	/// other session manager also returns takes a place under the cap without adding a collator.
	///
	/// The merged list becomes Aura's authority list, which Aura writes into the block header
	/// whenever it changes, and the relay chain rejects a header above its head-data limit. Size
	/// the cap so that the merged list, invulnerables and candidates included, keeps a
	/// session-change header within that limit.
	///
	/// Under a cap the validators are drawn at random, see [`Collators`].
	///
	/// A change with a stored set forces two rotations. The first draws [`Collators`] again from
	/// the stored set, so the drawn validators can change. The change is in force two blocks
	/// later. Removing the cap returns every validator with registered keys.
	#[pallet::storage]
	pub type MaxCollators<T: Config> = StorageValue<_, u32, OptionQuery>;

	/// The validators drawn from the stored set under [`MaxCollators`].
	///
	/// Drawn at the first forced rotation after a set arrives or the cap changes, with a seed made
	/// of the genesis hash, the era of the stored set and the cached [`EpochRandomness`]. Removed
	/// when the cap is removed. At every rotation the pallet returns the drawn validators that
	/// still have registered keys.
	#[pallet::storage]
	pub type Collators<T: Config> =
		StorageValue<_, DrawnCollators<T::AccountId, T::MaxValidators>, OptionQuery>;

	/// The relay chain randomness of one epoch ago, read from the relay chain state proof of every
	/// block and written when it changes.
	#[pallet::storage]
	pub type EpochRandomness<T: Config> = StorageValue<_, [u8; 32], OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A validator set was stored for an era.
		ValidatorSetReceived { era: EraIndex, count: u32 },
		/// The maximum number of validator collators was changed.
		MaxCollatorsSet { max: Option<u32> },
		/// Validators were drawn from the set of `era` under the cap.
		CollatorsDrawn { era: EraIndex, validators: Vec<T::AccountId> },
		/// The stored validator set does not decode, so no validators were returned for the
		/// session.
		StoredSetUndecodable,
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The era of the set is not newer than the era of the stored set.
		StaleEra,
		/// The set has more validators than [`Config::MaxValidators`].
		TooManyValidators,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Charges the read of [`PendingRotation`] by [`ShouldEndSession::should_end_session`],
		/// which pallet-session makes in every block without charging it.
		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			T::DbWeight::get()
				.reads(1)
				.saturating_add(Weight::from_parts(0, RotationState::max_encoded_len() as u64))
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(_: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Store the validator set of `era`.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::set_validators(validators.len() as u32))]
		pub fn set_validators(
			origin: OriginFor<T>,
			era: EraIndex,
			validators: BoundedBTreeSet<T::AccountId, T::MaxValidators>,
		) -> DispatchResult {
			T::SetOrigin::ensure_origin(origin)?;
			Self::do_receive_validator_set(era, validators)
		}

		/// Set the maximum number of validators returned as collators.
		///
		/// With a stored set, the change forces two rotations and the first draws [`Collators`]
		/// again, so the drawn validators can change. `None` clears [`Collators`].
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::set_max_collators())]
		pub fn set_max_collators(origin: OriginFor<T>, max: Option<u32>) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;
			MaxCollators::<T>::set(max);
			if max.is_none() {
				Collators::<T>::kill();
			}
			if ValidatorSet::<T>::exists() {
				PendingRotation::<T>::put(RotationState::AwaitingQueue);
			}
			Self::deposit_event(Event::MaxCollatorsSet { max });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Store the validator set of `era` and schedule the rotations that bring it into force.
		///
		/// An account listed more than once is kept once, and the set is checked against
		/// [`Config::MaxValidators`] after that.
		pub fn receive_validator_set(
			era: EraIndex,
			validators: impl IntoIterator<Item = T::AccountId>,
		) -> DispatchResult {
			let validators =
				BoundedBTreeSet::try_from(validators.into_iter().collect::<BTreeSet<_>>())
					.map_err(|_| Error::<T>::TooManyValidators)?;
			Self::do_receive_validator_set(era, validators)
		}

		pub(crate) fn do_receive_validator_set(
			era: EraIndex,
			validators: BoundedBTreeSet<T::AccountId, T::MaxValidators>,
		) -> DispatchResult {
			ensure!(
				ValidatorSet::<T>::get().is_none_or(|stored| era > stored.era),
				Error::<T>::StaleEra
			);
			let count = validators.len() as u32;
			ValidatorSet::<T>::put(EraValidatorSet { era, validators });
			PendingRotation::<T>::put(RotationState::AwaitingQueue);
			Self::deposit_event(Event::ValidatorSetReceived { era, count });
			Ok(())
		}

		/// The relay chain storage keys this pallet reads from the relay chain state proof.
		///
		/// A runtime returns them, next to the keys of other pallets, from
		/// [`KeyToIncludeInRelayProof`](cumulus_primitives_core::KeyToIncludeInRelayProof).
		pub fn relay_keys_to_prove() -> Vec<RelayStorageKey> {
			vec![RelayStorageKey::Top(well_known_keys::ONE_EPOCH_AGO_RANDOMNESS.to_vec())]
		}

		/// The seed of a draw from the set of `era`, made of the genesis hash, the era and
		/// [`EpochRandomness`], or a zero value when no randomness is cached.
		pub(crate) fn draw_seed(era: EraIndex) -> [u8; 32] {
			let randomness = EpochRandomness::<T>::get().unwrap_or_else(|| {
				log::error!(
					target: crate::LOG_TARGET,
					"no relay chain epoch randomness is cached, drawing with a zero value"
				);
				[0; 32]
			});
			let genesis = frame_system::Pallet::<T>::block_hash(BlockNumberFor::<T>::zero());
			BlakeTwo256::hash_of(&(b"validator-collators", genesis, era, randomness)).into()
		}

		/// Draw up to `cap` validators with registered keys from `set` into [`Collators`] and
		/// emit [`Event::CollatorsDrawn`].
		///
		/// The whole set is shuffled before the key filter, so the order depends only on the seed
		/// and the set.
		fn draw_collators(
			set: EraValidatorSet<T::AccountId, T::MaxValidators>,
			cap: u32,
		) -> BoundedVec<T::AccountId, T::MaxValidators> {
			let EraValidatorSet { era, validators } = set;
			let mut order: Vec<_> = validators.into_iter().collect();
			order.shuffle(&mut ChaCha20Rng::from_seed(Self::draw_seed(era)));
			let drawn: Vec<_> = order
				.into_iter()
				.filter(T::ValidatorRegistration::is_registered)
				.take(cap as usize)
				.collect();
			Self::deposit_event(Event::CollatorsDrawn { era, validators: drawn.clone() });
			// At most the length of the set, which is bounded by `MaxValidators`.
			let validators = BoundedVec::truncate_from(drawn);
			Collators::<T>::put(DrawnCollators { era, validators: validators.clone() });
			validators
		}

		/// Check the pallet invariants.
		#[cfg(any(test, feature = "try-runtime"))]
		pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
			ensure!(
				ValidatorSet::<T>::exists() || PendingRotation::<T>::get() == RotationState::Idle,
				"a rotation is pending without a stored validator set"
			);
			ensure!(
				!ValidatorSet::<T>::exists() || ValidatorSet::<T>::get().is_some(),
				"the stored validator set does not decode, it may exceed `MaxValidators`"
			);
			ensure!(
				!Collators::<T>::exists() ||
					(MaxCollators::<T>::exists() && ValidatorSet::<T>::exists()),
				"drawn collators are stored without both a cap and a validator set"
			);
			ensure!(
				!Collators::<T>::exists() || Collators::<T>::get().is_some(),
				"the drawn collators do not decode"
			);
			// With the pallet wired as `OnSystemEvent`, `on_relay_state_proof` caches a value in
			// every block's inherent and the cache is never removed. So this holds for every draw
			// after the first block that runs the pallet, and flags a runtime without that wiring.
			ensure!(
				!Collators::<T>::exists() || EpochRandomness::<T>::exists(),
				"collators are drawn without a cached relay chain epoch randomness"
			);
			// Until the first forced rotation the drawn collators may be missing or belong to a
			// previous set or cap.
			if PendingRotation::<T>::get() == RotationState::AwaitingQueue {
				return Ok(());
			}
			if let (Some(set), Some(cap)) = (ValidatorSet::<T>::get(), MaxCollators::<T>::get()) {
				let drawn = Collators::<T>::get().ok_or("no collators are drawn under a cap")?;
				ensure!(drawn.era == set.era, "the drawn collators belong to another era");
				let mut seen = BTreeSet::new();
				ensure!(
					drawn.validators.iter().all(|v| seen.insert(v)),
					"the drawn collators contain duplicates"
				);
				ensure!(
					drawn.validators.iter().all(|v| set.validators.contains(v)),
					"a drawn collator is not in the stored validator set"
				);
				ensure!(
					drawn.validators.len() <= cap as usize,
					"more collators are drawn than the cap allows"
				);
			}
			Ok(())
		}
	}

	impl<T: Config> SessionManager<T::AccountId> for Pallet<T> {
		fn new_session(_: SessionIndex) -> Option<Vec<T::AccountId>> {
			let pending = PendingRotation::<T>::get();
			match pending {
				RotationState::AwaitingQueue => {
					PendingRotation::<T>::put(RotationState::AwaitingEnactment)
				},
				RotationState::AwaitingEnactment => PendingRotation::<T>::kill(),
				RotationState::Idle => {},
			}
			let Some(set) = ValidatorSet::<T>::get() else {
				if ValidatorSet::<T>::exists() {
					log::error!(
						target: crate::LOG_TARGET,
						"the stored validator set does not decode"
					);
					Self::deposit_event(Event::StoredSetUndecodable);
				}
				return None;
			};
			// Drawn at the rotation, which pallet-session weighs as the whole block, since the key
			// reads may not fit the weight of the message or report that delivers the set.
			let validators = match MaxCollators::<T>::get() {
				Some(cap) if pending == RotationState::AwaitingQueue => {
					Self::draw_collators(set, cap).into_inner()
				},
				Some(_) => Collators::<T>::get()
					.map(|drawn| drawn.validators.into_inner())
					.unwrap_or_default(),
				None => set.validators.into_iter().collect(),
			};
			Some(validators.into_iter().filter(T::ValidatorRegistration::is_registered).collect())
		}

		fn start_session(_: SessionIndex) {}

		fn end_session(_: SessionIndex) {}
	}

	impl<T: Config> ShouldEndSession<BlockNumberFor<T>> for Pallet<T> {
		fn should_end_session(now: BlockNumberFor<T>) -> bool {
			PendingRotation::<T>::get() != RotationState::Idle ||
				T::PeriodicSession::should_end_session(now)
		}
	}

	impl<T: Config> OnSystemEvent for Pallet<T> {
		fn on_validation_data(_: &PersistedValidationData) {}

		fn on_validation_code_applied() {}

		/// Caches the relay chain randomness of one epoch ago in [`EpochRandomness`].
		///
		/// A proof that shows the key absent caches a zero value. A proof that lacks the trie nodes
		/// of the key, or holds a value that does not decode, panics and so makes the block
		/// invalid.
		fn on_relay_state_proof(relay_state_proof: &RelayChainStateProof) -> Weight {
			let randomness = match relay_state_proof
				.read_entry::<[u8; 32]>(well_known_keys::ONE_EPOCH_AGO_RANDOMNESS, None)
			{
				Ok(randomness) => randomness,
				// No randomness on the relay chain, as before its first epoch change, is cached as
				// the zero value a draw without randomness uses.
				Err(RelayProofError::ReadEntry(ReadEntryErr::Absent)) => {
					log::debug!(
						target: crate::LOG_TARGET,
						"the relay chain state proof shows no epoch randomness, caching a zero value"
					);
					[0; 32]
				},
				Err(RelayProofError::ReadEntry(
					error @ (ReadEntryErr::Proof | ReadEntryErr::Decode),
				)) => panic!(
					"Invalid relay chain epoch randomness in the relay chain state proof: {error:?}"
				),
				// `read_entry` returns only `ReadEntry` errors.
				Err(error) => panic!("Invalid relay chain state proof: {error:?}"),
			};
			if EpochRandomness::<T>::get() != Some(randomness) {
				EpochRandomness::<T>::put(randomness);
			}
			T::WeightInfo::on_relay_state_proof()
		}
	}
}
