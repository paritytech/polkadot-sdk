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

//! Call filters for Asset Hub during the Asset Hub Migration.

use crate::*;
use frame_support::traits::Contains;

/// Contains all calls that are enabled during the migration.
pub struct CallsEnabledDuringMigration;
impl Contains<<Runtime as frame_system::Config>::RuntimeCall> for CallsEnabledDuringMigration {
	fn contains(call: &<Runtime as frame_system::Config>::RuntimeCall) -> bool {
		let (during, _after) = call_allowed_status(call);
		if !during {
			log::warn!("Call bounced by the filter during the migration: {:?}", call);
		}
		during
	}
}

/// Contains all calls that are enabled after the migration.
pub struct CallsEnabledAfterMigration;
impl Contains<<Runtime as frame_system::Config>::RuntimeCall> for CallsEnabledAfterMigration {
	fn contains(call: &<Runtime as frame_system::Config>::RuntimeCall) -> bool {
		let (_during, after) = call_allowed_status(call);
		if !after {
			log::warn!("Call bounced by the filter after the migration: {:?}", call);
		}
		after
	}
}

/// Return whether a call should be enabled during and/or after the migration.
///
/// Time line of the migration looks like this:
///
/// --------|-----------|--------->
///       Start        End
///
/// We now define 2 periods:
///
/// 1. During the migration: [Start, End]
/// 2. After the migration: (End, ∞)
///
/// Visually:
///
/// ```text
///         |-----1-----|
///                      |---2---->
/// --------|-----------|--------->
///       Start        End
/// ```
///
/// This call returns a 2-tuple to indicate whether a call is enabled during these periods.
pub fn call_allowed_status(call: &<Runtime as frame_system::Config>::RuntimeCall) -> (bool, bool) {
	use RuntimeCall::*;
	const ON: bool = true;
	const OFF: bool = false;

	let during_migration = match call {
		AhMigrator(..) => ON,
		AhOps(..) => OFF,
		AssetConversion(..) => OFF,
		AssetRate(..) => OFF,
		Assets(..) => OFF,
		Balances(..) => OFF,
		CollatorSelection(..) => OFF, // TODO maybe disable them since staking is also disabled?
		ConvictionVoting(..) => OFF,
		CumulusXcm(..) => OFF, /* Empty call enum, see https://github.com/paritytech/polkadot-sdk/issues/8222 */
		FastUnstake(..) => OFF,
		ForeignAssets(..) => OFF,
		Indices(..) => OFF,
		MessageQueue(..) => ON, // TODO think about this
		MultiBlockMigrations(..) => ON,
		Multisig(..) => OFF,
		NftFractionalization(..) => OFF,
		Nfts(..) => OFF,
		NominationPools(..) => OFF,
		ParachainInfo(..) => OFF, /* Empty call enum, see https://github.com/paritytech/polkadot-sdk/issues/8222 */
		ParachainSystem(..) => ON, // Only inherent and root calls
		PolkadotXcm(..) => OFF,
		PoolAssets(..) => OFF,
		Preimage(..) => OFF,
		Proxy(..) => OFF,
		Referenda(..) => OFF,
		Scheduler(..) => OFF,
		Session(..) => OFF,
		StateTrieMigration(..) => OFF, // Deprecated
		System(..) => ON,
		Timestamp(..) => ON,
		ToRococoXcmRouter(..) => OFF,
		Treasury(..) => OFF,
		Staking(..) => OFF,
		MultiBlock(..) => OFF,
		MultiBlockVerifier(..) => OFF,
		MultiBlockUnsigned(..) => OFF,
		MultiBlockSigned(..) => OFF,
		AssetConversionMigration(..) => OFF,
		Revive(..) => OFF,
		AssetRewards(..) => OFF,
		Uniques(..) => OFF,
		Utility(..) => OFF,
		Vesting(..) => OFF,
		VoterList(..) => OFF,
		Whitelist(..) => OFF,
		XcmpQueue(..) => ON, /* Allow updating XCM settings. Only by Fellowship and root. */
		        
		
		/* Exhaustive match. Compiler ensures that we did not miss any. */
	};

	// All pallets are enabled on Asset Hub after the migration :)
	let after_migration = ON;
	(during_migration, after_migration)
}
