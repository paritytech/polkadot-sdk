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

//! Governance configurations for the Asset Hub runtime.

use super::*;
use crate::xcm_config::Collectives;
use frame_support::{
	parameter_types,
	traits::{tokens::UnityOrOuterConversion, EitherOf, EitherOfDiverse, FromContains},
};
use frame_system::EnsureRootWithSuccess;
use pallet_xcm::{EnsureXcm, IsVoiceOfBody};
use polkadot_runtime_common::{
	impls::{
		ContainsParts, LocatableAssetConverter, VersionedLocatableAsset, VersionedLocationConverter,
	},
	prod_or_fast,
};
use sp_runtime::{traits::IdentityLookup, Percent};
use xcm::latest::{
	prelude::{InteriorLocation, PalletInstance},
	BodyId,
};

mod origins;
pub use origins::{
	pallet_custom_origins, AuctionAdmin, FellowshipAdmin, GeneralAdmin, LeaseAdmin,
	ReferendumCanceller, ReferendumKiller, Spender, StakingAdmin, Treasurer, WhitelistedCaller,
};
mod tracks;
pub use tracks::TracksInfo;
use xcm_builder::PayOverXcm;
parameter_types! {
	pub const VoteLockingPeriod: BlockNumber = prod_or_fast!(7 * RC_DAYS, 1);
}

impl pallet_conviction_voting::Config for Runtime {
	type WeightInfo = weights::pallet_conviction_voting::WeightInfo<Self>;
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type VoteLockingPeriod = VoteLockingPeriod;
	type MaxVotes = ConstU32<512>; // TODO check with weight
								// TODO: review
	type MaxTurnout =
		frame_support::traits::tokens::currency::ActiveIssuanceOf<Balances, Self::AccountId>;
	type Polls = Referenda;
	type BlockNumberProvider = RelaychainDataProvider<Runtime>;
	type VotingHooks = ();// TODO review
}

parameter_types! {
	pub const AlarmInterval: BlockNumber = 1;
	pub const SubmissionDeposit: Balance = 1 * 3 * CENTS;
	pub const UndecidingTimeout: BlockNumber = 14 * RC_DAYS;
}

impl origins::pallet_custom_origins::Config for Runtime {}

parameter_types! {
	// Fellows pluralistic body.
	pub const FellowsBodyId: BodyId = BodyId::Technical;
}

impl pallet_whitelist::Config for Runtime {
	type WeightInfo = weights::pallet_whitelist::WeightInfo<Self>;
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type WhitelistOrigin = EitherOfDiverse<
		EnsureRoot<Self::AccountId>,
		EnsureXcm<IsVoiceOfBody<Collectives, FellowsBodyId>>,
	>;
	type DispatchWhitelistedOrigin = EitherOf<EnsureRoot<Self::AccountId>, WhitelistedCaller>;
	type Preimages = Preimage;
}

impl pallet_referenda::Config for Runtime {
	type WeightInfo = weights::pallet_referenda::WeightInfo<Self>;
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type Scheduler = Scheduler;
	type Currency = Balances;
	type SubmitOrigin = frame_system::EnsureSigned<AccountId>;
	type CancelOrigin = EitherOf<EnsureRoot<AccountId>, ReferendumCanceller>;
	type KillOrigin = EitherOf<EnsureRoot<AccountId>, ReferendumKiller>;
	type Slash = Treasury;
	type Votes = pallet_conviction_voting::VotesOf<Runtime>;
	type Tally = pallet_conviction_voting::TallyOf<Runtime>;
	type SubmissionDeposit = SubmissionDeposit;
	type MaxQueued = ConstU32<100>;
	type UndecidingTimeout = UndecidingTimeout;
	type AlarmInterval = AlarmInterval;
	type Tracks = TracksInfo;
	type Preimages = Preimage;
	type BlockNumberProvider = RelaychainDataProvider<Runtime>;
}

parameter_types! {
	pub const SpendPeriod: BlockNumber = 6 * DAYS;
	pub const Burn: Permill = Permill::from_perthousand(2);
	pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
	pub const PayoutSpendPeriod: BlockNumber = 30 * DAYS;
	// The asset's interior location for the paying account. This is the Treasury
	// pallet instance (which sits at index 37).
	pub TreasuryInteriorLocation: InteriorLocation = PalletInstance(37).into();

	pub const TipCountdown: BlockNumber = 1 * DAYS;
	pub const TipFindersFee: Percent = Percent::from_percent(20);
	pub const TipReportDepositBase: Balance = 100 * CENTS;
	pub const DataDepositPerByte: Balance = 1 * CENTS;
	pub const MaxApprovals: u32 = 100;
	pub const MaxAuthorities: u32 = 100_000;
	pub const MaxKeys: u32 = 10_000;
	pub const MaxPeerInHeartbeats: u32 = 10_000;
	pub const MaxBalance: Balance = Balance::max_value();
}

pub type TreasurySpender = EitherOf<EnsureRootWithSuccess<AccountId, MaxBalance>, Spender>;

impl pallet_treasury::Config for Runtime {
	type PalletId = TreasuryPalletId;
	type Currency = Balances;
	type RejectOrigin = EitherOfDiverse<EnsureRoot<AccountId>, Treasurer>;
	type RuntimeEvent = RuntimeEvent;
	type SpendPeriod = SpendPeriod;
	type Burn = Burn;
	type BurnDestination = ();
	type MaxApprovals = MaxApprovals;
	type WeightInfo = weights::pallet_treasury::WeightInfo<Runtime>;
	type SpendFunds = ();
	type SpendOrigin = TreasurySpender;
	type AssetKind = VersionedLocatableAsset;
	type Beneficiary = VersionedLocation;
	type BeneficiaryLookup = IdentityLookup<Self::Beneficiary>;
	type Paymaster = PayOverXcm<
		TreasuryInteriorLocation,
		crate::xcm_config::XcmRouter,
		crate::PolkadotXcm,
		ConstU32<{ 6 * HOURS }>,
		Self::Beneficiary,
		Self::AssetKind,
		LocatableAssetConverter,
		VersionedLocationConverter,
	>;
	type BalanceConverter = UnityOrOuterConversion<
		ContainsParts<
			FromContains<
				xcm_builder::IsChildSystemParachain<ParaId>,
				xcm_builder::IsParentsOnly<ConstU8<1>>,
			>,
		>,
		AssetRate,
	>;
	type PayoutPeriod = PayoutSpendPeriod;
	type BlockNumberProvider = RelaychainDataProvider<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = polkadot_runtime_common::impls::benchmarks::TreasuryArguments;
}

impl pallet_asset_rate::Config for Runtime {
	type WeightInfo = weights::pallet_asset_rate::WeightInfo<Runtime>;
	type RuntimeEvent = RuntimeEvent;
	type CreateOrigin = EnsureRoot<AccountId>;
	type RemoveOrigin = EnsureRoot<AccountId>;
	type UpdateOrigin = EnsureRoot<AccountId>;
	type Currency = Balances;
	type AssetKind = <Runtime as pallet_treasury::Config>::AssetKind;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = polkadot_runtime_common::impls::benchmarks::AssetRateArguments;
}
