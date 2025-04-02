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
use crate::dday::prover::{AssetHubAccountProver, StalledAssetHubDataProvider};
use crate::dday::tracks::TrackId;
use frame_support::{
	parameter_types,
	traits::{CallerTrait, ContainsPair, EitherOf, Equals, PollStatus, Polling},
};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_dday_detection::IsStalled;
use pallet_dday_voting::ProofBlockNumberOf;
use pallet_referenda::ReferendumIndex;
use sp_runtime::DispatchError;

parameter_types! {
	/// If the last AssetHub block update is older than this, AssetHub is considered stalled.
	pub storage StalledAssetHubBlockThreshold: BlockNumber = 6 * HOURS;

	// TODO: FAIL-CI - check constants bellow
	pub const AlarmInterval: BlockNumber = 1;
	pub const SubmissionDeposit: Balance = 1 * 3 * CENTS;
	pub const UndecidingTimeout: BlockNumber = 14 * DAYS;
}

/// Tracks the AssetHub state when it is stalled.
///
///  1. AssetHub can send XCM with its parachain head data from `on_idle`.
///  2. Alternatively, XCM from AssetHub may not be needed when custom key reading
///     from `RelayChainStateProof::read_entry(well_known_keys::para_head(asset_hub_id)`
/// 	is implemented. In that case, this pallet (`Pallet<T, I>::do_note_new_head(...)`)
/// 	can be updated directly:
///     - https://github.com/paritytech/polkadot-sdk/issues/82
///     - https://github.com/paritytech/polkadot-sdk/issues/7445
pub type DDayDetectionInstance = pallet_dday_detection::Instance1;
impl pallet_dday_detection::Config<DDayDetectionInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	// TODO: FAIL-CI setup benchmarks
	type WeightInfo = ();
	// Root or AssetHub can update heads
	type SubmitOrigin =
		EitherOfDiverse<EnsureRoot<AccountId>, EnsureXcm<Equals<xcm_config::AssetHub>>>;
	type RemoteBlockNumber = ProofBlockNumberOf<AssetHubAccountProver>;
	type StalledThreshold = StalledAssetHubBlockThreshold;
}

/// Wrapper implementation of `Polling` over `DDayReferenda`, allowing voting only when `IsAssetHubStalled == true`.
pub struct AllowPollingWhenAssetHubIsStalled<Chain>(core::marker::PhantomData<Chain>);
impl<Chain: IsStalled> Polling<pallet_referenda::TallyOf<Runtime, DDayReferendaInstance>>
	for AllowPollingWhenAssetHubIsStalled<Chain>
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
		if Chain::is_stalled() {
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
			if Chain::is_stalled() {
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
		if Chain::is_stalled() {
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
pub type DDayVotingInstance = pallet_dday_voting::Instance1;
impl pallet_dday_voting::Config<DDayVotingInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	// TODO: FAIL-CI - setup/generate benchmarks
	type WeightInfo = pallet_dday_voting::weights::SubstrateWeight<Self>;
	type Polls = AllowPollingWhenAssetHubIsStalled<DDayDetection>;
	type MaxVotes = ConstU32<3>;
	type BlockNumberProvider = System;

	type Prover = AssetHubAccountProver;
	type ProofRootProvider = StalledAssetHubDataProvider<DDayDetection>;
	type MaxTurnoutProvider = StalledAssetHubDataProvider<DDayDetection>;
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
	type Votes = pallet_dday_voting::VotesOf<Runtime, DDayVotingInstance>;
	type Tally = pallet_dday_voting::TallyOf<Runtime, DDayVotingInstance>;
	type SubmissionDeposit = SubmissionDeposit;
	type MaxQueued = ConstU32<2>;
	type UndecidingTimeout = UndecidingTimeout;
	type AlarmInterval = AlarmInterval;
	type Tracks = tracks::TracksInfo;
	type Preimages = Preimage;
	type BlockNumberProvider = System;
}

/// A [`TransactionExtension`] that skips the inner `Extension`
/// if and only if `ValidProofWhenStalledAssetHub` passes. Otherwise, the `Extension` is executed.
pub type SkipCheckIfValidProofWhenStalledAssetHub<Extension> =
	frame_system::SkipCheckIf<Runtime, Extension, ValidProofWhenStalledAssetHub>;

/// A DDay dedicated filter that passes only if and only if a `DDayVoting::vote` call is detected
/// with a valid proof and the AssetHub is stalled.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq)]
pub struct ValidProofWhenStalledAssetHub;
impl ContainsPair<RuntimeCall, RuntimeOrigin> for ValidProofWhenStalledAssetHub {
	fn contains(call: &RuntimeCall, origin: &RuntimeOrigin) -> bool {
		// Filter only `DDayVoting::vote` calls.
		let proof = match call {
			RuntimeCall::DDayVoting(pallet_dday_voting::Call::vote { proof, .. }) => proof,
			_ => return false,
		};
		let Some(signed) = origin.caller.as_signed() else {
			return false;
		};

		// Check if the proof is valid (i.e., the AssetHub is stalled,
		// and the proof is valid according to the stalled state root).
		DDayVoting::voting_power_of(signed.clone(), proof.clone()).is_ok()
	}
}
