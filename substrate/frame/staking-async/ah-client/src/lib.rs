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
//! * [`Call::validator_set`]: A new validator set for a planning session index.
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
//! * Implement [`pallet_session::SessionManager`] (and historical variant thereof) to _give_
//!   information to the session pallet.
//! * Implements [`SessionInterface`] to _receive_ information from the session pallet
//! * Implement [`sp_staking::offence::OnOffenceHandler`].
//! * Implement reward related APIs ([`frame_support::traits::RewardsReporter`]).
//!
//! ## Future Plans
//!
//! * Governance functions to force set validators.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

extern crate alloc;
use alloc::vec::Vec;
use frame_support::{pallet_prelude::*, traits::RewardsReporter};
use pallet_staking_async_rc_client::{self as rc_client};
use sp_staking::{
	offence::{OffenceDetails, OffenceSeverity},
	SessionIndex,
};

/// The balance type seen from this pallet's PoV.
pub type BalanceOf<T> = <T as Config>::CurrencyBalance;

const LOG_TARGET: &str = "runtime::staking-async::ah-client";

// syntactic sugar for logging.
#[macro_export]
macro_rules! log {
	($level:tt, $patter:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: $crate::LOG_TARGET,
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

/// A no-op implementation of [`SendToAssetHub`].
#[cfg(feature = "std")]
impl SendToAssetHub for () {
	type AccountId = u64;

	fn relay_session_report(_session_report: rc_client::SessionReport<Self::AccountId>) {
		panic!("relay_session_report not implemented");
	}

	fn relay_new_offence(
		_session_index: SessionIndex,
		_offences: Vec<rc_client::Offence<Self::AccountId>>,
	) {
		panic!("relay_new_offence not implemented");
	}
}

/// Interface to talk to the local session pallet.
pub trait SessionInterface {
	/// The validator id type of the session pallet
	type ValidatorId: Clone;

	fn validators() -> Vec<Self::ValidatorId>;

	/// prune up to the given session index.
	fn prune_up_to(index: SessionIndex);

	/// Report an offence.
	///
	/// This is used to disable validators directly on the RC, until the next validator set.
	fn report_offence(offender: Self::ValidatorId, severity: OffenceSeverity);
}

impl<T: Config + pallet_session::Config + pallet_session::historical::Config> SessionInterface
	for T
{
	type ValidatorId = <T as pallet_session::Config>::ValidatorId;

	fn validators() -> Vec<Self::ValidatorId> {
		pallet_session::Pallet::<T>::validators()
	}

	fn prune_up_to(index: SessionIndex) {
		pallet_session::historical::Pallet::<T>::prune_up_to(index)
	}
	fn report_offence(offender: Self::ValidatorId, severity: OffenceSeverity) {
		pallet_session::Pallet::<T>::report_offence(offender, severity)
	}
}

/// Represents the operating mode of the pallet.
#[derive(
	Default,
	DecodeWithMemTracking,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	Clone,
	PartialEq,
	Eq,
	RuntimeDebug,
	serde::Serialize,
	serde::Deserialize,
)]
pub enum OperatingMode {
	/// Fully delegated mode.
	///
	/// In this mode, the pallet performs no core logic and forwards all relevant operations
	/// to the fallback implementation defined in the pallet's `Config::Fallback`.
	///
	/// This mode is useful when staking is in synchronous mode and waiting for the signal to
	/// transition to asynchronous mode.
	#[default]
	Passive,

	/// Buffered mode for deferred execution.
	///
	/// In this mode, offences are accepted and buffered for later transmission to AssetHub.
	/// However, session change reports are dropped.
	///
	/// This mode is useful when the counterpart pallet `pallet-staking-async-rc-client` on
	/// AssetHub is not yet ready to process incoming messages.
	Buffered,

	/// Fully active mode.
	///
	/// The pallet performs all core logic directly and handles messages immediately.
	///
	/// This mode is useful when staking is ready to execute in asynchronous mode and the
	/// counterpart pallet `pallet-staking-async-rc-client` is ready to accept messages.
	Active,
}

impl OperatingMode {
	fn can_accept_validator_set(&self) -> bool {
		matches!(self, OperatingMode::Active)
	}
}

/// See `pallet_staking::DefaultExposureOf`. This type is the same, except it is duplicated here so
/// that an rc-runtime can use it after `pallet-staking` is fully removed as a dependency.
pub struct DefaultExposureOf<T>(core::marker::PhantomData<T>);

