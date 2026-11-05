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
//! [`Config::ValidatorRegistration`], and records the returned list in [`DrawnCollators`]. The
//! runtime may combine this pallet with `pallet-collator-selection` through
//! [`pallet_session::UnionSessionManager`], so invulnerables and candidates keep collating next to
//! the validators.
//!
//! [`MaxCollators`] optionally caps how many validators are returned. Governance can start with a
//! low cap and raise it in steps, or lower it again, to roll out validator collators gradually.
//! Under a cap the pallet draws the validators at random at the first forced rotation after a set
//! arrives or the cap changes, and returns them until the next draw. The draw shuffles the whole
//! set with a seed made of the genesis hash, the era and the relay chain epoch randomness cached in
//! [`EpochRandomness`], then keeps the first validators with registered keys up to the cap. Every
//! validator with keys has the same chance to be drawn in each era. A cap change draws again from
//! the stored set, so the drawn validators can change. Every draw takes its seed from the
//! randomness cached at the time of the draw. The seed includes the genesis hash, so each chain
//! draws independently from the same set. A drawn validator that removes its keys is dropped at
//! the next rotation and its place stays empty, and a validator that registers keys after the draw
//! waits, until the next set or cap change. Without a cap, key changes take effect at the next
//! rotation. Each set, so each era, gets a new draw, which is enacted at the second forced rotation
//! with no notice, so a validator with keys must already run a synced collator node to collate
//! when drawn.
//!
//! Until governance sets it, the cap is [`Config::DefaultMaxCollators`].
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
//! The pallet is also a `pallet_authorship::EventHandler` and counts the blocks each author
//! produces in [`AuthoredBlocks`]. At a report-time, it removes the counts of the authors outside
//! the stored set, and hands the counts of the others to [`Config::BlockReporter`] in one call. The
//! reporter delivers them to the chain that runs `pallet-staking-async` + `points-convertor`, where
//! they become era points. A report has at most one entry per validator of the stored set, and the
//! pallet's `integrity_test` checks that [`Config::MaxValidators`] entries fit the reporter's
//! [`ReportAuthoredBlocks::max_message_size`], so a full report is never too large to send and is
//! not split. If the reporter rejects it, the counts of ONLY the _currently stored validators_ are
//! kept for the next report, so [`AuthoredBlocks`] never holds more than the stored set plus the
//! authors since the last report. Without a stored set every count is dropped.
//!
//! A report runs in every block whose number is a multiple of [`ReportPeriod`], and once per
//! stored set shortly before the expected end of its era. The pallet records the arrival time of
//! each set in [`ExternalSetReceivedAt`], from [`Config::UnixTime`], and expects the era to end
//! [`ExpectedEraDuration`] after it. The early report runs in the first block at or after
//! [`ReportMargin`] before that end, so that in a normal era the counts reach the chain that runs
//! `pallet-staking-async` before the era rotates. The margin used is at most
//! [`Config::MaxReportMargin`] and at most half of [`ExpectedEraDuration`]. The early report
//! removes [`ExternalSetReceivedAt`] and is not retried: if the reporter rejects it, its counts go
//! out with the next [`ReportPeriod`] report. The periodic report also covers eras that end earlier
//! or later than expected, a set that never arrives, and the time before the first set arrives. If
//! both reports fall in the same block, the pallet reports once.
//!
//! A runtime must configure `pallet-collator-selection`'s `KickThreshold` to exceed a full Aura
//! round of the merged list, [`Config::MaxValidators`] plus the invulnerables and candidates, times
//! the blocks the chain produces per Aura slot. Otherwise bonded candidates are kicked as stale
//! between their slots. [`Config::MaxValidators`] rather than the cap is the bound, since the cap
//! can be raised or removed without a runtime upgrade.
//!
//! ## TODO
//!
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
pub mod migrations;
#[cfg(any(test, feature = "runtime-benchmarks"))]
mod relay_proof;
pub mod weights;

const LOG_TARGET: &str = "runtime::validator-collators";

/// Delivers the block counts of one report.
pub trait ReportAuthoredBlocks<AccountId> {
	/// Hand over the number of blocks each account authored.
	///
	/// `Ok` means the counts were accepted for delivery or credited, not that a remote chain
	/// applied them.
	#[allow(clippy::result_unit_err)]
	fn report(blocks: &[(AccountId, u32)]) -> Result<(), ()>;

