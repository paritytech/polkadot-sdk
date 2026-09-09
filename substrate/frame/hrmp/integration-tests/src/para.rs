// This file is part of Substrate.

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

//! The control-plane parachain: `pallet-hrmp-para` plus enough XCM to talk to the relay.
//!
//! Structured after `polkadot/xcm/xcm-simulator/example/src/parachain`, trimmed to what HRMP
//! needs and with `ParentAsSuperuser` added to the origin converter so the relay chain's
//! `Superuser` reports land as `Root` here.

use frame_support::{
	construct_runtime, derive_impl, parameter_types,
	traits::{
		fungible::HoldConsideration, ConstU128, ConstU32, Disabled, Everything, LinearStoragePrice,
		Nothing,
	},
	weights::Weight,
};
use frame_system::EnsureRoot;
use hrmp_primitives::ParaId as HrmpParaId;
use sp_runtime::{
	traits::{Convert, IdentityLookup},
	AccountId32,
};
use xcm::latest::prelude::*;
use xcm_builder::{
	AccountId32Aliases, AllowUnpaidExecutionFrom, ChildParachainConvertsVia, DescribeAllTerminal,
	DescribeFamily, EnsureDecodableXcm, EnsureXcmOrigin, FixedWeightBounds,
	FrameTransactionalProcessor, FungibleAdapter, HashedDescription, IsConcrete, ParentAsSuperuser,
	SiblingParachainConvertsVia, SignedAccountId32AsNative, SignedToAccountId32,
	SovereignSignedViaLocation,
};
use xcm_executor::XcmExecutor;
use xcm_simulator::mock_message_queue;

use crate::{
	senders::{ParaSendToRelay, PARA_ID},
	MAX_CAPACITY, MAX_INBOUND_CHANNELS, MAX_MESSAGE_SIZE, MAX_OUTBOUND_CHANNELS,
};

pub type AccountId = AccountId32;
pub type Balance = u128;

/// What one message of channel capacity costs the para putting the deposit up.
pub const PER_MESSAGE: Balance = 1_000;

/// Sizes used for channels involving a system chain, as `(max_message_size, max_capacity)`.
pub const SYSTEM_CHANNEL_SIZE_AND_CAPACITY: (u32, u32) = (MAX_MESSAGE_SIZE, MAX_CAPACITY);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type Balance = Balance;
	type ExistentialDeposit = ConstU128<1>;
	type AccountStore = System;
	type RuntimeHoldReason = RuntimeHoldReason;
}

impl mock_message_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
}

parameter_types! {
	pub const RelayLocation: Location = Location::parent();
	pub const RelayNetwork: NetworkId = ByGenesis([0; 32]);
	pub UniversalLocation: InteriorLocation =
		[GlobalConsensus(RelayNetwork::get()), Parachain(PARA_ID)].into();
	pub const BaseXcmWeight: Weight = Weight::from_parts(1_000, 1_000);
	pub const MaxInstructions: u32 = 100;
	pub const MaxAssetsIntoHolding: u32 = 64;
}

pub type LocationConverter = (
	HashedDescription<AccountId, DescribeFamily<DescribeAllTerminal>>,
	AccountId32Aliases<RelayNetwork, AccountId>,
);

pub type LocalAssetTransactor =
	FungibleAdapter<Balances, IsConcrete<RelayLocation>, LocationConverter, AccountId, ()>;

/// Note `ParentAsSuperuser`: it is what turns the relay chain's `OriginKind::Superuser` report
/// into `Root` here, which is what `pallet-hrmp-para`'s `RelayOrigin` accepts.
pub type OriginConverter = (
	SovereignSignedViaLocation<LocationConverter, RuntimeOrigin>,
	SignedAccountId32AsNative<RelayNetwork, RuntimeOrigin>,
	ParentAsSuperuser<RuntimeOrigin>,
	pallet_xcm::XcmPassthrough<RuntimeOrigin>,
);

pub type XcmRouter = EnsureDecodableXcm<crate::ParachainXcmRouter<MsgQueue>>;
pub type Weigher = FixedWeightBounds<BaseXcmWeight, RuntimeCall, MaxInstructions>;

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = XcmRouter;
	type XcmEventEmitter = PolkadotXcm;
	type AssetTransactor = LocalAssetTransactor;
	type OriginConverter = OriginConverter;
	type IsReserve = ();
	type IsTeleporter = ();
	type UniversalLocation = UniversalLocation;
	type Barrier = AllowUnpaidExecutionFrom<Everything>;
	type Weigher = Weigher;
	type Trader = ();
	type ResponseHandler = ();
	type AssetTrap = ();
	type AssetLocker = ();
	type AssetExchanger = ();
	type SubscriptionService = ();
	type PalletInstancesInfo = ();
	type FeeManager = ();
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
	type Aliasers = Nothing;
	type TransactionalProcessor = FrameTransactionalProcessor;
	type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
	type XcmRecorder = PolkadotXcm;
}

