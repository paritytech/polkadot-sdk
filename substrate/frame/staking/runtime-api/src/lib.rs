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

//! Runtime API definition for the staking pallet.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::Codec;
use sp_arithmetic::Perquintill;

sp_api::decl_runtime_apis! {
	pub trait StakingApi<Balance, AccountId>
		where
			Balance: Codec,
			AccountId: Codec,
	{
		/// Returns the nominations quota for a nominator with a given balance.
		fn nominations_quota(balance: Balance) -> u32;

		/// Returns the page count of exposures for a validator in a given era.
		fn eras_stakers_page_count(era: sp_staking::EraIndex, account: AccountId) -> sp_staking::Page;

		/// Ideal staking rate of the system.
		///
		/// In general, if the staking rate is higher than ideal, staking rewards would reduce.
		///
		/// Returns none if era payout does not depend on stake rate.
		fn ideal_staking_rate() -> Option<Perquintill>;
	}
}
