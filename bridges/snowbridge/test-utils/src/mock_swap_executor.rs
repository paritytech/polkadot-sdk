// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

use frame_support::pallet_prelude::DispatchError;
use pallet_asset_conversion::Swap;
use xcm::opaque::latest::Location;
pub struct SwapExecutor;

impl<AccountId> Swap<AccountId> for SwapExecutor {
	type Balance = u128;
	type AssetKind = Location;

	fn max_path_len() -> u32 {
		2
	}

	fn swap_exact_tokens_for_tokens(
		_sender: AccountId,
		_path: Vec<Self::AssetKind>,
		_amount_in: Self::Balance,
		_amount_out_min: Option<Self::Balance>,
		_send_to: AccountId,
		_keep_alive: bool,
	) -> Result<Self::Balance, DispatchError> {
		Ok(1_000_000_000u128)
	}

	fn swap_tokens_for_exact_tokens(
		_sender: AccountId,
		_path: Vec<Self::AssetKind>,
		_amount_out: Self::Balance,
		_amount_in_max: Option<Self::Balance>,
		_send_to: AccountId,
		_keep_alive: bool,
	) -> Result<Self::Balance, DispatchError> {
		unimplemented!()
	}
}