	/// The encoded size of what [`Self::report`] sends for `blocks`.
	///
	/// By default the encoded `blocks`, as if the transport added nothing: an XCM envelope adds a
	/// few bytes. Checked against [`Self::max_message_size`] in the pallet's `integrity_test`.
	#[cfg(feature = "std")]
	fn message_size(blocks: &[(AccountId, u32)]) -> usize
	where
		AccountId: codec::Encode,
	{
		codec::Encode::encoded_size(blocks)
	}

	/// The largest message the transport accepts, `None` without a limit, as for a local credit.
	#[cfg(feature = "std")]
	fn max_message_size() -> Option<usize> {
		None
	}

	/// Prepare the runtime so that [`Self::report`] succeeds for `blocks` in benchmarks.
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful_report(_blocks: &[(AccountId, u32)]) {}
}

/// Discards the counts.
impl<AccountId> ReportAuthoredBlocks<AccountId> for () {
	fn report(_: &[(AccountId, u32)]) -> Result<(), ()> {
		Ok(())
	}
}

#[frame_support::pallet]
pub mod pallet {
	pub use crate::weights::WeightInfo;
	use crate::ReportAuthoredBlocks;
	use alloc::{collections::BTreeSet, vec, vec::Vec};
	use cumulus_pallet_parachain_system::{
		relay_state_snapshot::{Error as RelayProofError, ReadEntryErr},
		OnSystemEvent, RelayChainStateProof,
	};
	use cumulus_primitives_core::{
		relay_chain::well_known_keys, PersistedValidationData, RelayStorageKey,
	};
	use frame_support::{
		dispatch::DispatchClass,
		pallet_prelude::*,
		storage::with_storage_layer,
		traits::{EnsureOrigin, UnixTime, ValidatorRegistration},
		weights::WeightMeter,
		BoundedBTreeSet,
	};
	use frame_system::pallet_prelude::*;
	use pallet_session::{SessionManager, ShouldEndSession};
	use rand::{seq::SliceRandom, SeedableRng};
	use rand_chacha::ChaCha20Rng;
	use sp_runtime::{
		traits::{BlakeTwo256, Hash, Saturating, Zero},
		SaturatedConversion,
	};
	use sp_staking::{EraIndex, SessionIndex};

	/// A validator set received for an era.
	#[frame_support::stored]
	pub struct EraValidatorSet<AccountId, MaxValidators: Get<u32>> {
		/// The era the set belongs to.
		pub era: EraIndex,
		/// The validator stashes of that era.
		pub validators: BoundedBTreeSet<AccountId, MaxValidators>,
	}

	/// The collators returned for a session from the validator set of `era`.
	///
	/// Always a subset of the [`EraValidatorSet`] of the same `era`, which need not be the one in
	/// [`ExternalValidatorSet`] any more.
	#[frame_support::stored]
	pub struct EraCollators<AccountId, MaxValidators: Get<u32>> {
		/// The era of the set the collators were taken from.
		pub era: EraIndex,
		/// The collators, in drawn order under a cap and in account order without one.
		pub collators: BoundedVec<AccountId, MaxValidators>,
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
	#[pallet::storage_version(crate::migrations::STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	/// Configuration trait of this pallet.
	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Origin allowed to submit a validator set.
		type SetOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Origin allowed to change [`MaxCollators`], [`ReportPeriod`], [`ExpectedEraDuration`]
		/// and [`ReportMargin`].
		type UpdateOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Lookup of registered session keys.
		type ValidatorRegistration: ValidatorRegistration<Self::AccountId>;

		/// Maximum number of validators in a received set.
		#[pallet::constant]
		type MaxValidators: Get<u32>;

		/// Maximum number of entries of [`AuthoredBlocks`] that one report reads.
		///
		/// Authors are counted between two reports: the validators of the stored set, and the
		/// other collators of the chain, such as invulnerables and candidates. A sensible value is
		/// [`Config::MaxValidators`], plus the maximum number of other collators the chain may
		/// have, plus a small buffer for the authors that leave the collators before the next
		/// report. Entries beyond it wait for the next report. The `integrity_test` checks that a
		/// report of this many entries fits a block.
		#[pallet::constant]
		type MaxEntriesPerReport: Get<u32>;

