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

//! Bridge definitions that can be used by multiple bridges.

use super::{weights, AccountId, Balance, Balances, BlockNumber, Runtime, RuntimeEvent};
use bp_relayers::RewardsAccountParams;

frame_support::parameter_types! {
	pub storage RequiredStakeForStakeAndSlash: Balance = 1_000_000;
	pub const RelayerStakeLease: u32 = 8;
	pub const RelayerStakeReserveId: [u8; 8] = *b"brdgrlrs";
	pub storage DeliveryRewardInBalance: u64 = 1_000_000;
}

/// Allows collect and claim rewards for the relayers
pub type BridgeRelayersInstance = pallet_bridge_relayers::Instance1;
impl pallet_bridge_relayers::Config<BridgeRelayersInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;

	type RewardBalance = Balance;
	type Reward = RewardsAccountParams<bp_messages::HashedLaneId>;
	type PaymentProcedure = bp_relayers::PayRewardFromAccount<
		pallet_balances::Pallet<Runtime>,
		AccountId,
		bp_messages::HashedLaneId,
		Self::RewardBalance,
	>;

	type StakeAndSlash = pallet_bridge_relayers::StakeAndSlashNamed<
		AccountId,
		BlockNumber,
		Balances,
		RelayerStakeReserveId,
		RequiredStakeForStakeAndSlash,
		RelayerStakeLease,
	>;
	type Balance = Balance;
	type WeightInfo = weights::pallet_bridge_relayers::WeightInfo<Runtime>;
}
