// Copyright (C) 2022 Parity Technologies (UK) Ltd.
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

//! The Ambassador Fellowship.
//!
//! The module defines the following on-chain functionality of the Ambassador Fellowship:
//!
//! - Managed set of fellowship members, where every member has a [rank](ranks)
//! (via [AmbassadorCollective](pallet_ranked_collective)).
//! - Referendum functionality for the program members to propose, vote on, and execute
//! proposals on behalf of the members of a certain [rank](Origin)
//! (via [AmbassadorReferenda](pallet_referenda)).
//! - Managed content (charter, announcements) (via [pallet_collective_content]).
//! - Promotion and demotion periods, register of members' activity, and rank based salaries
//! (via [AmbassadorCore](pallet_core_fellowship)).
//! - Members' salaries (via [AmbassadorSalary](pallet_salary), requiring a member to be
//! imported or inducted into [AmbassadorCore](pallet_core_fellowship)).
//! - Optimistic funding mechanism for fellowship initiatives.

pub mod origins;
mod tracks;

// Re-export types for external use
pub use origins::pallet_origins::{
    EnsureAmbassador, EnsureAmbassadorsVoice, EnsureAmbassadorsVoiceFrom,
    EnsureCanDemoteTo, EnsureCanPromoteTo, EnsureCanRetainAt,
    EnsureGlobalHeadAmbassadorsVoice, Origin,
};
pub use origins::pallet_origins as pallet_ambassador_origins;

use super::*;
use crate::xcm_config::{FellowshipAdminBodyId, LocationToAccountId, WndAssetHub};
use frame_support::traits::{EitherOf, MapSuccess, TryMapSuccess};
use frame_support::traits::tokens::GetSalary;
use frame_system::EnsureRootWithSuccess;
use pallet_optimistic_funding;
use sp_core::ConstU128;
use sp_runtime::traits::{CheckedReduceBy, ConstU16, ConvertToValue, Replace, ReplaceWithDefault};
use xcm::prelude::*;
use xcm_builder::{AliasesIntoAccountId32, PayOverXcm};

/// The Ambassador Fellowship's member ranks.
pub mod ranks {
	use pallet_ranked_collective::Rank;

	// Preliminary Ranks
	#[allow(dead_code)]
	pub const ADVOCATE_AMBASSADOR: Rank = 0;

	// Tier A: Learners
	pub const ASSOCIATE_AMBASSADOR: Rank = 1;
	pub const LEAD_AMBASSADOR: Rank = 2;

	// Tier B: Engagers
	pub const SENIOR_AMBASSADOR: Rank = 3;
	pub const PRINCIPAL_AMBASSADOR: Rank = 4;

	// Tier C: Drivers
	pub const GLOBAL_AMBASSADOR: Rank = 5;
	pub const GLOBAL_HEAD_AMBASSADOR: Rank = 6;
}

impl pallet_ambassador_origins::Config for Runtime {}

pub type AmbassadorCollectiveInstance = pallet_ranked_collective::Instance2;

/// Demotion is by any of:
/// - Root can demote arbitrarily.
/// - the FellowshipAdmin origin (i.e. token holder referendum);
/// - a senior members vote by the rank two above the current rank.
pub type DemoteOrigin = EitherOf<
	frame_system::EnsureRootWithSuccess<AccountId, ConstU16<65535>>,
	EitherOf<
		MapSuccess<
			EnsureXcm<IsVoiceOfBody<GovernanceLocation, FellowshipAdminBodyId>>,
			Replace<ConstU16<{ ranks::GLOBAL_HEAD_AMBASSADOR }>>,
		>,
		TryMapSuccess<
			EnsureAmbassadorsVoiceFrom<ConstU16<{ ranks::SENIOR_AMBASSADOR }>>,
			CheckedReduceBy<ConstU16<2>>,
		>,
	>,
>;

/// Promotion and approval (rank-retention) is by any of:
/// - Root can promote arbitrarily.
/// - the FellowshipAdmin origin (i.e. token holder referendum);
/// - a senior members vote by the rank two above the new/current rank.
/// - a member of rank `5` or above can add a candidate (rank `0`).
pub type PromoteOrigin = EitherOf<
	DemoteOrigin,
	TryMapSuccess<
		pallet_ranked_collective::EnsureMember<
			Runtime,
			AmbassadorCollectiveInstance,
			{ ranks::GLOBAL_AMBASSADOR },
		>,
		Replace<ConstU16<0>>,
	>,
>;

/// Exchange is by any of:
/// - Root can exchange arbitrarily.
/// - the Fellows origin
pub type ExchangeOrigin = EitherOf<EnsureRootWithSuccess<AccountId, ConstU16<65535>>, Fellows>;

