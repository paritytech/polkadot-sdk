// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Mock runtime for tests.
//! Implements both runtime APIs for fee estimation and getting the messages for transfers.

use codec::Encode;
use frame_system::EnsureRoot;
use frame_support::{
    construct_runtime, derive_impl, parameter_types,
    traits::{Nothing, ConstU32, ConstU128},
    weights::WeightToFee as WeightToFeeT,
};
use pallet_xcm::TestWeightInfo;
use sp_runtime::{AccountId32, traits::IdentityLookup, SaturatedConversion};
use xcm::{prelude::*, Version as XcmVersion};
use xcm_builder::{EnsureXcmOrigin, FixedWeightBounds, IsConcrete};
use xcm_executor::XcmExecutor;

use xcm_fee_payment_runtime_api::{XcmTransfersApi, XcmPaymentApi, XcmPaymentApiError};

construct_runtime! {
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        XcmPallet: pallet_xcm,
    }
}

type Block = frame_system::mocking::MockBlock<Test>;
type Balance = u128;
type AccountId = AccountId32;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type AccountId = AccountId;
    type AccountData = pallet_balances::AccountData<Balance>;
    type Lookup = IdentityLookup<AccountId>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
    type AccountStore = System;
    type Balance = Balance;
    type ExistentialDeposit = ConstU128<1>;
}

pub struct TestSendXcm;
impl SendXcm for TestSendXcm {
    type Ticket = (Location, Xcm<()>);
    fn validate(
        dest: &mut Option<Location>,
        msg: &mut Option<Xcm<()>>,
    ) -> SendResult<Self::Ticket> {
        let ticket = (dest.take().unwrap(), msg.take().unwrap());
        let fees: Assets = (HereLocation::get(), 1_000_000_000_000u128).into();
        Ok((ticket, fees))
    }
    fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
        let hash = fake_message_hash(&ticket.1);
        Ok(hash)
    }
}

fn fake_message_hash<Call>(message: &Xcm<Call>) -> XcmHash {
    message.using_encoded(sp_io::hashing::blake2_256)
}

pub type XcmRouter = TestSendXcm;

parameter_types! {
    pub const BaseXcmWeight: Weight = Weight::from_parts(1_000_000_000_000, 1024 * 1024);
    pub const MaxInstructions: u32 = 100;
    pub UniversalLocation: InteriorLocation = [GlobalConsensus(NetworkId::Westend)].into();
    pub static AdvertisedXcmVersion: XcmVersion = 4;
    pub const HereLocation: Location = Location::here();
    pub const MaxAssetsIntoHolding: u32 = 64;
}

/// Simple `WeightToFee` implementation that adds the ref_time by the proof_size.
pub struct WeightToFee;
impl WeightToFeeT for WeightToFee {
    type Balance = Balance;
    fn weight_to_fee(weight: &Weight) -> Self::Balance {
        Self::Balance::saturated_from(weight.ref_time())
            .saturating_add(Self::Balance::saturated_from(weight.proof_size()))
    }
}

type Weigher = FixedWeightBounds<BaseXcmWeight, RuntimeCall, MaxInstructions>;

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
    type RuntimeCall = RuntimeCall;
    type XcmSender = XcmRouter;
    type AssetTransactor = ();
    type OriginConverter = ();
    type IsReserve = ();
    type IsTeleporter = ();
    type UniversalLocation = UniversalLocation;
    type Barrier = ();
    type Weigher = Weigher;
    type Trader = ();
    type ResponseHandler = ();
    type AssetTrap = ();
    type AssetLocker = ();
    type AssetExchanger = ();
    type AssetClaims = ();
    type SubscriptionService = ();
    type PalletInstancesInfo = AllPalletsWithSystem;
    type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
    type FeeManager = ();
    type MessageExporter = ();
    type UniversalAliases = ();
    type CallDispatcher = RuntimeCall;
    type SafeCallFilter = Nothing;
    type Aliasers = Nothing;
    type TransactionalProcessor = ();
    type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
}

impl pallet_xcm::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, ()>;
    type XcmRouter = XcmRouter;
    type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, ()>;
    type XcmExecuteFilter = Nothing;
    type XcmExecutor = XcmExecutor<XcmConfig>;
    type XcmTeleportFilter = Nothing;
    type XcmReserveTransferFilter = Nothing;
    type Weigher = Weigher;
    type UniversalLocation = UniversalLocation;
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
    type AdvertisedXcmVersion = AdvertisedXcmVersion;
    type AdminOrigin = EnsureRoot<AccountId>;
    type TrustedLockers = ();
    type SovereignAccountOf = ();
    type Currency = Balances;
    type CurrencyMatcher = IsConcrete<HereLocation>;
    type MaxLockers = ConstU32<0>;
    type MaxRemoteLockConsumers = ConstU32<0>;
    type RemoteLockConsumerIdentifier = ();
    type WeightInfo = TestWeightInfo;
}

#[derive(Clone)]
pub(crate) struct TestClient;

pub(crate) struct RuntimeApi {
    _inner: TestClient,
}

impl sp_api::ProvideRuntimeApi<Block> for TestClient {
    type Api = RuntimeApi;
    fn runtime_api(&self) -> sp_api::ApiRef<Self::Api> {
        RuntimeApi { _inner: self.clone() }.into()
    }
}

sp_api::mock_impl_runtime_apis! {
    impl XcmPaymentApi<Block> for RuntimeApi {
        fn query_acceptable_payment_assets(xcm_version: XcmVersion) -> Result<Vec<VersionedAssetId>, XcmPaymentApiError> {
            todo!()
        }

        fn query_xcm_weight(message: VersionedXcm<()>) -> Result<Weight, XcmPaymentApiError> {
            XcmPallet::query_xcm_weight(message)
        }

        fn query_weight_to_asset_fee(weight: Weight, asset: VersionedAssetId) -> Result<u128, XcmPaymentApiError> {
            let local_asset = VersionedAssetId::V4(HereLocation::get().into());
            let asset = asset
                .into_version(4)
                .map_err(|_| XcmPaymentApiError::VersionedConversionFailed)?;

            if asset != local_asset { return Err(XcmPaymentApiError::AssetNotFound); }

            Ok(WeightToFee::weight_to_fee(&weight))
        }

        fn query_delivery_fees(destination: VersionedLocation, message: VersionedXcm<()>) -> Result<VersionedAssets, XcmPaymentApiError> {
            XcmPallet::query_delivery_fees(destination, message)
        }
    }

    impl XcmTransfersApi<Block> for RuntimeApi {
        fn transfer_assets() -> Vec<(Location, Xcm<()>)> {
            todo!()
        }
        fn teleport_assets(
			dest: Location,
			beneficiary: Location,
			assets: Assets,
        ) -> Vec<(VersionedLocation, VersionedXcm<()>)> {
			vec![
				(
					VersionedLocation::V4(Here.into()),
					VersionedXcm::V4(Xcm(vec![
						WithdrawAsset(assets.clone()),
						BurnAsset(assets.clone()),
					])),
				),
				(
					VersionedLocation::V4(dest),
					VersionedXcm::V4(Xcm(vec![
						ReceiveTeleportedAsset(assets.clone()),
						ClearOrigin,
                        BuyExecution { fees: (Here, 100u128).into(), weight_limit: Unlimited },
                        DepositAsset { assets: Wild(All), beneficiary },
					])),
				),
			]
        }
        fn reserve_transfer_assets() -> Vec<(Location, Xcm<()>)> {
            todo!()
        }
    }
}
