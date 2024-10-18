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

#![cfg_attr(not(feature = "std"), no_std)]

pub use constants::*;
pub use default_configs::*;

#[cfg(feature = "rococo")]
pub mod rococo;
#[cfg(feature = "westend")]
pub mod westend;

pub mod default_configs;

pub mod constants {
	use frame_support::parameter_types;
	use frame_system::limits::BlockLength;
	use parachains_common::NORMAL_DISPATCH_RATIO;

	parameter_types! {
		/// The block length of a parachain runtime.
		pub RuntimeBlockLength: BlockLength =
			BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
	}
}
