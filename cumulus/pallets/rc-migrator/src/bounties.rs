// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use crate::*;
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_bounties::{Bounty, BountyIndex};

pub type BalanceOf<T, I = ()> = pallet_treasury::BalanceOf<T, I>;

/// The stages of the bounties pallet data migration.
#[derive(Encode, Decode, Clone, Default, RuntimeDebug, TypeInfo, MaxEncodedLen, PartialEq, Eq)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub enum BountiesStage {
	#[default]
	BountyCount,
	BountyApprovals,
	BountyDescriptions {
		last_key: Option<BountyIndex>,
	},
	Bounties {
		last_key: Option<BountyIndex>,
	},
	Finished,
}

/// Bounties data message that is being sent to the AH Migrator.
#[derive(Encode, Decode, Debug, Clone, TypeInfo, PartialEq, Eq)]
pub enum RcBountiesMessage<AccountId, Balance, BlockNumber> {
	BountyCount(BountyIndex),
	BountyApprovals(Vec<BountyIndex>),
	BountyDescriptions((BountyIndex, Vec<u8>)),
	Bounties((BountyIndex, alias::Bounty<AccountId, Balance, BlockNumber>)),
}

/// Bounties data message that is being sent to the AH Migrator.
pub type RcBountiesMessageOf<T> =
	RcBountiesMessage<<T as frame_system::Config>::AccountId, BalanceOf<T>, BlockNumberFor<T>>;

pub struct BountiesMigrator<T> {
	_phantom: PhantomData<T>,
}

impl<T: Config> PalletMigration for BountiesMigrator<T> {
	type Key = BountiesStage;
	type Error = Error<T>;

	fn migrate_many(
		last_key: Option<Self::Key>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<Self::Key>, Self::Error> {
		let mut last_key = last_key.unwrap_or(BountiesStage::BountyCount);
		let mut messages = XcmBatchAndMeter::new_from_config::<T>();

		log::info!(target: LOG_TARGET, "Migrating Bounties at stage {:?}", &last_key);

		loop {
			if weight_counter.try_consume(T::DbWeight::get().reads_writes(1, 1)).is_err() ||
				weight_counter.try_consume(messages.consume_weight()).is_err()
			{
				log::info!("RC weight limit reached at batch length {}, stopping", messages.len());
				if messages.is_empty() {
					return Err(Error::OutOfWeight);
				} else {
					break;
				}
			}
			if T::MaxAhWeight::get()
				.any_lt(T::AhWeightInfo::receive_bounties_messages((messages.len() + 1) as u32))
			{
				log::info!("AH weight limit reached at batch length {}, stopping", messages.len());
				if messages.is_empty() {
					return Err(Error::OutOfWeight);
				} else {
					break;
				}
			}
			if messages.len() > 10_000 {
				log::warn!(target: LOG_TARGET, "Weight allowed very big batch, stopping");
				break;
			}

			last_key = match last_key {
				BountiesStage::BountyCount => {
					let count = pallet_bounties::BountyCount::<T>::take();
					log::debug!(target: LOG_TARGET, "Migration BountyCount {:?}", &count);
					messages.push(RcBountiesMessage::BountyCount(count));
					BountiesStage::BountyApprovals
				},
				BountiesStage::BountyApprovals => {
					let approvals = pallet_bounties::BountyApprovals::<T>::take();
					log::debug!(target: LOG_TARGET, "Migration BountyApprovals {:?}", &approvals);
					messages.push(RcBountiesMessage::BountyApprovals(approvals.into_inner()));
					BountiesStage::BountyDescriptions { last_key: None }
				},
				BountiesStage::BountyDescriptions { last_key } => {
					let mut iter = if let Some(last_key) = last_key {
						pallet_bounties::BountyDescriptions::<T>::iter_from_key(last_key)
					} else {
						pallet_bounties::BountyDescriptions::<T>::iter()
					};
					match iter.next() {
						Some((key, value)) => {
							log::debug!(
								target: LOG_TARGET,
								"Migration BountyDescription for bounty {:?}",
								&key
							);
							pallet_bounties::BountyDescriptions::<T>::remove(&key);
							messages.push(RcBountiesMessage::BountyDescriptions((
								key,
								value.into_inner(),
							)));
							BountiesStage::BountyDescriptions { last_key: Some(key) }
						},
						None => BountiesStage::Bounties { last_key: None },
					}
				},
				BountiesStage::Bounties { last_key } => {
					let mut iter = if let Some(last_key) = last_key {
						alias::Bounties::<T>::iter_from_key(last_key)
					} else {
						alias::Bounties::<T>::iter()
					};
					match iter.next() {
						Some((key, value)) => {
							log::debug!(target: LOG_TARGET, "Migration Bounty {:?}", &key);
							alias::Bounties::<T>::remove(&key);
							messages.push(RcBountiesMessage::Bounties((key, value)));
							BountiesStage::Bounties { last_key: Some(key) }
						},
						None => BountiesStage::Finished,
					}
				},
				BountiesStage::Finished => {
					break;
				},
			};
		}

		if !messages.is_empty() {
			Pallet::<T>::send_chunked_xcm_and_track(
				messages.into_inner(),
				|messages| types::AhMigratorCall::<T>::ReceiveBountiesMessages { messages },
				|len| T::AhWeightInfo::receive_bounties_messages(len),
			)?;
		}

		if last_key == BountiesStage::Finished {
			log::info!(target: LOG_TARGET, "Bounties migration finished");
			Ok(None)
		} else {
			log::info!(
				target: LOG_TARGET,
				"Bounties migration iteration stopped at {:?}",
				&last_key
			);
			Ok(Some(last_key))
		}
	}
}

pub mod alias {
	use super::*;
	use pallet_bounties::BountyStatus;

