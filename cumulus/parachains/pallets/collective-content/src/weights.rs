// Copyright (C) 2023 Parity Technologies (UK) Ltd.
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

//! The pallet weight info trait and its unit implementation.

use frame_support::weights::Weight;

/// Weights information needed for the pallet.
pub trait WeightInfo {
	/// Returns the weight of the set_charter extrinsic.
	fn set_charter() -> Weight;
	/// Returns the weight of the announce extrinsic.
	fn announce() -> Weight;
	/// Returns the weight of the remove_announcement extrinsic.
	fn remove_announcement() -> Weight;
}

/// Unit implementation of the [WeightInfo].
impl WeightInfo for () {
	fn set_charter() -> Weight {
		Weight::zero()
	}
	fn announce() -> Weight {
		Weight::zero()
	}
	fn remove_announcement() -> Weight {
		Weight::zero()
	}
}