		/// The regular session rotation rule kept next to the forced rotations.
		type PeriodicSession: ShouldEndSession<BlockNumberFor<Self>>;

		/// Hands the block counts of one report to the chain that credits them.
		type BlockReporter: ReportAuthoredBlocks<Self::AccountId>;

		/// Value of [`MaxCollators`] until governance sets it, `None` meaning no cap.
		///
		/// A runtime upgrade that changes it does not draw the collators again. Where
		/// [`MaxCollators`] is not stored and a set is, the upgrade needs a migration that sets
		/// [`PendingRotation`] to [`RotationState::AwaitingQueue`].
		#[pallet::constant]
		type DefaultMaxCollators: Get<Option<u32>>;

		/// Number of blocks between two reports until governance sets [`ReportPeriod`].
		#[pallet::constant]
		type DefaultReportPeriod: Get<BlockNumberFor<Self>>;

		/// Wall clock time, to time the report before the expected end of an era.
		type UnixTime: UnixTime;

		/// Value of [`ExpectedEraDuration`] until governance sets it, in milliseconds.
		#[pallet::constant]
		type DefaultExpectedEraDuration: Get<u64>;

		/// Value of [`ReportMargin`] until governance sets it, in milliseconds.
		#[pallet::constant]
		type DefaultReportMargin: Get<u64>;

		/// Largest value [`Pallet::set_report_margin`] accepts, in milliseconds.
		///
		/// A stored [`ReportMargin`] above it, as after a runtime upgrade that lowers it, is used
		/// up to it.
		#[pallet::constant]
		type MaxReportMargin: Get<u64>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	/// The latest received validator set.
	#[pallet::storage]
	pub type ExternalValidatorSet<T: Config> =
		StorageValue<_, EraValidatorSet<T::AccountId, T::MaxValidators>, OptionQuery>;

	/// Time at which we received the last [`ExternalValidatorSet`], in milliseconds. Removed when
	/// the report before the expected end of its era is made.
	#[pallet::storage]
	pub type ExternalSetReceivedAt<T: Config> = StorageValue<_, u64, OptionQuery>;

	/// Maximum number of validators returned as collators, `None` returns every validator with
	/// registered keys. Only validators with keys count against the cap. It is
	/// [`Config::DefaultMaxCollators`] until [`Pallet::set_max_collators`] stores a value,
	/// including an explicit `None`.
	///
	/// The cap applies before the union with the other session manager. A validator that the
	/// other session manager also returns takes a place under the cap without adding a collator.
	///
	/// The merged list becomes Aura's authority list, which Aura writes into the block header
	/// whenever it changes, and the relay chain rejects a header above its head-data limit. Size
	/// the cap so that the merged list, invulnerables and candidates included, keeps a
	/// session-change header within that limit.
	///
	/// Under a cap the validators are drawn at random, see [`DrawnCollators`].
	///
	/// A change with a stored set forces two rotations. The first draws [`DrawnCollators`] again
	/// from the stored set, so the drawn validators can change. The change is in force two blocks
	/// later. Removing the cap returns every validator of the stored set with registered keys.
	#[pallet::storage]
	pub type MaxCollators<T: Config> =
		StorageValue<_, Option<u32>, ValueQuery, <T as Config>::DefaultMaxCollators>;

	/// The collators last returned to pallet-session, taken from [`ExternalValidatorSet`].
	///
	/// Written at every session rotation, and holds only validators that had registered keys at
	/// that rotation. Without a cap it is every validator of the set with keys, in
	/// account order. Under a cap it is the draw made at the first rotation after a set arrives or
	/// the cap changes, and each later rotation returns it again without the validators that have
	/// removed their keys since. `None` when the last rotation returned no list, as before the
	/// first set arrives.
	///
	/// Pallet-session queues the returned list at one rotation and enacts it at the next, so this
	/// is ahead of the session validators while [`PendingRotation`] is
	/// [`RotationState::AwaitingEnactment`].
	///
	/// While [`PendingRotation`] is [`RotationState::AwaitingQueue`] it still belongs to the
	/// previous set or cap: it can be of an older era, missing before the first draw, and not a
	/// subset of [`ExternalValidatorSet`]. Otherwise it belongs to the era of
	/// [`ExternalValidatorSet`] and is a subset of it, of at most [`MaxCollators`] validators.
	#[pallet::storage]
	pub type DrawnCollators<T: Config> =
		StorageValue<_, EraCollators<T::AccountId, T::MaxValidators>, OptionQuery>;

