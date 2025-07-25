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

#[cfg(test)]
pub mod mock;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
pub mod weights;

pub use weights::WeightInfo;

extern crate alloc;
use alloc::{collections::BTreeMap, vec::Vec};
use frame_support::{pallet_prelude::*, traits::RewardsReporter};
use pallet_staking_async_rc_client::{self as rc_client};
use sp_staking::{
	offence::{OffenceDetails, OffenceSeverity},
	SessionIndex,
};

/// The balance type seen from this pallet's PoV.
pub type BalanceOf<T> = <T as Config>::CurrencyBalance;

/// Type alias for offence details
pub type OffenceDetailsOf<T> = OffenceDetails<
	<T as frame_system::Config>::AccountId,
	(
		<T as frame_system::Config>::AccountId,
		sp_staking::Exposure<<T as frame_system::Config>::AccountId, BalanceOf<T>>,
	),
>;

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

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct BufferedOffence<AccountId> {
	// rc_client::Offence takes multiple reporters, but in practice there is only one. In this
	// pallet, we assume this is the case and store only the first reporter or none if empty.
	pub reporter: Option<AccountId>,
	pub slash_fraction: sp_runtime::Perbill,
}

/// A map of buffered offences, keyed by session index and then by offender account id.
pub type BufferedOffencesMap<T> = BTreeMap<
	SessionIndex,
	BTreeMap<
		<T as frame_system::Config>::AccountId,
		BufferedOffence<<T as frame_system::Config>::AccountId>,
	>,
>;

#[frame_support::pallet]
pub mod pallet {
	use crate::*;
	use alloc::vec;
	use frame_support::traits::{Hooks, UnixTime};
	use frame_system::pallet_prelude::*;
	use pallet_session::{historical, SessionManager};
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

		/// Maximum number of offences to batch in a single message to AssetHub.
		///
		/// Used during `Active` mode to limit batch size when processing buffered offences
		/// in `on_initialize`. During `Buffered` mode, offences are accumulated without batching.
		/// When transitioning from `Buffered` to `Active` mode (via `on_migration_end`),
		/// buffered offences remain stored and are processed gradually by `on_initialize`
		/// using this batch size limit to prevent block overload.
		///
		/// **Performance characteristics**
		/// - Base cost: ~30.9ms (XCM infrastructure overhead)
		/// - Per-offence cost: ~0.073ms (linear scaling)
		/// - At batch size 50: ~34.6ms total (~1.7% of 2-second compute allowance)
		type MaxOffenceBatchSize: Get<u32>;

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

		/// Information on runtime weights.
		type WeightInfo: WeightInfo;
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

	/// Offences collected while in [`OperatingMode::Buffered`] mode.
	///
	/// These are temporarily stored and sent once the pallet switches to [`OperatingMode::Active`].
	/// For each offender, only the highest `slash_fraction` is kept.
	///
	/// Internally stores as a nested BTreeMap:
	/// `session_index -> (offender -> (reporter, slash_fraction))`.
	///
	/// Note: While the [`rc_client::Offence`] type includes a list of reporters, in practice there
	/// is only one. In this pallet, we assume this is the case and store only the first reporter.
	#[pallet::storage]
	#[pallet::unbounded]
	pub type BufferedOffences<T: Config> = StorageValue<_, BufferedOffencesMap<T>, ValueQuery>;

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
			log!(debug, "Received new validator set report {}", report);
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

		/// manually do what this pallet was meant to do at the end of the migration.
		#[pallet::call_index(2)]
		#[pallet::weight(T::DbWeight::get().writes(1))]
		pub fn force_on_migration_end(origin: OriginFor<T>) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			Self::on_migration_end();
			Ok(())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			let mut weight = Weight::zero();

			let mode = Mode::<T>::get();
			weight = weight.saturating_add(T::DbWeight::get().reads(1));
			if mode != OperatingMode::Active {
				return weight;
			}

			// Check if we have any buffered offences to send
			let buffered_offences = BufferedOffences::<T>::get();
			weight = weight.saturating_add(T::DbWeight::get().reads(1));
			if buffered_offences.is_empty() {
				return weight;
			}

			let processing_weight = Self::process_buffered_offences();
			weight = weight.saturating_add(processing_weight);

			weight
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

		fn new_session_genesis(
			new_index: SessionIndex,
		) -> Option<Vec<(T::AccountId, sp_staking::Exposure<T::AccountId, BalanceOf<T>>)>> {
			if Mode::<T>::get() == OperatingMode::Passive {
				T::Fallback::new_session_genesis(new_index).map(|validators| {
					validators.into_iter().map(|v| (v, sp_staking::Exposure::default())).collect()
				})
			} else {
				None
			}
		}

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

