// Copyright Parity Technologies (UK) Ltd.
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

//! Relay chain runtime mock.

use frame_support::{
	construct_runtime, derive_impl, parameter_types,
	traits::{Contains, Everything, Nothing},
	weights::Weight,
};

use frame_system::EnsureRoot;
use sp_core::{ConstU32, H256};
use sp_runtime::traits::IdentityLookup;

use polkadot_parachain_primitives::primitives::Id as ParaId;
use polkadot_runtime_parachains::{configuration, origin, shared};
use xcm::latest::prelude::*;
#[allow(deprecated)]
use xcm_builder::CurrencyAdapter as XcmCurrencyAdapter;
use xcm_builder::{
	AccountId32Aliases, AllowExplicitUnpaidExecutionFrom, AllowSubscriptionsFrom,
	AllowTopLevelPaidExecutionFrom, ChildParachainAsNative, ChildParachainConvertsVia,
	ChildSystemParachainAsSuperuser, DescribeAllTerminal, DescribeFamily, FixedRateOfFungible,
	FixedWeightBounds, HashedDescription, IsConcrete, SignedAccountId32AsNative,
	SignedToAccountId32, SovereignSignedViaLocation, WithComputedOrigin,
};
use xcm_executor::{Config, XcmExecutor};

use super::{
	mocks::relay_message_queue::*,
	primitives::{AccountId, Balance},
};

parameter_types! {
	pub const BlockHashCount: u64 = 250;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Block = Block;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = ::sp_runtime::traits::BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type BlockWeights = ();
	type BlockLength = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type DbWeight = ();
	type BaseCallFilter = Everything;
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}

parameter_types! {
	pub ExistentialDeposit: Balance = 1;
	pub const MaxLocks: u32 = 50;
	pub const MaxReserves: u32 = 50;
}

impl pallet_balances::Config for Runtime {
	type MaxLocks = MaxLocks;
	type Balance = Balance;
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = ();
	type MaxReserves = MaxReserves;
	type ReserveIdentifier = [u8; 8];
	type FreezeIdentifier = ();
	type MaxHolds = ConstU32<0>;
	type MaxFreezes = ConstU32<0>;
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
}

impl shared::Config for Runtime {
	type DisabledValidators = ();
}

impl configuration::Config for Runtime {
	type WeightInfo = configuration::TestWeightInfo;
}

parameter_types! {
	pub RelayNetwork: NetworkId = ByGenesis([0; 32]);
	pub const TokenLocation: Location = Here.into_location();
	pub UniversalLocation: InteriorLocation = Here;
	pub UnitWeightCost: u64 = 1_000;
}

pub type SovereignAccountOf = (
	HashedDescription<AccountId, DescribeFamily<DescribeAllTerminal>>,
	AccountId32Aliases<RelayNetwork, AccountId>,
	ChildParachainConvertsVia<ParaId, AccountId>,
);

#[allow(deprecated)]
pub type LocalBalancesTransactor =
	XcmCurrencyAdapter<Balances, IsConcrete<TokenLocation>, SovereignAccountOf, AccountId, ()>;

pub type AssetTransactors = LocalBalancesTransactor;

type LocalOriginConverter = (
	SovereignSignedViaLocation<SovereignAccountOf, RuntimeOrigin>,
	ChildParachainAsNative<origin::Origin, RuntimeOrigin>,
	SignedAccountId32AsNative<RelayNetwork, RuntimeOrigin>,
	ChildSystemParachainAsSuperuser<ParaId, RuntimeOrigin>,
);

parameter_types! {
	pub const XcmInstructionWeight: Weight = Weight::from_parts(1_000, 1_000);
	pub TokensPerSecondPerMegabyte: (AssetId, u128, u128) =
		(AssetId(TokenLocation::get()), 1_000_000_000_000, 1024 * 1024);
	pub const MaxInstructions: u32 = 100;
	pub const MaxAssetsIntoHolding: u32 = 64;
}

pub struct ChildrenParachains;
impl Contains<Location> for ChildrenParachains {
	fn contains(location: &Location) -> bool {
		matches!(location.unpack(), (0, [Parachain(_)]))
	}
}

pub type XcmRouter = crate::RelayChainXcmRouter;
pub type Barrier = WithComputedOrigin<
	(
		AllowExplicitUnpaidExecutionFrom<ChildrenParachains>,
		AllowTopLevelPaidExecutionFrom<Everything>,
		AllowSubscriptionsFrom<Everything>,
	),
	UniversalLocation,
	ConstU32<1>,
>;

pub struct XcmConfig;
impl Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = XcmRouter;
	type AssetTransactor = AssetTransactors;
	type OriginConverter = LocalOriginConverter;
	type IsReserve = ();
	type IsTeleporter = ();
	type UniversalLocation = UniversalLocation;
	type Barrier = Barrier;
	type Weigher = FixedWeightBounds<XcmInstructionWeight, RuntimeCall, MaxInstructions>;
	type Trader = FixedRateOfFungible<TokensPerSecondPerMegabyte, ()>;
	type ResponseHandler = XcmPallet;
	type AssetTrap = XcmPallet;
	type AssetLocker = XcmPallet;
	type AssetExchanger = ();
	type AssetClaims = XcmPallet;
	type SubscriptionService = XcmPallet;
	type PalletInstancesInfo = AllPalletsWithSystem;
	type FeeManager = ();
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
	type Aliasers = Nothing;
}

pub type LocalOriginToLocation = SignedToAccountId32<RuntimeOrigin, AccountId, RelayNetwork>;

impl pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SendXcmOrigin = xcm_builder::EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmRouter = XcmRouter;
	type ExecuteXcmOrigin = xcm_builder::EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmExecuteFilter = Everything;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = Everything;
	type XcmReserveTransferFilter = Everything;
	type Weigher = FixedWeightBounds<XcmInstructionWeight, RuntimeCall, MaxInstructions>;
	type UniversalLocation = UniversalLocation;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
	type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
	type Currency = Balances;
	type CurrencyMatcher = IsConcrete<TokenLocation>;
	type TrustedLockers = ();
	type SovereignAccountOf = SovereignAccountOf;
	type MaxLockers = ConstU32<8>;
	type MaxRemoteLockConsumers = ConstU32<0>;
	type RemoteLockConsumerIdentifier = ();
	type WeightInfo = pallet_xcm::TestWeightInfo;
	type AdminOrigin = EnsureRoot<AccountId>;
}

impl origin::Config for Runtime {}

type Block = frame_system::mocking::MockBlock<Runtime>;

impl pallet_message_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Size = u32;
	type HeapSize = MessageQueueHeapSize;
	type MaxStale = MessageQueueMaxStale;
	type ServiceWeight = MessageQueueServiceWeight;
	type MessageProcessor = MessageProcessor;
	type QueueChangeHandler = ();
	type WeightInfo = ();
	type QueuePausedQuery = ();
}

construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		Balances: pallet_balances,
		ParasOrigin: origin,
		XcmPallet: pallet_xcm,
		MessageQueue: pallet_message_queue,
	}
);
