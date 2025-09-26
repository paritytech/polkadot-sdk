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

//! # Assets Precompiles
//!
//! A wrapper pallet over the [`pallet-assets`] pallet that exposes common precompiles for accessing
//! and managing assets.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;
pub use precompiles::*;

#[cfg(test)]
mod mock;

mod precompiles;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config(with_default)]
	pub trait Config<I: 'static = ()>: frame_system::Config + pallet_assets::Config<I> {}

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);
}
