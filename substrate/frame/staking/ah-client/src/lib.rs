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

//! The client for AssetHub, intended to be used in the relay chain.
//!
//! The counter-part for this pallet is `pallet-staking-rc-client` on AssetHub.
//!
//! This documentation is divided into the following sections:
//!
//! 1. Incoming messages: the messages that we receive from the relay chian.
//! 2. Outgoing messages: the messaged that we sent to the relay chain.
//! 3. Local interfaces: the interfaces that we expose to other pallets in the runtime.
//!
//! ## Incoming Messages
//!
//! All incoming messages are handled via [`Call`]. They are all gated to be dispatched only by
//! [`Config::AssetHubOrigin`]. The only one is:
//!
//! * [`Call::new_validator_set`]: A new validator set for a planning session index.
//!
//! ## Outgoing Messages
//!
//! All outgoing messages are handled by a single trait [`SendToAssetHub`]. They match the
//! incoming messages of the `ah-client` pallet.
//!
//! ## Local Interfaces:
//!
//! Living on the relay chain, this pallet must:
//!
//! * Implement `SessionManager` (and historical variant thereof).
//! * Implement `OnOffenceHandler`.
//! * Implement
//! * If further communication is needed to the session pallet, either a custom trait (`trait
//!   SessionInterface`) or tightly coupling the session-pallet should work.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

extern crate alloc;
use alloc::vec::Vec;
use frame_support::pallet_prelude::*;
use pallet_staking_rc_client::{self as rc_client};
use sp_staking::{offence::OffenceDetails, SessionIndex};

const LOG_TARGET: &str = "runtime::staking::ah-client";

// syntactic sugar for logging.
#[macro_export]
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: crate::LOG_TARGET,
			concat!("[{:?}] ⬆️ ", $patter), <frame_system::Pallet<T>>::block_number() $(, $values)*
		)
	};
}

/// The interface to communicate to asset hub.
///
/// This trait should only encapsulate our outgoing communications. Any incoming message is handled
/// with `Call`s.
///
/// In a real runtime, this is implemented via XCM calls, much like how the coretime pallet works.
/// In a test runtime, it can be wired to direct function call.
pub trait SendToAssetHub {
	/// The validator account ids.
	type AccountId;

	/// Report a session change to AssetHub.
	fn relay_session_report(session_report: rc_client::SessionReport<Self::AccountId>);

	/// Report new offences.
	fn relay_new_offence(
		session_index: SessionIndex,
		offences: Vec<rc_client::Offence<Self::AccountId>>,
	);
}

#[frame_support::pallet]
pub mod pallet {
	use crate::*;
	use alloc::vec;
	use frame_support::traits::UnixTime;
	use frame_system::pallet_prelude::*;
	use pallet_session::historical;
	use sp_runtime::{traits::Saturating, Perbill};
	use sp_staking::{
		offence::{OffenceSeverity, OnOffenceHandler},
		SessionIndex,
	};

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// An origin type that ensures an incoming message is from asset hub.
		type AssetHubOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Our communication interface to AssetHub.
		type SendToAssetHub: SendToAssetHub<AccountId = Self::AccountId>;

		/// A safety measure that asserts an incoming validator set must be at least this large.
		type MinimumValidatorSetSize: Get<u32>;

		/// A type that gives us a reliable unix timestamp.
		type UnixTime: UnixTime;

