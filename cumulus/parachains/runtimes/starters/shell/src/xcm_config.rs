// Copyright (C) Parity Technologies (UK) Ltd.
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

use super::{
	AccountId, AllPalletsWithSystem, ParachainInfo, Runtime, RuntimeCall, RuntimeEvent,
	RuntimeOrigin,
};
use frame_support::{
	parameter_types,
	traits::{Contains, Everything, Nothing},
	weights::Weight,
};
use xcm::latest::prelude::*;
use xcm_builder::{
	AllowExplicitUnpaidExecutionFrom, FixedWeightBounds, ParentAsSuperuser, ParentIsPreset,
	SovereignSignedViaLocation,
};

parameter_types! {
	pub const RococoLocation: Location = Location::parent();
	pub const RococoNetwork: Option<NetworkId> = Some(NetworkId::Rococo);
	pub UniversalLocation: InteriorLocation = [Parachain(ParachainInfo::parachain_id().into())].into();
}

/// This is the type we use to convert an (incoming) XCM origin into a local `Origin` instance,
/// ready for dispatching a transaction with Xcm's `Transact`. There is an `OriginKind` which can
/// bias the kind of local `Origin` it will become.
pub type XcmOriginToTransactDispatchOrigin = (
	// Sovereign account converter; this attempts to derive an `AccountId` from the origin location
	// using `LocationToAccountId` and then turn that into the usual `Signed` origin. Useful for
	// foreign chains who want to have a local sovereign account on this chain which they control.
	SovereignSignedViaLocation<ParentIsPreset<AccountId>, RuntimeOrigin>,
	// Superuser converter for the Relay-chain (Parent) location. This will allow it to issue a
	// transaction from the Root origin.
	ParentAsSuperuser<RuntimeOrigin>,
);

pub struct JustTheParent;
impl Contains<Location> for JustTheParent {
	fn contains(location: &Location) -> bool {
		matches!(location.unpack(), (1, []))
	}
}

parameter_types! {
	// One XCM operation is 1_000_000_000 weight - almost certainly a conservative estimate.
	pub UnitWeightCost: Weight = Weight::from_parts(1_000_000_000, 64 * 1024);
	pub const MaxInstructions: u32 = 100;
	pub const MaxAssetsIntoHolding: u32 = 64;
}

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = (); // sending XCM not supported
	type AssetTransactor = (); // balances not supported
	type OriginConverter = XcmOriginToTransactDispatchOrigin;
	type IsReserve = (); // balances not supported
	type IsTeleporter = (); // balances not supported
	type UniversalLocation = UniversalLocation;
	type Barrier = AllowExplicitUnpaidExecutionFrom<JustTheParent>;
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>; // balances not supported
	type Trader = (); // balances not supported
	type ResponseHandler = (); // Don't handle responses for now.
	type AssetTrap = (); // don't trap for now
	type AssetClaims = (); // don't claim for now
	type SubscriptionService = (); // don't handle subscriptions for now
	type PalletInstancesInfo = AllPalletsWithSystem;
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type AssetLocker = ();
	type AssetExchanger = ();
	type FeeManager = ();
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
	type Aliasers = Nothing;
	type TransactionalProcessor = ();
}

impl cumulus_pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = xcm_executor::XcmExecutor<XcmConfig>;
}
