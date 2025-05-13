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

//! Generic checks for Relay and AH.

use crate::porting_prelude::*;

use frame_support::{
	pallet_prelude::*,
	traits::{Currency, Defensive},
};
use frame_system::pallet_prelude::*;
use pallet_ah_migrator::types::AhMigrationCheck;
use pallet_rc_migrator::types::{RcMigrationCheck, ToPolkadotSs58};
use sp_runtime::{
	traits::{Dispatchable, TryConvert},
	AccountId32,
};
use std::{collections::BTreeMap, str::FromStr};

pub struct SanityChecks;

impl RcMigrationCheck for SanityChecks {
	type RcPrePayload = ();

	fn pre_check() -> Self::RcPrePayload {
		assert!(
			pallet_rc_migrator::RcMigrationStage::<RcRuntime>::get() ==
				pallet_rc_migrator::MigrationStage::Scheduled { block_number: 0 }
		);
	}

	fn post_check(_: Self::RcPrePayload) {
		assert!(
			pallet_rc_migrator::RcMigrationStage::<RcRuntime>::get() ==
				pallet_rc_migrator::MigrationStage::MigrationDone
		);
	}
}

impl AhMigrationCheck for SanityChecks {
	type RcPrePayload = ();
	type AhPrePayload = ();

	fn pre_check(_: Self::RcPrePayload) -> Self::AhPrePayload {
		assert!(
			pallet_ah_migrator::AhMigrationStage::<AhRuntime>::get() ==
				pallet_ah_migrator::MigrationStage::Pending
		);
	}

	fn post_check(rc_pre_payload: Self::RcPrePayload, _: Self::AhPrePayload) {
		assert!(
			pallet_ah_migrator::AhMigrationStage::<AhRuntime>::get() ==
				pallet_ah_migrator::MigrationStage::MigrationDone
		);
	}
}
