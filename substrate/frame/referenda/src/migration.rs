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

//! Storage migrations for the referenda pallet.

use super::*;
use codec::{Decode, Encode, EncodeLike, MaxEncodedLen};
use frame_support::{pallet_prelude::*, storage_alias, traits::OnRuntimeUpgrade};
use log;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

type SystemBlockNumberFor<T> = frame_system::pallet_prelude::BlockNumberFor<T>;

/// Initial version of storage types.
pub mod v0 {
	use super::*;

	#[derive(
		Encode,
		Decode,
		DecodeWithMemTracking,
		Clone,
		PartialEq,
		Eq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct ReferendumStatus<
		TrackId: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		RuntimeOrigin: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		Moment: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone + EncodeLike,
		Call: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		Balance: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		Tally: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		AccountId: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		ScheduleAddress: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
	> {
		/// The track of this referendum.
		pub track: TrackId,
		/// The origin for this referendum.
		pub origin: RuntimeOrigin,
		/// The hash of the proposal up for referendum.
		pub proposal: Call,
		/// The time the proposal should be scheduled for enactment.
		pub enactment: DispatchTime<Moment>,
		/// The time of submission. Once `UndecidingTimeout` passes, it may be closed by anyone if
		/// `deciding` is `None`.
		pub submitted: Moment,
		/// The deposit reserved for the submission of this referendum.
		pub submission_deposit: Deposit<AccountId, Balance>,
		/// The deposit reserved for this referendum to be decided.
		pub decision_deposit: Option<Deposit<AccountId, Balance>>,
		/// The status of a decision being made. If `None`, it has not entered the deciding period.
		pub deciding: Option<DecidingStatus<Moment>>,
		/// The current tally of votes in this referendum.
		pub tally: Tally,
		/// Whether we have been placed in the queue for being decided or not.
		pub in_queue: bool,
		/// The next scheduled wake-up, if `Some`.
		pub alarm: Option<(Moment, ScheduleAddress)>,
	}

	pub type ReferendumStatusOf<T, I> = ReferendumStatus<
		TrackIdOf<T, I>,
		PalletsOriginOf<T>,
		BlockNumberFor<T, I>,
		BoundedCallOf<T, I>,
		BalanceOf<T, I>,
		TallyOf<T, I>,
		<T as frame_system::Config>::AccountId,
		ScheduleAddressOf<T, I>,
	>;

	pub type ReferendumInfoOf<T, I> = ReferendumInfo<
		TrackIdOf<T, I>,
		PalletsOriginOf<T>,
		SystemBlockNumberFor<T>,
		BoundedCallOf<T, I>,
		BalanceOf<T, I>,
		TallyOf<T, I>,
		<T as frame_system::Config>::AccountId,
		ScheduleAddressOf<T, I>,
	>;

	/// Info regarding a referendum, present or past.
	#[derive(
		Encode,
		Decode,
		Clone,
		PartialEq,
		Eq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
		DecodeWithMemTracking,
	)]
	pub enum ReferendumInfo<
		TrackId: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		RuntimeOrigin: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		Moment: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone + EncodeLike,
		Call: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		Balance: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		Tally: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		AccountId: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		ScheduleAddress: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
	> {
		/// Referendum has been submitted and is being voted on.
		Ongoing(
			ReferendumStatus<
				TrackId,
				RuntimeOrigin,
				Moment,
				Call,
				Balance,
				Tally,
				AccountId,
				ScheduleAddress,
			>,
		),
		/// Referendum finished with approval. Submission deposit is held.
		Approved(Moment, Deposit<AccountId, Balance>, Option<Deposit<AccountId, Balance>>),
		/// Referendum finished with rejection. Submission deposit is held.
		Rejected(Moment, Deposit<AccountId, Balance>, Option<Deposit<AccountId, Balance>>),
		/// Referendum finished with cancellation. Submission deposit is held.
		Cancelled(Moment, Deposit<AccountId, Balance>, Option<Deposit<AccountId, Balance>>),
		/// Referendum finished and was never decided. Submission deposit is held.
		TimedOut(Moment, Deposit<AccountId, Balance>, Option<Deposit<AccountId, Balance>>),
		/// Referendum finished with a kill.
		Killed(Moment),
	}

	#[storage_alias]
	pub type ReferendumInfoFor<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Blake2_128Concat, ReferendumIndex, ReferendumInfoOf<T, I>>;
}

pub mod v1 {
	use super::*;