		/// Number of points to award a validator per block authored.
		type PointsPerBlock: Get<u32>;
	}

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	/// The queued validator sets for a given planning session index.
	///
	/// This is received via a call from AssetHub.
	#[pallet::storage]
	#[pallet::unbounded]
	pub type ValidatorSet<T: Config> = StorageValue<_, (u32, Vec<T::AccountId>), OptionQuery>;

	/// An incomplete validator set report.
	#[pallet::storage]
	#[pallet::unbounded]
	pub type IncompleteValidatorSetReport<T: Config> =
		StorageValue<_, rc_client::ValidatorSetReport<T::AccountId>, OptionQuery>;

	/// All of the points of the validators.
	///
	/// This is populated during a session, and is flushed and sent over via [`SendToAssetHub`]
	/// at each session end.
	#[pallet::storage]
	pub type ValidatorPoints<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, u32, ValueQuery>;

	/// A storage value that is set when a `new_session` gives a new validator set to the session
	/// pallet, and is cleared on the next call.
	///
	/// Once cleared, we know a validator set has been activated, and therefore we can send a
	/// timestamp to AH.
	#[pallet::storage]
	pub type NextSessionChangesValidators<T: Config> = StorageValue<_, (), OptionQuery>;

	#[pallet::error]
	pub enum Error<T> {
		/// The validator set received is way too small, as per
		/// [`Config::MinimumValidatorSetSize`].
		MinimumValidatorSetSize,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(0)]
		pub fn validator_set(
			origin: OriginFor<T>,
			report: rc_client::ValidatorSetReport<T::AccountId>,
		) -> DispatchResult {
			// Ensure the origin is one of Root or whatever is representing AssetHub.
			T::AssetHubOrigin::ensure_origin_or_root(origin)?;
			log!(info, "Received new validator set report {:?}", report);
			let rc_client::ValidatorSetReport { id, leftover, mut new_validator_set, prune_up_to } =
				report;
			debug_assert!(!leftover);

			// TODO: buffer in `IncompleteValidatorSetReport` if incomplete, similar to how
			// rc-client does it.

			// ensure the validator set, deduplicated, is not too big.
			new_validator_set.sort();
			new_validator_set.dedup();

			ensure!(
				new_validator_set.len() as u32 >= T::MinimumValidatorSetSize::get(),
				Error::<T>::MinimumValidatorSetSize
			);

			// Save the validator set.
			ValidatorSet::<T>::put((id, new_validator_set));

			Ok(())
		}
	}

	impl<T: Config> historical::SessionManager<T::AccountId, ()> for Pallet<T> {
		fn new_session(
			_: sp_staking::SessionIndex,
		) -> Option<Vec<(<T as frame_system::Config>::AccountId, ())>> {
			let maybe_new_validator_set = ValidatorSet::<T>::take()
				.map(|(session, validators)| validators.into_iter().map(|v| (v, ())).collect());

			// A new validator set is an indication for a new era. Clear
			if maybe_new_validator_set.is_none() {
				// TODO: historical sessions should be pruned. This used to happen after the bonding
				// period for the session but it would be nice to avoid XCM messages for prunning
				// and trigger it from RC directly.

				// <pallet_session::historical::Pallet<T>>::prune_up_to(up_to); // TODO!!!
			}

			// TODO: move this to the normal impl
			if maybe_new_validator_set.is_some() {
				NextSessionChangesValidators::<T>::put(());
			}

			return maybe_new_validator_set
		}

		fn new_session_genesis(_: SessionIndex) -> Option<Vec<(T::AccountId, ())>> {
			ValidatorSet::<T>::take()
				.map(|(session, validators)| validators.into_iter().map(|v| (v, ())).collect())
		}

		fn start_session(start_index: SessionIndex) {
			<Self as pallet_session::SessionManager<_>>::start_session(start_index)
		}

		fn end_session(end_index: SessionIndex) {
			<Self as pallet_session::SessionManager<_>>::end_session(end_index)
		}
	}

	impl<T: Config> pallet_session::SessionManager<T::AccountId> for Pallet<T> {
		fn new_session(_: u32) -> Option<Vec<T::AccountId>> {
			// TODO return if we have a queued validator set.
			None
		}

		fn end_session(session_index: u32) {
			use sp_runtime::SaturatedConversion;

			let validator_points = ValidatorPoints::<T>::iter().drain().collect::<Vec<_>>();
			let activation_timestamp = NextSessionChangesValidators::<T>::take().map(|_| {
				// TODO: not setting the id for now, not sure if needed.
				(T::UnixTime::now().as_millis().saturated_into::<u64>(), 0)
			});

			let session_report = pallet_staking_rc_client::SessionReport {
				end_index: session_index,
				validator_points,
				activation_timestamp,
				leftover: false,
			};

			log!(info, "Sending session report {:?}", session_report);
			T::SendToAssetHub::relay_session_report(session_report);
		}

		fn start_session(session_index: u32) {}
	}

	impl<T: Config> pallet_authorship::EventHandler<T::AccountId, BlockNumberFor<T>> for Pallet<T> {
		fn note_author(author: T::AccountId) {
			ValidatorPoints::<T>::mutate(author, |points| {
				points.saturating_accrue(T::PointsPerBlock::get());
			});
		}
	}

	impl<T: Config, I: sp_runtime::traits::Convert<T::AccountId, Option<()>>>
		OnOffenceHandler<T::AccountId, pallet_session::historical::IdentificationTuple<T>, Weight>
		for Pallet<T>
	where
		T: pallet_session::Config<ValidatorId = <T as frame_system::Config>::AccountId>,
		T: pallet_session::historical::Config<FullIdentification = (), FullIdentificationOf = I>,
		T::SessionManager: pallet_session::SessionManager<<T as frame_system::Config>::AccountId>,
	{
		fn on_offence(
			offenders: &[OffenceDetails<
				T::AccountId,
				pallet_session::historical::IdentificationTuple<T>,
			>],
			slash_fraction: &[Perbill],
			slash_session: SessionIndex,
		) -> Weight {
			let mut offenders_and_slashes = Vec::new();

			// notify pallet-session about the offences
			for (offence, fraction) in offenders.iter().cloned().zip(slash_fraction) {
				<pallet_session::Pallet<T>>::report_offence(
					offence.offender.0.clone(),
					OffenceSeverity(*fraction),
				);

				// prepare an `Offence` instance for the XCM message
				offenders_and_slashes.push(rc_client::Offence {
					offender: offence.offender.0.into(),
					reporters: offence.reporters.into_iter().map(|r| r.into()).collect(),
					slash_fraction: *fraction,
				});
			}

			T::SendToAssetHub::relay_new_offence(slash_session, offenders_and_slashes);
			Weight::zero()
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn handle_parachain_rewards(
			validators_points: impl IntoIterator<Item = (T::AccountId, u32)>,
		) -> Weight {
			// TODO: accumulate this in our pending points, which is sent off in the next session.

			// TODO: if we move this trait `RewardsReporter` somewhere more easy to access, we can
			// implement it directly and not need a custom type on the runtime. We can do it now
			// too, but it would pull all of the polkadot-parachain pallets.

			Weight::zero()
		}
	}
}
