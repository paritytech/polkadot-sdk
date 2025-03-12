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

//! First phase of the Asset Hub Migration.

use crate::*;

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

	match call {
		System(..) => (ON, ON),
		Scheduler(..) => (OFF, OFF),
		Preimage(..) => (OFF, OFF),
		Babe(..) => (ON, ON), // TODO double check
		Timestamp(..) => (ON, ON),
		Indices(..) => (OFF, OFF),
		Balances(..) => (OFF, ON),
		// TransactionPayment has no calls
		// Authorship has no calls
		Staking(..) => (OFF, OFF),
		// Offences has no calls
		// Historical has no calls
		Session(..) => (OFF, OFF),
		Grandpa(..) => (ON, ON), // TODO double check
		// AuthorityDiscovery has no calls
		Treasury(..) => (OFF, OFF),
		ConvictionVoting(..) => (OFF, OFF),
		Referenda(..) => (OFF, OFF),
		// Origins has no calls
		Whitelist(..) => (OFF, OFF),
		// Claims(..) => (OFF, OFF), // Not on Westend.
		Vesting(..) => (OFF, OFF),
		Utility(..) => (OFF, ON), // batching etc
		Proxy(..) => (OFF, ON),
		Multisig(..) => (OFF, ON),
		// Bounties(..) => (OFF, OFF), // Not on Westend.
		// ChildBounties(..) => (OFF, OFF), // Not on Westend.
		ElectionProviderMultiPhase(..) => (OFF, OFF),
		VoterList(..) => (OFF, OFF),
		NominationPools(..) => (OFF, OFF),
		FastUnstake(..) => (OFF, OFF),
		// DelegatedStaking has no calls
		// ParachainsOrigin has no calls
		Configuration(..) => (ON, ON), /* TODO allow this to be called by fellow origin during the migration https://github.com/polkadot-fellows/runtimes/pull/559#discussion_r1928794490 */
		ParasShared(..) => (OFF, OFF), /* Has no calls but a call enum https://github.com/paritytech/polkadot-sdk/blob/ee803b74056fac5101c06ec5998586fa6eaac470/polkadot/runtime/parachains/src/shared.rs#L185-L186 */
		ParaInclusion(..) => (OFF, OFF), /* Has no calls but a call enum https://github.com/paritytech/polkadot-sdk/blob/74ec1ee226ace087748f38dfeffc869cd5534ac8/polkadot/runtime/parachains/src/inclusion/mod.rs#L352-L353 */
		ParaInherent(..) => (ON, ON),    // only inherents
		// ParaScheduler has no calls
		Paras(..) => (ON, ON),
		Initializer(..) => (ON, ON),
		// Dmp has no calls and deprecated
		Hrmp(..) => (OFF, OFF),
		// ParaSessionInfo has no calls
		ParasDisputes(..) => (OFF, ON), // TODO check with security
		ParasSlashing(..) => (OFF, ON), // TODO check with security
		OnDemandAssignmentProvider(..) => (OFF, ON),
		// CoretimeAssignmentProvider has no calls
		Registrar(..) => (OFF, ON),
		Slots(..) => (OFF, OFF),
		Auctions(..) => (OFF, OFF),
		Crowdloan(
			crowdloan::Call::<Runtime>::dissolve { .. } |
			crowdloan::Call::<Runtime>::refund { .. } |
			crowdloan::Call::<Runtime>::withdraw { .. },
		) => (OFF, ON),
		Crowdloan(..) => (OFF, OFF),
		Coretime(coretime::Call::<Runtime>::request_revenue_at { .. }) => (OFF, ON),
		Coretime(..) => (ON, ON),
		// StateTrieMigration(..) => (OFF, OFF), // Not on Westend.
		XcmPallet(..) => (OFF, ON), /* TODO allow para origins and root to call this during the migration, see https://github.com/polkadot-fellows/runtimes/pull/559#discussion_r1928789463 */
		MessageQueue(..) => (ON, ON), // TODO think about this
		AssetRate(..) => (OFF, OFF),
		Beefy(..) => (OFF, ON),     /* TODO @claravanstaden @bkontur */
		Identity(..) => (OFF, OFF), // Identity pallet is still hanging around filtered on westend.
		RcMigrator(..) => (ON, ON),
		// Exhaustive match. Compiler ensures that we did not miss any.
	}
}
