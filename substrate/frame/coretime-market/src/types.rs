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

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use fp_coretime::{
	market::{MarketSaleInfo, TickAction},
	CoreIndex, Timeslice,
};
use scale_info::TypeInfo;
use sp_arithmetic::Perbill;

pub(crate) type BalanceOf<T> = <T as crate::Config>::Balance;
pub(crate) type RelayBlockNumberOf<T> = <T as crate::Config>::RelayBlockNumber;
pub(crate) type ConfigRecordOf<T> = ConfigRecord<RelayBlockNumberOf<T>, BalanceOf<T>>;
pub(crate) type SaleInfoRecordOf<T> = SaleInfoRecord<BalanceOf<T>, RelayBlockNumberOf<T>>;
pub(crate) type TickActionOf<T> =
	TickAction<<T as frame_system::Config>::AccountId, BalanceOf<T>, RelayBlockNumberOf<T>>;

pub type BidId = u32;

/// Provider of renewal rights information from the broker pallet.
pub trait RenewalRightsProvider<AccountId> {
	/// Returns the number of renewal rights held by `who` at timeslice `when`.
	fn renewal_rights_count(who: &AccountId, when: Timeslice) -> u32;

	/// Set renewal rights for benchmarking purposes.
	#[cfg(feature = "runtime-benchmarks")]
	fn set_rights_count(who: &AccountId, when: Timeslice, count: u32);
}

/// Initialization data for starting sales.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct InitData<Balance> {
	/// The initial reserve (floor) price for the first sale.
	pub reserve_price: Balance,
}

/// The status of a Bulk Coretime Sale (RFC-17 model).
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct SaleInfoRecord<Balance, BlockNumber> {
	/// The relay block number at which the sale (Market phase) starts.
	pub sale_start: BlockNumber,
	/// The opening price of the descending Dutch auction.
	pub opening_price: Balance,
	/// The reserve price (floor price of the descending auction).
	pub reserve_price: Balance,
	/// The clearing price (uniform price all winners pay). Set after auction settlement.
	pub clearing_price: Option<Balance>,
	/// The first timeslice of the Regions which are being sold in this sale.
	pub region_begin: Timeslice,
	/// The timeslice on which the Regions being sold in this sale expire.
	pub region_end: Timeslice,
	/// The number of cores we want to sell, ideally. Selling this amount would result in no
	/// change to the price for the next sale.
	pub ideal_cores_sold: CoreIndex,
	/// Number of cores offered for sale.
	pub cores_offered: CoreIndex,
	/// The index of the first core for sale. Sold regions are assigned core indices
	/// incrementing from this value.
	pub first_core: CoreIndex,
	/// Number of cores which have been sold; never more than cores_offered.
	pub cores_sold: CoreIndex,
	/// Number of renewals exercised in the current Renewal phase.
	pub renewal_count: u32,
	/// The current phase of this sale cycle.
	pub phase: SalePhase,
}

impl<Balance: Clone, BlockNumber: Clone> SaleInfoRecord<Balance, BlockNumber> {
	/// Convert to the coretime market interface's `MarketSaleInfo` struct.
	pub(crate) fn to_market_sale_info(&self) -> MarketSaleInfo<BlockNumber> {
		MarketSaleInfo {
			sale_start: self.sale_start.clone(),
			region_begin: self.region_begin,
			region_end: self.region_end,
			cores_offered: self.cores_offered,
			first_core: self.first_core,
			cores_sold: self.cores_sold,
		}
	}
}

/// Configuration of the coretime system (RFC-17 model).
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct ConfigRecord<BlockNumber, Balance> {
	/// The number of Relay-chain blocks in advance which scheduling should be fixed and the
	/// `Coretime::assign` API used to inform the Relay-chain.
	pub advance_notice: BlockNumber,
	/// The length in blocks of the Market (auction) phase.
	pub market_period: BlockNumber,
	/// The length in blocks of the Renewal phase.
	pub renewal_period: BlockNumber,
	/// The length in timeslices of Regions which are up for sale in forthcoming sales.
	pub region_length: Timeslice,
	/// The proportion of cores available for sale which should be sold.
	pub ideal_bulk_proportion: Perbill,
	/// An artificial limit to the number of cores which are allowed to be sold. If `Some` then
	/// no more cores will be sold than this.
	pub limit_cores_offered: Option<CoreIndex>,
	/// Penalty applied to renewers who didn't win in the auction (when market is oversubscribed).
	pub penalty: Perbill,
	/// The duration by which rewards for contributions to the InstaPool must be collected.
	pub contribution_timeout: Timeslice,
	/// Multiplier applied to the reserve price to derive the opening price.
	pub price_multiplier: u32,
	/// Minimum opening price floor.
	pub min_opening_price: Balance,
	/// Target consumption rate for reserve price adjustment.
	pub target_consumption_rate: Perbill,
	/// Sensitivity parameter (K) in milliunits. Divide by 1000 to get the actual K value.
	pub sensitivity_millis: u32,
	/// Minimum reserve price floor.
	pub min_reserve_price: Balance,
	/// Minimum absolute reserve price increase when consumption is 100%.
	pub min_increment: Balance,
}

impl<BlockNumber, Balance> ConfigRecord<BlockNumber, Balance>
where
	BlockNumber: sp_arithmetic::traits::Zero,
{
	/// Check the config for basic validity constraints.
	pub fn validate(&self) -> Result<(), ()> {
		if self.market_period.is_zero() {
			return Err(());
		}
		Ok(())
	}
}

/// The phase of a Bulk Coretime Sale.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Copy,
	Clone,
	PartialEq,
	Eq,
	Debug,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum SalePhase {
	/// Market period: descending Dutch auction, bids accepted.
	Market,
	/// Renewal period: existing tenants can exercise renewal rights.
	Renewal,
	/// Settlement period: no primary sales, awaiting next sale rotation.
	Settlement,
}

/// A bid in the descending clock auction.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct BidRecord<AccountId, Balance> {
	/// Unique identifier for this bid.
	pub bid_id: BidId,
	/// The bidder's account.
	pub who: AccountId,
	/// The bid price (amount locked from the bidder).
	pub price: Balance,
}

/// Record of an auction winner after settlement.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct AllocationRecord<AccountId, Balance> {
	/// The winning bidder.
	pub who: AccountId,
	/// The original bid price (used for displacement priority — lowest bid displaced first).
	pub bid_price: Balance,
	/// The unique bid ID.
	pub bid_id: BidId,
	/// The core index assigned to this allocation.
	pub core: CoreIndex,
}

/// Per-account tracking of how many cores were acquired through each path in a sale.
/// Used to enforce the RFC-17 rule: auction wins + renewals ≤ total renewal rights.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Clone,
	Default,
	PartialEq,
	Eq,
	Debug,
	TypeInfo,
	MaxEncodedLen,
)]
pub struct AccountQuota {
	/// Number of cores won in the auction.
	pub auction_wins: u32,
	/// Number of renewals exercised during the Renewal phase.
	pub renewals_used: u32,
}

/// Representation of a bid that was displaced during the renewal phase that will resolve to
/// `TickAction::Refund` at the finalization.
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub struct BidDisplacement<AccountId, Balance> {
	/// The bidder account.
	pub who: AccountId,
	/// Amount to be refunded to the bidder.
	pub refund: Balance,
}
