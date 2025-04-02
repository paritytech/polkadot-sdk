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
//! The counter-part for this pallet is `pallet-staking-async-rc-client` on AssetHub.
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

/// Re-export the `FullIdentification` type from pallet-staking that should be used as in a rc
/// runtime.
pub use pallet_staking_async::NullIdentity;

extern crate alloc;
use alloc::vec::Vec;
use frame_support::pallet_prelude::*;
use pallet_staking_async_rc_client::{self as rc_client};
use sp_staking::{
	offence::{OffenceDetails, OffenceSeverity},
	SessionIndex,
};

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

/// Interface to talk to the local session pallet.
pub trait SessionInterface {
	/// The validator id type of the session pallet
	type ValidatorId: Clone;

	/// prune up to the given session index.
	fn prune_up_to(index: SessionIndex);

	/// Report an offence.
	fn report_offence(offender: Self::ValidatorId, severity: OffenceSeverity);
}

impl<T: Config + pallet_session::Config + pallet_session::historical::Config> SessionInterface
	for T
{
	type ValidatorId = <T as pallet_session::Config>::ValidatorId;

	fn prune_up_to(index: SessionIndex) {
		pallet_session::historical::Pallet::<T>::prune_up_to(index)
	}
	fn report_offence(offender: Self::ValidatorId, severity: OffenceSeverity) {
		pallet_session::Pallet::<T>::report_offence(offender, severity)
	}
}

