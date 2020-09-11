// This file is part of Substrate.

// Copyright (C) 2017-2020 Parity Technologies (UK) Ltd.
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

//! # Session Module
//!
//! The Session module allows validators to manage their session keys, provides a function for changing
//! the session length, and handles session rotation.
//!
//! - [`session::Trait`](./trait.Trait.html)
//! - [`Call`](./enum.Call.html)
//! - [`Module`](./struct.Module.html)
//!
//! ## Overview
//!
//! ### Terminology
//! <!-- Original author of paragraph: @gavofyork -->
//!
//! - **Session:** A session is a period of time that has a constant set of validators. Validators can only join
//! or exit the validator set at a session change. It is measured in block numbers. The block where a session is
//! ended is determined by the `ShouldEndSession` trait. When the session is ending, a new validator set
//! can be chosen by `OnSessionEnding` implementations.
//! - **Session key:** A session key is actually several keys kept together that provide the various signing
//! functions required by network authorities/validators in pursuit of their duties.
//! - **Validator ID:** Every account has an associated validator ID. For some simple staking systems, this
//! may just be the same as the account ID. For staking systems using a stash/controller model,
//! the validator ID would be the stash account ID of the controller.
//! - **Session key configuration process:** Session keys are set using `set_keys` for use not in
//! the next session, but the session after next. They are stored in `NextKeys`, a mapping between
//! the caller's `ValidatorId` and the session keys provided. `set_keys` allows users to set their
//! session key prior to being selected as validator.
//! It is a public call since it uses `ensure_signed`, which checks that the origin is a signed account.
//! As such, the account ID of the origin stored in `NextKeys` may not necessarily be associated with
//! a block author or a validator. The session keys of accounts are removed once their account balance is zero.
//! - **Session length:** This pallet does not assume anything about the length of each session.
//! Rather, it relies on an implementation of `ShouldEndSession` to dictate a new session's start.
//! This pallet provides the `PeriodicSessions` struct for simple periodic sessions.
//! - **Session rotation configuration:** Configure as either a 'normal' (rewardable session where rewards are
//! applied) or 'exceptional' (slashable) session rotation.
//! - **Session rotation process:** At the beginning of each block, the `on_initialize` function
//! queries the provided implementation of `ShouldEndSession`. If the session is to end the newly
//! activated validator IDs and session keys are taken from storage and passed to the
//! `SessionHandler`. The validator set supplied by `SessionManager::new_session` and the corresponding session
//! keys, which may have been registered via `set_keys` during the previous session, are written
//! to storage where they will wait one session before being passed to the `SessionHandler`
//! themselves.
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
//! ### Dispatchable Functions
//!
//! - `set_keys` - Set a validator's session keys for upcoming sessions.
//!
//! ### Public Functions
//!
//! - `rotate_session` - Change to the next session. Register the new authority set. Queue changes
//! for next session rotation.
//! - `disable_index` - Disable a validator by index.
//! - `disable` - Disable a validator by Validator ID
//!
//! ## Usage
//!
//! ### Example from the FRAME
//!
//! The [Staking pallet](../pallet_staking/index.html) uses the Session pallet to get the validator set.
//!
//! ```
//! use pallet_session as session;
//!
//! fn validators<T: pallet_session::Trait>() -> Vec<<T as pallet_session::Trait>::ValidatorId> {
//!	<pallet_session::Module<T>>::validators()
//! }
//! # fn main(){}
//! ```
//!
//! ## Related Modules
//!
//! - [Staking](../pallet_staking/index.html)

#![cfg_attr(not(feature = "std"), no_std)]

use sp_std::{prelude::*, marker::PhantomData, ops::{Sub, Rem}};
use codec::Decode;
use sp_runtime::{KeyTypeId, Perbill, RuntimeAppPublic, BoundToRuntimeAppPublic};
use sp_runtime::traits::{Convert, Zero, Member, OpaqueKeys, Saturating};
use sp_staking::SessionIndex;
use frame_support::{
	ensure, decl_module, decl_event, decl_storage, decl_error, ConsensusEngineId, Parameter,
	traits::{
		Get, FindAuthor, ValidatorRegistration, EstimateNextSessionRotation, EstimateNextNewSession,
	},
	dispatch::{self, DispatchResult, DispatchError},
	weights::Weight,
};
use frame_system::ensure_signed;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(feature = "historical")]
pub mod historical;

