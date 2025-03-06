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
// along with Polkadot. If not, see <http://www.gnu.org/licenses/>.

//! New governance configurations for the D-Day parachain rescue (primary AssetHub) scenario.

pub mod prover;
mod tracks;

use super::fellowship::{ranks, Architects, FellowshipCollectiveInstance, Masters};
use super::*;
use crate::dday::prover::{types::KnownAssetHubHead, AssetHubAccountProver, AssetHubStateProvider};
use crate::dday::tracks::TrackId;
use frame_support::parameter_types;
use frame_support::traits::{EitherOf, PollStatus, Polling};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_referenda::ReferendumIndex;
use sp_runtime::DispatchError;

parameter_types! {
	/// If the last AssetHub block update is older than this, we consider AssetHub stalled.
	pub storage StalledAssetHubBlockThreshold: BlockNumber = 6 * HOURS;

	/// We update this when we receive a new data from AssetHub.
	pub storage LastKnownAssetHubHead: Option<KnownAssetHubHead> = None;

	/// Returns true if the last AssetHub block update is too old (`StalledAssetHubBlockThreshold`).
	pub IsAssetHubStalled: bool = match LastKnownAssetHubHead::get() {
		Some(head) => {
			let now = System::block_number();
			let threshold = now.saturating_sub(StalledAssetHubBlockThreshold::get());
			head.known_at < threshold
		},
		None => false,
	};
}

// TODO: FAIL-CI - check constants
parameter_types! {
	pub const AlarmInterval: BlockNumber = 1;
	pub const SubmissionDeposit: Balance = 1 * 3 * CENTS;
	pub const UndecidingTimeout: BlockNumber = 14 * DAYS;
}

/// Wrapper implementation of `Polling` over `DDayReferenda`, allowing voting only when `IsAssetHubStalled == true`.
pub struct AllowPollingWhenAssetHubIsStalled;
impl AllowPollingWhenAssetHubIsStalled {
	fn is_stalled() -> bool {
		IsAssetHubStalled::get()
	}
}
impl Polling<pallet_referenda::TallyOf<Runtime, DDayReferendaInstance>>
	for AllowPollingWhenAssetHubIsStalled
{
	type Index = ReferendumIndex;
	type Votes = pallet_referenda::VotesOf<Runtime, DDayReferendaInstance>;
	type Class = TrackId;
	type Moment = BlockNumberFor<Runtime>;

	fn classes() -> Vec<Self::Class> {
		DDayReferenda::classes()
	}

	fn as_ongoing(
		index: Self::Index,
	) -> Option<(pallet_referenda::TallyOf<Runtime, DDayReferendaInstance>, Self::Class)> {
		if Self::is_stalled() {
			DDayReferenda::as_ongoing(index)
		} else {
			None
		}
	}

	fn access_poll<R>(
		index: Self::Index,
		f: impl FnOnce(
			PollStatus<
				&mut pallet_referenda::TallyOf<Runtime, DDayReferendaInstance>,
				Self::Moment,
				Self::Class,
			>,
		) -> R,
	) -> R {
		DDayReferenda::access_poll(index, |poll_status| {
			if Self::is_stalled() {
				f(poll_status)
			} else {
				f(PollStatus::None)
			}
		})
	}

	fn try_access_poll<R>(
		index: Self::Index,
		f: impl FnOnce(
			PollStatus<
				&mut pallet_referenda::TallyOf<Runtime, DDayReferendaInstance>,
				Self::Moment,
				Self::Class,
			>,
		) -> Result<R, DispatchError>,
	) -> Result<R, DispatchError> {
		if Self::is_stalled() {
			DDayReferenda::try_access_poll(index, f)
		} else {
			Err(DispatchError::Unavailable)
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn create_ongoing(class: Self::Class) -> Result<Self::Index, ()> {
		DDayReferenda::create_ongoing(class)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn end_ongoing(index: Self::Index, approved: bool) -> Result<(), ()> {
		DDayReferenda::end_ongoing(index, approved)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn max_ongoing() -> (Self::Class, u32) {
		DDayReferenda::max_ongoing()
	}
}

/// Setup voting by AssetHub account proofs.
pub type DDayVotingInstance = pallet_proofs_voting::Instance1;
impl pallet_proofs_voting::Config<DDayVotingInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	// TODO: FAIL-CI - setup/generate benchmarks
	type WeightInfo = pallet_proofs_voting::weights::SubstrateWeight<Self>;
	type Polls = AllowPollingWhenAssetHubIsStalled;
	// Get total issuance from the synced `LastKnownAssetHubHead`.
	type MaxTurnout = AssetHubStateProvider<LastKnownAssetHubHead>;
	type MaxVotes = ConstU32<3>;
	type BlockNumberProvider = System;

	type Prover = AssetHubAccountProver;
	type ProofRootProvider = AssetHubStateProvider<LastKnownAssetHubHead>;
}

/// Rank3+ member can start DDay referendum.
pub type DDayReferendaInstance = pallet_referenda::Instance3;
impl pallet_referenda::Config<DDayReferendaInstance> for Runtime {
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_referenda_dday::WeightInfo<Self>;
	type Scheduler = Scheduler;
	type Currency = Balances;
	/// Only rank3+ can start referendum
	type SubmitOrigin = pallet_ranked_collective::EnsureMember<
		Runtime,
		FellowshipCollectiveInstance,
		{ ranks::DAN_3 },
	>;
	/// Only rank4+ can cancel/kill referendum
	type CancelOrigin = EitherOf<Architects, Masters>;
	type KillOrigin = EitherOf<Architects, Masters>;
	type Slash = ToParentTreasury<WestendTreasuryAccount, LocationToAccountId, Runtime>;
	type Votes = pallet_proofs_voting::VotesOf<Runtime, DDayVotingInstance>;
	type Tally = pallet_proofs_voting::TallyOf<Runtime, DDayVotingInstance>;
	type SubmissionDeposit = SubmissionDeposit;
	type MaxQueued = ConstU32<2>;
	type UndecidingTimeout = UndecidingTimeout;
	type AlarmInterval = AlarmInterval;
	type Tracks = tracks::TracksInfo;
	type Preimages = Preimage;
	type BlockNumberProvider = System;
}
