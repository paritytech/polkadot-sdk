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

#![doc = include_str!("proxy.md")]

use frame_support::traits::Currency;

extern crate alloc;
use crate::{types::*, *};
use alloc::vec::Vec;
use frame_system::pallet_prelude::BlockNumberFor;

pub struct ProxyProxiesMigrator<T> {
	_marker: sp_std::marker::PhantomData<T>,
}

pub struct ProxyAnnouncementMigrator<T: Config> {
	_marker: sp_std::marker::PhantomData<T>,
}

type BalanceOf<T> = <<T as pallet_proxy::Config>::Currency as Currency<
	<T as frame_system::Config>::AccountId,
>>::Balance;

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub struct RcProxy<AccountId, Balance, ProxyType, BlockNumber> {
	/// The account that is delegating to their proxy.
	pub delegator: AccountId,
	/// The deposit that was `Currency::reserved` from the delegator.
	pub deposit: Balance,
	/// The proxies that were delegated to and that can act on behalf of the delegator.
	pub proxies: Vec<pallet_proxy::ProxyDefinition<AccountId, ProxyType, BlockNumber>>,
}

pub type RcProxyOf<T, ProxyType> =
	RcProxy<AccountIdOf<T>, BalanceOf<T>, ProxyType, BlockNumberFor<T>>;

/// A RcProxy in Relay chain format, can only be understood by the RC and must be translated first.
pub(crate) type RcProxyLocalOf<T> = RcProxyOf<T, <T as pallet_proxy::Config>::ProxyType>;

/// A deposit that was taken for a proxy announcement.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub struct RcProxyAnnouncement<AccountId, Balance> {
	pub depositor: AccountId,
	pub deposit: Balance,
}

pub type RcProxyAnnouncementOf<T> = RcProxyAnnouncement<AccountIdOf<T>, BalanceOf<T>>;

impl<T: Config> PalletMigration for ProxyProxiesMigrator<T> {
	type Key = T::AccountId;
	type Error = Error<T>;

	fn migrate_many(
		mut last_key: Option<AccountIdOf<T>>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<AccountIdOf<T>>, Error<T>> {
		let mut batch = XcmBatchAndMeter::new_from_config::<T>();

		// Get iterator starting after last processed key
		let mut key_iter = if let Some(last_key) = last_key.clone() {
			pallet_proxy::Proxies::<T>::iter_from(pallet_proxy::Proxies::<T>::hashed_key_for(
				&last_key,
			))
		} else {
			pallet_proxy::Proxies::<T>::iter()
		};

		// Process accounts until we run out of weight or accounts
		for (acc, (proxies, deposit)) in key_iter.by_ref() {
			if proxies.is_empty() {
				defensive!("The proxy pallet disallows empty proxy lists");
				continue;
			}

			match Self::migrate_single(
				acc.clone(),
				(proxies.into_inner(), deposit),
				weight_counter,
				&mut batch,
			) {
				Ok(proxy) => {
					pallet_proxy::Proxies::<T>::remove(&acc);
					batch.push(proxy);
					last_key = Some(acc); // Update last processed key
				},
				Err(OutOfWeightError) if !batch.is_empty() => {
					// We have items to process but ran out of weight
					break;
				},
				Err(OutOfWeightError) => {
					defensive!("Not enough weight to migrate a single account");
					return Err(Error::OutOfWeight);
				},
			}
		}

		// Send batch if we have any items
		if !batch.is_empty() {
			Pallet::<T>::send_chunked_xcm_and_track(
				batch,
				|batch| types::AhMigratorCall::<T>::ReceiveProxyProxies { proxies: batch },
				|n| T::AhWeightInfo::receive_proxy_proxies(n),
			)?;
		}

		// Return last processed key if there are more items, None if we're done
		if key_iter.next().is_some() {
			Ok(last_key)
		} else {
			Ok(None)
		}
	}
}

impl<T: Config> ProxyProxiesMigrator<T> {
	fn migrate_single(
		acc: AccountIdOf<T>,
		(proxies, deposit): (
			Vec<pallet_proxy::ProxyDefinition<T::AccountId, T::ProxyType, BlockNumberFor<T>>>,
			BalanceOf<T>,
		),
		weight_counter: &mut WeightMeter,
		batch: &mut XcmBatchAndMeter<RcProxyLocalOf<T>>,
	) -> Result<RcProxyLocalOf<T>, OutOfWeightError> {
		if weight_counter.try_consume(T::DbWeight::get().reads_writes(1, 1)).is_err() ||
			weight_counter.try_consume(batch.consume_weight()).is_err()
		{
			log::info!("RC weight limit reached at batch length {}, stopping", batch.len());
			return Err(OutOfWeightError);
		}

		if T::MaxAhWeight::get().any_lt(T::AhWeightInfo::receive_proxy_proxies(batch.len() + 1)) {
			log::info!("AH weight limit reached at batch length {}, stopping", batch.len());
			return Err(OutOfWeightError);
		}

		let translated_proxies = proxies
			.into_iter()
			.map(|proxy| pallet_proxy::ProxyDefinition {
				delegate: proxy.delegate,
				proxy_type: proxy.proxy_type,
				delay: proxy.delay,
			})
			.collect();

		let mapped = RcProxy { delegator: acc, deposit, proxies: translated_proxies };

		Ok(mapped)
	}
}

impl<T: Config> PalletMigration for ProxyAnnouncementMigrator<T> {
	type Key = T::AccountId;
	type Error = Error<T>;

