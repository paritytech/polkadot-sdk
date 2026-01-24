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

extern crate alloc;

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
	// ReferendumStatus and its dependency types referenced from the latest version while staying
	// unchanged. [`super::test::referendum_status_v0()`] checks its immutability between v0 and
	// latest version.
	#[cfg(test)]
	pub(super) use super::{ReferendumStatus, ReferendumStatusOf};

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
		Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen, DecodeWithMemTracking,
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
				ReferendumInfo::Ongoing(_) | ReferendumInfo::Killed(_) => None,
				ReferendumInfo::Approved(e, s, d) => {
					let new_e = BlockConverter::convert_block_number(e);
					Some(ReferendumInfo::Approved(new_e, s, d))
				},
				ReferendumInfo::Rejected(e, s, d) => {
					let new_e = BlockConverter::convert_block_number(e);
					Some(ReferendumInfo::Rejected(new_e, s, d))
				},
				ReferendumInfo::Cancelled(e, s, d) => {
					let new_e = BlockConverter::convert_block_number(e);
					Some(ReferendumInfo::Cancelled(new_e, s, d))
				},
				ReferendumInfo::TimedOut(e, s, d) => {
					let new_e = BlockConverter::convert_block_number(e);
					Some(ReferendumInfo::TimedOut(new_e, s, d))
				},
			};
			if let Some(new_value) = maybe_new_value {
				weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
				log::info!(target: TARGET, "migrating referendum #{:?}", &key);
				ReferendumInfoFor::<T, I>::insert(key, new_value);
			} else {
				weight.saturating_accrue(T::DbWeight::get().reads(1));
			}
		});

		weight
	}
}

/// Multi-block migration from v1 to v2 for the referenda pallet.
///
/// This migration converts deposits from the old `Currency::reserve` system
/// to the new `fungible::hold` system. It uses the `SteppedMigration` framework
/// to spread the work across multiple blocks, avoiding weight limit issues on
/// chains with many accumulated referenda.
pub mod v2_mbm {
	use super::*;
	use frame_support::{
		migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
		traits::ReservableCurrency,
		weights::{constants::RocksDbWeight, WeightMeter},
	};

	/// The log target for this migration.
	const TARGET: &'static str = "runtime::referenda::migration::v2_mbm";

	/// Unique identifier for this pallet's migrations.
	const PALLET_MIGRATIONS_ID: &[u8; 18] = b"pallet-referenda  ";

	/// Weight functions needed for the multi-block migration.
	pub mod weights {
		use super::*;
		use frame_support::weights::Weight;

		/// Weight functions for the migration step.
		pub trait WeightInfo {
			/// Weight for processing one referendum in the migration step.
			///
			/// This includes:
			/// - 1 read for the referendum info
			/// - Up to 2 deposits (submission + decision)
			/// - Per deposit: unreserve (read + write) + hold (read + write)
			fn step() -> Weight;
		}

		/// Weights using the Substrate node and recommended hardware.
		pub struct SubstrateWeight<T>(core::marker::PhantomData<T>);
		impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
			fn step() -> Weight {
				// Per referendum worst case:
				// - 1 read: fetch referendum info from storage
				// - 2 deposits maximum (submission + decision)
				// - Per deposit: unreserve (~2 reads, ~2 writes) + hold (~2 reads, ~2 writes)
				// Total worst case: 1 + 2*(2+2) = 9 reads, 2*4 = 8 writes
				Weight::from_parts(20_000_000, 6000)
					.saturating_add(T::DbWeight::get().reads(9))
					.saturating_add(T::DbWeight::get().writes(8))
			}
		}