/// Decides whether the session should be ended.
pub trait ShouldEndSession<BlockNumber> {
	/// Return `true` if the session should be ended.
	fn should_end_session(now: BlockNumber) -> bool;
}

/// Ends the session after a fixed period of blocks.
///
/// The first session will have length of `Offset`, and
/// the following sessions will have length of `Period`.
/// This may prove nonsensical if `Offset` >= `Period`.
pub struct PeriodicSessions<
	Period,
	Offset,
>(PhantomData<(Period, Offset)>);

impl<
	BlockNumber: Rem<Output=BlockNumber> + Sub<Output=BlockNumber> + Zero + PartialOrd,
	Period: Get<BlockNumber>,
	Offset: Get<BlockNumber>,
> ShouldEndSession<BlockNumber> for PeriodicSessions<Period, Offset> {
	fn should_end_session(now: BlockNumber) -> bool {
		let offset = Offset::get();
		now >= offset && ((now - offset) % Period::get()).is_zero()
	}
}

impl<
	BlockNumber: Rem<Output=BlockNumber> + Sub<Output=BlockNumber> + Zero + PartialOrd + Saturating + Clone,
	Period: Get<BlockNumber>,
	Offset: Get<BlockNumber>,
> EstimateNextSessionRotation<BlockNumber> for PeriodicSessions<Period, Offset> {
	fn estimate_next_session_rotation(now: BlockNumber) -> Option<BlockNumber> {
		let offset = Offset::get();
		let period = Period::get();
		Some(if now > offset {
			let block_after_last_session = (now.clone() - offset) % period.clone();
			if block_after_last_session > Zero::zero() {
				now.saturating_add(
					period.saturating_sub(block_after_last_session)
				)
			} else {
				Zero::zero()
			}
		} else {
			offset
		})
	}

	fn weight(_now: BlockNumber) -> Weight {
		// Weight note: `estimate_next_session_rotation` has no storage reads and trivial computational overhead.
		// There should be no risk to the chain having this weight value be zero for now.
		// However, this value of zero was not properly calculated, and so it would be reasonable
		// to come back here and properly calculate the weight of this function.
		0
	}
}

/// A trait for managing creation of new validator set.
pub trait SessionManager<ValidatorId> {
	/// Plan a new session, and optionally provide the new validator set.
	///
	/// Even if the validator-set is the same as before, if any underlying economic
	/// conditions have changed (i.e. stake-weights), the new validator set must be returned.
	/// This is necessary for consensus engines making use of the session module to
	/// issue a validator-set change so misbehavior can be provably associated with the new
	/// economic conditions as opposed to the old.
	/// The returned validator set, if any, will not be applied until `new_index`.
	/// `new_index` is strictly greater than from previous call.
	///
	/// The first session start at index 0.
	///
	/// `new_session(session)` is guaranteed to be called before `end_session(session-1)`.
	fn new_session(new_index: SessionIndex) -> Option<Vec<ValidatorId>>;
	/// End the session.
	///
	/// Because the session pallet can queue validator set the ending session can be lower than the
	/// last new session index.
	fn end_session(end_index: SessionIndex);
	/// Start the session.
	///
	/// The session start to be used for validation
	fn start_session(start_index: SessionIndex);
}

impl<A> SessionManager<A> for () {
	fn new_session(_: SessionIndex) -> Option<Vec<A>> { None }
	fn start_session(_: SessionIndex) {}
	fn end_session(_: SessionIndex) {}
}

/// Handler for session life cycle events.
pub trait SessionHandler<ValidatorId> {
	/// All the key type ids this session handler can process.
	///
	/// The order must be the same as it expects them in
	/// [`on_new_session`](Self::on_new_session<Ks>) and [`on_genesis_session`](Self::on_genesis_session<Ks>).
	const KEY_TYPE_IDS: &'static [KeyTypeId];