	fn migrate_many(
		last_key: Option<Self::Key>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<Self::Key>, Self::Error> {
		let mut batch = XcmBatchAndMeter::new_from_config::<T>();
		let mut last_processed = None;

		// Get iterator starting after last processed key
		let mut iter = if let Some(last_key) = last_key {
			pallet_proxy::Announcements::<T>::iter_from(
				pallet_proxy::Announcements::<T>::hashed_key_for(&last_key),
			)
		} else {
			pallet_proxy::Announcements::<T>::iter()
		};

		// Process announcements until we run out of weight
		for (acc, (_announcements, deposit)) in iter.by_ref() {
			if weight_counter.try_consume(T::DbWeight::get().reads_writes(1, 1)).is_err() ||
				weight_counter.try_consume(batch.consume_weight()).is_err()
			{
				log::info!("RC weight limit reached at batch length {}, stopping", batch.len());
				if batch.is_empty() {
					return Err(Error::OutOfWeight);
				} else {
					break;
				}
			}

			if T::MaxAhWeight::get()
				.any_lt(T::AhWeightInfo::receive_proxy_announcements((batch.len() + 1) as u32))
			{
				log::info!("AH weight limit reached at batch length {}, stopping", batch.len());
				if batch.is_empty() {
					return Err(Error::OutOfWeight);
				} else {
					break;
				}
			}

			batch.push(RcProxyAnnouncement { depositor: acc.clone(), deposit });
			last_processed = Some(acc);
		}

		// Send batch if we have any items
		if !batch.is_empty() {
			Pallet::<T>::send_chunked_xcm_and_track(
				batch,
				|batch| types::AhMigratorCall::<T>::ReceiveProxyAnnouncements {
					announcements: batch,
				},
				|n| T::AhWeightInfo::receive_proxy_announcements(n),
			)?;
		}

		// Return last processed key if there are more items, None if we're done
		if iter.next().is_some() {
			Ok(last_processed)
		} else {
			Ok(None)
		}
	}
}

#[cfg(feature = "std")]
use std::collections::btree_map::BTreeMap;

#[cfg(feature = "std")]
impl<T: Config> RcMigrationCheck for ProxyProxiesMigrator<T> {
	type RcPrePayload =
		BTreeMap<AccountId32, Vec<(<T as pallet_proxy::Config>::ProxyType, AccountId32)>>; // Map of Delegator -> (Kind, Delegatee)

	fn pre_check() -> Self::RcPrePayload {
		// Store the proxies per account before the migration
		let mut proxies = BTreeMap::new();
		for (delegator, (delegations, _deposit)) in pallet_proxy::Proxies::<T>::iter() {
			for delegation in delegations {
				proxies
					.entry(delegator.clone())
					.or_insert_with(Vec::new)
					.push((delegation.proxy_type, delegation.delegate));
			}
		}
		proxies
	}

	fn post_check(_: Self::RcPrePayload) {
		let count = pallet_proxy::Proxies::<T>::iter_keys().count();
		assert_eq!(count, 0, "Assert storage 'Proxy::Proxies::rc_post::empty'");
	}
}
