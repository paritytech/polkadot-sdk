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

//! Election Provider Multi-Block migration.

use pallet_election_provider_multi_block as pallet_staking;
use pallet_staking::types::*;
use crate::*;
use sp_core::H256;
pub use frame_election_provider_support::PageIndex;

pub struct StakingMigrator<T> {
	_phantom: PhantomData<T>,
}

#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Clone,
	Default,
	PartialEq,
	Eq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum StakingStage {
	#[default]
	ValidatorCount,
	Finished,
}

#[derive(Encode, Decode, DecodeWithMemTracking, TypeInfo, RuntimeDebug, Clone, PartialEq, Eq)]
pub enum StakingMessage {
	ValidatorCount(u32),
}

//pub type StakingMessageOf<T> = StakingMessage;

impl<T: Config> PalletMigration for StakingMigrator<T> {
	type Key = StakingStage;
	type Error = Error<T>;

	fn migrate_many(current_key: Option<Self::Key>, weight_counter: &mut WeightMeter) -> Result<Option<Self::Key>, Self::Error> {
		let mut inner_key = current_key.unwrap_or_default();
		let mut messages = Vec::new();

		loop {
			if weight_counter.try_consume(<T as frame_system::Config>::DbWeight::get().reads_writes(1, 1)).is_err() {
				if messages.is_empty() {
					return Err(Error::OutOfWeight);
				} else {
					break;
				}
			}

			if messages.len() > 10_000 {
				log::warn!("Weight allowed very big batch, stopping");
				break;
			}

			inner_key = match inner_key {
				StakingStage::ValidatorCount => {
					let validator_count = ::pallet_staking::ValidatorCount::<T>::take();
					messages.push(StakingMessage::ValidatorCount(validator_count));
					StakingStage::Finished
				},
				StakingStage::Finished => {
					break;
				}
			};
		}
			
		if !messages.is_empty() {
			Pallet::<T>::send_chunked_xcm(messages, |messages| {
				types::AhMigratorCall::<T>::ReceiveStakingMessages { messages }
			})?;
		}

		if inner_key == StakingStage::Finished {
			Ok(None)
		} else {
			Ok(Some(inner_key))
		}
	}
}
