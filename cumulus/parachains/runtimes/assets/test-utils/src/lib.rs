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

//! Module contains predefined test-case scenarios for `Runtime` with various assets.

pub mod test_cases;
pub mod test_cases_over_bridge;
pub mod xcm_helpers;

use frame_support::traits::ProcessMessageError;
pub use parachains_runtimes_test_utils::*;
use std::fmt::Debug;

use xcm::latest::prelude::*;
use xcm_builder::{CreateMatcher, MatchXcm};

/// Helper function to verify `xcm` contains all relevant instructions expected on destination
/// chain as part of a reserve-asset-transfer.
pub(crate) fn assert_matches_reserve_asset_deposited_instructions<RuntimeCall: Debug>(
	xcm: &mut Xcm<RuntimeCall>,
	expected_reserve_assets_deposited: &MultiAssets,
	expected_beneficiary: &MultiLocation,
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
