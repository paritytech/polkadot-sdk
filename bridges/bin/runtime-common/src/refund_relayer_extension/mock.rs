// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! A mock runtime for testing different stuff in the crate.

pub use crate::mock::*;

use super::*;

use frame_support::parameter_types;
use sp_runtime::traits::ConstU64;

parameter_types! {
	pub TestParachain: u32 = 1000;
	pub TestLaneId: LaneId = TEST_LANE_ID;
	pub MsgProofsRewardsAccount: RewardsAccountParams = RewardsAccountParams::new(
		TEST_LANE_ID,
		TEST_BRIDGED_CHAIN_ID,
		RewardsAccountOwner::ThisChain,
	);
	pub MsgDeliveryProofsRewardsAccount: RewardsAccountParams = RewardsAccountParams::new(
		TEST_LANE_ID,
		TEST_BRIDGED_CHAIN_ID,
		RewardsAccountOwner::BridgedChain,
	);
}

bp_runtime::generate_static_str_provider!(TestExtension);

pub type TestGrandpaExtensionProvider = RefundBridgedGrandpaMessages<
	TestRuntime,
	(),
	RefundableMessagesLane<(), TestLaneId>,
	ActualFeeRefund<TestRuntime>,
	ConstU64<1>,
	StrTestExtension,
>;
pub type TestGrandpaExtension = RefundTransactionExtensionAdapter<TestGrandpaExtensionProvider>;
pub type TestExtensionProvider = RefundBridgedParachainMessages<
	TestRuntime,
	DefaultRefundableParachainId<(), TestParachain>,
	RefundableMessagesLane<(), TestLaneId>,
	ActualFeeRefund<TestRuntime>,
	ConstU64<1>,
	StrTestExtension,
>;
pub type TestExtension = RefundTransactionExtensionAdapter<TestExtensionProvider>;
