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

#[frame_support::pallet]
mod pallet {
	#[pallet::config]
	pub trait Config: frame_system::Config + codec::Codec + scale_info::TypeInfo {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::view_functions]
	impl<T: Config> Pallet<T> {
		pub fn first() -> u32 {
			0
		}
	}

	#[pallet::view_functions]
	impl<T: Config> Pallet<T> {
		pub fn second(value: T) -> u32 {
			let _ = value;
			0
		}
	}
}

fn main() {}
