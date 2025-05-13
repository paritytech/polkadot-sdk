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

use super::Permission;
use crate::porting_prelude::*;

use super::ProxyBasicWorks;
use frame_support::{
	pallet_prelude::*,
	traits::{Currency, Defensive},
};
use frame_system::pallet_prelude::*;
use hex_literal::hex;
use pallet_ah_migrator::types::AhMigrationCheck;
use pallet_rc_migrator::types::{RcMigrationCheck, ToPolkadotSs58};
use sp_runtime::{
	traits::{Dispatchable, TryConvert},
	AccountId32,
};
use std::{collections::BTreeMap, str::FromStr};

type RelayRuntime = polkadot_runtime::Runtime;
type AssetHubRuntime = asset_hub_polkadot_runtime::Runtime;

/// Whale accounts that have a lot of proxies. We double-check those to make sure that all is well.
///
/// We also store the number of proxies.
const WHALES: &[(AccountId32, usize)] = &[
	(AccountId32::new(hex!("d10577dd7d364b294d2e9a0768363ac885efb8b1c469da6c4f2141d4f6560c1f")), 7),
	(AccountId32::new(hex!("6c1b752375304917c15af9c2e7a4426b3af513054d89f6c7bb26cd7e30e4413e")), 7),
	(AccountId32::new(hex!("9561809d76c46eaad3f19d2d392e0a4962086ce116a8739fe7d458bdc3bd4f1d")), 6),
	(AccountId32::new(hex!("429b067ff314c1fed75e57fcf00a6a4ff8611268e75917b5744ac8c4e1810d17")), 6),
];

const MILLION_DOT: polkadot_primitives::Balance =
	crate::porting_prelude::RC_DOLLARS * 1_000 * 1_000;

/// Proxy accounts can still be controlled by their delegates with the correct permissions.
///
/// This tests the actual functionality, not the raw data. It does so by dispatching calls from the
/// delegatee account on behalf of the delegator. It then checks for whether or not the correct
/// events were emitted.
pub struct ProxyWhaleWatching;

impl RcMigrationCheck for ProxyWhaleWatching {
	type RcPrePayload = ();

	fn pre_check() -> Self::RcPrePayload {
		// All whales alive
		for (whale, num_proxies) in WHALES {
			let acc = frame_system::Account::<RelayRuntime>::get(whale);
			assert!(acc.nonce == 0, "Whales are pure");
			assert!(
				acc.data.free + acc.data.reserved >= MILLION_DOT,
				"Whales are rich on the relay"
			);

			let delegations = pallet_proxy::Proxies::<RelayRuntime>::get(&whale).0;
			assert_eq!(
				delegations.len(),
				*num_proxies,
				"Number of proxies is correct for whale {:?}",
				whale
			);
		}
	}

	fn post_check(_: Self::RcPrePayload) {}
}

impl AhMigrationCheck for ProxyWhaleWatching {
	type RcPrePayload = ();
	type AhPrePayload = ();

	fn pre_check(_: Self::RcPrePayload) -> Self::AhPrePayload {}

	fn post_check(rc_pre_payload: Self::RcPrePayload, _: Self::AhPrePayload) {
		// Whales still afloat
		for (whale, num_proxies) in WHALES {
			let acc = frame_system::Account::<AssetHubRuntime>::get(whale);
			assert!(
				acc.data.free + acc.data.reserved >= MILLION_DOT,
				"Whales are rich on the asset hub"
			);

			let delegations = pallet_proxy::Proxies::<AssetHubRuntime>::get(&whale).0;
			assert_eq!(delegations.len(), *num_proxies, "Number of proxies is correct");

			for delegation in delegations.iter() {
				// We need to take the superset of the permissions here. Not that this means that we
				// will test the delegatee multiple times, but it should not matter and the code is
				// easier that to mess around with maps.
				let permissions = delegations
					.iter()
					.filter(|d| d.delegate == delegation.delegate)
					.map(|d|
						// The translation could fail at any point, but for now it seems to hold.
						Permission::try_convert(d.proxy_type).expect("Whale proxies must translate"))
					.collect::<Vec<_>>();

				ProxyBasicWorks::check_proxy(
					&delegation.delegate,
					whale,
					&permissions,
					delegation.delay,
				);
			}
		}
	}
}