	/// The given validator set will be used for the genesis session.
	/// It is guaranteed that the given validator set will also be used
	/// for the second session, therefore the first call to `on_new_session`
	/// should provide the same validator set.
	fn on_genesis_session<Ks: OpaqueKeys>(validators: &[(ValidatorId, Ks)]);

	/// Session set has changed; act appropriately. Note that this can be called
	/// before initialization of your module.
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
	/// Note it is triggered before any `SessionManager::end_session` handlers,
	/// so we can still affect the validator set.
	fn on_before_session_ending() {}

	/// A validator got disabled. Act accordingly until a new session begins.
	fn on_disabled(validator_index: usize);
}

/// A session handler for specific key type.
pub trait OneSessionHandler<ValidatorId>: BoundToRuntimeAppPublic {
	/// The key type expected.
	type Key: Decode + Default + RuntimeAppPublic;

	fn on_genesis_session<'a, I: 'a>(validators: I)
		where I: Iterator<Item=(&'a ValidatorId, Self::Key)>, ValidatorId: 'a;

	/// Session set has changed; act appropriately. Note that this can be called
	/// before initialization of your module.
	///
	/// `changed` is true when at least one of the session keys
	/// or the underlying economic identities/distribution behind one the
	/// session keys has changed, false otherwise.
	///
	/// The `validators` are the validators of the incoming session, and `queued_validators`
	/// will follow.
	fn on_new_session<'a, I: 'a>(
		changed: bool,
		validators: I,
		queued_validators: I,
	) where I: Iterator<Item=(&'a ValidatorId, Self::Key)>, ValidatorId: 'a;


	/// A notification for end of the session.
	///
	/// Note it is triggered before any `SessionManager::end_session` handlers,
	/// so we can still affect the validator set.
	fn on_before_session_ending() {}

	/// A validator got disabled. Act accordingly until a new session begins.
	fn on_disabled(_validator_index: usize);
}

#[impl_trait_for_tuples::impl_for_tuples(1, 30)]
#[tuple_types_no_default_trait_bound]
impl<AId> SessionHandler<AId> for Tuple {
	for_tuples!( where #( Tuple: OneSessionHandler<AId> )* );

	for_tuples!(
		const KEY_TYPE_IDS: &'static [KeyTypeId] = &[ #( <Tuple::Key as RuntimeAppPublic>::ID ),* ];
	);

	fn on_genesis_session<Ks: OpaqueKeys>(validators: &[(AId, Ks)]) {
		for_tuples!(
			#(
				let our_keys: Box<dyn Iterator<Item=_>> = Box::new(validators.iter()
					.map(|k| (&k.0, k.1.get::<Tuple::Key>(<Tuple::Key as RuntimeAppPublic>::ID)
						.unwrap_or_default())));

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
					.map(|k| (&k.0, k.1.get::<Tuple::Key>(<Tuple::Key as RuntimeAppPublic>::ID)
						.unwrap_or_default())));
				let queued_keys: Box<dyn Iterator<Item=_>> = Box::new(queued_validators.iter()
					.map(|k| (&k.0, k.1.get::<Tuple::Key>(<Tuple::Key as RuntimeAppPublic>::ID)
						.unwrap_or_default())));
				Tuple::on_new_session(changed, our_keys, queued_keys);
			)*
		)
	}

	fn on_before_session_ending() {
		for_tuples!( #( Tuple::on_before_session_ending(); )* )
	}

	fn on_disabled(i: usize) {
		for_tuples!( #( Tuple::on_disabled(i); )* )
	}
}

/// `SessionHandler` for tests that use `UintAuthorityId` as `Keys`.
pub struct TestSessionHandler;
impl<AId> SessionHandler<AId> for TestSessionHandler {
	const KEY_TYPE_IDS: &'static [KeyTypeId] = &[sp_runtime::key_types::DUMMY];

	fn on_genesis_session<Ks: OpaqueKeys>(_: &[(AId, Ks)]) {}

	fn on_new_session<Ks: OpaqueKeys>(_: bool, _: &[(AId, Ks)], _: &[(AId, Ks)]) {}

	fn on_before_session_ending() {}

	fn on_disabled(_: usize) {}
}

impl<T: Trait> ValidatorRegistration<T::ValidatorId> for Module<T> {
	fn is_registered(id: &T::ValidatorId) -> bool {
		Self::load_keys(id).is_some()
	}
}

pub trait WeightInfo {
	fn set_keys(n: u32, ) -> Weight;
	fn purge_keys(n: u32, ) -> Weight;
}

impl WeightInfo for () {
	fn set_keys(_n: u32, ) -> Weight { 1_000_000_000 }
	fn purge_keys(_n: u32, ) -> Weight { 1_000_000_000 }
}

pub trait Trait: frame_system::Trait {
	/// The overarching event type.
	type Event: From<Event> + Into<<Self as frame_system::Trait>::Event>;

	/// A stable ID for a validator.
	type ValidatorId: Member + Parameter;

	/// A conversion from account ID to validator ID.
	///
	/// Its cost must be at most one storage read.
	type ValidatorIdOf: Convert<Self::AccountId, Option<Self::ValidatorId>>;

	/// Indicator for when to end the session.
	type ShouldEndSession: ShouldEndSession<Self::BlockNumber>;

	/// Something that can predict the next session rotation. This should typically come from the
	/// same logical unit that provides [`ShouldEndSession`], yet, it gives a best effort estimate.
	/// It is helpful to implement [`EstimateNextNewSession`].
	type NextSessionRotation: EstimateNextSessionRotation<Self::BlockNumber>;

	/// Handler for managing new session.
	type SessionManager: SessionManager<Self::ValidatorId>;

	/// Handler when a session has changed.
	type SessionHandler: SessionHandler<Self::ValidatorId>;

	/// The keys.
	type Keys: OpaqueKeys + Member + Parameter + Default;

	/// The fraction of validators set that is safe to be disabled.
	///
	/// After the threshold is reached `disabled` method starts to return true,
	/// which in combination with `pallet_staking` forces a new era.
	type DisabledValidatorsThreshold: Get<Perbill>;

	/// Weight information for extrinsics in this pallet.
	type WeightInfo: WeightInfo;
}

decl_storage! {
	trait Store for Module<T: Trait> as Session {
		/// The current set of validators.
		Validators get(fn validators): Vec<T::ValidatorId>;

		/// Current index of the session.
		CurrentIndex get(fn current_index): SessionIndex;

		/// True if the underlying economic identities or weighting behind the validators
		/// has changed in the queued validator set.
		QueuedChanged: bool;

		/// The queued keys for the next session. When the next session begins, these keys
		/// will be used to determine the validator's session keys.
		QueuedKeys get(fn queued_keys): Vec<(T::ValidatorId, T::Keys)>;

		/// Indices of disabled validators.
		///
		/// The set is cleared when `on_session_ending` returns a new set of identities.
		DisabledValidators get(fn disabled_validators): Vec<u32>;

		/// The next session keys for a validator.
		NextKeys: map hasher(twox_64_concat) T::ValidatorId => Option<T::Keys>;

		/// The owner of a key. The key is the `KeyTypeId` + the encoded key.
		KeyOwner: map hasher(twox_64_concat) (KeyTypeId, Vec<u8>) => Option<T::ValidatorId>;
	}
	add_extra_genesis {
		config(keys): Vec<(T::AccountId, T::ValidatorId, T::Keys)>;
		build(|config: &GenesisConfig<T>| {
			if T::SessionHandler::KEY_TYPE_IDS.len() != T::Keys::key_ids().len() {
				panic!("Number of keys in session handler and session keys does not match");
			}

			T::SessionHandler::KEY_TYPE_IDS.iter().zip(T::Keys::key_ids()).enumerate()
				.for_each(|(i, (sk, kk))| {
					if sk != kk {
						panic!(
							"Session handler and session key expect different key type at index: {}",
							i,
						);
					}
				});

			for (account, val, keys) in config.keys.iter().cloned() {
				<Module<T>>::inner_set_keys(&val, keys)
					.expect("genesis config must not contain duplicates; qed");
				frame_system::Module::<T>::inc_ref(&account);
			}

			let initial_validators_0 = T::SessionManager::new_session(0)
				.unwrap_or_else(|| {
					frame_support::print("No initial validator provided by `SessionManager`, use \
						session config keys to generate initial validator set.");
					config.keys.iter().map(|x| x.1.clone()).collect()
				});
			assert!(!initial_validators_0.is_empty(), "Empty validator set for session 0 in genesis block!");

			let initial_validators_1 = T::SessionManager::new_session(1)
				.unwrap_or_else(|| initial_validators_0.clone());
			assert!(!initial_validators_1.is_empty(), "Empty validator set for session 1 in genesis block!");

			let queued_keys: Vec<_> = initial_validators_1
				.iter()
				.cloned()
				.map(|v| (
					v.clone(),
					<Module<T>>::load_keys(&v).unwrap_or_default(),
				))
				.collect();

			// Tell everyone about the genesis session keys
			T::SessionHandler::on_genesis_session::<T::Keys>(&queued_keys);

			<Validators<T>>::put(initial_validators_0);
			<QueuedKeys<T>>::put(queued_keys);

			T::SessionManager::start_session(0);
		});
	}
}

decl_event!(
	pub enum Event {
		/// New session has happened. Note that the argument is the \[session_index\], not the block
		/// number as the type might suggest.
		NewSession(SessionIndex),
	}
);

decl_error! {
	/// Error for the session module.
	pub enum Error for Module<T: Trait> {
		/// Invalid ownership proof.
		InvalidProof,
		/// No associated validator ID for account.
		NoAssociatedValidatorId,
		/// Registered duplicate key.
		DuplicatedKey,
		/// No keys are associated with this account.
		NoKeys,
	}
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		type Error = Error<T>;

		fn deposit_event() = default;

		/// Sets the session key(s) of the function caller to `keys`.
		/// Allows an account to set its session key prior to becoming a validator.
		/// This doesn't take effect until the next session.
		///
		/// The dispatch origin of this function must be signed.
		///
		/// # <weight>
		/// - Complexity: `O(1)`
		///   Actual cost depends on the number of length of `T::Keys::key_ids()` which is fixed.
		/// - DbReads: `origin account`, `T::ValidatorIdOf`, `NextKeys`
		/// - DbWrites: `origin account`, `NextKeys`
		/// - DbReads per key id: `KeyOwner`
		/// - DbWrites per key id: `KeyOwner`
		/// # </weight>
		#[weight = 200_000_000
			+ T::DbWeight::get().reads(2 + T::Keys::key_ids().len() as Weight)
			+ T::DbWeight::get().writes(1 + T::Keys::key_ids().len() as Weight)]
		pub fn set_keys(origin, keys: T::Keys, proof: Vec<u8>) -> dispatch::DispatchResult {
			let who = ensure_signed(origin)?;

			ensure!(keys.ownership_proof_is_valid(&proof), Error::<T>::InvalidProof);

			Self::do_set_keys(&who, keys)?;

			Ok(())
		}

		/// Removes any session key(s) of the function caller.
		/// This doesn't take effect until the next session.
		///
		/// The dispatch origin of this function must be signed.
		///
		/// # <weight>
		/// - Complexity: `O(1)` in number of key types.
		///   Actual cost depends on the number of length of `T::Keys::key_ids()` which is fixed.
		/// - DbReads: `T::ValidatorIdOf`, `NextKeys`, `origin account`
		/// - DbWrites: `NextKeys`, `origin account`
		/// - DbWrites per key id: `KeyOwnder`
		/// # </weight>
		#[weight = 120_000_000
			+ T::DbWeight::get().reads_writes(2, 1 + T::Keys::key_ids().len() as Weight)]
		pub fn purge_keys(origin) {
			let who = ensure_signed(origin)?;
			Self::do_purge_keys(&who)?;
		}

		/// Called when a block is initialized. Will rotate session if it is the last
		/// block of the current session.
		fn on_initialize(n: T::BlockNumber) -> Weight {
			if T::ShouldEndSession::should_end_session(n) {
				Self::rotate_session();
				T::MaximumBlockWeight::get()
			} else {
				// NOTE: the non-database part of the weight for `should_end_session(n)` is
				// included as weight for empty block, the database part is expected to be in
				// cache.
				0
			}
		}
	}
}

