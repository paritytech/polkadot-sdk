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

#![cfg_attr(not(feature = "std"), no_std)]

//! # Multi-Block Migrations Example Pallet
//!
//! This pallet serves as a minimal example of a pallet that uses the [Multi-Block Migrations
//! Framework](frame_support::migrations). You can observe how to configure it in a runtime in the
//! `kitchensink-runtime` crate.
//!
//! ## Introduction and Purpose
//!
//! The primary purpose of this pallet is to demonstrate the concept of Multi-Block Migrations in
//! Substrate. It showcases the migration of values from in the
//! [`MyMap`](`pallet::MyMap`) storage map a `u32` to a `u64` data type using the
//! [`SteppedMigration`](`frame_support::migrations::SteppedMigration`) implementation from the
//! [`migrations::v1`] module.
//!
//! The [`MyMap`](`pallet::MyMap`) storage item is defined in this `pallet`, and is
//! aliased to [`v0::MyMap`](`migrations::v1::v0::MyMap`) in the [`migrations::v1`]
//! module.
//!
//! ## How to Read the Documentation
//!
//! To access and navigate this documentation in your browser, use the following command:
//!
//! - `cargo doc --package pallet-example-mbm --open`
//!
//! This documentation is organized to help you understand the pallet's components, features, and
//! migration process.
//!
//! ## Example Usage
//!
//! To use this pallet and understand multi-block migrations, you can refer to the
//! [`migrations::v1`] module, which contains a step-by-step migration example.
//!
//! ## Pallet Structure
//!
//! The pallet is structured as follows:
//!
//! - [`migrations`]: Contains migration-related modules and migration logic.
//!   - [`v1`](`migrations::v1`): Demonstrates the migration process for changing the data type in
//!     the storage map.
//! - [`pallet`]: Defines the pallet configuration and storage items.
//!
//! ## Migration Safety
//!
//! When working with migrations, it's crucial to ensure the safety of your migrations. The
//! preferred tool to test migrations is
//! [`try-runtime-cli`](https://github.com/paritytech/try-runtime-cli). Support will be added to
//! dry-run MBMs once they are stable
//! (tracked: <https://github.com/paritytech/try-runtime-cli/issues/17>).

pub mod migrations;
mod mock;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{pallet_prelude::StorageMap, Blake2_128Concat};

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	/// Define a storage item to illustrate multi-block migrations.
	#[pallet::storage]
	pub type MyMap<T: Config> = StorageMap<_, Blake2_128Concat, u32, u64>;
}
