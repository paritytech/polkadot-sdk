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

//! # FRAME Pallet Examples
//!
//! This crate contains a collection of simple examples of FRAME pallets, demonstrating useful
//! features in action. It is not intended to be used in production.
//!
//! ## Pallets
//!
//! - [`pallet_example_basic`]: This pallet demonstrates concepts, APIs and structures common to
//!   most FRAME runtimes.
//!
//! - [`pallet_example_offchain_worker`]: This pallet demonstrates concepts, APIs and structures
//!   common to most offchain workers.
//!
//! - [`pallet_default_config_example`]: This pallet demonstrates different ways to implement the
//!   `Config` trait of pallets.
//!
//! - [`pallet_dev_mode`]: This pallet demonstrates the ease of requirements for a pallet in "dev
//!   mode".
//!
//! - [`pallet_example_kitchensink`]: This pallet demonstrates a catalog of all FRAME macros in use
//!   and their various syntax options.
//!
//! - [`pallet_example_split`]: A simple example of a FRAME pallet demonstrating the ability to
//!   split sections across multiple files.
//!
//! - [`pallet_example_frame_crate`]: Example pallet showcasing how one can be built using only the
//! `frame` umbrella crate.
//!
//! - [`pallet_example_single_block_migrations`]: An example pallet demonstrating best-practices for
//!   writing storage migrations.
//!
//! - [`pallet_example_tasks`]: This pallet demonstrates the use of `Tasks` to execute service work.
//!
//! - [`pallet_example_authorization_tx_extension`]: An example `TransactionExtension` that
//!   authorizes a custom origin through signature validation, along with two support pallets to
//!   showcase the usage.
//!
//! **Tip**: Use `cargo doc --package <pallet-name> --open` to view each pallet's documentation.