impl<T: Trait> Module<T> {
	/// Move on to next session. Register new validator set and session keys. Changes
	/// to the validator set have a session of delay to take effect. This allows for
	/// equivocation punishment after a fork.
	pub fn rotate_session() {
		let session_index = CurrentIndex::get();

		let changed = QueuedChanged::get();

		// Inform the session handlers that a session is going to end.
		T::SessionHandler::on_before_session_ending();

		T::SessionManager::end_session(session_index);

		// Get queued session keys and validators.
		let session_keys = <QueuedKeys<T>>::get();
		let validators = session_keys.iter()
			.map(|(validator, _)| validator.clone())
			.collect::<Vec<_>>();
		<Validators<T>>::put(&validators);

		if changed {
			// reset disabled validators
			DisabledValidators::take();
		}

		// Increment session index.
		let session_index = session_index + 1;
		CurrentIndex::put(session_index);

		T::SessionManager::start_session(session_index);

		// Get next validator set.
		let maybe_next_validators = T::SessionManager::new_session(session_index + 1);
		let (next_validators, next_identities_changed)
			= if let Some(validators) = maybe_next_validators
		{
			// NOTE: as per the documentation on `OnSessionEnding`, we consider
			// the validator set as having changed even if the validators are the
			// same as before, as underlying economic conditions may have changed.
			(validators, true)
		} else {
			(<Validators<T>>::get(), false)
		};

		// Queue next session keys.
		let (queued_amalgamated, next_changed) = {
			// until we are certain there has been a change, iterate the prior
			// validators along with the current and check for changes
			let mut changed = next_identities_changed;

			let mut now_session_keys = session_keys.iter();
			let mut check_next_changed = |keys: &T::Keys| {
				if changed { return }
				// since a new validator set always leads to `changed` starting
				// as true, we can ensure that `now_session_keys` and `next_validators`
				// have the same length. this function is called once per iteration.
				if let Some(&(_, ref old_keys)) = now_session_keys.next() {
					if old_keys != keys {
						changed = true;
						return
					}
				}
			};
			let queued_amalgamated = next_validators.into_iter()
				.map(|a| {
					let k = Self::load_keys(&a).unwrap_or_default();
					check_next_changed(&k);
					(a, k)
				})
				.collect::<Vec<_>>();

			(queued_amalgamated, changed)
		};

		<QueuedKeys<T>>::put(queued_amalgamated.clone());
		QueuedChanged::put(next_changed);

		// Record that this happened.
		Self::deposit_event(Event::NewSession(session_index));

		// Tell everyone about the new session keys.
		T::SessionHandler::on_new_session::<T::Keys>(
			changed,
			&session_keys,
			&queued_amalgamated,
		);
	}