/// Means to force this pallet to be partially blocked. This is useful for governance intervention.
#[derive(
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	PartialEq,
	Eq,
	Default,
	Clone,
	Copy,
)]
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
	use sp_runtime::{Perbill, Saturating};
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

		/// The origin that can control this pallet's operations.
		type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Our communication interface to AssetHub.
		type SendToAssetHub: SendToAssetHub<AccountId = Self::AccountId>;

		/// A safety measure that asserts an incoming validator set must be at least this large.
		type MinimumValidatorSetSize: Get<u32>;

		/// A type that gives us a reliable unix timestamp.
		type UnixTime: UnixTime;

		/// Number of points to award a validator per block authored.
		type PointsPerBlock: Get<u32>;

		/// Interface to talk to the local Session pallet.
		type SessionInterface: SessionInterface<ValidatorId = Self::AccountId>;
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
	/// The inner u32 is the id of the said activated validator set. While not relevant here, good
	/// to know this is the planning era index of staking-async on AH.
	///
	/// Once cleared, we know a validator set has been activated, and therefore we can send a
	/// timestamp to AH.
	#[pallet::storage]
	pub type NextSessionChangesValidators<T: Config> = StorageValue<_, u32, OptionQuery>;

	#[pallet::error]
	pub enum Error<T> {
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
			prune_up_to: Option<SessionIndex>,
			leftover: bool,
		},
		/// We could not merge, and therefore dropped a buffered message.
		///
		/// Note that this event is more resembling an error, but we use an event because in this
		/// pallet we need to mutate storage upon some failures.
		CouldNotMergeAndDropped,
		/// The validator set received is way too small, as per
		/// [`Config::MinimumValidatorSetSize`].
		SetTooSmallAndDropped,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(
			// Reads:
			// - IsBlocked
			// - IncompleteValidatorSetReport
			// Writes:
			// - IncompleteValidatorSetReport or ValidatorSet
			// ignoring `T::SessionInterface::prune_up_to`
			T::DbWeight::get().reads_writes(2, 1)
		)]
		pub fn validator_set(
			origin: OriginFor<T>,
			report: rc_client::ValidatorSetReport<T::AccountId>,
		) -> DispatchResult {
			// Ensure the origin is one of Root or whatever is representing AssetHub.
			log!(info, "Received new validator set report {:?}", report);
			T::AssetHubOrigin::ensure_origin_or_root(origin)?;
			ensure!(IsBlocked::<T>::get().allows_incoming(), Error::<T>::Blocked);

			let maybe_merged_report = match IncompleteValidatorSetReport::<T>::take() {
				Some(old) => old.merge(report.clone()),
				None => Ok(report),
			};

			if let Err(_) = maybe_merged_report {
				Self::deposit_event(Event::CouldNotMergeAndDropped);
				debug_assert!(
					IncompleteValidatorSetReport::<T>::get().is_none(),
					"we have ::take() it above, we don't want to keep the old data"
				);
				return Ok(());
			}

			let report = maybe_merged_report.expect("checked above; qed");

			if report.leftover {
				// buffer it, and nothing further to do.
				Self::deposit_event(Event::ValidatorSetReceived {
					id: report.id,
					new_validator_set_count: report.new_validator_set.len() as u32,
					prune_up_to: report.prune_up_to,
					leftover: report.leftover,
				});
				IncompleteValidatorSetReport::<T>::put(report);
			} else {
				// message is complete, process it.
				let rc_client::ValidatorSetReport {
					id,
					leftover,
					mut new_validator_set,
					prune_up_to,
				} = report;

				// ensure the validator set, deduplicated, is not too big.
				new_validator_set.sort();
				new_validator_set.dedup();

				if (new_validator_set.len() as u32) < T::MinimumValidatorSetSize::get() {
					Self::deposit_event(Event::SetTooSmallAndDropped);
					debug_assert!(
						IncompleteValidatorSetReport::<T>::get().is_none(),
						"we have ::take() it above, we don't want to keep the old data"
					);
					return Ok(());
				}

				Self::deposit_event(Event::ValidatorSetReceived {
					id,
					new_validator_set_count: new_validator_set.len() as u32,
					prune_up_to,
					leftover,
				});

				// Save the validator set.
				ValidatorSet::<T>::put((id, new_validator_set));
				if let Some(index) = prune_up_to {
					T::SessionInterface::prune_up_to(index);
				}
			}

			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(T::DbWeight::get().writes(1))]
		pub fn set_block(origin: OriginFor<T>, block: Blocked) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			IsBlocked::<T>::put(block);
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
			let maybe_validator_set = ValidatorSet::<T>::take().map(|(id, val_set)| {
				// store the id to be sent back in the next session back to AH
				NextSessionChangesValidators::<T>::put(id);
				val_set
			});

			maybe_validator_set
		}

		fn start_session(_: u32) {}

		fn end_session(session_index: u32) {
			use sp_runtime::SaturatedConversion;

			let validator_points = ValidatorPoints::<T>::iter().drain().collect::<Vec<_>>();
			let activation_timestamp = NextSessionChangesValidators::<T>::take()
				.map(|id| (T::UnixTime::now().as_millis().saturated_into::<u64>(), id));

			let session_report = pallet_staking_async_rc_client::SessionReport {
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

	impl<T: Config> OnOffenceHandler<T::AccountId, (T::AccountId, ()), Weight> for Pallet<T> {
		fn on_offence(
			offenders: &[OffenceDetails<T::AccountId, (T::AccountId, ())>],
			slash_fraction: &[Perbill],
			slash_session: SessionIndex,
		) -> Weight {
			let mut offenders_and_slashes = Vec::new();

			// notify pallet-session about the offences
			for (offence, fraction) in offenders.iter().cloned().zip(slash_fraction) {
				T::SessionInterface::report_offence(
					offence.offender.0.clone(),
					OffenceSeverity(*fraction),
				);

				// prepare an `Offence` instance for the XCM message
				offenders_and_slashes.push(rc_client::Offence {
					offender: offence.offender.0,
					reporters: offence.reporters,
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

	impl<T: Config> frame_support::traits::RewardsReporter<T::AccountId> for Pallet<T> {
		fn reward_by_ids(rewards: impl IntoIterator<Item = (T::AccountId, u32)>) {
			for (validator_id, points) in rewards {
				ValidatorPoints::<T>::mutate(validator_id, |balance| {
					balance.saturating_accrue(points);
				});
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
}
