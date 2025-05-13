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

//! Nomination pools data migrator module.

use crate::{types::*, *};
use alias::UnstakeRequest;

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub enum FastUnstakeStage<AccountId> {
	StorageValues,
	Queue(Option<AccountId>),
	Finished,
}

#[derive(
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	RuntimeDebugNoBound,
	CloneNoBound,
	PartialEqNoBound,
	EqNoBound,
)]
#[codec(mel_bound(T: Config))]
#[scale_info(skip_type_params(T))]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub enum RcFastUnstakeMessage<T: pallet_fast_unstake::Config> {
	StorageValues { values: FastUnstakeStorageValues<T> },
	Queue { member: (T::AccountId, alias::BalanceOf<T>) },
}

/// All the `StorageValues` from the fast unstake pallet.
#[derive(
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	CloneNoBound,
	PartialEqNoBound,
	EqNoBound,
	RuntimeDebugNoBound,
)]
#[codec(mel_bound(T: Config))]
#[scale_info(skip_type_params(T))]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub struct FastUnstakeStorageValues<T: pallet_fast_unstake::Config> {
	pub head: Option<UnstakeRequest<T>>,
	pub eras_to_check_per_block: u32,
}

impl<T: pallet_fast_unstake::Config> FastUnstakeMigrator<T> {
	pub fn take_values() -> FastUnstakeStorageValues<T> {
		FastUnstakeStorageValues {
			head: alias::Head::<T>::take(),
			eras_to_check_per_block: pallet_fast_unstake::ErasToCheckPerBlock::<T>::take(),
		}
	}

	pub fn put_values(values: FastUnstakeStorageValues<T>) {
		alias::Head::<T>::set(values.head);
		pallet_fast_unstake::ErasToCheckPerBlock::<T>::put(values.eras_to_check_per_block);
	}
}

pub struct FastUnstakeMigrator<T> {
	_phantom: PhantomData<T>,
}

impl<T: Config> PalletMigration for FastUnstakeMigrator<T> {
	type Key = FastUnstakeStage<T::AccountId>;
	type Error = Error<T>;

	fn migrate_many(
		current_key: Option<Self::Key>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<Self::Key>, Self::Error> {
		let mut inner_key = current_key.unwrap_or(FastUnstakeStage::StorageValues);
		let mut messages = XcmBatchAndMeter::new_from_config::<T>();

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
				.any_lt(T::AhWeightInfo::receive_fast_unstake_messages((messages.len() + 1) as u32))
			{
				log::info!("AH weight limit reached at batch length {}, stopping", messages.len());
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
				FastUnstakeStage::StorageValues => {
					let values = Self::take_values();
					messages.push(RcFastUnstakeMessage::StorageValues { values });
					FastUnstakeStage::Queue(None)
				},
				FastUnstakeStage::Queue(queue_iter) => {
					let mut new_queue_iter = match queue_iter.clone() {
						Some(queue_iter) => pallet_fast_unstake::Queue::<T>::iter_from(
							pallet_fast_unstake::Queue::<T>::hashed_key_for(queue_iter),
						),
						None => pallet_fast_unstake::Queue::<T>::iter(),
					};

					match new_queue_iter.next() {
						Some((key, member)) => {
							pallet_fast_unstake::Queue::<T>::remove(&key);
							messages.push(RcFastUnstakeMessage::Queue {
								member: (key.clone(), member),
							});
							FastUnstakeStage::Queue(Some(key))
						},
						None => FastUnstakeStage::Finished,
					}
				},
				FastUnstakeStage::Finished => {
					break;
				},
			}
		}

		if !messages.is_empty() {
			Pallet::<T>::send_chunked_xcm_and_track(
				messages,
				|messages| types::AhMigratorCall::<T>::ReceiveFastUnstakeMessages { messages },
				|len| T::AhWeightInfo::receive_fast_unstake_messages(len),
			)?;
		}

		if inner_key == FastUnstakeStage::Finished {
			Ok(None)
		} else {
			Ok(Some(inner_key))
		}
	}
}

#[cfg(feature = "std")]
impl<T: Config> crate::types::RcMigrationCheck for FastUnstakeMigrator<T> {
	type RcPrePayload = (Vec<(T::AccountId, alias::BalanceOf<T>)>, u32); // (queue, eras_to_check)

	fn pre_check() -> Self::RcPrePayload {
		let queue: Vec<_> = pallet_fast_unstake::Queue::<T>::iter().collect();
		let eras_to_check = pallet_fast_unstake::ErasToCheckPerBlock::<T>::get();

		assert!(
			alias::Head::<T>::get().is_none(),
			"Staking Heads must be empty on the relay chain before the migration"
		);

		(queue, eras_to_check)
	}

	fn post_check(_: Self::RcPrePayload) {
		// RC post: Ensure that entries have been deleted
		assert!(
			alias::Head::<T>::get().is_none(),
			"Assert storage 'FastUnstake::Head::rc_post::empty'"
		);
		assert!(
			pallet_fast_unstake::Queue::<T>::iter().next().is_none(),
			"Assert storage 'FastUnstake::Queue::rc_post::empty'"
		);
		assert!(
			pallet_fast_unstake::ErasToCheckPerBlock::<T>::get() == 0,
			"Assert storage 'FastUnstake::ErasToCheckPerBlock::rc_post::empty'"
		);
	}
}

pub mod alias {
	use super::*;
	use frame_support::traits::Currency;
	use pallet_fast_unstake::types::*;
	use sp_staking::EraIndex;

	pub type BalanceOf<T> = <<T as pallet_fast_unstake::Config>::Currency as Currency<
		<T as frame_system::Config>::AccountId,
	>>::Balance;

	/// An unstake request.
	///
	/// This is stored in [`crate::Head`] storage item and points to the current unstake request
	/// that is being processed.
	// From https://github.com/paritytech/polkadot-sdk/blob/7ecf3f757a5d6f622309cea7f788e8a547a5dce8/substrate/frame/fast-unstake/src/types.rs#L48-L57
	#[derive(
		Encode,
		Decode,
		EqNoBound,
		PartialEqNoBound,
		CloneNoBound,
		TypeInfo,
		RuntimeDebugNoBound,
		MaxEncodedLen,
	)]
	#[scale_info(skip_type_params(T))]
	#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
	pub struct UnstakeRequest<T: pallet_fast_unstake::Config> {
		/// This list of stashes are being processed in this request, and their corresponding
		/// deposit.
		pub stashes: BoundedVec<(T::AccountId, BalanceOf<T>), T::BatchSize>,
		/// The list of eras for which they have been checked.
		pub checked: BoundedVec<EraIndex, MaxChecking<T>>,
	}

	// From https://github.com/paritytech/polkadot-sdk/blob/7ecf3f757a5d6f622309cea7f788e8a547a5dce8/substrate/frame/fast-unstake/src/lib.rs#L213-L214
	#[frame_support::storage_alias(pallet_name)]
	pub type Head<T: pallet_fast_unstake::Config> =
		StorageValue<pallet_fast_unstake::Pallet<T>, UnstakeRequest<T>, OptionQuery>;
}