	/// Disable the validator of index `i`.
	///
	/// Returns `true` if this causes a `DisabledValidatorsThreshold` of validators
	/// to be already disabled.
	pub fn disable_index(i: usize) -> bool {
		let (fire_event, threshold_reached) = DisabledValidators::mutate(|disabled| {
			let i = i as u32;
			if let Err(index) = disabled.binary_search(&i) {
				let count = <Validators<T>>::decode_len().unwrap_or(0) as u32;
				let threshold = T::DisabledValidatorsThreshold::get() * count;
				disabled.insert(index, i);
				(true, disabled.len() as u32 > threshold)
			} else {
				(false, false)
			}
		});

		if fire_event {
			T::SessionHandler::on_disabled(i);
		}

		threshold_reached
	}

	/// Disable the validator identified by `c`. (If using with the staking module,
	/// this would be their *stash* account.)
	///
	/// Returns `Ok(true)` if more than `DisabledValidatorsThreshold` validators in current
	/// session is already disabled.
	/// If used with the staking module it allows to force a new era in such case.
	pub fn disable(c: &T::ValidatorId) -> sp_std::result::Result<bool, ()> {
		Self::validators().iter().position(|i| i == c).map(Self::disable_index).ok_or(())
	}

	/// Perform the set_key operation, checking for duplicates. Does not set `Changed`.
	///
	/// This ensures that the reference counter in system is incremented appropriately and as such
	/// must accept an account ID, rather than a validator ID.
	fn do_set_keys(account: &T::AccountId, keys: T::Keys) -> dispatch::DispatchResult {
		let who = T::ValidatorIdOf::convert(account.clone())
			.ok_or(Error::<T>::NoAssociatedValidatorId)?;

		let old_keys = Self::inner_set_keys(&who, keys)?;
		if old_keys.is_none() {
			frame_system::Module::<T>::inc_ref(&account);
		}

		Ok(())
	}

