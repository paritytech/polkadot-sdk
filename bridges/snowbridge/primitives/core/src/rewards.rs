// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

use frame_support::pallet_prelude::DispatchResult;
pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

pub trait RewardLedger<T: frame_system::Config> {
	// Deposit reward which can later be claimed by `account`
	fn deposit(account: AccountIdOf<T>, value: u128) -> DispatchResult;
}
