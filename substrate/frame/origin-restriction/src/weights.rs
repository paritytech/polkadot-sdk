// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.


#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::weights::Weight;

/// Weight functions needed for pallet origins restriction.
pub trait WeightInfo {
	fn clean_usage() -> Weight;
	fn restrict_origin_tx_ext() -> Weight;
}

// For tests
impl WeightInfo for () {
	fn clean_usage() -> Weight { Weight::zero() }
	fn restrict_origin_tx_ext() -> Weight { Weight::zero() }
}