	/// Progress of the forced rotations for the latest received set.
	#[pallet::storage]
	pub type PendingRotation<T: Config> = StorageValue<_, RotationState, ValueQuery>;

	/// The relay chain randomness of one epoch ago, read from the relay chain state proof of every
	/// block and written when it changes.
	#[pallet::storage]
	pub type EpochRandomness<T: Config> = StorageValue<_, [u8; 32], OptionQuery>;

	/// Number of blocks between two reports of [`AuthoredBlocks`]. A report is made in every
	/// block whose number is a multiple of it.
	#[pallet::storage]
	pub type ReportPeriod<T: Config> =
		StorageValue<_, BlockNumberFor<T>, ValueQuery, T::DefaultReportPeriod>;

	/// Blocks authored since the last report, per author.
	#[pallet::storage]
	pub type AuthoredBlocks<T: Config> =
		CountedStorageMap<_, Twox64Concat, T::AccountId, u32, ValueQuery>;

	/// Expected time from the arrival of a validator set to the end of its era, in milliseconds.
	#[pallet::storage]
	pub type ExpectedEraDuration<T: Config> =
		StorageValue<_, u64, ValueQuery, T::DefaultExpectedEraDuration>;

	/// Time before the expected end of an era at which the counts are reported, in milliseconds.
	/// Used up to [`Config::MaxReportMargin`] and up to half of [`ExpectedEraDuration`].
	#[pallet::storage]
	pub type ReportMargin<T: Config> = StorageValue<_, u64, ValueQuery, T::DefaultReportMargin>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A validator set was stored for an era.
		ValidatorSetReceived { era: EraIndex, count: u32 },
		/// The maximum number of validator collators was changed.
		MaxCollatorsSet { max: Option<u32> },
		/// No external set is stored, so no validators were returned for the session, with or
		/// without a cap. This happens in the sessions after this pallet is added to a runtime and
		/// before the first set arrives, so it is not an `UnexpectedKind`.
		NothingToDraw,
		/// `drawn_len` of the `external_len` validators of the set of `era`, all with registered
		/// keys, were returned for a session and recorded in [`DrawnCollators`]. `new_external` is
		/// `true` at the first rotation after a set arrives or the cap changes, and `false` at the
		/// later ones, which under a cap return the previous draw again.
		CollatorsDrawn { era: EraIndex, external_len: u32, drawn_len: u32, new_external: bool },
		/// The number of blocks between two reports was changed.
		ReportPeriodSet { period: BlockNumberFor<T> },
		/// The block counts of `authors` authors were removed. The `reported` of them in the
		/// stored set were handed to the reporter and the others were dropped.
		BlocksReported { authors: u32, reported: u32 },
		/// The reporter rejected the block counts. The counts of the `kept` authors in the stored
		/// set are kept for the next report, and those of the `dropped` other authors were
		/// removed.
		BlocksReportFailed { kept: u32, dropped: u32 },
		/// The expected time from the arrival of a set to the end of its era was changed.
		ExpectedEraDurationSet { duration: u64 },
		/// The time before the expected end of an era at which the counts are reported was
		/// changed.
		ReportMarginSet { margin: u64 },
		/// Something that should never happen happened. See [`UnexpectedKind`].
		Unexpected(UnexpectedKind),
	}

	/// The kinds of [`Event::Unexpected`].
	#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, TypeInfo, Debug)]
	pub enum UnexpectedKind {
		/// We expected a previously drawn set, but none was found.
		NoPreviousDrawSet,
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
		/// The report period must be greater than zero.
		ZeroReportPeriod,
		/// The expected era duration must be greater than zero.
		ZeroExpectedEraDuration,
		/// The report margin must be greater than zero.
		ZeroReportMargin,
		/// The report margin is above [`Config::MaxReportMargin`].
		ReportMarginTooHigh,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Charges the read of [`PendingRotation`] by [`ShouldEndSession::should_end_session`],
		/// which pallet-session makes in every block without charging it.
		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			T::DbWeight::get().reads(1)
		}

