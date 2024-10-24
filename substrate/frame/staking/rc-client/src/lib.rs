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

/// Re-exports this crate's trait implementations.
pub use impls::*;
/// Re-exports this crate's traits.
pub use traits::*;

use frame_support::{dispatch::Parameter, weights::Weight};
use frame_system::pallet_prelude::*;
use sp_runtime::{
	traits::{Member, OpaqueKeys},
	Perbill,
};

use pallet_authorship::EventHandler as AuthorshipEventHandler;
use sp_staking::{
	offence::{OffenceDetails, OnOffenceHandler},
	SessionIndex,
};

/// The type of session key proof expected by this pallet.
pub(crate) type SessionKeyProof = Vec<u8>;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: IsType<<Self as frame_system::Config>::RuntimeEvent> + From<Event<Self>>;

		/// The staking interface.
		type Staking: AuthorshipEventHandler<Self::AccountId, BlockNumberFor<Self>>
			+ OnOffenceHandler<Self::AccountId, Self::AccountId, Weight>;

		/// The max offenders a report supports.
		type MaxOffenders: Get<u32>;

		/// The session keys.
		type SessionKeys: OpaqueKeys + Member + Parameter + MaybeSerializeDeserialize;

		/// The session keys handler that requests from this pallet.
		type SessionKeysHandler: SessionKeysHandler<
			AccountId = Self::AccountId,
			Keys = Self::SessionKeys,
			Proof = SessionKeyProof,
		>;

		/// The max mumber of validators a [`Self::ValidatorSetHandler`] can operate.
		type MaxValidators: Get<u32>;

		/// An handler for when a new validator set must be enacted.
		type ValidatorSetHandler: ValidatorSetHandler<
			AccountId = Self::AccountId,
			MaxValidators = Self::MaxValidators,
		>;
	}

	#[pallet::pallet]
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

			<T::SessionKeysHandler as SessionKeysHandler>::set_keys(who, session_keys, proof)
				.map_err(|_| Error::<T>::SetKeys)?;

			todo!()
		}

		/// Removes any session key(s) of the function caller.
		#[pallet::call_index(1)]
		pub fn purge_validator_keys(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			// TODO(gpestana): any pre-checks?

			<T::SessionKeysHandler as SessionKeysHandler>::purge_keys(who)
				.map_err(|_| Error::<T>::PurgeKeys)?;

			Ok(())
		}

		/// Receives block authoring information and redirects it to staking.
		///
		/// Only `RuntimeOrigin::Root` is authorized to call this extrinsic.
		#[pallet::call_index(2)]
		pub fn author(origin: OriginFor<T>, author: T::AccountId) -> DispatchResult {
			let _ = ensure_root(origin);

			// TODO: (perhaps?) instead of calling directly staking, batch authoring points instead
			// and use `on_initialize` or some other mechanism to notify staking of a set of
			// authoring notes.

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

pub mod traits {
	use super::*;

	/// Something that handles the management of session keys.
	///
	/// It allows to define the behaviour when new session keys are set and purged.
	pub trait SessionKeysHandler {
		/// The account ID type.
		type AccountId;
		/// The keys type that is supported by the manager.
		type Keys;
		/// The proof type for [`Self::Keys`].
		type Proof;
		/// The error type.
		type Error;

		fn set_keys(
			who: Self::AccountId,
			session_keys: Self::Keys,
			proof: Self::Proof,
		) -> Result<(), Self::Error>;

		fn purge_keys(who: Self::AccountId) -> Result<(), Self::Error>;
	}

	/// Something that handles a new validator set.
	pub trait ValidatorSetHandler {
		/// The account ID type.
		type AccountId;
		/// The max number of validators the provider can return.
		type MaxValidators;
		/// The error type.
		type Error;

		/// A new validator set is ready.
		fn new_validator_set(
			session_index: SessionIndex,
			validator_set: sp_runtime::BoundedVec<Self::AccountId, Self::MaxValidators>,
		) -> Result<(), Self::Error>;
	}
}

pub mod impls {
	use std::marker::PhantomData;

	use super::{
		pallet::{Config, Error as PalletError},
		*,
	};

	/// Propagates session key management actions and data through XCM.
	pub struct SessionKeysHandlerXCM<T: Config>(PhantomData<T>);

	impl<T: Config> SessionKeysHandler for SessionKeysHandlerXCM<T> {
		type AccountId = T::AccountId;
		type Keys = T::SessionKeys;
		type Proof = SessionKeyProof;
		type Error = PalletError<T>;

		fn set_keys(
			_who: Self::AccountId,
			_session_keys: Self::Keys,
			_proof: Self::Proof,
		) -> Result<(), Self::Error> {
			todo!()
		}

		fn purge_keys(_who: Self::AccountId) -> Result<(), Self::Error> {
			todo!()
		}
	}

	/// Propagates a new set of validators through XCM.
	pub struct ValidatorSetHandlerXCM<T: Config>(PhantomData<T>);

	impl<T: Config> ValidatorSetHandler for ValidatorSetHandlerXCM<T> {
		type AccountId = T::AccountId;
		type MaxValidators = T::MaxValidators;
		type Error = PalletError<T>;

		fn new_validator_set(
			_session: SessionIndex,
			_validator_set: sp_runtime::BoundedVec<Self::AccountId, Self::MaxValidators>,
		) -> Result<(), Self::Error> {
			// TODO: consider doing batching, buffering, etc.

			/*
			// TODO: preparing and sending the XCM messages should probably be part of the
			// parachain's runtime config, not here.

			let new_validator_set_call = RelayRuntimePallets::StakingClient(validator_set, session_index);
			let call_weight: Weight = Default::default();

			let message = Xcm(
				Vec![Instruction::UnpaidExecution {
					weight_limit: WeightLimit::Unlimited,
					check_origin: None,
				},
					origin_kind: OriginKind::Native,
					require_weight_at_most: call_weight,
					call: new_validator_set_call,
				]
			);

			match PolkadotXcm::send_xcm(Here, Location::parent(), message.clone()) {
				Ok(_) => (),
				Err(_) => (),
			};
			*/

			Ok(())
		}
	}
}
