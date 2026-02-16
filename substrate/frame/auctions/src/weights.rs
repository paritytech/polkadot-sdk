// This file is part of Substrate.

// Copyright (C) Amforc AG.
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

//! Weights for `pallet_auctions`
//!
//! These weights are placeholder values and should be replaced with
//! actual benchmarked weights in production.

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{pallet_prelude::Weight, traits::Get, weights::constants::RocksDbWeight};
use core::marker::PhantomData;

/// Weight functions needed for `pallet_auctions`.
pub trait WeightInfo {
    fn take_liquidation() -> Weight;
    fn take_surplus() -> Weight;
    fn restart_auction() -> Weight;
    fn start_surplus_auction() -> Weight;
    fn set_buffer() -> Weight;
    fn set_maximum_duration() -> Weight;
    fn set_minimum_price() -> Weight;
    fn set_chip() -> Weight;
    fn set_tip() -> Weight;
    fn set_curve() -> Weight;
    fn set_stopped() -> Weight;
    fn set_surplus_mode() -> Weight;
    fn transfer_surplus() -> Weight;
    fn on_idle_one_auction() -> Weight;
}

#[cfg_attr(
    not(feature = "std"),
    deprecated(
        note = "SubstrateWeight is auto-generated and should not be used in production. Replace it with runtime benchmarked weights."
    )
)]
pub struct SubstrateWeight<T>(PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    /// Weight for `take_liquidation` extrinsic.
    /// Storage reads: `Auctions`, `AuctionConfig`, `Stopped`
    /// Storage writes: `Auctions` (on completion)
    fn take_liquidation() -> Weight {
        Weight::from_parts(50_000_000, 0)
            .saturating_add(T::DbWeight::get().reads(4_u64))
            .saturating_add(T::DbWeight::get().writes(3_u64))
    }

    /// Weight for `take_surplus` extrinsic.
    /// Storage reads: `Auctions`, `AuctionConfig`, `Stopped`, `ActiveSurplusAuctionId`
    /// Storage writes: `Auctions`, `ActiveSurplusAuctionId` (on completion)
    fn take_surplus() -> Weight {
        Weight::from_parts(50_000_000, 0)
            .saturating_add(T::DbWeight::get().reads(4_u64))
            .saturating_add(T::DbWeight::get().writes(3_u64))
    }

    /// Weight for `restart_auction` extrinsic.
    /// Storage reads: `Auctions`, `AuctionConfig`, `Stopped`
    /// Storage writes: `Auctions`
    fn restart_auction() -> Weight {
        Weight::from_parts(40_000_000, 0)
            .saturating_add(T::DbWeight::get().reads(3_u64))
            .saturating_add(T::DbWeight::get().writes(1_u64))
    }

    /// Weight for `start_surplus_auction` extrinsic.
    /// Storage reads: `SurplusMode`, `Stopped`, `ActiveSurplusAuctionId`, IF balance
    /// Storage writes: `Auctions`, `ActiveSurplusAuctionId`, `NextAuctionId`
    fn start_surplus_auction() -> Weight {
        Weight::from_parts(45_000_000, 0)
            .saturating_add(T::DbWeight::get().reads(4_u64))
            .saturating_add(T::DbWeight::get().writes(3_u64))
    }

    /// Weight for `set_buffer` extrinsic.
    fn set_buffer() -> Weight {
        Weight::from_parts(10_000_000, 0)
            .saturating_add(T::DbWeight::get().reads(1_u64))
            .saturating_add(T::DbWeight::get().writes(1_u64))
    }

    /// Weight for `set_maximum_duration` extrinsic.
    fn set_maximum_duration() -> Weight {
        Weight::from_parts(10_000_000, 0)
            .saturating_add(T::DbWeight::get().reads(1_u64))
            .saturating_add(T::DbWeight::get().writes(1_u64))
    }

    /// Weight for `set_minimum_price` extrinsic.
    fn set_minimum_price() -> Weight {
        Weight::from_parts(10_000_000, 0)
            .saturating_add(T::DbWeight::get().reads(1_u64))
            .saturating_add(T::DbWeight::get().writes(1_u64))
    }

    /// Weight for `set_chip` extrinsic.
    fn set_chip() -> Weight {
        Weight::from_parts(10_000_000, 0)
            .saturating_add(T::DbWeight::get().reads(1_u64))
            .saturating_add(T::DbWeight::get().writes(1_u64))
    }

    /// Weight for `set_tip` extrinsic.
    fn set_tip() -> Weight {
        Weight::from_parts(10_000_000, 0)
            .saturating_add(T::DbWeight::get().reads(1_u64))
            .saturating_add(T::DbWeight::get().writes(1_u64))
    }

    /// Weight for `set_curve` extrinsic.
    fn set_curve() -> Weight {
        Weight::from_parts(10_000_000, 0)
            .saturating_add(T::DbWeight::get().reads(1_u64))
            .saturating_add(T::DbWeight::get().writes(1_u64))
    }

    /// Weight for `set_stopped` extrinsic.
    fn set_stopped() -> Weight {
        Weight::from_parts(10_000_000, 0)
            .saturating_add(T::DbWeight::get().writes(1_u64))
    }

    /// Weight for `set_surplus_mode` extrinsic.
    fn set_surplus_mode() -> Weight {
        Weight::from_parts(10_000_000, 0)
            .saturating_add(T::DbWeight::get().writes(1_u64))
    }

    /// Weight for `transfer_surplus` extrinsic.
    /// Storage reads: `SurplusMode`, `Stopped`, IF balance (via `CollateralManager`)
    /// Storage writes: Asset transfer
    fn transfer_surplus() -> Weight {
        Weight::from_parts(40_000_000, 0)
            .saturating_add(T::DbWeight::get().reads(3_u64))
            .saturating_add(T::DbWeight::get().writes(2_u64))
    }

    /// Weight for processing one auction in `on_idle` hook.
    /// Storage reads: `OnIdleCursor`, `Stopped`, `AuctionConfig`, `Auctions`
    /// Storage writes: `OnIdleCursor`, `Auctions` (if restarted)
    fn on_idle_one_auction() -> Weight {
        Weight::from_parts(35_000_000, 0)
            .saturating_add(T::DbWeight::get().reads(4_u64))
            .saturating_add(T::DbWeight::get().writes(2_u64))
    }
}

