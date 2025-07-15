// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! The Polkadot Secretary Collective.

use crate::{xcm_config::FellowshipAdminBodyId, *};
use frame_support::{
	parameter_types,
	traits::{tokens::GetSalary, EitherOf, MapSuccess, NoOpPoll, PalletInfoAccess},
};
use frame_system::{pallet_prelude::BlockNumberFor, EnsureRootWithSuccess};
use pallet_xcm::{EnsureXcm, IsVoiceOfBody};
use sp_core::{ConstU128, ConstU32};
use sp_runtime::traits::{ConstU16, ConvertToValue, Identity, Replace};
use westend_runtime_constants::time::HOURS;

#[cfg(feature = "runtime-benchmarks")]
use crate::impls::benchmarks::OpenHrmpChannel;

use xcm::prelude::*;
use xcm_builder::{AliasesIntoAccountId32, PayOverXcm};

use self::xcm_config::UsdtAssetHub;

/// The Secretary members' ranks.
pub mod ranks {
	use pallet_ranked_collective::Rank;

	pub const SECRETARY_CANDIDATE: Rank = 0;
	pub const SECRETARY: Rank = 1;
}

/// Origins of:
/// - Root;
/// - FellowshipAdmin (i.e. token holder referendum);
/// - Plurality vote from Fellows can promote, demote, remove and approve rank retention of members
///   of the Secretary Collective (rank `2`).
type ApproveOrigin = EitherOf<
	EnsureRootWithSuccess<AccountId, ConstU16<65535>>,
	EitherOf<
		MapSuccess<
			EnsureXcm<IsVoiceOfBody<GovernanceLocation, FellowshipAdminBodyId>>,
			Replace<ConstU16<65535>>,
		>,
		MapSuccess<Fellows, Replace<ConstU16<65535>>>,
	>,
>;

pub type SecretaryCollectiveInstance = pallet_ranked_collective::Instance3;

impl pallet_ranked_collective::Config<SecretaryCollectiveInstance> for Runtime {
	type WeightInfo = ();
	type RuntimeEvent = RuntimeEvent;
	type AddOrigin = ApproveOrigin;
	type RemoveOrigin = ApproveOrigin;
	type PromoteOrigin = ApproveOrigin;
	type DemoteOrigin = ApproveOrigin;
	type ExchangeOrigin = ApproveOrigin;
	type Polls = NoOpPoll<BlockNumberFor<Runtime>>;
	type MinRankOfClass = Identity;
	type MemberSwappedHandler = crate::SecretarySalary;
	type VoteWeight = pallet_ranked_collective::Geometric;
	type MaxMemberCount = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkSetup = crate::SecretarySalary;
}

pub type SecretarySalaryInstance = pallet_salary::Instance3;

parameter_types! {
	// The interior location on AssetHub for the paying account. This is the Secretary Salary
	// pallet instance. This sovereign account will need funding.
	pub SecretarySalaryInteriorLocation: InteriorLocation = PalletInstance(<crate::SecretarySalary as PalletInfoAccess>::index() as u8).into();
}

const USDT_UNITS: u128 = 1_000_000;

/// [`PayOverXcm`] setup to pay the Secretary salary on the AssetHub in USDT.
pub type SecretarySalaryPaymaster = PayOverXcm<
	SecretarySalaryInteriorLocation,
	crate::xcm_config::XcmRouter,
	crate::PolkadotXcm,
	ConstU32<{ 6 * HOURS }>,
	AccountId,
	(),
	ConvertToValue<UsdtAssetHub>,
	AliasesIntoAccountId32<(), AccountId>,
>;

pub struct SalaryForRank;
impl GetSalary<u16, AccountId, Balance> for SalaryForRank {
	fn get_salary(rank: u16, _who: &AccountId) -> Balance {
		if rank == 1 {
			6666 * USDT_UNITS
		} else {
			0
		}
	}
}

impl pallet_salary::Config<SecretarySalaryInstance> for Runtime {
	type WeightInfo = ();
	type RuntimeEvent = RuntimeEvent;

	#[cfg(not(feature = "runtime-benchmarks"))]
	type Paymaster = SecretarySalaryPaymaster;
	#[cfg(feature = "runtime-benchmarks")]
	type Paymaster = crate::impls::benchmarks::PayWithEnsure<
		SecretarySalaryPaymaster,
		OpenHrmpChannel<ConstU32<1000>>,
	>;
	type Members = pallet_ranked_collective::Pallet<Runtime, SecretaryCollectiveInstance>;

	#[cfg(not(feature = "runtime-benchmarks"))]
	type Salary = SalaryForRank;
	#[cfg(feature = "runtime-benchmarks")]
	type Salary = frame_support::traits::tokens::ConvertRank<
		crate::impls::benchmarks::RankToSalary<Balances>,
	>;
	// 15 days to register for a salary payment.
	type RegistrationPeriod = ConstU32<{ 15 * DAYS }>;
	// 15 days to claim the salary payment.
	type PayoutPeriod = ConstU32<{ 15 * DAYS }>;
	// Total monthly salary budget.
	type Budget = ConstU128<{ 6666 * USDT_UNITS }>;
}
