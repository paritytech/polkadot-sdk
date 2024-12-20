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
use crate::{xcm_config, xcm_config::TreasuryAccount, XcmRouter};
use bp_messages::LegacyLaneId;
use frame_support::parameter_types;
use sp_core::H160;
use sp_runtime::traits::{ConstU128, ConstU32, ConstU8};
use testnet_parachains_constants::westend::snowbridge::{
	EthereumNetwork, INBOUND_QUEUE_PALLET_INDEX,
};

parameter_types! {
	pub storage RequiredStakeForStakeAndSlash: Balance = 1_000_000;
	pub const RelayerStakeLease: u32 = 8;
	pub const RelayerStakeReserveId: [u8; 8] = *b"brdgrlrs";

	pub storage DeliveryRewardInBalance: u64 = 1_000_000;

	pub WethAddress: H160 = H160(hex_literal::hex!("fff9976782d46cc05630d1f6ebab18b2324d6b14"));
}

pub const ASSET_HUB_ID: u32 = westend_runtime_constants::system_parachain::ASSET_HUB_ID;

/// Allows collect and claim rewards for relayers
pub type RelayersForLegacyLaneIdsMessagesInstance = ();
impl pallet_bridge_relayers::Config<RelayersForLegacyLaneIdsMessagesInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Reward = Balance;
	type PaymentProcedure = bp_relayers::PayRewardFromAccount<
		pallet_balances::Pallet<Runtime>,
		AccountId,
		Self::LaneId,
	>;
	type StakeAndSlash = pallet_bridge_relayers::StakeAndSlashNamed<
		AccountId,
		BlockNumber,
		Balances,
		RelayerStakeReserveId,
		RequiredStakeForStakeAndSlash,
		RelayerStakeLease,
	>;
	type WeightInfo = weights::pallet_bridge_relayers::WeightInfo<Runtime>;
	type LaneId = LegacyLaneId;

	type AssetHubParaId = ConstU32<ASSET_HUB_ID>;
	type EthereumNetwork = EthereumNetwork;
	type WethAddress = WethAddress;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type XcmSender = XcmRouter;
	#[cfg(feature = "runtime-benchmarks")]
	type XcmSender = DoNothingRouter;
	type Token = Balances;
	type AssetTransactor = <xcm_config::XcmConfig as xcm_executor::Config>::AssetTransactor;
	type InboundQueuePalletInstance = ConstU8<INBOUND_QUEUE_PALLET_INDEX>;
	/// Execution cost on AH in Weth. Cost is approximately 0.000000000000000008, added a slightly
	/// buffer.
	type AssetHubXCMFee = ConstU128<15>;
	type TreasuryAccount = TreasuryAccount;
}
