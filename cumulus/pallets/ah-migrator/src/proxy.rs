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
use pallet_rc_migrator::types::ToPolkadotSs58;
use sp_runtime::{traits::Zero, BoundedSlice, Saturating};

impl<T: Config> Pallet<T> {
	pub fn do_receive_proxies(proxies: Vec<RcProxyOf<T, T::RcProxyType>>) -> Result<(), Error<T>> {
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::ProxyProxies,
			count: proxies.len() as u32,
		});
		let (mut count_good, mut count_bad) = (0, 0);
		log::info!(target: LOG_TARGET, "Integrating batch proxies of with len {}", proxies.len());

		for proxy in proxies {
			match Self::do_receive_proxy(proxy) {
				Ok(()) => count_good += 1,
				Err(e) => {
					count_bad += 1;
					log::error!(target: LOG_TARGET, "Error while integrating proxy: {:?}", e);
				},
			}
		}
		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::ProxyProxies,
			count_good,
			count_bad,
		});

		Ok(())
	}

	/// Receive a single proxy and write it to storage.
	pub fn do_receive_proxy(proxy: RcProxyOf<T, T::RcProxyType>) -> Result<(), Error<T>> {
		log::info!(target: LOG_TARGET, "Integrating proxy {}, deposit {:?}", proxy.delegator.to_polkadot_ss58(), proxy.deposit);
		let max_proxies = <T as pallet_proxy::Config>::MaxProxies::get() as usize;

		// Translate the incoming ones from RC
		let mut proxies = proxy.proxies.into_iter().enumerate().filter_map(|(i, p)| {
			let Ok(proxy_type) = T::RcToProxyType::try_convert(p.proxy_type.clone()) else {
				log::info!(target: LOG_TARGET, "Dropping unsupported proxy kind of '{:?}' at index {} for {}", p.proxy_type, i, proxy.delegator.to_polkadot_ss58());
				// TODO unreserve deposit
				return None;
			};
			let delay = T::RcToAhDelay::convert(p.delay);

			log::info!(target: LOG_TARGET, "Proxy type: {:?} delegate: {:?}", proxy_type, p.delegate.to_polkadot_ss58());
			Some(pallet_proxy::ProxyDefinition {
				delegate: p.delegate,
				delay,
				proxy_type,
			})
		})
		.take(max_proxies)
		.collect::<Vec<_>>();

		// Add the already existing ones from AH
		let (existing_proxies, _deposit) = pallet_proxy::Proxies::<T>::get(&proxy.delegator);
		for delegation in existing_proxies {
			proxies.push(pallet_proxy::ProxyDefinition {
				delegate: delegation.delegate,
				delay: delegation.delay,
				proxy_type: delegation.proxy_type,
			});
		}

		if proxies.len() > max_proxies {
			// Some serious shit about to go down: user has more proxies than we can migrate :(
			// Best effort: we sort descending by Kind and Delay with the assumption that Kind 0 is
			// always the `Any` proxy and low Delay proxies are more important.
			defensive!("Truncating proxy list with best-effort priority");
			proxies.sort_by(|a, b| b.proxy_type.cmp(&a.proxy_type).then(b.delay.cmp(&a.delay)));
			proxies.truncate(max_proxies);
		}

		let Ok(bounded_proxies) =
			BoundedSlice::try_from(proxies.as_slice()).defensive_proof("Proxies should fit")
		else {
			return Err(Error::TODO);
		};

		// Add the proxies
		pallet_proxy::Proxies::<T>::insert(&proxy.delegator, (bounded_proxies, proxy.deposit));

		Ok(())
	}

	pub fn do_receive_proxy_announcements(
		announcements: Vec<RcProxyAnnouncementOf<T>>,
	) -> Result<(), Error<T>> {
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::ProxyAnnouncements,
			count: announcements.len() as u32,
		});

		let (mut count_good, mut count_bad) = (0, 0);
		log::info!(target: LOG_TARGET, "Unreserving deposits for {} proxy announcements", announcements.len());

		for announcement in announcements {
			match Self::do_receive_proxy_announcement(announcement) {
				Ok(()) => count_good += 1,
				Err(e) => {
					count_bad += 1;
					log::error!(target: LOG_TARGET, "Error while integrating proxy announcement: {:?}", e);
				},
			}
		}

		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::ProxyAnnouncements,
			count_good,
			count_bad,
		});

		Ok(())
	}

	pub fn do_receive_proxy_announcement(
		announcement: RcProxyAnnouncementOf<T>,
	) -> Result<(), Error<T>> {
		let before = frame_system::Account::<T>::get(&announcement.depositor);
		let missing = <T as pallet_proxy::Config>::Currency::unreserve(
			&announcement.depositor,
			announcement.deposit,
		);
		let unreserved = announcement.deposit.saturating_sub(missing);

		if !missing.is_zero() {
			log::warn!(target: LOG_TARGET, "Could not unreserve full proxy announcement deposit for {}, unreserved {:?} / {:?} since account had {:?} reserved", announcement.depositor.to_polkadot_ss58(), unreserved, &announcement.deposit, before.data.reserved);
		}

		Ok(())
	}
}

