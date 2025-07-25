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

use frame_support::weights::Weight;

/// Weight functions needed for pallet origins "AND Gate".
pub trait WeightInfo {
	fn set_dummy() -> Weight;
	fn propose() -> Weight;
	fn add_approval() -> Weight;
	fn amend_remark() -> Weight;
	fn cancel_proposal() -> Weight;
	fn withdraw_approval() -> Weight;
	fn clean() -> Weight;
	fn execute_proposal() -> Weight;
	fn add_storage_id() -> Weight;
	fn remove_storage_id() -> Weight;
}

// For tests
impl WeightInfo for () {
	fn set_dummy() -> Weight {
		Weight::from_parts(10_000_000, 0)
	}
	fn propose() -> Weight {
		Weight::zero()
	}
	fn add_approval() -> Weight {
		Weight::zero()
	}
	fn amend_remark() -> Weight {
		Weight::from_parts(20_000, 0)
	}
	fn cancel_proposal() -> Weight {
		Weight::zero()
	}
	fn withdraw_approval() -> Weight {
		Weight::from_parts(10_000, 0)
	}
	fn clean() -> Weight {
		Weight::from_parts(10_000, 0)
	}
	fn execute_proposal() -> Weight {
		Weight::zero()
	}
	fn add_storage_id() -> Weight {
		Weight::from_parts(10_000, 0)
	}
	fn remove_storage_id() -> Weight {
		Weight::from_parts(15_000, 0)
	}
}
