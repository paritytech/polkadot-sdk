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

/// Return type of [`StakingApi::inflation_info`].
#[derive(codec::Encode, codec::Decode, scale_info::TypeInfo)]
pub struct InflationInfo {
	/// The percentage of the tokens currently staked.
	pub staking_rate: sp_arithmetic::Perquintill,
	/// The inflation per annum based on the above, and possibly other parameters.
	pub inflation: sp_arithmetic::Perquintill,
	/// A "best effort prediction" of the next inflation amount, based on the current system
	/// parameters.
	pub next_mint: (u128, u128),
}

sp_api::decl_runtime_apis! {
	pub trait StakingApi<Balance, AccountId>
		where
			Balance: Codec,
			AccountId: Codec,
	{
		/// Returns the nominations quota for a nominator with a given balance.
		fn nominations_quota(balance: Balance) -> u32;

		/// Returns the page count of exposures for a validator `account` in a given era.
		fn eras_stakers_page_count(era: sp_staking::EraIndex, account: AccountId) -> sp_staking::Page;

		/// Returns true if validator `account` has pages to be claimed for the given era.
		fn pending_rewards(era: sp_staking::EraIndex, account: AccountId) -> bool;

		/// Returns a set of information about the staking rate.
		fn inflation_info() -> InflationInfo;
	}
}
