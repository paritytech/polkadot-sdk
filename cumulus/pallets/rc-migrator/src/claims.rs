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

use crate::*;
use alias::{EthereumAddress, StatementKind};
use frame_support::traits::{Currency, VestingSchedule};

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum ClaimsStage<AccountId> {
	StorageValues,
	Claims(Option<EthereumAddress>),
	Vesting(Option<EthereumAddress>),
	Signing(Option<EthereumAddress>),
	Preclaims(Option<AccountId>),
	Finished,
}

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, RuntimeDebug, Clone, PartialEq, Eq)]
pub enum RcClaimsMessage<AccountId, Balance, BlockNumber> {
	StorageValues { total: Balance },
	Claims((EthereumAddress, Balance)),
	Vesting { who: EthereumAddress, schedule: (Balance, Balance, BlockNumber) },
	Signing((EthereumAddress, StatementKind)),
	Preclaims((AccountId, EthereumAddress)),
}
pub type RcClaimsMessageOf<T> =
	RcClaimsMessage<<T as frame_system::Config>::AccountId, BalanceOf<T>, BlockNumberFor<T>>;

pub type BalanceOf<T> =
	<CurrencyOf<T> as Currency<<T as frame_system::Config>::AccountId>>::Balance;
type CurrencyOf<T> = <<T as pallet_claims::Config>::VestingSchedule as VestingSchedule<
	<T as frame_system::Config>::AccountId,
>>::Currency;

pub struct ClaimsMigrator<T> {
	_phantom: PhantomData<T>,
}

impl<T: Config> PalletMigration for ClaimsMigrator<T> {
	type Key = ClaimsStage<T::AccountId>;
	type Error = Error<T>;

	fn migrate_many(
		current_key: Option<Self::Key>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<Self::Key>, Self::Error> {
		let mut inner_key = current_key.unwrap_or(ClaimsStage::StorageValues);
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
				.any_lt(T::AhWeightInfo::receive_claims((messages.len() + 1) as u32))
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
				ClaimsStage::StorageValues => {
					let total = pallet_claims::Total::<T>::take();
					messages.push(RcClaimsMessage::StorageValues { total });
					ClaimsStage::Claims(None)
				},
				ClaimsStage::Claims(address) => {
					let mut iter = match address.clone() {
						Some(address) => alias::Claims::<T>::iter_from(
							alias::Claims::<T>::hashed_key_for(address),
						),
						None => alias::Claims::<T>::iter(),
					};

					match iter.next() {
						Some((address, amount)) => {
							alias::Claims::<T>::remove(&address);
							messages.push(RcClaimsMessage::Claims((address, amount)));
							ClaimsStage::Claims(Some(address))
						},
						None => ClaimsStage::Vesting(None),
					}
				},
				ClaimsStage::Vesting(address) => {
					let mut iter = match address.clone() {
						Some(address) => alias::Vesting::<T>::iter_from(
							alias::Vesting::<T>::hashed_key_for(address),
						),
						None => alias::Vesting::<T>::iter(),
					};

					match iter.next() {
						Some((address, schedule)) => {
							alias::Vesting::<T>::remove(&address);
							messages.push(RcClaimsMessage::Vesting { who: address, schedule });
							ClaimsStage::Vesting(Some(address))
						},
						None => ClaimsStage::Signing(None),
					}
				},
				ClaimsStage::Signing(address) => {
					let mut iter = match address.clone() {
						Some(address) => alias::Signing::<T>::iter_from(
							alias::Signing::<T>::hashed_key_for(address),
						),
						None => alias::Signing::<T>::iter(),
					};

					match iter.next() {
						Some((address, statement)) => {
							alias::Signing::<T>::remove(&address);
							messages.push(RcClaimsMessage::Signing((address, statement)));
							ClaimsStage::Signing(Some(address))
						},
						None => ClaimsStage::Preclaims(None),
					}
				},
				ClaimsStage::Preclaims(address) => {
					let mut iter = match address.clone() {
						Some(address) => alias::Preclaims::<T>::iter_from(
							alias::Preclaims::<T>::hashed_key_for(address),
						),
						None => alias::Preclaims::<T>::iter(),
					};

					match iter.next() {
						Some((address, statement)) => {
							alias::Preclaims::<T>::remove(&address);
							messages.push(RcClaimsMessage::Preclaims((address.clone(), statement)));
							ClaimsStage::Preclaims(Some(address))
						},
						None => ClaimsStage::Finished,
					}
				},
				ClaimsStage::Finished => {
					break;
				},
			}
		}

		if !messages.is_empty() {
			Pallet::<T>::send_chunked_xcm_and_track(
				messages,
				|messages| types::AhMigratorCall::<T>::ReceiveClaimsMessages { messages },
				|n| T::AhWeightInfo::receive_claims(n),
			)?;
		}