		/// Reports [`AuthoredBlocks`] at every multiple of [`ReportPeriod`] (`periodic_send` in the
		/// code), and once per stored set when [`ReportMargin`] or less remains until
		/// [`ExternalSetReceivedAt`] plus [`ExpectedEraDuration`] (`early_send` in the code).
		///
		/// The early report removes [`ExternalSetReceivedAt`], so it is made once per set and not
		/// retried. If the reporter rejects it, the counts go out with the next [`ReportPeriod`]
		/// report. Both rules in the same block make one report.
		///
		/// It runs after the inherents, so the timestamp is the one of this block. A report that
		/// does not fit `meter` is not made: an early report stays due and is made in the next
		/// block it fits, and the counts of a periodic report go out with the next report. During
		/// a multi-block migration no report is made, and the counts are kept.
		fn on_poll(now: BlockNumberFor<T>, meter: &mut WeightMeter) {
			// the base-weight, as per the benchmarks, is everything minus the report.
			let base_weight = T::WeightInfo::on_poll();
			if !meter.can_consume(base_weight) {
				return;
			}

			let early_send = ExternalSetReceivedAt::<T>::get().is_some_and(|received_at| {
				let duration = ExpectedEraDuration::<T>::get();
				let margin =
					ReportMargin::<T>::get().min(T::MaxReportMargin::get()).min(duration / 2);
				let now_ms: u64 = T::UnixTime::now().as_millis().saturated_into();
				now_ms >= received_at.saturating_add(duration.saturating_sub(margin))
			});
			let periodic_send = {
				let period = ReportPeriod::<T>::get();
				!period.is_zero() && (now % period).is_zero()
			};

			if !early_send && !periodic_send {
				// nothing should be sent, we're done.
				meter.consume(base_weight);
				return;
			}

			// either periodic send, or early send, which we don't care to distinguish anymore, and
			// just send the report.
			let authors = AuthoredBlocks::<T>::count().min(T::MaxEntriesPerReport::get());
			let report_weight = Self::report_weight(authors);
			if !meter.can_consume(base_weight.saturating_add(report_weight)) {
				log::warn!(
					target: crate::LOG_TARGET,
					"the report of authored blocks does not fit the block, postponing it"
				);
				meter.consume(base_weight);
				return;
			}

			// Remove the arrival only once the report is made, so the early report is made once
			// per set. Removing it sooner would lose an early report that does not fit `meter`,
			// which stays due for the next block.
			if early_send {
				ExternalSetReceivedAt::<T>::kill();
			}
			Self::report_blocks(authors);
			meter.consume(base_weight.saturating_add(report_weight));
		}

