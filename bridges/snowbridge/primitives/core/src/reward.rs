// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

use frame_support::pallet_prelude::DispatchResult;

pub trait RewardLedger<AccountId, Balance> {
	// Deposit reward which can later be claimed by `account`
	fn deposit(account: AccountId, value: Balance) -> DispatchResult;
}

impl<AccountId, Balance> RewardLedger<AccountId, Balance> for () {
	fn deposit(_: AccountId, _: Balance) -> DispatchResult {
		Ok(())
	}
}