pub struct ProxyBasicChecks<T, RcProxyType> {
	_p: core::marker::PhantomData<(T, RcProxyType)>,
}

#[cfg(feature = "std")]
use std::collections::BTreeMap;

#[cfg(feature = "std")]
impl<T, RcProxyType> crate::types::AhMigrationCheck for ProxyBasicChecks<T, RcProxyType>
where
	T: Config,
	RcProxyType: Into<T::RcProxyType> + Clone + core::fmt::Debug + Encode,
{
	type RcPrePayload = BTreeMap<AccountId32, Vec<(RcProxyType, AccountId32)>>; // Map of Delegator -> (Kind, Delegatee)
	type AhPrePayload =
		BTreeMap<AccountId32, Vec<(<T as pallet_proxy::Config>::ProxyType, AccountId32)>>; // Map of Delegator -> (Kind, Delegatee)

	fn pre_check(_: Self::RcPrePayload) -> Self::AhPrePayload {
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

	fn post_check(rc_pre: Self::RcPrePayload, ah_pre: Self::AhPrePayload) {
		// We now check that the ah-post proxies are the merged version of RC pre and AH pre,
		// excluding the ones that are un-translateable.

		let mut delegators =
			rc_pre.keys().chain(ah_pre.keys()).collect::<std::collections::BTreeSet<_>>();

		for delegator in delegators {
			let ah_pre_delegations = ah_pre.get(delegator).cloned().unwrap_or_default();
			let ah_post_delegations = pallet_proxy::Proxies::<T>::get(&delegator)
				.0
				.into_iter()
				.map(|d| (d.proxy_type, d.delegate))
				.collect::<Vec<_>>();

			// All existing AH delegations are still here
			for ah_pre_d in &ah_pre_delegations {
				assert!(ah_post_delegations.contains(&ah_pre_d), "AH delegations are still available on AH for delegator: {:?}, Missing {:?} in {:?} vs {:?}", delegator.to_polkadot_ss58(), ah_pre_d, ah_pre_delegations, ah_post_delegations);
			}

			// All RC delegations that could be translated are still here
			for rc_pre_d in &rc_pre.get(delegator).cloned().unwrap_or_default() {
				let translated: T::RcProxyType = rc_pre_d.0.clone().into();
				let Ok(translated_kind) = T::RcToProxyType::try_convert(translated.clone()) else {
					// Best effort sanity checking that only Auction and ParaRegistration dont work
					#[cfg(feature = "ahm-polkadot")]
					{
						let k = translated.encode().get(0).cloned();
						assert!(
							k == Some(7) || k == Some(9),
							"Must translate all proxy Kinds except Auction and ParaRegistration"
						);
					}
					continue;
				};
				let translated = (translated_kind, rc_pre_d.1.clone()); // Account Id stays the same

				assert!(ah_post_delegations.contains(&translated), "RC delegations are still available on AH for delegator: {:?}, Missing {:?} in {:?} vs {:?}", delegator.to_polkadot_ss58(), rc_pre_d, rc_pre.get(delegator).cloned().unwrap_or_default(), ah_pre_delegations);
			}
		}
	}
}
