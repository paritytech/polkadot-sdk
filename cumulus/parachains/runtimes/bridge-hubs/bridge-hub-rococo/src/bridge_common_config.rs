// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Bridge definitions that can be used by multiple BridgeHub flavors.
//! All configurations here should be dedicated to a single chain; in other words, we don't need two
//! chains for a single pallet configuration.
//!
//! For example, the messaging pallet needs to know the sending and receiving chains, but the
//! GRANDPA tracking pallet only needs to be aware of one chain.

use super::{weights, AccountId, Balance, Balances, BlockNumber, Runtime, RuntimeEvent};
use bp_parachains::SingleParaStoredHeaderDataBuilder;
use frame_support::{parameter_types, traits::ConstU32};

parameter_types! {
	pub const RelayChainHeadersToKeep: u32 = 1024;
	pub const ParachainHeadsToKeep: u32 = 64;

	pub const RococoBridgeParachainPalletName: &'static str = "Paras";
	pub const MaxRococoParaHeadDataSize: u32 = bp_rococo::MAX_NESTED_PARACHAIN_HEAD_DATA_SIZE;
	pub const WococoBridgeParachainPalletName: &'static str = "Paras";
	pub const MaxWococoParaHeadDataSize: u32 = bp_wococo::MAX_NESTED_PARACHAIN_HEAD_DATA_SIZE;
	pub const WestendBridgeParachainPalletName: &'static str = "Paras";
	pub const MaxWestendParaHeadDataSize: u32 = bp_westend::MAX_NESTED_PARACHAIN_HEAD_DATA_SIZE;

	pub storage RequiredStakeForStakeAndSlash: Balance = 1_000_000;
	pub const RelayerStakeLease: u32 = 8;
	pub const RelayerStakeReserveId: [u8; 8] = *b"brdgrlrs";

	pub storage DeliveryRewardInBalance: u64 = 1_000_000;
}

/// Add GRANDPA bridge pallet to track Wococo relay chain.
pub type BridgeGrandpaWococoInstance = pallet_bridge_grandpa::Instance1;
impl pallet_bridge_grandpa::Config<BridgeGrandpaWococoInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type BridgedChain = bp_wococo::Wococo;
	type MaxFreeMandatoryHeadersPerBlock = ConstU32<4>;
	type HeadersToKeep = RelayChainHeadersToKeep;
	type WeightInfo = weights::pallet_bridge_grandpa_wococo_finality::WeightInfo<Runtime>;
}

/// Add parachain bridge pallet to track Wococo BridgeHub parachain
pub type BridgeParachainWococoInstance = pallet_bridge_parachains::Instance1;
impl pallet_bridge_parachains::Config<BridgeParachainWococoInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_parachains_within_wococo::WeightInfo<Runtime>;
	type BridgesGrandpaPalletInstance = BridgeGrandpaWococoInstance;
	type ParasPalletName = WococoBridgeParachainPalletName;
	type ParaStoredHeaderDataBuilder =
		SingleParaStoredHeaderDataBuilder<bp_bridge_hub_wococo::BridgeHubWococo>;
	type HeadsToKeep = ParachainHeadsToKeep;
	type MaxParaHeadDataSize = MaxWococoParaHeadDataSize;
}

/// Add GRANDPA bridge pallet to track Rococo relay chain.
pub type BridgeGrandpaRococoInstance = pallet_bridge_grandpa::Instance2;
impl pallet_bridge_grandpa::Config<BridgeGrandpaRococoInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type BridgedChain = bp_rococo::Rococo;
	type MaxFreeMandatoryHeadersPerBlock = ConstU32<4>;
	type HeadersToKeep = RelayChainHeadersToKeep;
	type WeightInfo = weights::pallet_bridge_grandpa_rococo_finality::WeightInfo<Runtime>;
}

/// Add parachain bridge pallet to track Rococo BridgeHub parachain
pub type BridgeParachainRococoInstance = pallet_bridge_parachains::Instance2;
impl pallet_bridge_parachains::Config<BridgeParachainRococoInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_parachains_within_rococo::WeightInfo<Runtime>;
	type BridgesGrandpaPalletInstance = BridgeGrandpaRococoInstance;
	type ParasPalletName = RococoBridgeParachainPalletName;
	type ParaStoredHeaderDataBuilder =
		SingleParaStoredHeaderDataBuilder<bp_bridge_hub_rococo::BridgeHubRococo>;
	type HeadsToKeep = ParachainHeadsToKeep;
	type MaxParaHeadDataSize = MaxRococoParaHeadDataSize;
}

/// Add GRANDPA bridge pallet to track Westend relay chain.
pub type BridgeGrandpaWestendInstance = pallet_bridge_grandpa::Instance3;
impl pallet_bridge_grandpa::Config<BridgeGrandpaWestendInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type BridgedChain = bp_westend::Westend;
	type MaxFreeMandatoryHeadersPerBlock = ConstU32<4>;
	type HeadersToKeep = RelayChainHeadersToKeep;
	type WeightInfo = weights::pallet_bridge_grandpa_westend_finality::WeightInfo<Runtime>;
}

/// Add parachain bridge pallet to track Westend BridgeHub parachain
pub type BridgeParachainWestendInstance = pallet_bridge_parachains::Instance3;
impl pallet_bridge_parachains::Config<BridgeParachainWestendInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_parachains_within_westend::WeightInfo<Runtime>;
	type BridgesGrandpaPalletInstance = BridgeGrandpaWestendInstance;
	type ParasPalletName = WestendBridgeParachainPalletName;
	type ParaStoredHeaderDataBuilder =
		SingleParaStoredHeaderDataBuilder<bp_bridge_hub_westend::BridgeHubWestend>;
	type HeadsToKeep = ParachainHeadsToKeep;
	type MaxParaHeadDataSize = MaxWestendParaHeadDataSize;
}

/// Allows collect and claim rewards for relayers
impl pallet_bridge_relayers::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Reward = Balance;
	type PaymentProcedure =
		bp_relayers::PayRewardFromAccount<pallet_balances::Pallet<Runtime>, AccountId>;
	type StakeAndSlash = pallet_bridge_relayers::StakeAndSlashNamed<
		AccountId,
		BlockNumber,
		Balances,
		RelayerStakeReserveId,
		RequiredStakeForStakeAndSlash,
		RelayerStakeLease,
	>;
	type WeightInfo = weights::pallet_bridge_relayers::WeightInfo<Runtime>;
}
