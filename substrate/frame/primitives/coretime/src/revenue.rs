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

use crate::Timeslice;
use alloc::vec::Vec;
use sp_weights::Weight;

/// The outcome of claiming on-demand revenue through [`OnDemandRevenue::claim_revenue`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RevenueClaim<Balance> {
	/// The amount transferred for each timeslice whose revenue was paid out, oldest first,
	/// possibly zero.
	///
	/// If a timeslice is not listed, but a later timeslice is, it means there was no revenue
	/// booked for that timeslice. If no later timeslice is listed, the revenue should be
	/// assumed to not have been paid out yet, and if it exists, it will be paid out in a later
	/// claim.
	pub paid: Vec<(Timeslice, Balance)>,
	/// The weight consumed by the claim.
	pub weight: Weight,
}

/// A source of revenue from instantaneous (on-demand) Coretime sales.
///
/// The sales are expected to happen on the same chain as `pallet-broker`, so that the revenue can
/// be claimed and paid out synchronously.
pub trait OnDemandRevenue<Balance, AccountId> {
	/// Whether on-demand Coretime is sold here.
	///
	/// Once it is, the claimer is expected to claim the revenue through [`Self::claim_revenue`].
	/// Until then, no orders are accepted, and the claimer is expected to get its revenue from
	/// elsewhere.
	fn is_enabled() -> bool;

	/// Claim the revenue from the on-demand orders placed during timeslice `up_to` and during
	/// every earlier timeslice whose revenue has not been claimed yet, transferring it to
	/// `beneficiary`.
	///
	/// The claimer is expected to claim every timeslice once it has ended.
	///
	/// The revenue of a timeslice is either paid out in full or not at all. The oldest revenue is
	/// paid out first.
	fn claim_revenue(up_to: Timeslice, beneficiary: &AccountId) -> RevenueClaim<Balance>;
}

/// No on-demand Coretime market on this chain, for a broker which gets its revenue from elsewhere.
impl<Balance, AccountId> OnDemandRevenue<Balance, AccountId> for () {
	fn is_enabled() -> bool {
		false
	}

	fn claim_revenue(_up_to: Timeslice, _beneficiary: &AccountId) -> RevenueClaim<Balance> {
		RevenueClaim { paid: Vec::new(), weight: Weight::zero() }
	}
}