		if inner_key == ClaimsStage::Finished {
			Ok(None)
		} else {
			Ok(Some(inner_key))
		}
	}
}

pub mod alias {
	use super::*;

	/// Copy of the EthereumAddress type from the SDK since the version that we pull is is not MEL
	/// :(
	// From https://github.com/paritytech/polkadot-sdk/blob/d8df46c7a1488f2358e69368813fd772164c4dac/polkadot/runtime/common/src/claims/mod.rs#L130-L133
	#[derive(
		Clone, Copy, PartialEq, Eq, Encode, Decode, Default, RuntimeDebug, TypeInfo, MaxEncodedLen,
	)]
	pub struct EthereumAddress(pub [u8; 20]);

	// Also not MEL...
	// From https://github.com/paritytech/polkadot-sdk/blob/d8df46c7a1488f2358e69368813fd772164c4dac/polkadot/runtime/common/src/claims/mod.rs#L84-L103
	#[derive(Encode, Decode, Clone, Copy, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub enum StatementKind {
		Regular,
		Saft,
	}

	// From https://github.com/paritytech/polkadot-sdk/blob/d8df46c7a1488f2358e69368813fd772164c4dac/polkadot/runtime/common/src/claims/mod.rs#L226-L227
	#[frame_support::storage_alias(pallet_name)]
	pub type Claims<T: pallet_claims::Config> =
		StorageMap<pallet_claims::Pallet<T>, Identity, EthereumAddress, BalanceOf<T>>;

	// From https://github.com/paritytech/polkadot-sdk/blob/d8df46c7a1488f2358e69368813fd772164c4dac/polkadot/runtime/common/src/claims/mod.rs#L241-L242
	#[frame_support::storage_alias(pallet_name)]
	pub type Signing<T: pallet_claims::Config> =
		StorageMap<pallet_claims::Pallet<T>, Identity, EthereumAddress, StatementKind>;

	// From https://github.com/paritytech/polkadot-sdk/blob/d8df46c7a1488f2358e69368813fd772164c4dac/polkadot/runtime/common/src/claims/mod.rs#L245-L246
	#[frame_support::storage_alias(pallet_name)]
	pub type Preclaims<T: pallet_claims::Config> = StorageMap<
		pallet_claims::Pallet<T>,
		Identity,
		<T as frame_system::Config>::AccountId,
		EthereumAddress,
	>;

	// From https://github.com/paritytech/polkadot-sdk/blob/d8df46c7a1488f2358e69368813fd772164c4dac/polkadot/runtime/common/src/claims/mod.rs#L236-L238
	#[frame_support::storage_alias(pallet_name)]
	pub type Vesting<T: pallet_claims::Config> = StorageMap<
		pallet_claims::Pallet<T>,
		Identity,
		EthereumAddress,
		(BalanceOf<T>, BalanceOf<T>, BlockNumberFor<T>),
	>;
}

#[cfg(feature = "std")]
impl<T: Config> crate::types::RcMigrationCheck for ClaimsMigrator<T> {
	type RcPrePayload = Vec<RcClaimsMessageOf<T>>;

	fn pre_check() -> Self::RcPrePayload {
		let mut messages = Vec::new();

		// Collect StorageValues
		let total = pallet_claims::Total::<T>::get();
		messages.push(RcClaimsMessage::StorageValues { total });

		// Collect Claims
		for (address, amount) in alias::Claims::<T>::iter() {
			messages.push(RcClaimsMessage::Claims((address, amount)));
		}

		// Collect Vesting
		for (address, schedule) in alias::Vesting::<T>::iter() {
			messages.push(RcClaimsMessage::Vesting { who: address, schedule });
		}

		// Collect Signing
		for (address, statement) in alias::Signing::<T>::iter() {
			messages.push(RcClaimsMessage::Signing((address, statement)));
		}

		// Collect Preclaims
		for (account_id, address) in alias::Preclaims::<T>::iter() {
			messages.push(RcClaimsMessage::Preclaims((account_id, address)));
		}

		messages
	}

	fn post_check(_: Self::RcPrePayload) {
		assert!(
			!pallet_claims::Total::<T>::exists(),
			"Assert storage 'Claims::Total::rc_post::empty'"
		);
		assert!(
			alias::Claims::<T>::iter().next().is_none(),
			"Assert storage 'Claims::Claims::rc_post::empty'"
		);
		assert!(
			alias::Vesting::<T>::iter().next().is_none(),
			"Assert storage 'Claims::Vesting::rc_post::empty'"
		);
		assert!(
			alias::Signing::<T>::iter().next().is_none(),
			"Assert storage 'Claims::Signing::rc_post::empty'"
		);
		assert!(
			alias::Preclaims::<T>::iter().next().is_none(),
			"Assert storage 'Claims::Preclaims::rc_post::empty'"
		);

		log::info!("All claims data successfully migrated and cleared from the Relay Chain.");
	}
}
