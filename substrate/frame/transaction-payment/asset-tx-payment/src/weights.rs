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

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::weights::Weight;
use core::marker::PhantomData;

/// Weight functions needed for pallet-asset-tx-payment.
pub trait WeightInfo {
	fn charge_asset_tx_payment_zero() -> Weight;
    fn charge_asset_tx_payment_native() -> Weight;
    fn charge_asset_tx_payment_asset() -> Weight;
}

/// Weights for pallet-asset-tx-payment using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn charge_asset_tx_payment_zero() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 5_149_000 picoseconds.
		Weight::from_parts(5_268_000, 0)
	}
    fn charge_asset_tx_payment_native() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 5_149_000 picoseconds.
		Weight::from_parts(5_268_000, 0)
	}
    fn charge_asset_tx_payment_asset() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 5_149_000 picoseconds.
		Weight::from_parts(5_268_000, 0)
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	fn charge_asset_tx_payment_zero() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 5_149_000 picoseconds.
		Weight::from_parts(5_268_000, 0)
	}
    fn charge_asset_tx_payment_native() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 5_149_000 picoseconds.
		Weight::from_parts(5_268_000, 0)
	}
    fn charge_asset_tx_payment_asset() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 5_149_000 picoseconds.
		Weight::from_parts(5_268_000, 0)
	}
}
