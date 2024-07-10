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

use super::Config;
use frame_support::{derive_impl, pallet_prelude::inject_runtime_type};
use static_assertions::assert_type_eq_all;

#[docify::export]
pub mod custom_config_prelude {
	use super::*;

	pub trait CustomDefaultConfigProvider {
		type AccountId;
		type RuntimeOrigin;
		type RuntimeCall;
		type RuntimeTask;
	}

	pub struct CustomDefaultConfig;

	#[crate::register_default_impl(CustomDefaultConfig)]
	impl CustomDefaultConfigProvider for CustomDefaultConfig {
		type AccountId = u16;
		#[inject_runtime_type]
		type RuntimeOrigin = ();
		#[inject_runtime_type]
		type RuntimeCall = ();
		#[inject_runtime_type]
		type RuntimeTask = ();
	}
}

#[test]
fn derive_impl_works_with_custom_default_config() {
	struct DummyRuntime;

	#[derive_impl(custom_config_prelude::CustomDefaultConfig, no_aggregated_types)]
	impl Config for DummyRuntime {
		type BaseCallFilter = frame_support::traits::Everything;
		type Block = super::Block;
		type DbWeight = ();
		type PalletInfo = super::PalletInfo;
	}

	assert_type_eq_all!(<DummyRuntime as Config>::AccountId, u16);
	assert_type_eq_all!(<DummyRuntime as Config>::RuntimeOrigin, ());
	assert_type_eq_all!(<DummyRuntime as Config>::RuntimeCall, ());
}
