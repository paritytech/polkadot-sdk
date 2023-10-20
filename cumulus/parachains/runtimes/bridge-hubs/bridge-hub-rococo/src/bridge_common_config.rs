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

use super::{weights, Runtime, RuntimeEvent};
use bp_parachains::SingleParaStoredHeaderDataBuilder;
use frame_support::{parameter_types, traits::ConstU32};

parameter_types! {
	pub const RelayChainHeadersToKeep: u32 = 1024;
	pub const ParachainHeadsToKeep: u32 = 64;

	pub const RococoBridgeParachainPalletName: &'static str = "Paras";
	pub const MaxRococoParaHeadDataSize: u32 = bp_rococo::MAX_NESTED_PARACHAIN_HEAD_DATA_SIZE;
	pub const WococoBridgeParachainPalletName: &'static str = "Paras";
	pub const MaxWococoParaHeadDataSize: u32 = bp_wococo::MAX_NESTED_PARACHAIN_HEAD_DATA_SIZE;
}

/// Add GRANDPA bridge pallet to track Wococo relay chain.
pub type BridgeGrandpaWococoInstance = pallet_bridge_grandpa::Instance1;
impl pallet_bridge_grandpa::Config<BridgeGrandpaWococoInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type BridgedChain = bp_wococo::Wococo;
	type MaxFreeMandatoryHeadersPerBlock = ConstU32<4>;
	type HeadersToKeep = RelayChainHeadersToKeep;
	type WeightInfo = weights::pallet_bridge_grandpa_bridge_wococo_grandpa::WeightInfo<Runtime>;
}

/// Add parachain bridge pallet to track Wococo BridgeHub parachain
pub type BridgeParachainWococoInstance = pallet_bridge_parachains::Instance1;
impl pallet_bridge_parachains::Config<BridgeParachainWococoInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_parachains_bridge_parachains_bench_runtime_bridge_parachain_wococo_instance::WeightInfo<Runtime>;
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
	type WeightInfo = weights::pallet_bridge_grandpa_bridge_rococo_grandpa::WeightInfo<Runtime>;
}

/// Add parachain bridge pallet to track Rococo BridgeHub parachain
pub type BridgeParachainRococoInstance = pallet_bridge_parachains::Instance2;
impl pallet_bridge_parachains::Config<BridgeParachainRococoInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_parachains_bridge_parachains_bench_runtime_bridge_parachain_rococo_instance::WeightInfo<Runtime>;
	type BridgesGrandpaPalletInstance = BridgeGrandpaRococoInstance;
	type ParasPalletName = RococoBridgeParachainPalletName;
	type ParaStoredHeaderDataBuilder =
		SingleParaStoredHeaderDataBuilder<bp_bridge_hub_rococo::BridgeHubRococo>;
	type HeadsToKeep = ParachainHeadsToKeep;
	type MaxParaHeadDataSize = MaxRococoParaHeadDataSize;
}
