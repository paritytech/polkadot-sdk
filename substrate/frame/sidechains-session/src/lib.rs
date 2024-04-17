// This file was copied from substrate/frame/session/src/lib.rs and heavily modified.
// History of modifications can be seen by comparing the two files.

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

//! # Session Pallet
//!
//! The Session pallet allows validators to manage their session keys, provides a function for
//! changing the session length, and handles session rotation.
//!
//! - [`Config`]
//! - [`Call`]
//! - [`Pallet`]
//!
//! ## Overview
//!
//! ### Terminology
//! <!-- Original author of paragraph: @gavofyork -->
//!
//! - **Session:** A session is a period of time that has a constant set of validators. Validators
//!   can only join or exit the validator set at a session change. It is measured in block numbers.
//!   The block where a session is ended is determined by the `ShouldEndSession` trait. When the
//!   session is ending, a new validator set can be chosen by `OnSessionEnding` implementations.
//!
//! - **Session key:** A session key is actually several keys kept together that provide the various
//!   signing functions required by network authorities/validators in pursuit of their duties.
//! - **Validator ID:** Every account has an associated validator ID. For some simple staking
//!   systems, this may just be the same as the account ID. For staking systems using a
//!   stash/controller model, the validator ID would be the stash account ID of the controller.
//!
//! - **Session key configuration process:** Session keys are set using `set_keys` for use not in
//!   the next session, but the session after next. They are stored in `NextKeys`, a mapping between
//!   the caller's `ValidatorId` and the session keys provided. `set_keys` allows users to set their
//!   session key prior to being selected as validator. It is a public call since it uses
//!   `ensure_signed`, which checks that the origin is a signed account. As such, the account ID of
//!   the origin stored in `NextKeys` may not necessarily be associated with a block author or a
//!   validator. The session keys of accounts are removed once their account balance is zero.
//!
//! - **Session length:** This pallet does not assume anything about the length of each session.
//!   Rather, it relies on an implementation of `ShouldEndSession` to dictate a new session's start.
//!   This pallet provides the `PeriodicSessions` struct for simple periodic sessions.
//!
//! - **Session rotation configuration:** Configure as either a 'normal' (rewardable session where
//!   rewards are applied) or 'exceptional' (slashable) session rotation.
//!
//! - **Session rotation process:** At the beginning of each block, the `on_initialize` function
//!   queries the provided implementation of `ShouldEndSession`. If the session is to end the newly
//!   activated validator IDs and session keys are taken from storage and passed to the
//!   `SessionHandler`. The validator set supplied by `SessionManager::new_session` and the
//!   corresponding session keys, which may have been registered via `set_keys` during the previous
//!   session, are written to storage where they will wait one session before being passed to the
//!   `SessionHandler` themselves.
//!
//! ### Goals
//!
//! The Session pallet is designed to make the following possible:
//!
//! - Set session keys of the validator set for upcoming sessions.
//! - Control the length of sessions.
//! - Configure and switch between either normal or exceptional session rotations.
//!
//! ## Interface
//!
//!
//! ### Public Functions
//!
//! - `rotate_session` - Change to the next session. Register the new authority set.
//! - `disable_index` - Disable a validator by index.
//! - `disable` - Disable a validator by Validator ID
//!
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::collections::BTreeSet;
use frame_support::{
	traits::{
		EstimateNextNewSession, EstimateNextSessionRotation, OneSessionHandler,
		ValidatorRegistration,
	},
	weights::Weight,
};
use frame_system::pallet_prelude::BlockNumberFor;
use frame_system::DecRefStatus;
pub use pallet::*;
use sp_runtime::{traits::OpaqueKeys, DispatchError, KeyTypeId, RuntimeAppPublic};
use sp_staking::SessionIndex;
use sp_std::prelude::*;

/// Decides whether the session should be ended.
pub trait ShouldEndSession<BlockNumber> {
	/// Return `true` if the session should be ended.
	fn should_end_session(now: BlockNumber) -> bool;
}