	/// Perform the set_key operation, checking for duplicates. Does not set `Changed`.
	///
	/// The old keys for this validator are returned, or `None` if there were none.
	///
	/// This does not ensure that the reference counter in system is incremented appropriately, it
	/// must be done by the caller or the keys will be leaked in storage.
	fn inner_set_keys(who: &T::ValidatorId, keys: T::Keys) -> Result<Option<T::Keys>, DispatchError> {
		let old_keys = Self::load_keys(who);

		for id in T::Keys::key_ids() {
			let key = keys.get_raw(*id);

			// ensure keys are without duplication.
			ensure!(
				Self::key_owner(*id, key).map_or(true, |owner| &owner == who),
				Error::<T>::DuplicatedKey,
			);

			if let Some(old) = old_keys.as_ref().map(|k| k.get_raw(*id)) {
				if key == old {
					continue;
				}

				Self::clear_key_owner(*id, old);
			}

			Self::put_key_owner(*id, key, who);
		}

		Self::put_keys(who, &keys);
		Ok(old_keys)
	}

	fn do_purge_keys(account: &T::AccountId) -> DispatchResult {
		let who = T::ValidatorIdOf::convert(account.clone())
			.ok_or(Error::<T>::NoAssociatedValidatorId)?;

		let old_keys = Self::take_keys(&who).ok_or(Error::<T>::NoKeys)?;
		for id in T::Keys::key_ids() {
			let key_data = old_keys.get_raw(*id);
			Self::clear_key_owner(*id, key_data);
		}
		frame_system::Module::<T>::dec_ref(&account);

		Ok(())
	}

