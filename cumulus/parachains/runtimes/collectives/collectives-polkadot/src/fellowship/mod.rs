// Copyright 2023 Parity Technologies (UK) Ltd.
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

//! The Polkadot Technical Fellowship.

mod origins;
mod tracks;
pub use origins::{
	pallet_origins as pallet_fellowship_origins, Fellows, FellowshipCandidates, FellowshipExperts,
	FellowshipMasters,
};

use crate::{
	constants, impls::ToParentTreasury, weights, AccountId, Balance, Balances, BlockNumber,
	FellowshipReferenda, GovernanceLocation, Preimage, RelayTreasuryAccount, Runtime, RuntimeCall,
	RuntimeEvent, Scheduler, DAYS,
};
use frame_support::{
	parameter_types,
	traits::{EitherOf, MapSuccess, TryMapSuccess},
};
use pallet_xcm::{EnsureXcm, IsVoiceOfBody};
use polkadot_runtime_constants::xcm::body::FELLOWSHIP_ADMIN_INDEX;
use sp_arithmetic::traits::CheckedSub;
use sp_core::ConstU32;
use sp_runtime::{
	morph_types,
	traits::{AccountIdConversion, ConstU16, Replace, TypedGet},
};
use xcm::latest::BodyId;

use self::origins::EnsureFellowship;

/// The Fellowship members' ranks.
pub mod ranks {
	use pallet_ranked_collective::Rank;

	pub const CANDIDATES: Rank = 0;
	pub const DAN_1: Rank = 1;
	pub const DAN_2: Rank = 2;
	pub const DAN_3: Rank = 3; // aka Fellows.
	pub const DAN_4: Rank = 4;
	pub const DAN_5: Rank = 5; // aka Experts.
	pub const DAN_6: Rank = 6;
	pub const DAN_7: Rank = 7; // aka Masters.
	pub const DAN_8: Rank = 8;
	pub const DAN_9: Rank = 9;
}

parameter_types! {
	pub const AlarmInterval: BlockNumber = 1;
	pub const SubmissionDeposit: Balance = 0;
	pub const UndecidingTimeout: BlockNumber = 7 * DAYS;
	// Referenda pallet account, used to temporarily deposit slashed imbalance before teleporting.
	pub ReferendaPalletAccount: AccountId = constants::account::REFERENDA_PALLET_ID.into_account_truncating();
	pub const FellowshipAdminBodyId: BodyId = BodyId::Index(FELLOWSHIP_ADMIN_INDEX);
}

impl pallet_fellowship_origins::Config for Runtime {}

pub type FellowshipReferendaInstance = pallet_referenda::Instance1;

impl pallet_referenda::Config<FellowshipReferendaInstance> for Runtime {
	type WeightInfo = weights::pallet_referenda::WeightInfo<Runtime>;
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type Scheduler = Scheduler;
	type Currency = Balances;
	type SubmitOrigin =
		pallet_ranked_collective::EnsureMember<Runtime, FellowshipCollectiveInstance, 1>;
	type CancelOrigin = FellowshipExperts;
	type KillOrigin = FellowshipMasters;
	type Slash = ToParentTreasury<RelayTreasuryAccount, ReferendaPalletAccount, Runtime>;
	type Votes = pallet_ranked_collective::Votes;
	type Tally = pallet_ranked_collective::TallyOf<Runtime, FellowshipCollectiveInstance>;
	type SubmissionDeposit = SubmissionDeposit;
	type MaxQueued = ConstU32<100>;
	type UndecidingTimeout = UndecidingTimeout;
	type AlarmInterval = AlarmInterval;
	type Tracks = tracks::TracksInfo;
	type Preimages = Preimage;
}

pub type FellowshipCollectiveInstance = pallet_ranked_collective::Instance1;

morph_types! {
	/// A `TryMorph` implementation to reduce a scalar by a particular amount, checking for
	/// underflow.
	pub type CheckedReduceBy<N: TypedGet>: TryMorph = |r: N::Type| -> Result<N::Type, ()> {
		r.checked_sub(&N::get()).ok_or(())
	} where N::Type: CheckedSub;
}

impl pallet_ranked_collective::Config<FellowshipCollectiveInstance> for Runtime {
	type WeightInfo = weights::pallet_ranked_collective::WeightInfo<Runtime>;
	type RuntimeEvent = RuntimeEvent;
	// Promotion is by any of:
	// - Root can promote arbitrarily.
	// - the FellowshipAdmin origin (i.e. token holder referendum);
	// - a vote by the rank *above* the new rank.
	type PromoteOrigin = EitherOf<
		frame_system::EnsureRootWithSuccess<Self::AccountId, ConstU16<65535>>,
		EitherOf<
			MapSuccess<
				EnsureXcm<IsVoiceOfBody<GovernanceLocation, FellowshipAdminBodyId>>,
				Replace<ConstU16<9>>,
			>,
			TryMapSuccess<EnsureFellowship, CheckedReduceBy<ConstU16<1>>>,
		>,
	>;
	// Demotion is by any of:
	// - Root can demote arbitrarily.
	// - the FellowshipAdmin origin (i.e. token holder referendum);
	// - a vote by the rank two above the current rank.
	type DemoteOrigin = EitherOf<
		frame_system::EnsureRootWithSuccess<Self::AccountId, ConstU16<65535>>,
		EitherOf<
			MapSuccess<
				EnsureXcm<IsVoiceOfBody<GovernanceLocation, FellowshipAdminBodyId>>,
				Replace<ConstU16<9>>,
			>,
			TryMapSuccess<EnsureFellowship, CheckedReduceBy<ConstU16<2>>>,
		>,
	>;
	type Polls = FellowshipReferenda;
	type MinRankOfClass = sp_runtime::traits::Identity;
	type VoteWeight = pallet_ranked_collective::Geometric;
}