/// A trait for managing creation of new validator set.
pub trait SessionManager<ValidatorId, Keys> {
	/// Plan a new session, and optionally provide the new validator set.
	///
	/// Even if the validator-set is the same as before, if any underlying economic conditions have
	/// changed (i.e. stake-weights), the new validator set must be returned. This is necessary for
	/// consensus engines making use of the session pallet to issue a validator-set change so
	/// misbehavior can be provably associated with the new economic conditions as opposed to the
	/// old. The returned validator set, if any, will not be applied until `new_index`. `new_index`
	/// is strictly greater than from previous call.
	///
	/// The first session start at index 0.
	///
	/// `new_session(session)` is guaranteed to be called before `end_session(session-1)`. In other
	/// words, a new session must always be planned before an ongoing one can be finished.
	fn new_session(new_index: SessionIndex) -> Option<Vec<(ValidatorId, Keys)>>;
	/// Same as `new_session`, but it this should only be called at genesis.
	///
	/// The session manager might decide to treat this in a different way. Default impl is simply
	/// using [`new_session`](Self::new_session).
	fn new_session_genesis(new_index: SessionIndex) -> Option<Vec<(ValidatorId, Keys)>> {
		Self::new_session(new_index)
	}
	/// End the session.
	///
	/// Because the session pallet can queue validator set the ending session can be lower than the
	/// last new session index.
	fn end_session(end_index: SessionIndex);
	/// Start an already planned session.
	///
	/// The session start to be used for validation.
	fn start_session(start_index: SessionIndex);
}

impl<A, B> SessionManager<A, B> for () {
	fn new_session(_: SessionIndex) -> Option<Vec<(A, B)>> {
		None
	}
	fn start_session(_: SessionIndex) {}
	fn end_session(_: SessionIndex) {}
}

/// Handler for session life cycle events.
pub trait SessionHandler<ValidatorId> {
	/// All the key type ids this session handler can process.
	///
	/// The order must be the same as it expects them in
	/// [`on_new_session`](Self::on_new_session<Ks>) and
	/// [`on_genesis_session`](Self::on_genesis_session<Ks>).
	const KEY_TYPE_IDS: &'static [KeyTypeId];

	/// The given validator set will be used for the genesis session.
	/// It is guaranteed that the given validator set will also be used
	/// for the second session, therefore the first call to `on_new_session`
	/// should provide the same validator set.
	fn on_genesis_session<Ks: OpaqueKeys>(validators: &[(ValidatorId, Ks)]);

	/// Session set has changed; act appropriately. Note that this can be called
	/// before initialization of your pallet.
	///
	/// `changed` is true whenever any of the session keys or underlying economic
	/// identities or weightings behind those keys has changed.
	fn on_new_session<Ks: OpaqueKeys>(
		changed: bool,
		validators: &[(ValidatorId, Ks)],
		queued_validators: &[(ValidatorId, Ks)],
	);

	/// A notification for end of the session.
	///
	/// Note it is triggered before any [`SessionManager::end_session`] handlers,
	/// so we can still affect the validator set.
	fn on_before_session_ending() {}

	/// A validator got disabled. Act accordingly until a new session begins.
	fn on_disabled(validator_index: u32);
}

#[impl_trait_for_tuples::impl_for_tuples(1, 30)]
#[tuple_types_custom_trait_bound(OneSessionHandler<AId>)]
impl<AId> SessionHandler<AId> for Tuple {
	for_tuples!(
		const KEY_TYPE_IDS: &'static [KeyTypeId] = &[ #( <Tuple::Key as RuntimeAppPublic>::ID ),* ];
	);

