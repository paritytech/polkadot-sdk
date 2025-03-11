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
use pallet_bounties::{Bounty, BountyIndex};

pub type BalanceOf<T, I = ()> = pallet_treasury::BalanceOf<T, I>;

/// The stages of the bounties pallet data migration.
#[derive(Encode, Decode, Clone, Default, RuntimeDebug, TypeInfo, MaxEncodedLen, PartialEq, Eq)]
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
	Bounties((BountyIndex, Bounty<AccountId, Balance, BlockNumber>)),
}

/// Bounties data message that is being sent to the AH Migrator.
pub type RcBountiesMessageOf<T> =
	RcBountiesMessage<<T as frame_system::Config>::AccountId, BalanceOf<T>, BlockNumberFor<T>>;

pub struct BountiesMigrator<T: Config> {
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
		let mut messages = Vec::new();

		log::info!(target: LOG_TARGET, "Migrating Bounties at stage {:?}", &last_key);

		loop {
			if weight_counter
				.try_consume(<T as frame_system::Config>::DbWeight::get().reads_writes(1, 1))
				.is_err()
			{
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
						pallet_bounties::Bounties::<T>::iter_from_key(last_key)
					} else {
						pallet_bounties::Bounties::<T>::iter()
					};
					match iter.next() {
						Some((key, value)) => {
							log::debug!(target: LOG_TARGET, "Migration Bounty {:?}", &key);
							pallet_bounties::Bounties::<T>::remove(&key);
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

		Pallet::<T>::send_chunked_xcm(
			messages,
			|messages| types::AhMigratorCall::<T>::ReceiveBountiesMessages { messages },
			|_| Weight::from_all(1), // TODO
		)?;

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
