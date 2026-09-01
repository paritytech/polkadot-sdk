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

use crate::{
	market::Market, Config, CoreAssignment, CoreIndex, CoreMask, CoretimeInterface,
	RCBlockNumberOf, RegionId, TaskId, Timeslice, CORE_MASK_BITS,
};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::traits::fungible::Inspect;
use frame_system::{pallet_prelude::AccountIdFor, Config as SConfig};
use scale_info::TypeInfo;
use sp_core::ConstU32;
use sp_runtime::BoundedVec;

pub type BalanceOf<T> = <<T as Config>::Currency as Inspect<<T as SConfig>::AccountId>>::Balance;
pub type RelayBalanceOf<T> = <<T as Config>::Coretime as CoretimeInterface>::Balance;
pub type RelayBlockNumberOf<T> = RCBlockNumberOf<<T as Config>::Coretime>;
pub type RelayAccountIdOf<T> = <<T as Config>::Coretime as CoretimeInterface>::AccountId;

pub type MarketInitDataOf<T> = <<T as Config>::CoretimeMarket as Market<
	RelayBlockNumberOf<T>,
	BalanceOf<T>,
	AccountIdFor<T>,
>>::InitData;

pub type MarketBidIdOf<T> = <<T as Config>::CoretimeMarket as Market<
	RelayBlockNumberOf<T>,
	BalanceOf<T>,
	AccountIdFor<T>,
>>::BidId;

pub type MarketConfigOf<T> = <<T as Config>::CoretimeMarket as Market<
	RelayBlockNumberOf<T>,
	BalanceOf<T>,
	AccountIdFor<T>,
>>::Configuration;

pub type MarketWeightsOf<T> = <<T as Config>::CoretimeMarket as Market<
	RelayBlockNumberOf<T>,
	BalanceOf<T>,
	AccountIdFor<T>,
>>::Weights;

/// Counter for the total number of set bits over every core's `CoreMask`. `u32` so we don't
/// ever get an overflow. This is 1/80th of a Polkadot Core per timeslice. Assuming timeslices are
/// 80 blocks, then this indicates usage of a single core one time over a timeslice.
pub type CoreMaskBitCount = u32;
/// The same as `CoreMaskBitCount` but signed.
pub type SignedCoreMaskBitCount = i32;
/// Count of the renewal rights one has.
pub type RenewalRightsCount = u32;

/// Identifier of the renewal rights for some account at some timeslice.
#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub struct RenewalRightsId<AccountId> {
	/// Who holds the renewal rights.
	pub owner: AccountId,
	/// When the renewal rights are valid.
	pub when: Timeslice,
}
pub type RenewalRightsIdOf<T> = RenewalRightsId<AccountIdFor<T>>;

/// Whether a core assignment is revokable or not.
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
pub enum Finality {
	/// The region remains with the same owner allowing the assignment to be altered.
	Provisional,
	/// The region is removed; the assignment may be eligible for renewal.
	Final,
}

/// The rest of the information describing a Region.
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub struct RegionRecord<AccountId, Balance> {
	/// The end of the Region.
	pub end: Timeslice,
	/// The owner of the Region.
	pub owner: Option<AccountId>,
	/// The amount paid to Polkadot for this Region, or `None` if renewal is not allowed.
	pub paid: Option<Balance>,
}
pub type RegionRecordOf<T> = RegionRecord<<T as SConfig>::AccountId, BalanceOf<T>>;

/// An distinct item which can be scheduled on a Polkadot Core.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct ScheduleItem {
	/// The regularity parts in which this Item will be scheduled on the Core.
	pub mask: CoreMask,
	/// The job that the Core should be doing.
	pub assignment: CoreAssignment,
}
pub type Schedule = BoundedVec<ScheduleItem, ConstU32<{ CORE_MASK_BITS as u32 }>>;

/// The record body of a Region which was contributed to the Instantaneous Coretime Pool. This helps
/// with making pro rata payments to contributors.
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub struct ContributionRecord<AccountId> {
	/// The end of the Region contributed.
	pub length: Timeslice,
	/// The identity of the contributor.
	pub payee: AccountId,
}
pub type ContributionRecordOf<T> = ContributionRecord<<T as SConfig>::AccountId>;

/// A per-timeslice bookkeeping record for tracking Instantaneous Coretime Pool activity and
/// making proper payments to contributors.
#[derive(Encode, Decode, Clone, Default, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub struct InstaPoolHistoryRecord<Balance> {
	/// The total amount of Coretime (measured in Core Mask Bits minus any contributions which have
	/// already been paid out.
	pub private_contributions: CoreMaskBitCount,
	/// The total amount of Coretime (measured in Core Mask Bits contributed by the Polkadot System
	/// in this timeslice.
	pub system_contributions: CoreMaskBitCount,
	/// The payout remaining for the `private_contributions`, or `None` if the revenue is not yet
	/// known.
	pub maybe_payout: Option<Balance>,
}
pub type InstaPoolHistoryRecordOf<T> = InstaPoolHistoryRecord<BalanceOf<T>>;

/// How much of a core has been assigned or, if completely assigned, the workload itself.
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub enum CompletionStatus {
	/// The core is not fully assigned; the inner is the parts which have.
	Partial(CoreMask),
	/// The core is fully assigned; the inner is the workload which has been assigned.
	Complete(Schedule),
}
impl CompletionStatus {
	/// Return reference to the complete workload, or `None` if incomplete.
	pub fn complete(&self) -> Option<&Schedule> {
		match self {
			Self::Complete(s) => Some(s),
			Self::Partial(_) => None,
		}
	}
	/// Return the complete workload, or `None` if incomplete.
	pub fn drain_complete(self) -> Option<Schedule> {
		match self {
			Self::Complete(s) => Some(s),
			Self::Partial(_) => None,
		}
	}
}

