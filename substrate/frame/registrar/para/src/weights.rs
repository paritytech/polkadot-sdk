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

//! Weights for `pallet-registrar-para`.

use frame_support::weights::Weight;

/// Weight functions needed for `pallet-registrar-para`.
pub trait WeightInfo {
	fn reserve() -> Weight;
	/// `h` is the length of the genesis head data in bytes.
	fn register(h: u32) -> Weight;
	fn cancel_registration() -> Weight;
	fn receive() -> Weight;
}

/// Zero weights, for tests and mocks only.
impl WeightInfo for () {
	fn reserve() -> Weight {
		Weight::zero()
	}
	fn register(_h: u32) -> Weight {
		Weight::zero()
	}
	fn cancel_registration() -> Weight {
		Weight::zero()
	}
	fn receive() -> Weight {
		Weight::zero()
	}
}