	fn on_genesis_session<Ks: OpaqueKeys>(validators: &[(AId, Ks)]) {
		for_tuples!(
			#(
				let our_keys: Box<dyn Iterator<Item=_>> = Box::new(validators.iter()
					.filter_map(|k|
						k.1.get::<Tuple::Key>(<Tuple::Key as RuntimeAppPublic>::ID).map(|k1| (&k.0, k1))
					)
				);

				Tuple::on_genesis_session(our_keys);
			)*
		)
	}

	fn on_new_session<Ks: OpaqueKeys>(
		changed: bool,
		validators: &[(AId, Ks)],
		queued_validators: &[(AId, Ks)],
	) {
		for_tuples!(
			#(
				let our_keys: Box<dyn Iterator<Item=_>> = Box::new(validators.iter()
					.filter_map(|k|
						k.1.get::<Tuple::Key>(<Tuple::Key as RuntimeAppPublic>::ID).map(|k1| (&k.0, k1))
					));
				let queued_keys: Box<dyn Iterator<Item=_>> = Box::new(queued_validators.iter()
					.filter_map(|k|
						k.1.get::<Tuple::Key>(<Tuple::Key as RuntimeAppPublic>::ID).map(|k1| (&k.0, k1))
					));
				Tuple::on_new_session(changed, our_keys, queued_keys);
			)*
		)
	}

	fn on_before_session_ending() {
		for_tuples!( #( Tuple::on_before_session_ending(); )* )
	}

	fn on_disabled(i: u32) {
		for_tuples!( #( Tuple::on_disabled(i); )* )
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;

	/// The current storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// A stable ID for a validator.
		type ValidatorId: Member
			+ Parameter
			+ MaybeSerializeDeserialize
			+ MaxEncodedLen
			+ Into<Self::AccountId>;

		/// Indicator for when to end the session.
		type ShouldEndSession: ShouldEndSession<BlockNumberFor<Self>>;

		/// Something that can predict the next session rotation. This should typically come from
		/// the same logical unit that provides [`ShouldEndSession`], yet, it gives a best effort
		/// estimate. It is helpful to implement [`EstimateNextNewSession`].
		type NextSessionRotation: EstimateNextSessionRotation<BlockNumberFor<Self>>;

		/// Handler for managing new session.
		type SessionManager: SessionManager<Self::ValidatorId, Self::Keys>;

		/// Handler when a session has changed.
		type SessionHandler: SessionHandler<Self::ValidatorId>;

		/// The keys.
		type Keys: OpaqueKeys + Member + Parameter + MaybeSerializeDeserialize;
	}

	pub type ValidatorList<T> = Vec<<T as Config>::ValidatorId>;
	pub type ValidatorAndKeysList<T> = Vec<(<T as Config>::ValidatorId, <T as Config>::Keys)>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub initial_validators: ValidatorAndKeysList<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			if T::SessionHandler::KEY_TYPE_IDS.len() != T::Keys::key_ids().len() {
				panic!("Number of keys in session handler and session keys does not match");
			}

			T::SessionHandler::KEY_TYPE_IDS
				.iter()
				.zip(T::Keys::key_ids())
				.enumerate()
				.for_each(|(i, (sk, kk))| {
					if sk != kk {
						panic!(
							"Session handler and session key expect different key type at index: {}",
							i,
						);
					}
				});

			let maybe_genesis_validators = Pallet::<T>::new_session_genesis(0);
			let initial_validators = match &maybe_genesis_validators {
				Some(validators) => validators,
				None => {
					frame_support::print(
						"No initial validator provided by `SessionManager`, use \
						session config keys to generate initial validator set.",
					);
					&self.initial_validators
				},
			};
			Pallet::<T>::rotate_validators(initial_validators);
			T::SessionHandler::on_genesis_session::<T::Keys>(initial_validators);
			T::SessionManager::start_session(0);
		}
	}

	#[pallet::storage]
	#[pallet::getter(fn validators)]
	// This storage is only needed to keep compatibility with Polkadot.js
	pub type Validators<T: Config> = StorageValue<_, ValidatorList<T>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn validators_and_keys)]
	pub type ValidatorsAndKeys<T: Config> = StorageValue<_, ValidatorAndKeysList<T>, ValueQuery>;

	/// Current index of the session.
	#[pallet::storage]
	#[pallet::getter(fn current_index)]
	pub type CurrentIndex<T> = StorageValue<_, SessionIndex, ValueQuery>;

	/// Indices of disabled validators.
	///
	/// The vec is always kept sorted so that we can find whether a given validator is
	/// disabled using binary search. It gets cleared when `on_session_ending` returns
	/// a new set of identities.
	#[pallet::storage]
	#[pallet::getter(fn disabled_validators)]
	pub type DisabledValidators<T> = StorageValue<_, Vec<u32>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event {
		/// New session has happened. Note that the argument is the session index, not the
		/// block number as the type might suggest.
		NewSession { session_index: SessionIndex },
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Called when a block is initialized. Will rotate session if it is the last
		/// block of the current session.
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			if T::ShouldEndSession::should_end_session(n) {
				Self::rotate_session();
				T::BlockWeights::get().max_block
			} else {
				// NOTE: the non-database part of the weight for `should_end_session(n)` is
				// included as weight for empty block, the database part is expected to be in
				// cache.
				Weight::zero()
			}
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Move on to next session. Register new validator set with session keys.
	pub fn rotate_session() {
		let session_index = <CurrentIndex<T>>::get();
		log::trace!(target: "runtime::session", "rotating session {:?}", session_index);

		T::SessionHandler::on_before_session_ending();
		T::SessionManager::end_session(session_index);

		let session_index = session_index + 1;
		<CurrentIndex<T>>::put(session_index);

		let (validators, changed) = if let Some(validators) = Self::new_session(session_index) {
			Self::rotate_validators(&validators);
			(validators, true)
		} else {
			(ValidatorsAndKeys::<T>::get(), false)
		};

		T::SessionManager::start_session(session_index);
		Self::deposit_event(Event::NewSession { session_index });
		// TODO if possible, remove queued_validators from SessionHandler (both Aura and Grandpa aren't using them anyway)
		T::SessionHandler::on_new_session::<T::Keys>(changed, validators.as_ref(), &[]);
	}

	/// Disable the validator of index `i`, returns `false` if the validator was already disabled.
	pub fn disable_index(i: u32) -> bool {
		if i >= Validators::<T>::decode_len().unwrap_or(0) as u32 {
			return false;
		}

		<DisabledValidators<T>>::mutate(|disabled| {
			if let Err(index) = disabled.binary_search(&i) {
				disabled.insert(index, i);
				T::SessionHandler::on_disabled(i);
				return true;
			}

			false
		})
	}

	/// Disable the validator identified by `c`. (If using with the staking pallet,
	/// this would be their *stash* account.)
	///
	/// Returns `false` either if the validator could not be found or it was already
	/// disabled.
	pub fn disable(c: &T::ValidatorId) -> bool {
		Self::validators_and_keys()
			.iter()
			.position(|(i, _)| i == c)
			.map(|i| Self::disable_index(i as u32))
			.unwrap_or(false)
	}

	pub fn acc_ids(validators: &[(T::ValidatorId, T::Keys)]) -> Vec<T::AccountId> {
		validators.iter().map(|(v_id, _)| v_id.clone().into()).collect()
	}
	fn inc_provider(account: &T::AccountId) {
		frame_system::Pallet::<T>::inc_providers(account);
	}

	fn dec_provider(account: &T::AccountId) -> Result<DecRefStatus, DispatchError> {
		frame_system::Pallet::<T>::dec_providers(account)
	}

	fn change_account_providers(new_ids: &[T::AccountId], old_ids: &[T::AccountId]) {
		let new_ids = BTreeSet::from_iter(new_ids);
		let old_ids = BTreeSet::from_iter(old_ids);
		let to_inc = new_ids.difference(&old_ids);
		let to_dec = old_ids.difference(&new_ids);
		for account in to_inc {
			Self::inc_provider(account);
		}
		for account in to_dec {
			Self::dec_provider(account).expect("We always match dec_providers with corresponding inc_providers, thus it cannot fail");
		}
	}

	fn rotate_validators(new_validators: &ValidatorAndKeysList<T>) {
		ValidatorsAndKeys::<T>::put(new_validators);
		#[cfg(feature = "polkadot-js-compat")]
		{
			let validator_ids: Vec<_> = new_validators.iter().cloned().map(|v| v.0).collect();
			// This storage is not used for chain operation but is required by
			// Polkadot.js to show block producers in the explorer
			Validators::<T>::put(validator_ids);
		}
		Self::change_account_providers(
			&Self::acc_ids(new_validators),
			&Self::acc_ids(&ValidatorsAndKeys::<T>::get()),
		);
		<DisabledValidators<T>>::take();
	}

	pub fn new_session(index: SessionIndex) -> Option<ValidatorAndKeysList<T>> {
		let validators = T::SessionManager::new_session(index)?;
		Some(validators)
	}

	pub fn new_session_genesis(index: SessionIndex) -> Option<ValidatorAndKeysList<T>> {
		let validators = T::SessionManager::new_session_genesis(index)?;
		Some(validators)
	}
}

impl<T: Config> ValidatorRegistration<T::ValidatorId> for Pallet<T> {
	fn is_registered(id: &T::ValidatorId) -> bool {
		Self::validators_and_keys().iter().any(|(vid, _)| vid == id)
	}
}

impl<T: Config> EstimateNextNewSession<BlockNumberFor<T>> for Pallet<T> {
	fn average_session_length() -> BlockNumberFor<T> {
		T::NextSessionRotation::average_session_length()
	}

	/// This session pallet always calls new_session and next_session at the same time, hence we
	/// do a simple proxy and pass the function to next rotation.
	fn estimate_next_new_session(now: BlockNumberFor<T>) -> (Option<BlockNumberFor<T>>, Weight) {
		T::NextSessionRotation::estimate_next_session_rotation(now)
	}
}

impl<T: Config> frame_support::traits::DisabledValidators for Pallet<T> {
	fn is_disabled(index: u32) -> bool {
		<Pallet<T>>::disabled_validators().binary_search(&index).is_ok()
	}

	fn disabled_validators() -> Vec<u32> {
		<Pallet<T>>::disabled_validators()
	}
}
