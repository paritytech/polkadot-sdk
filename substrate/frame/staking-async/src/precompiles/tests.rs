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

use crate::{
	mock::*,
	precompiles::staking::{IStaking, IStaking::IStakingCalls},
};
use alloy_core::{self as alloy, primitives::U256, sol_types::SolCall};
use sp_runtime::Weight;

const ADDR: [u8; 20] =
	const_hex::const_decode_to_array(b"0000000000000000000000000000000000800000").unwrap();

/// Basic encoding/decoding tests to verify the Solidity ABI stability. Any breakage in these SHOULD
/// NOT BE FIXED HERE, and is rather a sign that the interface is breaking.
mod abi_tests {
	use super::*;

	#[test]
	fn bond() {
		// Test that the ABI encoding/decoding works correctly
		let bond_call = IStaking::bondCall { requested: alloy::primitives::U256::from(1000) };

		// Encode and then decode
		let encoded = bond_call.abi_encode();
		println!("[bond] Encoded: {:?}", encoded);
		let decoded = IStaking::bondCall::abi_decode(&encoded).unwrap();

		assert_eq!(decoded.requested, alloy::primitives::U256::from(1000));
	}
}

mod bond {
	use crate::precompiles::staking::IStaking::bondCall;
	use frame_support::assert_ok;

	use super::*;

	#[test]
	fn happy_path_works() {
		ExtBuilder::default().has_stakers(false).build_and_execute(|| {
			let call = bondCall { requested: U256::from(100) };
			assert_ok!(pallet_revive::Pallet::<T>::bare_call(
				RuntimeOrigin::signed(1),
				ADDR.into(),
				0u32.into(),
				Weight::MAX,
				pallet_revive::DepositLimit::UnsafeOnlyForDryRun,
				call.abi_encode(),
			));
		});
	}

	#[test]
	fn bonds_more_than_free_balance_emits_correct_event() {
		ExtBuilder::default().has_stakers(false).build_and_execute(|| {});
	}
}