/// A record of a potential renewal.
///
/// The renewal will only actually be allowed if `CompletionStatus` is `Complete` at the time of
/// renewal.
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub struct PotentialRenewalRecord {
	/// The workload which will be scheduled on the Core in the case a renewal is made, or if
	/// incomplete, then the parts of the core which have been scheduled.
	pub completion: CompletionStatus,
}

/// General status of the system.
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub struct StatusRecord {
	/// The total number of cores which can be assigned (one plus the maximum index which can
	/// be used in `Coretime::assign`).
	pub core_count: CoreIndex,
	/// The current size of the Instantaneous Coretime Pool, measured in
	/// Core Mask Bits.
	pub private_pool_size: CoreMaskBitCount,
	/// The current amount of the Instantaneous Coretime Pool which is provided by the Polkadot
	/// System, rather than provided as a result of privately operated Coretime.
	pub system_pool_size: CoreMaskBitCount,
	/// The last (Relay-chain) timeslice which we committed to the Relay-chain.
	pub last_committed_timeslice: Timeslice,
	/// The timeslice of the last time we ticked.
	pub last_timeslice: Timeslice,
}

/// A record of flux in the InstaPool.
#[derive(Encode, Decode, Clone, Copy, Default, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub struct PoolIoRecord {
	/// The total change of the portion of the pool supplied by purchased Bulk Coretime, measured
	/// in Core Mask Bits.
	pub private: SignedCoreMaskBitCount,
	/// The total change of the portion of the pool supplied by the Polkadot System, measured in
	/// Core Mask Bits.
	pub system: SignedCoreMaskBitCount,
}

/// Record for Polkadot Core reservations (generally tasked with the maintenance of System
/// Chains).
pub type ReservationsRecord<Max> = BoundedVec<Schedule, Max>;
pub type ReservationsRecordOf<T> = ReservationsRecord<<T as Config>::MaxReservedCores>;

/// Information on a single legacy lease.
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub struct LeaseRecordItem {
	/// The timeslice until the lease is valid.
	pub until: Timeslice,
	/// The task which the lease is for.
	pub task: TaskId,
}

/// Record for Polkadot Core legacy leases.
pub type LeasesRecord<Max> = BoundedVec<LeaseRecordItem, Max>;
pub type LeasesRecordOf<T> = LeasesRecord<<T as Config>::MaxLeasedCores>;

/// Record for On demand core sales.
///
/// The blocknumber is the relay chain block height `until` which the original request
/// for revenue was made.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct OnDemandRevenueRecord<RelayBlockNumber, RelayBalance> {
	/// The height of the Relay-chain at the time the revenue request was made.
	pub until: RelayBlockNumber,
	/// The accumulated balance of on demand sales made on the relay chain.
	pub amount: RelayBalance,
}

pub type OnDemandRevenueRecordOf<T> =
	OnDemandRevenueRecord<RelayBlockNumberOf<T>, RelayBalanceOf<T>>;

/// Configuration of this pallet.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct ConfigRecord<RelayBlockNumber> {
	/// The number of Relay-chain blocks in advance which scheduling should be fixed and the
	/// `Coretime::assign` API used to inform the Relay-chain.
	pub advance_notice: RelayBlockNumber,
	/// The length in timeslices of Regions which are up for sale in forthcoming sales.
	pub region_length: Timeslice,
	/// The duration by which rewards for contributions to the InstaPool must be collected.
	pub contribution_timeout: Timeslice,
}
pub type ConfigRecordOf<T> = ConfigRecord<RelayBlockNumberOf<T>>;

/// A record containing information regarding auto-renewal for a specific core.
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub struct AutoRenewalRecord {
	/// The core for which auto renewal is enabled.
	pub core: CoreIndex,
	/// The task assigned to the core. We keep track of it so we don't have to look it up when
	/// performing auto-renewal.
	pub task: TaskId,
	/// Specifies when the upcoming renewal should be performed. This is used for lease holding
	/// tasks to ensure that the renewal process does not begin until the lease expires.
	pub next_renewal: Timeslice,
}

/// A result of the `purchase` exectution.
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub(crate) enum PurchaseResult<Balance, BidId> {
	/// The core was purchased right away.
	Purchased {
		/// Id of the region that was sold.
		region_id: RegionId,
		/// Price that was paid for the purchased region.
		price: Balance,
		/// Validity period for the purchased region.
		duration: Timeslice,
	},
	/// A bid for purchase was placed.
	BidPlaced {
		/// Id of the bid that was placed.
		id: BidId,
	},
}
pub(crate) type PurchaseResultOf<T> = PurchaseResult<BalanceOf<T>, MarketBidIdOf<T>>;

/// A result of the `renew` exectution.
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub(crate) enum RenewResult<BidId> {
	/// The core was renewed right away.
	Renewed {
		/// Id of the region that the renewer received.
		new_region_id: RegionId,
		/// End of the new region.
		region_end: Timeslice,
	},
	/// A bid for core renewal was placed.
	BidPlaced {
		/// Id of the bid that was placed.
		id: BidId,
	},
}
pub(crate) type RenewResultOf<T> = RenewResult<MarketBidIdOf<T>>;