	/// The log target.
	const TARGET: &'static str = "runtime::referenda::migration::v1";

	// okay.. this really wouldn't be an issue but for some reason they want to do the block migration without
	// updating the storage version. But basically it goes v0 -> v1 -> v1::NewBlockType
	// I guess the question is can you treat v1 and v1::NewBlockType as the same? Is that why they did this?
	// Or do I just need to get the actual struct data from when these things were written?
	// And I suppose even if I am able to get that to work. Which seems possible considering they'll be similar
	// do I still need to go in and make sure that the structs are correct for the old migrations? I think so
	// because what I did was pull them from right before these changes, and there's been two migrations
	// well I guess not really. Because the second migration was just. No definitely v0 -> v1 was different
	// and I should reflect that or make sure it's reflected I guess.
	// Okay so v0 and v1 have the correct types
	// Well.. so then it went from v1 to v1::NewBlockType, which didn't actually change any fields on
	// the struct but more just said it may be this new type. But if you say 'may be this new type' the compiler
	// is going to treat it like they're not defacto compatible. So when I try and put this v1::ref into v1::ref
	// again, but this time with the possibly new type. It complains. I suppose I see why they didn't want
	// to increment the storage version, but that seems to have only made it more complicated unnecessarily.
	// like I don't see what we're getting out of this

	#[derive(
		Encode,
		Decode,
		DecodeWithMemTracking,
		Clone,
		PartialEq,
		Eq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct ReferendumStatus<
		TrackId: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		RuntimeOrigin: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		Moment: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone + EncodeLike,
		Call: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		Balance: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		Tally: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		AccountId: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		ScheduleAddress: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
	> {
		/// The track of this referendum.
		pub track: TrackId,
		/// The origin for this referendum.
		pub origin: RuntimeOrigin,
		/// The hash of the proposal up for referendum.
		pub proposal: Call,
		/// The time the proposal should be scheduled for enactment.
		pub enactment: DispatchTime<Moment>,
		/// The time of submission. Once `UndecidingTimeout` passes, it may be closed by anyone if
		/// `deciding` is `None`.
		pub submitted: Moment,
		/// The deposit reserved for the submission of this referendum.
		pub submission_deposit: Deposit<AccountId, Balance>,
		/// The deposit reserved for this referendum to be decided.
		pub decision_deposit: Option<Deposit<AccountId, Balance>>,
		/// The status of a decision being made. If `None`, it has not entered the deciding period.
		pub deciding: Option<DecidingStatus<Moment>>,
		/// The current tally of votes in this referendum.
		pub tally: Tally,
		/// Whether we have been placed in the queue for being decided or not.
		pub in_queue: bool,
		/// The next scheduled wake-up, if `Some`.
		pub alarm: Option<(Moment, ScheduleAddress)>,
	}