	fn load_keys(v: &T::ValidatorId) -> Option<T::Keys> {
		<NextKeys<T>>::get(v)
	}

	fn take_keys(v: &T::ValidatorId) -> Option<T::Keys> {
		<NextKeys<T>>::take(v)
	}

	fn put_keys(v: &T::ValidatorId, keys: &T::Keys) {
		<NextKeys<T>>::insert(v, keys);
	}

	fn key_owner(id: KeyTypeId, key_data: &[u8]) -> Option<T::ValidatorId> {
		<KeyOwner<T>>::get((id, key_data))
	}

	fn put_key_owner(id: KeyTypeId, key_data: &[u8], v: &T::ValidatorId) {
		<KeyOwner<T>>::insert((id, key_data), v)
	}

	fn clear_key_owner(id: KeyTypeId, key_data: &[u8]) {
		<KeyOwner<T>>::remove((id, key_data));
	}
}

/// Wraps the author-scraping logic for consensus engines that can recover
/// the canonical index of an author. This then transforms it into the
/// registering account-ID of that session key index.
pub struct FindAccountFromAuthorIndex<T, Inner>(sp_std::marker::PhantomData<(T, Inner)>);

impl<T: Trait, Inner: FindAuthor<u32>> FindAuthor<T::ValidatorId>
	for FindAccountFromAuthorIndex<T, Inner>
{
	fn find_author<'a, I>(digests: I) -> Option<T::ValidatorId>
		where I: 'a + IntoIterator<Item=(ConsensusEngineId, &'a [u8])>
	{
		let i = Inner::find_author(digests)?;

		let validators = <Module<T>>::validators();
		validators.get(i as usize).map(|k| k.clone())
	}
}

impl<T: Trait> EstimateNextNewSession<T::BlockNumber> for Module<T> {
	/// This session module always calls new_session and next_session at the same time, hence we
	/// do a simple proxy and pass the function to next rotation.
	fn estimate_next_new_session(now: T::BlockNumber) -> Option<T::BlockNumber> {
		T::NextSessionRotation::estimate_next_session_rotation(now)
	}

	fn weight(now: T::BlockNumber) -> Weight {
		T::NextSessionRotation::weight(now)
	}
}