	/// Alias of [pallet_bounties::BalanceOf].
	pub type BalanceOf<T, I = ()> = pallet_treasury::BalanceOf<T, I>;

	/// A bounty proposal.
	///
	/// Alias of [pallet_bounties::Bounty].
	#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
	pub struct Bounty<AccountId, Balance, BlockNumber> {
		/// The account proposing it.
		pub proposer: AccountId,
		/// The (total) amount that should be paid if the bounty is rewarded.
		pub value: Balance,
		/// The curator fee. Included in value.
		pub fee: Balance,
		/// The deposit of curator.
		pub curator_deposit: Balance,
		/// The amount held on deposit (reserved) for making this proposal.
		pub bond: Balance,
		/// The status of this bounty.
		pub status: BountyStatus<AccountId, BlockNumber>,
	}

	/// Bounties that have been made.
	///
	/// Alias of [pallet_bounties::Bounties].
	#[frame_support::storage_alias(pallet_name)]
	pub type Bounties<T: pallet_bounties::Config<()>> = StorageMap<
		pallet_bounties::Pallet<T, ()>,
		Twox64Concat,
		BountyIndex,
		Bounty<<T as frame_system::Config>::AccountId, BalanceOf<T, ()>, BlockNumberFor<T>>,
	>;
}

// (BountyCount, Bounties, BountyDescriptions, BountyApprovals)
pub type RcPrePayload<T> = (
	BountyIndex,
	Vec<(
		BountyIndex,
		Bounty<<T as frame_system::Config>::AccountId, BalanceOf<T>, BlockNumberFor<T>>,
	)>,
	Vec<(BountyIndex, Vec<u8>)>,
	Vec<BountyIndex>,
);

#[cfg(feature = "std")]
impl<T: Config> crate::types::RcMigrationCheck for BountiesMigrator<T> {
	type RcPrePayload = RcPrePayload<T>;

	fn pre_check() -> Self::RcPrePayload {
		let count = pallet_bounties::BountyCount::<T>::get();
		let bounties: Vec<_> = pallet_bounties::Bounties::<T>::iter().collect();
		let descriptions: Vec<_> = pallet_bounties::BountyDescriptions::<T>::iter()
			.map(|(key, bounded_vec)| (key, bounded_vec.into_inner()))
			.collect();
		let approvals = pallet_bounties::BountyApprovals::<T>::get().into_inner();
		(count, bounties, descriptions, approvals)
	}

	fn post_check(_rc_pre_payload: Self::RcPrePayload) {
		// Assert storage 'Bounties::BountyCount::rc_post::empty'
		assert_eq!(
			pallet_bounties::BountyCount::<T>::get(),
			0,
			"Bounty count should be 0 on RC after migration"
		);

		// Assert storage 'Bounties::Bounties::rc_post::empty'
		assert!(
			pallet_bounties::Bounties::<T>::iter().next().is_none(),
			"Bounties map should be empty on RC after migration"
		);

		// Assert storage 'Bounties::BountyDescriptions::rc_post::empty'
		assert!(
			pallet_bounties::BountyDescriptions::<T>::iter().next().is_none(),
			"Bount descriptions map should be empty on RC after migration"
		);

		// Assert storage 'Bounties::BountyApprovals::rc_post::empty'
		assert!(
			pallet_bounties::BountyApprovals::<T>::get().is_empty(),
			"Bounty Approvals vec should be empty on RC after migration"
		);
	}
}