impl pallet_ranked_collective::Config<AmbassadorCollectiveInstance> for Runtime {
	type WeightInfo = weights::pallet_ranked_collective_ambassador_collective::WeightInfo<Runtime>;
	type RuntimeEvent = RuntimeEvent;
	type AddOrigin = MapSuccess<Self::PromoteOrigin, ReplaceWithDefault<()>>;
	type PromoteOrigin = PromoteOrigin;
	type DemoteOrigin = DemoteOrigin;
	type RemoveOrigin = Self::DemoteOrigin;
	type ExchangeOrigin = ExchangeOrigin;
	type Polls = AmbassadorReferenda;
	type MinRankOfClass = sp_runtime::traits::Identity;
	type MemberSwappedHandler = (crate::AmbassadorCore, crate::AmbassadorSalary);
	type VoteWeight = pallet_ranked_collective::Linear;
	type MaxMemberCount = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkSetup = (crate::AmbassadorCore, crate::AmbassadorSalary);
}

parameter_types! {
	pub const AlarmInterval: BlockNumber = 1;
	pub const SubmissionDeposit: Balance = 0;
	pub const UndecidingTimeout: BlockNumber = 7 * DAYS;
}

pub type AmbassadorReferendaInstance = pallet_referenda::Instance2;

impl pallet_referenda::Config<AmbassadorReferendaInstance> for Runtime {
	type WeightInfo = weights::pallet_referenda_ambassador_referenda::WeightInfo<Runtime>;
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type Scheduler = Scheduler;
	type Currency = Balances;
	// A proposal can be submitted by a member of the Ambassador Fellowship of
	// [ranks::SENIOR_AMBASSADOR] rank or higher.
	type SubmitOrigin = pallet_ranked_collective::EnsureMember<
		Runtime,
		AmbassadorCollectiveInstance,
		{ ranks::SENIOR_AMBASSADOR },
	>;
	type CancelOrigin = EitherOf<EnsureRoot<AccountId>, EnsureGlobalHeadAmbassadorsVoice>;
	type KillOrigin = EitherOf<EnsureRoot<AccountId>, EnsureGlobalHeadAmbassadorsVoice>;
	type Slash = ToParentTreasury<WestendTreasuryAccount, LocationToAccountId, Runtime>;
	type Votes = pallet_ranked_collective::Votes;
	type Tally = pallet_ranked_collective::TallyOf<Runtime, AmbassadorCollectiveInstance>;
	type SubmissionDeposit = SubmissionDeposit;
	type MaxQueued = ConstU32<20>;
	type UndecidingTimeout = UndecidingTimeout;
	type AlarmInterval = AlarmInterval;
	type Tracks = tracks::TracksInfo;
	type Preimages = Preimage;
	type BlockNumberProvider = System;
}

parameter_types! {
	pub const AnnouncementLifetime: BlockNumber = 180 * DAYS;
	pub const MaxAnnouncements: u32 = 50;
}

pub type AmbassadorContentInstance = pallet_collective_content::Instance1;

impl pallet_collective_content::Config<AmbassadorContentInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type CharterOrigin = EitherOf<EnsureRoot<AccountId>, EnsureGlobalHeadAmbassadorsVoice>;
	type AnnouncementLifetime = AnnouncementLifetime;
	// An announcement can be submitted by a Senior Ambassador member or an ambassador plurality
	// voice taken via referendum.
	type AnnouncementOrigin = EitherOfDiverse<
		pallet_ranked_collective::EnsureMember<
			Runtime,
			AmbassadorCollectiveInstance,
			{ ranks::SENIOR_AMBASSADOR },
		>,
		EnsureAmbassadorsVoice,
	>;
	type MaxAnnouncements = MaxAnnouncements;
	type WeightInfo = weights::pallet_collective_content::WeightInfo<Runtime>;
}

pub type AmbassadorCoreInstance = pallet_core_fellowship::Instance2;

