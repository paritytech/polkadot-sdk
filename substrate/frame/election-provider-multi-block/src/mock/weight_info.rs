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

// TODO: would love to ditch this, too big to handle here.

use crate::{self as multi_block};
use frame_support::weights::Weight;
use sp_runtime::traits::Zero;

frame_support::parameter_types! {
	pub static MockWeightInfo: bool = false;
}

pub struct DualMockWeightInfo;
impl multi_block::WeightInfo for DualMockWeightInfo {
	fn on_initialize_nothing() -> Weight {
		if MockWeightInfo::get() {
			Zero::zero()
		} else {
			<() as multi_block::WeightInfo>::on_initialize_nothing()
		}
	}

	fn on_initialize_into_snapshot_msp() -> Weight {
		if MockWeightInfo::get() {
			Zero::zero()
		} else {
			<() as multi_block::WeightInfo>::on_initialize_into_snapshot_msp()
		}
	}

	fn on_initialize_into_snapshot_rest() -> Weight {
		if MockWeightInfo::get() {
			Zero::zero()
		} else {
			<() as multi_block::WeightInfo>::on_initialize_into_snapshot_rest()
		}
	}

	fn on_initialize_into_signed() -> Weight {
		if MockWeightInfo::get() {
			Zero::zero()
		} else {
			<() as multi_block::WeightInfo>::on_initialize_into_signed()
		}
	}

	fn on_initialize_into_signed_validation() -> Weight {
		if MockWeightInfo::get() {
			Zero::zero()
		} else {
			<() as multi_block::WeightInfo>::on_initialize_into_signed_validation()
		}
	}

	fn on_initialize_into_unsigned() -> Weight {
		if MockWeightInfo::get() {
			Zero::zero()
		} else {
			<() as multi_block::WeightInfo>::on_initialize_into_unsigned()
		}
	}

	fn manage() -> Weight {
		if MockWeightInfo::get() {
			Zero::zero()
		} else {
			<() as multi_block::WeightInfo>::manage()
		}
	}
}
