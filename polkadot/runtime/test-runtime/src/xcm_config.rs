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

use frame_support::{
	parameter_types,
	traits::{Everything, Get, Nothing},
	weights::Weight,
};
use frame_system::EnsureRoot;
use polkadot_runtime_parachains::FeeTracker;
use runtime_common::xcm_sender::{ChildParachainRouter, PriceForMessageDelivery};
use xcm::latest::prelude::*;
use xcm_builder::{
	AllowUnpaidExecutionFrom, EnsureXcmOrigin, FixedWeightBounds, FrameTransactionalProcessor,
	SignedAccountId32AsNative, SignedToAccountId32, WithUniqueTopic,
};
use xcm_executor::{
	traits::{TransactAsset, WeightTrader},
	AssetsInHolding,
};

parameter_types! {
	pub const BaseXcmWeight: xcm::latest::Weight = Weight::from_parts(1_000, 1_000);
	pub const AnyNetwork: Option<NetworkId> = None;
	pub const MaxInstructions: u32 = 100;
	pub const MaxAssetsIntoHolding: u32 = 16;
	pub const UniversalLocation: xcm::latest::InteriorLocation = xcm::latest::Junctions::Here;
	pub TokenLocation: Location = Here.into_location();
	pub FeeAssetId: AssetId = AssetId(TokenLocation::get());
}

/// Type to convert an `Origin` type value into a `Location` value which represents an interior
/// location of this chain.
pub type LocalOriginToLocation = (
	// And a usual Signed origin to be used in XCM as a corresponding AccountId32
	SignedToAccountId32<crate::RuntimeOrigin, crate::AccountId, AnyNetwork>,
);

/// Implementation of [`PriceForMessageDelivery`], returning a different price
/// based on whether a message contains a reanchored asset or not.
/// This implementation ensures that messages with non-reanchored assets return higher
/// prices than messages with reanchored assets.
/// Useful for `deposit_reserve_asset_works_for_any_xcm_sender` integration test.
pub struct TestDeliveryPrice<A, F>(sp_std::marker::PhantomData<(A, F)>);
impl<A: Get<AssetId>, F: FeeTracker> PriceForMessageDelivery for TestDeliveryPrice<A, F> {
	type Id = F::Id;

	fn price_for_delivery(_: Self::Id, msg: &Xcm<()>) -> Assets {
		let base_fee: super::Balance = 1_000_000;

		let parents = msg.iter().find_map(|xcm| match xcm {
			ReserveAssetDeposited(assets) => {
				let AssetId(location) = &assets.inner().first().unwrap().id;
				Some(location.parents)
			},
			_ => None,
		});

		// If no asset is found, price defaults to `base_fee`.
		let amount = base_fee
			.saturating_add(base_fee.saturating_mul(parents.unwrap_or(0) as super::Balance));

		(A::get(), amount).into()
	}
}

pub type PriceForChildParachainDelivery = TestDeliveryPrice<FeeAssetId, super::Dmp>;

/// The XCM router. When we want to send an XCM message, we use this type. It amalgamates all of our
/// individual routers.
pub type XcmRouter = WithUniqueTopic<
	// Only one router so far - use DMP to communicate with child parachains.
	ChildParachainRouter<super::Runtime, super::Xcm, PriceForChildParachainDelivery>,
>;

pub type Barrier = AllowUnpaidExecutionFrom<Everything>;

pub struct DummyAssetTransactor;
impl TransactAsset for DummyAssetTransactor {
	fn deposit_asset(_what: &Asset, _who: &Location, _context: Option<&XcmContext>) -> XcmResult {
		Ok(())
	}

	fn withdraw_asset(
		_what: &Asset,
		_who: &Location,
		_maybe_context: Option<&XcmContext>,
	) -> Result<AssetsInHolding, XcmError> {
		let asset: Asset = (Parent, 100_000).into();
		Ok(asset.into())
	}
}

#[derive(Clone)]
pub struct DummyWeightTrader;
impl WeightTrader for DummyWeightTrader {
	fn new() -> Self {
		DummyWeightTrader
	}

	fn buy_weight(
		&mut self,
		_weight: Weight,
		_payment: AssetsInHolding,
		_context: &XcmContext,
	) -> Result<AssetsInHolding, XcmError> {
		Ok(AssetsInHolding::default())
	}
}

type OriginConverter = (
	pallet_xcm::XcmPassthrough<super::RuntimeOrigin>,
	SignedAccountId32AsNative<AnyNetwork, super::RuntimeOrigin>,
);

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = super::RuntimeCall;
	type XcmSender = XcmRouter;
	type AssetTransactor = DummyAssetTransactor;
	type OriginConverter = OriginConverter;
	type IsReserve = ();
	type IsTeleporter = ();
	type UniversalLocation = UniversalLocation;
	type Barrier = Barrier;
	type Weigher = FixedWeightBounds<BaseXcmWeight, super::RuntimeCall, MaxInstructions>;
	type Trader = DummyWeightTrader;
	type ResponseHandler = super::Xcm;
	type AssetTrap = super::Xcm;
	type AssetLocker = ();
	type AssetExchanger = ();
	type AssetClaims = super::Xcm;
	type SubscriptionService = super::Xcm;
	type PalletInstancesInfo = ();
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type FeeManager = ();
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = super::RuntimeCall;
	type SafeCallFilter = Everything;
	type Aliasers = Nothing;
	type TransactionalProcessor = FrameTransactionalProcessor;
	type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
}

impl pallet_xcm::Config for crate::Runtime {
	// The config types here are entirely configurable, since the only one that is sorely needed
	// is `XcmExecutor`, which will be used in unit tests located in xcm-executor.
	type RuntimeEvent = crate::RuntimeEvent;
	type ExecuteXcmOrigin = EnsureXcmOrigin<crate::RuntimeOrigin, LocalOriginToLocation>;
	type UniversalLocation = UniversalLocation;
	type SendXcmOrigin = EnsureXcmOrigin<crate::RuntimeOrigin, LocalOriginToLocation>;
	type Weigher = FixedWeightBounds<BaseXcmWeight, crate::RuntimeCall, MaxInstructions>;
	type XcmRouter = XcmRouter;
	type XcmExecuteFilter = Everything;
	type XcmExecutor = xcm_executor::XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = Everything;
	type XcmReserveTransferFilter = Everything;
	type RuntimeOrigin = crate::RuntimeOrigin;
	type RuntimeCall = crate::RuntimeCall;
	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
	type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
	type Currency = crate::Balances;
	type CurrencyMatcher = ();
	type TrustedLockers = ();
	type SovereignAccountOf = ();
	type MaxLockers = frame_support::traits::ConstU32<8>;
	type MaxRemoteLockConsumers = frame_support::traits::ConstU32<0>;
	type RemoteLockConsumerIdentifier = ();
	type WeightInfo = pallet_xcm::TestWeightInfo;
	type AdminOrigin = EnsureRoot<crate::AccountId>;
}
