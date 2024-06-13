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
//!
//! The module defines the following on-chain functionality of the Secretary Program:
//!
//! - Managed set of program members, where every member is at the same rank
//! (via [SecretaryCollective](pallet_ranked_collective)).
//! - Referendum functionality for the program members to propose, vote on, and execute
//! proposals on behalf of the program (via [SecretaryReferenda](pallet_referenda)).
//! - Promotion and demotion periods, register of members' activity, and rank based salaries
//! (via [SecretaryCore](pallet_core_fellowship)).
//! - Members' salaries (via [SecretarySalary](pallet_salary), requiring a member to be
//! imported or inducted into [SecretaryCore](pallet_core_fellowship)).
pub mod origins;
mod tracks;

use super::*;
use crate::{
	fellowship::{ranks::DAN_3, FellowshipCollectiveInstance},
	xcm_config::{FellowshipAdminBodyId, LocationToAccountId, WndAssetHub},
};
use frame_support::traits::{tokens::GetSalary, EitherOf, EitherOfDiverse, MapSuccess};
use frame_system::EnsureRootWithSuccess;
pub use origins::pallet_origins as pallet_secretary_origins;
use origins::pallet_origins::Secretary;
use sp_core::ConstU128;
use sp_runtime::traits::{ConstU16, ConvertToValue, Identity, Replace, ReplaceWithDefault};
use xcm::prelude::*;
use xcm_builder::{AliasesIntoAccountId32, PayOverXcm};

/// The Secretary members' ranks.
pub mod ranks {
	use pallet_ranked_collective::Rank;

	#[allow(dead_code)]
	pub const SECRETARY_CANDIDATE: Rank = 0;
	pub const SECRETARY: Rank = 1;
}

/// Origins of:
/// - Root;
/// - FellowshipAdmin (i.e. token holder referendum);
/// - Plurarity vote from Fellows can promote, demote, remove and approve rank retention
/// of members of the Secretary Collective (rank `2`).
type ApproveOrigin = EitherOf<
	EnsureRootWithSuccess<AccountId, ConstU16<65535>>,
	MapSuccess<Fellows, Replace<ConstU16<2>>>,
>;

/// Origins of:
/// - Root;
/// - FellowshipAdmin (i.e. token holder referendum);
/// - Secretary;
/// - Plurarity vote from Fellows can kill and cancel proposals.
/// of members of the Secretary Collective.
type OpenGovOrSecretaryOrFellow = EitherOfDiverse<
	EitherOfDiverse<EnsureRoot<AccountId>, Fellows>,
	EitherOfDiverse<Secretary, EnsureXcm<IsVoiceOfBody<GovernanceLocation, FellowshipAdminBodyId>>>,
>;

/// Origins of:
/// - Root;
/// - FellowshipAdmin (i.e. token holder referendum);
/// - Plurarity vote from Fellows can exchange origins and
/// - configure the parameters that govern the Collective.
type OpenGovOrFellow = EitherOfDiverse<
	EnsureRoot<AccountId>,
	EitherOfDiverse<Fellows, EnsureXcm<IsVoiceOfBody<GovernanceLocation, FellowshipAdminBodyId>>>,
>;

impl pallet_secretary_origins::Config for Runtime {}

pub type SecretaryReferendaInstance = pallet_referenda::Instance3;

impl pallet_referenda::Config<SecretaryReferendaInstance> for Runtime {
	type WeightInfo = (); // TODO weights::pallet_referenda_secretary_referenda::WeightInfo<Runtime>;
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type Scheduler = Scheduler;
	type Currency = Balances;
	// Secretary collective can submit proposals.
	type SubmitOrigin = pallet_ranked_collective::EnsureMember<
		Runtime,
		SecretaryCollectiveInstance,
		{ ranks::SECRETARY },
	>;
	type CancelOrigin = OpenGovOrSecretaryOrFellow;
	type KillOrigin = OpenGovOrSecretaryOrFellow;
	type Slash = ToParentTreasury<WestendTreasuryAccount, LocationToAccountId, Runtime>;
	type Votes = pallet_ranked_collective::Votes;
	type Tally = pallet_ranked_collective::TallyOf<Runtime, SecretaryCollectiveInstance>;
	type SubmissionDeposit = ConstU128<0>;
	type MaxQueued = ConstU32<100>;
	type UndecidingTimeout = ConstU32<{ 7 * DAYS }>;
	type AlarmInterval = ConstU32<1>;
	type Tracks = tracks::TracksInfo;
	type Preimages = Preimage;
}

