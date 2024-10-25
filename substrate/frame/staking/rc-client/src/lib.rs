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

//! # Staking Relay chain client
//!
//! The Staking Relay chain client is used as a interface between the Staking pallet and an external
//! consensus system (e.g. Polkadot's Relay chain).
//!
//! ## Overview
//!
//! The Staking Relay chain client (`rc-client` pallet) implements an abstraction for the i/o
//! of the staking pallet. This abstraction is especially helpfull when the staking pallet
//! and the its "consumers" live in different consensus systems. Most notably, this pallet handles
//! the following i/o tasks:
//!
//! - Communicates a new set of validators to a party an external consensus system;
//! - Communicates setting and purging validator keys to an external consensus system;
//! - Receives and pre-processes offence reports from a trusteed external consensus system;
//! - Receives and pre-processes block authoring reports;
//!
//! This pallet also exposes an extrinsic for signed origins to report staking offences which are
//! communicated to both the staking pallet and an external consensus system.
//!
//!	In sum, this pallet works as an adapter pallet that can be used for the staking pallet to
//!	communicate with external consensus systems.
//!
//! ## Inbound
//!
//! All the inbound request should be performed through extrinsics. External consensus systems may
//! call the inbound extrinsics through XCM transact.
//!
//! ### Block authoring
//!
//! This pallet exposes an extrinsict, [`Call::author`], that processes block authoring events.
//! Block authoring information can only be submitted by the runtime's root origin. Successfull
//! calls will be redirected to staking through the [`pallet_authorship::EventHandler`]) interface.
//!
//! ### Offence reports
//!
//! This pallet exposes an extrinsict, [`Call::report_offence`], that processes priviledged offence
//! reports. These reports can only be submitted by the runtime's root origin. Successfull calls
//! will be redirected to staking through the [`sp_staking::offence::OnOffenceHandler`]) interface.
//!
//! ## Outbound
//!
//!	### Validator keys
//!
//!	This pallet implements a set of extrinsics that handles session key management requests for
//! staking validators. Note, however, that this pallet is not the source of truth for session
//! keys. It only exposes interfaces for accounts to request session key initialization and
//! termination, performs pre-checks and propagates that information to another pallet or consensus
//! system through the [`crate::Config::SessionKeysHandler`].
//!
//!	Callers can request to 1) set validator keys and 2) purge validator keys. These actions *may
//! not be atomic*, i.e., the action and correspoding data may need to be propagated to an external
//! consensus system and take several blocks to be enacted.
//
//!	The session key management actions exposed by this pallet are:
//!
//! 1. **Set session keys**: Extrinsic [`Call::set_session_keys`] alows an account to set its
//!    session key prior to becoming a validator.
//! 2. **Purge session keys**: Extrinsic [`Call::purge_session_keys`] removes any session key(s) of
//!    the caller.
//!
//! ### New set of validator IDs
//!
//! This pallet exposes a configuration type that implements the [`traits::ValidatorSetHandler`],
//! which defines what to do when pallet staking has a new validator set ready. Note, however, that
//! this pallet is not the source of truth for validator sets.
//!
//! ### Signed offence reports

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

/// Re-exports this crate's traits.
pub use traits::*;

use frame_support::{dispatch::Parameter, weights::Weight};
use frame_system::pallet_prelude::*;
use sp_runtime::{
	traits::{Member, OpaqueKeys},
	Perbill,
};

use pallet_authorship::EventHandler as AuthorshipEventHandler;
use pallet_session::SessionManager;
use pallet_staking::SessionInterface;
use sp_staking::{
	offence::{OffenceDetails, OnOffenceHandler},
	SessionIndex,
};

/// An account ID type for the runtime.
pub(crate) type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
/// The type of session keys proof expected by this pallet.
pub(crate) type SessionKeysProof = Vec<u8>;

// TODO:
// - This pallet is the session manager from Staking (not RC-session-pallet)
// 		- All the outbound actions should be a *existing* or new trait
// 		RC Session (impl SessionManager) - Broker(impl SessionManager) - Staking(impl SessionManager)
// - Rename pallet to "Broker" or something else (client is actually the type that sends the XCM,
// which is implemented in the runtime config).
//
// pallet_session needs two traits as config
// - Session Manager: Staking
// - Session Handler: (Babe, grandpa, para_validator, para_assignment, authority_discover, beefy.)
// (rename: relay-chain session proxy OR relay-chain proxy?)
// - + track relay-chain session here (mapping RC session <> Staking session here)
//
// - New inbound message:
// 	- When a new session changes (potentially with a delay) in the RC
//
// pallet-rc-proxy/broker
//  - type AsyncSessionBroker
//  - type OffenceBroker
//
// two traits:
// - XCM relay-chain client (outbound trait) `trait RelayChainClient`
// - XCM inbound trait `trait StakingClient`
// 	- maybe add these traits in `sp-staking` (later)

pub mod traits {
	use sp_staking::offence::OffenceReportSystem;

	use super::*;

	/// Marker trait that encapsulates all the behaviour that an async broker (staking -> relay
	/// chain) must implement.
	pub trait AsyncBroker: AsyncSessionBroker + AsyncOffenceBroker {}

	/// Something that implements a session broker.
	///
	/// It supports the following functionality:
	///
	/// * Handles setting and purging validator session keys.
	/// * Handles a new set of validator keys computed by staking.
	/// * Implements the [`SessionInterface`] trait to manage sessions.
	pub trait AsyncSessionBroker: SessionInterface<Self::AccountId> {
		/// The account ID type.
		type AccountId;