	#[derive(
		Encode,
		Decode,
		DecodeWithMemTracking,
		Clone,
		PartialEq,
		Eq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]	
	pub enum ReferendumInfo<
		TrackId: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		RuntimeOrigin: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		Moment: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone + EncodeLike,
		Call: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		Balance: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		Tally: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		AccountId: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
		ScheduleAddress: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone,
	> {
		/// Referendum has been submitted and is being voted on.
		Ongoing(
			ReferendumStatus<
				TrackId,
				RuntimeOrigin,
				Moment,
				Call,
				Balance,
				Tally,
				AccountId,
				ScheduleAddress,
			>,
		),
		/// Referendum finished with approval. Submission deposit is held.
		Approved(Moment, Option<Deposit<AccountId, Balance>>, Option<Deposit<AccountId, Balance>>),
		/// Referendum finished with rejection. Submission deposit is held.
		Rejected(Moment, Option<Deposit<AccountId, Balance>>, Option<Deposit<AccountId, Balance>>),
		/// Referendum finished with cancellation. Submission deposit is held.
		Cancelled(Moment, Option<Deposit<AccountId, Balance>>, Option<Deposit<AccountId, Balance>>),
		/// Referendum finished and was never decided. Submission deposit is held.
		TimedOut(Moment, Option<Deposit<AccountId, Balance>>, Option<Deposit<AccountId, Balance>>),
		/// Referendum finished with a kill.
		Killed(Moment),
	}

	pub(crate) type ReferendumInfoOf<T, I> = ReferendumInfo<
		TrackIdOf<T, I>,
		PalletsOriginOf<T>,
		SystemBlockNumberFor<T>,
		BoundedCallOf<T, I>,
		BalanceOf<T, I>,
		TallyOf<T, I>,
		<T as frame_system::Config>::AccountId,
		ScheduleAddressOf<T, I>,
	>;

	#[storage_alias]
	pub type ReferendumInfoFor<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Blake2_128Concat, ReferendumIndex, ReferendumInfoOf<T, I>>;

	/// Transforms a submission deposit of ReferendumInfo(Approved|Rejected|Cancelled|TimedOut) to
	/// optional value, making it refundable.
	pub struct MigrateV0ToV1<T, I = ()>(PhantomData<(T, I)>);
	impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for MigrateV0ToV1<T, I> {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			let referendum_count = v0::ReferendumInfoFor::<T, I>::iter().count();
			log::info!(
				target: TARGET,
				"pre-upgrade state contains '{}' referendums.",
				referendum_count
			);
			Ok((referendum_count as u32).encode())
		}

		fn on_runtime_upgrade() -> Weight {
			let in_code_version = Pallet::<T, I>::in_code_storage_version();
			let on_chain_version = Pallet::<T, I>::on_chain_storage_version();
			let mut weight = T::DbWeight::get().reads(1);
			log::info!(
				target: TARGET,
				"running migration with in-code storage version {:?} / onchain {:?}.",
				in_code_version,
				on_chain_version
			);
			if on_chain_version != 0 {
				log::warn!(target: TARGET, "skipping migration from v0 to v1.");
				return weight
			}
			v0::ReferendumInfoFor::<T, I>::iter().for_each(|(key, value)| {
				let maybe_new_value = match value {
					v0::ReferendumInfo::Ongoing(_) | v0::ReferendumInfo::Killed(_) => None,
					v0::ReferendumInfo::Approved(e, s, d) =>
						Some(ReferendumInfo::Approved(e, Some(s), d)),
					v0::ReferendumInfo::Rejected(e, s, d) =>
						Some(ReferendumInfo::Rejected(e, Some(s), d)),
					v0::ReferendumInfo::Cancelled(e, s, d) =>
						Some(ReferendumInfo::Cancelled(e, Some(s), d)),
					v0::ReferendumInfo::TimedOut(e, s, d) =>
						Some(ReferendumInfo::TimedOut(e, Some(s), d)),
				};
				if let Some(new_value) = maybe_new_value {
					weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
					log::info!(target: TARGET, "migrating referendum #{:?}", &key);
					v1::ReferendumInfoFor::<T, I>::insert(key, new_value);
				} else {
					weight.saturating_accrue(T::DbWeight::get().reads(1));
				}
			});
			StorageVersion::new(1).put::<Pallet<T, I>>();
			weight.saturating_accrue(T::DbWeight::get().writes(1));
			weight
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			let on_chain_version = Pallet::<T, I>::on_chain_storage_version();
			ensure!(on_chain_version == 1, "must upgrade from version 0 to 1.");
			let pre_referendum_count: u32 = Decode::decode(&mut &state[..])
				.expect("failed to decode the state from pre-upgrade.");
			let post_referendum_count = ReferendumInfoFor::<T, I>::iter().count() as u32;
			ensure!(post_referendum_count == pre_referendum_count, "must migrate all referendums.");
			log::info!(target: TARGET, "migrated all referendums.");
			Ok(())
		}
	}
}

/// Migration for when changing the block number provider.
///
/// This migration is not guarded
pub mod switch_block_number_provider {
	use super::*;

	/// The log target.
	const TARGET: &'static str = "runtime::referenda::migration::change_block_number_provider";

	pub(crate) type ReferendumInfoOf<T, I> = v1::ReferendumInfo<
		TrackIdOf<T, I>,
		PalletsOriginOf<T>,
		BlockNumberFor<T, I>,
		BoundedCallOf<T, I>,
		BalanceOf<T, I>,
		TallyOf<T, I>,
		<T as frame_system::Config>::AccountId,
		ScheduleAddressOf<T, I>,
	>;

	#[storage_alias]
	pub type ReferendumInfoFor<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Blake2_128Concat, ReferendumIndex, ReferendumInfoOf<T, I>>;

	/// Convert from one to another block number provider/type.
	pub trait BlockNumberConversion<Old, New> {
		/// Convert the `old` block number type to the new block number type.
		///
		/// Any changes in the rate of blocks need to be taken into account.
		fn convert_block_number(block_number: Old) -> New;
	}