impl<T: Config>
	sp_runtime::traits::Convert<
		T::AccountId,
		Option<sp_staking::Exposure<T::AccountId, BalanceOf<T>>>,
	> for DefaultExposureOf<T>
{
	fn convert(
		validator: T::AccountId,
	) -> Option<sp_staking::Exposure<T::AccountId, BalanceOf<T>>> {
		T::SessionInterface::validators()
			.contains(&validator)
			.then_some(Default::default())
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
		/// The balance type of the runtime's currency interface.
		type CurrencyBalance: sp_runtime::traits::AtLeast32BitUnsigned
			+ codec::FullCodec
			+ DecodeWithMemTracking
			+ codec::HasCompact<Type: DecodeWithMemTracking>
			+ Copy
			+ MaybeSerializeDeserialize
			+ core::fmt::Debug
			+ Default
			+ From<u64>
			+ TypeInfo
			+ Send
			+ Sync
			+ MaxEncodedLen;

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

		/// A fallback implementation to delegate logic to when the pallet is in
		/// [`OperatingMode::Passive`].
		///
		/// This type must implement the `historical::SessionManager` and `OnOffenceHandler`
		/// interface and is expected to behave as a stand-in for this pallet’s core logic when
		/// delegation is active.
		type Fallback: pallet_session::SessionManager<Self::AccountId>
			+ OnOffenceHandler<
				Self::AccountId,
				(Self::AccountId, sp_staking::Exposure<Self::AccountId, BalanceOf<Self>>),
				Weight,
			> + frame_support::traits::RewardsReporter<Self::AccountId>
			+ pallet_authorship::EventHandler<Self::AccountId, BlockNumberFor<Self>>;
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

	/// Indicates the current operating mode of the pallet.
	///
	/// This value determines how the pallet behaves in response to incoming and outgoing messages,
	/// particularly whether it should execute logic directly, defer it, or delegate it entirely.
	#[pallet::storage]
	pub type Mode<T: Config> = StorageValue<_, OperatingMode, ValueQuery>;

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

	/// The session index at which the latest elected validator set was applied.
	///
	/// This is used to determine if an offence, given a session index, is in the current active era
	/// or not.
	#[pallet::storage]
	pub type ValidatorSetAppliedAt<T: Config> = StorageValue<_, SessionIndex, OptionQuery>;

	/// Stores offences that have been received while the pallet is in [`OperatingMode::Buffered`]
	/// mode.
	///
	/// These offences are collected and buffered for later processing when the pallet transitions
	/// to [`OperatingMode::Active`]. This allows the system to defer slashing or reporting logic
	/// until communication with the counterpart pallet on AssetHub is fully established.
	///
	/// This storage is only used in `Buffered` mode; in `Active` mode, offences are immediately
	/// sent, and in `Passive` mode, they are delegated to the [`Config::Fallback`] implementation.
	#[pallet::storage]
	#[pallet::unbounded]
	pub type BufferedOffences<T: Config> =
		StorageValue<_, Vec<(SessionIndex, Vec<rc_client::Offence<T::AccountId>>)>, ValueQuery>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound, frame_support::DebugNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// The initial operating mode of the pallet.
		pub operating_mode: OperatingMode,
		pub _marker: core::marker::PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			// Set the initial operating mode of the pallet.
			Mode::<T>::put(self.operating_mode.clone());
		}
	}

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
		/// Something occurred that should never happen under normal operation. Logged as an event
		/// for fail-safe observability.
		Unexpected(UnexpectedKind),
	}

	/// Represents unexpected or invariant-breaking conditions encountered during execution.
	///
	/// These variants are emitted as [`Event::Unexpected`] and indicate a defensive check has
	/// failed. While these should never occur under normal operation, they are useful for
	/// diagnosing issues in production or test environments.
	#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, TypeInfo, RuntimeDebug)]
	pub enum UnexpectedKind {
		/// A validator set was received while the pallet is in [`OperatingMode::Passive`].
		ReceivedValidatorSetWhilePassive,

		/// An unexpected transition was applied between operating modes.
		///
		/// Expected transitions are linear and forward-only: `Passive` → `Buffered` → `Active`.
		UnexpectedModeTransition,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(
			// Reads:
			// - OperatingMode
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

			// Check the operating mode.
			let mode = Mode::<T>::get();
			ensure!(mode.can_accept_validator_set(), Error::<T>::Blocked);

			let maybe_merged_report = match IncompleteValidatorSetReport::<T>::take() {
				Some(old) => old.merge(report.clone()),
				None => Ok(report),
			};

			if maybe_merged_report.is_err() {
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

		/// Allows governance to force set the operating mode of the pallet.
		#[pallet::call_index(1)]
		#[pallet::weight(T::DbWeight::get().writes(1))]
		pub fn set_mode(origin: OriginFor<T>, mode: OperatingMode) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			Self::do_set_mode(mode);
			Ok(())
		}
	}

	impl<T: Config>
		historical::SessionManager<T::AccountId, sp_staking::Exposure<T::AccountId, BalanceOf<T>>>
		for Pallet<T>
	{
		fn new_session(
			new_index: sp_staking::SessionIndex,
		) -> Option<
			Vec<(
				<T as frame_system::Config>::AccountId,
				sp_staking::Exposure<T::AccountId, BalanceOf<T>>,
			)>,
		> {
			<Self as pallet_session::SessionManager<_>>::new_session(new_index)
				.map(|v| v.into_iter().map(|v| (v, sp_staking::Exposure::default())).collect())
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
		fn new_session(session_index: u32) -> Option<Vec<T::AccountId>> {
			match Mode::<T>::get() {
				OperatingMode::Passive => T::Fallback::new_session(session_index),
				// In `Buffered` mode, we drop the session report and do nothing.
				OperatingMode::Buffered => None,
				OperatingMode::Active => Self::do_new_session(),
			}
		}

		fn start_session(session_index: u32) {
			if Mode::<T>::get() == OperatingMode::Passive {
				T::Fallback::start_session(session_index)
			}
		}

		fn end_session(session_index: u32) {
			match Mode::<T>::get() {
				OperatingMode::Passive => T::Fallback::end_session(session_index),
				// In `Buffered` mode, we drop the session report and do nothing.
				OperatingMode::Buffered => (),
				OperatingMode::Active => Self::do_end_session(session_index),
			}
		}
	}

	impl<T: Config>
		OnOffenceHandler<
			T::AccountId,
			(T::AccountId, sp_staking::Exposure<T::AccountId, BalanceOf<T>>),
			Weight,
		> for Pallet<T>
	{
		fn on_offence(
			offenders: &[OffenceDetails<
				T::AccountId,
				(T::AccountId, sp_staking::Exposure<T::AccountId, BalanceOf<T>>),
			>],
			slash_fraction: &[Perbill],
			slash_session: SessionIndex,
		) -> Weight {
			let mode = Mode::<T>::get();
			if mode == OperatingMode::Passive {
				// delegate to the fallback implementation.
				return T::Fallback::on_offence(offenders, slash_fraction, slash_session);
			}

			// check if offence is from the active validator set.
			let ongoing_offence = ValidatorSetAppliedAt::<T>::get()
				.map(|start_session| slash_session >= start_session)
				.unwrap_or(false);

			let mut offenders_and_slashes = Vec::new();

			// notify pallet-session about the offences
			for (offence, fraction) in offenders.iter().cloned().zip(slash_fraction) {
				if ongoing_offence {
					// report the offence to the session pallet.
					T::SessionInterface::report_offence(
						offence.offender.0.clone(),
						OffenceSeverity(*fraction),
					);
				}

				// prepare an `Offence` instance for the XCM message. Note that we drop the
				// identification.
				let (offender, _full_identification) = offence.offender;
				let reporters = offence.reporters;
				offenders_and_slashes.push(rc_client::Offence {
					offender,
					reporters,
					slash_fraction: *fraction,
				});
			}

			match mode {
				OperatingMode::Buffered => {
					BufferedOffences::<T>::mutate(|buffered| {
						buffered.push((slash_session, offenders_and_slashes.clone()));
					});
					log!(info, "Buffered offences: {:?}", offenders_and_slashes);
				},
				OperatingMode::Active => {
					log!(info, "sending offence report to AH");
					T::SendToAssetHub::relay_new_offence(slash_session, offenders_and_slashes);
				},
				_ => (),
			}

			Weight::zero()
		}
	}

	impl<T: Config> RewardsReporter<T::AccountId> for Pallet<T> {
		fn reward_by_ids(rewards: impl IntoIterator<Item = (T::AccountId, u32)>) {
			match Mode::<T>::get() {
				OperatingMode::Passive => T::Fallback::reward_by_ids(rewards),
				OperatingMode::Buffered | OperatingMode::Active => Self::do_reward_by_ids(rewards),
			}
		}
	}

	impl<T: Config> pallet_authorship::EventHandler<T::AccountId, BlockNumberFor<T>> for Pallet<T> {
		fn note_author(author: T::AccountId) {
			match Mode::<T>::get() {
				OperatingMode::Passive => T::Fallback::note_author(author),
				OperatingMode::Buffered | OperatingMode::Active => Self::do_note_author(author),
			}
		}
	}

	impl<T: Config> Pallet<T> {
		/// Hook to be called when the AssetHub migration begins.
		///
		/// This transitions the pallet into [`OperatingMode::Buffered`], meaning it will act as the
		/// primary staking module on the relay chain but will buffer outgoing messages instead of
		/// sending them to AssetHub.
		///
		/// While in this mode, the pallet stops delegating to the fallback implementation and
		/// temporarily accumulates events for later processing.
		pub fn on_migration_start() {
			debug_assert!(
				Mode::<T>::get() == OperatingMode::Passive,
				"we should only be called when in passive mode"
			);
			Self::do_set_mode(OperatingMode::Buffered);
		}

		/// Hook to be called when the AssetHub migration is complete.
		///
		/// This transitions the pallet into [`OperatingMode::Active`], meaning the counterpart
		/// pallet on AssetHub is ready to accept incoming messages, and this pallet can resume
		/// sending them.
		///
		/// In this mode, the pallet becomes fully active and processes all staking-related events
		/// directly.
		pub fn on_migration_end() {
			debug_assert!(
				Mode::<T>::get() == OperatingMode::Buffered,
				"we should only be called when in buffered mode"
			);
			Self::do_set_mode(OperatingMode::Active);

			// send all buffered offences to AssetHub.
			BufferedOffences::<T>::take().into_iter().for_each(|(slash_session, offences)| {
				T::SendToAssetHub::relay_new_offence(slash_session, offences)
			});
		}

		fn do_set_mode(new_mode: OperatingMode) {
			let old_mode = Mode::<T>::get();
			let unexpected = match new_mode {
				// `Passive` is the initial state, and not expected to be set by the user.
				OperatingMode::Passive => true,
				OperatingMode::Buffered => old_mode != OperatingMode::Passive,
				OperatingMode::Active => old_mode != OperatingMode::Buffered,
			};

			// this is a defensive check, and should never happen under normal operation.
			if unexpected {
				log!(warn, "Unexpected mode transition from {:?} to {:?}", old_mode, new_mode);
				Self::deposit_event(Event::Unexpected(UnexpectedKind::UnexpectedModeTransition));
			}

			// apply new mode anyway.
			Mode::<T>::put(new_mode);
		}

		fn do_new_session() -> Option<Vec<T::AccountId>> {
			ValidatorSet::<T>::take().map(|(id, val_set)| {
				// store the id to be sent back in the next session back to AH
				NextSessionChangesValidators::<T>::put(id);
				val_set
			})
		}

		fn do_end_session(session_index: u32) {
			use sp_runtime::SaturatedConversion;

			let validator_points = ValidatorPoints::<T>::iter().drain().collect::<Vec<_>>();
			let activation_timestamp = NextSessionChangesValidators::<T>::take().map(|id| {
				// keep track of starting session index at which the validator set was applied.
				ValidatorSetAppliedAt::<T>::put(session_index + 1);
				// set the timestamp and the identifier of the validator set.
				(T::UnixTime::now().as_millis().saturated_into::<u64>(), id)
			});

			let session_report = pallet_staking_async_rc_client::SessionReport {
				end_index: session_index,
				validator_points,
				activation_timestamp,
				leftover: false,
			};

			T::SendToAssetHub::relay_session_report(session_report);
		}

		fn do_reward_by_ids(rewards: impl IntoIterator<Item = (T::AccountId, u32)>) {
			for (validator_id, points) in rewards {
				ValidatorPoints::<T>::mutate(validator_id, |balance| {
					balance.saturating_accrue(points);
				});
			}
		}

		fn do_note_author(author: T::AccountId) {
			ValidatorPoints::<T>::mutate(author, |points| {
				points.saturating_accrue(T::PointsPerBlock::get());
			});
		}
	}
}
