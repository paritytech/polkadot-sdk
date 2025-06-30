// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Module contains predefined test-case scenarios for `Runtime` with various assets.

pub mod test_cases;
pub mod test_cases_over_bridge;
pub mod xcm_helpers;

use frame_support::traits::ProcessMessageError;
pub use parachains_runtimes_test_utils::*;
use std::fmt::Debug;

use xcm::latest::prelude::*;
use xcm_builder::{CreateMatcher, MatchXcm};

/// Given a message, a sender, and a destination, it returns the delivery fees
fn get_fungible_delivery_fees<S: SendXcm>(destination: Location, message: Xcm<()>) -> u128 {
	let Ok((_, delivery_fees)) = validate_send::<S>(destination, message) else {
		unreachable!("message can be sent; qed")
	};
	if let Some(delivery_fee) = delivery_fees.inner().first() {
		let Fungible(delivery_fee_amount) = delivery_fee.fun else {
			unreachable!("asset is fungible; qed");
		};
		delivery_fee_amount
	} else {
		0
	}
}

/// Helper function to verify `xcm` contains all relevant instructions expected on destination
/// chain as part of a reserve-asset-transfer.
pub(crate) fn assert_matches_reserve_asset_deposited_instructions<RuntimeCall: Debug>(
	xcm: &mut Xcm<RuntimeCall>,
	expected_reserve_assets_deposited: &Assets,
	expected_beneficiary: &Location,
) {
	let _ = xcm
		.0
		.matcher()
		.skip_inst_while(|inst| !matches!(inst, ReserveAssetDeposited(..)))
		.expect("no instruction ReserveAssetDeposited?")
		.match_next_inst(|instr| match instr {
			ReserveAssetDeposited(reserve_assets) => {
				assert_eq!(reserve_assets, expected_reserve_assets_deposited);
				Ok(())
			},
			_ => Err(ProcessMessageError::BadFormat),
		})
		.expect("expected instruction ReserveAssetDeposited")
		.match_next_inst(|instr| match instr {
			ClearOrigin => Ok(()),
			_ => Err(ProcessMessageError::BadFormat),
		})
		.expect("expected instruction ClearOrigin")
		.match_next_inst(|instr| match instr {
			BuyExecution { .. } => Ok(()),
			_ => Err(ProcessMessageError::BadFormat),
		})
		.expect("expected instruction BuyExecution")
		.match_next_inst(|instr| match instr {
			DepositAsset { assets: _, beneficiary } if beneficiary == expected_beneficiary =>
				Ok(()),
			_ => Err(ProcessMessageError::BadFormat),
		})
		.expect("expected instruction DepositAsset");
}