	/// Transforms `SystemBlockNumberFor<T>` to `BlockNumberFor<T,I>`
	pub struct MigrateBlockNumberProvider<BlockConverter, T, I = ()>(
		PhantomData<(T, I)>,
		PhantomData<BlockConverter>,
	);
	impl<BlockConverter: BlockNumberConversion<T, I>, T: Config<I>, I: 'static> OnRuntimeUpgrade
		for MigrateBlockNumberProvider<BlockConverter, T, I>
	where
		BlockConverter: BlockNumberConversion<SystemBlockNumberFor<T>, BlockNumberFor<T, I>>,
		T: Config<I>,
	{
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			let referendum_count = v1::ReferendumInfoFor::<T, I>::iter().count();
			log::info!(
				target: TARGET,
				"pre-upgrade state contains '{}' referendums.",
				referendum_count
			);
			Ok((referendum_count as u32).encode())
		}

		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();
			weight.saturating_accrue(migrate_block_number_provider::<BlockConverter, T, I>());
			weight
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			let on_chain_version = Pallet::<T, I>::on_chain_storage_version();
			ensure!(on_chain_version == 1, "must upgrade from version 1 to 2.");
			let pre_referendum_count: u32 = Decode::decode(&mut &state[..])
				.expect("failed to decode the state from pre-upgrade.");
			let post_referendum_count = ReferendumInfoFor::<T, I>::iter().count() as u32;
			ensure!(post_referendum_count == pre_referendum_count, "must migrate all referendums.");
			log::info!(target: TARGET, "migrated all referendums.");
			Ok(())
		}
	}

	pub fn migrate_block_number_provider<BlockConverter, T, I: 'static>() -> Weight
	where
		BlockConverter: BlockNumberConversion<SystemBlockNumberFor<T>, BlockNumberFor<T, I>>,
		T: Config<I>,
	{
		let in_code_version = Pallet::<T, I>::in_code_storage_version();
		let on_chain_version = Pallet::<T, I>::on_chain_storage_version();
		let mut weight = T::DbWeight::get().reads(1);
		log::info!(
			target: "runtime::referenda::migration::change_block_number_provider",
			"running migration with in-code storage version {:?} / onchain {:?}.",
			in_code_version,
			on_chain_version
		);
		if on_chain_version == 0 {
			log::error!(target: TARGET, "skipping migration from v0 to switch_block_number_provider.");
			return weight
		}

		// Migration logic here
		v1::ReferendumInfoFor::<T, I>::iter().for_each(|(key, value)| {
			let maybe_new_value = match value {
				v1::ReferendumInfo::Ongoing(_) | v1::ReferendumInfo::Killed(_) => None,
				v1::ReferendumInfo::Approved(e, s, d) => {
					let new_e = BlockConverter::convert_block_number(e);
					Some(v1::ReferendumInfo::Approved(new_e, s, d))
				},
				v1::ReferendumInfo::Rejected(e, s, d) => {
					let new_e = BlockConverter::convert_block_number(e);
					Some(v1::ReferendumInfo::Rejected(new_e, s, d))
				},
				v1::ReferendumInfo::Cancelled(e, s, d) => {
					let new_e = BlockConverter::convert_block_number(e);
					Some(v1::ReferendumInfo::Cancelled(new_e, s, d))
				},
				v1::ReferendumInfo::TimedOut(e, s, d) => {
					let new_e = BlockConverter::convert_block_number(e);
					Some(v1::ReferendumInfo::TimedOut(new_e, s, d))
				},
			};
			if let Some(new_value) = maybe_new_value {
				weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
				log::info!(target: TARGET, "migrating referendum #{:?}", &key);
				switch_block_number_provider::ReferendumInfoFor::<T, I>::insert(key, new_value);
			} else {
				weight.saturating_accrue(T::DbWeight::get().reads(1));
			}
		});

		weight
	}
}

#[cfg(test)]
pub mod test {
	use super::*;
	use crate::{
		migration::switch_block_number_provider::{
			migrate_block_number_provider, BlockNumberConversion,
		},
		mock::{Test as T, *},
	};
	use core::str::FromStr;