		/// The session keys type that is supported by staking and the relay chain.
		type SessionKeys;

		/// The proof type for [`Self::SessionKeys`].
		type SessionKeysProof;

		/// A bound for the max number of validators in the set.
		type MaxValidatorSet;

		/// The error type.
		type Error;

		// Sets the validator session keys.
		fn set_session_keys(
			who: Self::AccountId,
			session_keys: Self::SessionKeys,
			proof: Self::SessionKeysProof,
		) -> Result<(), Self::Error>;

		/// Purges the validator session keys.
		fn purge_session_keys(who: Self::AccountId) -> Result<(), Self::Error>;

		/// A new validator set has been computed and it is ready to be communicated to the
		/// relay-chain.
		fn new_validator_set(
			session_index: SessionIndex,
			validator_set: sp_runtime::BoundedVec<Self::AccountId, Self::MaxValidatorSet>,
		) -> Result<(), Self::Error>;
	}

	/// Something that implement a offence broker for staking.
	pub trait AsyncOffenceBroker {}
}

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: IsType<<Self as frame_system::Config>::RuntimeEvent> + From<Event<Self>>;

		/// The staking type to redirect inbound calls.
		type Staking: SessionManager<Self::AccountId>
			+ AuthorshipEventHandler<Self::AccountId, BlockNumberFor<Self>>
			+ OnOffenceHandler<Self::AccountId, Self::AccountId, Weight>;

		/// The max offenders a report supports.
		type MaxOffenders: Get<u32>;

		/// The max mumber of validators a [`Self::ValidatorSetHandler`] can operate.
		type MaxValidatorSet: Get<u32>;

		/// The session keys.
		type SessionKeys: OpaqueKeys + Member + Parameter + MaybeSerializeDeserialize;

		/// The async broker that handles the communication and logic with the relay-chain by
		/// sending outbound XCM messages.
		type RelayChainClient: AsyncBroker<
			AccountId = Self::AccountId,
			MaxValidatorSet = Self::MaxValidatorSet,
			SessionKeys = Self::SessionKeys,
			SessionKeysProof = SessionKeysProof,
		>;
	}

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::event]
	pub enum Event<T: Config> {}

	#[pallet::error]
	#[derive(PartialEq)]
	pub enum Error<T> {
		/// Session key set request was unsuccessful.
		SetKeys,
		/// Session key purge request was unsuccessful.
		PurgeKeys,
	}

	/// Keepts track of the active validator set, as seen by the relay chain.
	#[pallet::storage]
	pub type ActiveValidators<T: Config> =
		StorageValue<_, BoundedVec<AccountIdOf<T>, T::MaxValidatorSet>>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Sets the session key(s) of the function caller to `keys`.
		#[pallet::call_index(0)]
		pub fn set_validator_keys(
			origin: OriginFor<T>,
			session_keys: T::SessionKeys,
			proof: Vec<u8>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			// TODO(gpestana): any pre-checks?

			<T::RelayChainClient as AsyncSessionBroker>::set_session_keys(who, session_keys, proof)
				.map_err(|_| Error::<T>::PurgeKeys)?;

			Ok(())
		}

		/// Removes any session key(s) of the function caller.
		#[pallet::call_index(1)]
		pub fn purge_validator_keys(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			// TODO(gpestana): any pre-checks?

			<T::RelayChainClient as AsyncSessionBroker>::purge_session_keys(who)
				.map_err(|_| Error::<T>::PurgeKeys)?;

			Ok(())
		}

		/// Receives block authoring information and redirects it to staking.
		///
		/// Only `RuntimeOrigin::Root` is authorized to call this extrinsic.
		#[pallet::call_index(2)]
		pub fn author(origin: OriginFor<T>, author: T::AccountId) -> DispatchResult {
			let _ = ensure_root(origin);

			<T::Staking as AuthorshipEventHandler<_, _>>::note_author(author);

			Ok(())
		}

		/// Receives offence reports and redirects them to staking.
		///
		/// Only `RuntimeOrigin::Root` is authorized to call this extrinsic.
		#[pallet::call_index(3)]
		pub fn report_offence(
			origin: OriginFor<T>,
			offenders: BoundedVec<OffenceDetails<T::AccountId, T::AccountId>, T::MaxOffenders>,
			slash_fraction: BoundedVec<Perbill, T::MaxOffenders>,
			session: SessionIndex,
		) -> DispatchResult {
			let _ = ensure_root(origin);

			let _weight = <T::Staking as OnOffenceHandler<_, _, _>>::on_offence(
				&offenders,
				&slash_fraction,
				session,
			);

			Ok(())
		}
	}
}

impl<T: Config> SessionInterface<AccountIdOf<T>> for Pallet<T> {
	fn disable_validator(validator_index: u32) -> bool {
		<T::RelayChainClient as SessionInterface<AccountIdOf<T>>>::disable_validator(
			validator_index,
		)
	}

	// TODO: this trait needs to be bounded.
	fn validators() -> Vec<AccountIdOf<T>> {
		ActiveValidators::<T>::get().map(|v| v.into()).unwrap_or_default()
	}

	fn prune_historical_up_to(up_to: SessionIndex) {
		<T::RelayChainClient as SessionInterface<AccountIdOf<T>>>::prune_historical_up_to(up_to);
	}
}