pub type SecretaryCollectiveInstance = pallet_ranked_collective::Instance3;

impl pallet_ranked_collective::Config<SecretaryCollectiveInstance> for Runtime {
	type WeightInfo = (); // TODO weights::pallet_ranked_collective_secretary_collective::WeightInfo<Runtime>;
	type RuntimeEvent = RuntimeEvent;
	type AddOrigin = MapSuccess<Self::PromoteOrigin, ReplaceWithDefault<()>>;
	type RemoveOrigin = ApproveOrigin;
	type PromoteOrigin = ApproveOrigin;
	type DemoteOrigin = ApproveOrigin;
	type ExchangeOrigin = OpenGovOrFellow;
	type Polls = SecretaryReferenda;
	type MinRankOfClass = Identity;
	type MemberSwappedHandler = (crate::SecretaryCore, crate::SecretarySalary);
	type VoteWeight = pallet_ranked_collective::Geometric;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkSetup = (crate::SecretaryCore, crate::SecretarySalary);
}

pub type SecretaryCoreInstance = pallet_core_fellowship::Instance3;

impl pallet_core_fellowship::Config<SecretaryCoreInstance> for Runtime {
	type WeightInfo = (); // TODO weights::pallet_core_fellowship_secretary_core::WeightInfo<Runtime>;
	type RuntimeEvent = RuntimeEvent;
	type Members = pallet_ranked_collective::Pallet<Runtime, SecretaryCollectiveInstance>;
	type Balance = Balance;
	type ParamsOrigin = OpenGovOrFellow;
	// Induction (creating a candidate) is by any of:
	// - Root;
	// - the FellowshipAdmin origin (i.e. token holder referendum);
	// - a single member of the Fellowship Program (DAN III);
	// - a single member of the Secretary Program.
	type InductOrigin = EitherOfDiverse<
		EnsureRoot<AccountId>,
		EitherOfDiverse<
			pallet_ranked_collective::EnsureMember<
				Runtime,
				FellowshipCollectiveInstance,
				{ DAN_3 },
			>,
			pallet_ranked_collective::EnsureMember<
				Runtime,
				SecretaryCollectiveInstance,
				{ ranks::SECRETARY },
			>,
		>,
	>;
	type ApproveOrigin = ApproveOrigin;
	type PromoteOrigin = ApproveOrigin;
	type EvidenceSize = ConstU32<65536>;
	type MaxRank = ConstU32<1>;
}

pub type SecretarySalaryInstance = pallet_salary::Instance3;

parameter_types! {
	// The interior location on AssetHub for the paying account. This is the Secretary Salary
	// pallet instance. This sovereign account will need funding.
	pub SecretarySalaryInteriorLocation: InteriorLocation = PalletInstance(94).into();
}

/// [`PayOverXcm`] setup to pay the Secretary salary on the AssetHub in USDT.
pub type SecretarySalaryPaymaster = PayOverXcm<
	SecretarySalaryInteriorLocation,
	crate::xcm_config::XcmRouter,
	crate::PolkadotXcm,
	ConstU32<{ 6 * HOURS }>,
	AccountId,
	(),
	ConvertToValue<WndAssetHub>,
	AliasesIntoAccountId32<(), AccountId>,
>;

pub struct SalaryForRank;
impl GetSalary<u16, AccountId, Balance> for SalaryForRank {
	fn get_salary(rank: u16, _who: &AccountId) -> Balance {
		if rank == 1 {
			1000
		} else {
			0
		}
	}
}

impl pallet_salary::Config<SecretarySalaryInstance> for Runtime {
	type WeightInfo = (); // TODO weights::pallet_salary_secretary_salary::WeightInfo<Runtime>;
	type RuntimeEvent = RuntimeEvent;

	#[cfg(not(feature = "runtime-benchmarks"))]
	type Paymaster = SecretarySalaryPaymaster;
	#[cfg(feature = "runtime-benchmarks")]
	type Paymaster = crate::impls::benchmarks::PayWithEnsure<
		SecretarySalaryPaymaster,
		crate::impls::benchmarks::OpenHrmpChannel<ConstU32<1000>>,
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
	type Budget = ConstU128<{ 10_000 * DOLLARS }>;
}