	// create referendum status v0.
	fn create_status_v0() -> v0::ReferendumStatusOf<T, ()> {
		let origin: OriginCaller = frame_system::RawOrigin::Root.into();
		let track = <T as Config<()>>::Tracks::track_for(&origin).unwrap();
		v0::ReferendumStatusOf::<T, ()> {
			track,
			in_queue: true,
			origin,
			proposal: set_balance_proposal_bounded(1),
			enactment: DispatchTime::At(1),
			tally: TallyOf::<T, ()>::new(track),
			submission_deposit: Deposit { who: 1, amount: 10 },
			submitted: 1,
			decision_deposit: None,
			alarm: None,
			deciding: None,
		}
	}
	#[test]
	pub fn referendum_status_v0() {
		// make sure the bytes of the encoded referendum v0 is decodable.
		let ongoing_encoded = sp_core::Bytes::from_str("0x00000000012c01082a0000000000000004000100000000000000010000000000000001000000000000000a00000000000000000000000000000000000100").unwrap();
		let ongoing_dec = v0::ReferendumInfoOf::<T, ()>::decode(&mut &*ongoing_encoded).unwrap();
		let ongoing = v0::ReferendumInfoOf::<T, ()>::Ongoing(create_status_v0());
		assert_eq!(ongoing, ongoing_dec);
	}

	#[test]
	fn migration_v0_to_v1_works() {
		ExtBuilder::default().build_and_execute(|| {
			// create and insert into the storage an ongoing referendum v0.
			let status_v0 = create_status_v0();
			let ongoing_v0 = v0::ReferendumInfoOf::<T, ()>::Ongoing(status_v0.clone());
			ReferendumCount::<T, ()>::mutate(|x| x.saturating_inc());
			v0::ReferendumInfoFor::<T, ()>::insert(2, ongoing_v0);
			// create and insert into the storage an approved referendum v0.
			let approved_v0 = v0::ReferendumInfoOf::<T, ()>::Approved(
				123,
				Deposit { who: 1, amount: 10 },
				Some(Deposit { who: 2, amount: 20 }),
			);
			ReferendumCount::<T, ()>::mutate(|x| x.saturating_inc());
			v0::ReferendumInfoFor::<T, ()>::insert(5, approved_v0);
			// run migration from v0 to v1.
			v1::MigrateV0ToV1::<T, ()>::on_runtime_upgrade();
			// fetch and assert migrated into v1 the ongoing referendum.
			let ongoing_v1 = v1::ReferendumInfoFor::<T, ()>::get(2).unwrap();
			// referendum status schema is the same for v0 and v1.
			assert_eq!(v1::ReferendumInfoOf::<T, ()>::Ongoing(status_v0), ongoing_v1);
			// fetch and assert migrated into v1 the approved referendum.
			let approved_v1 = v1::ReferendumInfoFor::<T, ()>::get(5).unwrap();
			assert_eq!(
				approved_v1,
				v1::ReferendumInfoOf::<T, ()>::Approved(
					123,
					Some(Deposit { who: 1, amount: 10 }),
					Some(Deposit { who: 2, amount: 20 }),
				)
			);
		});
	}

	#[test]
	fn migration_v1_to_switch_block_number_provider_works() {
		ExtBuilder::default().build_and_execute(|| {
			pub struct MockBlockConverter;

			impl BlockNumberConversion<SystemBlockNumberFor<T>, BlockNumberFor<T, ()>> for MockBlockConverter {
				fn convert_block_number(block_number: SystemBlockNumberFor<T>) -> BlockNumberFor<T, ()> {
					block_number as u64 + 10u64
				}
			}

			let referendum_ongoing = v1::ReferendumInfoOf::<T, ()>::Ongoing(create_status_v0());
			let referendum_approved = v1::ReferendumInfoOf::<T, ()>::Approved(
				50, //old block number
				Some(Deposit { who: 1, amount: 10 }),
				Some(Deposit { who: 2, amount: 20 }),
			);

			ReferendumCount::<T, ()>::mutate(|x| x.saturating_inc());
			v1::ReferendumInfoFor::<T, ()>::insert(1, referendum_ongoing);

			ReferendumCount::<T, ()>::mutate(|x| x.saturating_inc());
			v1::ReferendumInfoFor::<T, ()>::insert(2, referendum_approved);

			migrate_block_number_provider::<MockBlockConverter, T, ()>();

			let ongoing_v2 = ReferendumInfoFor::<T, ()>::get(1).unwrap();
			assert_eq!(
				ongoing_v2,
				ReferendumInfoOf::<T, ()>::Ongoing(create_status_v0())
			);

			let approved_v2 = ReferendumInfoFor::<T, ()>::get(2).unwrap();
			assert_eq!(
				approved_v2,
				ReferendumInfoOf::<T, ()>::Approved(
					50,
					Some(Deposit { who: 1, amount: 10 }),
					Some(Deposit { who: 2, amount: 20 })
				)
			);
		});
	}
}
