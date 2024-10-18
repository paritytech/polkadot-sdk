// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

use crate::AccountIdOf;
use sp_core::U256;
pub trait RewardLedger<T: frame_system::Config> {
    // Deposit reward which can later be claimed by `account`
    fn deposit(account: AccountIdOf<T>, value: U256);
}
