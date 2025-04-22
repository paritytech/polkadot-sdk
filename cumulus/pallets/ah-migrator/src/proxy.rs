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
use sp_runtime::{traits::Zero, BoundedSlice};

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
		log::debug!(target: LOG_TARGET, "Integrating proxy {}, deposit {:?}", proxy.delegator.to_polkadot_ss58(), proxy.deposit);

		let max_proxies = <T as pallet_proxy::Config>::MaxProxies::get() as usize;
		if proxy.proxies.len() > max_proxies {
			log::error!(target: LOG_TARGET, "Truncating proxy list of {}", proxy.delegator.to_ss58check());
		}

		let proxies = proxy.proxies.into_iter().enumerate().filter_map(|(i, p)| {
			let Ok(proxy_type) = T::RcToProxyType::try_convert(p.proxy_type) else {
				// This is fine, eg. `Auction` proxy is not supported on AH
				log::warn!(target: LOG_TARGET, "Dropping unsupported proxy at index {} for {}", i, proxy.delegator.to_polkadot_ss58());
				// TODO unreserve deposit
				return None;
			};
			let delay = T::RcToAhDelay::convert(p.delay);

			log::debug!(target: LOG_TARGET, "Proxy type: {:?} delegate: {:?}", proxy_type, p.delegate.to_polkadot_ss58());
			Some(pallet_proxy::ProxyDefinition {
				delegate: p.delegate,
				delay,
				proxy_type,
			})
		})
		.take(max_proxies)
		.collect::<Vec<_>>();

		let Ok(bounded_proxies) =
			BoundedSlice::try_from(proxies.as_slice()).defensive_proof("unreachable")
		else {
			return Err(Error::TODO);
		};

		// The deposit was already taken by the account migration

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

		if !missing.is_zero() {
			log::warn!(target: LOG_TARGET, "Could not unreserve full proxy announcement deposit for {}, missing {:?}, before {:?}", announcement.depositor.to_ss58check(), missing, before);
		}

		Ok(())
	}
}

#[cfg(feature = "std")]
impl<T: Config> crate::types::AhMigrationCheck for ProxyProxiesMigrator<T> {
	type RcPrePayload = usize; // number of delegators
	type AhPrePayload = (); // number of proxies

	fn pre_check(_: Self::RcPrePayload) -> Self::AhPrePayload {
		()
	}

	fn post_check(rc_pre_payload: Self::RcPrePayload, _: Self::AhPrePayload) {
		let count = pallet_proxy::Proxies::<T>::iter_keys().count();

		log::info!(target: LOG_TARGET, "Total number of proxies: {}", count);
		// TODO: This is not necessarily correct, since some proxy types are not migrated.
		// Assert storage "Proxy::Proxies::ah_post::length"
		if count < rc_pre_payload {
			panic!(
				"Some proxies were not migrated. Expected at least {} proxies, got {}",
				rc_pre_payload, count
			);
		}
	}
}
