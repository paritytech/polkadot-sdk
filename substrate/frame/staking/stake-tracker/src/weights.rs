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

use core::marker::PhantomData;
use frame_support::weights::Weight;

/// Weight functions needed for `pallet_stake_tracker`.
pub trait WeightInfo {
	fn drop_dangling_nomination() -> Weight;
}

/// Weights for `pallet_stake_tracker` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn drop_dangling_nomination() -> Weight {
		// TODO(gpestana): benchmarks.
		Weight::default()
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	fn drop_dangling_nomination() -> Weight {
		// TODO(gpestana): benchmarks.
		Weight::default()
	}
}
