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

//! Pallets gated on `cfg` predicates other than `feature`.

use frame_support::{
	derive_impl,
	traits::{PalletInfoAccess, PalletsInfoAccess},
};

#[frame_support::pallet(dev_mode)]
pub mod pallet_a {
	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}
}

#[frame_support::pallet(dev_mode)]
pub mod pallet_b {
	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}
}

fn pallet_names<P: PalletsInfoAccess>() -> Vec<&'static str> {
	P::infos().iter().map(|info| info.name).collect()
}

mod runtime_macro {
	use super::*;

	#[frame_support::runtime]
	mod runtime {
		#[runtime::runtime]
		#[runtime::derive(
			RuntimeCall,
			RuntimeEvent,
			RuntimeError,
			RuntimeOrigin,
			RuntimeTask,
			RuntimeViewFunction
		)]
		pub struct Runtime;

		#[runtime::pallet_index(0)]
		pub type System = frame_system;
		#[cfg(debug_assertions)]
		#[runtime::pallet_index(1)]
		pub type A = pallet_a;
		// Shares the index with `A`, as only one of them is part of any build.
		#[cfg(not(debug_assertions))]
		#[runtime::pallet_index(1)]
		pub type B = pallet_b;
	}

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Runtime {
		type Block = frame_system::mocking::MockBlock<Runtime>;
	}
	impl pallet_a::Config for Runtime {}
	impl pallet_b::Config for Runtime {}

	#[test]
	fn pallets_follow_any_cfg_predicate_works() {
		let names = pallet_names::<AllPalletsWithSystem>();
		assert_eq!(names.contains(&"A"), cfg!(debug_assertions));
		assert_eq!(names.contains(&"B"), !cfg!(debug_assertions));
	}

	#[test]
	fn mutually_exclusive_pallets_share_index() {
		#[cfg(debug_assertions)]
		assert_eq!(A::index(), 1);
		#[cfg(not(debug_assertions))]
		assert_eq!(B::index(), 1);
	}
}

mod construct_runtime_macro {
	use super::*;

	frame_support::construct_runtime!(
		pub enum Runtime {
			System: frame_system,
			#[cfg(debug_assertions)]
			A: pallet_a = 1,
			// Shares the index with `A`, as only one of them is part of any build.
			#[cfg(not(debug_assertions))]
			B: pallet_b = 1,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Runtime {
		type Block = frame_system::mocking::MockBlock<Runtime>;
	}
	impl pallet_a::Config for Runtime {}
	impl pallet_b::Config for Runtime {}

	#[test]
	fn pallets_follow_any_cfg_predicate_works() {
		let names = pallet_names::<AllPalletsWithSystem>();
		assert_eq!(names.contains(&"A"), cfg!(debug_assertions));
		assert_eq!(names.contains(&"B"), !cfg!(debug_assertions));
	}

	#[test]
	fn mutually_exclusive_pallets_share_index() {
		#[cfg(debug_assertions)]
		assert_eq!(A::index(), 1);
		#[cfg(not(debug_assertions))]
		assert_eq!(B::index(), 1);
	}
}
