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

#![doc = include_str!("multisig.md")]

use frame_support::traits::Currency;

extern crate alloc;
use crate::{types::*, *};
use alloc::vec::Vec;

mod aliases {
	use super::*;
	use frame_system::pallet_prelude::BlockNumberFor;
	use pallet_multisig::Timepoint;

	/// Copied from https://github.com/paritytech/polkadot-sdk/blob/7c5224cb01710d0c14c87bf3463cc79e49b3e7b5/substrate/frame/multisig/src/lib.rs#L96-L111
	#[derive(
		Clone, Eq, PartialEq, Encode, Decode, Default, RuntimeDebug, TypeInfo, MaxEncodedLen,
	)]
	#[scale_info(skip_type_params(MaxApprovals))]
	pub struct Multisig<BlockNumber, Balance, AccountId, MaxApprovals>
	where
		MaxApprovals: Get<u32>,
	{
		/// The extrinsic when the multisig operation was opened.
		pub when: Timepoint<BlockNumber>,
		/// The amount held in reserve of the `depositor`, to be returned once the operation ends.
		pub deposit: Balance,
		/// The account who opened it (i.e. the first to approve it).
		pub depositor: AccountId,
		/// The approvals achieved so far, including the depositor. Always sorted.
		pub approvals: BoundedVec<AccountId, MaxApprovals>,
	}

	/// Copied from https://github.com/paritytech/polkadot-sdk/blob/7c5224cb01710d0c14c87bf3463cc79e49b3e7b5/substrate/frame/multisig/src/lib.rs#L77-L78
	pub type BalanceOf<T> = <<T as pallet_multisig::Config>::Currency as Currency<
		<T as frame_system::Config>::AccountId,
	>>::Balance;

	/// Copied from https://github.com/paritytech/polkadot-sdk/blob/7c5224cb01710d0c14c87bf3463cc79e49b3e7b5/substrate/frame/multisig/src/lib.rs#L171-L180
	#[frame_support::storage_alias(pallet_name)]
	pub type Multisigs<T: pallet_multisig::Config> = StorageDoubleMap<
		pallet_multisig::Pallet<T>,
		Twox64Concat,
		<T as frame_system::Config>::AccountId,
		Blake2_128Concat,
		[u8; 32],
		Multisig<
			BlockNumberFor<T>,
			BalanceOf<T>,
			<T as frame_system::Config>::AccountId,
			<T as pallet_multisig::Config>::MaxSignatories,
		>,
	>;

	pub type MultisigOf<T> = Multisig<
		BlockNumberFor<T>,
		BalanceOf<T>,
		AccountIdOf<T>,
		<T as pallet_multisig::Config>::MaxSignatories,
	>;
}

/// A multi sig that was migrated out and is ready to be received by AH.
// NOTE I am not sure if generics here are so smart, since RC and AH *have* to put the same
// generics, otherwise it would be a bug and fail to decode. However, we can just prevent that but
// by not exposing generics... On the other hand: for Westend and Kusama it could possibly help if
// we don't hard-code all types.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub struct RcMultisig<AccountId, Balance> {
	/// The creator of the multisig who placed the deposit.
	pub creator: AccountId,
	/// Amount of the deposit.
	pub deposit: Balance,
	/// Optional details field to debug. Can be `None` in prod. Contains the derived account.
	pub details: Option<AccountId>,
}

pub type RcMultisigOf<T> = RcMultisig<AccountIdOf<T>, BalanceOf<T>>;

type BalanceOf<T> = <<T as pallet_multisig::Config>::Currency as Currency<
	<T as frame_system::Config>::AccountId,
>>::Balance;

pub struct MultisigMigrator<T, W, L> {
	_marker: sp_std::marker::PhantomData<(T, W, L)>,
}

impl<T: pallet_multisig::Config, W: AhWeightInfo, L: Get<Weight>> MultisigMigrator<T, W, L> {
	pub fn migrate_out_many(
		mut last_key: Option<(T::AccountId, [u8; 32])>,
		rc_weight: &mut WeightMeter,
	) -> Result<(Vec<RcMultisigOf<T>>, Option<(T::AccountId, [u8; 32])>), Error<T>> {
		let mut batch = Vec::new();
		let mut iter = match last_key.clone() {
			Some((k1, k2)) =>
				aliases::Multisigs::<T>::iter_from(aliases::Multisigs::<T>::hashed_key_for(k1, k2)),
			None => aliases::Multisigs::<T>::iter(),
		};

		loop {
			let kv = iter.next();
			let Some((k1, k2, multisig)) = kv else {
				last_key = None;
				log::info!(target: LOG_TARGET, "No more multisigs to migrate");
				break;
			};

			log::debug!(target: LOG_TARGET, "Migrating multisigs of acc {:?}", k1);

			match Self::migrate_single(k1.clone(), multisig, rc_weight, batch.len() as u32) {
				Ok(ms) => batch.push(ms), // TODO continue here
				// Account does not need to be migrated
				// Not enough weight, lets try again in the next block since we made some progress.
				Err(OutOfWeightError) if !batch.is_empty() => break,
				// Not enough weight and was unable to make progress, bad.
				Err(OutOfWeightError) => {
					defensive!("Not enough weight to migrate a single account");
					return Err(Error::OutOfWeight);
				},
			}

			// TODO delete old
			last_key = Some((k1, k2));
		}

		Ok((batch, last_key))
	}
}

impl<T: Config, W: AhWeightInfo, L: Get<Weight>> PalletMigration for MultisigMigrator<T, W, L> {
	type Key = (T::AccountId, [u8; 32]);
	type Error = Error<T>;

	/// Migrate until the weight is exhausted. Start at the given key.
	///
	/// Storage changes must be rolled back on error.
	fn migrate_many(
		last_key: Option<Self::Key>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<Self::Key>, Error<T>> {
		let (batch, last_key) = Self::migrate_out_many(last_key, weight_counter)?;

		if !batch.is_empty() {
			Pallet::<T>::send_chunked_xcm_and_track(
				batch,
				|batch| types::AhMigratorCall::<T>::ReceiveMultisigs { multisigs: batch },
				|n| W::receive_multisigs(n),
			)?;
		}

		Ok(last_key)
	}
}

impl<T: pallet_multisig::Config, W: AhWeightInfo, L: Get<Weight>> MultisigMigrator<T, W, L> {
	fn migrate_single(
		k1: AccountIdOf<T>,
		ms: aliases::MultisigOf<T>,
		rc_weight: &mut WeightMeter,
		batch_len: u32,
	) -> Result<RcMultisigOf<T>, OutOfWeightError> {
		// TODO weight
		if rc_weight.try_consume(Weight::from_all(1_000)).is_err() {
			return Err(OutOfWeightError);
		}

		if L::get().any_lt(W::receive_multisigs(batch_len + 1)) {
			log::info!("AH weight limit reached at batch length {}, stopping", batch_len);
			return Err(OutOfWeightError);
		}

		Ok(RcMultisig { creator: ms.depositor, deposit: ms.deposit, details: Some(k1) })
	}
}
