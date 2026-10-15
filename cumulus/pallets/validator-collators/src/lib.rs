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
//! [`Config::ValidatorRegistration`], except those dropped for authoring no blocks. The runtime
//! combines this pallet with `pallet-collator-selection` through
//! [`pallet_session::UnionSessionManager`], so invulnerables and candidates keep collating next to
//! the validators.
//!
//! [`MaxCollators`] optionally caps how many validators are returned. Governance can start with a
//! low cap and raise it in steps, or lower it again, to roll out validator collators gradually.
//! Under a cap the pallet draws the validators at random at the first forced rotation after a set
//! arrives or the cap changes, stores them in [`Collators`] and returns those with registered keys
//! until the next draw. The draw shuffles the whole set with a seed made of the genesis hash, the
//! era and the relay chain epoch randomness cached in [`EpochRandomness`], then keeps the first
//! validators with registered keys up to the cap, skipping those in [`Dropped`]. Every validator
//! with keys that is not dropped has the same chance to be drawn in each era. A cap change draws
//! again from the stored set, so the drawn validators can change. Every draw takes its seed from
//! the randomness cached at the time of the draw. The seed includes the genesis hash, so each chain
//! draws independently from the same set. A validator that registers keys after the draw waits for
//! the next set or for a cap change. Each set, so each era, gets a new draw, which is enacted at
//! the second forced rotation with no notice, so a validator with keys must already run a synced
//! collator node to collate when drawn.
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
//! produces in [`AuthoredBlocks`]. At a report it hands the counts of the authors in the stored set
//! to [`Config::BlockReporter`], in chunks of at most [`Config::MaxEntriesPerReport`] entries, and
//! drops the counts of other authors. The reporter delivers them to the chain that runs
//! `pallet-staking-async`, where they become era points. All chunks of one report are handed over
//! in one storage layer. If the reporter rejects any of them, every count is kept for the next
//! report and the chunks already handed over are rolled back. Without a stored set every count is
//! dropped.
//!
//! A report runs in every block whose number is a multiple of [`ReportPeriod`], and once per
//! stored set shortly before the expected end of its era. The pallet records the arrival time of
//! each set in [`SetReceivedAt`], from [`Config::UnixTime`], and expects the era to end
//! [`ExpectedEraDuration`] after it. The early report runs in the first block at or after
//! [`ReportMargin`] before that end, so that in a normal era the counts reach the chain that runs
//! `pallet-staking-async` before the era rotates. The margin used is at most
//! [`Config::MaxReportMargin`] and at most half of [`ExpectedEraDuration`]. The early report
//! removes [`SetReceivedAt`] and is not retried: if the reporter rejects it, its counts go out with
//! the next [`ReportPeriod`] report. The periodic report also covers eras that end earlier or later
//! than expected, a set that never arrives, and the time before the first set arrives. If both
//! reports fall in the same block, the pallet reports once.
//!
//! Governance can set a [`DropRule`] in [`CurrentDropRule`] to stop returning validators that
//! author no blocks. The rule measures time in counted Aura slots. [`CountedSlots`] advances in
//! every block, with or without a rule, so a rule set later judges on a live count. It adds the
//! slots since the previous block, read from the Aura pre-runtime digest, if they are at most the
//! Aura authority count, and one slot otherwise. While one authority authors, a run of authorities
//! that author nothing is shorter than the list, so its empty slots count in full. A longer gap
//! means that every authority missed a turn, as in a chain stall, and counts as one slot. So a
//! counted slot is never more than an elapsed slot. The authority count is that of the list in
//! force during the gap, since the first advance of a block runs before its session rotation
//! changes the list. With no authorities counted, every gap counts as one slot. A run of slots
//! without blocks no longer than the list counts in full whatever its cause, so a partial relay
//! chain outage or collators that are not synced look like idleness. Governance raises `rounds`
//! or removes the rule during such incidents.
//!
//! Counts are in slots, so a change of the slot duration changes the wall time of windows and
//! drops. A change that lowers the slot numbers re-anchors the count at the new slot without
//! adding to it, and one that raises them by more than the authority count counts as one slot.
//!
//! [`LastActivity`] holds, for each validator returned at the last rotation, the count of the
//! block it last authored or, before its first block, of the rotation that enacted it, with the
//! largest Aura authority count of any list in force since then. A returned validator whose
//! record is at least `rounds × authorities` counted slots old is idle. This number of slots is
//! the window. Aura gives slot `s` to authority `s mod authorities`, so while the authority list
//! does not change, every authority has at least `rounds` slots in a window. A change of the list
//! within a window, such as a candidate joining or leaving, keys set or purged, a new set, a cap
//! change, or validators dropped or returned by this pallet, moves the authorities to other
//! positions and can cost an authority one of its slots. The `min_collators` floor bounds how many
//! validators one rotation drops.
//! A stall can hide a turn of every authority but counts as one slot, so like a list change it
//! costs an authority at most one of its slots. So a rule allows one more round than the list
//! changes and stalls expected in a window, and needs at least two rounds.
//!
//! A rotation drops the idle validators: it does not return them. Pallet-session enacts the list
//! a rotation returns at the next rotation, so a dropped validator keeps its slots until then.
//! That next rotation starts the drop: no rotation returns the validator within `slots` counted
//! slots from it, see [`Dropped`]. The first rotation at or after that end returns it again, less
//! than one session period later, and it gets its slots back at the rotation after that one. So a
//! dropped validator holds no slots for at least `slots` counted slots, unless the rule is removed
//! or the validator leaves the stored set during the drop. A validator returned for the first
//! time, or again after a drop, is first judged a full window after the rotation that enacts it.
//! A rotation never drops validators below `min_collators` of the validators it would return, and
//! keeps the idle ones with the most recent records. Under a cap only the drawn validators are
//! judged, and a place that empties after the draw stays empty until the drop ends or the next
//! draw. A validator dropped before a draw is not drawn, so after its drop it waits for the next
//! draw. Without a rule nobody is dropped.
//!
//! A runtime must configure `pallet-collator-selection`'s `KickThreshold` to exceed a full Aura
//! round of the merged list, [`Config::MaxValidators`] plus the invulnerables and candidates, times
//! the blocks the chain produces per Aura slot. Otherwise bonded candidates are kicked as stale
//! between their slots. [`Config::MaxValidators`] rather than the cap is the bound, since the cap
//! can be raised or removed without a runtime upgrade.
//!
//! ## TODO
//!
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
	use alloc::{
		collections::{BTreeMap, BTreeSet},
		vec,
		vec::Vec,
	};
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
		BoundedBTreeMap, BoundedBTreeSet,
	};
	use frame_system::pallet_prelude::*;
	use pallet_session::{SessionManager, ShouldEndSession};
	use rand::{seq::SliceRandom, SeedableRng};
	use rand_chacha::ChaCha20Rng;
	use sp_consensus_aura::{Slot, AURA_ENGINE_ID};
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

	/// The validators drawn under [`MaxCollators`], with the era of the set they were drawn from.
	#[frame_support::stored]
	pub struct DrawnCollators<AccountId, MaxValidators: Get<u32>> {
		/// The era of the set the validators were drawn from.
		pub era: EraIndex,
		/// The drawn validators, in drawn order.
		pub validators: BoundedVec<AccountId, MaxValidators>,
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

	/// Activity of a validator this pallet returns as a collator.
	#[derive(Clone, Copy, Eq, PartialEq, Encode, Decode, Debug, TypeInfo, MaxEncodedLen)]
	pub enum Activity {
		/// Returned at the last rotation, a collator from the next one.
		Queued,
		/// A collator since `since`, the [`CountedSlots`] count of the block it last authored or,
		/// before its first block, of the block that enacted it, with the largest number of Aura
		/// authorities of any list in force since then. Written as zero and raised at every
		/// session rotation.
		Active { since: u64, authorities: u32 },
	}

	/// The Aura slots counted for a [`DropRule`].
	#[derive(Clone, Copy, Eq, PartialEq, Encode, Decode, Debug, TypeInfo, MaxEncodedLen)]
	pub struct SlotCount {
		/// The slot of the last block that advanced the count.
		pub slot: Slot,
		/// Slots counted since the first block that ran this pallet, a gap between two blocks
		/// longer than the Aura authority count counted as one slot.
		pub count: u64,
	}

	/// The rule that drops validators which author no blocks.
	///
	/// Its lengths are counted Aura slots, see [`CountedSlots`], which are `u64` like Aura's
	/// [`Slot`].
	#[derive(
		Clone,
		Copy,
		Eq,
		PartialEq,
		Encode,
		Decode,
		DecodeWithMemTracking,
		Debug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct DropRule {
		/// Full Aura rounds of the authority list a returned validator may go without authoring
		/// before it is dropped. At least two.
		///
		/// Each change of the authority list within a window, including validators dropped or
		/// returned by this pallet, moves the authorities to other positions and can cost an
		/// authority one of its slots, and so can a stall. So a rule allows one more round than
		/// the list changes and stalls expected in a window. `min_collators` bounds how many
		/// validators one rotation drops.
		pub rounds: u32,
		/// Counted slots during which a dropped validator is not returned, counted from the
		/// rotation that enacts the list without it, so it holds no slots for at least this many
		/// counted slots. At least one.
		///
		/// Drops end at rotations, so the first rotation at or after the end returns the validator
		/// again.
		pub slots: u64,
		/// Number of validators a rotation never drops below, among the validators this pallet
		/// would return, so under a cap it is bounded by the cap. The idle validators with the
		/// most recent records are kept: the [`CountedSlots`] count of the block each last
		/// authored or, before its first block, of the block that enacted it.
		///
		/// When this pallet is the only session manager that returns collators, the floor keeps
		/// the session list from becoming empty. Aura ignores an empty list, so the previous
		/// authorities, dropped ones included, would keep their slots.
		pub min_collators: u32,
	}

	/// Drop records, with the [`CountedSlots`] count from which each validator is returned again,
	/// `None` until the rotation that enacts the drop.
	pub type DropRecords<T> = BoundedBTreeMap<
		<T as frame_system::Config>::AccountId,
		Option<u64>,
		<T as Config>::MaxValidators,
	>;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Configuration trait of this pallet.
	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Origin allowed to submit a validator set.
		type SetOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Origin allowed to change [`MaxCollators`], [`ReportPeriod`], [`ExpectedEraDuration`],
		/// [`ReportMargin`] and [`CurrentDropRule`].
		type UpdateOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Lookup of registered session keys.
		type ValidatorRegistration: ValidatorRegistration<Self::AccountId>;

		/// Maximum number of validators in a received set.
		#[pallet::constant]
		type MaxValidators: Get<u32>;

		/// The regular session rotation rule kept next to the forced rotations.
		type PeriodicSession: ShouldEndSession<BlockNumberFor<Self>>;

		/// Hands the block counts of one report to the chain that credits them.
		type BlockReporter: ReportAuthoredBlocks<Self::AccountId>;

		/// Number of blocks between two reports until governance sets [`ReportPeriod`].
		#[pallet::constant]
		type DefaultReportPeriod: Get<BlockNumberFor<Self>>;

		/// Maximum number of entries handed to [`Config::BlockReporter`] in one call.
		///
		/// A chunk of this size must fit what the receiving chain can process in one message.
		#[pallet::constant]
		type MaxEntriesPerReport: Get<u32>;

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

		/// Number of Aura authorities of the current session.
		///
		/// The authority list Aura assigns slots on, so the merged list of every session manager.
		/// Read in every block that advances [`CountedSlots`].
		type AuthorityCount: Get<u32>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	/// The latest received validator set.
	#[pallet::storage]
	pub type ValidatorSet<T: Config> =
		StorageValue<_, EraValidatorSet<T::AccountId, T::MaxValidators>, OptionQuery>;

	/// Progress of the forced rotations for the latest received set.
	#[pallet::storage]
	pub type PendingRotation<T: Config> = StorageValue<_, RotationState, ValueQuery>;

	/// Maximum number of validators returned as collators, `None` returns every validator with
	/// registered keys.
	///
	/// The cap applies before the union with the other session manager. A validator that the
	/// other session manager also returns takes a place under the cap without adding a collator.
	///
	/// The merged list becomes Aura's authority list, which Aura writes into the block header
	/// whenever it changes, and the relay chain rejects a header above its head-data limit. Size
	/// the cap so that the merged list, invulnerables and candidates included, keeps a
	/// session-change header within that limit.
	///
	/// Under a cap the validators are drawn at random, see [`Collators`].
	///
	/// A change with a stored set forces two rotations. The first draws [`Collators`] again from
	/// the stored set, so the drawn validators can change. The change is in force two blocks
	/// later. Removing the cap returns every validator with registered keys.
	#[pallet::storage]
	pub type MaxCollators<T: Config> = StorageValue<_, u32, OptionQuery>;

	/// The validators drawn from the stored set under [`MaxCollators`].
	///
	/// Drawn at the first forced rotation after a set arrives or the cap changes, with a seed made
	/// of the genesis hash, the era of the stored set and the cached [`EpochRandomness`]. Removed
	/// when the cap is removed. At every rotation the pallet returns the drawn validators that
	/// still have registered keys and are not in [`Dropped`].
	#[pallet::storage]
	pub type Collators<T: Config> =
		StorageValue<_, DrawnCollators<T::AccountId, T::MaxValidators>, OptionQuery>;

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

	/// Time at which the stored validator set arrived, in milliseconds. Removed when the report
	/// before the expected end of its era is made.
	#[pallet::storage]
	pub type SetReceivedAt<T: Config> = StorageValue<_, u64, OptionQuery>;

	/// Expected time from the arrival of a validator set to the end of its era, in milliseconds.
	#[pallet::storage]
	pub type ExpectedEraDuration<T: Config> =
		StorageValue<_, u64, ValueQuery, T::DefaultExpectedEraDuration>;

	/// Time before the expected end of an era at which the counts are reported, in milliseconds.
	/// Used up to [`Config::MaxReportMargin`] and up to half of [`ExpectedEraDuration`].
	#[pallet::storage]
	pub type ReportMargin<T: Config> = StorageValue<_, u64, ValueQuery, T::DefaultReportMargin>;

	/// The rule in force. Absent, no validator is dropped.
	#[pallet::storage]
	pub type CurrentDropRule<T: Config> = StorageValue<_, DropRule, OptionQuery>;

	/// The Aura slots counted since the first block that ran this pallet.
	///
	/// Advanced once per block, with or without a [`CurrentDropRule`], at whichever of this
	/// pallet's `on_initialize`, the authorship handler and the session rotation runs first in the
	/// block. It adds the slots since the slot it holds if they are at most the Aura authority
	/// count, and one slot otherwise. The slot comes from the Aura pre-runtime digest of the block,
	/// so the count is current whatever the order of the pallets. A block in the same slot, or
	/// without the digest, leaves it unchanged, and a block in an earlier slot stores that slot
	/// without adding to the count.
	#[pallet::storage]
	pub type CountedSlots<T: Config> = StorageValue<_, SlotCount, OptionQuery>;

	/// Activity of the validators this pallet returned at the last rotation.
	///
	/// An [`Activity::Queued`] record holds no count: it becomes [`Activity::Active`] at the
	/// rotation that enacts the validator.
	///
	/// Created and removed at session rotations only, and refreshed when the validator authors.
	/// Kept with or without a [`CurrentDropRule`], so a rule set later judges on the activity
	/// recorded before it, from the next rotation.
	#[pallet::storage]
	pub type LastActivity<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, Activity, OptionQuery>;

	/// Validators of the stored set dropped for authoring no blocks, with the [`CountedSlots`]
	/// count from which each is returned again.
	///
	/// A record holds `None` from the rotation that drops the validator to the next one, which
	/// enacts the list without it and sets the end to the count there plus the `slots` of the
	/// rule in force. Records end at rotations: the first rotation at or after that count returns
	/// the validator again. Under a cap, a validator dropped before a draw is not drawn, so it is
	/// returned only after the next draw.
	///
	/// A record is removed when it expires, when its validator leaves the stored set, and with
	/// every other record when the rule is removed. Removing the rule also removes a value that no
	/// longer decodes.
	#[pallet::storage]
	pub type Dropped<T: Config> = StorageValue<_, DropRecords<T>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A validator set was stored for an era.
		ValidatorSetReceived { era: EraIndex, count: u32 },
		/// The maximum number of validator collators was changed.
		MaxCollatorsSet { max: Option<u32> },
		/// Validators were drawn from the set of `era` under the cap.
		CollatorsDrawn { era: EraIndex, validators: Vec<T::AccountId> },
		/// The stored validator set does not decode, so no validators were returned for the
		/// session.
		StoredSetUndecodable,
		/// The number of blocks between two reports was changed.
		ReportPeriodSet { period: BlockNumberFor<T> },
		/// The block counts of `authors` authors were removed. The `reported` of them in the
		/// stored set were handed to the reporter and the others were dropped.
		BlocksReported { authors: u32, reported: u32 },
		/// The reporter rejected the block counts, which are kept for the next report.
		BlocksReportFailed { authors: u32 },
		/// The expected time from the arrival of a set to the end of its era was changed.
		ExpectedEraDurationSet { duration: u64 },
		/// The time before the expected end of an era at which the counts are reported was
		/// changed.
		ReportMarginSet { margin: u64 },
		/// The rule that drops validators which author no blocks was set or removed.
		DropRuleSet { rule: Option<DropRule> },
		/// `validators` authored no blocks within the window and are no longer returned. Their
		/// drops start at the next rotation, which sets their end in [`Dropped`]. `kept` idle
		/// validators are still returned because of the floor.
		///
		/// Emitted with an empty `validators` when the floor keeps every idle validator.
		CollatorsDropped { validators: Vec<T::AccountId>, kept: u32 },
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
		/// A drop rule needs at least two rounds and at least one slot.
		InvalidDropRule,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Reports [`AuthoredBlocks`] at every multiple of [`ReportPeriod`], and once per stored
		/// set when [`ReportMargin`] or less remains until [`SetReceivedAt`] plus
		/// [`ExpectedEraDuration`].
		///
		/// The early report removes [`SetReceivedAt`], so it is made once per set and not
		/// retried. If the reporter rejects it, the counts go out with the next [`ReportPeriod`]
		/// report. Both rules in the same block make one report.
		///
		/// Advances [`CountedSlots`] if no earlier hook of the block did.
		///
		/// The weight includes the read of [`PendingRotation`] by
		/// [`ShouldEndSession::should_end_session`], which pallet-session makes in every block
		/// without charging it.
		fn on_initialize(now: BlockNumberFor<T>) -> Weight {
			Self::advance_counted_slots(Self::current_slot());
			let early = SetReceivedAt::<T>::get().is_some_and(|received_at| {
				let duration = ExpectedEraDuration::<T>::get();
				let margin =
					ReportMargin::<T>::get().min(T::MaxReportMargin::get()).min(duration / 2);
				let now_ms: u64 = T::UnixTime::now().as_millis().saturated_into();
				now_ms >= received_at.saturating_add(duration.saturating_sub(margin))
			});
			if early {
				SetReceivedAt::<T>::kill();
			}
			let period = ReportPeriod::<T>::get();
			let report = if early || (!period.is_zero() && (now % period).is_zero()) {
				Self::report_blocks()
			} else {
				Weight::zero()
			};
			T::WeightInfo::on_initialize().saturating_add(report)
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
		/// With a stored set, the change forces two rotations and the first draws [`Collators`]
		/// again, so the drawn validators can change. `None` clears [`Collators`].
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::set_max_collators())]
		pub fn set_max_collators(origin: OriginFor<T>, max: Option<u32>) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;
			MaxCollators::<T>::set(max);
			if max.is_none() {
				Collators::<T>::kill();
			}
			if ValidatorSet::<T>::exists() {
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

		/// Set the rule that drops validators which author no blocks, or remove it.
		///
		/// The rule applies from the next rotation. Changing it keeps the end of the drops already
		/// counting, and the drops that start at the next rotation take the `slots` of the rule in
		/// force then. Removing it also removes every drop record, so every validator is returned
		/// again at the next rotation.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::set_drop_rule())]
		pub fn set_drop_rule(origin: OriginFor<T>, rule: Option<DropRule>) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;
			match rule {
				Some(rule) => {
					ensure!(rule.rounds >= 2 && rule.slots > 0, Error::<T>::InvalidDropRule)
				},
				None => Dropped::<T>::kill(),
			}
			CurrentDropRule::<T>::set(rule);
			Self::deposit_event(Event::DropRuleSet { rule });
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
				ValidatorSet::<T>::get().is_none_or(|stored| era > stored.era),
				Error::<T>::StaleEra
			);
			let count = validators.len() as u32;
			ValidatorSet::<T>::put(EraValidatorSet { era, validators });
			SetReceivedAt::<T>::put(T::UnixTime::now().as_millis().saturated_into::<u64>());
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

		/// Draw up to `cap` validators with registered keys and not in `dropped` from `set` into
		/// [`Collators`] and emit [`Event::CollatorsDrawn`].
		///
		/// The whole set is shuffled before the filter, so the order depends only on the seed and
		/// the set.
		fn draw_collators(
			set: EraValidatorSet<T::AccountId, T::MaxValidators>,
			cap: u32,
			dropped: &DropRecords<T>,
		) -> BoundedVec<T::AccountId, T::MaxValidators> {
			let EraValidatorSet { era, validators } = set;
			let mut order: Vec<_> = validators.into_iter().collect();
			order.shuffle(&mut ChaCha20Rng::from_seed(Self::draw_seed(era)));
			let drawn: Vec<_> = order
				.into_iter()
				.filter(|v| T::ValidatorRegistration::is_registered(v) && !dropped.contains_key(v))
				.take(cap as usize)
				.collect();
			Self::deposit_event(Event::CollatorsDrawn { era, validators: drawn.clone() });
			// At most the length of the set, which is bounded by `MaxValidators`.
			let validators = BoundedVec::truncate_from(drawn);
			Collators::<T>::put(DrawnCollators { era, validators: validators.clone() });
			validators
		}

		/// Hand the counted blocks of the stored validators to [`Config::BlockReporter`].
		///
		/// Sends at most [`Config::MaxEntriesPerReport`] entries per call to the reporter. Counts
		/// are removed only if the reporter accepts every chunk. Authors outside the stored set
		/// are dropped.
		pub(crate) fn report_blocks() -> Weight {
			let authors = AuthoredBlocks::<T>::count();
			let weight = T::WeightInfo::report_blocks(authors);
			if authors == 0 {
				return weight;
			}
			let validators = ValidatorSet::<T>::get().map(|set| set.validators);
			let chunk_size = T::MaxEntriesPerReport::get().max(1) as usize;
			let result = with_storage_layer(|| -> Result<u32, DispatchError> {
				let blocks = AuthoredBlocks::<T>::drain()
					.filter(|(who, _)| validators.as_ref().is_some_and(|set| set.contains(who)))
					.collect::<Vec<_>>();
				blocks.chunks(chunk_size).try_for_each(|chunk| {
					T::BlockReporter::report(chunk)
						.map_err(|()| DispatchError::Other("the reporter rejected the counts"))
				})?;
				Ok(blocks.len() as u32)
			});
			let event = match result {
				Ok(reported) => Event::BlocksReported { authors, reported },
				Err(_) => {
					log::warn!(
						target: crate::LOG_TARGET,
						"the reporter rejected the counts of {authors} authors, keeping them"
					);
					Event::BlocksReportFailed { authors }
				},
			};
			Self::deposit_event(event);
			weight
		}

		/// The Aura slot of the current block, from its pre-runtime digest.
		pub(crate) fn current_slot() -> Option<Slot> {
			frame_system::Pallet::<T>::digest()
				.convert_first(|item| item.pre_runtime_try_to::<Slot>(&AURA_ENGINE_ID))
		}

		/// Advance [`CountedSlots`] to `slot`, the slot of the current block, and return the count.
		///
		/// The first call stores the slot with a zero count. Later calls add the slots since the
		/// stored one, or one slot for a gap longer than the authority count, so a second call in
		/// the same block changes nothing.
		pub(crate) fn advance_counted_slots(slot: Option<Slot>) -> u64 {
			let stored = CountedSlots::<T>::get();
			let Some(slot) = slot else {
				return stored.map_or(0, |counted| counted.count);
			};
			let count = match stored {
				None => 0,
				Some(counted) if slot == counted.slot => return counted.count,
				// A longer slot duration lowers the slot numbers.
				Some(counted) if slot < counted.slot => counted.count,
				Some(counted) => {
					let gap = *slot - *counted.slot;
					let counted_gap = if gap <= T::AuthorityCount::get().into() { gap } else { 1 };
					counted.count.saturating_add(counted_gap)
				},
			};
			CountedSlots::<T>::put(SlotCount { slot, count });
			count
		}

		/// Record the activity of `candidates` and return those that are not dropped.
		///
		/// `activity` is [`LastActivity`] as stored before the rotation, `now` the [`CountedSlots`]
		/// count of the rotation and `rule` the [`CurrentDropRule`]. Under a rule, idle candidates
		/// are dropped into `dropped` within the floor.
		fn filter_idle(
			candidates: Vec<T::AccountId>,
			now: u64,
			mut rule: Option<DropRule>,
			activity: &BTreeMap<T::AccountId, Activity>,
			dropped: &mut DropRecords<T>,
		) -> Vec<T::AccountId> {
			// The list that ends at this rotation, since Aura writes the list this rotation enacts
			// only after the session managers ran.
			let ending = T::AuthorityCount::get();
			if rule.is_some() && ending == 0 {
				log::error!(
					target: crate::LOG_TARGET,
					"no Aura authorities are counted, dropping nobody"
				);
				rule = None;
			}
			let eligible: Vec<_> =
				candidates.into_iter().filter(|v| !dropped.contains_key(v)).collect();
			let mut idle = Vec::new();
			for v in &eligible {
				match activity.get(v) {
					None => LastActivity::<T>::insert(v, Activity::Queued),
					// Enacted by this rotation. The next one raises the count to the enacted list.
					Some(Activity::Queued) => LastActivity::<T>::insert(
						v,
						Activity::Active { since: now, authorities: 0 },
					),
					Some(&Activity::Active { since, authorities }) => {
						if ending > authorities {
							LastActivity::<T>::insert(
								v,
								Activity::Active { since, authorities: ending },
							);
						}
						let Some(rule) = rule else { continue };
						let window = rule.rounds.saturating_mul(authorities.max(ending));
						if now.saturating_sub(since) >= window.into() {
							idle.push((since, v.clone()));
						}
					},
				}
			}
			let Some(rule) = rule else { return eligible };
			idle.sort();
			let droppable = eligible.len().saturating_sub(rule.min_collators as usize);
			let kept = idle.len().saturating_sub(droppable) as u32;
			let newly_dropped: BTreeSet<_> = idle
				.into_iter()
				.take(droppable)
				.filter_map(|(_, v)| match dropped.try_insert(v.clone(), None) {
					Ok(_) => Some(v),
					Err(_) => {
						log::error!(
							target: crate::LOG_TARGET,
							"the drop records are full, returning {v:?} instead of dropping it"
						);
						None
					},
				})
				.collect();
			if !newly_dropped.is_empty() || kept > 0 {
				Self::deposit_event(Event::CollatorsDropped {
					validators: newly_dropped.iter().cloned().collect(),
					kept,
				});
			}
			eligible.into_iter().filter(|v| !newly_dropped.contains(v)).collect()
		}

		/// Check the pallet invariants.
		#[cfg(any(test, feature = "try-runtime"))]
		pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
			ensure!(
				ValidatorSet::<T>::exists() || PendingRotation::<T>::get() == RotationState::Idle,
				"a rotation is pending without a stored validator set"
			);
			ensure!(
				!ValidatorSet::<T>::exists() || ValidatorSet::<T>::get().is_some(),
				"the stored validator set does not decode, it may exceed `MaxValidators`"
			);
			ensure!(
				!Collators::<T>::exists() ||
					(MaxCollators::<T>::exists() && ValidatorSet::<T>::exists()),
				"drawn collators are stored without both a cap and a validator set"
			);
			ensure!(
				!Collators::<T>::exists() || Collators::<T>::get().is_some(),
				"the drawn collators do not decode"
			);
			// With the pallet wired as `OnSystemEvent`, `on_relay_state_proof` caches a value in
			// every block's inherent and the cache is never removed. So this holds for every draw
			// after the first block that runs the pallet, and flags a runtime without that wiring.
			ensure!(
				!Collators::<T>::exists() || EpochRandomness::<T>::exists(),
				"collators are drawn without a cached relay chain epoch randomness"
			);
			ensure!(
				AuthoredBlocks::<T>::iter_values().all(|blocks| blocks > 0),
				"an author is counted with zero blocks"
			);
			ensure!(ExpectedEraDuration::<T>::get() > 0, "the expected era duration is zero");
			ensure!(ReportMargin::<T>::get() > 0, "the report margin is zero");
			ensure!(
				!Dropped::<T>::exists() || Dropped::<T>::try_get().is_ok(),
				"the drop records do not decode"
			);
			let counted = CountedSlots::<T>::get().map_or(0, |counted| counted.count);
			ensure!(
				LastActivity::<T>::iter_values().all(|activity| match activity {
					Activity::Active { since, .. } => since <= counted,
					Activity::Queued => true,
				}),
				"an activity record is newer than the counted slots"
			);
			let dropped = Dropped::<T>::get();
			ensure!(
				dropped.keys().all(|v| !LastActivity::<T>::contains_key(v)),
				"a dropped validator has an activity record"
			);
			ensure!(
				CurrentDropRule::<T>::exists() || dropped.is_empty(),
				"validators are dropped without a drop rule"
			);
			// Until the first forced rotation the drawn collators may be missing or belong to a
			// previous set or cap, and the records may belong to validators the new set left out.
			if PendingRotation::<T>::get() == RotationState::AwaitingQueue {
				return Ok(());
			}
			let set = ValidatorSet::<T>::get();
			let in_set =
				|v: &T::AccountId| set.as_ref().is_some_and(|set| set.validators.contains(v));
			ensure!(
				LastActivity::<T>::iter_keys().all(|v| in_set(&v)),
				"an activity record belongs to a validator outside the stored set"
			);
			ensure!(
				dropped.keys().all(in_set),
				"a drop record belongs to a validator outside the stored set"
			);
			if let (Some(set), Some(cap)) = (ValidatorSet::<T>::get(), MaxCollators::<T>::get()) {
				let drawn = Collators::<T>::get().ok_or("no collators are drawn under a cap")?;
				ensure!(drawn.era == set.era, "the drawn collators belong to another era");
				let mut seen = BTreeSet::new();
				ensure!(
					drawn.validators.iter().all(|v| seen.insert(v)),
					"the drawn collators contain duplicates"
				);
				ensure!(
					drawn.validators.iter().all(|v| set.validators.contains(v)),
					"a drawn collator is not in the stored validator set"
				);
				ensure!(
					drawn.validators.len() <= cap as usize,
					"more collators are drawn than the cap allows"
				);
				ensure!(
					LastActivity::<T>::iter_keys().all(|v| drawn.validators.contains(&v)),
					"an activity record belongs to a validator that is not drawn"
				);
			}
			Ok(())
		}
	}

	impl<T: Config> SessionManager<T::AccountId> for Pallet<T> {
		fn new_session(_: SessionIndex) -> Option<Vec<T::AccountId>> {
			let pending = PendingRotation::<T>::get();
			match pending {
				RotationState::AwaitingQueue => {
					PendingRotation::<T>::put(RotationState::AwaitingEnactment)
				},
				RotationState::AwaitingEnactment => PendingRotation::<T>::kill(),
				RotationState::Idle => {},
			}
			let slot = Self::current_slot();
			let now = Self::advance_counted_slots(slot);
			let set = ValidatorSet::<T>::get();
			let stored_dropped = Dropped::<T>::get();
			let mut dropped = stored_dropped.clone();
			let rule = CurrentDropRule::<T>::get();
			if rule.is_some() && slot.is_none() {
				log::error!(
					target: crate::LOG_TARGET,
					"the block has no Aura slot, so the counted slots do not advance"
				);
			}
			dropped.retain(|v, until| {
				let Some(rule) = rule else { return false };
				if !set.as_ref().is_some_and(|set| set.validators.contains(v)) {
					return false;
				}
				match until {
					Some(until) => *until > now,
					// Dropped at the previous rotation, so out of the list this rotation enacts.
					None => {
						*until = Some(now.saturating_add(rule.slots));
						true
					},
				}
			});
			let activity: BTreeMap<_, _> = LastActivity::<T>::iter().collect();
			let returned = match set {
				Some(set) => {
					// Drawn at the rotation, which pallet-session weighs as the whole block, since
					// the key reads may not fit the weight of the message or report that delivers
					// the set.
					let validators = match MaxCollators::<T>::get() {
						Some(cap) if pending == RotationState::AwaitingQueue => {
							Self::draw_collators(set, cap, &dropped).into_inner()
						},
						Some(_) => Collators::<T>::get()
							.map(|drawn| drawn.validators.into_inner())
							.unwrap_or_default(),
						None => set.validators.into_iter().collect(),
					};
					let candidates = validators
						.into_iter()
						.filter(T::ValidatorRegistration::is_registered)
						.collect();
					Some(Self::filter_idle(candidates, now, rule, &activity, &mut dropped))
				},
				None => {
					if ValidatorSet::<T>::exists() {
						log::error!(
							target: crate::LOG_TARGET,
							"the stored validator set does not decode"
						);
						Self::deposit_event(Event::StoredSetUndecodable);
					}
					None
				},
			};
			let returned_set: BTreeSet<_> = returned.iter().flatten().collect();
			activity
				.keys()
				.filter(|v| !returned_set.contains(v))
				.for_each(LastActivity::<T>::remove);
			if dropped != stored_dropped {
				if dropped.is_empty() {
					Dropped::<T>::kill();
				} else {
					Dropped::<T>::put(dropped);
				}
			}
			returned
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
		/// Counts the block for `author` in [`AuthoredBlocks`] and refreshes its [`LastActivity`]
		/// record if it is an active collator of this pallet.
		///
		/// Never creates a record, so a block of a validator that is no longer returned does not
		/// count as activity when it is returned again.
		///
		/// Advances [`CountedSlots`] first, since the authorship handler can run before this
		/// pallet's hooks.
		fn note_author(author: T::AccountId) {
			frame_system::Pallet::<T>::register_extra_weight_unchecked(
				T::WeightInfo::note_author(),
				DispatchClass::Mandatory,
			);
			let since = Self::advance_counted_slots(Self::current_slot());
			AuthoredBlocks::<T>::mutate(&author, |blocks| blocks.saturating_inc());
			if let Some(Activity::Active { .. }) = LastActivity::<T>::get(&author) {
				LastActivity::<T>::insert(author, Activity::Active { since, authorities: 0 });
			}
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