pub type LocalOriginToLocation = SignedToAccountId32<RuntimeOrigin, AccountId, RelayNetwork>;

impl pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmRouter = XcmRouter;
	type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmExecuteFilter = Everything;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = Nothing;
	type XcmReserveTransferFilter = Everything;
	type Weigher = Weigher;
	type UniversalLocation = UniversalLocation;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
	type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
	type Currency = Balances;
	type CurrencyMatcher = ();
	type TrustedLockers = ();
	type SovereignAccountOf = LocationConverter;
	type MaxLockers = ConstU32<8>;
	type MaxRemoteLockConsumers = ConstU32<0>;
	type RemoteLockConsumerIdentifier = ();
	type WeightInfo = pallet_xcm::TestWeightInfo;
	type AdminOrigin = EnsureRoot<AccountId>;
	type AuthorizedAliasConsideration = Disabled;
}

/// The account a para's channel deposits come out of, its sovereign account here.
///
/// Same role as `pallet-broker`'s `SovereignAccountOf` on the Coretime chain: a para id becomes
/// the sibling location, which the location converter turns into an account.
pub struct SovereignAccountOf;

impl Convert<HrmpParaId, AccountId> for SovereignAccountOf {
	fn convert(para_id: HrmpParaId) -> AccountId {
		type Sovereign = (
			SiblingParachainConvertsVia<
				polkadot_parachain_primitives::primitives::Sibling,
				AccountId,
			>,
			ChildParachainConvertsVia<polkadot_parachain_primitives::primitives::Id, AccountId>,
		);

		<Sovereign as xcm_executor::traits::ConvertLocation<AccountId>>::convert_location(
			&Location::new(1, [Junction::Parachain(para_id)]),
		)
		.expect("sibling parachain locations always convert; qed")
	}
}

parameter_types! {
	pub const DepositPerMessage: Balance = PER_MESSAGE;
	pub const SenderHoldReason: RuntimeHoldReason =
		RuntimeHoldReason::Hrmp(pallet_hrmp_para::HoldReason::SenderDeposit);
	pub const RecipientHoldReason: RuntimeHoldReason =
		RuntimeHoldReason::Hrmp(pallet_hrmp_para::HoldReason::RecipientDeposit);
	pub const SystemChannelSizes: (u32, u32) = SYSTEM_CHANNEL_SIZE_AND_CAPACITY;
}

impl pallet_hrmp_para::Config for Runtime {
	type SenderConsideration = HoldConsideration<
		AccountId,
		Balances,
		SenderHoldReason,
		LinearStoragePrice<ConstU128<0>, DepositPerMessage, Balance>,
	>;
	type RecipientConsideration = HoldConsideration<
		AccountId,
		Balances,
		RecipientHoldReason,
		LinearStoragePrice<ConstU128<0>, DepositPerMessage, Balance>,
	>;
	type SendToRelay = ParaSendToRelay;
	// The relay chain reports with `OriginKind::Superuser`, which `ParentAsSuperuser` turns into
	// `Root`. Nothing else on this chain can produce a `Root` origin in these tests.
	type RelayOrigin = EnsureRoot<AccountId>;
	type ParachainOrigin = frame_system::EnsureNever<HrmpParaId>;
	type ChannelManager = EnsureRoot<AccountId>;
	type SovereignAccountOf = SovereignAccountOf;
	type MaxCapacity = ConstU32<MAX_CAPACITY>;
	type MaxMessageSize = ConstU32<MAX_MESSAGE_SIZE>;
	type MaxInboundChannels = ConstU32<MAX_INBOUND_CHANNELS>;
	type MaxOutboundChannels = ConstU32<MAX_OUTBOUND_CHANNELS>;
	type DefaultChannelSizeAndCapacityWithSystem = SystemChannelSizes;
	// No system para in the harness, so every channel takes a deposit.
	type IsSystemPara = Nothing;
	type WeightInfo = ();
}

type Block = frame_system::mocking::MockBlock<Runtime>;

construct_runtime!(
	pub struct Runtime {
		System: frame_system,
		Balances: pallet_balances,
		MsgQueue: mock_message_queue,
		PolkadotXcm: pallet_xcm,
		Hrmp: pallet_hrmp_para,
	}
);
