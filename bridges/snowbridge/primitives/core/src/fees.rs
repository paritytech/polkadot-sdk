// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use log;
use sp_runtime::{DispatchResult, SaturatedConversion, Saturating, TokenError};
use xcm::opaque::latest::{Location, XcmContext};
use xcm_executor::traits::TransactAsset;
const LOG_TARGET: &str = "xcm_fees";

/// Burns the fees embedded in the XCM for teleports.
pub fn burn_fees<AssetTransactor, Balance>(dest: Location, fee: Balance) -> DispatchResult
where
	AssetTransactor: TransactAsset,
	Balance: Saturating + TryInto<u128> + Copy,
{
	let dummy_context = XcmContext { origin: None, message_id: Default::default(), topic: None };
	let fees = (Location::parent(), fee.saturated_into::<u128>()).into();

	// Check if the asset can be checked out
	AssetTransactor::can_check_out(&dest, &fees, &dummy_context).map_err(|error| {
		log::error!(
			target: LOG_TARGET,
			"XCM asset check out failed with error {:?}",
			error
		);
		TokenError::FundsUnavailable
	})?;

	// Check out the asset
	AssetTransactor::check_out(&dest, &fees, &dummy_context);

	// Withdraw the asset and handle potential errors
	AssetTransactor::withdraw_asset(&fees, &dest, None).map_err(|error| {
		log::error!(
			target: LOG_TARGET,
			"XCM asset withdraw failed with error {:?}",
			error
		);
		TokenError::FundsUnavailable
	})?;

	Ok(())
}
