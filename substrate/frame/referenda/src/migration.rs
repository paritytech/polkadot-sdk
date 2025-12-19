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

pub mod v2 {
    use super::*;
    use frame_support::traits::{
        fungible::MutateHold,
        tokens::{Precision, Preservation},
        Currency, ReservableCurrency,
    };

    /// The log target.
    const TARGET: &'static str = "runtime::referenda::migration::v2";

    /// Migrate from the old `Currency` reserve system to the new `fungible` hold system.
    ///
    /// This migration:
    /// 1. Iterates through all referenda with deposits
    /// 2. Unreserves the old reserved balance
    /// 3. Places a hold with the new `HoldReason::DecisionDeposit`
    pub struct MigrateV1ToV2<T, I, OldCurrency>(PhantomData<(T, I, OldCurrency)>);

    impl<T, I, OldCurrency> OnRuntimeUpgrade for MigrateV1ToV2<T, I, OldCurrency>
    where
        T: Config<I>,
        I: 'static,
        OldCurrency: ReservableCurrency<T::AccountId, Balance = BalanceOf<T, I>>,
    {
        #[cfg(feature = "try-runtime")]
        fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
            let referendum_count = ReferendumInfoFor::<T, I>::iter().count();
            log::info!(
                target: TARGET,
                "pre-upgrade state contains '{}' referendums.",
                referendum_count
            );

            // Count deposits that need migration
            let mut deposit_count = 0u32;
            for (_, info) in ReferendumInfoFor::<T, I>::iter() {
                match info {
                    ReferendumInfo::Ongoing(status) => {
                        if status.submission_deposit.amount > Zero::zero() {
                            deposit_count += 1;
                        }
                        if let Some(ref d) = status.decision_deposit {
                            if d.amount > Zero::zero() {
                                deposit_count += 1;
                            }
                        }
                    },
                    ReferendumInfo::Approved(_, ref s, ref d)
                    | ReferendumInfo::Rejected(_, ref s, ref d)
                    | ReferendumInfo::Cancelled(_, ref s, ref d)
                    | ReferendumInfo::TimedOut(_, ref s, ref d) => {
                        if let Some(ref submission) = s {
                            if submission.amount > Zero::zero() {
                                deposit_count += 1;
                            }
                        }
                        if let Some(ref decision) = d {
                            if decision.amount > Zero::zero() {
                                deposit_count += 1;
                            }
                        }
                    },
                    ReferendumInfo::Killed(_) => {},
                }
            }

            log::info!(
                target: TARGET,
                "pre-upgrade: '{}' deposits to migrate.",
                deposit_count
            );

            Ok((referendum_count as u32, deposit_count).encode())
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

            if on_chain_version != 1 {
                log::warn!(target: TARGET, "skipping migration from v1 to v2.");
                return weight;
            }

            let mut migrated_deposits = 0u32;

            for (index, info) in ReferendumInfoFor::<T, I>::iter() {
                weight.saturating_accrue(T::DbWeight::get().reads(1));

                let deposits_to_migrate = Self::collect_deposits(&info);

                for Deposit { who, amount } in deposits_to_migrate {
                    if amount.is_zero() {
                        continue;
                    }

                    // Unreserve the old reserved balance
                    let remaining = OldCurrency::unreserve(&who, amount);
                    if !remaining.is_zero() {
                        log::warn!(
                            target: TARGET,
                            "referendum #{:?}: could not fully unreserve for {:?}. Remaining: {:?}",
                            index,
                            who,
                            remaining
                        );
                    }

                    // Hold with the new HoldReason
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
                            migrated_deposits += 1;
                            log::info!(
                                target: TARGET,
                                "referendum #{:?}: migrated deposit of {:?} for {:?}",
                                index,
                                amount_to_hold,
                                who
                            );
                        }
                    }

                    weight.saturating_accrue(T::DbWeight::get().reads_writes(2, 2));
                }
            }

            StorageVersion::new(2).put::<Pallet<T, I>>();
            weight.saturating_accrue(T::DbWeight::get().writes(1));

            log::info!(
                target: TARGET,
                "migration complete. Migrated {} deposits.",
                migrated_deposits
            );

            weight
        }

        #[cfg(feature = "try-runtime")]
        fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
            let on_chain_version = Pallet::<T, I>::on_chain_storage_version();
            ensure!(on_chain_version == 2, "must upgrade from version 1 to 2.");

            let (pre_referendum_count, _pre_deposit_count): (u32, u32) =
                Decode::decode(&mut &state[..])
                    .expect("failed to decode the state from pre-upgrade.");

            let post_referendum_count = ReferendumInfoFor::<T, I>::iter().count() as u32;
            ensure!(
                post_referendum_count == pre_referendum_count,
                "referendum count must remain the same."
            );

            log::info!(target: TARGET, "migration verification complete.");
            Ok(())
        }
    }

    impl<T, I, OldCurrency> MigrateV1ToV2<T, I, OldCurrency>
    where
        T: Config<I>,
        I: 'static,
    {
        /// Collect all deposits from a referendum that need migration.
        fn collect_deposits(
            info: &ReferendumInfoOf<T, I>,
        ) -> Vec<Deposit<T::AccountId, BalanceOf<T, I>>> {
            let mut deposits = Vec::new();

            match info {
                ReferendumInfo::Ongoing(status) => {
                    deposits.push(status.submission_deposit.clone());
                    if let Some(ref d) = status.decision_deposit {
                        deposits.push(d.clone());
                    }
                },
                ReferendumInfo::Approved(_, ref s, ref d)
                | ReferendumInfo::Rejected(_, ref s, ref d)
                | ReferendumInfo::Cancelled(_, ref s, ref d)
                | ReferendumInfo::TimedOut(_, ref s, ref d) => {
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
}