		fn integrity_test() {
			assert!(
				!T::DefaultReportPeriod::get().is_zero(),
				"`DefaultReportPeriod` must not be zero"
			);
			assert!(
				T::DefaultExpectedEraDuration::get() > 0,
				"`DefaultExpectedEraDuration` must not be zero"
			);
			let margin = T::DefaultReportMargin::get();
			assert!(
				margin > 0 && margin <= T::MaxReportMargin::get(),
				"`DefaultReportMargin` must not be zero and at most `MaxReportMargin`"
			);
			let max_items = T::MaxEntriesPerReport::get();
			assert!(
				max_items >= T::MaxValidators::get(),
				"`MaxEntriesPerReport` must leave room for every validator of a set"
			);
			// `on_poll` postpones a report that does not fit, so one that does not fit an empty
			// block is never made.
			let report = T::WeightInfo::on_poll().saturating_add(Self::report_weight(max_items));
			let max_block = T::BlockWeights::get().max_block;
			assert!(
				report.all_lte(max_block),
				"a report of {max_items} authors weighs {report:?}, above the block {max_block:?}"
			);

			#[cfg(feature = "std")]
			if let Some(max) = T::BlockReporter::max_message_size() {
				// biggest possible report we could send.
				let entries = (0..T::MaxValidators::get())
					.map(|i| {
						let who = T::AccountId::decode(
							&mut sp_runtime::traits::TrailingZeroInput::new(&i.encode()),
						)
						.expect("infinite input decodes into an account; qed");
						(who, u32::MAX)
					})
					.collect::<Vec<_>>();
				let size = T::BlockReporter::message_size(&entries);
				assert!(
					size <= max,
					"a report of {} entries is {size} bytes, above the limit of {max}",
					entries.len()
				);
			}
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
		/// With a stored set, the change forces two rotations and the first draws
		/// [`DrawnCollators`] again, so the drawn validators can change. `None` returns the whole
		/// stored set from then on, and replaces [`Config::DefaultMaxCollators`].
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::set_max_collators())]
		pub fn set_max_collators(origin: OriginFor<T>, max: Option<u32>) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;
			MaxCollators::<T>::set(max);
			if ExternalValidatorSet::<T>::exists() {
				PendingRotation::<T>::put(RotationState::AwaitingQueue);
			}
			Self::deposit_event(Event::MaxCollatorsSet { max });
			Ok(())
		}

		/// Set the number of blocks between two reports of the authored blocks.
		///
		/// The next report is made at the next multiple of `period`.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::set_report_period())]
		pub fn set_report_period(
			origin: OriginFor<T>,
			period: BlockNumberFor<T>,
		) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;
			ensure!(!period.is_zero(), Error::<T>::ZeroReportPeriod);
			ReportPeriod::<T>::put(period);
			Self::deposit_event(Event::ReportPeriodSet { period });
			Ok(())
		}

		/// Set the expected time from the arrival of a validator set to the end of its era, in
		/// milliseconds.
		///
		/// It applies to the stored set too, whose arrival time is kept.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::set_expected_era_duration())]
		pub fn set_expected_era_duration(origin: OriginFor<T>, duration: u64) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;
			ensure!(duration > 0, Error::<T>::ZeroExpectedEraDuration);
			ExpectedEraDuration::<T>::put(duration);
			Self::deposit_event(Event::ExpectedEraDurationSet { duration });
			Ok(())
		}

		/// Set the time before the expected end of an era at which the counts are reported, in
		/// milliseconds.
		///
		/// It applies to the stored set too, whose arrival time is kept.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::set_report_margin())]
		pub fn set_report_margin(origin: OriginFor<T>, margin: u64) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;
			ensure!(margin > 0, Error::<T>::ZeroReportMargin);
			ensure!(margin <= T::MaxReportMargin::get(), Error::<T>::ReportMarginTooHigh);
			ReportMargin::<T>::put(margin);
			Self::deposit_event(Event::ReportMarginSet { margin });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Store the validator set of `era` and its arrival time, and schedule the rotations that
		/// bring it into force.
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
				ExternalValidatorSet::<T>::get().is_none_or(|stored| era > stored.era),
				Error::<T>::StaleEra
			);
			let count = validators.len() as u32;
			ExternalValidatorSet::<T>::put(EraValidatorSet { era, validators });
			ExternalSetReceivedAt::<T>::put(T::UnixTime::now().as_millis().saturated_into::<u64>());
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

		/// Draw up to `cap` validators with registered keys from `set`, in drawn order.
		///
		/// The whole set is shuffled before the key filter, so the order depends only on the seed
		/// and the set.
		fn draw_collators(
			set: EraValidatorSet<T::AccountId, T::MaxValidators>,
			cap: u32,
		) -> Vec<T::AccountId> {
			let EraValidatorSet { era, validators } = set;
			let mut order: Vec<_> = validators.into_iter().collect();
			order.shuffle(&mut ChaCha20Rng::from_seed(Self::draw_seed(era)));
			order
				.into_iter()
				.filter(T::ValidatorRegistration::is_registered)
				.take(cap as usize)
				.collect()
		}

		/// Weight of [`Self::report_blocks`] with `authors` entries read from [`AuthoredBlocks`].
		///
		/// An entry of a validator in the stored set is handed to the reporter and removed, while
		/// an entry of another author is only removed, which costs less. So the weight assumes as
		/// many validators as the set holds, and the other authors beyond them.
		pub(crate) fn report_weight(authors: u32) -> Weight {
			let validators = authors.min(T::MaxValidators::get());
			T::WeightInfo::report_blocks(validators, authors.saturating_sub(validators))
		}

		/// Hand the counted blocks of the stored validators to [`Config::BlockReporter`].
		///
		/// Hands the counts of every author in [`ExternalValidatorSet`] to the reporter in one
		/// call, and removes the counts of the other authors. The filter is the latest set, so
		/// once a new set arrives, the counts of a [`DrawnCollators`] validator that is not in it
		/// are dropped. The counts handed over are removed only if the reporter accepts them, and
		/// the reporter's storage changes are rolled back if it rejects them. Without a stored
		/// set every count is removed.
		///
		/// `authors` is the number of entries in [`AuthoredBlocks`], which the caller reads to
		/// weigh the report with [`Self::report_weight`] before making it. A rejected report
		/// removes fewer counts than an accepted one, which is the weighed case.
		pub(crate) fn report_blocks(authors: u32) {
			if authors == 0 {
				return;
			}

			let validators =
				ExternalValidatorSet::<T>::get().map(|set| set.validators).unwrap_or_default();

			// read all authored-blocks, and partition them in those that we send and those we may
			// drop.
			let (to_send, others): (Vec<_>, Vec<_>) = AuthoredBlocks::<T>::iter()
				.take(T::MaxEntriesPerReport::get() as usize)
				.partition(|(who, _)| validators.contains(who));
			let (kept, dropped) = (to_send.len() as u32, others.len() as u32);

			// We definitely want to delete these ones.
			others.iter().for_each(|(who, _)| AuthoredBlocks::<T>::remove(who));

			// No message is sent for a report with no entries.
			let result = with_storage_layer(|| match to_send.is_empty() {
				true => Ok(()),
				false => T::BlockReporter::report(&to_send)
					.map_err(|()| DispatchError::Other("the reporter rejected the counts")),
			});

			let event = match result {
				Ok(()) => {
					// we managed to send, delete the sent ones as well.
					to_send.iter().for_each(|(who, _)| AuthoredBlocks::<T>::remove(who));
					Event::BlocksReported { authors, reported: kept }
				},
				Err(_) => {
					// we failed to send, we keep the ones that were authors now, having already
					// dropped the rest.
					log::warn!(
						target: crate::LOG_TARGET,
						"the reporter rejected the counts of {kept} authors, keeping them and \
						dropping those of {dropped} authors outside the stored set"
					);
					Event::BlocksReportFailed { kept, dropped }
				},
			};
			Self::deposit_event(event);
		}

		/// Check the pallet invariants.
		#[cfg(any(test, feature = "try-runtime"))]
		pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
			// `a` implies `b`: fails only if `a` holds and `b` does not.
			let implies = |a: bool, b: bool| !a || b;

			// ExternalValidatorSet related
			ensure!(
				implies(
					PendingRotation::<T>::get() != RotationState::Idle,
					ExternalValidatorSet::<T>::exists()
				),
				"a rotation is pending without a stored validator set"
			);
			ensure!(
				ExternalValidatorSet::<T>::exists() == ExternalValidatorSet::<T>::get().is_some(),
				"the stored validator set does not decode, it may exceed `MaxValidators`"
			);

			// DrawnCollators related
			ensure!(
				implies(DrawnCollators::<T>::exists(), ExternalValidatorSet::<T>::exists()),
				"drawn collators are stored without a validator set"
			);
			ensure!(
				DrawnCollators::<T>::exists() == DrawnCollators::<T>::get().is_some(),
				"the drawn collators do not decode"
			);
			ensure!(
				AuthoredBlocks::<T>::iter_values().all(|blocks| blocks > 0),
				"an author is counted with zero blocks"
			);
			ensure!(ExpectedEraDuration::<T>::get() > 0, "the expected era duration is zero");
			ensure!(ReportMargin::<T>::get() > 0, "the report margin is zero");

			if PendingRotation::<T>::get() == RotationState::AwaitingQueue {
				// Until the next rotation draws, the drawn collators still belong to the previous
				// set or cap: of an older era after a new set, possibly by more than one era, or of
				// the same era after a cap change. Only their era is checked.
				if let (Some(set), Some(drawn)) =
					(ExternalValidatorSet::<T>::get(), DrawnCollators::<T>::get())
				{
					ensure!(
						drawn.era <= set.era,
						"the drawn collators belong to a newer era than the stored set"
					);
				}
				return Ok(());
			}

			// Keys can change between rotations, so the key filter of the last rotation is not
			// checked.
			let Some(set) = ExternalValidatorSet::<T>::get() else { return Ok(()) };
			let drawn = DrawnCollators::<T>::get().ok_or("no collators are drawn for the set")?;
			ensure!(drawn.era == set.era, "the drawn collators belong to another era");
			let mut seen = BTreeSet::new();
			ensure!(
				drawn.collators.iter().all(|v| seen.insert(v)),
				"the drawn collators contain duplicates"
			);
			ensure!(
				drawn.collators.iter().all(|v| set.validators.contains(v)),
				"a drawn collator is not in the stored validator set"
			);
			match MaxCollators::<T>::get() {
				Some(cap) => {
					ensure!(
						drawn.collators.len() <= cap as usize,
						"more collators are drawn than the cap allows"
					);
					// With the pallet wired as `OnSystemEvent`, `on_relay_state_proof` caches a
					// value in every block's inherent and the cache is never removed. So this holds
					// for every draw after the first block that runs the pallet, and flags a
					// runtime without that wiring.
					ensure!(
						EpochRandomness::<T>::exists(),
						"collators are drawn without a cached relay chain epoch randomness"
					);
				},
				None => ensure!(
					drawn.collators.windows(2).all(|pair| pair[0] < pair[1]),
					"without a cap the drawn collators are not in account order"
				),
			}
			Ok(())
		}

		/// The stored set, or `None` without one. A stored set that does not decode is `None` too,
		/// and is logged and reported with [`UnexpectedKind::StoredSetUndecodable`].
		fn external_validator_set() -> Option<EraValidatorSet<T::AccountId, T::MaxValidators>> {
			if let Some(external_set) = ExternalValidatorSet::<T>::get() {
				return Some(external_set);
			} else if ExternalValidatorSet::<T>::exists() {
				log::error!(
					target: crate::LOG_TARGET,
					"the stored external validator set does not decode"
				);
				Self::deposit_event(Event::Unexpected(UnexpectedKind::StoredSetUndecodable));
			}
			None
		}
	}

	impl<T: Config> SessionManager<T::AccountId> for Pallet<T> {
		fn new_session(_: SessionIndex) -> Option<Vec<T::AccountId>> {
			// Advance the forced rotations. `AwaitingQueue` means a new external set or a new cap,
			// so we draw again.
			let new_external = match PendingRotation::<T>::get() {
				RotationState::AwaitingQueue => {
					PendingRotation::<T>::put(RotationState::AwaitingEnactment);
					true
				},
				RotationState::AwaitingEnactment => {
					PendingRotation::<T>::kill();
					false
				},
				RotationState::Idle => false,
			};

			let Some(external_set) = Self::external_validator_set() else {
				// Nothing is returned, so nothing is recorded. Without a stored set there is
				// nothing to remove, but a set that no longer decodes leaves its collators behind.
				DrawnCollators::<T>::kill();
				Self::deposit_event(Event::NothingToDraw);
				return None;
			};
			let (era, external_len) = (external_set.era, external_set.validators.len() as u32);

			// Drawn and filtered at the rotation, which pallet-session weighs as the whole block,
			// since the key reads may not fit the weight of the message or report that delivers
			// the set.
			let collators: Vec<_> = match (MaxCollators::<T>::get(), new_external) {
				// no cap: every validator of the set with keys, in account order.
				(None, _) => external_set
					.validators
					.into_iter()
					.filter(T::ValidatorRegistration::is_registered)
					.collect(),
				// a cap, and a new set or cap: draw again, from the validators with keys.
				(Some(cap), true) => Self::draw_collators(external_set, cap),
				// a cap, nothing new: the last draw, without those that removed their keys since.
				(Some(_), false) => match DrawnCollators::<T>::get() {
					Some(prev) => prev
						.collators
						.into_iter()
						.filter(T::ValidatorRegistration::is_registered)
						.collect(),
					None => {
						Self::deposit_event(Event::Unexpected(UnexpectedKind::NoPreviousDrawSet));
						return None;
					},
				},
			};
			// At most the length of the set, which is bounded by `MaxValidators`.
			let collators = BoundedVec::truncate_from(collators);

			Self::deposit_event(Event::CollatorsDrawn {
				era,
				external_len,
				drawn_len: collators.len() as u32,
				new_external,
			});
			DrawnCollators::<T>::put(EraCollators { era, collators: collators.clone() });
			Some(collators.into_inner())
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

	impl<T: Config> pallet_authorship::EventHandler<T::AccountId, BlockNumberFor<T>> for Pallet<T> {
		/// Counts the block for `author` in [`AuthoredBlocks`].
		fn note_author(author: T::AccountId) {
			frame_system::Pallet::<T>::register_extra_weight_unchecked(
				T::WeightInfo::note_author(),
				DispatchClass::Mandatory,
			);
			AuthoredBlocks::<T>::mutate(author, |blocks| blocks.saturating_inc());
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
