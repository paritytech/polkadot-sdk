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

use super::{AccountId, Call, Event, Origin, ParachainInfo, Runtime};
use frame_support::{match_type, parameter_types, weights::Weight};
use xcm::latest::prelude::*;
use xcm_builder::{
	AllowUnpaidExecutionFrom, FixedWeightBounds, LocationInverter, ParentAsSuperuser,
	ParentIsPreset, SovereignSignedViaLocation,
};

parameter_types! {
	pub const RococoLocation: MultiLocation = MultiLocation::parent();
	pub const RococoNetwork: NetworkId = NetworkId::Polkadot;
	pub Ancestry: MultiLocation = Parachain(ParachainInfo::parachain_id().into()).into();
}

/// This is the type we use to convert an (incoming) XCM origin into a local `Origin` instance,
/// ready for dispatching a transaction with Xcm's `Transact`. There is an `OriginKind` which can
/// bias the kind of local `Origin` it will become.
pub type XcmOriginToTransactDispatchOrigin = (
	// Sovereign account converter; this attempts to derive an `AccountId` from the origin location
	// using `LocationToAccountId` and then turn that into the usual `Signed` origin. Useful for
	// foreign chains who want to have a local sovereign account on this chain which they control.
	SovereignSignedViaLocation<ParentIsPreset<AccountId>, Origin>,
	// Superuser converter for the Relay-chain (Parent) location. This will allow it to issue a
	// transaction from the Root origin.
	ParentAsSuperuser<Origin>,
);

match_type! {
	pub type JustTheParent: impl Contains<MultiLocation> = { MultiLocation { parents:1, interior: Here } };
}

parameter_types! {
	// One XCM operation is 1_000_000_000 weight - almost certainly a conservative estimate.
	pub UnitWeightCost: Weight = 1_000_000_000;
	pub const MaxInstructions: u32 = 100;
}

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type Call = Call;
	type XcmSender = (); // sending XCM not supported
	type AssetTransactor = (); // balances not supported
	type OriginConverter = XcmOriginToTransactDispatchOrigin;
	type IsReserve = (); // balances not supported
	type IsTeleporter = (); // balances not supported
	type LocationInverter = LocationInverter<Ancestry>;
	type Barrier = AllowUnpaidExecutionFrom<JustTheParent>;
	type Weigher = FixedWeightBounds<UnitWeightCost, Call, MaxInstructions>; // balances not supported
	type Trader = (); // balances not supported
	type ResponseHandler = (); // Don't handle responses for now.
	type AssetTrap = (); // don't trap for now
	type AssetClaims = (); // don't claim for now
	type SubscriptionService = (); // don't handle subscriptions for now
}

impl cumulus_pallet_xcm::Config for Runtime {
	type Event = Event;
	type XcmExecutor = xcm_executor::XcmExecutor<XcmConfig>;
}