/// For backwards compatibility and tests.
impl WeightInfo for () {
    fn take_liquidation() -> Weight {
        Weight::from_parts(50_000_000, 0)
            .saturating_add(RocksDbWeight::get().reads(4_u64))
            .saturating_add(RocksDbWeight::get().writes(3_u64))
    }

    fn take_surplus() -> Weight {
        Weight::from_parts(50_000_000, 0)
            .saturating_add(RocksDbWeight::get().reads(4_u64))
            .saturating_add(RocksDbWeight::get().writes(3_u64))
    }

    fn restart_auction() -> Weight {
        Weight::from_parts(40_000_000, 0)
            .saturating_add(RocksDbWeight::get().reads(3_u64))
            .saturating_add(RocksDbWeight::get().writes(1_u64))
    }

    fn start_surplus_auction() -> Weight {
        Weight::from_parts(45_000_000, 0)
            .saturating_add(RocksDbWeight::get().reads(4_u64))
            .saturating_add(RocksDbWeight::get().writes(3_u64))
    }

    fn set_buffer() -> Weight {
        Weight::from_parts(10_000_000, 0)
            .saturating_add(RocksDbWeight::get().reads(1_u64))
            .saturating_add(RocksDbWeight::get().writes(1_u64))
    }

    fn set_maximum_duration() -> Weight {
        Weight::from_parts(10_000_000, 0)
            .saturating_add(RocksDbWeight::get().reads(1_u64))
            .saturating_add(RocksDbWeight::get().writes(1_u64))
    }

    fn set_minimum_price() -> Weight {
        Weight::from_parts(10_000_000, 0)
            .saturating_add(RocksDbWeight::get().reads(1_u64))
            .saturating_add(RocksDbWeight::get().writes(1_u64))
    }

    fn set_chip() -> Weight {
        Weight::from_parts(10_000_000, 0)
            .saturating_add(RocksDbWeight::get().reads(1_u64))
            .saturating_add(RocksDbWeight::get().writes(1_u64))
    }

    fn set_tip() -> Weight {
        Weight::from_parts(10_000_000, 0)
            .saturating_add(RocksDbWeight::get().reads(1_u64))
            .saturating_add(RocksDbWeight::get().writes(1_u64))
    }

    fn set_curve() -> Weight {
        Weight::from_parts(10_000_000, 0)
            .saturating_add(RocksDbWeight::get().reads(1_u64))
            .saturating_add(RocksDbWeight::get().writes(1_u64))
    }

    fn set_stopped() -> Weight {
        Weight::from_parts(10_000_000, 0)
            .saturating_add(RocksDbWeight::get().writes(1_u64))
    }

    fn set_surplus_mode() -> Weight {
        Weight::from_parts(10_000_000, 0)
            .saturating_add(RocksDbWeight::get().writes(1_u64))
    }

    fn transfer_surplus() -> Weight {
        Weight::from_parts(40_000_000, 0)
            .saturating_add(RocksDbWeight::get().reads(3_u64))
            .saturating_add(RocksDbWeight::get().writes(2_u64))
    }

    fn on_idle_one_auction() -> Weight {
        Weight::from_parts(35_000_000, 0)
            .saturating_add(RocksDbWeight::get().reads(4_u64))
            .saturating_add(RocksDbWeight::get().writes(2_u64))
    }
}