impl pallet_core_fellowship::Config<AmbassadorCoreInstance> for Runtime {
	type WeightInfo = weights::pallet_core_fellowship_ambassador_core::WeightInfo<Runtime>;
	type RuntimeEvent = RuntimeEvent;
	type Members = pallet_ranked_collective::Pallet<Runtime, AmbassadorCollectiveInstance>;
	type Balance = Balance;
	// Parameters are set by any of:
	// - Root;
	// - the FellowshipAdmin origin (i.e. token holder referendum);
	// - a vote among all Global Head Ambassadors.
	type ParamsOrigin = EitherOfDiverse<
		EnsureRoot<AccountId>,
		EitherOfDiverse<
			EnsureXcm<IsVoiceOfBody<GovernanceLocation, FellowshipAdminBodyId>>,
			EnsureGlobalHeadAmbassadorsVoice,
		>,
	>;
	// Induction (adding a member at rank 0) is by any of:
	// - Root;
	// - the FellowshipAdmin origin (i.e. token holder referendum);
	// - a vote among all Global Ambassadors.
	type InductOrigin = EitherOfDiverse<
		EnsureRoot<AccountId>,
		EitherOfDiverse<
			EnsureXcm<IsVoiceOfBody<GovernanceLocation, FellowshipAdminBodyId>>,
			EnsureAmbassadorsVoiceFrom<ConstU16<{ ranks::GLOBAL_AMBASSADOR }>>,
		>,
	>;
	// Approval (retention of rank) is by any of:
	// - Root;
	// - the FellowshipAdmin origin (i.e. token holder referendum);
	// - a vote among all members of rank two above the candidate.
	type ApproveOrigin = PromoteOrigin;
	// Promotion is by any of:
	// - Root;
	// - the FellowshipAdmin origin (i.e. token holder referendum);
	// - a vote among all members of rank two above the candidate.
	type PromoteOrigin = PromoteOrigin;
	type FastPromoteOrigin = PromoteOrigin;
	type MaxRank = ConstU32<9>;
	type EvidenceSize = ConstU32<65536>;
}

parameter_types! {
	// The interior location on AssetHub for the paying account. This is the Ambassador Salary
	// pallet instance (which sits at index 74). This sovereign account will need funding.
	pub AmbassadorSalaryLocation: InteriorLocation = PalletInstance(74).into();
}

pub type AmbassadorSalaryInstance = pallet_salary::Instance2;

pub struct SalaryForRank;
impl GetSalary<pallet_ranked_collective::Rank, sp_runtime::AccountId32, u128> for SalaryForRank {
	fn get_salary(a: pallet_ranked_collective::Rank, _: &sp_runtime::AccountId32) -> u128 {
		u128::from(a) * 1000 * DOLLARS
	}
}

impl pallet_salary::Config<AmbassadorSalaryInstance> for Runtime {
	type WeightInfo = weights::pallet_salary_ambassador_salary::WeightInfo<Runtime>;
	type RuntimeEvent = RuntimeEvent;
	type Paymaster = PayOverXcm<
		AmbassadorSalaryLocation, // Interior
		crate::xcm_config::XcmRouter, // Router
		crate::PolkadotXcm,  // Querier
		// crate::xcm_config::LocalAssetTransactor, // Querier
		ConstU32<{ 6 * HOURS }>,
		// LocationToAccountId, // BeneficiaryRefToLocation
		AccountId,
		(), // AssetKind
		ConvertToValue<WndAssetHub>,  // AssetKindToLocatableAsset
		AliasesIntoAccountId32<(), AccountId>,  // BeneficiaryRefToLocation
	>;
	// The salary that determines how much salary each rank receives.
	type Salary = SalaryForRank;
	// The total budget is 1000 WND per cycle.
	type Budget = ConstU128<{ 1_000 * DOLLARS }>;
	// The payment is made to the registered identity of the member.
	type Members = pallet_ranked_collective::Pallet<Runtime, AmbassadorCollectiveInstance>;
	// 15 days to register for a salary payment.
	type RegistrationPeriod = ConstU32<{ 15 * DAYS }>;
	// 15 days to claim the salary payment.
	type PayoutPeriod = ConstU32<{ 15 * DAYS }>;
}

// Optimistic Funding parameters
parameter_types! {
	pub const FundingPeriod: BlockNumber = 28 * DAYS;
	pub const MinimumRequestAmount: Balance = 10 * DOLLARS;
	pub const MaximumRequestAmount: Balance = 1_000 * DOLLARS;
	pub const RequestDeposit: Balance = 1 * DOLLARS;
	pub const MaxActiveRequests: u32 = 100;
	pub const OptimisticFundingPalletId: PalletId = PalletId(*b"opt/fund");
}

pub type AmbassadorOptimisticFundingInstance = pallet_optimistic_funding::Instance1;

impl pallet_optimistic_funding::Config<AmbassadorOptimisticFundingInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type FundingPeriod = FundingPeriod;
	type MinimumRequestAmount = MinimumRequestAmount;
	type MaximumRequestAmount = MaximumRequestAmount;
	type RequestDeposit = RequestDeposit;
	type MaxActiveRequests = MaxActiveRequests;
	type TreasuryOrigin = EnsureGlobalHeadAmbassadorsVoice;
	type WeightInfo = pallet_optimistic_funding::weights::SubstrateWeight<Runtime>;
	type PalletId = OptimisticFundingPalletId;
}
