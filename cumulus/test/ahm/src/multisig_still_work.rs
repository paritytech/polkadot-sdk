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

//! Test that Multisig Account IDs result in the same IDs and they can still dispatch calls.

use crate::porting_prelude::*;

use frame_support::{
	pallet_prelude::*,
	traits::{Currency, Defensive},
};
use frame_system::pallet_prelude::*;
use pallet_ah_migrator::types::AhMigrationCheck;
use pallet_rc_migrator::types::{RcMigrationCheck, ToPolkadotSs58};
use rand::prelude::*;
use sp_runtime::{
	traits::{Dispatchable, TryConvert},
	AccountId32,
};
use std::{collections::BTreeMap, str::FromStr};

type RelayRuntime = polkadot_runtime::Runtime;
type AssetHubRuntime = asset_hub_polkadot_runtime::Runtime;

pub struct MultisigStillWork;

#[derive(Clone)]
pub struct Multisig<AccountId> {
	pub signatories: Vec<AccountId>,
	pub threshold: usize,
	pub pure: AccountId,
}

pub type MultisigOf<T> = Multisig<<T as frame_system::Config>::AccountId>;

impl RcMigrationCheck for MultisigStillWork {
	type RcPrePayload = Vec<MultisigOf<RelayRuntime>>;

	fn pre_check() -> Self::RcPrePayload {
		// We generate 100 multisigs consisting of between 1 and 10 signatories.
		// Just use the first 1000 accs to make the generation a bit faster.
		let accounts = frame_system::Account::<RelayRuntime>::iter()
			.take(1000)
			.map(|(_id, a)| a.data)
			.collect::<Vec<_>>();
		let mut multisigs = Vec::new();
		//let mut rng = rand::rng();

		for i in 0..100 {
			//let num_signatories = rng.gen_range(1..=10);
			//let signatories = TODO
		}

		multisigs
	}

	fn post_check(_: Self::RcPrePayload) {}
}

impl AhMigrationCheck for MultisigStillWork {
	type RcPrePayload = Vec<MultisigOf<AssetHubRuntime>>;
	type AhPrePayload = ();

	fn pre_check(_: Self::RcPrePayload) -> Self::AhPrePayload {}
	fn post_check(_rc_pre: Self::RcPrePayload, _: Self::AhPrePayload) {}
}
