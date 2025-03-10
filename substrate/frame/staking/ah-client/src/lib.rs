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
//! * Implement reward related APIs.
//! * If further communication is needed to the session pallet, either a custom trait (`trait
//!   SessionInterface`) or tightly coupling the session-pallet should work.
//!
//! TODO:
//! * Governance functions to force set validators.

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
			concat!("[{:?}] ⬇️ ", $patter), <frame_system::Pallet<T>>::block_number() $(, $values)*
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

/// Means to force this pallet to be partially blocked. This is useful for governance intervention.
#[derive(Debug, Encode, Decode, MaxEncodedLen, TypeInfo, PartialEq, Eq, Default)]
pub enum Blocked {
	/// Normal working operations.
	#[default]
	Not,
	/// Block all incoming messages.
	Incoming,
	/// Block all outgoing messages.
	Outgoing,
	/// Block both incoming and outgoing messages.
	Both,
}

impl Blocked {
	pub(crate) fn allows_incoming(&self) -> bool {
		matches!(self, Self::Not | Self::Outgoing)
	}

	pub(crate) fn allows_outgoing(&self) -> bool {
		matches!(self, Self::Not | Self::Incoming)
	}
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
		/// Overarching runtime event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

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

	/// Stores whether this pallet is blocked in any way or not.
	#[pallet::storage]
	pub type IsBlocked<T: Config> = StorageValue<_, Blocked, ValueQuery>;

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
		/// Could not process incoming message because incoming messages are blocked.
		Blocked,
	}

	#[pallet::event]
	#[pallet::generate_deposit(fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new validator set has been received.
		ValidatorSetReceived {
			id: u32,
			new_validator_set_count: u32,
			prune_up_to: u32,
			leftover: bool,
		},
		/// We could not merge, and therefore dropped a buffered message.
		ValidatorSetDropped,
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
			log!(info, "Received new validator set report {:?}", report);
			T::AssetHubOrigin::ensure_origin_or_root(origin)?;
			ensure!(IsBlocked::<T>::get().allows_incoming(), Error::<T>::Blocked);

			let maybe_new_validator_set_report = match IncompleteValidatorSetReport::<T>::take() {
				Some(old) => old.merge(report.clone()),
				None => Ok(report),
			};

			if let Err(_) = maybe_new_validator_set_report {
				Self::deposit_event(Event::ValidatorSetDropped);
				// note -- if we return error the storage ops are reverted, so we do this instead.
				return Ok(());
			}

			let new_validator_set_report =
				maybe_new_validator_set_report.expect("checked above; qed");

			if new_validator_set_report.leftover {
				// buffer it, and nothing further to do.
				IncompleteValidatorSetReport::<T>::put(new_validator_set_report);
			} else {
				let rc_client::ValidatorSetReport {
					id,
					leftover,
					mut new_validator_set,
					prune_up_to,
				} = new_validator_set_report;

				// ensure the validator set, deduplicated, is not too big.
				new_validator_set.sort();
				new_validator_set.dedup();

				ensure!(
					new_validator_set.len() as u32 >= T::MinimumValidatorSetSize::get(),
					Error::<T>::MinimumValidatorSetSize
				);

				// Save the validator set.
				Self::deposit_event(Event::ValidatorSetReceived {
					id,
					new_validator_set_count: new_validator_set.len() as u32,
					prune_up_to,
					leftover,
				});
				ValidatorSet::<T>::put((id, new_validator_set));
			}

			Ok(())
		}
	}

	impl<T: Config> historical::SessionManager<T::AccountId, ()> for Pallet<T> {
		fn new_session(
			new_index: sp_staking::SessionIndex,
		) -> Option<Vec<(<T as frame_system::Config>::AccountId, ())>> {
			<Self as pallet_session::SessionManager<_>>::new_session(new_index)
				.map(|v| v.into_iter().map(|v| (v, ())).collect())
		}

		// We don't implement `new_session_genesis` because we rely on the default implementation
		// which calls `new_session`

		fn start_session(start_index: SessionIndex) {
			<Self as pallet_session::SessionManager<_>>::start_session(start_index)
		}

		fn end_session(end_index: SessionIndex) {
			<Self as pallet_session::SessionManager<_>>::end_session(end_index)
		}
	}

	impl<T: Config> pallet_session::SessionManager<T::AccountId> for Pallet<T> {
		fn new_session(_: u32) -> Option<Vec<T::AccountId>> {
			let maybe_new_validator_set =
				ValidatorSet::<T>::take().map(|(_, validators)| validators);

			if maybe_new_validator_set.is_some() {
				NextSessionChangesValidators::<T>::put(());
			}

			maybe_new_validator_set
		}

		fn start_session(_: u32) {}

		fn end_session(session_index: u32) {
			use sp_runtime::SaturatedConversion;

			let validator_points = ValidatorPoints::<T>::iter().drain().collect::<Vec<_>>();
			let activation_timestamp = NextSessionChangesValidators::<T>::take().map(|_| {
				// TODO(ank4n): not setting the id for now, not sure if needed.
				(T::UnixTime::now().as_millis().saturated_into::<u64>(), 0)
			});

			let session_report = pallet_staking_rc_client::SessionReport {
				end_index: session_index,
				validator_points,
				activation_timestamp,
				leftover: false,
			};

			if IsBlocked::<T>::get().allows_outgoing() {
				log!(info, "Sending session report {:?}", session_report);
				T::SendToAssetHub::relay_session_report(session_report);
			} else {
				log!(warn, "Session report is blocked and not sent.");
			}
		}
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

			if IsBlocked::<T>::get().allows_outgoing() {
				log!(info, "sending offence report to AH");
				T::SendToAssetHub::relay_new_offence(slash_session, offenders_and_slashes);
			} else {
				log!(warn, "offence report is blocked and not sent")
			}

			Weight::zero()
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn handle_parachain_rewards(
			validators_points: impl IntoIterator<Item = (T::AccountId, u32)>,
		) -> Weight {
			// Accumulate this in our pending points, which is sent off in the next session.
			//
			// Note: The input is the number of ACTUAL points which should be added to
			// validator's balance!
			for (validator_id, points) in validators_points {
				ValidatorPoints::<T>::mutate(validator_id, |balance| {
					balance.saturating_accrue(points);
				});
			}

			// TODO: if we move this trait `RewardsReporter` somewhere more easy to access, we can
			// implement it directly and not need a custom type on the runtime. We can do it now
			// too, but it would pull all of the polkadot-parachain pallets.

			Weight::zero()
		}
	}
}