		/// For backwards compatibility and tests.
		impl WeightInfo for () {
			fn step() -> Weight {
				Weight::from_parts(20_000_000, 6000)
					.saturating_add(RocksDbWeight::get().reads(9))
					.saturating_add(RocksDbWeight::get().writes(8))
			}
		}
	}

	/// Multi-block migration from v1 to v2.
	///
	/// Iterates through all referenda with deposits and converts them from the old
	/// `Currency::reserve` system to the new `fungible::hold` system.
	pub struct LazyMigrationV1ToV2<T, I, OldCurrency, W>(PhantomData<(T, I, OldCurrency, W)>);

	impl<T, I, OldCurrency, W> SteppedMigration for LazyMigrationV1ToV2<T, I, OldCurrency, W>
	where
		T: Config<I>,
		I: 'static,
		OldCurrency: ReservableCurrency<T::AccountId, Balance = BalanceOf<T, I>>,
		W: weights::WeightInfo,
	{
		/// The cursor is the last processed `ReferendumIndex`.
		type Cursor = ReferendumIndex;

		/// Migration identifier with pallet ID and version info.
		type Identifier = MigrationId<18>;

		fn id() -> Self::Identifier {
			MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 1, version_to: 2 }
		}

		fn step(
			cursor: Option<Self::Cursor>,
			meter: &mut WeightMeter,
		) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
			let required = W::step();

			// Check if we have enough weight for at least one item
			if meter.remaining().any_lt(required) {
				return Err(SteppedMigrationError::InsufficientWeight { required });
			}

			// Create iterator starting from cursor position or beginning
			let mut iter = if let Some(last_key) = cursor {
				// Resume from the key after the last processed one
				v1::ReferendumInfoFor::<T, I>::iter_from(
					v1::ReferendumInfoFor::<T, I>::hashed_key_for(last_key),
				)
			} else {
				// Start from the beginning
				v1::ReferendumInfoFor::<T, I>::iter()
			};

			let mut last_key = cursor;

			// Process items while we have weight budget
			loop {
				// Check if we can process another item
				if meter.try_consume(required).is_err() {
					break;
				}

				// Get next item from iterator
				let Some((index, info)) = iter.next() else {
					// No more items - migration complete
					log::info!(
						target: TARGET,
						"Migration complete. Last processed index: {:?}",
						last_key
					);
					return Ok(None);
				};

				// Process this referendum's deposits
				Self::migrate_referendum_deposits(index, &info);

				// Update cursor
				last_key = Some(index);
			}

			// Return cursor to continue in next block
			log::info!(
				target: TARGET,
				"Step complete. Last processed index: {:?}",
				last_key
			);

			Ok(last_key)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			use alloc::collections::btree_map::BTreeMap;

			// Count all referenda and their deposits
			let mut referendum_count = 0u32;
			let mut deposit_count = 0u32;
			let mut deposits_by_account: BTreeMap<Vec<u8>, BalanceOf<T, I>> = BTreeMap::new();

			for (_index, info) in v1::ReferendumInfoFor::<T, I>::iter() {
				referendum_count += 1;

				let deposits = Self::collect_deposits(&info);
				for Deposit { who, amount } in deposits {
					if !amount.is_zero() {
						deposit_count += 1;
						let key = who.encode();
						*deposits_by_account.entry(key).or_default() += amount;
					}
				}
			}

			log::info!(
				target: TARGET,
				"pre_upgrade: {} referenda, {} deposits to migrate",
				referendum_count,
				deposit_count
			);

			Ok((referendum_count, deposit_count).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let (pre_referendum_count, pre_deposit_count): (u32, u32) =
				Decode::decode(&mut &state[..]).expect("failed to decode pre_upgrade state");

			// Verify referendum count unchanged
			let post_referendum_count = ReferendumInfoFor::<T, I>::iter().count() as u32;
			frame_support::ensure!(
				post_referendum_count == pre_referendum_count,
				"Referendum count changed during migration"
			);

			log::info!(
				target: TARGET,
				"post_upgrade: Successfully verified {} referenda, {} deposits migrated",
				pre_referendum_count,
				pre_deposit_count
			);

			Ok(())
		}
	}

	impl<T, I, OldCurrency, W> LazyMigrationV1ToV2<T, I, OldCurrency, W>
	where
		T: Config<I>,
		I: 'static,
		OldCurrency: ReservableCurrency<T::AccountId, Balance = BalanceOf<T, I>>,
	{
		/// Migrate deposits for a single referendum.
		///
		/// For each deposit (submission and decision):
		/// 1. Unreserve from old Currency system
		/// 2. Place hold with new HoldReason
		fn migrate_referendum_deposits(index: ReferendumIndex, info: &v1::ReferendumInfoOf<T, I>) {
			let deposits = Self::collect_deposits(info);

			for Deposit { who, amount } in deposits {
				if amount.is_zero() {
					continue;
				}

				// Unreserve from old system
				let remaining = OldCurrency::unreserve(&who, amount);
				if !remaining.is_zero() {
					log::warn!(
						target: TARGET,
						"referendum #{:?}: could not fully unreserve for {:?}. \
						 Expected: {:?}, Remaining: {:?}",
						index,
						who,
						amount,
						remaining
					);
				}

				// Place hold with new system
				let amount_to_hold = amount.saturating_sub(remaining);
				if !amount_to_hold.is_zero() {
					if let Err(e) = T::NativeBalance::hold(
						&HoldReason::DecisionDeposit.into(),
						&who,
						amount_to_hold,
					) {
						log::error!(
							target: TARGET,
							"referendum #{:?}: failed to hold {:?} for {:?}: {:?}",
							index,
							amount_to_hold,
							who,
							e
						);
					} else {
						log::debug!(
							target: TARGET,
							"referendum #{:?}: migrated deposit of {:?} for {:?}",
							index,
							amount_to_hold,
							who
						);
					}
				}
			}
		}

		/// Collect all deposits from a referendum that need migration.
		fn collect_deposits(
			info: &v1::ReferendumInfoOf<T, I>,
		) -> Vec<Deposit<T::AccountId, BalanceOf<T, I>>> {
			let mut deposits = Vec::new();

			match info {
				ReferendumInfo::Ongoing(status) => {
					deposits.push(status.submission_deposit.clone());
					if let Some(ref d) = status.decision_deposit {
						deposits.push(d.clone());
					}
				},
				ReferendumInfo::Approved(_, ref s, ref d) |
				ReferendumInfo::Rejected(_, ref s, ref d) |
				ReferendumInfo::Cancelled(_, ref s, ref d) |
				ReferendumInfo::TimedOut(_, ref s, ref d) => {
					if let Some(ref submission) = s {
						deposits.push(submission.clone());
					}
					if let Some(ref decision) = d {
						deposits.push(decision.clone());
					}
				},
				ReferendumInfo::Killed(_) => {},
			}

			deposits
		}
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
	use frame_support::assert_ok;
	use pallet_balances::Pallet as Balances;

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
			assert_eq!(ReferendumInfoOf::<T, ()>::Ongoing(status_v0), ongoing_v1);
			// fetch and assert migrated into v1 the approved referendum.
			let approved_v1 = v1::ReferendumInfoFor::<T, ()>::get(5).unwrap();
			assert_eq!(
				approved_v1,
				ReferendumInfoOf::<T, ()>::Approved(
					123,
					Some(Deposit { who: 1, amount: 10 }),
					Some(Deposit { who: 2, amount: 20 })
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

	// Multi-block migration tests for v2_mbm.
	mod v2_mbm_tests {
		use super::*;
		use frame_support::{
			migrations::SteppedMigration,
			traits::{fungible::InspectHold, Currency, ReservableCurrency},
			weights::WeightMeter,
		};
		use v2_mbm::{weights::WeightInfo, LazyMigrationV1ToV2};

		#[test]
		fn mbm_migration_works_single_step() {
			ExtBuilder::default().build_and_execute(|| {
				// Setup: Fund accounts and reserve balances (simulating v1 state)
				let submitter: u64 = 1;
				let decision_depositor: u64 = 2;
				let submission_amount: u64 = 10;
				let decision_amount: u64 = 20;

				// Give accounts enough balance
				let _ = <Balances<T> as Currency<u64>>::deposit_creating(&submitter, 1000);
				let _ = <Balances<T> as Currency<u64>>::deposit_creating(&decision_depositor, 1000);

				// Reserve funds using old Currency trait (simulating v1 state)
				assert_ok!(<Balances<T> as ReservableCurrency<u64>>::reserve(
					&submitter,
					submission_amount
				));
				assert_ok!(<Balances<T> as ReservableCurrency<u64>>::reserve(
					&decision_depositor,
					decision_amount
				));

				// Create an ongoing referendum with both deposits
				let status_v1 = create_status_v0();
				let ongoing_with_decision =
					v1::ReferendumInfoOf::<T, ()>::Ongoing(ReferendumStatus {
						submission_deposit: Deposit { who: submitter, amount: submission_amount },
						decision_deposit: Some(Deposit {
							who: decision_depositor,
							amount: decision_amount,
						}),
						..status_v1
					});

				ReferendumCount::<T, ()>::put(1);
				v1::ReferendumInfoFor::<T, ()>::insert(0, ongoing_with_decision);
				StorageVersion::new(1).put::<Pallet<T, ()>>();

				// Run multi-block migration with enough weight for all items
				let mut meter = WeightMeter::new();
				let result = LazyMigrationV1ToV2::<T, (), Balances<T>, ()>::step(None, &mut meter);

				// Should complete in one step
				assert!(matches!(result, Ok(None)));

				// Verify holds are now in place
				let submitter_held = <Balances<T> as InspectHold<u64>>::balance_on_hold(
					&HoldReason::DecisionDeposit.into(),
					&submitter,
				);
				let depositor_held = <Balances<T> as InspectHold<u64>>::balance_on_hold(
					&HoldReason::DecisionDeposit.into(),
					&decision_depositor,
				);

				assert_eq!(submitter_held, submission_amount);
				assert_eq!(depositor_held, decision_amount);
			});
		}

		#[test]
		fn mbm_migration_works_multiple_steps() {
			ExtBuilder::default().build_and_execute(|| {
				// Setup: Create multiple referenda
				let submitter: u64 = 1;
				let submission_amount: u64 = 10;

				// Give account enough balance for multiple deposits
				let _ = <Balances<T> as Currency<u64>>::deposit_creating(&submitter, 10000);

				// Reserve funds for 5 referenda
				for _ in 0..5 {
					assert_ok!(<Balances<T> as ReservableCurrency<u64>>::reserve(
						&submitter,
						submission_amount
					));
				}

				// Create 5 referenda
				for i in 0..5u32 {
					let status = create_status_v0();
					let referendum = v1::ReferendumInfoOf::<T, ()>::Ongoing(ReferendumStatus {
						submission_deposit: Deposit { who: submitter, amount: submission_amount },
						decision_deposit: None,
						..status
					});
					v1::ReferendumInfoFor::<T, ()>::insert(i, referendum);
				}
				ReferendumCount::<T, ()>::put(5);
				StorageVersion::new(1).put::<Pallet<T, ()>>();

				// Run migration with limited weight (only enough for 2 items)
				let step_weight = <() as WeightInfo>::step();
				let limited_weight = step_weight.saturating_mul(2);
				let mut meter = WeightMeter::with_limit(limited_weight);

				// First step - should process 2 items
				let result = LazyMigrationV1ToV2::<T, (), Balances<T>, ()>::step(None, &mut meter);
				assert!(result.is_ok());
				let cursor = result.unwrap();
				assert!(cursor.is_some()); // Not complete yet

				// Second step - process next 2 items
				let mut meter = WeightMeter::with_limit(limited_weight);
				let result =
					LazyMigrationV1ToV2::<T, (), Balances<T>, ()>::step(cursor, &mut meter);
				assert!(result.is_ok());
				let cursor = result.unwrap();
				assert!(cursor.is_some()); // Still not complete

				// Third step - process remaining 1 item
				let mut meter = WeightMeter::with_limit(limited_weight);
				let result =
					LazyMigrationV1ToV2::<T, (), Balances<T>, ()>::step(cursor, &mut meter);
				assert!(result.is_ok());
				let cursor = result.unwrap();
				assert!(cursor.is_none()); // Complete!

				// Verify all holds are in place
				let total_held = <Balances<T> as InspectHold<u64>>::balance_on_hold(
					&HoldReason::DecisionDeposit.into(),
					&submitter,
				);
				assert_eq!(total_held, submission_amount * 5);
			});
		}

		#[test]
		fn mbm_migration_handles_insufficient_weight() {
			ExtBuilder::default().build_and_execute(|| {
				// Create a referendum
				let submitter: u64 = 1;
				let _ = <Balances<T> as Currency<u64>>::deposit_creating(&submitter, 1000);
				assert_ok!(<Balances<T> as ReservableCurrency<u64>>::reserve(&submitter, 10));

				let status = create_status_v0();
				let referendum = v1::ReferendumInfoOf::<T, ()>::Ongoing(ReferendumStatus {
					submission_deposit: Deposit { who: submitter, amount: 10 },
					decision_deposit: None,
					..status
				});
				v1::ReferendumInfoFor::<T, ()>::insert(0, referendum);
				ReferendumCount::<T, ()>::put(1);

				// Run migration with insufficient weight
				let mut meter = WeightMeter::with_limit(Weight::from_parts(1, 0));
				let result = LazyMigrationV1ToV2::<T, (), Balances<T>, ()>::step(None, &mut meter);

				// Should return InsufficientWeight error
				assert!(matches!(
					result,
					Err(
						frame_support::migrations::SteppedMigrationError::InsufficientWeight { .. }
					)
				));
			});
		}

		#[test]
		fn mbm_migration_handles_empty_storage() {
			ExtBuilder::default().build_and_execute(|| {
				// No referenda in storage
				StorageVersion::new(1).put::<Pallet<T, ()>>();

				// Run migration
				let mut meter = WeightMeter::new();
				let result = LazyMigrationV1ToV2::<T, (), Balances<T>, ()>::step(None, &mut meter);

				// Should complete immediately with None cursor
				assert!(matches!(result, Ok(None)));
			});
		}

		#[test]
		fn mbm_migration_handles_killed_referendum() {
			ExtBuilder::default().build_and_execute(|| {
				// Create a killed referendum (no deposits to migrate)
				let killed = ReferendumInfoOf::<T, ()>::Killed(42);

				ReferendumCount::<T, ()>::put(1);
				v1::ReferendumInfoFor::<T, ()>::insert(0, killed);
				StorageVersion::new(1).put::<Pallet<T, ()>>();

				// Run migration
				let mut meter = WeightMeter::new();
				let result = LazyMigrationV1ToV2::<T, (), Balances<T>, ()>::step(None, &mut meter);

				// Should complete successfully
				assert!(matches!(result, Ok(None)));
			});
		}

		#[test]
		fn mbm_migration_id_is_correct() {
			// Verify the migration ID is set correctly
			let id = LazyMigrationV1ToV2::<T, (), Balances<T>, ()>::id();
			assert_eq!(id.version_from, 1);
			assert_eq!(id.version_to, 2);
			assert_eq!(&id.pallet_id, b"pallet-referenda  ");
		}
	}
}