		fn new_session_genesis(new_index: SessionIndex) -> Option<Vec<T::AccountId>> {
			if Mode::<T>::get() == OperatingMode::Passive {
				T::Fallback::new_session_genesis(new_index)
			} else {
				None
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
			match Mode::<T>::get() {
				OperatingMode::Passive => {
					// delegate to the fallback implementation.
					T::Fallback::on_offence(offenders, slash_fraction, slash_session)
				},
				OperatingMode::Buffered =>
					Self::on_offence_buffered(offenders, slash_fraction, slash_session),
				OperatingMode::Active =>
					Self::on_offence_active(offenders, slash_fraction, slash_session),
			}
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

			// Buffered offences will be processed gradually by on_initialize
			// using MaxOffenceBatchSize to prevent block overload.
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

		/// Process buffered offences and send them to AssetHub in batches.
		pub(crate) fn process_buffered_offences() -> Weight {
			let max_batch_size = T::MaxOffenceBatchSize::get() as usize;

			// Process and remove offences one session at a time
			let offences_sent = BufferedOffences::<T>::mutate(|buffered| {
				let first_session_key = buffered.keys().next().copied()?;

				let session_map = buffered.get_mut(&first_session_key)?;

				// Take up to max_batch_size offences from this session
				let keys_to_drain: Vec<_> =
					session_map.keys().take(max_batch_size).cloned().collect();

				let offences_to_send: Vec<_> = keys_to_drain
					.into_iter()
					.filter_map(|key| {
						session_map.remove(&key).map(|offence| rc_client::Offence {
							offender: key,
							reporters: offence.reporter.into_iter().collect(),
							slash_fraction: offence.slash_fraction,
						})
					})
					.collect();

				if !offences_to_send.is_empty() {
					// Remove the entire session if it's now empty
					if session_map.is_empty() {
						buffered.remove(&first_session_key);
						log!(debug, "Cleared all offences for session {}", first_session_key);
					}

					Some((first_session_key, offences_to_send))
				} else {
					None
				}
			});

			if let Some((slash_session, offences_to_send)) = offences_sent {
				log!(
					info,
					"Sending {} buffered offences for session {} to AssetHub",
					offences_to_send.len(),
					slash_session
				);

				let batch_size = offences_to_send.len();
				T::SendToAssetHub::relay_new_offence(slash_session, offences_to_send);

				T::WeightInfo::process_buffered_offences(batch_size as u32)
			} else {
				Weight::zero()
			}
		}

		/// Check if an offence is from the active validator set.
		fn is_ongoing_offence(slash_session: SessionIndex) -> bool {
			ValidatorSetAppliedAt::<T>::get()
				.map(|start_session| slash_session >= start_session)
				.unwrap_or(false)
		}

		/// Handle offences in Buffered mode.
		fn on_offence_buffered(
			offenders: &[OffenceDetailsOf<T>],
			slash_fraction: &[Perbill],
			slash_session: SessionIndex,
		) -> Weight {
			let ongoing_offence = Self::is_ongoing_offence(slash_session);

			let _: Vec<_> = offenders
				.iter()
				.cloned()
				.zip(slash_fraction)
				.map(|(offence, fraction)| {
					if ongoing_offence {
						// report the offence to the session pallet.
						T::SessionInterface::report_offence(
							offence.offender.0.clone(),
							OffenceSeverity(*fraction),
						);
					}

					let (offender, _full_identification) = offence.offender;
					let reporters = offence.reporters;

					// In `Buffered` mode, we buffer the offences for later processing.
					// We only keep the highest slash fraction for each offender per session.
					BufferedOffences::<T>::mutate(|buffered| {
						let session_offences = buffered.entry(slash_session).or_default();
						let entry = session_offences.entry(offender);

						entry
							.and_modify(|existing| {
								if existing.slash_fraction < *fraction {
									*existing = BufferedOffence {
										reporter: reporters.first().cloned(),
										slash_fraction: *fraction,
									};
								}
							})
							.or_insert(BufferedOffence {
								reporter: reporters.first().cloned(),
								slash_fraction: *fraction,
							});
					});

					// Return unit for the map operation
				})
				.collect();

			Weight::zero()
		}

		/// Handle offences in Active mode.
		fn on_offence_active(
			offenders: &[OffenceDetailsOf<T>],
			slash_fraction: &[Perbill],
			slash_session: SessionIndex,
		) -> Weight {
			let ongoing_offence = Self::is_ongoing_offence(slash_session);

			let offenders_and_slashes_message: Vec<_> = offenders
				.iter()
				.cloned()
				.zip(slash_fraction)
				.map(|(offence, fraction)| {
					if ongoing_offence {
						// report the offence to the session pallet.
						T::SessionInterface::report_offence(
							offence.offender.0.clone(),
							OffenceSeverity(*fraction),
						);
					}

					let (offender, _full_identification) = offence.offender;
					let reporters = offence.reporters;

					// prepare an `Offence` instance for the XCM message. Note that we drop
					// the identification.
					rc_client::Offence { offender, reporters, slash_fraction: *fraction }
				})
				.collect();

			// Send offence report to Asset Hub
			if !offenders_and_slashes_message.is_empty() {
				log!(info, "sending offence report to AH");
				T::SendToAssetHub::relay_new_offence(slash_session, offenders_and_slashes_message);
			}

			Weight::zero()
		}
	}
}
