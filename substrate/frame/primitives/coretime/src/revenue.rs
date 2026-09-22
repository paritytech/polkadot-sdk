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

//! Interface between `pallet-broker` and the on-demand Coretime market it shares a chain with.

use sp_weights::Weight;

/// A source of revenue from instantaneous (on-demand) Coretime sales.
///
/// The sales are expected to happen on the same chain as `pallet-broker`, so that the revenue can
/// be claimed and paid out synchronously.
pub trait OnDemandRevenue<RelayBlockNumber, Balance, AccountId> {
	/// Claim the revenue accumulated from on-demand orders placed before Relay-chain block
	/// `until`, transferring it to `beneficiary`.
	///
	/// Returns the amount actually transferred.
	fn claim_revenue_until(until: RelayBlockNumber, beneficiary: &AccountId) -> Balance;

	/// The worst-case weight of [`Self::claim_revenue_until`].
	fn claim_revenue_until_weight() -> Weight {
		Weight::zero()
	}
}
